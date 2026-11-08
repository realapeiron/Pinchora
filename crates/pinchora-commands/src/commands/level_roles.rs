use std::time::{Duration, Instant};

use pinchora_core::{i18n::Lang, leveling::LevelReward, settings, t};
use pinchora_db::MAX_LEVEL_ROLES;
use twilight_interactions::command::{CommandModel, CommandOption, CreateCommand, CreateOption};
use twilight_model::{
    channel::message::MessageFlags,
    guild::Role,
    http::interaction::InteractionResponseData,
    id::{Id, marker::RoleMarker},
};
use twilight_util::builder::embed::{EmbedBuilder, EmbedFooterBuilder};

use crate::{App, Cx, roles, util::BRAND};

/// Members checked per `sync` run, newest XP first.
const SYNC_LIMIT: i64 = 1_000;
/// Stop a sync before the interaction token (15 minutes) runs out.
const SYNC_BUDGET: Duration = Duration::from_secs(10 * 60);
/// One sync per server at a time, whoever starts it; past this a crashed one no longer blocks the next.
const SYNC_LOCK: Duration = Duration::from_secs(15 * 60);
/// How long a server waits after a sync before it can run another: each one is a Discord call per member.
const SYNC_REST: Duration = Duration::from_secs(5 * 60);

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
                let level = match settings::check_level(c.level) {
                    Ok(level) => level,
                    Err(error) => return cx.reply_ephemeral(error.text(cx.lang())).await,
                };
                if let Some(why) = roles::unassignable(cx, guild, &c.role).await {
                    return cx.reply_ephemeral(why).await;
                }
                if !db.add_level_role(guild, level, c.role.id.get()).await? {
                    return cx
                        .reply_ephemeral(t!(cx.lang(), "level-roles.full", max = MAX_LEVEL_ROLES))
                        .await;
                }
                cx.reply_ephemeral(t!(
                    cx.lang(),
                    "level-roles.added",
                    level = c.level,
                    role = format!("<@&{}>", c.role.id)
                ))
                .await
            }
            Self::Remove(c) => {
                let role = format!("<@&{}>", c.role);
                let text = if db.remove_level_role(guild, c.role.get()).await? {
                    t!(cx.lang(), "level-roles.removed", role = role)
                } else {
                    t!(cx.lang(), "level-roles.not-one", role = role)
                };
                cx.reply_ephemeral(text).await
            }
            Self::List(_) => {
                let rows = db.level_roles(guild).await?;
                if rows.is_empty() {
                    return cx.reply_ephemeral(t!(cx.lang(), "level-roles.empty")).await;
                }
                let stack = db.guild_settings(guild).await?.level_roles_stack;
                let lines: Vec<String> = rows
                    .iter()
                    .map(|r| {
                        t!(
                            cx.lang(),
                            "level-roles.row",
                            level = r.level,
                            role = format!("<@&{}>", r.role_id)
                        )
                    })
                    .collect();
                let embed = EmbedBuilder::new()
                    .color(BRAND)
                    .title(t!(cx.lang(), "level-roles.title"))
                    .description(lines.join("\n"))
                    .footer(EmbedFooterBuilder::new(t!(
                        cx.lang(),
                        "level-roles.footer",
                        count = rows.len(),
                        max = MAX_LEVEL_ROLES,
                        mode = t!(
                            cx.lang(),
                            if stack {
                                "level-roles.footer-stack"
                            } else {
                                "level-roles.footer-replace"
                            }
                        )
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
                cx.reply_ephemeral(t!(
                    cx.lang(),
                    if stack {
                        "level-roles.mode-stack"
                    } else {
                        "level-roles.mode-replace"
                    }
                ))
                .await
            }
            Self::Sync(_) => sync(cx, guild).await,
        }
    }
}

/// Re-applies the level roles to the members with XP here, newest XP first.
async fn sync(cx: &Cx, guild: u64) -> anyhow::Result<()> {
    if roles::level_rewards(&cx.app, guild).await?.is_empty() {
        return cx.reply_ephemeral(t!(cx.lang(), "level-roles.empty")).await;
    }
    // One lookup and a few role changes per member, so this outlasts Discord's first-answer window.
    cx.responder.defer(true).await?;
    let text = match sync_level_roles(&cx.app, guild, |_| {}).await? {
        SyncOutcome::NoRoles => t!(cx.lang(), "level-roles.empty"),
        SyncOutcome::Done(report) => report.summary_in(cx.lang()),
        other => other.refusal_in(cx.lang()).unwrap_or_default(),
    };
    cx.reply_ephemeral(text).await
}

/// What a level role sync did, so far or in the end.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SyncReport {
    /// Members looked at.
    pub checked: usize,
    /// Members whose roles changed.
    pub updated: usize,
    /// Members with XP here, up to the per-run limit.
    pub total: usize,
}

impl SyncReport {
    /// The sentence `/config level-roles sync` ends with.
    pub fn summary(&self) -> String {
        self.summary_in(Lang::En)
    }

