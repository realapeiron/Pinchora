use std::{str::Split, time::Duration};

use chrono::{DateTime, Utc};
use pinchora_db::{PetRow, Wake};
use pinchora_game::{
    Content,
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
    util::{BRAND, WARN, thousands},
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
        match self {
            Self::View(_) => match cx.app.db.active_pet(user).await? {
                Some(pet) => {
                    let data = pet_card(&cx.app, &pet, None).await?;
                    cx.reply(data).await
                }
                None => {
                    cx.reply_ephemeral("You don't have a pet yet. Get one with `/pet adopt`.")
                        .await
                }
            },
            Self::Adopt(_) => adopt_menu(cx, user).await,
            Self::Capsule(_) => capsule(cx, user).await,
            Self::List(_) => nursery::open(cx, user).await,
            Self::Rename(c) => {
                let Some(pet) = cx.app.db.active_pet(user).await? else {
                    return cx.reply_ephemeral("You don't have a pet yet.").await;
                };
                let name: String = c.name.split_whitespace().collect::<Vec<_>>().join(" ");
                cx.app.db.rename_pet(user, pet.id, &name).await?;
                cx.reply_ephemeral(format!("Your pet is now called **{name}**.")).await
            }
            Self::Trip(trip) => trip.run(cx, user).await,
        }
    }
}

// /Pet state

