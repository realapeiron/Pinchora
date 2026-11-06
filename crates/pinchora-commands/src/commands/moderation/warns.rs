//! `/warn`, `/warnings` and `/unwarn`, and the warn ladder: a number of warns that brings a timeout,
//! a kick or a ban, given by the bot itself.

use pinchora_core::moderation::{CaseKind, Punishment, Threshold, check_reason, format_duration, reached};
use pinchora_db::NewCase;
use twilight_http::request::AuditLogReason;
use twilight_interactions::command::{CommandModel, CreateCommand, ResolvedUser};
use twilight_model::{guild::Permissions, id::Id};
use twilight_util::builder::embed::{EmbedBuilder, EmbedFooterBuilder};

use super::{
    Act, BOT_NAME, Moderator, NO_ROLES, Ranks, color,
    members::{private, public, refusal, set_timeout},
    permitted, record, reply_embed, tell,
};
use crate::{
    Cx,
    util::{WARN, relative, resolved_display_name},
};

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
        let Some(guild) = permitted(cx, Permissions::MODERATE_MEMBERS, "Timeout Members").await? else {
            return Ok(());
        };
        let reason = match check_reason(Some(&self.reason)) {
            Ok(Some(reason)) => reason,
            Ok(None) => return cx.reply_ephemeral("Say why you are warning them.").await,
            Err(error) => return cx.reply_ephemeral(error.message).await,
        };
        if self.user.member.is_none() {
            return cx.reply_ephemeral("That user isn't a member of this server.").await;
        }
        let moderator = Moderator::of(cx)?;
        let Some(ranks) = Ranks::read(&cx.app, guild, &moderator).await else {
            return cx.reply_ephemeral(NO_ROLES).await;
        };
        if let Some(why) = ranks.refusal(&self.user, "warn") {
            return cx.reply_ephemeral(why).await;
        }
        let act = Act::on(CaseKind::Warn, &self.user, Some(reason));
        let opened = record(&cx.app, guild, (moderator.id, &moderator.name), &act, None).await?;
        let warns = opened.active_warns;
        let mut lines = vec![format!(
            "They have **{warns}** active warn{}.",
            if warns == 1 { "" } else { "s" }
        )];
        let ladder = cx.app.db.warn_thresholds(guild).await?;
        if let Some(rung) = reached(&ladder, warns) {
            lines.push(climb(cx, guild, &ranks, &self.user, rung, warns).await?);
        }
        cx.reply(public(reply_embed(&opened.case, &lines))).await
    }
}

/// Applies the rung a warn reached, with the bot as the moderator. Returns the line for the answer.
async fn climb(
    cx: &Cx,
    guild: u64,
    ranks: &Ranks,
    user: &ResolvedUser,
    rung: Threshold,
    warns: i64,
) -> anyhow::Result<String> {
    let (kind, verb) = match rung.action {
        Punishment::Timeout => (CaseKind::Timeout, "time out"),
        Punishment::Kick => (CaseKind::Kick, "kick"),
        Punishment::Ban => (CaseKind::Ban, "ban"),
    };
    let reached_text = format!("{warns} warns reached");
    if let Some(why) = ranks.bot_may(user, verb) {
        return Ok(format!("{reached_text}, but {}", lowercase_first(&why)));
    }
    if kind == CaseKind::Timeout
        && user
            .member
            .as_ref()
            .is_some_and(|m| m.permissions.contains(Permissions::ADMINISTRATOR))
    {
        return Ok(format!("{reached_text}, but administrators can't be timed out."));
    }
    let mut act = Act::on(kind, user, Some(format!("Automatic: {}", rung.describe())));
    act.duration = rung.duration;
    let audit = format!("{BOT_NAME}: {}", rung.describe());
    let told = if kind.tells_first() {
        Some(tell(&cx.app, guild, &act).await)
    } else {
        None
    };
    let http = &cx.app.http;
    let done = match rung.action {
        Punishment::Timeout => {
            let until = cx.app.now() + rung.duration.unwrap_or(super::members::LADDER_TIMEOUT);
            set_timeout(cx, guild, user, Some(until), &audit).await
        }
        Punishment::Kick => http
            .remove_guild_member(Id::new(guild), user.resolved.id)
            .reason(&audit)
            .await
            .map(drop)
            .map_err(|e| refusal(&e).to_owned()),
        Punishment::Ban => http
            .create_ban(Id::new(guild), user.resolved.id)
            .reason(&audit)
            .await
            .map(drop)
            .map_err(|e| refusal(&e).to_owned()),
    };
    if let Err(why) = done {
        return Ok(format!("{reached_text}, but Discord refused to {verb} them: {why}."));
    }
    let opened = record(&cx.app, guild, (cx.app.application_id.get(), BOT_NAME), &act, told).await?;
    let what = match (rung.action, rung.duration) {
        (Punishment::Timeout, Some(d)) => format!("timed out for {}", format_duration(d)),
        (Punishment::Ban, Some(d)) => format!("banned for {}", format_duration(d)),
        (Punishment::Ban, None) => "banned".to_owned(),
        _ => "kicked".to_owned(),
    };
    Ok(format!("{reached_text}: {what} · case #{}", opened.case.number))
}

