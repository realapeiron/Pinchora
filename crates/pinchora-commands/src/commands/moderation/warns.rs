//! `/warn`, `/warnings` and `/unwarn`, and the warn ladder: a number of warns that brings a timeout,
//! a kick or a ban, given by the bot itself.

use pinchora_core::moderation::moderation::CaseKind;
use pinchora_core::moderation::moderation::Punishment;
use pinchora_core::moderation::moderation::Threshold;
use pinchora_core::moderation::moderation::check_reason;
use pinchora_core::moderation::moderation::outranks;
use pinchora_core::moderation::moderation::reached;
use pinchora_core::t;
use pinchora_db::NewCase;
use twilight_http::request::AuditLogReason;
use twilight_interactions::command::{CommandModel, CreateCommand, ResolvedUser};
use twilight_model::{guild::Permissions, id::Id};
use twilight_util::builder::embed::{EmbedBuilder, EmbedFooterBuilder};

use crate::Cx;
use crate::commands::moderation::Act;
use crate::commands::moderation::BOT_NAME;
use crate::commands::moderation::Moderator;
use crate::commands::moderation::Ranks;
use crate::commands::moderation::color;
use crate::commands::moderation::members::private;
use crate::commands::moderation::members::public;
use crate::commands::moderation::members::refusal;
use crate::commands::moderation::members::set_timeout;
use crate::commands::moderation::permitted;
use crate::commands::moderation::record;
use crate::commands::moderation::reply_embed;
use crate::commands::moderation::take_back;
use crate::commands::moderation::tell;
use crate::commands::moderation::words;
use crate::infra::util::WARN;
use crate::infra::util::mention;
use crate::infra::util::relative;
use crate::infra::util::resolved_display_name;

fn moderate_members() -> Permissions {
    Permissions::MODERATE_MEMBERS
}

#[derive(CommandModel, CreateCommand)]
#[command(
    name = "warn",
    desc = "Warn a member; enough warns act on their own",
    default_permissions = "moderate_members",
    contexts = "guild"
)]
pub struct Warn {
    /// Who to warn
    user: ResolvedUser,
    /// Why (they are told, and the mod-log keeps it)
    #[command(min_length = 1, max_length = 512)]
    reason: String,
}

impl Warn {
    pub async fn run(self, cx: &Cx) -> anyhow::Result<()> {
        let lang = cx.lang();
        let Some(guild) = permitted(cx, Permissions::MODERATE_MEMBERS, "timeout-members").await? else {
            return Ok(());
        };
        let reason = match check_reason(Some(&self.reason)) {
            Ok(Some(reason)) => reason,
            Ok(None) => return cx.reply_ephemeral(t!(lang, "mod.warn-why")).await,
            Err(error) => return cx.reply_ephemeral(error.text(lang)).await,
        };
        if self.user.member.is_none() {
            return cx.reply_ephemeral(t!(lang, "mod.not-a-member")).await;
        }
        let moderator = Moderator::of(cx)?;
        let Some(ranks) = Ranks::read(&cx.app, guild, &moderator).await else {
            return cx.reply_ephemeral(t!(lang, "mod.no-roles")).await;
        };
        if let Some(why) = ranks.refusal(lang, &self.user, "warn") {
            return cx.reply_ephemeral(why).await;
        }
        let act = Act::on(CaseKind::Warn, &self.user, Some(reason));
        let opened = record(&cx.app, guild, (moderator.id, &moderator.name), &act, None).await?;
        let warns = opened.active_warns;
        let mut lines = vec![t!(lang, "mod.active-warns", count = warns)];
        let ladder = cx.app.db.warn_thresholds(guild).await?;
        if let Some(rung) = reached(&ladder, warns) {
            lines.push(climb(cx, guild, &ranks, &self.user, rung, warns).await?);
        }
        cx.reply(public(reply_embed(&opened.case, &lines, lang))).await
    }
}

