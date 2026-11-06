use pinchora_core::economy::DAILY_COOLDOWN;
use pinchora_db::DailyOutcome;
use twilight_interactions::command::{CommandModel, CreateCommand};
use twilight_util::builder::embed::EmbedBuilder;

use crate::{
    Cx,
    commands::{customize, remind},
    util::{SUCCESS, WARN, relative, thousands},
};

#[derive(CommandModel, CreateCommand)]
#[command(name = "daily", desc = "Collect your daily shells")]
pub struct Daily;

impl Daily {
    pub async fn run(self, cx: &Cx) -> anyhow::Result<()> {
        let outcome = cx.app.db.claim_daily(cx.author()?.id.get()).await?;
        let claimed = matches!(outcome, DailyOutcome::Claimed { .. });
        if claimed {
            cx.app
                .quest(cx.author()?.id.get(), pinchora_game::quests::QuestKind::Daily)
                .await;
            let ready = chrono::Utc::now() + DAILY_COOLDOWN;
            remind::after_claim(&cx.app, cx.author()?.id.get(), "daily", ready).await;
        }
        let embed = match outcome {
            DailyOutcome::Claimed {
                amount,
                streak,
                balance,
            } => EmbedBuilder::new()
                .color(SUCCESS)
                .title(format!("+{} shells", thousands(amount)))
                .description(format!(
                    "Day **{streak}** of your streak. You now have **{}** shells.\nCome back tomorrow; the streak bonus grows for 20 days.",
                    thousands(balance)
                ))
                .build(),
            DailyOutcome::Cooldown { next_at } => EmbedBuilder::new()
                .color(WARN)
                .title("Already collected today")
                .description(format!("Your next daily is ready {}.", relative(next_at)))
                .build(),
        };
        cx.reply_embed(embed).await?;
        // The shells and the streak may have just earned a badge.
        if claimed {
            customize::announce(cx).await;
        }
        Ok(())
    }
}
