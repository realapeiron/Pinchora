//! Field trips: `/pet trip go` sends a pet somewhere cosy for a few hours, and `/pet trip claim`
//! (or the Claim button) pays what it found. A trip is stored as an end time, so it runs on
//! through restarts and a claim pays exactly once.

use std::str::Split;

use chrono::Duration;
use pinchora_db::{PetRow, TripClaim, TripPayout};
use pinchora_game::{
    Content, pets,
    trips::{self, Haul, Rolls, Trip},
};
use twilight_interactions::command::{CommandModel, CommandOption, CreateCommand, CreateOption};
use twilight_model::{
    channel::message::component::{ButtonStyle, SelectMenuOption},
    http::interaction::InteractionResponseData,
};
use twilight_util::builder::embed::{EmbedBuilder, EmbedFieldBuilder};

use super::{
    button, current_needs, is_asleep, level_up_note, pet_card, pet_png, pet_title, place_of, row, select, span,
};
use crate::{
    App, Cx,
    commands::customize,
    util::{BRAND, thousands},
};

// /Commands

#[derive(CommandModel, CreateCommand)]
#[command(name = "trip", desc = "Send a pet on a field trip, or collect what it found")]
pub enum PetTrip {
    #[command(name = "go")]
    Go(PetTripGo),
    #[command(name = "claim")]
    Claim(PetTripClaim),
}

#[derive(CommandModel, CreateCommand)]
#[command(
    name = "go",
    desc = "Send a pet away for 1, 4 or 8 hours; it brings back shells and sometimes furniture"
)]
pub struct PetTripGo {
    /// Where to
    destination: Place,
    /// Which pet, by name (default: your active pet)
    #[command(max_length = 20)]
    pet: Option<String>,
}

#[derive(CommandModel, CreateCommand)]
#[command(
    name = "claim",
    desc = "Collect the souvenirs of pets that are back, or see who is still away"
)]
pub struct PetTripClaim;

/// The destinations as the command offers them: a choice per place in `content/pet_trips.json`
/// (the test below keeps the two in step).
#[derive(CommandOption, CreateOption, Clone, Copy, Debug, PartialEq, Eq)]
pub enum Place {
    #[option(name = "Tidepool Meadow · 1 h · level 1", value = "tidepool_meadow")]
    TidepoolMeadow,
    #[option(name = "Driftwood Cove · 1 h · level 4", value = "driftwood_cove")]
    DriftwoodCove,
    #[option(name = "Sandcastle Shallows · 1 h · level 8", value = "sandcastle_shallows")]
    SandcastleShallows,
    #[option(name = "Kelp Forest Library · 4 h · level 10", value = "kelp_forest_library")]
    KelpForestLibrary,
    #[option(name = "Lantern Lagoon · 4 h · level 14", value = "lantern_lagoon")]
    LanternLagoon,
    #[option(name = "Coral Bakery Reef · 4 h · level 18", value = "coral_bakery_reef")]
    CoralBakeryReef,
    #[option(name = "Moonpearl Isles · 8 h · level 22", value = "moonpearl_isles")]
    MoonpearlIsles,
    #[option(name = "Starfall Sandbar · 8 h · level 26", value = "starfall_sandbar")]
    StarfallSandbar,
}

impl Place {
    #[cfg(test)]
    const ALL: [Place; 8] = [
        Self::TidepoolMeadow,
        Self::DriftwoodCove,
        Self::SandcastleShallows,
        Self::KelpForestLibrary,
        Self::LanternLagoon,
        Self::CoralBakeryReef,
        Self::MoonpearlIsles,
        Self::StarfallSandbar,
    ];

