//! `/house decorate`: buy and apply wallpapers and floors.

use std::str::Split;

use pinchora_db::{BuyOutcome, Currency as DbCurrency};
use pinchora_game::{Content, Currency, Surface, house};
use twilight_model::{
    channel::message::{
        Component,
        component::{ActionRow, Button, ButtonStyle, SelectMenu, SelectMenuOption, SelectMenuType},
    },
    http::interaction::InteractionResponseData,
};
use twilight_util::builder::embed::EmbedBuilder;

use super::house::ensure_house;
use crate::{
    Cx,
    game::scene::{RoomView, render_view},
    util::{BRAND, thousands},
};

const PAGE: usize = 25;

fn price_text(s: &Surface) -> String {
    match s.currency {
        Currency::Shells => format!("{} shells", thousands(s.price)),
        Currency::Pearls => format!("{} pearls", thousands(s.price)),
    }
}

fn menu(custom_id: String, placeholder: String, list: &[Surface], page: usize, current: &str) -> Component {
    let options = list
        .iter()
        .skip(page * PAGE)
        .take(PAGE)
        .map(|s| SelectMenuOption {
            default: s.id == current,
            description: Some(format!("{} · {}", s.rarity.label(), price_text(s))),
            emoji: None,
            label: s.name.clone(),
            value: s.id.clone(),
        })
        .collect();
    Component::ActionRow(ActionRow {
        id: None,
        components: vec![Component::SelectMenu(SelectMenu {
            channel_types: None,
            custom_id,
            default_values: None,
            disabled: false,
            id: None,
            kind: SelectMenuType::Text,
            max_values: Some(1),
            min_values: Some(1),
            options: Some(options),
            placeholder: Some(placeholder),
            required: None,
        })],
    })
}

pub(crate) async fn panel(cx: &Cx, room: i16, page: usize, note: &str) -> anyhow::Result<InteractionResponseData> {
    let user = cx.author()?.id.get();
    let content = Content::get();
    let (house_row, rooms) = ensure_house(&cx.app, user).await?;
    let room = room.clamp(0, rooms.len() as i16 - 1);
    let row = rooms[room as usize].clone();
    let tier = house::tier(house_row.tier);
    let pages = content
        .surfaces
        .wallpapers
        .len()
        .max(content.surfaces.floors.len())
        .div_ceil(PAGE)
        .max(1);
    let page = page % pages;
    let (wp, fl) = (row.wallpaper.clone(), row.floor.clone());
    let placements = cx.app.db.placements(user, room).await?;
    let sprites = cx.app.sprites.clone();
    let view = RoomView {
        width: tier.width,
        depth: tier.depth,
        room: row,
        placements,
        cursor: None,
        selected: None,
        edit: false,
        pets: Vec::new(),
        seed: 0,
    };
    let png = cx.app.render(move |_| render_view(&sprites, &view)).await?;
    let name = |list: &[Surface], id: &str| {
        list.iter()
            .find(|s| s.id == id)
            .map_or(id.to_owned(), |s| s.name.clone())
    };
    let embed = EmbedBuilder::new().color(BRAND).title(format!("Decorating room {}", room + 1)).description(format!(
        "{note}\nWallpaper: **{}** · Floor: **{}**\nA new pattern is bought the first time you use it; switching back is free.",
        name(&content.surfaces.wallpapers, &wp),
        name(&content.surfaces.floors, &fl)
    ));
    let components = vec![
        menu(
            format!("dw:{room}:{page}"),
            format!("Wallpaper (page {}/{pages})…", page + 1),
            &content.surfaces.wallpapers,
            page,
            &wp,
        ),
        menu(
            format!("df:{room}:{page}"),
            format!("Floor (page {}/{pages})…", page + 1),
            &content.surfaces.floors,
            page,
            &fl,
        ),
        Component::ActionRow(ActionRow {
            id: None,
            components: vec![
                Component::Button(Button {
                    custom_id: Some(format!("dn:{room}:{}", page + 1)),
                    disabled: pages < 2,
                    emoji: None,
                    id: None,
                    label: Some("More patterns ▶".to_owned()),
                    sku_id: None,
                    style: ButtonStyle::Secondary,
                    url: None,
                }),
                Component::Button(Button {
                    custom_id: Some(format!("dn:{}:0", (room + 1) % rooms.len() as i16)),
                    disabled: rooms.len() < 2,
                    emoji: None,
                    id: None,
                    label: Some("Next room".to_owned()),
                    sku_id: None,
                    style: ButtonStyle::Secondary,
                    url: None,
                }),
            ],
        }),
    ];
    Ok(Cx::card(embed, "room.png", png, components, true))
}

/// `dw` / `df` selects and the `dn` page button.
pub(crate) async fn on_component(
    cx: &Cx,
    kind: &str,
    mut args: Split<'_, char>,
    value: Option<&str>,
) -> anyhow::Result<()> {
    let room: i16 = args.next().and_then(|v| v.parse().ok()).unwrap_or(0);
    let page: usize = args.next().and_then(|v| v.parse().ok()).unwrap_or(0);
    if kind == "dn" {
        let data = panel(cx, room, page, "").await?;
        return cx.update(data).await;
    }
    let content = Content::get();
    let wall = kind == "dw";
    let list = if wall {
        &content.surfaces.wallpapers
    } else {
        &content.surfaces.floors
    };
    let Some(surface) = value.and_then(|v| list.iter().find(|s| s.id == v)) else {
        return cx.reply_ephemeral("Pick a pattern.").await;
    };
    let user = cx.author()?.id.get();
    let key = format!("{}:{}", if wall { "wp" } else { "fl" }, surface.id);
    let mut note = String::new();
    if !cx.app.db.owns(user, &key).await? {
        let currency = match surface.currency {
            Currency::Shells => DbCurrency::Shells,
            Currency::Pearls => DbCurrency::Pearls,
        };
        match cx.app.db.buy_item(user, &key, surface.price, currency, 1).await? {
            BuyOutcome::Bought { .. } => note = format!("Bought **{}** for {}.", surface.name, price_text(surface)),
            _ => {
                return cx
                    .reply_ephemeral(format!("**{}** costs {}.", surface.name, price_text(surface)))
                    .await;
            }
        }
    }
    let (wp, fl) = if wall {
        (Some(surface.id.as_str()), None)
    } else {
        (None, Some(surface.id.as_str()))
    };
    cx.app.db.set_room_surfaces(user, room, wp, fl).await?;
    let data = panel(cx, room, page, &note).await?;
    cx.update(data).await
}
