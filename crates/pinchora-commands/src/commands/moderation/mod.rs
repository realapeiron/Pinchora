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
pub(crate) mod words;

use std::{collections::HashMap, sync::Arc, time::Duration as StdDuration};

use chrono::{DateTime, Duration, Utc};
use moka::future::Cache as Moka;
use pinchora_core::{
    i18n::Lang,
    logging::LogCategory,
    moderation::{CaseKind, Rank, may_act},
    t,
};
use pinchora_db::{CaseRow, NewCase, OpenedCase};
use twilight_http::request::AuditLogReason;
use twilight_interactions::command::ResolvedUser;
use twilight_model::{
    channel::message::{AllowedMentions, Embed},
    guild::Permissions,
    id::{
        Id,
        marker::{ChannelMarker, MessageMarker},
    },
    util::Timestamp,
};
use twilight_util::builder::embed::{EmbedBuilder, EmbedFieldBuilder, EmbedFooterBuilder};

pub(crate) use cases::{CaseCommand, ReasonCommand};
pub(crate) use channels::{Lock, Purge, Slowmode, Unlock};
pub(crate) use members::{Ban, Kick, Nick, RoleCommand, Timeout, Unban, Untimeout};
pub(crate) use warns::{Unwarn, Warn, Warnings};

use crate::{
    App, Cx,
    leveling::private_channel,
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
const UNKNOWN_MEMBER: u64 = 10007;
const CANNOT_DM: u64 = 50007;

/// Permissions that make a member staff for the staff log: the ones that run or police a server.
const STAFF_PERMISSIONS: Permissions = Permissions::ADMINISTRATOR
    .union(Permissions::MANAGE_GUILD)
    .union(Permissions::MANAGE_ROLES)
    .union(Permissions::MANAGE_CHANNELS)
    .union(Permissions::MANAGE_WEBHOOKS)
    .union(Permissions::MANAGE_GUILD_EXPRESSIONS)
    .union(Permissions::MANAGE_EVENTS)
    .union(Permissions::MANAGE_THREADS)
    .union(Permissions::MANAGE_MESSAGES)
    .union(Permissions::MANAGE_NICKNAMES)
    .union(Permissions::KICK_MEMBERS)
    .union(Permissions::BAN_MEMBERS)
    .union(Permissions::MODERATE_MEMBERS);

/// What moderation remembers about servers for a little while: the roles with their positions and
/// permissions, the bot's own roles, and who is staff. The owner comes from [`App::guild_facts`].
pub(crate) struct ModState {
    roles: Moka<u64, Arc<HashMap<u64, RoleInfo>>>,
    mine: Moka<u64, Arc<Vec<u64>>>,
    staff: Moka<(u64, u64), bool>,
}

impl ModState {
    pub(crate) fn new() -> Self {
        Self {
            roles: Moka::builder()
                .max_capacity(20_000)
                .time_to_live(StdDuration::from_secs(30))
                .build(),
            mine: Moka::builder()
                .max_capacity(20_000)
                .time_to_live(StdDuration::from_secs(60))
                .build(),
            staff: Moka::builder()
                .max_capacity(50_000)
                .time_to_live(StdDuration::from_secs(60))
                .build(),
        }
    }
}

/// One role, as the hierarchy and the staff check see it.
#[derive(Debug, Clone)]
pub(crate) struct RoleInfo {
    pub(crate) position: i64,
    pub(crate) permissions: Permissions,
}

impl App {
    pub(crate) async fn role_table(&self, guild: u64) -> anyhow::Result<Arc<HashMap<u64, RoleInfo>>> {
        self.moderation
            .roles
            .try_get_with(guild, async {
                let roles = self.http.roles(Id::new(guild.max(1))).await?.models().await?;
                anyhow::Ok(Arc::new(
                    roles
                        .into_iter()
                        .map(|r| {
                            let info = RoleInfo {
                                position: r.position,
                                permissions: r.permissions,
                            };
                            (r.id.get(), info)
                        })
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

    /// Whether `user` is staff on `guild`: the owner, or a member whose roles (with @everyone's) hold
    /// one of [`STAFF_PERMISSIONS`]. Kept a minute; `false` when Discord could not say.
    pub(crate) async fn is_staff(&self, guild: u64, user: u64) -> bool {
        if let Some(known) = self.moderation.staff.get(&(guild, user)).await {
            return known;
        }
        let found = async {
            if self.guild_facts(guild).await.is_some_and(|f| f.owner == user) {
                return Some(true);
            }
            let table = self.role_table(guild).await.ok()?;
            let member = self
                .http
                .guild_member(Id::new(guild.max(1)), Id::new(user.max(1)))
                .await
                .ok()?
                .model()
                .await
                .ok()?;
            let held = member.roles.iter().map(|r| r.get()).chain([guild]);
            let permissions = held
                .filter_map(|r| table.get(&r))
                .fold(Permissions::empty(), |all, r| all | r.permissions);
            Some(permissions.intersects(STAFF_PERMISSIONS))
        }
        .await;
        match found {
            Some(staff) => {
                self.moderation.staff.insert((guild, user), staff).await;
                staff
            }
            None => false,
        }
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

/// Whether the invoker holds Administrator here.
pub(crate) fn administrator(cx: &Cx) -> bool {
    cx.interaction
        .member
        .as_ref()
        .and_then(|m| m.permissions)
        .is_some_and(|p| p.contains(Permissions::ADMINISTRATOR))
}

/// The server, when the invoker has one of `needed`; otherwise they are refused privately (`name` is
/// the permission's key in `[mod.perm]`) and `None` comes back. Outside a server everybody is refused.
pub(crate) async fn permitted(cx: &Cx, needed: Permissions, name: &str) -> anyhow::Result<Option<u64>> {
    let Some(guild) = cx.guild_id() else {
        cx.reply_ephemeral(t!(cx.lang(), "mod.guild-only")).await?;
        return Ok(None);
    };
    if allowed(cx, needed) {
        return Ok(Some(guild));
    }
    let permission = words::permission(cx.lang(), name);
    cx.reply_ephemeral(t!(cx.lang(), "mod.needs", permission = permission))
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

impl Ranks {
    pub(crate) async fn read(app: &App, guild: u64, moderator: &Moderator) -> Option<Self> {
        let (facts, table, mine) = tokio::join!(app.guild_facts(guild), app.role_table(guild), app.my_roles(guild));
        let (facts, table, mine) = match (facts, table, mine) {
            (Some(f), Ok(t), Ok(m)) => (f, t, m),
            (f, t, m) => {
                tracing::warn!(guild, facts = f.is_some(), roles = ?t.err(), mine = ?m.err(), "could not read the hierarchy");
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

    /// Why the moderator may not `verb` (a key of `[mod.verb]`) `target`, if they may not. A user who
    /// is not a member is below everyone (a ban by id); the owner, the moderator and the bot stay out of
    /// reach.
    pub(crate) fn refusal(&self, lang: Lang, target: &ResolvedUser, verb: &str) -> Option<String> {
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
            .map(|r| words::refusal(lang, r, verb))
    }

    /// Whether the bot itself may act on `target` (the warn ladder, where the bot is the moderator).
    pub(crate) fn bot_may(&self, lang: Lang, target: &ResolvedUser, verb: &str) -> Option<String> {
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
            .map(|r| words::refusal(lang, r, verb))
    }

    /// Where `user` stands, asked of Discord: their highest role, or below everyone when they are not
    /// a member (any more). `None` when Discord could not say.
    pub(crate) async fn of(&self, app: &App, guild: u64, user: u64) -> Option<Rank> {
        match app.http.guild_member(Id::new(guild.max(1)), Id::new(user.max(1))).await {
            Ok(response) => {
                let member = response.model().await.ok()?;
                let top = self::top(&self.table, member.roles.iter().map(|r| r.get()));
                Some(Rank { id: user, top })
            }
            Err(error) if error_code(&error) == Some(UNKNOWN_MEMBER) => Some(Rank { id: user, top: -1 }),
            Err(error) => {
                tracing::debug!(%error, guild, "could not look a member up");
                None
            }
        }
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
        None if act.kind.tells_member() => Some(tell(app, guild, act).await.is_some()),
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

/// A direct message that went out, so it can be taken back when the action fails after it.
pub(crate) struct Told {
    channel: Id<ChannelMarker>,
    message: Id<MessageMarker>,
    lang: Lang,
    server: String,
}

/// The language a member reads Pinchora in: their own pick, else the server's.
async fn member_lang(app: &App, guild: u64, user: u64) -> Lang {
    match app.user_lang_choice(user).await {
        Some(lang) => lang,
        None => app.guild_lang(guild).await,
    }
}

/// Writes to the member about what was done, when the server lets Pinchora and the member takes
/// direct messages from servers (`/privacy`). The message, when it went out.
pub(crate) async fn tell(app: &App, guild: u64, act: &Act) -> Option<Told> {
    match app.db.mod_settings(guild).await {
        Ok(settings) if !settings.dm_members => return None,
        Ok(_) => {}
        Err(error) => {
            tracing::warn!(%error, guild, "mod settings unavailable");
            return None;
        }
    }
    let channel = private_channel(app, act.target).await?;
    let lang = member_lang(app, guild, act.target).await;
    let server = match app.guild_facts(guild).await {
        Some(facts) => facts.name.replace('*', ""),
        None => t!(lang, "mod.dm.a-server"),
    };
    let embed = dm_embed(act, &server, app.now(), lang);
    let mentions = AllowedMentions::default();
    let sent = app
        .http
        .create_message(channel)
        .embeds(&[embed])
        .allowed_mentions(Some(&mentions))
        .await;
    let message = match sent {
        Ok(response) => response.model().await.ok()?,
        Err(error) => {
            let closed = error_code(&error) == Some(CANNOT_DM);
            tracing::debug!(%error, closed, guild, "could not tell the member");
            return None;
        }
    };
    Some(Told {
        channel,
        message: message.id,
        lang,
        server,
    })
}

/// Turns a direct message sent before a ban or kick into one that says it did not go through.
pub(crate) async fn take_back(app: &App, told: &Told, kind: CaseKind) {
    let key = if kind == CaseKind::Kick {
        "mod.dm.kick-failed"
    } else {
        "mod.dm.ban-failed"
    };
    let text = t!(told.lang, key, server = told.server);
    let embed = EmbedBuilder::new().color(BRAND).description(text).build();
    if let Err(error) = app
        .http
        .update_message(told.channel, told.message)
        .embeds(Some(&[embed]))
        .await
    {
        tracing::debug!(%error, "could not take the direct message back");
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

/// Posts a case to the mod-log, in the server's language, and remembers the post. A mod-log that is
/// gone or closed is skipped.
pub(crate) async fn post_case(app: &App, guild: u64, case: &CaseRow) {
    let Some(channel) = mod_log_channel(app, guild).await else {
        return;
    };
    let embed = case_embed(case, app.guild_lang(guild).await);
    let mentions = AllowedMentions::default();
    let posted = app
        .http
        .create_message(Id::new(channel.max(1)))
        .embeds(&[embed])
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
    let embed = case_embed(case, app.guild_lang(case.guild_id as u64).await);
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
pub(crate) fn summary(case: &CaseRow, lang: Lang) -> String {
    let who = format!("**{}**", case.target_name.replace('*', ""));
    let channel = format!("<#{}>", case.target_id);
    let details = case.details.as_deref().unwrap_or_default();
    let for_how_long = case.duration().map(|d| words::duration(lang, d));
    match (case.kind(), for_how_long) {
        (Some(CaseKind::Ban), Some(duration)) => t!(lang, "mod.summary.ban-for", who = who, duration = duration),
        (Some(CaseKind::Timeout), Some(duration)) => {
            t!(lang, "mod.summary.timeout-for", who = who, duration = duration)
        }
        (Some(CaseKind::Purge | CaseKind::Slowmode | CaseKind::Lock | CaseKind::Unlock), _) => {
            let key = format!("mod.summary.{}", case.kind);
            t!(lang, &key, channel = channel, details = details)
        }
        (Some(kind), _) => t!(
            lang,
            &format!("mod.summary.{}", kind.key()),
            who = who,
            details = details
        ),
        (None, _) => t!(lang, "mod.summary.other", kind = case.kind, who = who),
    }
}

fn timestamp(at: DateTime<Utc>) -> Option<Timestamp> {
    Timestamp::from_secs(at.timestamp()).ok()
}

/// The mod-log entry, and what `/case` shows.
pub(crate) fn case_embed(case: &CaseRow, lang: Lang) -> Embed {
    let kind = case.kind();
    let label = words::kind(lang, kind, &case.kind);
    let on_channel = kind.is_some_and(CaseKind::on_channel);
    let target = if on_channel {
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
        .title(t!(lang, "mod.case.title", number = case.number, label = label))
        .description(summary(case, lang))
        .field(
            EmbedFieldBuilder::new(
                t!(
                    lang,
                    if on_channel {
                        "mod.case.channel"
                    } else {
                        "mod.case.member"
                    }
                ),
                target,
            )
            .inline(),
        )
        .field(EmbedFieldBuilder::new(t!(lang, "mod.case.moderator"), moderator).inline())
        .field(EmbedFieldBuilder::new(
            t!(lang, "mod.case.reason"),
            case.reason.clone().unwrap_or_else(|| t!(lang, "mod.case.no-reason")),
        ));
    if let Some(duration) = case.duration() {
        let text = match case.expires_at {
            Some(ends) => t!(
                lang,
                "mod.case.ends",
                duration = words::duration(lang, duration),
                when = relative(ends)
            ),
            None => words::duration(lang, duration),
        };
        embed = embed.field(EmbedFieldBuilder::new(t!(lang, "mod.case.duration"), text).inline());
    }
    if case.kind() == Some(CaseKind::Warn) && !case.active {
        embed = embed.field(EmbedFieldBuilder::new(t!(lang, "mod.case.status"), t!(lang, "mod.case.removed")).inline());
    }
    if case.dm_sent == Some(false) {
        embed =
            embed.field(EmbedFieldBuilder::new(t!(lang, "mod.case.dm"), t!(lang, "mod.case.not-delivered")).inline());
    }
    embed = embed.footer(EmbedFooterBuilder::new(t!(lang, "mod.case.id", id = case.target_id)));
    if let Some(at) = timestamp(case.created_at) {
        embed = embed.timestamp(at);
    }
    embed.build()
}

/// The answer in the channel: what was done, the case number, and anything else worth a line.
pub(crate) fn reply_embed(case: &CaseRow, extra: &[String], lang: Lang) -> Embed {
    let mut text = t!(
        lang,
        "mod.case.reply",
        summary = summary(case, lang),
        number = case.number
    );
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
fn dm_embed(act: &Act, server: &str, now: DateTime<Utc>, lang: Lang) -> Embed {
    let (key, color) = match act.kind {
        CaseKind::Ban if act.duration.is_some() => ("mod.dm.ban-for", DANGER),
        CaseKind::Ban => ("mod.dm.ban", DANGER),
        CaseKind::Kick => ("mod.dm.kick", DANGER),
        CaseKind::Timeout => ("mod.dm.timeout", WARN),
        CaseKind::Untimeout => ("mod.dm.untimeout", SUCCESS),
        CaseKind::Warn => ("mod.dm.warn", WARN),
        _ => ("mod.dm.other", BRAND),
    };
    let mut embed = EmbedBuilder::new()
        .color(color)
        .description(t!(lang, key, server = server))
        .field(EmbedFieldBuilder::new(
            t!(lang, "mod.case.reason"),
            act.reason.clone().unwrap_or_else(|| t!(lang, "mod.case.no-reason")),
        ));
    if let Some(duration) = act.duration {
        let text = t!(
            lang,
            "mod.case.ends",
            duration = words::duration(lang, duration),
            when = relative(now + duration)
        );
        embed = embed.field(EmbedFieldBuilder::new(t!(lang, "mod.case.duration"), text));
    }
    embed.build()
}

/// Lifts the temporary bans whose time is up: each is claimed by one pod, ended in the database right
/// before Discord is asked (a ban lifted or replaced meanwhile is skipped), lifted on Discord, and
/// closed with an unban case. A ban Discord no longer has counts as lifted; one Discord refuses to
/// lift stands again and waits an hour.
pub async fn lift_expired_bans(app: &Arc<App>) -> anyhow::Result<usize> {
    let mut lifted = 0;
    loop {
        let due = app.db.claim_expired_bans(app.now(), BATCH).await?;
        let full = due.len() as i64 == BATCH;
        for ban in due {
            let guild = ban.guild_id as u64;
            // Taken off right before Discord is asked: a ban lifted or replaced since the claim stays.
            if !app.db.close_ban(guild, ban.number).await? {
                continue;
            }
            let lang = app.guild_lang(guild).await;
            let audit = t!(lang, "mod.temp-ban-ended-audit");
            let done = app
                .http
                .delete_ban(Id::new(guild.max(1)), Id::new((ban.target_id as u64).max(1)))
                .reason(&audit)
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
                reason: Some(t!(lang, "mod.temp-ban-ended", number = ban.number)),
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

/// Someone lifted a member's ban on Discord: by hand, another bot or Pinchora. Unless the member is
/// banned again by now, none of their ban cases stands any more, so the ticker never lifts a ban made
/// after this one ended. A ban that stands again is left to the audit log entry ([`unbanned_by`]).
pub(crate) async fn ban_removed(app: &App, guild: u64, user: u64) {
    match app.http.ban(Id::new(guild.max(1)), Id::new(user.max(1))).await {
        Ok(_) => return,
        Err(error) if error_code(&error) == Some(UNKNOWN_BAN) => {}
        Err(error) => tracing::debug!(%error, guild, "could not check the ban; ending the cases anyway"),
    }
    if let Err(error) = app.db.end_bans(guild, user, None).await {
        tracing::warn!(%error, guild, "ban cases not ended");
    }
}

/// The audit log says `actor` lifted a member's ban at `at`. When that was not Pinchora (whose own
/// unbans close their cases), the bans opened before it end: a ban made since, by anyone, stands.
pub(crate) async fn unbanned_by(app: &App, guild: u64, user: u64, actor: u64, at: DateTime<Utc>) {
    if actor == app.application_id.get() {
        return;
    }
    if let Err(error) = app.db.end_bans(guild, user, Some(at)).await {
        tracing::warn!(%error, guild, "ban cases not ended");
    }
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
        assert_eq!(
            summary(&case("ban", None, Some(604_800)), Lang::En),
            "**Mia** was banned for 1w"
        );
        assert_eq!(
            summary(&case("timeout", None, Some(3_600)), Lang::En),
            "**Mia** was timed out for 1h"
        );
        assert_eq!(
            summary(&case("purge", Some("Deleted 12 messages"), None), Lang::En),
            "Deleted 12 messages in <#42>"
        );
        assert_eq!(summary(&case("lock", None, None), Lang::En), "<#42> was locked");
        assert_eq!(
            summary(&case("role_add", Some("<@&5>"), None), Lang::En),
            "**Mia** was given <@&5>"
        );
        for kind in CaseKind::ALL {
            assert!(!summary(&case(kind.key(), Some("x"), None), Lang::En).is_empty());
        }
    }

    #[test]
    fn the_mod_log_entry_has_the_case_number_moderator_and_reason() {
        let embed = case_embed(&case("ban", None, Some(86_400)), Lang::En);
        assert_eq!(embed.title.as_deref(), Some("Case #7 · Ban"));
        let field = |name: &str| embed.fields.iter().find(|f| f.name == name).map(|f| f.value.clone());
        assert_eq!(field("Member").as_deref(), Some("<@42> · Mia"));
        assert_eq!(field("Moderator").as_deref(), Some("<@9> · Mod"));
        assert_eq!(field("Reason").as_deref(), Some("spam"));
        assert_eq!(field("Duration").as_deref(), Some("1d"));
        assert_eq!(field("Direct message").as_deref(), Some("Not delivered"));
        let channel = case_embed(&case("lock", None, None), Lang::En);
        assert_eq!(channel.fields[0].name, "Channel");
        let turkish = case_embed(&case("ban", None, Some(86_400)), Lang::Tr);
        assert_eq!(turkish.title.as_deref(), Some("Vaka #7 · Yasak"));
        assert_eq!(turkish.description.as_deref(), Some("**Mia** 1 gün süreyle yasaklandı"));
    }
}
