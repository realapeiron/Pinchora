use std::{str::Split, sync::Arc, time::Duration};

use chrono::{Timelike, Utc};
use pinchora_core::{
    i18n::Lang,
    leveling::{self, week_start},
    t,
};
use pinchora_db::{Cosmetic, MilestonePaid, MilestoneReward, NewCatch};
use pinchora_game::{
    Content, Rarity,
    fishing::{self, BAIT_PRICE, BUCKET_CAPACITY, Bait, CAST_COOLDOWN_SECS, Gear, Reel, Reward, Rod, Spot, Tackle},
};
use twilight_interactions::command::{CommandModel, CommandOption, CreateCommand, CreateOption};
use twilight_model::{
    channel::message::{
        Component, Embed,
        component::{ActionRow, Button, ButtonStyle},
    },
    http::interaction::InteractionResponseData,
};
use twilight_util::builder::embed::{EmbedBuilder, EmbedFooterBuilder};

use crate::{
    App, Cx,
    commands::customize,
    util::{BRAND, SUCCESS, WARN, thousands_in},
};

mod art;
mod book;
mod bucket;
mod gear;

pub(crate) use bucket::on_bucket;
pub(crate) use gear::on_gear;

// /Command

#[derive(CommandModel, CreateCommand)]
#[command(name = "fish", desc = "Fishing: cast a line, buy rods and bait, sell your catch")]
pub enum FishCommand {
    #[command(name = "cast")]
    Cast(FishCast),
    #[command(name = "shop")]
    Shop(FishShop),
    #[command(name = "gear")]
    Gear(FishGear),
    #[command(name = "sell")]
    Sell(FishSell),
    #[command(name = "book")]
    Book(FishBook),
    #[command(name = "dock")]
    Dock(super::reef_loop::Dock),
    #[command(name = "contest")]
    Contest(super::reef_loop::Contest),
    #[command(name = "season")]
    Season(super::reef_loop::Season),
    #[command(name = "dive")]
    Dive(super::reef_loop::Dive),
}

#[derive(CommandModel, CreateCommand)]
#[command(
    name = "cast",
    desc = "Cast a line (10 shells of plain bait). Press Reel the moment something bites!"
)]
pub struct FishCast {
    /// Where to fish; the last spot you fished is remembered
    spot: Option<SpotChoice>,
}

#[derive(CommandModel, CreateCommand)]
#[command(name = "shop", desc = "Buy rods and bait with shells")]
pub struct FishShop;

#[derive(CommandModel, CreateCommand)]
#[command(name = "gear", desc = "Pick your spot, rod and bait")]
pub struct FishGear;

#[derive(CommandModel, CreateCommand)]
#[command(name = "sell", desc = "Sell the fish in your bucket for shells")]
pub struct FishSell;

#[derive(CommandModel, CreateCommand)]
#[command(
    name = "book",
    desc = "Your fish book: every species, record sizes and milestone rewards"
)]
pub struct FishBook;

/// The fishing spots, as the choices of `/fish cast`. `content/fishing_gear.json` has the same ids.
#[derive(CommandOption, CreateOption, Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpotChoice {
    #[option(name = "Shore", value = "shore")]
    Shore,
    #[option(name = "Reef (level 5)", value = "reef")]
    Reef,
    #[option(name = "Kelp Forest (level 12)", value = "kelp_forest")]
    KelpForest,
    #[option(name = "Deep Sea (level 20)", value = "deep_sea")]
    DeepSea,
    #[option(name = "Abyss (level 30)", value = "abyss")]
    Abyss,
}

impl FishCommand {
    pub async fn run(self, cx: &Cx) -> anyhow::Result<()> {
        super::social::remember(cx, cx.author()?).await?;
        match self {
            Self::Dock(c) => c.run(cx).await,
            Self::Contest(c) => c.run(cx).await,
            Self::Season(c) => c.run(cx).await,
            Self::Dive(c) => c.run(cx).await,

            Self::Cast(c) => cast(cx, c.spot).await,
            Self::Shop(_) => gear::shop(cx).await,
            Self::Gear(_) => gear::panel(cx).await,
            Self::Sell(_) => bucket::sell(cx).await,
            Self::Book(_) => book::view(cx).await,
        }
    }
}

