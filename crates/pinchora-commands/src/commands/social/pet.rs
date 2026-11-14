mod state;
use state::*;
pub use state::{current_needs, is_asleep};
mod components;
use components::*;
mod cards;
use cards::*;
pub use cards::{pet_png};
mod adoption;
use adoption::*;
pub(crate) use adoption::{on_adopt, on_switch};
#[cfg(test)]
mod tests;

use std::{borrow::Cow, str::Split, time::Duration};

use chrono::{DateTime, Utc};
use pinchora_core::admin::server_stats::decimal_in;
use pinchora_core::infra::i18n::Lang;
use pinchora_core::t;
use pinchora_db::{PetRow, Wake};
use pinchora_game::Content;
use pinchora_game::Rarity;
use pinchora_game::lang::lower;
use pinchora_game::pets;
use pinchora_game::pets::CAPSULE_PRICE;
use pinchora_game::pets::Care;
use pinchora_game::pets::DUPLICATE_REFUND;
use pinchora_game::pets::FOOD_PRICE;
use pinchora_game::pets::Needs;
use pinchora_game::pets::WAKE_FEE;
use pinchora_game::pets::WAKE_NEEDS;
use pinchora_game::trips;
use pinchora_render::{PetCard, PetStatus, Theme};
use twilight_interactions::command::{CommandModel, CreateCommand};
use twilight_model::{
    channel::message::{
        Component, MessageFlags,
        component::{ActionRow, Button, ButtonStyle, SelectMenu, SelectMenuOption, SelectMenuType},
    },
    http::interaction::InteractionResponseData,
};
use twilight_util::builder::embed::EmbedBuilder;

use crate::App;
use crate::Cx;
use crate::commands::community::remind;
use crate::commands::social::customize;
use crate::games::game::scene::Sprites;
use crate::infra::util::BRAND;
use crate::infra::util::WARN;
use crate::infra::util::thousands_in;

mod nursery;
mod trip;

pub(crate) use crate::commands::social::pet::nursery::on_nursery;
pub(crate) use crate::commands::social::pet::trip::on_trip;

// /Commands

#[derive(CommandModel, CreateCommand)]
#[command(name = "pet", desc = "Your pets")]
pub enum PetCommand {
    #[command(name = "view")]
    View(PetView),
    #[command(name = "adopt")]
    Adopt(PetAdopt),
    #[command(name = "capsule")]
    Capsule(PetCapsule),
    #[command(name = "list")]
    List(PetList),
    #[command(name = "rename")]
    Rename(PetRename),
    #[command(name = "trip")]
    Trip(crate::commands::social::pet::trip::PetTrip),
    #[command(name = "streak")]
    PetStreak(crate::commands::games::reef_loop::PetStreak),
}

#[derive(CommandModel, CreateCommand)]
#[command(name = "view", desc = "See your active pet and look after it")]
pub struct PetView;

#[derive(CommandModel, CreateCommand)]
#[command(name = "adopt", desc = "Adopt your first pet for free")]
pub struct PetAdopt;

#[derive(CommandModel, CreateCommand)]
#[command(name = "capsule", desc = "Open a pet capsule (1,500 shells) for a random pet")]
pub struct PetCapsule;

#[derive(CommandModel, CreateCommand)]
#[command(
    name = "list",
    desc = "Your nursery: every pet at a glance; open one or pick your active pet"
)]
pub struct PetList;

#[derive(CommandModel, CreateCommand)]
#[command(name = "rename", desc = "Rename your active pet")]
pub struct PetRename {
    /// New name
    #[command(min_length = 1, max_length = 20)]
    name: String,
}