/// A pet's needs as of `now`. They drift from `needs_at`; a pet on a trip has its `needs_at` in
/// the future, so they stay as they were when it left.
fn current_needs(pet: &PetRow, now: DateTime<Utc>) -> Needs {
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
pub(crate) fn is_asleep(pet: &PetRow, now: DateTime<Utc>) -> bool {
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
fn span(secs: i64) -> String {
    let minutes = (secs.max(0) + 59) / 60;
    match (minutes / 60, minutes % 60) {
        (0, 0) => "under a minute".to_owned(),
        (0, m) => format!("{m} min"),
        (h, 0) => format!("{h} h"),
        (h, m) => format!("{h} h {m} min"),
    }
}

/// The name of the place a pet is away at or came back from.
fn place_of(pet: &PetRow) -> &'static str {
    pet.trip_dest
        .as_deref()
        .and_then(trips::get)
        .map_or("somewhere nice", |t| t.name.as_str())
}

/// The day number of `now`, for the daily free wake-up.
fn day_of(now: DateTime<Utc>) -> i32 {
    now.timestamp().div_euclid(86_400) as i32
}

/// What a level-up says: the new level, and any trip it opens.
fn level_up_note(name: &str, from: i32, to: i32) -> String {
    let mut note = format!("🎉 **{name} reached level {to}!**");
    for trip in trips::all()
        .iter()
        .filter(|t| t.unlock_level > from && t.unlock_level <= to)
    {
        note.push_str(&format!("\nNew trip unlocked: **{}**.", trip.name));
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
async fn pet_png(app: &App, pet: &PetRow) -> anyhow::Result<Vec<u8>> {
    let content = Content::get();
    let species = content.pet(&pet.species);
    let now = app.now();
    let needs = current_needs(pet, now);
    let state = status(pet, now);
    let card = PetCard {
        sprite_png: Vec::new(),
        sprite_w: 0,
        sprite_h: 0,
        name: pet.name.clone(),
        species: species.map_or_else(|| pet.species.clone(), |s| s.name.clone()),
        rarity: species.map_or("common", |s| s.rarity.label()).to_owned(),
        rarity_color: species.map_or(0x8fa0d8, |s| s.rarity.color()),
        level: pet.level,
        xp: pet.xp,
        xp_needed: pets::xp_to_next(pet.level),
        needs: [needs.hunger, needs.fun, needs.energy, needs.hygiene],
        mood: needs.mood().to_owned(),
        personality: species.map_or_else(String::new, |s| s.personality.clone()),
        status: state,
        place: if pet.trip_dest.is_some() {
            place_of(pet).to_owned()
        } else {
            String::new()
        },
        back: pet
            .trip_ends_at
            .filter(|_| state == PetStatus::Away)
            .map_or_else(String::new, |ends| format!("in {}", span((ends - now).num_seconds()))),
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

fn pet_title(pet: &PetRow) -> String {
    let species = Content::get().pet(&pet.species);
    format!("{} the {}", pet.name, species.map_or("pet", |s| s.name.as_str()))
}

/// What the card says under its title when the caller has nothing of its own to say.
fn status_note(pet: &PetRow, now: DateTime<Utc>) -> Option<String> {
    match status(pet, now) {
        PetStatus::Awake => None,
        PetStatus::Asleep => Some(format!(
            "**{}** has dozed off after a long time with nothing to eat, play with or wash off. \
             It earns no XP and stays home until you wake it up.",
            pet.name
        )),
        PetStatus::Away => Some(format!(
            "**{}** is away at **{}**, back in {}. It can't be looked after until then.",
            pet.name,
            place_of(pet),
            pet.trip_ends_at
                .map_or_else(String::new, |ends| span((ends - now).num_seconds()))
        )),
        PetStatus::Back => Some(format!(
            "**{}** is back from **{}** with a souvenir. Press **Claim souvenir**!",
            pet.name,
            place_of(pet)
        )),
    }
}

/// The card for a pet with its buttons: care, and by state the claim, wake-up and trip buttons.
async fn pet_card(app: &App, pet: &PetRow, note: Option<String>) -> anyhow::Result<InteractionResponseData> {
    // Showing a pet is how its state changes reach the reminders about it.
    remind::refresh_pet(app, pet).await;
    let png = pet_png(app, pet).await?;
    let now = app.now();
    let state = status(pet, now);
    let mut embed = EmbedBuilder::new().color(BRAND).title(pet_title(pet));
    if let Some(text) = note.or_else(|| status_note(pet, now)) {
        embed = embed.description(text);
    }
    let id = pet.id;
    // A pet that is asleep or away cannot be looked after.
    let still = matches!(state, PetStatus::Asleep | PetStatus::Away);
    let care = row(vec![
        button_if(
            format!("pc:feed:{id}"),
            &format!("Feed ({FOOD_PRICE} shells)"),
            ButtonStyle::Success,
            still,
        ),
        button_if(format!("pc:play:{id}"), "Play", ButtonStyle::Primary, still),
        button_if(format!("pc:walk:{id}"), "Walk", ButtonStyle::Primary, still),
        button_if(format!("pc:clean:{id}"), "Bath", ButtonStyle::Secondary, still),
    ]);
    let mut more = Vec::new();
    match state {
        PetStatus::Away => {
            let left = pet.trip_ends_at.map_or(0, |ends| (ends - now).num_seconds());
            more.push(button_if(
                format!("py:x:{id}"),
                &format!("Back in {}", span(left)),
                ButtonStyle::Secondary,
                true,
            ));
        }
        PetStatus::Asleep => {
            let free = app.db.free_wake_ready(pet.owner_id as u64, day_of(now)).await?;
            let label = if free {
                "Wake up (free today)".to_owned()
            } else {
                format!("Wake up ({WAKE_FEE} shells)")
            };
            more.push(button(format!("pc:wake:{id}"), &label, ButtonStyle::Primary));
        }
        PetStatus::Awake | PetStatus::Back => {}
    }
    if pet.has_souvenir(now) {
        more.push(button(format!("py:cl:{id}"), "Claim souvenir", ButtonStyle::Success));
    } else if state == PetStatus::Awake {
        more.push(button(format!("py:o:{id}"), "Trip", ButtonStyle::Primary));
    }
    if !pet.active {
        more.push(button(format!("pn:a:{id}"), "Make active", ButtonStyle::Secondary));
    }
    more.push(button(format!("pn:f:{id}"), "Nursery", ButtonStyle::Secondary));
    Ok(Cx::card(embed, "pet.png", png, vec![care, row(more)], false))
}

// /Care

/// `pc:<action>:<pet>` buttons.
pub(crate) async fn on_care(cx: &Cx, mut args: Split<'_, char>) -> anyhow::Result<()> {
    let action = args.next();
    let pet_id: i64 = args.next().and_then(|v| v.parse().ok()).unwrap_or(0);
    let action = match action {
        Some("feed") => Care::Feed,
        Some("play") => Care::Play,
        Some("walk") => Care::Walk,
        Some("clean") => Care::Clean,
        Some("wake") => return wake(cx, pet_id).await,
        _ => {
            return cx
                .reply_ephemeral("That button doesn't do anything any more. Open your pet again with `/pet view`.")
                .await;
        }
    };
    let user = cx.author()?.id.get();
    let Some(pet) = cx.app.db.pets(user).await?.into_iter().find(|p| p.id == pet_id) else {
        return cx
            .reply_ephemeral("That's not your pet. Get your own with `/pet adopt`!")
            .await;
    };
    let now = cx.app.now();
    match status(&pet, now) {
        PetStatus::Away => {
            let left = pet.trip_ends_at.map_or(0, |ends| (ends - now).num_seconds());
            return cx
                .reply_ephemeral(format!(
                    "{} is away at {}, back in {}. It can't be looked after until then.",
                    pet.name,
                    place_of(&pet),
                    span(left)
                ))
                .await;
        }
        PetStatus::Asleep => {
            return cx
                .reply_ephemeral(format!("{} is fast asleep. Wake it up first.", pet.name))
                .await;
        }
        PetStatus::Awake | PetStatus::Back => {}
    }
    // The walk's cooldown belongs to the pet, so a player who deleted their data and adopted again is
    // held to the one their old pet was on.
    if action == Care::Walk
        && let Some(until) = cx.app.db.walk_locked_until(user, now).await?
    {
        return cx
            .reply_ephemeral(format!(
                "{} needs a break. Try again in {} min.",
                pet.name,
                (until - now).num_minutes() + 1
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
            .reply_ephemeral(format!(
                "{} needs a break. Try again in {} min.",
                pet.name,
                left.as_secs() / 60 + 1
            ))
            .await;
    }
    if action == Care::Feed && cx.app.db.spend_shells(user, FOOD_PRICE, "pet_food").await?.is_none() {
        return cx.reply_ephemeral(format!("Pet food costs {FOOD_PRICE} shells.")).await;
    }
    let needs = current_needs(&pet, now);
    let Some(after) = action.apply(needs) else {
        return cx
            .reply_ephemeral(format!("{} is too tired for that. Let it rest.", pet.name))
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
        // click got in first); nothing was cared for, so the food is refunded.
        if action == Care::Feed {
            cx.app.db.earn_shells(user, FOOD_PRICE, "pet_food_refund").await?;
        }
        let left = cx
            .app
            .db
            .pets(user)
            .await?
            .into_iter()
            .any(|p| p.id == pet.id && p.is_away(now));
        let note = if left {
            format!(
                "{} just left on a trip. It can't be looked after until it is back.",
                pet.name
            )
        } else {
            format!(
                "{} changed while you clicked, so nothing was done. Open it again with `/pet view`.",
                pet.name
            )
        };
        return cx.reply_ephemeral(note).await;
    }
    cx.app.quest(user, pinchora_game::quests::QuestKind::Care).await;
    cx.app.count(user, pinchora_db::Counter::PetCare, 1).await;
    let verb = match action {
        Care::Feed => "munches happily",
        Care::Play => "bounces around with you",
        Care::Clean => "is squeaky clean",
        Care::Walk => "trots along the shore with you",
    };
    let mut note = if pet.level < pets::MAX_LEVEL {
        format!("{} {verb}. +{} XP", pet.name, action.xp())
    } else {
        format!("{} {verb}.", pet.name)
    };
    // Walks turn up a few shells on the beach; a higher level finds more.
    if action == Care::Walk {
        let found = pets::walk_shells(pet.level, rand::random::<f64>());
        cx.app.db.earn_shells(user, found, "pet_walk").await?;
        note.push_str(&format!("\nFound **{found}** shells on the walk!"));
    }
    if progress.gained > 0 {
        note.push('\n');
        note.push_str(&level_up_note(&pet.name, pet.level, progress.level));
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
    let data = pet_card(&cx.app, &fresh, Some(note)).await?;
    cx.update(data).await?;
    customize::announce(cx).await;
    Ok(())
}

/// `pc:wake:<pet>`: wakes a sleeping pet with the day's free wake-up, or for a small fee.
async fn wake(cx: &Cx, pet_id: i64) -> anyhow::Result<()> {
    let user = cx.author()?.id.get();
    let Some(pet) = cx.app.db.pets(user).await?.into_iter().find(|p| p.id == pet_id) else {
        return cx
            .reply_ephemeral("That's not your pet. Get your own with `/pet adopt`!")
            .await;
    };
    let now = cx.app.now();
    if status(&pet, now) != PetStatus::Asleep {
        return cx.reply_ephemeral(format!("{} is already awake.", pet.name)).await;
    }
    let needs = [WAKE_NEEDS.hunger, WAKE_NEEDS.fun, WAKE_NEEDS.energy, WAKE_NEEDS.hygiene];
    let outcome = cx
        .app
        .db
        .wake_pet(user, pet.id, pet.needs_at, needs, now, day_of(now), WAKE_FEE)
        .await?;
    let note = match outcome {
        Wake::Woke { free: true, .. } => format!(
            "{} stretches, yawns and is awake again! That was your free wake-up for today.",
            pet.name
        ),
        Wake::Woke { balance, .. } => format!(
            "{} stretches, yawns and is awake again! The wake-up cost {WAKE_FEE} shells ({} left).",
            pet.name,
            thousands(balance)
        ),
        Wake::Stale => {
            return cx.reply_ephemeral(format!("{} is already awake.", pet.name)).await;
        }
        Wake::Poor { balance } => {
            return cx
                .reply_ephemeral(format!(
                    "Your free wake-up for today is used, and another costs {WAKE_FEE} shells (you have {}). \
                     Your free one comes back tomorrow, and {} can sleep as long as it likes.",
                    thousands(balance),
                    pet.name
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
    let data = pet_card(&cx.app, &fresh, Some(note)).await?;
    cx.update(data).await
}

// /Adopting

async fn adopt_menu(cx: &Cx, user: u64) -> anyhow::Result<()> {
    if !cx.app.db.pets(user).await?.is_empty() {
        return cx
            .reply_ephemeral("You already have a pet. More come from `/pet capsule`.")
            .await;
    }
    let content = Content::get();
    let options: Vec<SelectMenuOption> = content
        .pets
        .iter()
        .filter(|p| p.starter)
        .map(|p| SelectMenuOption {
            default: false,
            description: Some(p.personality.chars().take(100).collect()),
            emoji: None,
            label: p.name.clone(),
            value: p.id.clone(),
        })
        .collect();
    if options.is_empty() {
        return cx.reply_ephemeral("No pets are ready for adoption yet.").await;
    }
    let embed = EmbedBuilder::new()
        .color(BRAND)
        .title("Adopt a pet")
        .description("Pick your first companion. It will live in your house and follow you on visits.")
        .build();
    cx.reply(InteractionResponseData {
        embeds: Some(vec![embed]),
        components: Some(vec![select("pa".to_owned(), "Choose a starter pet…", options)]),
        flags: Some(MessageFlags::EPHEMERAL),
        ..Default::default()
    })
    .await
}

/// `pa` select: adopt the chosen starter (only when the player has no pets).
pub(crate) async fn on_adopt(cx: &Cx, value: Option<&str>) -> anyhow::Result<()> {
    let user = cx.author()?.id.get();
    let content = Content::get();
    let Some(species) = value.and_then(|v| content.pet(v)).filter(|p| p.starter) else {
        return cx.reply_ephemeral("Pick one of the starter pets.").await;
    };
    if !cx.app.db.pets(user).await?.is_empty() {
        return cx.reply_ephemeral("You already adopted a pet.").await;
    }
    let pet = cx.app.db.adopt(user, &species.id, &species.name).await?;
    let data = pet_card(
        &cx.app,
        &pet,
        Some(format!(
            "Welcome home, **{}**! Rename it with `/pet rename`.",
            species.name
        )),
    )
    .await?;
    cx.update(data).await?;
    customize::announce(cx).await;
    Ok(())
}

async fn capsule(cx: &Cx, user: u64) -> anyhow::Result<()> {
    let content = Content::get();
    let Some(balance) = cx.app.db.spend_shells(user, CAPSULE_PRICE, "pet_capsule").await? else {
        let odds = pets::odds(content)
            .into_iter()
            .map(|(r, p)| format!("{} {:.1}%", r.label(), p * 100.0))
            .collect::<Vec<_>>()
            .join(" · ");
        return cx
            .reply_ephemeral(format!(
                "A capsule costs {} shells.\nOdds: {odds}",
                thousands(CAPSULE_PRICE)
            ))
            .await;
    };
    let Some(species) = pets::hatch(content, rand::random::<f64>()) else {
        cx.app.db.earn_shells(user, CAPSULE_PRICE, "pet_capsule_refund").await?;
        return cx.reply_ephemeral("The capsules are still being restocked.").await;
    };
    if cx.app.db.has_species(user, &species.id).await? {
        let balance = cx.app.db.earn_shells(user, DUPLICATE_REFUND, "pet_duplicate").await?;
        let embed = EmbedBuilder::new()
            .color(WARN)
            .title(format!("A {} {}!", species.rarity.label().to_lowercase(), species.name))
            .description(format!(
                "You already have one, so it went to a friend's house and left you {} shells. Balance: {}.",
                DUPLICATE_REFUND,
                thousands(balance)
            ))
            .build();
        return cx.reply_embed(embed).await;
    }
    let pet = cx.app.db.adopt(user, &species.id, &species.name).await?;
    let note = format!(
        "✨ The capsule cracks open… a **{}** {}! ({} shells left)\nMake it your active pet from `/pet list`.",
        species.rarity.label(),
        species.name,
        thousands(balance)
    );
    let data = pet_card(&cx.app, &pet, Some(note)).await?;
    cx.reply(data).await?;
    customize::announce(cx).await;
    Ok(())
}

/// `ps` select: switch the active pet. Kept for nursery lists posted before the grid card.
pub(crate) async fn on_switch(cx: &Cx, value: Option<&str>) -> anyhow::Result<()> {
    let user = cx.author()?.id.get();
    let pet_id: i64 = value.and_then(|v| v.parse().ok()).unwrap_or(0);
    if cx.app.db.set_active_pet(user, pet_id).await? {
        cx.update(InteractionResponseData {
            content: Some("Active pet changed. It will greet visitors in your house.".to_owned()),
            embeds: Some(Vec::new()),
            components: Some(Vec::new()),
            ..Default::default()
        })
        .await
    } else {
        cx.reply_ephemeral("That pet isn't yours.").await
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
        assert_eq!(place_of(&away), "Kelp Forest Library");
    }

    #[test]
    fn spans_read_naturally() {
        assert_eq!(span(0), "under a minute");
        assert_eq!(span(1), "1 min");
        assert_eq!(span(25 * 60), "25 min");
        assert_eq!(span(3600), "1 h");
        assert_eq!(span(2 * 3600 + 5 * 60), "2 h 5 min");
        assert_eq!(span(-5), "under a minute");
    }

    #[test]
    fn a_level_up_names_the_trips_it_opens() {
        let note = level_up_note("Pip", 3, 4);
        assert!(
            note.contains("reached level 4") && note.contains("Driftwood Cove"),
            "{note}"
        );
        let jump = level_up_note("Pip", 1, 10);
        assert!(jump.contains("Driftwood Cove") && jump.contains("Sandcastle Shallows"));
        assert!(jump.contains("Kelp Forest Library"));
        assert!(!jump.contains("Lantern Lagoon"));
        assert!(!level_up_note("Pip", 5, 6).contains("unlocked"));
    }
}
