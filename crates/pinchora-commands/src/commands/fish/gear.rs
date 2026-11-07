//! `/fish shop` and `/fish gear`: buying rods and bait, and putting a spot, rod and bait on the line.

use std::str::Split;

use pinchora_core::{i18n::Lang, t};
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
    util::{BRAND, thousands_in},
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

/// The blurb of a spot, rod or bait in `lang`; `fallback` is the English one.
fn blurb(content: &Content, id: &str, lang: Lang, fallback: &str) -> String {
    content
        .description(id, lang)
        .map_or_else(|| fallback.to_owned(), |text| text.into_owned())
}

/// "reel 3.0 s · luck +10%".
fn rod_stats(lang: Lang, rod: &Rod) -> String {
    t!(
        lang,
        "fish.gear.rod-stats",
        reel = pinchora_core::server_stats::decimal_in(lang, rod.window_ms as f64 / 1000.0, 1),
        luck = ((rod.luck - 1.0) * 100.0).round()
    )
}

// /Shop

pub(super) async fn shop(cx: &Cx) -> anyhow::Result<()> {
    let data = shop_panel(&cx.app, cx.lang(), cx.author()?.id.get(), "").await?;
    cx.reply(data).await
}

async fn shop_panel(app: &App, lang: Lang, user: u64, note: &str) -> anyhow::Result<InteractionResponseData> {
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
            (true, _) => (t!(lang, "fish.shop.equipped"), false),
            (false, true) => (t!(lang, "fish.shop.owned"), false),
            (false, false) => (thousands_in(lang, rod.price), true),
        };
        slots.push(Slot {
            art: Art::Rod(n),
            name: content.name(&rod.id, lang).into_owned(),
            line1,
            line2: t!(
                lang,
                "fish.shop.rod-line",
                reel = pinchora_core::server_stats::decimal_in(lang, rod.window_ms as f64 / 1000.0, 1),
                luck = ((rod.luck - 1.0) * 100.0).round()
            ),
            bar: None,
            frame: ROD_TIERS[n % ROD_TIERS.len()].color(),
            badge: t!(lang, "fish.shop.rod-badge"),
            shell_price,
        });
        if !owned {
            options.push(option(
                content.name(&rod.id, lang).into_owned(),
                format!("r:{}", rod.id),
                t!(
                    lang,
                    "fish.shop.price-line",
                    price = thousands_in(lang, rod.price),
                    text = rod_stats(lang, rod)
                ),
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
            name: content.name(&bait.id, lang).into_owned(),
            line1: thousands_in(lang, bait.price),
            line2: t!(lang, "fish.shop.bait-line", pack = bait.pack, have = have(&bait.id)),
            bar: None,
            frame,
            badge: t!(lang, "fish.shop.bait-badge"),
            shell_price: true,
        });
        options.push(option(
            t!(
                lang,
                "fish.shop.bait-option",
                name = content.name(&bait.id, lang),
                pack = bait.pack
            ),
            format!("b:{}", bait.id),
            t!(
                lang,
                "fish.shop.price-line",
                price = thousands_in(lang, bait.price),
                text = blurb(content, &bait.id, lang, &bait.blurb)
            ),
            false,
        ));
    }
    let png = grid_png(
        app,
        lang,
        lang.upper(&t!(lang, "fish.shop.title")),
        t!(lang, "fish.shop.balance", shells = thousands_in(lang, balance)),
        t!(lang, "fish.shop.footer", price = BAIT_PRICE),
        slots,
    )
    .await?;
    let description = if note.is_empty() {
        t!(
            lang,
            "fish.shop.intro",
            lure = content.name("silver_lure", lang),
            crumbs = content.name("tide_crumbs", lang),
            abyss = content.name("abyss", lang)
        )
    } else {
        note.to_owned()
    };
    let embed = EmbedBuilder::new()
        .color(BRAND)
        .title(format!("🎣 {}", t!(lang, "fish.shop.title")))
        .description(description);
    Ok(Cx::card(
        embed,
        "fish-shop.png",
        png,
        vec![row(menu(format!("fx:p:{user}"), &t!(lang, "fish.shop.pick"), options))],
        true,
    ))
}