// /Helpers

/// The player's global level.
pub(super) async fn player_level(app: &App, user: u64) -> anyhow::Result<u32> {
    Ok(leveling::level_of(app.db.user(user).await?.xp))
}

/// The ids of the species in the game: the fish in a bucket that are not among them do not count.
fn known_fish() -> Vec<&'static str> {
    Content::get().fish.iter().map(|f| f.id.as_str()).collect()
}

/// The book milestones in the form the database pays them.
fn milestone_rewards(gear: &Gear) -> Vec<MilestoneReward> {
    gear.milestones
        .iter()
        .map(|m| MilestoneReward {
            species: m.species as i32,
            shells: m.shells,
            cosmetic: match &m.reward {
                Reward::Badge { id } => Cosmetic::Badge(id.clone()),
                Reward::Background { id } => Cosmetic::Background(id.clone()),
            },
        })
        .collect()
}

/// "Reef · Reedline Rod · Glow Worm ×4 left": what is on the line.
fn tackle_line(lang: Lang, spot: &Spot, rod: &Rod, bait: Option<(&Bait, i32)>) -> String {
    let content = Content::get();
    let bait = match bait {
        Some((bait, left)) => t!(
            lang,
            "fish.cast.bait-left",
            bait = content.name(&bait.id, lang),
            left = left
        ),
        None => t!(lang, "fish.plain-bait"),
    };
    format!(
        "{} · {} · {bait}",
        content.name(&spot.id, lang),
        content.name(&rod.id, lang)
    )
}

fn reel_row(lang: Lang, custom_id: String, enabled: bool) -> Vec<Component> {
    vec![Component::ActionRow(ActionRow {
        id: None,
        components: vec![Component::Button(Button {
            custom_id: Some(custom_id),
            disabled: !enabled,
            emoji: None,
            id: None,
            label: Some(t!(lang, "fish.cast.reel-button")),
            sku_id: None,
            style: ButtonStyle::Success,
            url: None,
        })],
    })]
}

// /Cast

