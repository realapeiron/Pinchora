//! The commands that act on one member: `/ban`, `/unban`, `/kick`, `/timeout`, `/untimeout`, `/nick`
//! and `/role add|remove`.

use chrono::Duration;
use pinchora_core::{
    i18n::Lang,
    moderation::{CaseKind, check_reason, check_temp_ban, check_timeout, role_refusal_for},
    server_events::parse_duration,
    settings::{Grantor, RoleFacts},
    t,
};
use twilight_http::request::AuditLogReason;
use twilight_interactions::command::{CommandModel, CreateCommand, ResolvedUser};
use twilight_model::{
    channel::message::{AllowedMentions, Embed, MessageFlags},
    guild::{Permissions, Role},
    http::interaction::InteractionResponseData,
    id::Id,
    util::Timestamp,
};

use super::{Act, Moderator, Ranks, administrator, permitted, record, reply_embed, take_back, tell};
use crate::{Cx, roles::error_code};

fn ban_members() -> Permissions {
    Permissions::BAN_MEMBERS
}

fn kick_members() -> Permissions {
    Permissions::KICK_MEMBERS
}

fn moderate_members() -> Permissions {
    Permissions::MODERATE_MEMBERS
}

fn manage_nicknames() -> Permissions {
    Permissions::MANAGE_NICKNAMES
}

fn manage_roles() -> Permissions {
    Permissions::MANAGE_ROLES
}

/// What Discord's refusal means, to follow "Discord refused it:".
pub(crate) fn refusal(lang: Lang, error: &twilight_http::Error) -> String {
    let key = match error_code(error) {
        Some(50013) => "missing-permission",
        Some(10007) => "unknown-member",
        Some(10013) => "unknown-user",
        Some(10026) => "unknown-ban",
        Some(_) => "said-no",
        None => "unreachable",
    };
    t!(lang, &format!("mod.why.{key}"))
}

/// The audit log reason: who asked, and why. Discord keeps 512 characters.
pub(crate) fn audit_reason(lang: Lang, moderator: &Moderator, reason: Option<&str>) -> String {
    let reason = match reason {
        Some(reason) => reason.to_owned(),
        None => t!(lang, "mod.audit-no-reason"),
    };
    format!("{}: {reason}", moderator.name).chars().take(512).collect()
}

/// A public answer that pings nobody.
pub(crate) fn public(embed: Embed) -> InteractionResponseData {
    InteractionResponseData {
        embeds: Some(vec![embed]),
        allowed_mentions: Some(AllowedMentions::default()),
        ..Default::default()
    }
}

/// The same, privately.
pub(crate) fn private(embed: Embed) -> InteractionResponseData {
    InteractionResponseData {
        flags: Some(MessageFlags::EPHEMERAL),
        ..public(embed)
    }
}

/// A reason as typed, or the sentence that refuses it.
fn reason_of(text: Option<&str>) -> Result<Option<String>, String> {
    check_reason(text).map_err(|e| e.message)
}

#[derive(CommandModel, CreateCommand)]
#[command(
    name = "ban",
    desc = "Ban someone, for good or for a while",
    default_permissions = "ban_members",
    contexts = "guild"
)]
pub struct Ban {
    /// Who to ban: a member, or anyone by their user id
    user: ResolvedUser,
    /// Why (they are told, and the mod-log keeps it)
    #[command(max_length = 512)]
    reason: Option<String>,
    /// How long, like 12h or 7d (default: for good)
    #[command(max_length = 32)]
    duration: Option<String>,
    /// Delete their messages from the last 0 to 7 days
    #[command(min_value = 0, max_value = 7)]
    delete_days: Option<i64>,
}

