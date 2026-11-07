//! Reminders: direct messages that wait for their time.
//!
//! `/remind me when daily|weekly|monthly|pet|trip on|off` is an opt-in: the message for each thing
//! is scheduled when the thing starts (a claim, a trip, a pet that was just looked after) and goes
//! out when it is ready. `/remind me in 2h30m <text>` is a note to yourself. Everything waits in the
//! `reminders` table, so it survives restarts; one ticker on every pod sends what is due, and the
//! table's leases keep two pods from sending the same message.

use std::{
    str::Split,
    sync::Arc,
    time::{Duration, Instant},
};

use chrono::{DateTime, Utc};
use pinchora_core::{
    economy::{DAILY_COOLDOWN, Period},
    i18n::Lang,
    t,
};
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

use crate::{
    App, Cx,
    commands::pet,
    leveling::open_private_channel,
    util::{BRAND, relative},
};

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

// /Text reminders

async fn remind_in(cx: &Cx, user: u64, duration: &str, text: &str) -> anyhow::Result<()> {
    let lang = cx.lang();
    let after = match parse_duration(duration) {
        Ok(after) => after,
        Err(why) => return cx.reply_ephemeral(t!(lang, why)).await,
    };
    let body = strip_mentions(text);
    if body.is_empty() {
        return cx.reply_ephemeral(t!(lang, "remind.write-text")).await;
    }
    let due = cx.app.now() + after;
    match cx.app.db.add_text_reminder_in(user, &body, lang.code(), due).await? {
        None => {
            cx.reply_ephemeral(t!(lang, "remind.too-many", max = MAX_TEXT_REMINDERS))
                .await
        }
        Some(id) => {
            let embed = EmbedBuilder::new()
                .color(BRAND)
                .title(t!(lang, "remind.set-title"))
                .description(t!(lang, "remind.set-text", when = relative(due), body = body))
                .footer(EmbedFooterBuilder::new(t!(lang, "remind.set-footer", id = id)))
                .build();
            cx.reply(InteractionResponseData {
                embeds: Some(vec![embed]),
                flags: Some(MessageFlags::EPHEMERAL),
                allowed_mentions: Some(AllowedMentions::default()),
                ..Default::default()
            })
            .await
        }
    }
}

/// "2h30m", "45 min", "1d 12h", "2sa30dk": a time from now. Units are `w`, `d`, `h`, `m` and `s` (and
/// their longer spellings), or in Turkish `hf`, `g`, `sa`, `dk` and `sn`; a bare number is refused
/// because it could mean anything. An error is the catalog key of what to tell the player.
fn parse_duration(text: &str) -> Result<chrono::Duration, &'static str> {
    const BAD: &str = "remind.bad-time";
    const TOO_LONG: &str = "remind.time-too-long";
    let mut total = chrono::Duration::zero();
    let mut chars = text.trim().chars().peekable();
    let mut any = false;
    while chars.peek().is_some() {
        while chars.next_if(|c| c.is_whitespace() || *c == ',').is_some() {}
        let mut digits = String::new();
        while let Some(c) = chars.next_if(char::is_ascii_digit) {
            digits.push(c);
        }
        while !digits.is_empty() && chars.next_if(|c| c.is_whitespace()).is_some() {}
        let mut unit = String::new();
        while let Some(c) = chars.next_if(|c| c.is_alphabetic()) {
            unit.extend(c.to_lowercase());
        }
        if digits.is_empty() && unit.is_empty() && chars.peek().is_none() {
            break;
        }
        let amount: i64 = digits.parse().map_err(|_| BAD)?;
        let step = match unit.as_str() {
            "w" | "wk" | "wks" | "week" | "weeks" | "hf" | "hafta" => chrono::Duration::try_weeks(amount),
            "d" | "day" | "days" | "g" | "gün" | "gun" => chrono::Duration::try_days(amount),
            "h" | "hr" | "hrs" | "hour" | "hours" | "sa" | "saat" => chrono::Duration::try_hours(amount),
            "m" | "min" | "mins" | "minute" | "minutes" | "dk" | "dakika" => chrono::Duration::try_minutes(amount),
            "s" | "sec" | "secs" | "second" | "seconds" | "sn" | "saniye" => chrono::Duration::try_seconds(amount),
            _ => return Err(BAD),
        }
        .ok_or(TOO_LONG)?;
        total = total.checked_add(&step).ok_or(TOO_LONG)?;
        any = true;
        if total > MAX_AFTER {
            return Err("remind.time-year");
        }
    }
    if !any {
        return Err(BAD);
    }
    if total < MIN_AFTER {
        return Err("remind.time-too-soon");
    }
    Ok(total)
}