    /// The trip's id in `content/pet_trips.json`.
    fn id(self) -> &'static str {
        match self {
            Self::TidepoolMeadow => "tidepool_meadow",
            Self::DriftwoodCove => "driftwood_cove",
            Self::SandcastleShallows => "sandcastle_shallows",
            Self::KelpForestLibrary => "kelp_forest_library",
            Self::LanternLagoon => "lantern_lagoon",
            Self::CoralBakeryReef => "coral_bakery_reef",
            Self::MoonpearlIsles => "moonpearl_isles",
            Self::StarfallSandbar => "starfall_sandbar",
        }
    }
}

impl PetTrip {
    pub(super) async fn run(self, cx: &Cx, user: u64) -> anyhow::Result<()> {
        match self {
            Self::Go(go) => go_command(cx, user, go).await,
            Self::Claim(_) => claim_command(cx, user).await,
        }
    }
}

// /Setting off

/// The pet a player means: their active pet, or the one whose name (else the start of it, else
/// its species) matches, ignoring case.
fn pick_pet<'a>(pets: &'a [PetRow], name: Option<&str>) -> Result<&'a PetRow, String> {
    let Some(name) = name.map(str::trim).filter(|n| !n.is_empty()) else {
        return pets
            .iter()
            .find(|p| p.active)
            .or(pets.first())
            .ok_or_else(|| "You don't have a pet yet. Get one with `/pet adopt`.".to_owned());
    };
    let wanted = name.to_lowercase();
    let content = Content::get();
    let species = |p: &PetRow| content.pet(&p.species).map_or(String::new(), |s| s.name.to_lowercase());
    pets.iter()
        .find(|p| p.name.to_lowercase() == wanted)
        .or_else(|| pets.iter().find(|p| p.name.to_lowercase().starts_with(&wanted)))
        .or_else(|| pets.iter().find(|p| species(p) == wanted))
        .ok_or_else(|| {
            let names: Vec<String> = pets.iter().take(10).map(|p| format!("**{}**", p.name)).collect();
            format!("None of your pets is called **{name}**. You have {}.", names.join(", "))
        })
}

/// Why this pet cannot set off right now, if it cannot.
fn cannot_leave(app: &App, pet: &PetRow) -> Option<String> {
    let now = app.now();
    if pet.is_away(now) {
        let left = pet.trip_ends_at.map_or(0, |ends| (ends - now).num_seconds());
        Some(format!(
            "**{}** is already away at **{}**, back in {}.",
            pet.name,
            place_of(pet),
            span(left)
        ))
    } else if pet.has_souvenir(now) {
        Some(format!(
            "**{}** is back from **{}** with a souvenir. Claim it first with `/pet trip claim`.",
            pet.name,
            place_of(pet)
        ))
    } else if is_asleep(pet, now) {
        Some(format!(
            "**{}** is fast asleep. Wake it up before it travels.",
            pet.name
        ))
    } else {
        None
    }
}

enum Departure {
    /// Off it goes; the pet as it is now.
    Left(PetRow),
    Refused(String),
}

/// Sends `pet` to `trip` if it may go.
async fn depart(app: &App, user: u64, pet: &PetRow, trip: &Trip) -> anyhow::Result<Departure> {
    if let Some(why) = cannot_leave(app, pet) {
        return Ok(Departure::Refused(why));
    }
    if pet.level < trip.unlock_level {
        return Ok(Departure::Refused(format!(
            "**{}** opens when {} reaches level {} (it is level {}). Care, walks and trips all earn XP.",
            trip.name, pet.name, trip.unlock_level, pet.level
        )));
    }
    let now = app.now();
    let needs = current_needs(pet, now);
    let ends = now + Duration::hours(i64::from(trip.hours));
    let kept = [needs.hunger, needs.fun, needs.energy, needs.hygiene];
    if !app.db.start_trip(user, pet.id, &trip.id, kept, ends).await? {
        return Ok(Departure::Refused(format!("**{}** can't set off right now.", pet.name)));
    }
    Ok(Departure::Left(PetRow {
        hunger: needs.hunger,
        fun: needs.fun,
        energy: needs.energy,
        hygiene: needs.hygiene,
        needs_at: ends,
        trip_dest: Some(trip.id.clone()),
        trip_ends_at: Some(ends),
        ..pet.clone()
    }))
}