async fn cast(cx: &Cx, choice: Option<SpotChoice>) -> anyhow::Result<()> {
    let user = cx.author()?.id.get();
    let lang = cx.lang();
    let content = Content::get();
    let gear = &content.fishing;
    let saved = cx.app.db.fishing_gear(user).await?;
    let level = player_level(&cx.app, user).await?;
    let spot = gear.spot_for(choice.map_or(saved.spot.as_str(), |c| c.value()), level);
    if let Some(choice) = choice
        && spot.id != choice.value()
        && let Some(wanted) = gear.spot(choice.value())
    {
        return cx
            .reply_ephemeral(t!(
                lang,
                "fish.cast.locked",
                wanted = content.name(&wanted.id, lang),
                need = wanted.level,
                level = level,
                current = content.name(&spot.id, lang)
            ))
            .await;
    }
    if saved.spot != spot.id {
        cx.app.db.set_fishing_spot(user, &spot.id).await?;
    }
    let rod = gear.rod(&saved.rod).unwrap_or_else(|| gear.starter_rod());
    if cx.app.db.bucket_count(user, &known_fish()).await? >= BUCKET_CAPACITY {
        return cx
            .reply_ephemeral(t!(lang, "fish.cast.bucket-full", capacity = BUCKET_CAPACITY))
            .await;
    }
    if let Some(left) = cx
        .app
        .cache
        .try_cooldown(&format!("fish:{user}"), Duration::from_secs(CAST_COOLDOWN_SECS))
        .await?
    {
        return cx
            .reply_ephemeral(t!(lang, "fish.cast.cooldown", secs = left.as_secs() + 1))
            .await;
    }
    // A bait on the hook is used up instead of plain bait; when it has run out, plain bait it is.
    let mut bait: Option<(&Bait, i32)> = None;
    if let Some(equipped) = saved.bait.as_deref().and_then(|id| gear.bait(id))
        && let Some(left) = cx.app.db.take_bait(user, &equipped.id).await?
    {
        bait = Some((equipped, left));
    }
    if bait.is_none() && cx.app.db.spend_shells(user, BAIT_PRICE, "bait").await?.is_none() {
        return cx
            .reply_ephemeral(t!(lang, "fish.cast.no-bait", price = BAIT_PRICE))
            .await;
    }
    let nonce: u32 = rand::random();
    let delay_ms = fishing::bite_delay_ms(rand::random::<f64>());
    let bite_at = Utc::now().timestamp_millis() + delay_ms;
    // What the cast was made with: the reel works from this, so a gear change in between counts next time.
    let state = format!(
        "{bite_at}:{}:{}:{}",
        spot.id,
        rod.id,
        bait.map_or("-", |(b, _)| b.id.as_str())
    );
    cx.app
        .cache
        .put(
            &format!("cast:{user}:{nonce}"),
            &state,
            Duration::from_secs(fishing::LINE_TTL_SECS),
        )
        .await?;
    let custom_id = format!("fr:{user}:{nonce}");
    let footer = tackle_line(lang, spot, rod, bait);
    let waiting = EmbedBuilder::new()
        .color(BRAND)
        .title(t!(lang, "fish.cast.waiting-title"))
        .description(t!(lang, "fish.cast.waiting"))
        .footer(EmbedFooterBuilder::new(footer.clone()))
        .build();
    cx.reply(InteractionResponseData {
        embeds: Some(vec![waiting]),
        components: Some(reel_row(lang, custom_id.clone(), true)),
        ..Default::default()
    })
    .await?;

    // Reveal the bite at the bite time by editing the message, unless Reel! was already pressed too early.
    // The language is read now: the task below has no interaction to ask.
    let bite_title = t!(lang, "fish.cast.bite-title");
    let (responder, app) = (Arc::clone(&cx.responder), Arc::clone(&cx.app));
    cx.app.spawn(async move {
        let wait = bite_at - Utc::now().timestamp_millis();
        tokio::time::sleep(Duration::from_millis(wait.max(0) as u64)).await;
        if let Ok(None) = app.cache.get(&format!("cast:{user}:{nonce}")).await {
            return;
        }
        let bite = EmbedBuilder::new()
            .color(WARN)
            .title(bite_title)
            .footer(EmbedFooterBuilder::new(footer))
            .build();
        let _ = responder
            .edit(&InteractionResponseData {
                embeds: Some(vec![bite]),
                components: Some(reel_row(lang, custom_id, true)),
                ..Default::default()
            })
            .await;
    });
    Ok(())
}

// /Reel