impl Ban {
    pub async fn run(self, cx: &Cx) -> anyhow::Result<()> {
        let lang = cx.lang();
        let Some(guild) = permitted(cx, Permissions::BAN_MEMBERS, "ban-members").await? else {
            return Ok(());
        };
        let reason = match reason_of(self.reason.as_deref()) {
            Ok(reason) => reason,
            Err(why) => return cx.reply_ephemeral(why).await,
        };
        let duration = match self.duration.as_deref().map(str::trim).filter(|d| !d.is_empty()) {
            Some(text) => match parse_duration(text).map(check_temp_ban) {
                Some(Ok(d)) => Some(d),
                Some(Err(error)) => return cx.reply_ephemeral(error.message).await,
                None => return cx.reply_ephemeral(t!(lang, "mod.ban-duration-format")).await,
            },
            None => None,
        };
        let moderator = Moderator::of(cx)?;
        let Some(ranks) = Ranks::read(&cx.app, guild, &moderator).await else {
            return cx.reply_ephemeral(t!(lang, "mod.no-roles")).await;
        };
        if let Some(why) = ranks.refusal(lang, &self.user, "ban") {
            return cx.reply_ephemeral(why).await;
        }
        let mut act = Act::on(CaseKind::Ban, &self.user, reason);
        act.duration = duration;
        // Told first: once banned, the member shares no server with the bot to be written to from. A ban
        // Discord then refuses turns the message into one that says so.
        let told = match self.user.member {
            Some(_) => tell(&cx.app, guild, &act).await,
            None => None,
        };
        let audit = audit_reason(lang, &moderator, act.reason.as_deref());
        let _guard = cx.app.db.lock_ban(guild, act.target).await?;
        let days = self.delete_days.unwrap_or(0).clamp(0, 7) as u32;
        if let Err(error) = cx
            .app
            .http
            .create_ban(Id::new(guild), self.user.resolved.id)
            .delete_message_seconds(days * 86_400)
            .reason(&audit)
            .await
        {
            tracing::debug!(%error, guild, "ban refused");
            let mut text = t!(lang, "mod.refused-ban", why = refusal(lang, &error));
            if let Some(told) = &told {
                take_back(&cx.app, told, CaseKind::Ban).await;
                text = format!("{text}\n{}", t!(lang, "mod.told-anyway"));
            }
            return cx.reply_ephemeral(text).await;
        }
        let told = told.is_some();
        let opened = record(&cx.app, guild, (moderator.id, &moderator.name), &act, Some(told)).await?;
        cx.reply(public(reply_embed(&opened.case, &[], lang))).await
    }
}

#[derive(CommandModel, CreateCommand)]
#[command(
    name = "unban",
    desc = "Lift someone's ban",
    default_permissions = "ban_members",
    contexts = "guild"
)]
pub struct Unban {
    /// Who to unban (paste their user id)
    user: ResolvedUser,
    /// Why
    #[command(max_length = 512)]
    reason: Option<String>,
}

impl Unban {
    pub async fn run(self, cx: &Cx) -> anyhow::Result<()> {
        let lang = cx.lang();
        let Some(guild) = permitted(cx, Permissions::BAN_MEMBERS, "ban-members").await? else {
            return Ok(());
        };
        let reason = match reason_of(self.reason.as_deref()) {
            Ok(reason) => reason,
            Err(why) => return cx.reply_ephemeral(why).await,
        };
        let moderator = Moderator::of(cx)?;
        let act = Act::on(CaseKind::Unban, &self.user, reason);
        let _guard = cx.app.db.lock_ban(guild, act.target).await?;
        let audit = audit_reason(lang, &moderator, act.reason.as_deref());
        if let Err(error) = cx
            .app
            .http
            .delete_ban(Id::new(guild), self.user.resolved.id)
            .reason(&audit)
            .await
        {
            let text = t!(lang, "mod.refused-unban", why = refusal(lang, &error));
            return cx.reply_ephemeral(text).await;
        }
        let opened = record(&cx.app, guild, (moderator.id, &moderator.name), &act, None).await?;
        cx.reply(public(reply_embed(&opened.case, &[], lang))).await
    }
}

#[derive(CommandModel, CreateCommand)]
#[command(
    name = "kick",
    desc = "Remove a member (they can join again)",
    default_permissions = "kick_members",
    contexts = "guild"
)]
pub struct Kick {
    /// Who to kick
    user: ResolvedUser,
    /// Why (they are told, and the mod-log keeps it)
    #[command(max_length = 512)]
    reason: Option<String>,
}

