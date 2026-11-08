use std::{borrow::Cow, str::Split, time::Duration};

use chrono::{DateTime, Utc};
use pinchora_core::{i18n::Lang, server_stats::decimal_in, t};
use pinchora_db::{PetRow, Wake};
use pinchora_game::{
    Content, Rarity,
    lang::lower,
    pets::{self, CAPSULE_PRICE, Care, DUPLICATE_REFUND, FOOD_PRICE, Needs, WAKE_FEE, WAKE_NEEDS},
    trips,
};
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

use crate::{
    App, Cx,
    commands::{customize, remind},
    game::scene::Sprites,
    util::{BRAND, WARN, thousands_in},
};

mod nursery;
mod trip;

pub(crate) use nursery::on_nursery;
pub(crate) use trip::on_trip;

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
    Trip(trip::PetTrip),
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
            Self::View(_) => match cx.app.db.active_pet(user).await? {
                Some(pet) => {
                    let data = pet_card(&cx.app, lang, &pet, None).await?;
                    cx.reply(data).await
                }
                None => cx.reply_ephemeral(t!(lang, "pet.no-pet")).await,
            },
            Self::Adopt(_) => adopt_menu(cx, user).await,
            Self::Capsule(_) => capsule(cx, user).await,
            Self::List(_) => nursery::open(cx, user).await,
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

// /Pet state

/// A pet's needs as of `now`. They drift from `needs_at`; a pet on a trip has its `needs_at` in
/// the future, so they stay as they were when it left.
pub fn current_needs(pet: &PetRow, now: DateTime<Utc>) -> Needs {
    let hours = (now - pet.needs_at).num_seconds().max(0) as f32 / 3600.0;
    Needs {
        hunger: pet.hunger,
        fun: pet.fun,
        energy: pet.energy,
        hygiene: pet.hygiene,
    }
    .after(hours)
}

/// Whether the pet fell asleep: nobody looked after it from the day its hunger, fun and hygiene
/// all ran out until a day later. Worked out from the stored needs, so no job has to watch for it.
pub fn is_asleep(pet: &PetRow, now: DateTime<Utc>) -> bool {
    if pet.is_away(now) {
        return false;
    }
    let stored = Needs {
        hunger: pet.hunger,
        fun: pet.fun,
        energy: pet.energy,
        hygiene: pet.hygiene,
    };
    (now - pet.needs_at).num_seconds() as f32 / 3600.0 >= stored.sleeps_after_hours()
}

fn status(pet: &PetRow, now: DateTime<Utc>) -> PetStatus {
    if pet.is_away(now) {
        PetStatus::Away
    } else if is_asleep(pet, now) {
        PetStatus::Asleep
    } else if pet.has_souvenir(now) {
        PetStatus::Back
    } else {
        PetStatus::Awake
    }
}

/// "2 h 5 min", "25 min", "under a minute".
fn span(secs: i64, lang: Lang) -> String {
    let minutes = (secs.max(0) + 59) / 60;
    match (minutes / 60, minutes % 60) {
        (0, 0) => t!(lang, "pet.span.under-minute"),
        (0, m) => t!(lang, "pet.span.minutes", m = m),
        (h, 0) => t!(lang, "pet.span.hours", h = h),
        (h, m) => t!(lang, "pet.span.hours-minutes", h = h, m = m),
    }
}

/// The name of the place a pet is away at or came back from.
fn place_of(pet: &PetRow, lang: Lang) -> String {
    match pet.trip_dest.as_deref().and_then(trips::get) {
        Some(trip) => Content::get().name(&trip.id, lang).into_owned(),
        None => t!(lang, "pet.somewhere"),
    }
}

/// How long until `ends`, as `span` writes it; empty without an end.
fn left_in(ends: Option<DateTime<Utc>>, now: DateTime<Utc>, lang: Lang) -> String {
    ends.map_or_else(String::new, |ends| span((ends - now).num_seconds(), lang))
}

