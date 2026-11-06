//! Moderation: the commands, the numbered cases they open, the member's direct message, the mod-log,
//! the warn ladder and the ticker that lifts temporary bans.
//!
//! Every command checks the moderator's own permission (the one Discord would ask for) from the
//! member permissions Discord sends with the interaction, and the role hierarchy the way Discord does:
//! nobody acts on the owner, on themselves or on the bot, and both the moderator and the bot must sit
//! strictly above the member.

mod cases;
mod channels;
mod members;
pub(crate) mod setup;
mod warns;

use std::{collections::HashMap, sync::Arc, time::Duration as StdDuration};

use chrono::{DateTime, Duration, Utc};
use moka::future::Cache as Moka;
use pinchora_core::{
    logging::LogCategory,
    moderation::{CaseKind, Rank, format_duration, may_act},
};
use pinchora_db::{CaseRow, NewCase, OpenedCase};
use twilight_http::request::AuditLogReason;
use twilight_interactions::command::ResolvedUser;
use twilight_model::{
    channel::message::{AllowedMentions, Embed},
    guild::Permissions,
    id::Id,
    util::Timestamp,
};
use twilight_util::builder::embed::{EmbedBuilder, EmbedFieldBuilder, EmbedFooterBuilder};

pub(crate) use cases::{CaseCommand, ReasonCommand};
pub(crate) use channels::{Lock, Purge, Slowmode, Unlock};
pub(crate) use members::{Ban, Kick, Nick, RoleCommand, Timeout, Unban, Untimeout};
pub(crate) use warns::{Unwarn, Warn, Warnings};

use crate::{
    App, Cx,
    roles::error_code,
    util::{BRAND, DANGER, SUCCESS, WARN, mention, relative, resolved_display_name},
};

/// The name cases give the bot when it acts on its own (the warn ladder, a temporary ban that ends).
pub(crate) const BOT_NAME: &str = "Pinchora";

/// How often the ticker looks for temporary bans that ended, and how many it lifts per look.
const WAKE: StdDuration = StdDuration::from_secs(30);
const BATCH: i64 = 25;
/// The staff log keeps 180 days; the ticker forgets older lines about once an hour.
const STAFF_LOG_DAYS: i64 = 180;
const FORGET_EVERY: u32 = 120;

const UNKNOWN_BAN: u64 = 10026;
const CANNOT_DM: u64 = 50007;

/// What moderation remembers about servers for a little while: the owner and name, the roles with
/// their positions, and the bot's own roles.
pub(crate) struct ModState {
    guilds: Moka<u64, Arc<GuildFacts>>,
    roles: Moka<u64, Arc<HashMap<u64, RoleInfo>>>,
    mine: Moka<u64, Arc<Vec<u64>>>,
}

impl ModState {
    pub(crate) fn new() -> Self {
        Self {
            guilds: Moka::builder()
                .max_capacity(20_000)
                .time_to_live(StdDuration::from_secs(300))
                .build(),
            roles: Moka::builder()
                .max_capacity(20_000)
                .time_to_live(StdDuration::from_secs(30))
                .build(),
            mine: Moka::builder()
                .max_capacity(20_000)
                .time_to_live(StdDuration::from_secs(60))
                .build(),
        }
    }
}

/// A server's owner and name.
pub(crate) struct GuildFacts {
    pub(crate) owner: u64,
    pub(crate) name: String,
}

/// One role, as the hierarchy sees it.
#[derive(Debug, Clone)]
pub(crate) struct RoleInfo {
    pub(crate) position: i64,
}

