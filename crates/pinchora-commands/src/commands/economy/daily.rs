use pinchora_core::economy::legacy::DAILY_COOLDOWN;
use pinchora_core::t;
use pinchora_db::DailyOutcome;
use twilight_interactions::command::{CommandModel, CreateCommand};
use twilight_util::builder::embed::EmbedBuilder;

use crate::Cx;
use crate::commands::community::remind;
use crate::commands::social::customize;
use crate::infra::util::SUCCESS;
use crate::infra::util::WARN;
use crate::infra::util::relative;
use crate::infra::util::thousands_in;

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
            remind::after_claim(&cx.app, cx.author()?.id.get(), "daily", ready, cx.lang()).await;
        }
        let lang = cx.lang();
        let embed = match outcome {
            DailyOutcome::Claimed {
                amount,
                streak,
                balance,
            } => EmbedBuilder::new()
                .color(SUCCESS)
                .title(t!(lang, "daily.claimed-title", amount = thousands_in(lang, amount)))
                .description(t!(
                    lang,
                    "daily.claimed",
                    streak = streak,
                    balance = thousands_in(lang, balance)
                ))
                .build(),
            DailyOutcome::Cooldown { next_at } => EmbedBuilder::new()
                .color(WARN)
                .title(t!(lang, "daily.cooldown-title"))
                .description(t!(lang, "daily.cooldown", when = relative(next_at)))
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
