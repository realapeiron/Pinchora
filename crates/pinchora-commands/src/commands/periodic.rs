//! `/weekly` and `/monthly`: shell rewards with a cooldown of their own, like `/daily`.

use chrono::{DateTime, Utc};
use pinchora_core::economy::Period;
use pinchora_db::PeriodOutcome;
use twilight_interactions::command::{CommandModel, CreateCommand};
use twilight_util::builder::embed::EmbedBuilder;

use crate::{
    Cx,
    commands::{customize, remind},
    util::{SUCCESS, WARN, relative, thousands},
};

#[derive(CommandModel, CreateCommand)]
#[command(name = "weekly", desc = "Collect your weekly shells (every 7 days)")]
pub struct Weekly;

#[derive(CommandModel, CreateCommand)]
#[command(name = "monthly", desc = "Collect your monthly shells (every 30 days)")]
pub struct Monthly;

impl Weekly {
    pub async fn run(self, cx: &Cx) -> anyhow::Result<()> {
        claim(cx, Period::Weekly).await
    }
}

impl Monthly {
    pub async fn run(self, cx: &Cx) -> anyhow::Result<()> {
        claim(cx, Period::Monthly).await
    }
}

/// "week" or "month": what the reward comes around once in.
fn span(period: Period) -> &'static str {
    match period {
        Period::Weekly => "week",
        Period::Monthly => "month",
    }
}

async fn claim(cx: &Cx, period: Period) -> anyhow::Result<()> {
    let user = cx.author()?.id.get();
    let outcome = cx.app.db.claim_period(user, period, cx.app.now()).await?;
    let claimed = matches!(outcome, PeriodOutcome::Claimed { .. });
    let embed = match outcome {
        PeriodOutcome::Claimed {
            amount,
            balance,
            next_at,
        } => {
            remind::after_claim(&cx.app, user, period.key(), next_at).await;
            EmbedBuilder::new()
                .color(SUCCESS)
                .title(format!("+{} shells", thousands(amount)))
                .description(format!(
                    "Your {} reward. You now have **{}** shells.\nThe next one is ready {}.",
                    period.key(),
                    thousands(balance),
                    relative(next_at)
                ))
                .build()
        }
        PeriodOutcome::Cooldown { next_at } => EmbedBuilder::new()
            .color(WARN)
            .title(format!("Already collected this {}", span(period)))
            .description(format!(
                "Your next {} reward is ready {}.",
                period.key(),
                relative(next_at)
            ))
            .build(),
    };
    cx.reply_embed(embed).await?;
    // The shells may have just earned a badge.
    if claimed {
        customize::announce(cx).await;
    }
    Ok(())
}

/// How long until a reward is ready again, as the wallet card writes it: `ready`, `5m`, `3h 20m`,
/// `12d 4h`, and `24h` or `7d` when nothing is left over.
pub(crate) fn ready_in(claimed_at: Option<DateTime<Utc>>, cooldown: chrono::Duration, now: DateTime<Utc>) -> String {
    let left = match claimed_at {
        Some(t) if t + cooldown > now => t + cooldown - now,
        _ => return "ready".to_owned(),
    };
    let minutes = (left.num_seconds() + 59) / 60;
    match minutes {
        // A day's worth reads better in hours; the days start once there are two of them.
        m if m >= 48 * 60 => match m % (24 * 60) / 60 {
            0 => format!("{}d", m / (24 * 60)),
            h => format!("{}d {h}h", m / (24 * 60)),
        },
        m if m >= 60 => match m % 60 {
            0 => format!("{}h", m / 60),
            rest => format!("{}h {rest}m", m / 60),
        },
        m => format!("{}m", m.max(1)),
    }
}

#[cfg(test)]
mod tests {
    use chrono::Duration;

    use super::*;

    #[test]
    fn timers_read_naturally() {
        let now = Utc::now();
        let week = Duration::days(7);
        assert_eq!(ready_in(None, week, now), "ready");
        assert_eq!(ready_in(Some(now - week), week, now), "ready");
        assert_eq!(ready_in(Some(now - week - Duration::hours(3)), week, now), "ready");
        assert_eq!(ready_in(Some(now - week + Duration::seconds(20)), week, now), "1m");
        assert_eq!(ready_in(Some(now - week + Duration::minutes(5)), week, now), "5m");
        assert_eq!(
            ready_in(Some(now - week + Duration::hours(3) + Duration::minutes(20)), week, now),
            "3h 20m"
        );
        assert_eq!(
            ready_in(
                Some(now - Duration::days(30) + Duration::days(12) + Duration::hours(4)),
                Duration::days(30),
                now
            ),
            "12d 4h"
        );
        assert_eq!(ready_in(Some(now), Duration::days(30), now), "30d");
        assert_eq!(ready_in(Some(now), Duration::days(1), now), "24h");
        assert_eq!(ready_in(Some(now - week + Duration::hours(47)), week, now), "47h");
        assert_eq!(ready_in(Some(now - week + Duration::hours(50)), week, now), "2d 2h");
    }
}