fn departure_note(pet: &PetRow, trip: &Trip) -> String {
    format!(
        "**{}** packs a tiny bag and sets off for **{}**. Back in {} h.\n*{}*",
        pet.name, trip.name, trip.hours, trip.blurb
    )
}

async fn go_command(cx: &Cx, user: u64, go: PetTripGo) -> anyhow::Result<()> {
    let Some(trip) = trips::get(go.destination.id()) else {
        return cx.reply_ephemeral("That place isn't on the map yet.").await;
    };
    let pets = cx.app.db.pets(user).await?;
    let pet = match pick_pet(&pets, go.pet.as_deref()) {
        Ok(pet) => pet,
        Err(why) => return cx.reply_ephemeral(why).await,
    };
    match depart(&cx.app, user, pet, trip).await? {
        Departure::Left(fresh) => {
            let data = pet_card(&cx.app, &fresh, Some(departure_note(&fresh, trip))).await?;
            cx.reply(data).await
        }
        Departure::Refused(why) => cx.reply_ephemeral(why).await,
    }
}

// /Choosing a place

/// The "where to?" card behind the Trip button: every place, and a menu of the ones that are open.
async fn picker(cx: &Cx, user: u64, pet_id: i64) -> anyhow::Result<()> {
    let Some(pet) = cx.app.db.pets(user).await?.into_iter().find(|p| p.id == pet_id) else {
        return cx.reply_ephemeral("That's not your pet.").await;
    };
    if let Some(why) = cannot_leave(&cx.app, &pet) {
        return cx.reply_ephemeral(why).await;
    }
    let mut lines = Vec::new();
    let mut options = Vec::new();
    for trip in trips::all() {
        let open = pet.level >= trip.unlock_level;
        lines.push(if open {
            format!(
                "**{}** · {} · level {}\n{} Finds {}.",
                trip.name,
                trip.length(),
                trip.unlock_level,
                trip.blurb,
                trip.finds()
            )
        } else {
            format!(
                "🔒 **{}** · {} · opens at level {}\n{}",
                trip.name,
                trip.length(),
                trip.unlock_level,
                trip.blurb
            )
        });
        options.push(SelectMenuOption {
            default: false,
            description: Some(if open {
                format!("{} · {}", trip.length(), trip.blurb)
            } else {
                format!("Opens at level {} (now {})", trip.unlock_level, pet.level)
            }),
            emoji: None,
            label: trip.name.clone(),
            value: trip.id.clone(),
        });
    }
    let png = pet_png(&cx.app, &pet).await?;
    let embed = EmbedBuilder::new()
        .color(BRAND)
        .title(format!("Where should {} go?", pet.name))
        .description(lines.join("\n\n"));
    let id = pet.id;
    cx.update(Cx::card(
        embed,
        "pet.png",
        png,
        vec![
            select(format!("py:s:{id}"), "Choose a destination…", options),
            row(vec![button(format!("py:b:{id}"), "Stay home", ButtonStyle::Secondary)]),
        ],
        false,
    ))
    .await
}

/// `py:s:<pet>` select: set off for the chosen place.
async fn set_off(cx: &Cx, user: u64, pet_id: i64, place: Option<&str>) -> anyhow::Result<()> {
    let Some(trip) = place.and_then(trips::get) else {
        return cx.reply_ephemeral("That place isn't on the map yet.").await;
    };
    let Some(pet) = cx.app.db.pets(user).await?.into_iter().find(|p| p.id == pet_id) else {
        return cx.reply_ephemeral("That's not your pet.").await;
    };
    match depart(&cx.app, user, &pet, trip).await? {
        Departure::Left(fresh) => {
            let data = pet_card(&cx.app, &fresh, Some(departure_note(&fresh, trip))).await?;
            cx.update(data).await
        }
        Departure::Refused(why) => cx.reply_ephemeral(why).await,
    }
}

