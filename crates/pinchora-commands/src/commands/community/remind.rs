//! Reminders: direct messages that wait for their time.
//!
//! `/remind me when daily|weekly|monthly|pet|trip on|off` is an opt-in: the message for each thing
//! is scheduled when the thing starts (a claim, a trip, a pet that was just looked after) and goes
//! out when it is ready. `/remind me in 2h30m <text>` is a note to yourself. Everything waits in the
//! `reminders` table, so it survives restarts; one ticker on every pod sends what is due, and the
//! table's leases keep two pods from sending the same message.

mod panel;
use panel::*;
mod delivery;
pub use delivery::*;
#[cfg(test)]
mod tests;


use std::{
    str::Split,
    sync::Arc,
    time::{Duration, Instant},
};

use chrono::{DateTime, Utc};
use pinchora_core::economy::legacy::DAILY_COOLDOWN;
use pinchora_core::economy::legacy::Period;
use pinchora_core::infra::i18n::Lang;
use pinchora_core::t;
use pinchora_db::{MAX_TEXT_REMINDERS, OPT_IN_KINDS, PetRow, ReminderRow, TEXT_REMINDER};
use pinchora_game::pets::Needs;
use twilight_interactions::command::{CommandModel, CommandOption, CreateCommand, CreateOption};
use twilight_model::{
    channel::message::{
        AllowedMentions, Component, MessageFlags,
        component::{ActionRow, SelectMenu, SelectMenuOption, SelectMenuType},
    },
    http::interaction::InteractionResponseData,
};
use twilight_util::builder::embed::{EmbedBuilder, EmbedFieldBuilder, EmbedFooterBuilder};

use crate::App;
use crate::Cx;
use crate::commands::social::pet;
use crate::infra::util::BRAND;
use crate::infra::util::relative;
use crate::progression::leveling::open_private_channel;

/// The ticker wakes this often.
const WAKE: Duration = Duration::from_secs(30);
/// Every this many wake-ups (an hour) the tombstones of old deletions are dropped.
const FORGET_EVERY: u32 = 120;
/// Reminders taken from the table and sent at once.
const BATCH: i64 = 25;
/// A pass keeps taking batches until it has run this long (the leases outlast it).
const BUDGET: Duration = Duration::from_secs(20);
/// Shortest and longest time a text reminder can be set for.
const MIN_AFTER: chrono::Duration = chrono::Duration::minutes(1);
const MAX_AFTER: chrono::Duration = chrono::Duration::days(365);
/// A pet is told about no sooner than this after it was last looked after, so a pet that has just
/// been fed is not nagged about its other needs at once.
const PET_GRACE_HOURS: f32 = 2.0;
/// Longest text a reminder keeps.
const MAX_TEXT: usize = 200;

// /Commands

#[derive(CommandModel, CreateCommand)]
#[command(
    name = "remind",
    desc = "A direct message when something is ready, or at a time you pick"
)]
pub enum RemindCommand {
    #[command(name = "me")]
    Me(RemindMe),
    #[command(name = "list")]
    List(RemindList),
    #[command(name = "cancel")]
    Cancel(RemindCancel),
    #[command(name = "repeat")]
    Repeat(RemindRepeat),
    #[command(name = "repeat-list")]
    RepeatList(RemindRepeatList),
    #[command(name = "repeat-cancel")]
    RepeatCancel(RemindRepeatCancel),
}

#[derive(CommandModel, CreateCommand)]
#[command(name = "repeat", desc = "Remind me every day, week, or chosen interval")]
pub struct RemindRepeat {
    /// Interval such as 1d or 1w, from one hour to one year
    duration: String,
    /// Private reminder text
    #[command(min_length = 1, max_length = 200)]
    text: String,
}
#[derive(CommandModel, CreateCommand)]
#[command(name = "repeat-list", desc = "Show my recurring reminders")]
pub struct RemindRepeatList;
#[derive(CommandModel, CreateCommand)]
#[command(name = "repeat-cancel", desc = "Cancel a recurring reminder")]
pub struct RemindRepeatCancel {
    /// Recurring reminder number
    id: i64,
}

