//! `/case` and `/reason`: look a case up, and give it a reason after the fact.

use pinchora_core::moderation::moderation::CaseKind;
use pinchora_core::moderation::moderation::check_reason;
use pinchora_core::moderation::moderation::outranks;
use pinchora_core::t;
use twilight_interactions::command::{CommandModel, CreateCommand};
use twilight_model::guild::Permissions;

use crate::Cx;
use crate::commands::moderation::Moderator;
use crate::commands::moderation::Ranks;
use crate::commands::moderation::case_embed;
use crate::commands::moderation::members::private;
use crate::commands::moderation::permitted;
use crate::commands::moderation::refresh_post;
use crate::infra::util::mention;

/// Anyone who can do one of the moderation commands may read and annotate the cases.
const STAFF: Permissions = Permissions::BAN_MEMBERS
    .union(Permissions::KICK_MEMBERS)
    .union(Permissions::MODERATE_MEMBERS)
    .union(Permissions::MANAGE_MESSAGES)
    .union(Permissions::MANAGE_GUILD);

fn moderate_members() -> Permissions {
    Permissions::MODERATE_MEMBERS
}

#[derive(CommandModel, CreateCommand)]
#[command(
    name = "case",
    desc = "Look up a moderation case",
    default_permissions = "moderate_members",
    contexts = "guild"
)]
pub struct CaseCommand {
    /// The case number
    #[command(min_value = 1, max_value = 2_000_000_000)]
    id: i64,
}

impl CaseCommand {
    pub async fn run(self, cx: &Cx) -> anyhow::Result<()> {
        let Some(guild) = permitted(cx, STAFF, "timeout-members").await? else {
            return Ok(());
        };
        let number = self.id.clamp(1, i64::from(i32::MAX)) as i32;
        match cx.app.db.case(guild, number).await? {
            Some(case) => cx.reply(private(case_embed(&case, cx.lang()))).await,
            None => cx.reply_ephemeral(t!(cx.lang(), "mod.no-case", number = number)).await,
        }
    }
}

#[derive(CommandModel, CreateCommand)]
#[command(
    name = "reason",
    desc = "Give a moderation case a new reason",
    default_permissions = "moderate_members",
    contexts = "guild"
)]
pub struct ReasonCommand {
    /// The case number
    #[command(min_value = 1, max_value = 2_000_000_000)]
    id: i64,
    /// The reason
    #[command(min_length = 1, max_length = 512)]
    reason: String,
}

impl ReasonCommand {
    pub async fn run(self, cx: &Cx) -> anyhow::Result<()> {
        let lang = cx.lang();
        let Some(guild) = permitted(cx, STAFF, "timeout-members").await? else {
            return Ok(());
        };
        let reason = match check_reason(Some(&self.reason)) {
            Ok(Some(reason)) => reason,
            Ok(None) => return cx.reply_ephemeral(t!(lang, "mod.new-reason")).await,
            Err(error) => return cx.reply_ephemeral(error.text(lang)).await,
        };
        let number = self.id.clamp(1, i64::from(i32::MAX)) as i32;
        let moderator = Moderator::of(cx)?;
        let Some(case) = cx.app.db.case(guild, number).await? else {
            return cx.reply_ephemeral(t!(lang, "mod.no-case", number = number)).await;
        };
        // A case's reason is its moderator's word: theirs to change, or someone's above them. A case the
        // bot opened (the warn ladder, a temporary ban that ended) is open to whoever is above its member.
        let opened_by = case.moderator_id as u64;
        if opened_by != moderator.id {
            let Some(ranks) = Ranks::read(&cx.app, guild, &moderator).await else {
                return cx.reply_ephemeral(t!(lang, "mod.no-roles")).await;
            };
            let by_bot = opened_by == ranks.bot.id || opened_by == 0;
            let on_member = !case.kind().is_some_and(CaseKind::on_channel);
            let (other, refusal) = if by_bot {
                (on_member.then_some(case.target_id as u64), "mod.reason-target")
            } else {
                (Some(opened_by), "mod.reason-moderator")
            };
            if let Some(other) = other {
                let Some(rank) = ranks.of(&cx.app, guild, other).await else {
                    return cx.reply_ephemeral(t!(lang, "mod.no-roles")).await;
                };
                if !outranks(ranks.owner, ranks.actor, rank) {
                    let text = t!(lang, refusal, number = number, moderator = mention(opened_by));
                    return cx.reply_ephemeral(text).await;
                }
            }
        }
        let updated = cx
            .app
            .db
            .set_case_reason(guild, number, Some(&reason), moderator.id, &moderator.name, "command")
            .await?;
        let Some(case) = updated else {
            return cx.reply_ephemeral(t!(lang, "mod.no-case", number = number)).await;
        };
        refresh_post(&cx.app, &case).await;
        cx.reply(private(case_embed(&case, lang))).await
    }
}