/// The day number of `now`, for the daily free wake-up.
fn day_of(now: DateTime<Utc>) -> i32 {
    now.timestamp().div_euclid(86_400) as i32
}

/// What a level-up says: the new level, and any trip it opens.
fn level_up_note(lang: Lang, name: &str, from: i32, to: i32) -> String {
    let mut note = t!(lang, "pet.level-up", name = name, level = to);
    for trip in trips::all()
        .iter()
        .filter(|t| t.unlock_level > from && t.unlock_level <= to)
    {
        note.push('\n');
        note.push_str(&t!(
            lang,
            "pet.trip-unlocked",
            trip = Content::get().name(&trip.id, lang)
        ));
    }
    note
}

// /Components

fn button_if(custom_id: String, label: &str, style: ButtonStyle, disabled: bool) -> Component {
    Component::Button(Button {
        custom_id: Some(custom_id),
        disabled,
        emoji: None,
        id: None,
        label: Some(label.to_owned()),
        sku_id: None,
        style,
        url: None,
    })
}

fn button(custom_id: String, label: &str, style: ButtonStyle) -> Component {
    button_if(custom_id, label, style, false)
}

fn row(components: Vec<Component>) -> Component {
    Component::ActionRow(ActionRow { id: None, components })
}

fn select(custom_id: String, placeholder: &str, options: Vec<SelectMenuOption>) -> Component {
    row(vec![Component::SelectMenu(SelectMenu {
        channel_types: None,
        custom_id,
        default_values: None,
        disabled: false,
        id: None,
        kind: SelectMenuType::Text,
        max_values: Some(1),
        min_values: Some(1),
        options: Some(options),
        placeholder: Some(placeholder.to_owned()),
        required: None,
    })])
}

// /Cards

/// A species' sprite scaled up with nearest-neighbour to fit a box, as PNG, with its size.
fn scaled_sprite(sprites: &Sprites, species: &str, bx: u32, by: u32) -> anyhow::Result<(Vec<u8>, u32, u32)> {
    let sprite = sprites.pet(species);
    let factor = (bx / sprite.width().max(1))
        .min(by / sprite.height().max(1))
        .clamp(1, 16);
    let (w, h) = (sprite.width() * factor, sprite.height() * factor);
    let big = image::imageops::resize(sprite.as_ref(), w, h, image::imageops::FilterType::Nearest);
    let mut buf = std::io::Cursor::new(Vec::new());
    big.write_to(&mut buf, image::ImageFormat::Png)?;
    Ok((buf.into_inner(), w, h))
}

/// The pet's status card.
pub async fn pet_png(app: &App, lang: Lang, pet: &PetRow) -> anyhow::Result<Vec<u8>> {
    let content = Content::get();
    let species = content.pet(&pet.species);
    let now = app.now();
    let needs = current_needs(pet, now);
    let state = status(pet, now);
    let card = PetCard {
        lang,
        sprite_png: Vec::new(),
        sprite_w: 0,
        sprite_h: 0,
        name: pet.name.clone(),
        species: content.name(&pet.species, lang).into_owned(),
        rarity: lang.upper(species.map_or(Rarity::Common, |s| s.rarity).label_in(lang)),
        rarity_color: species.map_or(0x8fa0d8, |s| s.rarity.color()),
        level: pet.level,
        xp: pet.xp,
        xp_needed: pets::xp_to_next(pet.level),
        needs: [needs.hunger, needs.fun, needs.energy, needs.hygiene],
        mood: lang.upper(&t!(lang, &format!("pet.mood.{}", needs.mood()))),
        personality: content
            .description(&pet.species, lang)
            .map_or_else(String::new, Cow::into_owned),
        status: state,
        place: if pet.trip_dest.is_some() {
            place_of(pet, lang)
        } else {
            String::new()
        },
        back: pet
            .trip_ends_at
            .filter(|_| state == PetStatus::Away)
            .map_or_else(String::new, |ends| {
                t!(lang, "pet.back-in", span = span((ends - now).num_seconds(), lang))
            }),
        theme: Theme::reef(),
    };
    let sprites = app.sprites.clone();
    let species_id = pet.species.clone();
    app.render(move |r| {
        let (png, w, h) = scaled_sprite(&sprites, &species_id, PetCard::SPRITE_BOX, PetCard::SPRITE_BOX)?;
        r.pet_card(&PetCard {
            sprite_png: png,
            sprite_w: w,
            sprite_h: h,
            ..card
        })
    })
    .await
}