#[derive(CommandModel, CreateCommand)]
#[command(name = "me", desc = "Set a reminder for yourself")]
pub enum RemindMe {
    #[command(name = "when")]
    When(RemindWhen),
    #[command(name = "in")]
    In(RemindIn),
}

#[derive(CommandModel, CreateCommand)]
#[command(name = "when", desc = "Message me when a reward, my pet or a pet trip is ready")]
pub struct RemindWhen {
    /// What to be told about
    what: Thing,
    /// Turn it on or off
    state: Toggle,
}

#[derive(CommandModel, CreateCommand)]
#[command(name = "in", desc = "Message me after a while: 45m, 2h30m, 3d")]
pub struct RemindIn {
    /// How long from now, e.g. 45m, 2h30m or 3d
    #[command(min_length = 2, max_length = 30)]
    duration: String,
    /// What to remind you of
    #[command(min_length = 1, max_length = 200)]
    text: String,
}

#[derive(CommandModel, CreateCommand)]
#[command(name = "list", desc = "Your reminders and which notifications are on")]
pub struct RemindList;

#[derive(CommandModel, CreateCommand)]
#[command(name = "cancel", desc = "Cancel a reminder you set with /remind me in")]
pub struct RemindCancel {
    /// The number from /remind list
    #[command(min_value = 1)]
    id: i64,
}

/// What `/remind me when` can tell about.
#[derive(CommandOption, CreateOption, Clone, Copy, Debug, PartialEq, Eq)]
pub enum Thing {
    #[option(name = "daily", value = "daily")]
    Daily,
    #[option(name = "weekly", value = "weekly")]
    Weekly,
    #[option(name = "monthly", value = "monthly")]
    Monthly,
    #[option(name = "pet (needs looking after)", value = "pet")]
    Pet,
    #[option(name = "trip (a pet is home)", value = "trip")]
    Trip,
}

impl Thing {
    fn key(self) -> &'static str {
        match self {
            Self::Daily => "daily",
            Self::Weekly => "weekly",
            Self::Monthly => "monthly",
            Self::Pet => "pet",
            Self::Trip => "trip",
        }
    }

    /// What it is about, in a sentence.
    fn phrase(self, lang: Lang) -> String {
        t!(lang, &format!("remind.thing.{}", self.key()))
    }
}

#[derive(CommandOption, CreateOption, Clone, Copy, Debug, PartialEq, Eq)]
pub enum Toggle {
    #[option(name = "on", value = "on")]
    On,
    #[option(name = "off", value = "off")]
    Off,
}