impl PetCommand {
    pub async fn run(self, cx: &Cx) -> anyhow::Result<()> {
        let user = cx.author()?.id.get();
        let lang = cx.lang();
        match self {
            Self::PetStreak(c) => c.run(cx).await,

            Self::View(_) => match cx.app.db.active_pet(user).await? {
                Some(pet) => {
                    let data = pet_card(&cx.app, lang, &pet, None).await?;
                    cx.reply(data).await
                }
                None => cx.reply_ephemeral(t!(lang, "pet.no-pet")).await,
            },
            Self::Adopt(_) => adopt_menu(cx, user).await,
            Self::Capsule(_) => capsule(cx, user).await,
            Self::List(_) => crate::commands::social::pet::nursery::open(cx, user).await,
            Self::Rename(c) => {
                let Some(pet) = cx.app.db.active_pet(user).await? else {
                    return cx.reply_ephemeral(t!(lang, "pet.no-pet-short")).await;
                };
                let name: String = c.name.split_whitespace().collect::<Vec<_>>().join(" ");
                cx.app.db.rename_pet(user, pet.id, &name).await?;
                cx.reply_ephemeral(t!(lang, "pet.renamed", name = name)).await
            }
            Self::Trip(trip) => trip.run(cx, user).await,
        }
    }
}

// /Care

/// `pc:<action>:<pet>` buttons.
pub(crate) async fn on_care(cx: &Cx, mut args: Split<'_, char>) -> anyhow::Result<()> {
    let lang = cx.lang();
    let action = args.next();
    let pet_id: i64 = args.next().and_then(|v| v.parse().ok()).unwrap_or(0);
    let action = match action {
        Some("feed") => Care::Feed,
        Some("play") => Care::Play,
        Some("walk") => Care::Walk,
        Some("clean") => Care::Clean,
        Some("wake") => return wake(cx, pet_id).await,
        _ => return cx.reply_ephemeral(t!(lang, "pet.care.stale")).await,
    };
    let user = cx.author()?.id.get();
    let Some(pet) = cx.app.db.pets(user).await?.into_iter().find(|p| p.id == pet_id) else {
        return cx.reply_ephemeral(t!(lang, "pet.care.not-yours")).await;
    };
    let now = cx.app.now();
    match status(&pet, now) {
        PetStatus::Away => {
            let left = pet.trip_ends_at.map_or(0, |ends| (ends - now).num_seconds());
            return cx
                .reply_ephemeral(t!(
                    lang,
                    "pet.care.away",
                    name = pet.name,
                    place = place_of(&pet, lang),
                    span = span(left, lang)
                ))
                .await;
        }
        PetStatus::Asleep => {
            return cx.reply_ephemeral(t!(lang, "pet.care.asleep", name = pet.name)).await;
        }
        PetStatus::Awake | PetStatus::Back => {}
    }
    // The walk's cooldown belongs to the pet, so a player who deleted their data and adopted again is
    // held to the one their old pet was on.
    if action == Care::Walk
        && let Some(until) = cx.app.db.walk_locked_until(user, now).await?
    {
        return cx
            .reply_ephemeral(t!(
                lang,
                "pet.care.break",
                name = pet.name,
                minutes = (until - now).num_minutes() + 1
            ))
            .await;
    }
    let key = format!("pet:{pet_id}:{action:?}");
    if let Some(left) = cx
        .app
        .cache
        .try_cooldown(&key, Duration::from_secs(action.cooldown_secs()))
        .await?
    {
        return cx
            .reply_ephemeral(t!(
                lang,
                "pet.care.break",
                name = pet.name,
                minutes = left.as_secs() / 60 + 1
            ))
            .await;
    }
    let needs = current_needs(&pet, now);
    let Some(after) = action.apply(needs) else {
        cx.app.cache.release_cooldown(&key).await?;
        return cx
            .reply_ephemeral(t!(lang, "pet.care.too-tired", name = pet.name))
            .await;
    };
    let progress = pets::add_xp(pet.level, pet.xp, action.xp());
    let found = if action == Care::Walk {
        pets::walk_shells(pet.level, rand::random::<f64>())
    } else {
        0
    };
    let saved = cx
        .app
        .db
        .care_pet(
            user,
            &pet,
            [after.hunger, after.fun, after.energy, after.hygiene],
            progress.level,
            progress.xp,
            now,
            if action == Care::Feed { -FOOD_PRICE } else { found },
        )
        .await?;
    if saved != pinchora_db::PetCare::Saved {
        if let Err(error) = cx.app.cache.release_cooldown(&key).await {
            tracing::debug!(%error, pet = pet_id, "care cooldown not given back");
        }
        if saved == pinchora_db::PetCare::Poor {
            return cx
                .reply_ephemeral(t!(lang, "pet.care.food-price", price = FOOD_PRICE))
                .await;
        }
        let left = cx
            .app
            .db
            .pets(user)
            .await?
            .into_iter()
            .any(|p| p.id == pet.id && p.is_away(now));
        let note = if left {
            t!(lang, "pet.care.left-on-trip", name = pet.name)
        } else {
            t!(lang, "pet.care.changed", name = pet.name)
        };
        return cx.reply_ephemeral(note).await;
    }
    cx.app.quest(user, pinchora_game::quests::QuestKind::Care).await;
    cx.app.count(user, pinchora_db::Counter::PetCare, 1).await;
    let verb = match action {
        Care::Feed => t!(lang, "pet.care.verb.feed"),
        Care::Play => t!(lang, "pet.care.verb.play"),
        Care::Clean => t!(lang, "pet.care.verb.clean"),
        Care::Walk => t!(lang, "pet.care.verb.walk"),
    };
    let mut note = if pet.level < pets::MAX_LEVEL {
        t!(lang, "pet.care.note-xp", name = pet.name, verb = verb, xp = action.xp())
    } else {
        t!(lang, "pet.care.note", name = pet.name, verb = verb)
    };
    // Walks turn up a few shells on the beach; a higher level finds more.
    if action == Care::Walk {
        note.push_str(&t!(lang, "pet.care.found", found = found));
    }
    if progress.gained > 0 {
        note.push('\n');
        note.push_str(&level_up_note(lang, &pet.name, pet.level, progress.level));
    }
    let fresh = PetRow {
        level: progress.level,
        xp: progress.xp,
        hunger: after.hunger,
        fun: after.fun,
        energy: after.energy,
        hygiene: after.hygiene,
        needs_at: now,
        ..pet
    };
    let data = pet_card(&cx.app, lang, &fresh, Some(note)).await?;
    cx.update(data).await?;
    customize::announce(cx).await;
    Ok(())
}