// /Coming home

/// A claimed trip.
struct Souvenir {
    /// The pet after the claim.
    pet: PetRow,
    trip: &'static Trip,
    haul: Haul,
    progress: pets::Progress,
    balance: i64,
}

enum Claimed {
    Paid(Box<Souvenir>),
    /// Still away: what to tell the player.
    NotYet(String),
    Nothing,
}

/// Claims one pet's souvenir.
async fn claim_one(app: &App, user: u64, pet_id: i64) -> anyhow::Result<Claimed> {
    let content = Content::get();
    let now = app.now();
    let rolls = Rolls {
        shells: rand::random(),
        furniture: rand::random(),
        pick: rand::random(),
        story: rand::random(),
    };
    let mut made = None;
    let outcome = app
        .db
        .claim_trip(user, pet_id, now, |pet, trip_id| {
            let trip = trips::get(trip_id).unwrap_or(&trips::all()[0]);
            let mut haul = trip.haul(content, &pet.name, pet.level, rolls);
            // A pet that dozed off while waiting is too sleepy to learn from the trip.
            if is_asleep(pet, now) {
                haul.xp = 0;
            }
            let progress = pets::add_xp(pet.level, pet.xp, haul.xp);
            let payout = TripPayout {
                shells: haul.shells,
                item: haul.item.clone(),
                level: progress.level,
                xp: progress.xp,
            };
            made = Some((trip, haul, progress));
            payout
        })
        .await?;
    Ok(match (outcome, made) {
        (TripClaim::Claimed { pet, balance }, Some((trip, haul, progress))) => Claimed::Paid(Box::new(Souvenir {
            pet: PetRow {
                level: progress.level,
                xp: progress.xp,
                trip_dest: None,
                trip_ends_at: None,
                ..pet
            },
            trip,
            haul,
            progress,
            balance,
        })),
        (TripClaim::Away { trip, ends_at }, _) => Claimed::NotYet(format!(
            "That pet is still at {}, back in {}.",
            trips::get(&trip).map_or("its trip", |t| t.name.as_str()),
            span((ends_at - now).num_seconds())
        )),
        _ => Claimed::Nothing,
    })
}

impl Souvenir {
    /// The text of the claim: where it was, the story, the pay and any find.
    fn note(&self, before: i32) -> String {
        let content = Content::get();
        let name = &self.pet.name;
        let mut note = format!(
            "**{name}** is back from **{}**!\n*{}*\n**+{}** shells",
            self.trip.name,
            self.haul.story,
            thousands(self.haul.shells)
        );
        if self.haul.xp > 0 && before < pets::MAX_LEVEL {
            note.push_str(&format!(" · **+{}** XP", self.haul.xp));
        }
        if let Some(item) = self.haul.item.as_deref().and_then(|id| content.item(id)) {
            note.push_str(&format!(
                "\nIt also found a **{}** ({}), now in your `/inventory`.",
                item.name(),
                item.base.rarity.label().to_lowercase()
            ));
        }
        if self.progress.gained > 0 {
            note.push('\n');
            note.push_str(&level_up_note(name, before, self.progress.level));
        }
        note
    }
}

/// `py:cl:<pet>`: the Claim souvenir button.
async fn claim_button(cx: &Cx, user: u64, pet_id: i64) -> anyhow::Result<()> {
    let before = cx.app.db.pets(user).await?.into_iter().find(|p| p.id == pet_id);
    let Some(before) = before else {
        return cx.reply_ephemeral("That's not your pet.").await;
    };
    match claim_one(&cx.app, user, pet_id).await? {
        Claimed::Paid(souvenir) => {
            let data = pet_card(&cx.app, &souvenir.pet, Some(souvenir.note(before.level))).await?;
            cx.update(data).await?;
            customize::announce(cx).await;
            Ok(())
        }
        Claimed::NotYet(why) => cx.reply_ephemeral(why).await,
        // Somebody already collected it from another message: show the pet as it is.
        Claimed::Nothing => {
            let fresh = cx.app.db.pets(user).await?.into_iter().find(|p| p.id == pet_id);
            match fresh {
                Some(pet) => {
                    let note = format!("**{}** has no souvenir waiting.", pet.name);
                    let data = pet_card(&cx.app, &pet, Some(note)).await?;
                    cx.update(data).await
                }
                None => cx.reply_ephemeral("That's not your pet.").await,
            }
        }
    }
}

