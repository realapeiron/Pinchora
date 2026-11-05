use std::{str::Split, time::Duration};

use chrono::Utc;
use pinchora_db::PetRow;
use pinchora_game::{
    Content,
    pets::{self, CAPSULE_PRICE, Care, DUPLICATE_REFUND, FOOD_PRICE, Needs},
};
use pinchora_render::{PetCard, Theme};
use twilight_interactions::command::{CommandModel, CreateCommand};
use twilight_model::{
    channel::message::{
        Component, MessageFlags,
        component::{ActionRow, Button, ButtonStyle, SelectMenu, SelectMenuOption, SelectMenuType},
    },
    http::interaction::InteractionResponseData,
};
use twilight_util::builder::embed::{EmbedBuilder, EmbedFieldBuilder};

use crate::{
    App, Cx,
    commands::customize,
    util::{BRAND, WARN, thousands},
};

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
#[command(name = "list", desc = "All your pets; choose which one is active")]
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
            Self::List(_) => list(cx, user).await,
            Self::Rename(c) => {
                let Some(pet) = cx.app.db.active_pet(user).await? else {
                    return cx.reply_ephemeral("You don't have a pet yet.").await;
                };
                let name: String = c.name.split_whitespace().collect::<Vec<_>>().join(" ");
                cx.app.db.rename_pet(user, pet.id, &name).await?;
                cx.reply_ephemeral(format!("Your pet is now called **{name}**.")).await
            }
        }
    }
}

fn current_needs(pet: &PetRow) -> Needs {
    let hours = (Utc::now() - pet.needs_at).num_seconds() as f32 / 3600.0;
    Needs {
        hunger: pet.hunger,
        fun: pet.fun,
        energy: pet.energy,
        hygiene: pet.hygiene,
    }
    .after(hours)
}

fn button(custom_id: String, label: &str, style: ButtonStyle) -> Component {
    Component::Button(Button {
        custom_id: Some(custom_id),
        disabled: false,
        emoji: None,
        id: None,
        label: Some(label.to_owned()),
        sku_id: None,
        style,
        url: None,
    })
}

async fn pet_card(app: &App, pet: &PetRow, note: Option<String>) -> anyhow::Result<InteractionResponseData> {
    let content = Content::get();
    let species = content.pet(&pet.species);
    let needs = current_needs(pet);
    let sprites = app.sprites.clone();
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
        theme: Theme::reef(),
    };
    let species_id = pet.species.clone();
    let png = app
        .render(move |r| {
            let sprite = sprites.pet(&species_id);
            let factor = (PetCard::SPRITE_BOX / sprite.width().max(sprite.height()).max(1)).clamp(1, 16);
            let (w, h) = (sprite.width() * factor, sprite.height() * factor);
            let big = image::imageops::resize(sprite.as_ref(), w, h, image::imageops::FilterType::Nearest);
            let mut buf = std::io::Cursor::new(Vec::new());
            big.write_to(&mut buf, image::ImageFormat::Png)?;
            r.pet_card(&PetCard {
                sprite_png: buf.into_inner(),
                sprite_w: w,
                sprite_h: h,
                ..card
            })
        })
        .await?;
    let mut embed = EmbedBuilder::new().color(BRAND).title(format!(
        "{} the {}",
        pet.name,
        species.map_or("pet", |s| s.name.as_str())
    ));
    if let Some(note) = note {
        embed = embed.description(note);
    }
    let id = pet.id;
    let components = vec![Component::ActionRow(ActionRow {
        id: None,
        components: vec![
            button(
                format!("pc:feed:{id}"),
                &format!("Feed ({FOOD_PRICE} shells)"),
                ButtonStyle::Success,
            ),
            button(format!("pc:play:{id}"), "Play", ButtonStyle::Primary),
            button(format!("pc:walk:{id}"), "Walk", ButtonStyle::Primary),
            button(format!("pc:clean:{id}"), "Bath", ButtonStyle::Secondary),
        ],
    })];
    Ok(Cx::card(embed, "pet.png", png, components, false))
}