/// Applies the rung a warn reached, with the bot as the moderator. Returns the line for the answer.
pub(crate) async fn climb(
    cx: &Cx,
    guild: u64,
    ranks: &Ranks,
    user: &ResolvedUser,
    rung: Threshold,
    warns: i64,
) -> anyhow::Result<String> {
    let lang = cx.lang();
    let (kind, verb) = match rung.action {
        Punishment::Timeout => (CaseKind::Timeout, "timeout"),
        Punishment::Kick => (CaseKind::Kick, "kick"),
        Punishment::Ban => (CaseKind::Ban, "ban"),
    };
    if let Some(why) = crate::moderation::anti_nuke::vip_refusal(&cx.app, guild, user.resolved.id.get()).await? {
        return Ok(t!(lang, "mod.ladder-stopped", count = warns, why = why));
    }
    if let Some(why) = ranks.bot_may(lang, user, verb) {
        return Ok(t!(lang, "mod.ladder-stopped", count = warns, why = why));
    }
    if kind == CaseKind::Timeout
        && user
            .member
            .as_ref()
            .is_some_and(|m| m.permissions.contains(Permissions::ADMINISTRATOR))
    {
        return Ok(t!(lang, "mod.ladder-admin", count = warns));
    }
    // The case and the audit log are the server's record, in the server's language.
    let server = cx.app.guild_lang(guild).await;
    let described = words::rung(server, &rung);
    let mut act = Act::on(kind, user, Some(t!(server, "mod.automatic", rung = described)));
    act.duration = rung.duration;
    let audit = format!("{BOT_NAME}: {described}");
    let told = if kind.tells_first() {
        tell(&cx.app, guild, &act).await
    } else {
        None
    };
    let _guard = if kind == CaseKind::Ban {
        Some(cx.app.db.lock_ban(guild, act.target).await?)
    } else {
        None
    };
    let http = &cx.app.http;
    let done = match rung.action {
        Punishment::Timeout => {
            let until = cx.app.now()
                + rung
                    .duration
                    .unwrap_or(crate::commands::moderation::members::LADDER_TIMEOUT);
            set_timeout(cx, guild, user, Some(until), &audit).await
        }
        Punishment::Kick => http
            .remove_guild_member(Id::new(guild), user.resolved.id)
            .reason(&audit)
            .await
            .map(drop)
            .map_err(|e| refusal(lang, &e)),
        Punishment::Ban => http
            .create_ban(Id::new(guild), user.resolved.id)
            .reason(&audit)
            .await
            .map(drop)
            .map_err(|e| refusal(lang, &e)),
    };
    if let Err(why) = done {
        if let Some(told) = &told {
            take_back(&cx.app, told, kind).await;
        }
        return Ok(t!(lang, "mod.ladder-refused", count = warns, why = why));
    }
    let told = kind.tells_first().then_some(told.is_some());
    let opened = record(&cx.app, guild, (cx.app.application_id.get(), BOT_NAME), &act, told).await?;
    let what = match (rung.action, rung.duration) {
        (Punishment::Timeout, Some(d)) => t!(lang, "mod.ladder-timed-out", duration = words::duration(lang, d)),
        (Punishment::Ban, Some(d)) => t!(lang, "mod.ladder-banned-for", duration = words::duration(lang, d)),
        (Punishment::Ban, None) => t!(lang, "mod.ladder-banned"),
        _ => t!(lang, "mod.ladder-kicked"),
    };
    Ok(t!(
        lang,
        "mod.ladder-done",
        count = warns,
        what = what,
        number = opened.case.number
    ))
}

#[derive(CommandModel, CreateCommand)]
#[command(
    name = "warnings",
    desc = "A member's warns that still count",
    default_permissions = "moderate_members",
    contexts = "guild"
)]
pub struct Warnings {
    /// Whose warns
    user: ResolvedUser,
}

impl Warnings {
    pub async fn run(self, cx: &Cx) -> anyhow::Result<()> {
        let lang = cx.lang();
        let Some(guild) = permitted(cx, Permissions::MODERATE_MEMBERS, "timeout-members").await? else {
            return Ok(());
        };
        let warns = cx.app.db.active_warns(guild, self.user.resolved.id.get()).await?;
        let name = resolved_display_name(&self.user);
        let text = if warns.is_empty() {
            t!(lang, "mod.no-warns")
        } else {
            warns
                .iter()
                .take(20)
                .map(|w| {
                    let reason = match &w.reason {
                        Some(reason) => reason.replace('\n', " "),
                        None => t!(lang, "mod.no-reason"),
                    };
                    t!(
                        lang,
                        "mod.warn-line",
                        number = w.number,
                        when = relative(w.created_at),
                        moderator = w.moderator_name,
                        reason = reason
                    )
                })
                .collect::<Vec<_>>()
                .join("\n")
        };
        let ladder = cx.app.db.warn_thresholds(guild).await?;
        let title = t!(
            lang,
            "mod.warns-title",
            name = name.replace('*', ""),
            count = warns.len()
        );
        let mut embed = EmbedBuilder::new().color(WARN).title(title).description(text);
        if !ladder.is_empty() {
            let rungs: Vec<String> = ladder.iter().map(|r| words::rung(lang, r)).collect();
            let footer = t!(lang, "mod.ladder", rungs = rungs.join(" · "));
            embed = embed.footer(EmbedFooterBuilder::new(footer));
        }
        cx.reply(private(embed.build())).await
    }
}