impl Kick {
    pub async fn run(self, cx: &Cx) -> anyhow::Result<()> {
        let lang = cx.lang();
        let Some(guild) = permitted(cx, Permissions::KICK_MEMBERS, "kick-members").await? else {
            return Ok(());
        };
        let reason = match reason_of(self.reason.as_deref()) {
            Ok(reason) => reason,
            Err(why) => return cx.reply_ephemeral(why).await,
        };
        if self.user.member.is_none() {
            return cx.reply_ephemeral(t!(lang, "mod.not-a-member")).await;
        }
        let moderator = Moderator::of(cx)?;
        let Some(ranks) = Ranks::read(&cx.app, guild, &moderator).await else {
            return cx.reply_ephemeral(t!(lang, "mod.no-roles")).await;
        };
        if let Some(why) = ranks.refusal(lang, &self.user, "kick") {
            return cx.reply_ephemeral(why).await;
        }
        let act = Act::on(CaseKind::Kick, &self.user, reason);
        let told = tell(&cx.app, guild, &act).await;
        let audit = audit_reason(lang, &moderator, act.reason.as_deref());
        if let Err(error) = cx
            .app
            .http
            .remove_guild_member(Id::new(guild), self.user.resolved.id)
            .reason(&audit)
            .await
        {
            let mut text = t!(lang, "mod.refused-kick", why = refusal(lang, &error));
            if let Some(told) = &told {
                take_back(&cx.app, told, CaseKind::Kick).await;
                text = format!("{text}\n{}", t!(lang, "mod.told-anyway"));
            }
            return cx.reply_ephemeral(text).await;
        }
        let told = told.is_some();
        let opened = record(&cx.app, guild, (moderator.id, &moderator.name), &act, Some(told)).await?;
        cx.reply(public(reply_embed(&opened.case, &[], lang))).await
    }
}

/// Times a member out on Discord until `until` (`None` lifts it).
pub(crate) async fn set_timeout(
    cx: &Cx,
    guild: u64,
    user: &ResolvedUser,
    until: Option<chrono::DateTime<chrono::Utc>>,
    audit: &str,
) -> Result<(), String> {
    let lang = cx.lang();
    let until = match until.map(|at| Timestamp::from_secs(at.timestamp())) {
        Some(Ok(at)) => Some(at),
        Some(Err(_)) => return Err(t!(lang, "mod.why.bad-time")),
        None => None,
    };
    cx.app
        .http
        .update_guild_member(Id::new(guild), user.resolved.id)
        .communication_disabled_until(until)
        .reason(audit)
        .await
        .map(drop)
        .map_err(|error| refusal(lang, &error))
}

#[derive(CommandModel, CreateCommand)]
#[command(
    name = "timeout",
    desc = "Stop a member chatting and talking for a while",
    default_permissions = "moderate_members",
    contexts = "guild"
)]
pub struct Timeout {
    /// Who to time out
    user: ResolvedUser,
    /// How long, like 10m, 2h or 7d (up to 28 days)
    #[command(max_length = 32)]
    duration: String,
    /// Why (they are told, and the mod-log keeps it)
    #[command(max_length = 512)]
    reason: Option<String>,
}

impl Timeout {
    pub async fn run(self, cx: &Cx) -> anyhow::Result<()> {
        let lang = cx.lang();
        let Some(guild) = permitted(cx, Permissions::MODERATE_MEMBERS, "timeout-members").await? else {
            return Ok(());
        };
        let reason = match reason_of(self.reason.as_deref()) {
            Ok(reason) => reason,
            Err(why) => return cx.reply_ephemeral(why).await,
        };
        let duration = match check_timeout(parse_duration(&self.duration)) {
            Ok(d) => d,
            Err(error) => return cx.reply_ephemeral(error.message).await,
        };
        let Some(member) = &self.user.member else {
            return cx.reply_ephemeral(t!(lang, "mod.not-a-member")).await;
        };
        if member.permissions.contains(Permissions::ADMINISTRATOR) {
            return cx.reply_ephemeral(t!(lang, "mod.admin-no-timeout")).await;
        }
        let moderator = Moderator::of(cx)?;
        let Some(ranks) = Ranks::read(&cx.app, guild, &moderator).await else {
            return cx.reply_ephemeral(t!(lang, "mod.no-roles")).await;
        };
        if let Some(why) = ranks.refusal(lang, &self.user, "timeout") {
            return cx.reply_ephemeral(why).await;
        }
        let mut act = Act::on(CaseKind::Timeout, &self.user, reason);
        act.duration = Some(duration);
        let audit = audit_reason(lang, &moderator, act.reason.as_deref());
        if let Err(why) = set_timeout(cx, guild, &self.user, Some(cx.app.now() + duration), &audit).await {
            return cx.reply_ephemeral(t!(lang, "mod.refused-timeout", why = why)).await;
        }
        let opened = record(&cx.app, guild, (moderator.id, &moderator.name), &act, None).await?;
        cx.reply(public(reply_embed(&opened.case, &[], lang))).await
    }
}