/// `/pet trip claim`: collects every souvenir that is ready; with none ready, says who is away.
async fn claim_command(cx: &Cx, user: u64) -> anyhow::Result<()> {
    let pets = cx.app.db.pets(user).await?;
    let now = cx.app.now();
    let ready: Vec<&PetRow> = pets.iter().filter(|p| p.has_souvenir(now)).take(8).collect();
    if ready.is_empty() {
        let away: Vec<String> = pets
            .iter()
            .filter(|p| p.is_away(now))
            .map(|p| {
                let left = p.trip_ends_at.map_or(0, |ends| (ends - now).num_seconds());
                format!("**{}** is at {}, back in {}.", p.name, place_of(p), span(left))
            })
            .collect();
        return cx
            .reply_ephemeral(if away.is_empty() {
                "None of your pets is on a trip. Send one with `/pet trip go`.".to_owned()
            } else {
                away.join("\n")
            })
            .await;
    }
    let mut paid = Vec::new();
    for pet in ready {
        if let Claimed::Paid(souvenir) = claim_one(&cx.app, user, pet.id).await? {
            paid.push((pet.level, *souvenir));
        }
    }
    match paid.as_slice() {
        [] => cx.reply_ephemeral("Those souvenirs were already collected.").await,
        [(before, souvenir)] => {
            let data = pet_card(&cx.app, &souvenir.pet, Some(souvenir.note(*before))).await?;
            cx.reply(data).await?;
            customize::announce(cx).await;
            Ok(())
        }
        many => {
            let mut embed = EmbedBuilder::new()
                .color(BRAND)
                .title(format!("{} pets are home", many.len()));
            for (before, souvenir) in many {
                embed = embed.field(EmbedFieldBuilder::new(pet_title(&souvenir.pet), souvenir.note(*before)));
            }
            let total: i64 = many.iter().map(|(_, s)| s.haul.shells).sum();
            let balance = many.iter().map(|(_, s)| s.balance).max().unwrap_or(0);
            cx.reply_embed(
                embed
                    .description(format!(
                        "**+{}** shells in all. Balance: {}.",
                        thousands(total),
                        thousands(balance)
                    ))
                    .build(),
            )
            .await?;
            customize::announce(cx).await;
            Ok(())
        }
    }
}

// /Buttons

/// `py:<action>:<pet>`: `o` open the place menu, `s` set off for the chosen place, `cl` claim
/// the souvenir, `b` back to the card.
pub(crate) async fn on_trip(cx: &Cx, mut args: Split<'_, char>, value: Option<&str>) -> anyhow::Result<()> {
    let user = cx.author()?.id.get();
    let action = args.next();
    let pet_id: i64 = args.next().and_then(|v| v.parse().ok()).unwrap_or(0);
    match action {
        Some("o") => picker(cx, user, pet_id).await,
        Some("s") => set_off(cx, user, pet_id, value).await,
        Some("cl") => claim_button(cx, user, pet_id).await,
        Some("b") => {
            let Some(pet) = cx.app.db.pets(user).await?.into_iter().find(|p| p.id == pet_id) else {
                return cx.reply_ephemeral("That's not your pet.").await;
            };
            let data: InteractionResponseData = pet_card(&cx.app, &pet, None).await?;
            cx.update(data).await
        }
        _ => cx.reply_ephemeral("This button has expired.").await,
    }
}