impl RemindCommand {
    pub async fn run(self, cx: &Cx) -> anyhow::Result<()> {
        let user = cx.author()?.id.get();
        match self {
            Self::Me(RemindMe::When(c)) => when(cx, user, c.what, c.state == Toggle::On).await,
            Self::Me(RemindMe::In(c)) => remind_in(cx, user, &c.duration, &c.text).await,
            Self::List(_) => {
                let data = list_panel(cx, user, "").await?;
                cx.reply(data).await
            }
            Self::Cancel(c) => {
                let lang = cx.lang();
                let note = if cx.app.db.cancel_text_reminder(user, c.id).await? {
                    t!(lang, "remind.cancelled", id = c.id)
                } else {
                    t!(lang, "remind.no-such", id = c.id)
                };
                cx.reply_ephemeral(note).await
            }
            Self::Repeat(c) => {
                let Some(span) = pinchora_core::admin::server_events::parse_duration(&c.duration) else {
                    return cx.reply_ephemeral(t!(cx.lang(), "community-v2.bad-interval")).await;
                };
                if span < chrono::Duration::hours(1) || span > chrono::Duration::days(365) {
                    return cx.reply_ephemeral(t!(cx.lang(), "community-v2.bad-interval")).await;
                }
                let text = crate::infra::util::strip_mentions(&c.text);
                if text.trim().is_empty() {
                    return cx.reply_ephemeral(t!(cx.lang(), "community-v2.text-required")).await;
                }
                let Some(id) = cx
                    .app
                    .db
                    .add_recurring_reminder(
                        user,
                        &text,
                        span.num_minutes() as i32,
                        cx.app.now() + span,
                        cx.lang().code(),
                    )
                    .await?
                else {
                    return cx.reply_ephemeral(t!(cx.lang(), "community-v2.recurring-full")).await;
                };
                cx.reply_ephemeral(t!(cx.lang(), "community-v2.recurring-set", id = id))
                    .await
            }
            Self::RepeatList(_) => {
                let rows: Vec<(i64,String,chrono::DateTime<chrono::Utc>)> = sqlx::query_as("SELECT id,body,next_at FROM personal_recurring_reminders WHERE user_id=$1 AND enabled ORDER BY next_at LIMIT 25")
                    .bind(user as i64).fetch_all(cx.app.db.pool()).await?;
                let lines = rows
                    .iter()
                    .map(|(id, body, next)| format!("`#{id}` · {} · {body}", relative(*next)))
                    .collect::<Vec<_>>()
                    .join("\n");
                cx.reply_ephemeral(if lines.is_empty() {
                    t!(cx.lang(), "community-v2.recurring-empty")
                } else {
                    lines.chars().take(1900).collect::<String>()
                })
                .await
            }
            Self::RepeatCancel(c) => {
                sqlx::query("DELETE FROM personal_recurring_reminders WHERE user_id=$1 AND id=$2")
                    .bind(user as i64)
                    .bind(c.id)
                    .execute(cx.app.db.pool())
                    .await?;
                cx.reply_ephemeral(t!(cx.lang(), "community-v2.recurring-removed"))
                    .await
            }
        }
    }
}

// /Notifications

async fn when(cx: &Cx, user: u64, what: Thing, on: bool) -> anyhow::Result<()> {
    let lang = cx.lang();
    cx.app.db.set_optin(user, what.key(), on).await?;
    if !on {
        return cx
            .reply_ephemeral(t!(lang, "remind.off", thing = what.phrase(lang)))
            .await;
    }
    let note = arm(&cx.app, user, what, lang).await?;
    cx.reply_ephemeral(t!(lang, "remind.on-hint", note = note, what = what.key()))
        .await
}

/// Schedules the message for what is already under way when a notification is turned on, and says
/// what will happen.
async fn arm(app: &App, user: u64, what: Thing, lang: Lang) -> anyhow::Result<String> {
    let now = app.now();
    let key = what.key();
    let next = match what {
        Thing::Daily => app.db.user(user).await?.last_daily_at.map(|t| t + DAILY_COOLDOWN),
        Thing::Weekly | Thing::Monthly => {
            let period = if what == Thing::Weekly {
                Period::Weekly
            } else {
                Period::Monthly
            };
            app.db
                .period_claimed_at(user, period)
                .await?
                .map(|t| t + period.cooldown())
        }
        Thing::Pet | Thing::Trip => None,
    };
    match what {
        Thing::Daily | Thing::Weekly | Thing::Monthly => Ok(match next.filter(|at| *at > now) {
            Some(at) => {
                app.db.schedule_reminder(user, key, 0, lang.code(), at).await?;
                t!(
                    lang,
                    "remind.next-ready",
                    kind = t!(lang, &format!("remind.kind.{key}")),
                    when = relative(at)
                )
            }
            None => t!(lang, "remind.ready-now", kind = t!(lang, &format!("remind.kind.{key}"))),
        }),
        Thing::Trip => {
            let away: Vec<PetRow> = app
                .db
                .pets(user)
                .await?
                .into_iter()
                .filter(|p| p.trip_dest.is_some())
                .collect();
            for pet in &away {
                refresh_pet(app, pet, lang).await;
            }
            Ok(match away.len() {
                0 => t!(lang, "remind.trip-none"),
                1 => t!(lang, "remind.trip-one", name = away[0].name),
                n => t!(lang, "remind.trip-many", count = n),
            })
        }
        Thing::Pet => Ok(match app.db.active_pet(user).await? {
            None => t!(lang, "remind.pet-none"),
            Some(pet) => {
                refresh_pet(app, &pet, lang).await;
                match app.db.scheduled_reminder(user, key, pet.id).await? {
                    Some(at) => t!(lang, "remind.pet-at", name = pet.name, when = relative(at)),
                    None => t!(lang, "remind.pet-later", name = pet.name),
                }
            }
        }),
    }
}