fn pet_title(lang: Lang, pet: &PetRow) -> String {
    let content = Content::get();
    let species = match content.pet(&pet.species) {
        Some(species) => content.name(&species.id, lang),
        None => Cow::Owned(t!(lang, "pet.species-fallback")),
    };
    t!(lang, "pet.title", name = pet.name, species = species)
}

/// What the card says under its title when the caller has nothing of its own to say.
fn status_note(lang: Lang, pet: &PetRow, now: DateTime<Utc>) -> Option<String> {
    match status(pet, now) {
        PetStatus::Awake => None,
        PetStatus::Asleep => Some(t!(lang, "pet.status.asleep", name = pet.name)),
        PetStatus::Away => Some(t!(
            lang,
            "pet.status.away",
            name = pet.name,
            place = place_of(pet, lang),
            span = left_in(pet.trip_ends_at, now, lang)
        )),
        PetStatus::Back => Some(t!(
            lang,
            "pet.status.back",
            name = pet.name,
            place = place_of(pet, lang)
        )),
    }
}

/// The card for a pet with its buttons: care, and by state the claim, wake-up and trip buttons.
async fn pet_card(
    app: &App,
    lang: Lang,
    pet: &PetRow,
    note: Option<String>,
) -> anyhow::Result<InteractionResponseData> {
    // Showing a pet is how its state changes reach the reminders about it.
    remind::refresh_pet(app, pet, lang).await;
    let png = pet_png(app, lang, pet).await?;
    let now = app.now();
    let state = status(pet, now);
    let mut embed = EmbedBuilder::new().color(BRAND).title(pet_title(lang, pet));
    if let Some(text) = note.or_else(|| status_note(lang, pet, now)) {
        embed = embed.description(text);
    }
    let id = pet.id;
    // A pet that is asleep or away cannot be looked after.
    let still = matches!(state, PetStatus::Asleep | PetStatus::Away);
    let care = row(vec![
        button_if(
            format!("pc:feed:{id}"),
            &t!(lang, "pet.button.feed", price = FOOD_PRICE),
            ButtonStyle::Success,
            still,
        ),
        button_if(
            format!("pc:play:{id}"),
            &t!(lang, "pet.button.play"),
            ButtonStyle::Primary,
            still,
        ),
        button_if(
            format!("pc:walk:{id}"),
            &t!(lang, "pet.button.walk"),
            ButtonStyle::Primary,
            still,
        ),
        button_if(
            format!("pc:clean:{id}"),
            &t!(lang, "pet.button.bath"),
            ButtonStyle::Secondary,
            still,
        ),
    ]);
    let mut more = Vec::new();
    match state {
        PetStatus::Away => {
            let left = pet.trip_ends_at.map_or(0, |ends| (ends - now).num_seconds());
            more.push(button_if(
                format!("py:x:{id}"),
                &t!(lang, "pet.button.back-in", span = span(left, lang)),
                ButtonStyle::Secondary,
                true,
            ));
        }
        PetStatus::Asleep => {
            let free = app.db.free_wake_ready(pet.owner_id as u64, day_of(now)).await?;
            let label = if free {
                t!(lang, "pet.button.wake-free")
            } else {
                t!(lang, "pet.button.wake-fee", fee = WAKE_FEE)
            };
            more.push(button(format!("pc:wake:{id}"), &label, ButtonStyle::Primary));
        }
        PetStatus::Awake | PetStatus::Back => {}
    }
    if pet.has_souvenir(now) {
        more.push(button(
            format!("py:cl:{id}"),
            &t!(lang, "pet.button.claim"),
            ButtonStyle::Success,
        ));
    } else if state == PetStatus::Awake {
        more.push(button(
            format!("py:o:{id}"),
            &t!(lang, "pet.button.trip"),
            ButtonStyle::Primary,
        ));
    }
    if !pet.active {
        more.push(button(
            format!("pn:a:{id}"),
            &t!(lang, "pet.button.make-active"),
            ButtonStyle::Secondary,
        ));
    }
    more.push(button(
        format!("pn:f:{id}"),
        &t!(lang, "pet.button.nursery"),
        ButtonStyle::Secondary,
    ));
    Ok(Cx::card(embed, "pet.png", png, vec![care, row(more)], false))
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
    if action == Care::Feed && cx.app.db.spend_shells(user, FOOD_PRICE, "pet_food").await?.is_none() {
        return cx
            .reply_ephemeral(t!(lang, "pet.care.food-price", price = FOOD_PRICE))
            .await;
    }
    let needs = current_needs(&pet, now);
    let Some(after) = action.apply(needs) else {
        return cx
            .reply_ephemeral(t!(lang, "pet.care.too-tired", name = pet.name))
            .await;
    };
    let progress = pets::add_xp(pet.level, pet.xp, action.xp());
    let saved = cx
        .app
        .db
        .save_pet(
            user,
            &pet,
            [after.hunger, after.fun, after.energy, after.hygiene],
            progress.level,
            progress.xp,
            now,
        )
        .await?;
    if !saved {
        // The pet changed between the read and the write (a trip started, a claim paid, another
        // click got in first); nothing was cared for, so the food is refunded and the action's
        // cooldown, started above, does not count either.
        if action == Care::Feed {
            cx.app.db.earn_shells(user, FOOD_PRICE, "pet_food_refund").await?;
        }
        if let Err(error) = cx.app.cache.release_cooldown(&key).await {
            tracing::debug!(%error, pet = pet_id, "care cooldown not given back");
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
        let found = pets::walk_shells(pet.level, rand::random::<f64>());
        cx.app.db.earn_shells(user, found, "pet_walk").await?;
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

// /Adopting

async fn adopt_menu(cx: &Cx, user: u64) -> anyhow::Result<()> {
    let lang = cx.lang();
    if !cx.app.db.pets(user).await?.is_empty() {
        return cx.reply_ephemeral(t!(lang, "pet.adopt.have-one")).await;
    }
    let content = Content::get();
    let options: Vec<SelectMenuOption> = content
        .pets
        .iter()
        .filter(|p| p.starter)
        .map(|p| SelectMenuOption {
            default: false,
            description: Some(
                content
                    .description(&p.id, lang)
                    .unwrap_or_default()
                    .chars()
                    .take(100)
                    .collect(),
            ),
            emoji: None,
            label: content.name(&p.id, lang).into_owned(),
            value: p.id.clone(),
        })
        .collect();
    if options.is_empty() {
        return cx.reply_ephemeral(t!(lang, "pet.adopt.none-ready")).await;
    }
    let embed = EmbedBuilder::new()
        .color(BRAND)
        .title(t!(lang, "pet.adopt.title"))
        .description(t!(lang, "pet.adopt.description"))
        .build();
    cx.reply(InteractionResponseData {
        embeds: Some(vec![embed]),
        components: Some(vec![select(
            "pa".to_owned(),
            &t!(lang, "pet.adopt.placeholder"),
            options,
        )]),
        flags: Some(MessageFlags::EPHEMERAL),
        ..Default::default()
    })
    .await
}

/// `pa` select: adopt the chosen starter (only when the player has no pets).
pub(crate) async fn on_adopt(cx: &Cx, value: Option<&str>) -> anyhow::Result<()> {
    let lang = cx.lang();
    let user = cx.author()?.id.get();
    let content = Content::get();
    let Some(species) = value.and_then(|v| content.pet(v)).filter(|p| p.starter) else {
        return cx.reply_ephemeral(t!(lang, "pet.adopt.pick-starter")).await;
    };
    if !cx.app.db.pets(user).await?.is_empty() {
        return cx.reply_ephemeral(t!(lang, "pet.adopt.already-adopted")).await;
    }
    // A new pet is named after its species, in the adopter's language; it is theirs to rename.
    let name = content.name(&species.id, lang);
    let pet = cx.app.db.adopt(user, &species.id, &name).await?;
    let data = pet_card(&cx.app, lang, &pet, Some(t!(lang, "pet.adopt.welcome", name = name))).await?;
    cx.update(data).await?;
    customize::announce(cx).await;
    Ok(())
}

async fn capsule(cx: &Cx, user: u64) -> anyhow::Result<()> {
    let lang = cx.lang();
    let content = Content::get();
    let Some(balance) = cx.app.db.spend_shells(user, CAPSULE_PRICE, "pet_capsule").await? else {
        let odds = pets::odds(content)
            .into_iter()
            .map(|(r, p)| format!("{} {}%", r.label_in(lang), decimal_in(lang, p * 100.0, 1)))
            .collect::<Vec<_>>()
            .join(" · ");
        return cx
            .reply_ephemeral(t!(
                lang,
                "pet.capsule.odds",
                price = thousands_in(lang, CAPSULE_PRICE),
                odds = odds
            ))
            .await;
    };
    let Some(species) = pets::hatch(content, rand::random::<f64>()) else {
        cx.app.db.earn_shells(user, CAPSULE_PRICE, "pet_capsule_refund").await?;
        return cx.reply_ephemeral(t!(lang, "pet.capsule.restocking")).await;
    };
    let name = content.name(&species.id, lang);
    if cx.app.db.has_species(user, &species.id).await? {
        let balance = cx.app.db.earn_shells(user, DUPLICATE_REFUND, "pet_duplicate").await?;
        let embed = EmbedBuilder::new()
            .color(WARN)
            .title(t!(
                lang,
                "pet.capsule.duplicate-title",
                rarity = lower(lang, species.rarity.label_in(lang)),
                name = name
            ))
            .description(t!(
                lang,
                "pet.capsule.duplicate",
                refund = DUPLICATE_REFUND,
                balance = thousands_in(lang, balance)
            ))
            .build();
        return cx.reply_embed(embed).await;
    }
    let pet = cx.app.db.adopt(user, &species.id, &name).await?;
    let note = t!(
        lang,
        "pet.capsule.opened",
        rarity = species.rarity.label_in(lang),
        name = name,
        balance = thousands_in(lang, balance)
    );
    let data = pet_card(&cx.app, lang, &pet, Some(note)).await?;
    cx.reply(data).await?;
    customize::announce(cx).await;
    Ok(())
}

/// `ps` select: switch the active pet. Kept for nursery lists posted before the grid card.
pub(crate) async fn on_switch(cx: &Cx, value: Option<&str>) -> anyhow::Result<()> {
    let lang = cx.lang();
    let user = cx.author()?.id.get();
    let pet_id: i64 = value.and_then(|v| v.parse().ok()).unwrap_or(0);
    if cx.app.db.set_active_pet(user, pet_id).await? {
        cx.update(InteractionResponseData {
            content: Some(t!(lang, "pet.switch.changed")),
            embeds: Some(Vec::new()),
            components: Some(Vec::new()),
            ..Default::default()
        })
        .await
    } else {
        cx.reply_ephemeral(t!(lang, "pet.switch.not-yours")).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pet(hunger: i16, fun: i16, hygiene: i16, needs_at: DateTime<Utc>) -> PetRow {
        PetRow {
            id: 1,
            owner_id: 1,
            species: "crumbclaw_crab".to_owned(),
            name: "Pip".to_owned(),
            level: 1,
            xp: 0,
            hunger,
            fun,
            energy: 100,
            hygiene,
            needs_at,
            active: true,
            trip_dest: None,
            trip_ends_at: None,
        }
    }

    #[test]
    fn a_pet_sleeps_a_day_after_everything_ran_out() {
        let t0 = Utc::now();
        let hours = |h: i64| t0 + chrono::Duration::hours(h);
        // 80 hunger lasts 20 h, 80 fun 16 h and 80 hygiene 32 h: all dry at 32 h, asleep at 56 h.
        let tended = pet(80, 80, 80, t0);
        assert_eq!(status(&tended, hours(1)), PetStatus::Awake);
        assert_eq!(status(&tended, hours(55)), PetStatus::Awake);
        assert_eq!(status(&tended, hours(57)), PetStatus::Asleep);
        // Already dry when stored: a day later.
        let dry = pet(0, 0, 0, t0);
        assert_eq!(status(&dry, hours(23)), PetStatus::Awake);
        assert_eq!(status(&dry, hours(25)), PetStatus::Asleep);
        // One need that was looked after keeps it up.
        assert_eq!(status(&pet(0, 0, 40, t0), hours(24 + 15)), PetStatus::Awake);
    }

    #[test]
    fn a_pet_on_a_trip_keeps_its_needs_and_comes_back_to_a_souvenir() {
        let t0 = Utc::now();
        let hours = |h: i64| t0 + chrono::Duration::hours(h);
        let mut away = pet(40, 40, 40, hours(4));
        away.trip_dest = Some("kelp_forest_library".to_owned());
        away.trip_ends_at = Some(hours(4));
        // Mid-trip the needs are frozen and it cannot sleep; there is no souvenir yet.
        assert_eq!(status(&away, hours(1)), PetStatus::Away);
        assert_eq!(current_needs(&away, hours(1)).hunger, 40);
        assert_eq!(current_needs(&away, hours(4)).hunger, 40);
        // Back: needs drift from the return, and the souvenir waits.
        assert_eq!(status(&away, hours(5)), PetStatus::Back);
        assert_eq!(current_needs(&away, hours(5)).hunger, 36);
        assert!(away.has_souvenir(hours(5)) && !away.is_away(hours(5)));
        assert_eq!(place_of(&away, Lang::En), "Kelp Forest Library");
    }

    #[test]
    fn spans_read_naturally() {
        assert_eq!(span(0, Lang::En), "under a minute");
        assert_eq!(span(1, Lang::En), "1 min");
        assert_eq!(span(25 * 60, Lang::En), "25 min");
        assert_eq!(span(3600, Lang::En), "1 h");
        assert_eq!(span(2 * 3600 + 5 * 60, Lang::En), "2 h 5 min");
        assert_eq!(span(-5, Lang::En), "under a minute");
        assert_eq!(span(2 * 3600 + 5 * 60, Lang::Tr), "2 sa 5 dk");
    }

    #[test]
    fn a_level_up_names_the_trips_it_opens() {
        let note = level_up_note(Lang::En, "Pip", 3, 4);
        assert!(
            note.contains("reached level 4") && note.contains("Driftwood Cove"),
            "{note}"
        );
        let jump = level_up_note(Lang::En, "Pip", 1, 10);
        assert!(jump.contains("Driftwood Cove") && jump.contains("Sandcastle Shallows"));
        assert!(jump.contains("Kelp Forest Library"));
        assert!(!jump.contains("Lantern Lagoon"));
        assert!(!level_up_note(Lang::En, "Pip", 5, 6).contains("unlocked"));
    }
}
