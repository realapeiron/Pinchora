//! `/weekly` and `/monthly`: shell rewards with a cooldown of their own, like `/daily`.

use chrono::{DateTime, Utc};
use pinchora_core::{economy::Period, i18n::Lang, t};
use pinchora_db::PeriodOutcome;
use twilight_interactions::command::{CommandModel, CreateCommand};
use twilight_util::builder::embed::EmbedBuilder;

use crate::{
    Cx,
    commands::{customize, remind},
    util::{SUCCESS, WARN, relative, thousands_in},
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

async fn claim(cx: &Cx, period: Period) -> anyhow::Result<()> {
    let user = cx.author()?.id.get();
    let outcome = cx.app.db.claim_period(user, period, cx.app.now()).await?;
    let claimed = matches!(outcome, PeriodOutcome::Claimed { .. });
    let lang = cx.lang();
    // The catalog keys are `weekly.*` and `monthly.*`.
    let key = |name: &str| format!("{}.{name}", period.key());
    let embed = match outcome {
        PeriodOutcome::Claimed {
            amount,
            balance,
            next_at,
        } => {
            remind::after_claim(&cx.app, user, period.key(), next_at, cx.lang()).await;
            EmbedBuilder::new()
                .color(SUCCESS)
                .title(t!(lang, "daily.claimed-title", amount = thousands_in(lang, amount)))
                .description(t!(
                    lang,
                    &key("claimed"),
                    balance = thousands_in(lang, balance),
                    when = relative(next_at)
                ))
                .build()
        }
        PeriodOutcome::Cooldown { next_at } => EmbedBuilder::new()
            .color(WARN)
            .title(t!(lang, &key("cooldown-title")))
            .description(t!(lang, &key("cooldown"), when = relative(next_at)))
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
/// `12d 4h`, and `24h` or `7d` when nothing is left over; in Turkish `hazır`, `3sa 20dk`, `12g 4sa`.
pub(crate) fn ready_in(
    claimed_at: Option<DateTime<Utc>>,
    cooldown: chrono::Duration,
    now: DateTime<Utc>,
    lang: Lang,
) -> String {
    let left = match claimed_at {
        Some(t) if t + cooldown > now => t + cooldown - now,
        _ => return t!(lang, "time.ready"),
    };
    let (days, hours, minutes) = (
        |n: i64| t!(lang, "time.days", n = n),
        |n: i64| t!(lang, "time.hours", n = n),
        |n: i64| t!(lang, "time.minutes", n = n),
    );
    let total = (left.num_seconds() + 59) / 60;
    match total {
        // A day's worth reads better in hours; the days start once there are two of them.
        m if m >= 48 * 60 => match m % (24 * 60) / 60 {
            0 => days(m / (24 * 60)),
            h => format!("{} {}", days(m / (24 * 60)), hours(h)),
        },
        m if m >= 60 => match m % 60 {
            0 => hours(m / 60),
            rest => format!("{} {}", hours(m / 60), minutes(rest)),
        },
        m => minutes(m.max(1)),
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
        assert_eq!(ready_in(None, week, now, Lang::En), "ready");
        assert_eq!(ready_in(Some(now - week), week, now, Lang::En), "ready");
        assert_eq!(
            ready_in(Some(now - week - Duration::hours(3)), week, now, Lang::En),
            "ready"
        );
        assert_eq!(
            ready_in(Some(now - week + Duration::seconds(20)), week, now, Lang::En),
            "1m"
        );
        assert_eq!(
            ready_in(Some(now - week + Duration::minutes(5)), week, now, Lang::En),
            "5m"
        );
        assert_eq!(
            ready_in(
                Some(now - week + Duration::hours(3) + Duration::minutes(20)),
                week,
                now,
                Lang::En
            ),
            "3h 20m"
        );
        assert_eq!(
            ready_in(
                Some(now - Duration::days(30) + Duration::days(12) + Duration::hours(4)),
                Duration::days(30),
                now,
                Lang::En
            ),
            "12d 4h"
        );
        assert_eq!(ready_in(Some(now), Duration::days(30), now, Lang::En), "30d");
        assert_eq!(ready_in(Some(now), Duration::days(1), now, Lang::En), "24h");
        assert_eq!(
            ready_in(Some(now - week + Duration::hours(47)), week, now, Lang::En),
            "47h"
        );
        assert_eq!(
            ready_in(Some(now - week + Duration::hours(50)), week, now, Lang::En),
            "2d 2h"
        );
        assert_eq!(
            ready_in(Some(now - week + Duration::hours(50)), week, now, Lang::Tr),
            "2g 2sa"
        );
        assert_eq!(ready_in(None, week, now, Lang::Tr), "hazır");
    }
}
