//! `/fish shop` and `/fish gear`: buying rods and bait, and putting a spot, rod and bait on the line.

use std::str::Split;

use pinchora_db::GearBuy;
use pinchora_game::{
    Content, Rarity,
    fishing::{BAIT_PRICE, Boost, Rod},
};
use twilight_model::{
    channel::message::{
        Component, MessageFlags,
        component::{ActionRow, SelectMenu, SelectMenuOption, SelectMenuType},
    },
    http::interaction::InteractionResponseData,
};
use twilight_util::builder::embed::{EmbedBuilder, EmbedFieldBuilder, EmbedFooterBuilder};

use super::{
    art::{Art, BaitArt, Slot, grid_png},
    player_level,
};
use crate::{
    App, Cx,
    util::{BRAND, thousands},
};

/// The rarity colours the five rods are framed in, plainest first.
const ROD_TIERS: [Rarity; 5] = [
    Rarity::Common,
    Rarity::Uncommon,
    Rarity::Rare,
    Rarity::Epic,
    Rarity::Legendary,
];

fn row(menu: SelectMenu) -> Component {
    Component::ActionRow(ActionRow {
        id: None,
        components: vec![Component::SelectMenu(menu)],
    })
}

fn menu(custom_id: String, placeholder: &str, options: Vec<SelectMenuOption>) -> SelectMenu {
    SelectMenu {
        channel_types: None,
        custom_id,
        default_values: None,
        disabled: options.is_empty(),
        id: None,
        kind: SelectMenuType::Text,
        max_values: Some(1),
        min_values: Some(1),
        options: Some(options),
        placeholder: Some(placeholder.to_owned()),
        required: None,
    }
}

fn option(label: String, value: String, description: String, default: bool) -> SelectMenuOption {
    SelectMenuOption {
        default,
        description: Some(description.chars().take(100).collect()),
        emoji: None,
        label,
        value,
    }
}

/// "reel 3.0 s · luck +10%".
fn rod_stats(rod: &Rod) -> String {
    format!(
        "reel {:.1} s · rare luck +{}%",
        rod.window_ms as f64 / 1000.0,
        ((rod.luck - 1.0) * 100.0).round()
    )
}

// /Shop

pub(super) async fn shop(cx: &Cx) -> anyhow::Result<()> {
    let data = shop_panel(&cx.app, cx.author()?.id.get(), "").await?;
    cx.reply(data).await
}

async fn shop_panel(app: &App, user: u64, note: &str) -> anyhow::Result<InteractionResponseData> {
    let content = Content::get();
    let gear = &content.fishing;
    let balance = app.db.user(user).await?.credits;
    let owned_rods = app.db.owned_rods(user).await?;
    let stock = app.db.bait_stock(user).await?;
    let on_line = app.db.fishing_gear(user).await?;
    let have = |id: &str| stock.iter().find(|(bait, _)| bait == id).map_or(0, |(_, n)| *n);

    let mut slots = Vec::new();
    let mut options = Vec::new();
    for (n, rod) in gear.rods.iter().enumerate() {
        let owned = rod.price == 0 || owned_rods.contains(&rod.id);
        let (line1, shell_price) = match (on_line.rod == rod.id, owned) {
            (true, _) => ("EQUIPPED".to_owned(), false),
            (false, true) => ("OWNED".to_owned(), false),
            (false, false) => (thousands(rod.price), true),
        };
        slots.push(Slot {
            art: Art::Rod(n),
            name: rod.name.clone(),
            line1,
            line2: format!(
                "REEL {:.1} S · LUCK +{}%",
                rod.window_ms as f64 / 1000.0,
                ((rod.luck - 1.0) * 100.0).round()
            ),
            bar: None,
            frame: ROD_TIERS[n % ROD_TIERS.len()].color(),
            badge: "ROD".to_owned(),
            shell_price,
        });
        if !owned {
            options.push(option(
                rod.name.clone(),
                format!("r:{}", rod.id),
                format!("{} shells · {}", thousands(rod.price), rod_stats(rod)),
                false,
            ));
        }
    }
    for bait in &gear.baits {
        let frame = match bait.boost {
            Boost::Rarity { rarity, .. } => rarity.color(),
            Boost::Specials { .. } => Rarity::Epic.color(),
        };
        slots.push(Slot {
            art: Art::Bait(BaitArt::of(bait)),
            name: bait.name.clone(),
            line1: thousands(bait.price),
            line2: format!("PACK OF {} · HAVE {}", bait.pack, have(&bait.id)),
            bar: None,
            frame,
            badge: "BAIT".to_owned(),
            shell_price: true,
        });
        options.push(option(
            format!("{} (pack of {})", bait.name, bait.pack),
            format!("b:{}", bait.id),
            format!("{} shells · {}", thousands(bait.price), bait.blurb),
            false,
        ));
    }
    let png = grid_png(
        app,
        "Tackle shop".to_owned(),
        format!("YOU HAVE {} SHELLS", thousands(balance)),
        format!("PLAIN BAIT IS {BAIT_PRICE} SHELLS A CAST · PUT GEAR ON WITH /FISH GEAR"),
        slots,
    )
    .await?;
    let description = if note.is_empty() {
        "Rods are bought once; better ones give you more time to Reel! and better odds at rare fish. \
         Bait comes in packs and is used up one a cast, instead of plain bait. Silver Lure and Tide Crumbs are \
         for hunting rare and signature fish, not for income: they lose shells at the shallow spots, and \
         Tide Crumbs cuts the wage at the Abyss."
            .to_owned()
    } else {
        note.to_owned()
    };
    let embed = EmbedBuilder::new()
        .color(BRAND)
        .title("🎣 Tackle shop")
        .description(description);
    Ok(Cx::card(
        embed,
        "fish-shop.png",
        png,
        vec![row(menu(
            format!("fx:p:{user}"),
            "Buy a rod or a pack of bait…",
            options,
        ))],
        true,
    ))
}