/// `fr:<user>:<nonce>` button.
pub(crate) async fn on_reel(cx: &Cx, mut args: Split<'_, char>) -> anyhow::Result<()> {
    let caster: u64 = args.next().and_then(|v| v.parse().ok()).unwrap_or(0);
    let nonce = args.next().unwrap_or_default();
    let user = cx.author()?.id.get();
    let lang = cx.lang();
    if user != caster {
        return cx.reply_ephemeral(t!(lang, "fish.reel.not-yours")).await;
    }
    let Some(state) = cx.app.cache.take(&format!("cast:{user}:{nonce}")).await? else {
        return cx.reply_ephemeral(t!(lang, "fish.reel.gone")).await;
    };
    // `<bite time>[:<spot>:<rod>:<bait>]`; a cast without the rest is fished with what the player has now.
    let mut parts = state.split(':');
    let Some(bite_at) = parts.next().and_then(|v| v.parse::<i64>().ok()) else {
        return cx.reply_ephemeral(t!(lang, "fish.reel.gone")).await;
    };
    let content = Content::get();
    let gear = &content.fishing;
    let saved = cx.app.db.fishing_gear(user).await?;
    let spot = match parts.next() {
        Some(id) => gear.spot(id),
        None => None,
    }
    .unwrap_or_else(|| gear.spot_for(&saved.spot, 0));
    let rod = match parts.next() {
        Some(id) => gear.rod(id),
        None => gear.rod(&saved.rod),
    }
    .unwrap_or_else(|| gear.starter_rod());
    let bait = parts.next().and_then(|id| gear.bait(id));

    let now = Utc::now();
    let late_by = now.timestamp_millis() - bite_at;
    let miss = |title: &str, text: &str| InteractionResponseData {
        embeds: Some(vec![
            EmbedBuilder::new().color(WARN).title(title).description(text).build(),
        ]),
        components: Some(Vec::new()),
        ..Default::default()
    };
    match fishing::reel_result(late_by, rod.window_ms) {
        Reel::Early => {
            return cx
                .update(miss(
                    &t!(lang, "fish.reel.too-early"),
                    &t!(lang, "fish.reel.too-early-text"),
                ))
                .await;
        }
        Reel::Late => {
            return cx
                .update(miss(
                    &t!(lang, "fish.reel.too-slow"),
                    &t!(lang, "fish.reel.too-slow-text"),
                ))
                .await;
        }
        Reel::Caught => {}
    }
    let clock = cx.app.now();
    let Some(fish) = fishing::catch_fish(
        content,
        spot,
        clock.hour(),
        chrono::Datelike::month(&clock),
        Tackle { luck: rod.luck, bait },
        rand::random::<f64>(),
        rand::random::<f64>(),
    ) else {
        return cx
            .update(miss(
                &t!(lang, "fish.reel.nothing"),
                &t!(lang, "fish.reel.nothing-text"),
            ))
            .await;
    };
    let size = fishing::roll_size(fish, rand::random::<f64>(), rand::random::<f64>());
    let ratio = fishing::size_ratio(fish, size);
    let value = fishing::sale_value(fish, size);
    let milestones = milestone_rewards(gear);
    let known = known_fish();
    let log = cx
        .app
        .db
        .record_catch(
            user,
            NewCatch {
                fish_id: &fish.id,
                size_cm: size,
                ratio: f64::from(ratio),
                // Boots and tins are no catch to brag about.
                weekly: cx.guild_id().filter(|_| !fish.junk).map(|g| (g, week_start(clock))),
                milestones: &milestones,
                known: &known,
            },
        )
        .await?;
    cx.app.quest(user, pinchora_game::quests::QuestKind::Fish).await;
    let sprite = cx.app.sprites.fish(&fish.id);
    let png = cx
        .app
        .render(move |_| {
            let factor = (144 / sprite.width().max(sprite.height()).max(1)).clamp(1, 12);
            let big = image::imageops::resize(
                sprite.as_ref(),
                sprite.width() * factor,
                sprite.height() * factor,
                image::imageops::FilterType::Nearest,
            );
            let mut buf = std::io::Cursor::new(Vec::new());
            big.write_to(&mut buf, image::ImageFormat::Png)?;
            Ok(buf.into_inner())
        })
        .await?;
    let mut notes = Vec::new();
    if log.first {
        notes.push(t!(lang, "fish.catch.new-species"));
    } else if log.record {
        notes.push(t!(
            lang,
            "fish.catch.record",
            best = pinchora_core::server_stats::decimal_in(lang, log.previous_best, 1)
        ));
    }
    if log.week_best && ratio >= 1.15 {
        notes.push(t!(lang, "fish.catch.week-best"));
    }
    if log.bucket >= BUCKET_CAPACITY {
        notes.push(t!(lang, "fish.catch.bucket-full"));
    }
    let notes = if notes.is_empty() {
        String::new()
    } else {
        format!("\n{}", notes.join("\n"))
    };
    let reaction = pinchora_core::server_stats::decimal_in(lang, late_by as f64 / 1000.0, 2);
    let flavor = content
        .description(&fish.id, lang)
        .map_or_else(|| fish.flavor.clone(), |text| text.into_owned());
    let (new, rarity) = (
        if log.first { "🆕 " } else { "" },
        pinchora_game::lang::lower(lang, fish.rarity.label_in(lang)),
    );
    let title = if matches!(fish.rarity, Rarity::Uncommon | Rarity::Epic) {
        t!(
            lang,
            "fish.catch.title-an",
            new = new,
            rarity = rarity,
            fish = content.name(&fish.id, lang)
        )
    } else {
        t!(
            lang,
            "fish.catch.title-a",
            new = new,
            rarity = rarity,
            fish = content.name(&fish.id, lang)
        )
    };
    let embed = EmbedBuilder::new()
        .color(if log.first { SUCCESS } else { fish.rarity.color() })
        .title(title)
        .description(t!(
            lang,
            "fish.catch.body",
            flavor = flavor,
            size = pinchora_core::server_stats::decimal_in(lang, size, 1),
            value = thousands_in(lang, value),
            notes = notes
        ))
        .footer(EmbedFooterBuilder::new(t!(
            lang,
            "fish.catch.footer",
            spot = content.name(&spot.id, lang),
            reaction = reaction,
            species = log.species,
            total = content.fish.len(),
            bucket = log.bucket,
            capacity = BUCKET_CAPACITY
        )));
    let mut data = Cx::card(embed, "catch.png", png, Vec::new(), false);
    if let Some(embeds) = data.embeds.as_mut() {
        embeds.extend(log.paid.iter().map(|paid| milestone_embed(lang, paid)));
    }
    cx.update(data).await?;
    // A new species in the fish book may have just earned a badge.
    customize::announce(cx).await;
    Ok(())
}