fn lowercase_first(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => first.to_lowercase().chain(chars).collect(),
        None => String::new(),
    }
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
        let Some(guild) = permitted(cx, Permissions::MODERATE_MEMBERS, "Timeout Members").await? else {
            return Ok(());
        };
        let warns = cx.app.db.active_warns(guild, self.user.resolved.id.get()).await?;
        let name = resolved_display_name(&self.user);
        let text = if warns.is_empty() {
            "No warns that count.".to_owned()
        } else {
            warns
                .iter()
                .take(20)
                .map(|w| {
                    format!(
                        "**#{}** · {} · by {}\n> {}",
                        w.number,
                        relative(w.created_at),
                        w.moderator_name,
                        w.reason.as_deref().unwrap_or("No reason given").replace('\n', " ")
                    )
                })
                .collect::<Vec<_>>()
                .join("\n")
        };
        let ladder = cx.app.db.warn_thresholds(guild).await?;
        let mut embed = EmbedBuilder::new()
            .color(WARN)
            .title(format!("Warns of {} · {}", name.replace('*', ""), warns.len()))
            .description(text);
        if !ladder.is_empty() {
            let rungs: Vec<String> = ladder.iter().map(Threshold::describe).collect();
            embed = embed.footer(EmbedFooterBuilder::new(format!("Ladder: {}", rungs.join(" · "))));
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
        let Some(guild) = permitted(cx, Permissions::MODERATE_MEMBERS, "Timeout Members").await? else {
            return Ok(());
        };
        let reason = match check_reason(self.reason.as_deref()) {
            Ok(reason) => reason,
            Err(error) => return cx.reply_ephemeral(error.message).await,
        };
        let moderator = Moderator::of(cx)?;
        let number = self.case.clamp(1, i64::from(i32::MAX)) as i32;
        // A moderator's own warns are for someone else to take back.
        if cx
            .app
            .db
            .case(guild, number)
            .await?
            .is_some_and(|c| c.target_id as u64 == moderator.id)
        {
            return cx.reply_ephemeral("You can't take back a warn of your own.").await;
        }
        let details = format!("case #{number}");
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
            return cx
                .reply_ephemeral(format!("Case #{number} is not a warn that still counts."))
                .await;
        };
        super::post_case(&cx.app, guild, &opened.case).await;
        let warns = opened.active_warns;
        let line = format!(
            "They have **{warns}** active warn{} now.",
            if warns == 1 { "" } else { "s" }
        );
        let mut embed = reply_embed(&opened.case, &[line]);
        embed.color = Some(color(Some(CaseKind::Unwarn)));
        cx.reply(public(embed)).await
    }
}
