use super::*;

// /Cards

/// A species' sprite scaled up with nearest-neighbour to fit a box, as PNG, with its size.
pub(super) fn scaled_sprite(sprites: &Sprites, species: &str, bx: u32, by: u32) -> anyhow::Result<(Vec<u8>, u32, u32)> {
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

pub(super) fn pet_title(lang: Lang, pet: &PetRow) -> String {
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
pub(super) async fn pet_card(
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