#[derive(CommandModel, CreateCommand)]
#[command(
    name = "unwarn",
    desc = "Take a warn back, so it no longer counts",
    default_permissions = "moderate_members",
    contexts = "guild"
)]
pub struct Unwarn {
    /// The warn's case number
    #[command(min_value = 1, max_value = 2_000_000_000)]
    case: i64,
    /// Why
    #[command(max_length = 512)]
    reason: Option<String>,
}

impl Unwarn {
    pub async fn run(self, cx: &Cx) -> anyhow::Result<()> {
        let lang = cx.lang();
        let Some(guild) = permitted(cx, Permissions::MODERATE_MEMBERS, "timeout-members").await? else {
            return Ok(());
        };
        let reason = match check_reason(self.reason.as_deref()) {
            Ok(reason) => reason,
            Err(error) => return cx.reply_ephemeral(error.text(lang)).await,
        };
        let moderator = Moderator::of(cx)?;
        let number = self.case.clamp(1, i64::from(i32::MAX)) as i32;
        let warn = cx.app.db.case(guild, number).await?;
        let Some(warn) = warn.filter(|c| c.kind() == Some(CaseKind::Warn) && c.active) else {
            return cx.reply_ephemeral(t!(lang, "mod.not-a-warn", number = number)).await;
        };
        // A moderator's own warns are for someone else to take back.
        let target = warn.target_id as u64;
        if target == moderator.id {
            return cx.reply_ephemeral(t!(lang, "mod.own-warn")).await;
        }
        // Like the warn itself: only over a member below you, and a warn given by someone below you.
        let Some(ranks) = Ranks::read(&cx.app, guild, &moderator).await else {
            return cx.reply_ephemeral(t!(lang, "mod.no-roles")).await;
        };
        let Some(member) = ranks.of(&cx.app, guild, target).await else {
            return cx.reply_ephemeral(t!(lang, "mod.no-roles")).await;
        };
        if !outranks(ranks.owner, ranks.actor, member) {
            return cx.reply_ephemeral(t!(lang, "mod.unwarn-above")).await;
        }
        let given_by = warn.moderator_id as u64;
        if given_by != moderator.id && given_by != ranks.bot.id && given_by != 0 {
            let Some(giver) = ranks.of(&cx.app, guild, given_by).await else {
                return cx.reply_ephemeral(t!(lang, "mod.no-roles")).await;
            };
            if !outranks(ranks.owner, ranks.actor, giver) {
                let text = t!(lang, "mod.unwarn-moderator", moderator = mention(given_by));
                return cx.reply_ephemeral(text).await;
            }
        }
        let server = cx.app.guild_lang(guild).await;
        let details = t!(server, "mod.details.unwarn", number = number);
        let removed = cx
            .app
            .db
            .remove_warn(
                number,
                &NewCase {
                    guild,
                    kind: CaseKind::Unwarn,
                    target: 0,
                    target_name: "",
                    moderator: moderator.id,
                    moderator_name: &moderator.name,
                    reason: reason.as_deref(),
                    duration: None,
                    expires_at: None,
                    details: Some(&details),
                },
            )
            .await?;
        let Some((_, opened)) = removed else {
            return cx.reply_ephemeral(t!(lang, "mod.not-a-warn", number = number)).await;
        };
        crate::commands::moderation::post_case(&cx.app, guild, &opened.case).await;
        let line = t!(lang, "mod.active-warns-now", count = opened.active_warns);
        let mut embed = reply_embed(&opened.case, &[line], lang);
        embed.color = Some(color(Some(CaseKind::Unwarn)));
        cx.reply(public(embed)).await
    }
}