#[cfg(test)]
mod tests {
    use pinchora_render::PetStatus;
    use twilight_model::application::command::CommandOptionChoiceValue;

    use super::*;
    use crate::commands::pet::status;

    fn pet(id: i64, name: &str, species: &str, active: bool) -> PetRow {
        PetRow {
            id,
            owner_id: 1,
            species: species.to_owned(),
            name: name.to_owned(),
            level: 1,
            xp: 0,
            hunger: 80,
            fun: 80,
            energy: 80,
            hygiene: 80,
            needs_at: chrono::Utc::now(),
            active,
            trip_dest: None,
            trip_ends_at: None,
        }
    }

    #[test]
    fn the_command_offers_exactly_the_shipped_places() {
        let ids: Vec<&str> = Place::ALL.iter().map(|p| p.id()).collect();
        let shipped: Vec<&str> = trips::all().iter().map(|t| t.id.as_str()).collect();
        assert_eq!(ids, shipped, "keep `Place` in step with content/pet_trips.json");

        // /pet trip go|claim, and the choices of `destination` carry each place's name, length and level.
        let command = <super::super::PetCommand as CreateCommand>::create_command();
        let trip = command.options.iter().find(|o| o.name == "trip").expect("/pet trip");
        let subs: Vec<&str> = trip.options.iter().flatten().map(|o| o.name.as_str()).collect();
        assert_eq!(subs, ["go", "claim"]);
        let go = trip.options.iter().flatten().find(|o| o.name == "go").unwrap();
        let destination = go
            .options
            .iter()
            .flatten()
            .find(|o| o.name == "destination")
            .expect("the destination option");
        assert_eq!(destination.required, Some(true));
        let pet_option = go
            .options
            .iter()
            .flatten()
            .find(|o| o.name == "pet")
            .expect("the pet option");
        assert_ne!(pet_option.required, Some(true), "the pet is optional");
        let choices = destination.choices.as_ref().expect("a choice per place");
        assert_eq!(choices.len(), trips::all().len());
        for (choice, t) in choices.iter().zip(trips::all()) {
            assert_eq!(
                choice.name,
                format!("{} · {} h · level {}", t.name, t.hours, t.unlock_level)
            );
            assert!(
                matches!(&choice.value, CommandOptionChoiceValue::String(v) if v == &t.id),
                "{choice:?}"
            );
        }
    }

    #[test]
    fn a_pet_is_found_by_name_prefix_or_species() {
        let pets = vec![
            pet(1, "Pip", "crumbclaw_crab", false),
            pet(2, "Biscuit", "ribbonfin_guppy", true),
            pet(3, "Pipette", "puddlesock_otter", false),
        ];
        // No name: the active pet.
        assert_eq!(pick_pet(&pets, None).unwrap().id, 2);
        assert_eq!(pick_pet(&pets, Some("  ")).unwrap().id, 2);
        // The exact name beats a longer one that starts with it.
        assert_eq!(pick_pet(&pets, Some("pip")).unwrap().id, 1);
        assert_eq!(pick_pet(&pets, Some("PIPE")).unwrap().id, 3);
        assert_eq!(pick_pet(&pets, Some("ribbonfin guppy")).unwrap().id, 2);
        let err = pick_pet(&pets, Some("Nobody")).unwrap_err();
        assert!(err.contains("Nobody") && err.contains("**Biscuit**"), "{err}");
        assert!(pick_pet(&[], None).is_err());
    }

    #[test]
    fn only_awake_pets_at_home_can_leave() {
        // The state checks run on a clock; the pure ones are covered through `status`.
        let mut p = pet(1, "Pip", "crumbclaw_crab", true);
        let now = chrono::Utc::now();
        assert_eq!(status(&p, now), PetStatus::Awake);
        p.trip_dest = Some("tidepool_meadow".to_owned());
        p.trip_ends_at = Some(now + Duration::hours(1));
        assert_eq!(status(&p, now), PetStatus::Away);
    }
}
