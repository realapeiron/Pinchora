//! `/case` and `/reason`: look a case up, and give it a reason after the fact.

use pinchora_core::moderation::check_reason;
use twilight_interactions::command::{CommandModel, CreateCommand};
use twilight_model::guild::Permissions;

use super::{Moderator, case_embed, members::private, permitted, refresh_post};
use crate::Cx;

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
        let Some(guild) = permitted(cx, STAFF, "Timeout Members").await? else {
            return Ok(());
        };
        let number = self.id.clamp(1, i64::from(i32::MAX)) as i32;
        match cx.app.db.case(guild, number).await? {
            Some(case) => cx.reply(private(case_embed(&case))).await,
            None => cx.reply_ephemeral(format!("There is no case #{number}.")).await,
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
        let Some(guild) = permitted(cx, STAFF, "Timeout Members").await? else {
            return Ok(());
        };
        let reason = match check_reason(Some(&self.reason)) {
            Ok(Some(reason)) => reason,
            Ok(None) => return cx.reply_ephemeral("Write the new reason.").await,
            Err(error) => return cx.reply_ephemeral(error.message).await,
        };
        let number = self.id.clamp(1, i64::from(i32::MAX)) as i32;
        let moderator = Moderator::of(cx)?;
        let updated = cx
            .app
            .db
            .set_case_reason(guild, number, Some(&reason), moderator.id, &moderator.name, "command")
            .await?;
        let Some(case) = updated else {
            return cx.reply_ephemeral(format!("There is no case #{number}.")).await;
        };
        refresh_post(&cx.app, &case).await;
        cx.reply(private(case_embed(&case))).await
    }
}
