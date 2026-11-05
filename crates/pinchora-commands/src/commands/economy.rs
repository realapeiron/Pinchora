use chrono::Utc;
use pinchora_core::economy::{DAILY_COOLDOWN, DAILY_STREAK_WINDOW};
use pinchora_db::TransferOutcome;
use twilight_interactions::command::{CommandModel, CreateCommand, ResolvedUser};
use twilight_util::builder::embed::{EmbedBuilder, EmbedFieldBuilder};

use crate::{
    Cx,
    util::{BRAND, SUCCESS, mention, relative, resolved_display_name, thousands},
};

#[derive(CommandModel, CreateCommand)]
#[command(name = "balance", desc = "Show a shell wallet")]
pub struct Balance {
    /// Whose wallet to show (default: you)
    user: Option<ResolvedUser>,
}

impl Balance {
    pub async fn run(self, cx: &Cx) -> anyhow::Result<()> {
        let (user, name) = match &self.user {
            Some(r) => (r.resolved.clone(), resolved_display_name(r)),
            None => (cx.author()?.clone(), cx.author_display_name()),
        };
        let row = cx.app.db.user(user.id.get()).await?;
        let now = Utc::now();
        let streak = match row.last_daily_at {
            Some(t) if now - t <= DAILY_STREAK_WINDOW => row.daily_streak,
            _ => 0,
        };
        let next_daily = match row.last_daily_at {
            Some(t) if now - t < DAILY_COOLDOWN => relative(t + DAILY_COOLDOWN),
            _ => "ready now — `/daily`".to_owned(),
        };
        let embed = EmbedBuilder::new()
            .color(BRAND)
            .title(format!("{name}'s wallet"))
            .field(EmbedFieldBuilder::new("Shells", thousands(row.credits)).inline())
            .field(EmbedFieldBuilder::new("Daily streak", format!("{streak} days")).inline())
            .field(EmbedFieldBuilder::new("Next daily", next_daily).inline())
            .build();
        cx.reply_embed(embed).await
    }
}

#[derive(CommandModel, CreateCommand)]
#[command(name = "give", desc = "Send shells to someone")]
pub struct Give {
    /// Who gets the shells
    user: ResolvedUser,
    /// How many shells
    #[command(min_value = 1, max_value = 1_000_000_000)]
    amount: i64,
}

impl Give {
    pub async fn run(self, cx: &Cx) -> anyhow::Result<()> {
        let from = cx.author()?.id;
        let to = self.user.resolved.id;
        if to == from {
            return cx.reply_ephemeral("You can't send shells to yourself.").await;
        }
        if self.user.resolved.bot {
            return cx.reply_ephemeral("Bots have no use for shells.").await;
        }
        match cx.app.db.transfer_credits(from.get(), to.get(), self.amount).await? {
            TransferOutcome::Done { sender_balance } => {
                let embed = EmbedBuilder::new()
                    .color(SUCCESS)
                    .description(format!(
                        "Sent **{}** shells to {}. You have **{}** left.",
                        thousands(self.amount),
                        mention(to),
                        thousands(sender_balance)
                    ))
                    .build();
                cx.reply_embed(embed).await
            }
            TransferOutcome::Insufficient { balance } => {
                cx.reply_ephemeral(format!("You only have {} shells.", thousands(balance)))
                    .await
            }
        }
    }
}