impl App {
    pub(crate) async fn guild_facts(&self, guild: u64) -> anyhow::Result<Arc<GuildFacts>> {
        self.moderation
            .guilds
            .try_get_with(guild, async {
                let g = self.http.guild(Id::new(guild.max(1))).await?.model().await?;
                anyhow::Ok(Arc::new(GuildFacts {
                    owner: g.owner_id.get(),
                    name: g.name,
                }))
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e:#}"))
    }

    pub(crate) async fn role_table(&self, guild: u64) -> anyhow::Result<Arc<HashMap<u64, RoleInfo>>> {
        self.moderation
            .roles
            .try_get_with(guild, async {
                let roles = self.http.roles(Id::new(guild.max(1))).await?.models().await?;
                anyhow::Ok(Arc::new(
                    roles
                        .into_iter()
                        .map(|r| (r.id.get(), RoleInfo { position: r.position }))
                        .collect(),
                ))
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e:#}"))
    }

    /// The roles the bot holds in a server.
    pub(crate) async fn my_roles(&self, guild: u64) -> anyhow::Result<Arc<Vec<u64>>> {
        self.moderation
            .mine
            .try_get_with(guild, async {
                let me = self
                    .http
                    .guild_member(Id::new(guild.max(1)), Id::new(self.application_id.get()))
                    .await?
                    .model()
                    .await?;
                anyhow::Ok(Arc::new(me.roles.iter().map(|r| r.get()).collect()))
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e:#}"))
    }
}

/// The position of the highest of `held` that the server has; 0 for none.
pub(crate) fn top(table: &HashMap<u64, RoleInfo>, held: impl IntoIterator<Item = u64>) -> i64 {
    held.into_iter()
        .filter_map(|id| table.get(&id).map(|r| r.position))
        .max()
        .unwrap_or(0)
}

/// Whether the invoker has `needed` (or Administrator) here.
pub(crate) fn allowed(cx: &Cx, needed: Permissions) -> bool {
    cx.interaction
        .member
        .as_ref()
        .and_then(|m| m.permissions)
        .is_some_and(|p| p.intersects(needed) || p.contains(Permissions::ADMINISTRATOR))
}

/// The server, when the invoker has one of `needed`; otherwise they are refused privately (`name` names
/// the permission) and `None` comes back. Outside a server everybody is refused.
pub(crate) async fn permitted(cx: &Cx, needed: Permissions, name: &str) -> anyhow::Result<Option<u64>> {
    let Some(guild) = cx.guild_id() else {
        cx.reply_ephemeral("Moderation only works inside a server.").await?;
        return Ok(None);
    };
    if allowed(cx, needed) {
        return Ok(Some(guild));
    }
    cx.reply_ephemeral(format!("You need the {name} permission to do that."))
        .await?;
    Ok(None)
}

/// Who is acting.
pub(crate) struct Moderator {
    pub(crate) id: u64,
    pub(crate) name: String,
    roles: Vec<u64>,
}

impl Moderator {
    pub(crate) fn of(cx: &Cx) -> anyhow::Result<Self> {
        Ok(Self {
            id: cx.author()?.id.get(),
            name: cx.author_display_name(),
            roles: cx
                .interaction
                .member
                .as_ref()
                .map(|m| m.roles.iter().map(|r| r.get()).collect())
                .unwrap_or_default(),
        })
    }
}

/// The ranks of the moderator and the bot, read from Discord (cached).
pub(crate) struct Ranks {
    pub(crate) owner: u64,
    pub(crate) actor: Rank,
    pub(crate) bot: Rank,
    pub(crate) table: Arc<HashMap<u64, RoleInfo>>,
}

/// The sentence for a server whose roles could not be read.
pub(crate) const NO_ROLES: &str = "I couldn't check the server's roles just now. Try again in a moment.";

impl Ranks {
    pub(crate) async fn read(app: &App, guild: u64, moderator: &Moderator) -> Option<Self> {
        let (facts, table, mine) = tokio::join!(app.guild_facts(guild), app.role_table(guild), app.my_roles(guild));
        let (facts, table, mine) = match (facts, table, mine) {
            (Ok(f), Ok(t), Ok(m)) => (f, t, m),
            (f, t, m) => {
                tracing::warn!(guild, facts = ?f.err(), roles = ?t.err(), mine = ?m.err(), "could not read the hierarchy");
                return None;
            }
        };
        Some(Self {
            owner: facts.owner,
            actor: Rank {
                id: moderator.id,
                top: top(&table, moderator.roles.iter().copied()),
            },
            bot: Rank {
                id: app.application_id.get(),
                top: top(&table, mine.iter().copied()),
            },
            table,
        })
    }

    /// Why the moderator may not `verb` `target`, if they may not. A user who is not a member is
    /// below everyone (a ban by id); the owner, the moderator and the bot stay out of reach.
    pub(crate) fn refusal(&self, target: &ResolvedUser, verb: &str) -> Option<String> {
        let top = match &target.member {
            Some(member) => self::top(&self.table, member.roles.iter().map(|r| r.get())),
            None => -1,
        };
        let rank = Rank {
            id: target.resolved.id.get(),
            top,
        };
        may_act(self.owner, self.actor, self.bot, rank)
            .err()
            .map(|r| r.text(verb))
    }

    /// Whether the bot itself may act on `target` (the warn ladder, where the bot is the moderator).
    pub(crate) fn bot_may(&self, target: &ResolvedUser, verb: &str) -> Option<String> {
        let top = target
            .member
            .as_ref()
            .map_or(-1, |m| self::top(&self.table, m.roles.iter().map(|r| r.get())));
        let rank = Rank {
            id: target.resolved.id.get(),
            top,
        };
        may_act(self.owner, self.bot, self.bot, rank)
            .err()
            .map(|r| r.text(verb))
    }
}

/// What an action records, before it has a number.
pub(crate) struct Act {
    pub(crate) kind: CaseKind,
    pub(crate) target: u64,
    pub(crate) target_name: String,
    pub(crate) reason: Option<String>,
    pub(crate) duration: Option<Duration>,
    pub(crate) details: Option<String>,
}

impl Act {
    pub(crate) fn on(kind: CaseKind, target: &ResolvedUser, reason: Option<String>) -> Self {
        Self {
            kind,
            target: target.resolved.id.get(),
            target_name: resolved_display_name(target),
            reason,
            duration: None,
            details: None,
        }
    }

    /// When a temporary ban ends.
    fn expires_at(&self, now: DateTime<Utc>) -> Option<DateTime<Utc>> {
        self.duration.filter(|_| self.kind == CaseKind::Ban).map(|d| now + d)
    }
}

/// Opens the case, tells the member (unless `told` says it was done before the action), posts the
/// mod-log, and returns the case.
pub(crate) async fn record(
    app: &App,
    guild: u64,
    moderator: (u64, &str),
    act: &Act,
    told: Option<bool>,
) -> anyhow::Result<OpenedCase> {
    let opened = app
        .db
        .open_case(&NewCase {
            guild,
            kind: act.kind,
            target: act.target,
            target_name: &act.target_name,
            moderator: moderator.0,
            moderator_name: moderator.1,
            reason: act.reason.as_deref(),
            duration: act.duration,
            expires_at: act.expires_at(app.now()),
            details: act.details.as_deref(),
        })
        .await?;
    let case = &opened.case;
    let told = match told {
        Some(told) => Some(told),
        None if act.kind.tells_member() => Some(tell(app, guild, act).await),
        None => None,
    };
    if let Some(told) = told
        && let Err(error) = app.db.set_case_dm(guild, case.number, told).await
    {
        tracing::warn!(%error, guild, "could not note the direct message");
    }
    post_case(app, guild, case).await;
    metrics::counter!("pinchora_mod_cases_total", "kind" => act.kind.key()).increment(1);
    Ok(opened)
}

/// Writes to the member about what was done, when the server lets Pinchora. Whether it went out.
pub(crate) async fn tell(app: &App, guild: u64, act: &Act) -> bool {
    match app.db.mod_settings(guild).await {
        Ok(settings) if !settings.dm_members => return false,
        Ok(_) => {}
        Err(error) => {
            tracing::warn!(%error, guild, "mod settings unavailable");
            return false;
        }
    }
    let server = match app.guild_facts(guild).await {
        Ok(facts) => facts.name.clone(),
        Err(_) => "a server".to_owned(),
    };
    let embed = dm_embed(act, &server, app.now());
    let sent = async {
        let channel = app
            .http
            .create_private_channel(Id::new(act.target.max(1)))
            .await?
            .model()
            .await?;
        let mentions = AllowedMentions::default();
        app.http
            .create_message(channel.id)
            .embeds(&[embed])
            .allowed_mentions(Some(&mentions))
            .await?;
        anyhow::Ok(())
    }
    .await;
    match sent {
        Ok(()) => true,
        Err(error) => {
            let closed = error
                .downcast_ref::<twilight_http::Error>()
                .and_then(error_code)
                .is_some_and(|code| code == CANNOT_DM);
            tracing::debug!(%error, closed, guild, "could not tell the member");
            false
        }
    }
}

/// Where a server's cases are posted: its mod-log, else the server log's moderation channel.
async fn mod_log_channel(app: &App, guild: u64) -> Option<u64> {
    match app.db.mod_settings(guild).await {
        Ok(settings) if settings.log_channel_id.is_some() => return settings.log_channel_id.map(|c| c as u64),
        Ok(_) => {}
        Err(error) => tracing::warn!(%error, guild, "mod settings unavailable"),
    }
    match app.log_config(guild).await {
        Ok(config) => config.category_channel(LogCategory::Moderation),
        Err(error) => {
            tracing::warn!(%error, guild, "log settings unavailable");
            None
        }
    }
}

/// Posts a case to the mod-log and remembers the post. A mod-log that is gone or closed is skipped.
pub(crate) async fn post_case(app: &App, guild: u64, case: &CaseRow) {
    let Some(channel) = mod_log_channel(app, guild).await else {
        return;
    };
    let mentions = AllowedMentions::default();
    let posted = app
        .http
        .create_message(Id::new(channel.max(1)))
        .embeds(&[case_embed(case)])
        .allowed_mentions(Some(&mentions))
        .await;
    match posted {
        Ok(response) => match response.model().await {
            Ok(message) => {
                if let Err(error) = app
                    .db
                    .set_case_post(guild, case.number, channel, message.id.get())
                    .await
                {
                    tracing::warn!(%error, guild, "could not remember the mod-log post");
                }
            }
            Err(error) => tracing::debug!(%error, guild, "mod-log post unreadable"),
        },
        Err(error) => tracing::debug!(%error, guild, channel, "could not post to the mod-log"),
    }
}

/// Brings a case's mod-log post up to date (a new reason, from `/reason` or the dashboard).
pub async fn refresh_post(app: &App, case: &CaseRow) {
    let (Some(channel), Some(message)) = (case.log_channel_id, case.log_message_id) else {
        return;
    };
    let embed = case_embed(case);
    if let Err(error) = app
        .http
        .update_message(Id::new((channel as u64).max(1)), Id::new((message as u64).max(1)))
        .embeds(Some(&[embed]))
        .await
    {
        tracing::debug!(%error, case = case.number, "could not edit the mod-log post");
    }
}

/// The colour of a kind of case.
pub(crate) fn color(kind: Option<CaseKind>) -> u32 {
    match kind {
        Some(CaseKind::Ban | CaseKind::Kick) => DANGER,
        Some(CaseKind::Timeout | CaseKind::Warn | CaseKind::Purge | CaseKind::Lock) => WARN,
        Some(CaseKind::Unban | CaseKind::Untimeout | CaseKind::Unwarn | CaseKind::Unlock) => SUCCESS,
        _ => BRAND,
    }
}

/// What a case did, in one line: "**Mia** was banned for 7d".
pub(crate) fn summary(case: &CaseRow) -> String {
    let who = format!("**{}**", case.target_name.replace('*', ""));
    let channel = format!("<#{}>", case.target_id);
    let for_how_long = case
        .duration()
        .map(|d| format!(" for {}", format_duration(d)))
        .unwrap_or_default();
    let details = case.details.as_deref().unwrap_or_default();
    match case.kind() {
        Some(CaseKind::Ban) => format!("{who} was banned{for_how_long}"),
        Some(CaseKind::Unban) => format!("{who} was unbanned"),
        Some(CaseKind::Kick) => format!("{who} was kicked"),
        Some(CaseKind::Timeout) => format!("{who} was timed out{for_how_long}"),
        Some(CaseKind::Untimeout) => format!("{who}'s timeout was removed"),
        Some(CaseKind::Warn) => format!("{who} was warned"),
        Some(CaseKind::Unwarn) => format!("A warn of {who} was removed ({details})"),
        Some(CaseKind::Purge) => format!("{details} in {channel}"),
        Some(CaseKind::Slowmode) => format!("Slowmode in {channel}: {details}"),
        Some(CaseKind::Lock) => format!("{channel} was locked"),
        Some(CaseKind::Unlock) => format!("{channel} was unlocked"),
        Some(CaseKind::Nick) => format!("{who}: {details}"),
        Some(CaseKind::RoleAdd) => format!("{who} was given {details}"),
        Some(CaseKind::RoleRemove) => format!("{who} lost {details}"),
        None => format!("{} on {who}", case.kind),
    }
}

fn timestamp(at: DateTime<Utc>) -> Option<Timestamp> {
    Timestamp::from_secs(at.timestamp()).ok()
}

/// The mod-log entry, and what `/case` shows.
pub(crate) fn case_embed(case: &CaseRow) -> Embed {
    let kind = case.kind();
    let label: &str = match kind {
        Some(kind) => kind.label(),
        None => &case.kind,
    };
    let target = if kind.is_some_and(CaseKind::on_channel) {
        format!("<#{}>", case.target_id)
    } else {
        format!("{} · {}", mention(case.target_id), case.target_name)
    };
    let moderator = if case.moderator_id == 0 {
        case.moderator_name.clone()
    } else {
        format!("{} · {}", mention(case.moderator_id), case.moderator_name)
    };
    let mut embed = EmbedBuilder::new()
        .color(color(kind))
        .title(format!("Case #{} · {label}", case.number))
        .description(summary(case))
        .field(
            EmbedFieldBuilder::new(
                if kind.is_some_and(CaseKind::on_channel) {
                    "Channel"
                } else {
                    "Member"
                },
                target,
            )
            .inline(),
        )
        .field(EmbedFieldBuilder::new("Moderator", moderator).inline())
        .field(EmbedFieldBuilder::new(
            "Reason",
            case.reason.clone().unwrap_or_else(|| "*No reason given*".to_owned()),
        ));
    if let Some(duration) = case.duration() {
        let mut text = format_duration(duration);
        if let Some(ends) = case.expires_at {
            text.push_str(&format!(" · ends {}", relative(ends)));
        }
        embed = embed.field(EmbedFieldBuilder::new("Duration", text).inline());
    }
    if case.kind() == Some(CaseKind::Warn) && !case.active {
        embed = embed.field(EmbedFieldBuilder::new("Status", "Removed").inline());
    }
    if case.dm_sent == Some(false) {
        embed = embed.field(EmbedFieldBuilder::new("Direct message", "Not delivered").inline());
    }
    embed = embed.footer(EmbedFooterBuilder::new(format!("ID {}", case.target_id)));
    if let Some(at) = timestamp(case.created_at) {
        embed = embed.timestamp(at);
    }
    embed.build()
}

/// The answer in the channel: what was done, the case number, and anything else worth a line.
pub(crate) fn reply_embed(case: &CaseRow, extra: &[String]) -> Embed {
    let mut text = format!("{} · case #{}", summary(case), case.number);
    if let Some(reason) = &case.reason {
        text.push_str(&format!("\n> {}", reason.replace('\n', " ")));
    }
    for line in extra {
        text.push('\n');
        text.push_str(line);
    }
    EmbedBuilder::new().color(color(case.kind())).description(text).build()
}

/// The direct message to the member.
fn dm_embed(act: &Act, server: &str, now: DateTime<Utc>) -> Embed {
    let server = format!("**{}**", server.replace('*', ""));
    let (title, color) = match act.kind {
        CaseKind::Ban if act.duration.is_some() => (format!("You were banned from {server} for a while"), DANGER),
        CaseKind::Ban => (format!("You were banned from {server}"), DANGER),
        CaseKind::Kick => (format!("You were kicked from {server}"), DANGER),
        CaseKind::Timeout => (format!("You were timed out in {server}"), WARN),
        CaseKind::Untimeout => (format!("Your timeout in {server} was removed"), SUCCESS),
        CaseKind::Warn => (format!("You were warned in {server}"), WARN),
        _ => (format!("A moderator acted on your membership of {server}"), BRAND),
    };
    let mut embed = EmbedBuilder::new()
        .color(color)
        .description(title)
        .field(EmbedFieldBuilder::new(
            "Reason",
            act.reason.clone().unwrap_or_else(|| "*No reason given*".to_owned()),
        ));
    if let Some(duration) = act.duration {
        embed = embed.field(EmbedFieldBuilder::new(
            "Duration",
            format!("{} · ends {}", format_duration(duration), relative(now + duration)),
        ));
    }
    embed.build()
}

/// Lifts the temporary bans whose time is up: each is claimed by one pod, lifted on Discord, and
/// closed with an unban case. A ban Discord no longer has counts as lifted; one Discord refuses to
/// lift waits an hour.
pub async fn lift_expired_bans(app: &Arc<App>) -> anyhow::Result<usize> {
    let mut lifted = 0;
    loop {
        let due = app.db.claim_expired_bans(app.now(), BATCH).await?;
        let full = due.len() as i64 == BATCH;
        for ban in due {
            let guild = ban.guild_id as u64;
            let done = app
                .http
                .delete_ban(Id::new(guild.max(1)), Id::new((ban.target_id as u64).max(1)))
                .reason("Temporary ban ended")
                .await;
            match done {
                Ok(_) => {}
                Err(error) if error_code(&error) == Some(UNKNOWN_BAN) => {}
                Err(error) => {
                    tracing::warn!(%error, guild, case = ban.number, "could not lift a temporary ban");
                    app.db.postpone_lift(guild, ban.number).await?;
                    continue;
                }
            }
            let act = Act {
                kind: CaseKind::Unban,
                target: ban.target_id as u64,
                target_name: ban.target_name.clone(),
                reason: Some(format!("Temporary ban ended (case #{})", ban.number)),
                duration: None,
                details: None,
            };
            record(app, guild, (app.application_id.get(), BOT_NAME), &act, None).await?;
            lifted += 1;
        }
        if !full {
            break;
        }
    }
    metrics::counter!("pinchora_temp_bans_lifted_total").increment(lifted as u64);
    Ok(lifted)
}

/// Runs [`lift_expired_bans`] every 30 seconds on every pod, and forgets old staff log lines now and then.
pub fn spawn_ticker(app: Arc<App>) {
    tokio::spawn(async move {
        let mut wake = tokio::time::interval(WAKE);
        wake.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let mut rounds = 0u32;
        loop {
            wake.tick().await;
            if let Err(error) = lift_expired_bans(&app).await {
                tracing::warn!(%error, "temporary ban tick failed");
            }
            rounds = rounds.wrapping_add(1);
            if rounds.is_multiple_of(FORGET_EVERY) {
                let before = app.now() - Duration::days(STAFF_LOG_DAYS);
                if let Err(error) = app.db.forget_staff_actions_before(before).await {
                    tracing::warn!(%error, "old staff actions not forgotten");
                }
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn case(kind: &str, details: Option<&str>, duration: Option<i64>) -> CaseRow {
        CaseRow {
            guild_id: 1,
            number: 7,
            kind: kind.to_owned(),
            target_id: 42,
            target_name: "Mia".to_owned(),
            moderator_id: 9,
            moderator_name: "Mod".to_owned(),
            reason: Some("spam".to_owned()),
            duration_secs: duration,
            expires_at: None,
            details: details.map(str::to_owned),
            active: true,
            dm_sent: Some(false),
            log_channel_id: None,
            log_message_id: None,
            created_at: Utc::now(),
            updated_at: None,
        }
    }

    #[test]
    fn every_kind_reads_as_a_sentence() {
        assert_eq!(summary(&case("ban", None, Some(604_800))), "**Mia** was banned for 1w");
        assert_eq!(
            summary(&case("timeout", None, Some(3_600))),
            "**Mia** was timed out for 1h"
        );
        assert_eq!(
            summary(&case("purge", Some("Deleted 12 messages"), None)),
            "Deleted 12 messages in <#42>"
        );
        assert_eq!(summary(&case("lock", None, None)), "<#42> was locked");
        assert_eq!(
            summary(&case("role_add", Some("<@&5>"), None)),
            "**Mia** was given <@&5>"
        );
        for kind in CaseKind::ALL {
            assert!(!summary(&case(kind.key(), Some("x"), None)).is_empty());
        }
    }

    #[test]
    fn the_mod_log_entry_has_the_case_number_moderator_and_reason() {
        let embed = case_embed(&case("ban", None, Some(86_400)));
        assert_eq!(embed.title.as_deref(), Some("Case #7 · Ban"));
        let field = |name: &str| embed.fields.iter().find(|f| f.name == name).map(|f| f.value.clone());
        assert_eq!(field("Member").as_deref(), Some("<@42> · Mia"));
        assert_eq!(field("Moderator").as_deref(), Some("<@9> · Mod"));
        assert_eq!(field("Reason").as_deref(), Some("spam"));
        assert_eq!(field("Duration").as_deref(), Some("1d"));
        assert_eq!(field("Direct message").as_deref(), Some("Not delivered"));
        let channel = case_embed(&case("lock", None, None));
        assert_eq!(channel.fields[0].name, "Channel");
    }
}
