use std::{str::Split, sync::Arc, time::Duration};

use chrono::{Timelike, Utc};
use pinchora_core::leveling::{self, week_start};
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
    util::{BRAND, SUCCESS, WARN, thousands},
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
        match self {
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
fn tackle_line(spot: &Spot, rod: &Rod, bait: Option<(&Bait, i32)>) -> String {
    let bait = match bait {
        Some((bait, left)) => format!("{} ×{left} left", bait.name),
        None => "Plain bait".to_owned(),
    };
    format!("{} · {} · {bait}", spot.name, rod.name)
}

fn reel_row(custom_id: String, enabled: bool) -> Vec<Component> {
    vec![Component::ActionRow(ActionRow {
        id: None,
        components: vec![Component::Button(Button {
            custom_id: Some(custom_id),
            disabled: !enabled,
            emoji: None,
            id: None,
            label: Some("Reel!".to_owned()),
            sku_id: None,
            style: ButtonStyle::Success,
            url: None,
        })],
    })]
}

// /Cast

async fn cast(cx: &Cx, choice: Option<SpotChoice>) -> anyhow::Result<()> {
    let user = cx.author()?.id.get();
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
            .reply_ephemeral(format!(
                "The **{}** opens at level {} and you are level {level}. Until then the **{}** is yours.",
                wanted.name, wanted.level, spot.name
            ))
            .await;
    }
    if saved.spot != spot.id {
        cx.app.db.set_fishing_spot(user, &spot.id).await?;
    }
    let rod = gear.rod(&saved.rod).unwrap_or_else(|| gear.starter_rod());
    if cx.app.db.bucket_count(user, &known_fish()).await? >= BUCKET_CAPACITY {
        return cx
            .reply_ephemeral(format!(
                "Your bucket is full ({BUCKET_CAPACITY}/{BUCKET_CAPACITY}). Sell some fish with `/fish sell` first."
            ))
            .await;
    }
    if let Some(left) = cx
        .app
        .cache
        .try_cooldown(&format!("fish:{user}"), Duration::from_secs(CAST_COOLDOWN_SECS))
        .await?
    {
        return cx
            .reply_ephemeral(format!(
                "Your line is still drying. Cast again in {} s.",
                left.as_secs() + 1
            ))
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
            .reply_ephemeral(format!("Plain bait costs {BAIT_PRICE} shells. Try `/daily`!"))
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
    let footer = tackle_line(spot, rod, bait);
    let waiting = EmbedBuilder::new()
        .color(BRAND)
        .title("🎣 Line in the water…")
        .description("Wait for the bite, then press **Reel!** Too early scares the fish away.")
        .footer(EmbedFooterBuilder::new(footer.clone()))
        .build();
    cx.reply(InteractionResponseData {
        embeds: Some(vec![waiting]),
        components: Some(reel_row(custom_id.clone(), true)),
        ..Default::default()
    })
    .await?;

    // Reveal the bite after the delay by editing the message.
    let responder = Arc::clone(&cx.responder);
    cx.app.spawn(async move {
        tokio::time::sleep(Duration::from_millis(delay_ms as u64)).await;
        let bite = EmbedBuilder::new()
            .color(WARN)
            .title("❗ Something bit! REEL IT IN!")
            .footer(EmbedFooterBuilder::new(footer))
            .build();
        let _ = responder
            .edit(&InteractionResponseData {
                embeds: Some(vec![bite]),
                components: Some(reel_row(custom_id, true)),
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
    if user != caster {
        return cx
            .reply_ephemeral("That's someone else's line! Cast your own with `/fish cast`.")
            .await;
    }
    let Some(state) = cx.app.cache.take(&format!("cast:{user}:{nonce}")).await? else {
        return cx
            .reply_ephemeral("This line is gone. Cast again with `/fish cast`.")
            .await;
    };
    // `<bite time>[:<spot>:<rod>:<bait>]`; a cast without the rest is fished with what the player has now.
    let mut parts = state.split(':');
    let Some(bite_at) = parts.next().and_then(|v| v.parse::<i64>().ok()) else {
        return cx
            .reply_ephemeral("This line is gone. Cast again with `/fish cast`.")
            .await;
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
                    "Too early!",
                    "The splash scared the fish away. Cast again with `/fish cast`.",
                ))
                .await;
        }
        Reel::Late => {
            return cx
                .update(miss(
                    "Too slow!",
                    "It slipped off the hook. Cast again with `/fish cast`.",
                ))
                .await;
        }
        Reel::Caught => {}
    }
    let clock = cx.app.now();
    let pool = fishing::odds(
        content,
        spot,
        clock.hour(),
        chrono::Datelike::month(&clock),
        Tackle { luck: rod.luck, bait },
    );
    let Some(fish) = fishing::pick(&pool, rand::random::<f64>()) else {
        return cx
            .update(miss("Nothing bites today", "The sea is quiet. Try again later."))
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
        notes.push("A new species for your fish book!".to_owned());
    } else if log.record {
        notes.push(format!(
            "🏆 A new record! Your biggest was {:.1} cm.",
            log.previous_best
        ));
    }
    if log.week_best && ratio >= 1.15 {
        notes.push("📏 A big one: top of your week here so far.".to_owned());
    }
    if log.bucket >= BUCKET_CAPACITY {
        notes.push("Your bucket is full. Sell some fish with `/fish sell`.".to_owned());
    }
    let notes = if notes.is_empty() {
        String::new()
    } else {
        format!("\n{}", notes.join("\n"))
    };
    let reaction = format!("{:.2} s", late_by as f64 / 1000.0);
    let embed = EmbedBuilder::new()
        .color(if log.first { SUCCESS } else { fish.rarity.color() })
        .title(format!(
            "{}You caught {} {} {}!",
            if log.first { "🆕 " } else { "" },
            if matches!(fish.rarity, Rarity::Uncommon | Rarity::Epic) {
                "an"
            } else {
                "a"
            },
            fish.rarity.label().to_lowercase(),
            fish.name
        ))
        .description(format!(
            "*{}*\n**{size:.1} cm** · worth **{}** shells in your bucket{notes}",
            fish.flavor,
            thousands(value),
        ))
        .footer(EmbedFooterBuilder::new(format!(
            "{} · Reaction {reaction} · Fish book {}/{} · Bucket {}/{BUCKET_CAPACITY}",
            spot.name,
            log.species,
            content.fish.len(),
            log.bucket
        )));
    let mut data = Cx::card(embed, "catch.png", png, Vec::new(), false);
    if let Some(embeds) = data.embeds.as_mut() {
        embeds.extend(log.paid.iter().map(milestone_embed));
    }
    cx.update(data).await?;
    // A new species in the fish book may have just earned a badge.
    customize::announce(cx).await;
    Ok(())
}

/// The embed that tells of a fish-book milestone just paid.
fn milestone_embed(paid: &MilestonePaid) -> Embed {
    let content = Content::get();
    let (kind, name, hint) = match &paid.reward.cosmetic {
        Cosmetic::Badge(id) => (
            "badge",
            content.badge(id).map_or(id.as_str(), |b| b.name.as_str()),
            "/customize badges",
        ),
        Cosmetic::Background(id) => (
            "profile background",
            content.background(id).map_or(id.as_str(), |b| b.name.as_str()),
            "/customize background",
        ),
    };
    let gift = if paid.cosmetic_new {
        format!("the **{name}** {kind}, yours to wear with `{hint}`")
    } else {
        format!("the **{name}** {kind}, which you already had")
    };
    EmbedBuilder::new()
        .color(Rarity::Legendary.color())
        .title(format!("📖 Fish book milestone: {} species!", paid.reward.species))
        .description(format!("**+{} shells** and {gift}.", thousands(paid.reward.shells)))
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