#[derive(CommandModel, CreateCommand)]
#[command(
    name = "untimeout",
    desc = "End a member's timeout early",
    default_permissions = "moderate_members",
    contexts = "guild"
)]
pub struct Untimeout {
    /// Whose timeout to end
    user: ResolvedUser,
    /// Why
    #[command(max_length = 512)]
    reason: Option<String>,
}

impl Untimeout {
    pub async fn run(self, cx: &Cx) -> anyhow::Result<()> {
        let lang = cx.lang();
        let Some(guild) = permitted(cx, Permissions::MODERATE_MEMBERS, "timeout-members").await? else {
            return Ok(());
        };
        let reason = match reason_of(self.reason.as_deref()) {
            Ok(reason) => reason,
            Err(why) => return cx.reply_ephemeral(why).await,
        };
        let Some(member) = &self.user.member else {
            return cx.reply_ephemeral(t!(lang, "mod.not-a-member")).await;
        };
        let now = cx.app.now().timestamp();
        if member
            .communication_disabled_until
            .is_none_or(|until| until.as_secs() <= now)
        {
            return cx.reply_ephemeral(t!(lang, "mod.not-timed-out")).await;
        }
        let moderator = Moderator::of(cx)?;
        let Some(ranks) = Ranks::read(&cx.app, guild, &moderator).await else {
            return cx.reply_ephemeral(t!(lang, "mod.no-roles")).await;
        };
        if let Some(why) = ranks.refusal(lang, &self.user, "untimeout") {
            return cx.reply_ephemeral(why).await;
        }
        let act = Act::on(CaseKind::Untimeout, &self.user, reason);
        let audit = audit_reason(lang, &moderator, act.reason.as_deref());
        if let Err(why) = set_timeout(cx, guild, &self.user, None, &audit).await {
            return cx.reply_ephemeral(t!(lang, "mod.refused", why = why)).await;
        }
        let opened = record(&cx.app, guild, (moderator.id, &moderator.name), &act, None).await?;
        cx.reply(public(reply_embed(&opened.case, &[], lang))).await
    }
}

#[derive(CommandModel, CreateCommand)]
#[command(
    name = "nick",
    desc = "Change or reset a member's nickname",
    default_permissions = "manage_nicknames",
    contexts = "guild"
)]
pub struct Nick {
    /// Whose nickname
    user: ResolvedUser,
    /// The new nickname (leave it out to reset it)
    #[command(max_length = 32)]
    nickname: Option<String>,
    /// Why
    #[command(max_length = 512)]
    reason: Option<String>,
}

impl Nick {
    pub async fn run(self, cx: &Cx) -> anyhow::Result<()> {
        let lang = cx.lang();
        let Some(guild) = permitted(cx, Permissions::MANAGE_NICKNAMES, "manage-nicknames").await? else {
            return Ok(());
        };
        let reason = match reason_of(self.reason.as_deref()) {
            Ok(reason) => reason,
            Err(why) => return cx.reply_ephemeral(why).await,
        };
        if self.user.member.is_none() {
            return cx.reply_ephemeral(t!(lang, "mod.not-a-member")).await;
        }
        let nickname = self.nickname.as_deref().map(str::trim).filter(|n| !n.is_empty());
        let moderator = Moderator::of(cx)?;
        let Some(ranks) = Ranks::read(&cx.app, guild, &moderator).await else {
            return cx.reply_ephemeral(t!(lang, "mod.no-roles")).await;
        };
        // Your own nickname is yours to change; anyone else's follows the hierarchy.
        let why = if self.user.resolved.id.get() == moderator.id {
            ranks.bot_may(lang, &self.user, "rename")
        } else {
            ranks.refusal(lang, &self.user, "rename")
        };
        if let Some(why) = why {
            return cx.reply_ephemeral(why).await;
        }
        // The case's details are the server's record, in the server's language.
        let server = cx.app.guild_lang(guild).await;
        let mut act = Act::on(CaseKind::Nick, &self.user, reason);
        act.details = Some(match nickname {
            Some(nick) => {
                let nick = format!("`{}`", nick.replace('`', "'"));
                t!(server, "mod.details.nick-set", nick = nick)
            }
            None => t!(server, "mod.details.nick-reset"),
        });
        let audit = audit_reason(lang, &moderator, act.reason.as_deref());
        if let Err(error) = cx
            .app
            .http
            .update_guild_member(Id::new(guild), self.user.resolved.id)
            .nick(nickname)
            .reason(&audit)
            .await
        {
            return cx
                .reply_ephemeral(t!(lang, "mod.refused", why = refusal(lang, &error)))
                .await;
        }
        let opened = record(&cx.app, guild, (moderator.id, &moderator.name), &act, None).await?;
        cx.reply(private(reply_embed(&opened.case, &[], lang))).await
    }
}

