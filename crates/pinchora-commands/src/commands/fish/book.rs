//! `/fish book`: every species, the record sizes and the milestone rewards.

use std::{collections::HashMap, str::Split};

use pinchora_db::FishBookRow;
use pinchora_game::{Content, Fish, fishing::Reward};
use twilight_model::{
    channel::message::{
        Component,
        component::{ActionRow, Button, ButtonStyle},
    },
    http::interaction::InteractionResponseData,
};
use twilight_util::builder::embed::EmbedBuilder;

use super::art::{Art, Slot, grid_png};
use crate::{
    App, Cx,
    util::{BRAND, thousands},
};

const PER_PAGE: usize = 8;

/// The frame of a species not caught yet.
const UNSEEN: u32 = 0x3a4a7a;

pub(super) async fn view(cx: &Cx) -> anyhow::Result<()> {
    let data = page(&cx.app, cx.author()?.id.get(), 0).await?;
    cx.reply(data).await
}

/// The spots a species lives at, shallowest first, as "SHORE · REEF".
fn habitat(fish: &Fish) -> String {
    let spots: Vec<_> = Content::get()
        .fishing
        .spots
        .iter()
        .filter(|s| fish.spots.contains_key(&s.id))
        .map(|s| s.name.to_uppercase())
        .collect();
    match spots.len() {
        0..=2 => spots.join(" · "),
        _ => format!("{} +{}", spots[0], spots.len() - 1),
    }
}

async fn page(app: &App, owner: u64, page: usize) -> anyhow::Result<InteractionResponseData> {
    let content = Content::get();
    let book: HashMap<String, FishBookRow> = app
        .db
        .fish_book(owner)
        .await?
        .into_iter()
        .map(|row| (row.fish_id.clone(), row))
        .collect();
    let species = book.len() as i64;
    let pages = content.fish.len().div_ceil(PER_PAGE).max(1);
    let page = page.min(pages - 1);
    let slots = content
        .fish
        .iter()
        .enumerate()
        .skip(page * PER_PAGE)
        .take(PER_PAGE)
        .map(|(n, fish)| match book.get(&fish.id) {
            Some(entry) => Slot {
                art: Art::Fish {
                    id: fish.id.clone(),
                    hidden: false,
                },
                name: fish.name.clone(),
                line1: format!("RECORD {:.1} CM", entry.best_cm),
                line2: format!("CAUGHT ×{}", entry.caught),
                // How close the record is to the biggest the species grows.
                bar: Some((entry.best_cm - fish.min_cm) / (fish.max_cm - fish.min_cm)),
                frame: fish.rarity.color(),
                badge: format!("#{:02}", n + 1),
                shell_price: false,
            },
            None => Slot {
                art: Art::Fish {
                    id: fish.id.clone(),
                    hidden: true,
                },
                name: "? ? ?".to_owned(),
                line1: "NOT CAUGHT".to_owned(),
                line2: habitat(fish),
                bar: None,
                frame: UNSEEN,
                badge: format!("#{:02}", n + 1),
                shell_price: false,
            },
        })
        .collect();
    let next = content.fishing.next_milestone(species);
    let png = grid_png(
        app,
        "Fish book".to_owned(),
        format!("{species}/{} SPECIES · PAGE {}/{pages}", content.fish.len(), page + 1),
        match next {
            Some(m) => format!("NEXT REWARD AT {} SPECIES", m.species),
            None => "THE WHOLE BOOK IS FILLED".to_owned(),
        },
        slots,
    )
    .await?;

    let rewards: Vec<String> = content
        .fishing
        .milestones
        .iter()
        .map(|m| {
            let (kind, name) = match &m.reward {
                Reward::Badge { id } => ("badge", content.badge(id).map_or(id.as_str(), |b| b.name.as_str())),
                Reward::Background { id } => (
                    "background",
                    content.background(id).map_or(id.as_str(), |b| b.name.as_str()),
                ),
            };
            format!(
                "{} **{} species**: {} shells and the {name} {kind}",
                if species >= m.species { "✅" } else { "▫️" },
                m.species,
                thousands(m.shells)
            )
        })
        .collect();
    let embed = EmbedBuilder::new()
        .color(BRAND)
        .title("📖 Fish book")
        .description(rewards.join("\n"));
    let button = |label: &str, target: usize, disabled: bool| {
        Component::Button(Button {
            custom_id: Some(format!("fz:b:{owner}:{target}")),
            disabled,
            emoji: None,
            id: None,
            label: Some(label.to_owned()),
            sku_id: None,
            style: ButtonStyle::Secondary,
            url: None,
        })
    };
    Ok(Cx::card(
        embed,
        "fish-book.png",
        png,
        vec![Component::ActionRow(ActionRow {
            id: None,
            components: vec![
                button("◀ Page", page.saturating_sub(1), page == 0),
                button("Page ▶", page + 1, page + 1 >= pages),
            ],
        })],
        false,
    ))
}

/// `fz:b:<user>:<page>`: the page buttons of the book.
pub(super) async fn on_page(cx: &Cx, mut args: Split<'_, char>) -> anyhow::Result<()> {
    let owner: u64 = args.next().and_then(|v| v.parse().ok()).unwrap_or(0);
    let target: usize = args.next().and_then(|v| v.parse().ok()).unwrap_or(0);
    if cx.author()?.id.get() != owner {
        return cx
            .reply_ephemeral("That's someone else's book. Open yours with `/fish book`.")
            .await;
    }
    cx.update(page(&cx.app, owner, target).await?).await
}