/// The embed that tells of a fish-book milestone just paid.
fn milestone_embed(lang: Lang, paid: &MilestonePaid) -> Embed {
    let content = Content::get();
    let (id, hint) = match &paid.reward.cosmetic {
        Cosmetic::Badge(id) => (id, "/customize badges"),
        Cosmetic::Background(id) => (id, "/customize background"),
    };
    let key = match (&paid.reward.cosmetic, paid.cosmetic_new) {
        (Cosmetic::Badge(_), true) => "fish.milestone.badge-new",
        (Cosmetic::Badge(_), false) => "fish.milestone.badge-old",
        (Cosmetic::Background(_), true) => "fish.milestone.background-new",
        (Cosmetic::Background(_), false) => "fish.milestone.background-old",
    };
    EmbedBuilder::new()
        .color(Rarity::Legendary.color())
        .title(t!(lang, "fish.milestone.title", species = paid.reward.species))
        .description(t!(
            lang,
            key,
            shells = thousands_in(lang, paid.reward.shells),
            name = content.name(id, lang),
            hint = hint
        ))
        .build()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_spot_choices_are_the_content_spots() {
        let spots: Vec<_> = Content::get().fishing.spots.iter().map(|s| s.id.as_str()).collect();
        let choices: Vec<_> = [
            SpotChoice::Shore,
            SpotChoice::Reef,
            SpotChoice::KelpForest,
            SpotChoice::DeepSea,
            SpotChoice::Abyss,
        ]
        .iter()
        .map(|c| c.value())
        .collect();
        assert_eq!(spots, choices);
    }

    #[test]
    fn the_database_defaults_are_the_starter_gear() {
        let gear = &Content::get().fishing;
        assert_eq!(gear.first_spot().id, pinchora_db::DEFAULT_SPOT);
        assert_eq!(gear.starter_rod().id, pinchora_db::DEFAULT_ROD);
    }
}