/// After a reward was collected: if the player asked to be told, schedules the message for when it
/// is ready again. `lang` is stored with it (in its `body`), for the message to be written in when
/// the player has made no `/language` choice. Failures are logged and swallowed: a reminder never
/// breaks a command.
pub(crate) async fn after_claim(app: &App, user: u64, kind: &str, ready_at: DateTime<Utc>, lang: Lang) {
    let result = async {
        if app.db.has_optin(user, kind).await? {
            app.db.schedule_reminder(user, kind, 0, lang.code(), ready_at).await?;
        }
        anyhow::Ok(())
    };
    if let Err(error) = result.await {
        tracing::debug!(%error, user, kind, "reminder not scheduled");
    }
}

/// When a pet needs looking after, as of its last care: the first of hunger, fun and hygiene to run
/// low, but not before the grace period after it was last looked after.
fn pet_due(pet: &PetRow) -> DateTime<Utc> {
    let stored = Needs {
        hunger: pet.hunger,
        fun: pet.fun,
        energy: pet.energy,
        hygiene: pet.hygiene,
    };
    let hours = stored.hours_until_low().max(PET_GRACE_HOURS);
    pet.needs_at + chrono::Duration::seconds((hours * 3600.0) as i64)
}

/// Keeps the notifications about `pet` in step with how it is now: a trip that ends, and the
/// moment the active pet will need looking after. Called whenever a pet's card is shown, which is
/// every time its state changes. `lang` is stored with the messages (see [`after_claim`]). Failures
/// are logged and swallowed.
pub(crate) async fn refresh_pet(app: &App, pet: &PetRow, lang: Lang) {
    if let Err(error) = try_refresh_pet(app, pet, lang).await {
        tracing::debug!(%error, pet = pet.id, "pet reminders not scheduled");
    }
}

async fn try_refresh_pet(app: &App, pet: &PetRow, lang: Lang) -> anyhow::Result<()> {
    let user = pet.owner_id as u64;
    let optins = app.db.reminder_optins(user).await?;
    if optins.is_empty() {
        return Ok(());
    }
    let now = app.now();
    // Only a moment still to come is scheduled: one that has passed was sent (or is about to be) and
    // looking at the pet again must not send it again.
    if optins.iter().any(|k| k == "trip") {
        match (&pet.trip_dest, pet.trip_ends_at) {
            (Some(_), Some(ends)) if ends > now => {
                app.db
                    .schedule_reminder(user, "trip", pet.id, lang.code(), ends)
                    .await?;
            }
            (Some(_), Some(_)) => {}
            _ => app.db.unschedule_reminder(user, "trip", pet.id).await?,
        }
    }
    if optins.iter().any(|k| k == "pet") {
        if pet.active && !pet.is_away(now) && !pet::is_asleep(pet, now) {
            let due = pet_due(pet);
            if due > now {
                app.db.schedule_reminder(user, "pet", pet.id, lang.code(), due).await?;
            }
        } else {
            app.db.unschedule_reminder(user, "pet", pet.id).await?;
        }
    }
    Ok(())
}

/// `rm:c`: the menu under the list cancels the reminder picked.
pub(crate) async fn on_component(cx: &Cx, mut args: Split<'_, char>, value: Option<&str>) -> anyhow::Result<()> {
    let lang = cx.lang();
    match args.next() {
        Some("c") => {
            let user = cx.author()?.id.get();
            let id: i64 = value.and_then(|v| v.parse().ok()).unwrap_or(0);
            let note = if cx.app.db.cancel_text_reminder(user, id).await? {
                t!(lang, "remind.cancelled", id = id)
            } else {
                t!(lang, "remind.already-gone")
            };
            let data = list_panel(cx, user, &note).await?;
            cx.update(data).await
        }
        _ => cx.reply_ephemeral(t!(lang, "remind.expired")).await,
    }
}
