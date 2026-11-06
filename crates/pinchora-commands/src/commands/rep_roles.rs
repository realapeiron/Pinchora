//! `/config rep-roles`: roles members earn when their reputation reaches a count.

use pinchora_core::settings;
use pinchora_db::MAX_REP_ROLES;
use twilight_interactions::command::{CommandModel, CreateCommand};
use twilight_model::{
    channel::message::MessageFlags,
    guild::Role,
    http::interaction::InteractionResponseData,
    id::{Id, marker::RoleMarker},
};
use twilight_util::builder::embed::{EmbedBuilder, EmbedFooterBuilder};

use crate::{Cx, roles, util::BRAND};

#[derive(CommandModel, CreateCommand)]
#[command(name = "rep-roles", desc = "Roles members earn at a reputation count")]
#[allow(clippy::large_enum_variant)] // parsed once per interaction, never stored
pub enum ConfigRepRoles {
    #[command(name = "add")]
    Add(RepRolesAdd),
    #[command(name = "remove")]
    Remove(RepRolesRemove),
    #[command(name = "list")]
    List(RepRolesList),
}

#[derive(CommandModel, CreateCommand)]
#[command(name = "add", desc = "Give a role to everyone whose reputation reaches a count")]
pub struct RepRolesAdd {
    /// Reputation that earns the role
    #[command(min_value = 1, max_value = 100_000)]
    reputation: i64,
    /// The role to give (adding it again moves it to the new count)
    role: Role,
}

#[derive(CommandModel, CreateCommand)]
#[command(name = "remove", desc = "Stop giving a role for reputation")]
pub struct RepRolesRemove {
    /// The role to stop giving
    role: Id<RoleMarker>,
}

#[derive(CommandModel, CreateCommand)]
#[command(name = "list", desc = "Show the reputation roles")]
pub struct RepRolesList;

impl ConfigRepRoles {
    pub async fn run(self, cx: &Cx, guild: u64) -> anyhow::Result<()> {
        let db = &cx.app.db;
        match self {
            Self::Add(c) => {
                let reputation = match settings::check_reputation(c.reputation) {
                    Ok(reputation) => reputation,
                    Err(error) => return cx.reply_ephemeral(error.message).await,
                };
                if let Some(why) = roles::unassignable(&cx.app, guild, &c.role).await {
                    return cx.reply_ephemeral(why).await;
                }
                if !db.add_rep_role(guild, reputation, c.role.id.get()).await? {
                    return cx
                        .reply_ephemeral(format!(
                            "A server can have {MAX_REP_ROLES} reputation roles. Remove one with `/config rep-roles remove` first."
                        ))
                        .await;
                }
                cx.reply_ephemeral(format!(
                    "Members who reach **{}** reputation now get <@&{}> the moment they receive a point.",
                    c.reputation, c.role.id
                ))
                .await
            }
            Self::Remove(c) => {
                let text = if db.remove_rep_role(guild, c.role.get()).await? {
                    format!(
                        "<@&{}> is no longer a reputation role. Members keep the role they already have.",
                        c.role
                    )
                } else {
                    format!("<@&{}> isn't a reputation role.", c.role)
                };
                cx.reply_ephemeral(text).await
            }
            Self::List(_) => {
                let rows = db.rep_roles(guild).await?;
                if rows.is_empty() {
                    return cx
                        .reply_ephemeral("No reputation roles yet. Add one with `/config rep-roles add`.")
                        .await;
                }
                let lines: Vec<String> = rows
                    .iter()
                    .map(|r| format!("**{}** reputation · <@&{}>", r.reputation, r.role_id))
                    .collect();
                let embed = EmbedBuilder::new()
                    .color(BRAND)
                    .title("Reputation roles")
                    .description(lines.join("\n"))
                    .footer(EmbedFooterBuilder::new(format!(
                        "{}/{MAX_REP_ROLES} · members keep every role they earn",
                        rows.len()
                    )))
                    .build();
                cx.reply(InteractionResponseData {
                    embeds: Some(vec![embed]),
                    flags: Some(MessageFlags::EPHEMERAL),
                    ..Default::default()
                })
                .await
            }
        }
    }
}