/// `pc:wake:<pet>`: wakes a sleeping pet with the day's free wake-up, or for a small fee.
async fn wake(cx: &Cx, pet_id: i64) -> anyhow::Result<()> {
    let lang = cx.lang();
    let user = cx.author()?.id.get();
    let Some(pet) = cx.app.db.pets(user).await?.into_iter().find(|p| p.id == pet_id) else {
        return cx.reply_ephemeral(t!(lang, "pet.care.not-yours")).await;
    };
    let now = cx.app.now();
    if status(&pet, now) != PetStatus::Asleep {
        return cx.reply_ephemeral(t!(lang, "pet.wake.awake", name = pet.name)).await;
    }
    let needs = [WAKE_NEEDS.hunger, WAKE_NEEDS.fun, WAKE_NEEDS.energy, WAKE_NEEDS.hygiene];
    let outcome = cx
        .app
        .db
        .wake_pet(user, pet.id, pet.needs_at, needs, now, day_of(now), WAKE_FEE)
        .await?;
    let note = match outcome {
        Wake::Woke { free: true, .. } => t!(lang, "pet.wake.free", name = pet.name),
        Wake::Woke { balance, .. } => t!(
            lang,
            "pet.wake.paid",
            name = pet.name,
            fee = WAKE_FEE,
            balance = thousands_in(lang, balance)
        ),
        Wake::Stale => {
            return cx.reply_ephemeral(t!(lang, "pet.wake.awake", name = pet.name)).await;
        }
        Wake::Poor { balance } => {
            return cx
                .reply_ephemeral(t!(
                    lang,
                    "pet.wake.poor",
                    fee = WAKE_FEE,
                    balance = thousands_in(lang, balance),
                    name = pet.name
                ))
                .await;
        }
    };
    let fresh = PetRow {
        hunger: WAKE_NEEDS.hunger,
        fun: WAKE_NEEDS.fun,
        energy: WAKE_NEEDS.energy,
        hygiene: WAKE_NEEDS.hygiene,
        needs_at: now,
        ..pet
    };
    let data = pet_card(&cx.app, lang, &fresh, Some(note)).await?;
    cx.update(data).await
}