    /// [`SyncReport::summary`] in `lang`.
    pub fn summary_in(&self, lang: Lang) -> String {
        let early = if self.checked < self.total {
            t!(lang, "level-roles.early", checked = self.checked, total = self.total)
        } else {
            String::new()
        };
        t!(
            lang,
            "level-roles.summary",
            checked = self.checked,
            updated = self.updated,
            early = early
        )
    }
}

/// How a level role sync ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncOutcome {
    /// The server has no level roles to sync.
    NoRoles,
    /// A sync of this server is running right now, started from Discord or the dashboard.
    Busy,
    /// One ended a moment ago; another may start in this long.
    Resting(Duration),
    Done(SyncReport),
}

impl SyncOutcome {
    /// What to tell whoever asked, when the sync did not run.
    pub fn refusal(&self) -> Option<String> {
        self.refusal_in(Lang::En)
    }

    /// [`SyncOutcome::refusal`] in `lang`.
    pub fn refusal_in(&self, lang: Lang) -> Option<String> {
        match self {
            Self::Busy => Some(t!(lang, "level-roles.busy")),
            Self::Resting(left) => Some(t!(
                lang,
                "level-roles.resting",
                minutes = left.as_secs().div_ceil(60).max(1)
            )),
            Self::NoRoles | Self::Done(_) => None,
        }
    }
}

/// How long until `guild` may sync again, when a sync ended recently.
pub async fn sync_resting(app: &App, guild: u64) -> anyhow::Result<Option<Duration>> {
    let Some(until) = app.cache.get(&format!("levelsync-rest:{guild}")).await? else {
        return Ok(None);
    };
    let left = until.parse::<i64>().unwrap_or(0) - chrono::Utc::now().timestamp();
    Ok((left > 0).then(|| Duration::from_secs(left as u64)))
}

/// `/config level-roles sync` for anyone holding an [`App`] (the dashboard runs it as a job). It takes
/// the server's lock itself, so a sync from Discord and one from the dashboard never run at once, and
/// rests the server for a few minutes after each. `progress` hears the count after every member.
pub async fn sync_level_roles(app: &App, guild: u64, progress: impl FnMut(SyncReport)) -> anyhow::Result<SyncOutcome> {
    let rewards = roles::level_rewards(app, guild).await?;
    if rewards.is_empty() {
        return Ok(SyncOutcome::NoRoles);
    }
    let lock = format!("levelsync:{guild}");
    if app.cache.try_cooldown(&lock, SYNC_LOCK).await?.is_some() {
        return Ok(SyncOutcome::Busy);
    }
    let outcome = async {
        if let Some(left) = sync_resting(app, guild).await? {
            return anyhow::Ok(SyncOutcome::Resting(left));
        }
        let report = sync_with(app, guild, &rewards, progress).await;
        // The rest starts when the sync ends, whether it finished or not.
        let until = (chrono::Utc::now().timestamp() + SYNC_REST.as_secs() as i64).to_string();
        if let Err(error) = app
            .cache
            .put(&format!("levelsync-rest:{guild}"), &until, SYNC_REST)
            .await
        {
            tracing::warn!(%error, guild, "sync rest not saved");
        }
        Ok(SyncOutcome::Done(report?))
    }
    .await;
    if let Err(error) = app.cache.release_cooldown(&lock).await {
        tracing::warn!(%error, guild, "sync lock not released; it runs out by itself");
    }
    outcome
}

async fn sync_with(
    app: &App,
    guild: u64,
    rewards: &[LevelReward],
    mut progress: impl FnMut(SyncReport),
) -> anyhow::Result<SyncReport> {
    let stack = app.db.guild_settings(guild).await?.level_roles_stack;
    let members = app.db.recent_xp_members(guild, SYNC_LIMIT).await?;
    let started = Instant::now();
    let mut report = SyncReport {
        total: members.len(),
        ..Default::default()
    };
    progress(report);
    for member in &members {
        if started.elapsed() > SYNC_BUDGET {
            break;
        }
        report.checked += 1;
        let user = member.user_id as u64;
        let current: Vec<u64> = match app.http.guild_member(Id::new(guild), Id::new(user)).await {
            Ok(response) => match response.model().await {
                Ok(m) => m.roles.iter().map(|r| r.get()).collect(),
                Err(error) => {
                    tracing::warn!(guild, user, %error, "could not read a member during a level role sync");
                    progress(report);
                    continue;
                }
            },
            Err(error) => {
                if !roles::member_is_gone(&error) {
                    tracing::warn!(guild, user, %error, "could not look up a member during a level role sync");
                }
                progress(report);
                continue;
            }
        };
        let (added, removed) = roles::apply_level_roles(
            app,
            guild,
            user,
            app.db.leveling_parity(guild).await?.level_of(member.value),
            &current,
            rewards,
            stack,
        )
        .await;
        if added.len() + removed > 0 {
            report.updated += 1;
        }
        progress(report);
    }
    Ok(report)
}