// /Gear

pub(super) async fn panel(cx: &Cx) -> anyhow::Result<()> {
    let data = gear_panel(&cx.app, cx.lang(), cx.author()?.id.get(), "").await?;
    cx.reply(data).await
}

async fn gear_panel(app: &App, lang: Lang, user: u64, note: &str) -> anyhow::Result<InteractionResponseData> {
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

    let plain = t!(lang, "fish.plain-bait");
    let per_cast = t!(lang, "fish.gear.per-cast", price = BAIT_PRICE);
    let bait_text = match bait {
        Some((bait, left)) => format!(
            "**{}** ×{left}\n{}",
            content.name(&bait.id, lang),
            blurb(content, &bait.id, lang, &bait.blurb)
        ),
        None => format!("**{plain}**\n{per_cast}"),
    };
    let next = gear
        .spots
        .iter()
        .find(|s| !gear.is_open(s, level))
        .map_or_else(String::new, |s| {
            t!(
                lang,
                "fish.gear.next-spot",
                spot = content.name(&s.id, lang),
                need = s.level
            )
        });
    let embed = EmbedBuilder::new()
        .color(BRAND)
        .title(t!(lang, "fish.gear.title"))
        .description(if note.is_empty() {
            t!(lang, "fish.gear.hint")
        } else {
            note.to_owned()
        })
        .field(
            EmbedFieldBuilder::new(
                t!(lang, "fish.gear.field-spot"),
                format!(
                    "**{}**\n{}",
                    content.name(&spot.id, lang),
                    blurb(content, &spot.id, lang, &spot.blurb)
                ),
            )
            .inline(),
        )
        .field(
            EmbedFieldBuilder::new(
                t!(lang, "fish.gear.field-rod"),
                format!("**{}**\n{}", content.name(&rod.id, lang), rod_stats(lang, rod)),
            )
            .inline(),
        )
        .field(EmbedFieldBuilder::new(t!(lang, "fish.gear.field-bait"), bait_text).inline())
        .footer(EmbedFooterBuilder::new(t!(
            lang,
            "fish.gear.footer",
            level = level,
            next = next
        )))
        .build();

    let spots = gear
        .spots
        .iter()
        .map(|s| {
            let locked = !gear.is_open(s, level);
            let name = content.name(&s.id, lang);
            option(
                if locked {
                    format!("🔒 {name}")
                } else {
                    name.into_owned()
                },
                s.id.clone(),
                if locked {
                    t!(lang, "fish.gear.opens-at", level = s.level)
                } else {
                    blurb(content, &s.id, lang, &s.blurb)
                },
                s.id == spot.id,
            )
        })
        .collect();
    let rods = gear
        .rods
        .iter()
        .filter(|r| owned(r))
        .map(|r| {
            option(
                content.name(&r.id, lang).into_owned(),
                r.id.clone(),
                rod_stats(lang, r),
                r.id == rod.id,
            )
        })
        .collect();
    let mut baits = vec![option(plain, "plain".to_owned(), per_cast, bait.is_none())];
    for (id, left) in &stock {
        if let Some(b) = gear.bait(id) {
            baits.push(option(
                format!("{} ×{left}", content.name(&b.id, lang)),
                b.id.clone(),
                blurb(content, &b.id, lang, &b.blurb),
                bait.is_some_and(|(on, _)| on.id == b.id),
            ));
        }
    }
    Ok(InteractionResponseData {
        embeds: Some(vec![embed]),
        components: Some(vec![
            row(menu(format!("fx:s:{user}"), &t!(lang, "fish.gear.pick-spot"), spots)),
            row(menu(format!("fx:r:{user}"), &t!(lang, "fish.gear.pick-rod"), rods)),
            row(menu(format!("fx:b:{user}"), &t!(lang, "fish.gear.pick-bait"), baits)),
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
    let lang = cx.lang();
    if user != owner {
        return cx.reply_ephemeral(t!(lang, "fish.gear.not-yours")).await;
    }
    let Some(value) = value else {
        return cx.reply_ephemeral(t!(lang, "fish.gear.pick-something")).await;
    };
    let content = Content::get();
    let gear = &content.fishing;
    if what == "p" {
        let note = buy(&cx.app, lang, user, value).await?;
        return cx.update(shop_panel(&cx.app, lang, user, &note).await?).await;
    }
    let note = match what {
        "s" => match gear.spot(value) {
            Some(spot) => {
                let level = player_level(&cx.app, user).await?;
                if !gear.is_open(spot, level) {
                    t!(
                        lang,
                        "fish.gear.spot-locked",
                        spot = content.name(&spot.id, lang),
                        need = spot.level,
                        level = level
                    )
                } else {
                    cx.app.db.set_fishing_spot(user, &spot.id).await?;
                    t!(lang, "fish.gear.spot-set", spot = content.name(&spot.id, lang))
                }
            }
            None => t!(lang, "fish.gear.no-spot"),
        },
        "r" => match gear.rod(value) {
            Some(rod) if cx.app.db.equip_rod(user, &rod.id, rod.price == 0).await? => {
                t!(lang, "fish.gear.rod-on", rod = content.name(&rod.id, lang))
            }
            Some(rod) => t!(lang, "fish.gear.rod-not-owned", rod = content.name(&rod.id, lang)),
            None => t!(lang, "fish.gear.no-rod"),
        },
        "b" if value == "plain" => {
            cx.app.db.equip_bait(user, None).await?;
            t!(lang, "fish.gear.plain-set", price = BAIT_PRICE)
        }
        "b" => match gear.bait(value) {
            Some(bait) if cx.app.db.equip_bait(user, Some(&bait.id)).await? => {
                t!(lang, "fish.gear.bait-on", bait = content.name(&bait.id, lang))
            }
            Some(bait) => t!(lang, "fish.gear.bait-out", bait = content.name(&bait.id, lang)),
            None => t!(lang, "fish.gear.no-bait"),
        },
        _ => return cx.reply_ephemeral(t!(lang, "fish.gear.expired")).await,
    };
    cx.update(gear_panel(&cx.app, lang, user, &note).await?).await
}

/// Buys what the shop menu picked (`r:<rod>` or `b:<bait>`) and says how it went.
async fn buy(app: &App, lang: Lang, user: u64, value: &str) -> anyhow::Result<String> {
    let content = Content::get();
    let gear = &content.fishing;
    let (kind, id) = value.split_once(':').unwrap_or_default();
    Ok(match kind {
        "r" => match gear.rod(id).filter(|r| r.price > 0) {
            Some(rod) => match app.db.buy_rod(user, &rod.id, rod.price).await? {
                GearBuy::Bought { .. } => t!(
                    lang,
                    "fish.buy.rod-bought",
                    rod = content.name(&rod.id, lang),
                    price = thousands_in(lang, rod.price)
                ),
                GearBuy::AlreadyOwned => t!(lang, "fish.buy.rod-owned", rod = content.name(&rod.id, lang)),
                GearBuy::Insufficient { balance } => t!(
                    lang,
                    "fish.buy.rod-poor",
                    rod = content.name(&rod.id, lang),
                    price = thousands_in(lang, rod.price),
                    balance = thousands_in(lang, balance)
                ),
            },
            None => t!(lang, "fish.gear.no-rod"),
        },
        "b" => match gear.bait(id) {
            Some(bait) => match app.db.buy_bait(user, &bait.id, bait.pack as i32, bait.price).await? {
                GearBuy::Bought { owned, .. } => t!(
                    lang,
                    "fish.buy.bait-bought",
                    pack = bait.pack,
                    bait = content.name(&bait.id, lang),
                    price = thousands_in(lang, bait.price),
                    owned = owned
                ),
                GearBuy::AlreadyOwned => String::new(),
                GearBuy::Insufficient { balance } => t!(
                    lang,
                    "fish.buy.bait-poor",
                    bait = content.name(&bait.id, lang),
                    price = thousands_in(lang, bait.price),
                    balance = thousands_in(lang, balance)
                ),
            },
            None => t!(lang, "fish.gear.no-bait"),
        },
        _ => t!(lang, "fish.gear.expired"),
    })
}