// /Gear

pub(super) async fn panel(cx: &Cx) -> anyhow::Result<()> {
    let data = gear_panel(&cx.app, cx.author()?.id.get(), "").await?;
    cx.reply(data).await
}

async fn gear_panel(app: &App, user: u64, note: &str) -> anyhow::Result<InteractionResponseData> {
    let content = Content::get();
    let gear = &content.fishing;
    let level = player_level(app, user).await?;
    let saved = app.db.fishing_gear(user).await?;
    let owned_rods = app.db.owned_rods(user).await?;
    let stock = app.db.bait_stock(user).await?;
    let spot = gear.spot_for(&saved.spot, level);
    let owned = |rod: &Rod| rod.price == 0 || owned_rods.contains(&rod.id);
    let rod = gear
        .rod(&saved.rod)
        .filter(|r| owned(r))
        .unwrap_or_else(|| gear.starter_rod());
    let bait = saved
        .bait
        .as_deref()
        .and_then(|id| gear.bait(id))
        .and_then(|b| stock.iter().find(|(id, _)| *id == b.id).map(|(_, left)| (b, *left)));

    let bait_text = match bait {
        Some((bait, left)) => format!("**{}** ×{left}\n{}", bait.name, bait.blurb),
        None => format!("**Plain bait**\n{BAIT_PRICE} shells a cast"),
    };
    let next = gear
        .spots
        .iter()
        .find(|s| !gear.is_open(s, level))
        .map_or_else(String::new, |s| format!(" · {} opens at level {}", s.name, s.level));
    let embed = EmbedBuilder::new()
        .color(BRAND)
        .title("🎣 Your tackle")
        .description(if note.is_empty() {
            "Pick from the menus; the next cast uses them."
        } else {
            note
        })
        .field(EmbedFieldBuilder::new("Spot", format!("**{}**\n{}", spot.name, spot.blurb)).inline())
        .field(EmbedFieldBuilder::new("Rod", format!("**{}**\n{}", rod.name, rod_stats(rod))).inline())
        .field(EmbedFieldBuilder::new("Bait", bait_text).inline())
        .footer(EmbedFooterBuilder::new(format!("Level {level}{next}")))
        .build();

    let spots = gear
        .spots
        .iter()
        .map(|s| {
            let locked = !gear.is_open(s, level);
            option(
                if locked {
                    format!("🔒 {}", s.name)
                } else {
                    s.name.clone()
                },
                s.id.clone(),
                if locked {
                    format!("Opens at level {}", s.level)
                } else {
                    s.blurb.clone()
                },
                s.id == spot.id,
            )
        })
        .collect();
    let rods = gear
        .rods
        .iter()
        .filter(|r| owned(r))
        .map(|r| option(r.name.clone(), r.id.clone(), rod_stats(r), r.id == rod.id))
        .collect();
    let mut baits = vec![option(
        "Plain bait".to_owned(),
        "plain".to_owned(),
        format!("{BAIT_PRICE} shells a cast"),
        bait.is_none(),
    )];
    for (id, left) in &stock {
        if let Some(b) = gear.bait(id) {
            baits.push(option(
                format!("{} ×{left}", b.name),
                b.id.clone(),
                b.blurb.clone(),
                bait.is_some_and(|(on, _)| on.id == b.id),
            ));
        }
    }
    Ok(InteractionResponseData {
        embeds: Some(vec![embed]),
        components: Some(vec![
            row(menu(format!("fx:s:{user}"), "Where to fish…", spots)),
            row(menu(format!("fx:r:{user}"), "Which rod…", rods)),
            row(menu(format!("fx:b:{user}"), "Which bait…", baits)),
        ]),
        flags: Some(MessageFlags::EPHEMERAL),
        ..Default::default()
    })
}