/// `pc:<action>:<pet>` buttons.
pub(crate) async fn on_care(cx: &Cx, mut args: Split<'_, char>) -> anyhow::Result<()> {
    let action = match args.next() {
        Some("feed") => Care::Feed,
        Some("play") => Care::Play,
        Some("walk") => Care::Walk,
        _ => Care::Clean,
    };
    let pet_id: i64 = args.next().and_then(|v| v.parse().ok()).unwrap_or(0);
    let user = cx.author()?.id.get();
    let Some(pet) = cx.app.db.pets(user).await?.into_iter().find(|p| p.id == pet_id) else {
        return cx
            .reply_ephemeral("That's not your pet. Get your own with `/pet adopt`!")
            .await;
    };
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
    let needs = current_needs(&pet);
    let Some(after) = action.apply(needs) else {
        return cx
            .reply_ephemeral(format!("{} is too tired for that. Let it rest.", pet.name))
            .await;
    };
    let (mut level, mut xp) = (pet.level, pet.xp + action.xp());
    let mut leveled = false;
    while xp >= pets::xp_to_next(level) {
        xp -= pets::xp_to_next(level);
        level += 1;
        leveled = true;
    }
    cx.app
        .db
        .save_pet(
            user,
            pet.id,
            [after.hunger, after.fun, after.energy, after.hygiene],
            level,
            xp,
        )
        .await?;
    cx.app.quest(user, pinchora_game::quests::QuestKind::Care).await;
    let verb = match action {
        Care::Feed => "munches happily",
        Care::Play => "bounces around with you",
        Care::Clean => "is squeaky clean",
        Care::Walk => "trots along the shore with you",
    };
    let mut note = format!("{} {verb}. +{} XP", pet.name, action.xp());
    // Walks turn up a few shells on the beach.
    if action == Care::Walk {
        let found = rand::random_range(5..=30);
        cx.app.db.earn_shells(user, found, "pet_walk").await?;
        note.push_str(&format!("\nFound **{found}** shells on the walk!"));
    }
    if leveled {
        note.push_str(&format!("\n🎉 **{} reached level {level}!**", pet.name));
    }
    let fresh = PetRow {
        level,
        xp,
        hunger: after.hunger,
        fun: after.fun,
        energy: after.energy,
        hygiene: after.hygiene,
        needs_at: Utc::now(),
        ..pet
    };
    let data = pet_card(&cx.app, &fresh, Some(note)).await?;
    cx.update(data).await
}

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
        components: Some(vec![Component::ActionRow(ActionRow {
            id: None,
            components: vec![Component::SelectMenu(SelectMenu {
                channel_types: None,
                custom_id: "pa".to_owned(),
                default_values: None,
                disabled: false,
                id: None,
                kind: SelectMenuType::Text,
                max_values: Some(1),
                min_values: Some(1),
                options: Some(options),
                placeholder: Some("Choose a starter pet…".to_owned()),
                required: None,
            })],
        })]),
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

async fn list(cx: &Cx, user: u64) -> anyhow::Result<()> {
    let content = Content::get();
    let pets = cx.app.db.pets(user).await?;
    if pets.is_empty() {
        return cx.reply_ephemeral("No pets yet. Start with `/pet adopt`.").await;
    }
    let mut embed = EmbedBuilder::new()
        .color(BRAND)
        .title(format!("Your pets ({})", pets.len()));
    for pet in pets.iter().take(25) {
        let species = content.pet(&pet.species);
        let needs = current_needs(pet);
        embed = embed.field(
            EmbedFieldBuilder::new(
                format!("{}{}", if pet.active { "★ " } else { "" }, pet.name),
                format!(
                    "{} {} · Lv. {} · {}",
                    species.map_or("", |s| s.rarity.label()),
                    species.map_or(pet.species.as_str(), |s| s.name.as_str()),
                    pet.level,
                    needs.mood()
                ),
            )
            .inline(),
        );
    }
    let options = pets
        .iter()
        .take(25)
        .map(|p| SelectMenuOption {
            default: p.active,
            description: content.pet(&p.species).map(|s| s.name.clone()),
            emoji: None,
            label: p.name.clone(),
            value: p.id.to_string(),
        })
        .collect();
    cx.reply(InteractionResponseData {
        embeds: Some(vec![embed.build()]),
        components: Some(vec![Component::ActionRow(ActionRow {
            id: None,
            components: vec![Component::SelectMenu(SelectMenu {
                channel_types: None,
                custom_id: "ps".to_owned(),
                default_values: None,
                disabled: false,
                id: None,
                kind: SelectMenuType::Text,
                max_values: Some(1),
                min_values: Some(1),
                options: Some(options),
                placeholder: Some("Choose your active pet…".to_owned()),
                required: None,
            })],
        })]),
        flags: Some(MessageFlags::EPHEMERAL),
        ..Default::default()
    })
    .await
}

/// `ps` select: switch the active pet.
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