/// A reminder's text without any mention: users, roles, channels, `@everyone` and `@here` are
/// dropped, and the rest tidied up.
fn strip_mentions(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(open) = rest.find('<') {
        out.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        let end = after.find('>');
        let is_mention = end.is_some_and(|end| {
            let inner = &after[..end];
            let digits = inner
                .strip_prefix("@&")
                .or_else(|| inner.strip_prefix("@!"))
                .or_else(|| inner.strip_prefix('@'))
                .or_else(|| inner.strip_prefix('#'));
            digits.is_some_and(|d| !d.is_empty() && d.bytes().all(|b| b.is_ascii_digit()))
        });
        match end {
            Some(end) if is_mention => rest = &after[end + 1..],
            _ => {
                out.push('<');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    // `@everyone` and `@here`, whatever their case, and the zero-width characters people hide them in.
    let plain: String = out
        .chars()
        .filter(|c| !matches!(c, '\u{200b}' | '\u{200c}' | '\u{200d}' | '\u{feff}'))
        .collect();
    let mut cleaned = String::with_capacity(plain.len());
    let mut i = 0;
    while i < plain.len() {
        let rest = &plain.as_bytes()[i..];
        // The words are ASCII, so a match ends on a character boundary.
        let skip = ["@everyone", "@here"]
            .iter()
            .find(|m| rest.len() >= m.len() && rest[..m.len()].eq_ignore_ascii_case(m.as_bytes()))
            .map(|m| m.len());
        match skip {
            Some(n) => i += n,
            None => {
                let c = plain[i..].chars().next().unwrap_or(' ');
                cleaned.push(c);
                i += c.len_utf8();
            }
        }
    }
    let tidy = cleaned.split_whitespace().collect::<Vec<_>>().join(" ");
    tidy.chars().take(MAX_TEXT).collect()
}

// /List

fn chosen(kind: &str, optins: &[String]) -> bool {
    optins.iter().any(|k| k == kind)
}

fn label_of(kind: &str, lang: Lang) -> String {
    match kind {
        "daily" | "weekly" | "monthly" | "pet" | "trip" => t!(lang, &format!("remind.label.{kind}")),
        _ => String::new(),
    }
}

/// The list: which notifications are on, and the reminders still to come, with a menu to cancel one.
async fn list_panel(cx: &Cx, user: u64, note: &str) -> anyhow::Result<InteractionResponseData> {
    let lang = cx.lang();
    let optins = cx.app.db.reminder_optins(user).await?;
    let rows = cx.app.db.text_reminders(user).await?;
    let notifications = OPT_IN_KINDS
        .iter()
        .map(|kind| {
            format!(
                "{} {}",
                if chosen(kind, &optins) { "🔔" } else { "🔕" },
                label_of(kind, lang)
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    // An embed field holds 1,024 characters: long notes are cut here, and the menu below has them all.
    let mut reminders = String::new();
    for (i, r) in rows.iter().enumerate() {
        let mut body: String = r.body.chars().take(40).collect();
        if body.len() < r.body.len() {
            body.push('…');
        }
        let line = format!("`#{}` {} · {body}\n", r.id, relative(r.due_at));
        if reminders.chars().count() + line.chars().count() > 980 {
            reminders.push_str(&t!(lang, "remind.list-more", count = rows.len() - i));
            break;
        }
        reminders.push_str(&line);
    }
    let reminders = if rows.is_empty() {
        t!(lang, "remind.list-none")
    } else {
        reminders.trim_end().to_owned()
    };
    let mut embed = EmbedBuilder::new()
        .color(BRAND)
        .title(t!(lang, "remind.list-title"))
        .field(EmbedFieldBuilder::new(
            t!(lang, "remind.list-notifications"),
            notifications,
        ))
        .field(EmbedFieldBuilder::new(t!(lang, "remind.list-reminders"), reminders))
        .footer(EmbedFooterBuilder::new(t!(
            lang,
            "remind.list-footer",
            count = rows.len(),
            max = MAX_TEXT_REMINDERS
        )));
    if !note.is_empty() {
        embed = embed.description(note);
    }
    let mut components = Vec::new();
    if !rows.is_empty() {
        let options = rows
            .iter()
            .map(|r| SelectMenuOption {
                default: false,
                description: Some(r.due_at.format("%Y-%m-%d %H:%M UTC").to_string()),
                emoji: None,
                label: r.body.chars().take(100).collect(),
                value: r.id.to_string(),
            })
            .collect();
        components.push(Component::ActionRow(ActionRow {
            id: None,
            components: vec![Component::SelectMenu(SelectMenu {
                channel_types: None,
                custom_id: "rm:c".to_owned(),
                default_values: None,
                disabled: false,
                id: None,
                kind: SelectMenuType::Text,
                max_values: Some(1),
                min_values: Some(1),
                options: Some(options),
                placeholder: Some(t!(lang, "remind.list-placeholder")),
                required: None,
            })],
        }));
    }
    Ok(InteractionResponseData {
        embeds: Some(vec![embed.build()]),
        components: Some(components),
        flags: Some(MessageFlags::EPHEMERAL),
        allowed_mentions: Some(AllowedMentions::default()),
        ..Default::default()
    })
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

// /Sending

/// What to do with a reminder that is due.
enum Delivery {
    Send {
        title: String,
        text: String,
    },
    /// Not wanted any more, or about something that no longer holds.
    Drop,
    /// Not time yet after all.
    Later(DateTime<Utc>),
}

/// The language a due reminder is written in. There is no interaction to ask: the player's
/// `/language` pick, else the language their command was answered in when the message was scheduled
/// (the automatic kinds keep it in their `body`), else English.
async fn delivery_lang(app: &App, r: &ReminderRow) -> Lang {
    let stored = if r.kind == TEXT_REMINDER {
        Lang::from_code(&r.language)
    } else {
        Lang::from_code(&r.body)
    };
    match app.user_lang_choice(r.user_id as u64).await {
        Some(lang) => lang,
        None => stored.unwrap_or_default(),
    }
}

async fn plan(app: &App, r: &ReminderRow, now: DateTime<Utc>, lang: Lang) -> anyhow::Result<Delivery> {
    let user = r.user_id as u64;
    if r.kind == TEXT_REMINDER {
        return Ok(Delivery::Send {
            title: t!(lang, "remind.text-title"),
            text: r.body.clone(),
        });
    }
    if let Some(minutes) = r.kind.strip_prefix("event_").and_then(|v| v.parse::<i64>().ok()) {
        return Ok(
            match crate::event_rsvp::reminder_plan(app, r.ref_id, user, minutes, lang).await? {
                Some((title, text)) => Delivery::Send { title, text },
                None => Delivery::Drop,
            },
        );
    }
    // Everything else only goes out while the player still wants it.
    if !app.db.has_optin(user, &r.kind).await? {
        return Ok(Delivery::Drop);
    }
    Ok(match r.kind.as_str() {
        "daily" | "weekly" | "monthly" => Delivery::Send {
            title: t!(lang, &format!("remind.title.{}", r.kind)),
            text: format!(
                "{}\n{}",
                t!(lang, "remind.collect", cmd = r.kind),
                t!(lang, "remind.turn-off", what = r.kind)
            ),
        },
        "trip" => match app.db.pets(user).await?.into_iter().find(|p| p.id == r.ref_id) {
            Some(pet) if pet.has_souvenir(now) => Delivery::Send {
                title: t!(lang, "remind.trip-title", name = pet.name),
                text: format!(
                    "{}\n{}",
                    t!(lang, "remind.trip-text"),
                    t!(lang, "remind.turn-off", what = "trip")
                ),
            },
            _ => Delivery::Drop,
        },
        "pet" => match app.db.pets(user).await?.into_iter().find(|p| p.id == r.ref_id) {
            Some(pet) if pet.active && !pet.is_away(now) && !pet::is_asleep(&pet, now) => {
                let due = pet_due(&pet);
                if due > now {
                    Delivery::Later(due)
                } else {
                    let stored = Needs {
                        hunger: pet.hunger,
                        fun: pet.fun,
                        energy: pet.energy,
                        hygiene: pet.hygiene,
                    };
                    let hours = (now - pet.needs_at).num_seconds() as f32 / 3600.0;
                    let wants = t!(lang, &format!("remind.wants.{}", stored.after(hours).want_key()));
                    Delivery::Send {
                        title: t!(lang, "remind.pet-title", name = pet.name),
                        text: format!(
                            "{}\n{}",
                            t!(lang, "remind.pet-text", wants = wants),
                            t!(lang, "remind.turn-off", what = "pet")
                        ),
                    }
                }
            }
            _ => Delivery::Drop,
        },
        _ => Delivery::Drop,
    })
}

/// Sends one due reminder. True when a message went out.
async fn deliver(app: &App, r: &ReminderRow, now: DateTime<Utc>) -> anyhow::Result<bool> {
    let lang = delivery_lang(app, r).await;
    let (title, text) = match plan(app, r, now, lang).await? {
        Delivery::Send { title, text } => (title, text),
        Delivery::Drop => {
            app.db.finish_reminder(r).await?;
            return Ok(false);
        }
        Delivery::Later(at) => {
            app.db.postpone_reminder(r, at).await?;
            return Ok(false);
        }
    };
    let user = r.user_id as u64;
    let embed = EmbedBuilder::new().color(BRAND).title(title).description(text).build();
    let mentions = AllowedMentions::default();
    let sent = async {
        let channel = open_private_channel(app, user)
            .await
            .ok_or_else(|| anyhow::anyhow!("direct messages are closed"))?;
        app.http
            .create_message(channel)
            .embeds(&[embed])
            .allowed_mentions(Some(&mentions))
            .await?;
        anyhow::Ok(())
    }
    .await;
    // Closed direct messages are the player's choice: the reminder is dropped, not retried.
    if let Err(error) = &sent {
        tracing::debug!(%error, user, kind = %r.kind, "reminder not delivered");
    }
    app.db.finish_reminder(r).await?;
    Ok(sent.is_ok())
}

/// Sends every reminder that is due: one pass of the ticker. A batch goes out at once, and batches
/// follow each other while there are more and the pass has used less than [`BUDGET`]. Returns how
/// many messages went out.
pub async fn tick(app: &Arc<App>) -> anyhow::Result<usize> {
    let now = app.now();
    let started = Instant::now();
    let mut sent = 0;
    loop {
        let due = app.db.claim_due_reminders(now, BATCH).await?;
        let full = due.len() as i64 == BATCH;
        let mut batch = tokio::task::JoinSet::new();
        for reminder in due {
            let app = app.clone();
            batch.spawn(async move {
                let result = deliver(&app, &reminder, now).await;
                (reminder.id, result)
            });
        }
        while let Some(done) = batch.join_next().await {
            match done {
                Ok((_, Ok(true))) => sent += 1,
                Ok((_, Ok(false))) => {}
                // The lease runs out and the reminder is tried again.
                Ok((id, Err(error))) => tracing::warn!(%error, id, "reminder not handled"),
                Err(error) => tracing::warn!(%error, "reminder task failed"),
            }
        }
        if !full || started.elapsed() > BUDGET {
            break;
        }
    }
    metrics::counter!("pinchora_reminders_sent_total").increment(sent as u64);
    Ok(sent)
}

/// Runs [`tick`] every 30 seconds for the life of the process, and forgets the players who deleted
/// their data long enough ago once an hour.
pub fn spawn_ticker(app: Arc<App>) {
    tokio::spawn(async move {
        let mut wake = tokio::time::interval(WAKE);
        wake.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let mut rounds = 0u32;
        loop {
            wake.tick().await;
            if let Err(error) = tick(&app).await {
                tracing::warn!(%error, "reminder tick failed");
            }
            rounds = rounds.wrapping_add(1);
            if rounds.is_multiple_of(FORGET_EVERY)
                && let Err(error) = app.db.forget_old_tombstones(app.now()).await
            {
                tracing::warn!(%error, "old deletions not forgotten");
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn durations_are_read_like_people_write_them() {
        let m = chrono::Duration::minutes;
        let h = chrono::Duration::hours;
        assert_eq!(parse_duration("45m"), Ok(m(45)));
        assert_eq!(parse_duration("2h30m"), Ok(h(2) + m(30)));
        assert_eq!(parse_duration(" 1d 12H "), Ok(h(36)));
        assert_eq!(parse_duration("90 minutes"), Ok(m(90)));
        assert_eq!(parse_duration("1w"), Ok(chrono::Duration::days(7)));
        assert_eq!(parse_duration("1h,30min"), Ok(m(90)));
        assert_eq!(parse_duration("60s"), Ok(m(1)));
    }

    #[test]
    fn turkish_units_are_read_too() {
        let m = chrono::Duration::minutes;
        let h = chrono::Duration::hours;
        assert_eq!(parse_duration("45dk"), Ok(m(45)));
        assert_eq!(parse_duration("2sa30dk"), Ok(h(2) + m(30)));
        assert_eq!(parse_duration("2 saat 30 dakika"), Ok(h(2) + m(30)));
        assert_eq!(parse_duration(" 1G 12SA "), Ok(h(36)));
        assert_eq!(parse_duration("3 gün"), Ok(chrono::Duration::days(3)));
        assert_eq!(parse_duration("3 gun"), Ok(chrono::Duration::days(3)));
        assert_eq!(parse_duration("1hf"), Ok(chrono::Duration::days(7)));
        assert_eq!(parse_duration("2 HAFTA"), Ok(chrono::Duration::days(14)));
        assert_eq!(parse_duration("60sn"), Ok(m(1)));
        assert_eq!(parse_duration("90 saniye"), Ok(m(1) + chrono::Duration::seconds(30)));
        assert_eq!(parse_duration("1d 30dk"), Ok(h(24) + m(30)));
        for bad in ["yakında", "5ş", "5gü", "dk", "2sa30"] {
            assert_eq!(parse_duration(bad), Err("remind.bad-time"), "{bad:?}");
        }
        assert!(t!(Lang::Tr, parse_duration("yakında").unwrap_err()).contains("hf, g, sa, dk, sn"));
    }

    #[test]
    fn durations_that_make_no_sense_are_refused() {
        for bad in ["", "soon", "30", "m5", "5x", "h", "2h30", "1.5h", "-5m"] {
            assert!(parse_duration(bad).is_err(), "{bad:?}");
        }
        assert!(t!(Lang::En, parse_duration("30s").unwrap_err()).contains("too soon"));
        assert!(parse_duration("0m").is_err());
        assert!(t!(Lang::En, parse_duration("366d").unwrap_err()).contains("a year"));
        assert!(parse_duration("99999999999999999999h").is_err());
        assert!(parse_duration("9999999999999w").is_err());
        assert!(parse_duration("365d").is_ok());
    }

    #[test]
    fn mentions_are_stripped_and_the_text_tidied() {
        assert_eq!(strip_mentions("call <@123> and <@!456>"), "call and");
        assert_eq!(strip_mentions("<@&789> meeting in <#42>"), "meeting in");
        assert_eq!(strip_mentions("ping @everyone and @HERE now"), "ping and now");
        assert_eq!(strip_mentions("sneaky @ever\u{200b}yone"), "sneaky");
        assert_eq!(strip_mentions("a < b and b > c"), "a < b and b > c");
        assert_eq!(strip_mentions("<@abc> stays"), "<@abc> stays");
        assert_eq!(strip_mentions("  two   spaces\nand a line "), "two spaces and a line");
        assert_eq!(strip_mentions("<@1>"), "");
        assert_eq!(strip_mentions("plain 🐚 text <t:1234:R>"), "plain 🐚 text <t:1234:R>");
        assert_eq!(strip_mentions(&"x".repeat(300)).chars().count(), MAX_TEXT);
    }

    #[test]
    fn a_pet_is_not_told_about_at_once() {
        let now = Utc::now();
        let pet = |fun: i16| PetRow {
            id: 1,
            owner_id: 1,
            species: "crab".into(),
            name: "Pinchy".into(),
            level: 1,
            xp: 0,
            hunger: 100,
            fun,
            energy: 100,
            hygiene: 100,
            needs_at: now,
            active: true,
            trip_dest: None,
            trip_ends_at: None,
        };
        // Fun 80 loses 5 an hour and is low (30) after 10 h.
        assert_eq!(pet_due(&pet(80)), now + chrono::Duration::hours(10));
        // Already low: the grace period after the last care.
        assert_eq!(pet_due(&pet(5)), now + chrono::Duration::hours(2));
    }
}