#[derive(CommandModel, CreateCommand)]
#[command(
    name = "role",
    desc = "Give a member a role or take one away",
    default_permissions = "manage_roles",
    contexts = "guild"
)]
pub enum RoleCommand {
    #[command(name = "add")]
    Add(RoleAdd),
    #[command(name = "remove")]
    Remove(RoleRemove),
}

#[derive(CommandModel, CreateCommand)]
#[command(name = "add", desc = "Give a member a role")]
pub struct RoleAdd {
    /// Who gets it
    user: ResolvedUser,
    /// The role (below yours and mine)
    role: Role,
    /// Why
    #[command(max_length = 512)]
    reason: Option<String>,
}

#[derive(CommandModel, CreateCommand)]
#[command(name = "remove", desc = "Take a role from a member")]
pub struct RoleRemove {
    /// Who loses it
    user: ResolvedUser,
    /// The role (below yours and mine)
    role: Role,
    /// Why
    #[command(max_length = 512)]
    reason: Option<String>,
}

impl RoleCommand {
    pub async fn run(self, cx: &Cx) -> anyhow::Result<()> {
        let lang = cx.lang();
        let (user, role, reason, add) = match self {
            Self::Add(c) => (c.user, c.role, c.reason, true),
            Self::Remove(c) => (c.user, c.role, c.reason, false),
        };
        let Some(guild) = permitted(cx, Permissions::MANAGE_ROLES, "manage-roles").await? else {
            return Ok(());
        };
        let reason = match reason_of(reason.as_deref()) {
            Ok(reason) => reason,
            Err(why) => return cx.reply_ephemeral(why).await,
        };
        let Some(member) = &user.member else {
            return cx.reply_ephemeral(t!(lang, "mod.not-a-member")).await;
        };
        let label = format!("<@&{}>", role.id);
        let holds = member.roles.contains(&role.id);
        if add && holds {
            return cx.reply_ephemeral(t!(lang, "mod.already-has", role = label)).await;
        }
        if !add && !holds {
            return cx.reply_ephemeral(t!(lang, "mod.lacks", role = label)).await;
        }
        let moderator = Moderator::of(cx)?;
        let Some(ranks) = Ranks::read(&cx.app, guild, &moderator).await else {
            return cx.reply_ephemeral(t!(lang, "mod.no-roles")).await;
        };
        // The same rule as every role Pinchora hands out: one the bot can give, below the moderator's
        // highest role, and none with a dangerous permission unless they own or administer the server.
        let known = ranks.table.get(&role.id.get());
        let facts = RoleFacts {
            everyone: role.id.get() == guild,
            managed: role.managed,
            position: known.map_or(role.position, |r| r.position),
            permissions: known.map_or(role.permissions, |r| r.permissions).bits(),
        };
        let grantor = Grantor {
            owner: ranks.owner == moderator.id,
            administrator: administrator(cx),
            top_role: Some(ranks.actor.top),
        };
        if let Some(why) = role_refusal_for(facts, grantor, Some(ranks.bot.top), &label) {
            return cx.reply_ephemeral(why).await;
        }
        let kind = if add { CaseKind::RoleAdd } else { CaseKind::RoleRemove };
        let mut act = Act::on(kind, &user, reason);
        act.details = Some(format!("@{}", role.name.replace(['`', '*'], "")));
        let audit = audit_reason(lang, &moderator, act.reason.as_deref());
        let http = &cx.app.http;
        let done = if add {
            http.add_guild_member_role(Id::new(guild), user.resolved.id, role.id)
                .reason(&audit)
                .await
        } else {
            http.remove_guild_member_role(Id::new(guild), user.resolved.id, role.id)
                .reason(&audit)
                .await
        };
        if let Err(error) = done {
            return cx
                .reply_ephemeral(t!(lang, "mod.refused", why = refusal(lang, &error)))
                .await;
        }
        let opened = record(&cx.app, guild, (moderator.id, &moderator.name), &act, None).await?;
        cx.reply(private(reply_embed(&opened.case, &[], lang))).await
    }
}

/// How long a timeout given by the warn ladder lasts, when it says nothing (it always does).
pub(crate) const LADDER_TIMEOUT: Duration = Duration::hours(1);
