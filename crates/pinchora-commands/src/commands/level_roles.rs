use std::time::{Duration, Instant};

use pinchora_core::leveling::level_of;
use pinchora_db::MAX_LEVEL_ROLES;
use twilight_interactions::command::{CommandModel, CommandOption, CreateCommand, CreateOption};
use twilight_model::{
    channel::message::MessageFlags,
    guild::Role,
    http::interaction::InteractionResponseData,
    id::{Id, marker::RoleMarker},
};
use twilight_util::builder::embed::{EmbedBuilder, EmbedFooterBuilder};

use crate::{Cx, roles, util::BRAND};

/// Members checked per `sync` run, newest XP first.
const SYNC_LIMIT: i64 = 1_000;
/// Stop a sync before the interaction token (15 minutes) runs out.
const SYNC_BUDGET: Duration = Duration::from_secs(10 * 60);

#[derive(CommandModel, CreateCommand)]
#[command(name = "level-roles", desc = "Roles members earn when they reach a level")]
#[allow(clippy::large_enum_variant)] // parsed once per interaction, never stored
pub enum ConfigLevelRoles {
    #[command(name = "add")]
    Add(LevelRolesAdd),
    #[command(name = "remove")]
    Remove(LevelRolesRemove),
    #[command(name = "list")]
    List(LevelRolesList),
    #[command(name = "mode")]
    Mode(LevelRolesMode),
    #[command(name = "sync")]
    Sync(LevelRolesSync),
}

#[derive(CommandModel, CreateCommand)]
#[command(name = "add", desc = "Give a role to everyone who reaches a level")]
pub struct LevelRolesAdd {
    /// Level that earns the role
    #[command(min_value = 1, max_value = 1_000)]
    level: i64,
    /// The role to give (adding it again moves it to the new level)
    role: Role,
}

#[derive(CommandModel, CreateCommand)]
#[command(name = "remove", desc = "Stop giving a role at a level")]
pub struct LevelRolesRemove {
    /// The role to stop giving
    role: Id<RoleMarker>,
}

#[derive(CommandModel, CreateCommand)]
#[command(name = "list", desc = "Show the level roles")]
pub struct LevelRolesList;

#[derive(CommandOption, CreateOption, Clone, Copy)]
pub enum RoleMode {
    #[option(name = "Stack: keep every role earned", value = "stack")]
    Stack,
    #[option(name = "Replace: keep only the highest role", value = "replace")]
    Replace,
}

#[derive(CommandModel, CreateCommand)]
#[command(name = "mode", desc = "Keep every level role, or only the highest one")]
pub struct LevelRolesMode {
    /// How roles pile up
    mode: RoleMode,
}

#[derive(CommandModel, CreateCommand)]
#[command(
    name = "sync",
    desc = "Give members who are already past a level the roles they earned"
)]
pub struct LevelRolesSync;

impl ConfigLevelRoles {
    pub async fn run(self, cx: &Cx, guild: u64) -> anyhow::Result<()> {
        let db = &cx.app.db;
        match self {
            Self::Add(c) => {
                if let Some(why) = roles::unassignable(&cx.app, guild, &c.role).await {
                    return cx.reply_ephemeral(why).await;
                }
                if !db.add_level_role(guild, c.level as i32, c.role.id.get()).await? {
                    return cx
                        .reply_ephemeral(format!(
                            "A server can have {MAX_LEVEL_ROLES} level roles. Remove one with `/config level-roles remove` first."
                        ))
                        .await;
                }
                cx.reply_ephemeral(format!(
                    "Members who reach level **{}** now get <@&{}>. Run `/config level-roles sync` to give it to members who are already there.",
                    c.level, c.role.id
                ))
                .await
            }
            Self::Remove(c) => {
                let text = if db.remove_level_role(guild, c.role.get()).await? {
                    format!(
                        "<@&{}> is no longer a level role. Members keep the role they already have.",
                        c.role
                    )
                } else {
                    format!("<@&{}> isn't a level role.", c.role)
                };
                cx.reply_ephemeral(text).await
            }
            Self::List(_) => {
                let rows = db.level_roles(guild).await?;
                if rows.is_empty() {
                    return cx
                        .reply_ephemeral("No level roles yet. Add one with `/config level-roles add`.")
                        .await;
                }
                let stack = db.guild_settings(guild).await?.level_roles_stack;
                let lines: Vec<String> = rows
                    .iter()
                    .map(|r| format!("Level **{}** · <@&{}>", r.level, r.role_id))
                    .collect();
                let embed = EmbedBuilder::new()
                    .color(BRAND)
                    .title("Level roles")
                    .description(lines.join("\n"))
                    .footer(EmbedFooterBuilder::new(format!(
                        "{}/{MAX_LEVEL_ROLES} · {}",
                        rows.len(),
                        if stack {
                            "stack: members keep every role they earn"
                        } else {
                            "replace: members keep only their highest role"
                        }
                    )))
                    .build();
                cx.reply(InteractionResponseData {
                    embeds: Some(vec![embed]),
                    flags: Some(MessageFlags::EPHEMERAL),
                    ..Default::default()
                })
                .await
            }
            Self::Mode(c) => {
                let stack = matches!(c.mode, RoleMode::Stack);
                db.set_level_roles_stack(guild, stack).await?;
                cx.app.forget_guild_settings(guild).await;
                cx.reply_ephemeral(if stack {
                    "Level roles now **stack**: members keep every role they earn."
                } else {
                    "Level roles now **replace** each other: members keep only the role of the highest level they reached."
                })
                .await
            }
            Self::Sync(_) => sync(cx, guild).await,
        }
    }
}

/// Re-applies the level roles to the members with XP here, newest XP first.
async fn sync(cx: &Cx, guild: u64) -> anyhow::Result<()> {
    let rewards = roles::level_rewards(&cx.app, guild).await?;
    if rewards.is_empty() {
        return cx
            .reply_ephemeral("No level roles yet. Add one with `/config level-roles add`.")
            .await;
    }
    // One lookup and a few role changes per member, so this outlasts Discord's first-answer window.
    cx.responder.defer(true).await?;
    let stack = cx.app.db.guild_settings(guild).await?.level_roles_stack;
    let members = cx.app.db.recent_xp_members(guild, SYNC_LIMIT).await?;
    let started = Instant::now();
    let (mut checked, mut updated) = (0usize, 0usize);
    for member in &members {
        if started.elapsed() > SYNC_BUDGET {
            break;
        }
        checked += 1;
        let user = member.user_id as u64;
        let current: Vec<u64> = match cx.app.http.guild_member(Id::new(guild), Id::new(user)).await {
            Ok(response) => match response.model().await {
                Ok(m) => m.roles.iter().map(|r| r.get()).collect(),
                Err(error) => {
                    tracing::warn!(guild, user, %error, "could not read a member during a level role sync");
                    continue;
                }
            },
            Err(error) => {
                if !roles::member_is_gone(&error) {
                    tracing::warn!(guild, user, %error, "could not look up a member during a level role sync");
                }
                continue;
            }
        };
        let (added, removed) =
            roles::apply_level_roles(&cx.app, guild, user, level_of(member.value), &current, &rewards, stack).await;
        if added.len() + removed > 0 {
            updated += 1;
        }
    }
    let early = if checked < members.len() {
        format!(
            " Stopped early after {checked} of {} members; it was taking too long.",
            members.len()
        )
    } else {
        String::new()
    };
    cx.reply_ephemeral(format!(
        "Checked **{checked}** members and updated the level roles of **{updated}**.{early}"
    ))
    .await
}