// /Clicks

/// `fx:<what>:<user>` menus: `p` buys in the shop, `s`, `r` and `b` set the spot, rod and bait.
pub(crate) async fn on_gear(cx: &Cx, mut args: Split<'_, char>, value: Option<&str>) -> anyhow::Result<()> {
    let what = args.next().unwrap_or_default();
    let owner: u64 = args.next().and_then(|v| v.parse().ok()).unwrap_or(0);
    let user = cx.author()?.id.get();
    if user != owner {
        return cx
            .reply_ephemeral("That's someone else's panel. Open yours with `/fish shop` or `/fish gear`.")
            .await;
    }
    let Some(value) = value else {
        return cx.reply_ephemeral("Pick something from the menu.").await;
    };
    let gear = &Content::get().fishing;
    if what == "p" {
        let note = buy(&cx.app, user, value).await?;
        return cx.update(shop_panel(&cx.app, user, &note).await?).await;
    }
    let note = match what {
        "s" => match gear.spot(value) {
            Some(spot) => {
                let level = player_level(&cx.app, user).await?;
                if !gear.is_open(spot, level) {
                    format!(
                        "The **{}** opens at level {} and you are level {level}.",
                        spot.name, spot.level
                    )
                } else {
                    cx.app.db.set_fishing_spot(user, &spot.id).await?;
                    format!("Fishing at the **{}** from now on.", spot.name)
                }
            }
            None => "That spot is not on the map.".to_owned(),
        },
        "r" => match gear.rod(value) {
            Some(rod) if cx.app.db.equip_rod(user, &rod.id, rod.price == 0).await? => {
                format!("The **{}** is on the line.", rod.name)
            }
            Some(rod) => format!("The {} is not yours yet. Buy it with `/fish shop`.", rod.name),
            None => "That rod is not in the shop.".to_owned(),
        },
        "b" if value == "plain" => {
            cx.app.db.equip_bait(user, None).await?;
            format!("Plain bait it is: {BAIT_PRICE} shells a cast.")
        }
        "b" => match gear.bait(value) {
            Some(bait) if cx.app.db.equip_bait(user, Some(&bait.id)).await? => {
                format!("**{}** is on the hook.", bait.name)
            }
            Some(bait) => format!("You have no {} left. Buy more with `/fish shop`.", bait.name),
            None => "That bait is not in the shop.".to_owned(),
        },
        _ => return cx.reply_ephemeral("This menu has expired.").await,
    };
    cx.update(gear_panel(&cx.app, user, &note).await?).await
}

/// Buys what the shop menu picked (`r:<rod>` or `b:<bait>`) and says how it went.
async fn buy(app: &App, user: u64, value: &str) -> anyhow::Result<String> {
    let gear = &Content::get().fishing;
    let (kind, id) = value.split_once(':').unwrap_or_default();
    Ok(match kind {
        "r" => match gear.rod(id).filter(|r| r.price > 0) {
            Some(rod) => match app.db.buy_rod(user, &rod.id, rod.price).await? {
                GearBuy::Bought { .. } => format!(
                    "Bought the **{}** for {} shells. Put it on with `/fish gear`.",
                    rod.name,
                    thousands(rod.price)
                ),
                GearBuy::AlreadyOwned => format!("You already own the **{}**.", rod.name),
                GearBuy::Insufficient { balance } => format!(
                    "The **{}** costs {} shells and you have {}.",
                    rod.name,
                    thousands(rod.price),
                    thousands(balance)
                ),
            },
            None => "That rod is not in the shop.".to_owned(),
        },
        "b" => match gear.bait(id) {
            Some(bait) => match app.db.buy_bait(user, &bait.id, bait.pack as i32, bait.price).await? {
                GearBuy::Bought { owned, .. } => format!(
                    "Bought {} **{}** for {} shells; you have {owned}. Put them on with `/fish gear`.",
                    bait.pack,
                    bait.name,
                    thousands(bait.price)
                ),
                GearBuy::AlreadyOwned => String::new(),
                GearBuy::Insufficient { balance } => format!(
                    "A pack of **{}** costs {} shells and you have {}.",
                    bait.name,
                    thousands(bait.price),
                    thousands(balance)
                ),
            },
            None => "That bait is not in the shop.".to_owned(),
        },
        _ => "This menu has expired.".to_owned(),
    })
}
