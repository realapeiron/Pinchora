//! `/fish book`: every species, the record sizes and the milestone rewards.

use std::{collections::HashMap, str::Split};

use pinchora_core::infra::i18n::Lang;
use pinchora_core::t;
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

use crate::App;
use crate::Cx;
use crate::commands::games::fish::art::Art;
use crate::commands::games::fish::art::Slot;
use crate::commands::games::fish::art::grid_png;
use crate::infra::util::BRAND;
use crate::infra::util::thousands_in;

const PER_PAGE: usize = 8;

/// The frame of a species not caught yet.
const UNSEEN: u32 = 0x3a4a7a;

pub(super) async fn view(cx: &Cx) -> anyhow::Result<()> {
    let data = page(&cx.app, cx.lang(), cx.author()?.id.get(), 0).await?;
    cx.reply(data).await
}

/// The spots a species lives at, shallowest first, as "SHORE · REEF".
fn habitat(fish: &Fish, lang: Lang) -> String {
    let content = Content::get();
    let spots: Vec<_> = content
        .fishing
        .spots
        .iter()
        .filter(|s| fish.spots.contains_key(&s.id))
        .map(|s| lang.upper(&content.name(&s.id, lang)))
        .collect();
    match spots.len() {
        0..=2 => spots.join(" · "),
        _ => format!("{} +{}", spots[0], spots.len() - 1),
    }
}

async fn page(app: &App, lang: Lang, owner: u64, page: usize) -> anyhow::Result<InteractionResponseData> {
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
                name: content.name(&fish.id, lang).into_owned(),
                line1: t!(
                    lang,
                    "fish.book.record",
                    size = pinchora_core::admin::server_stats::decimal_in(lang, entry.best_cm, 1)
                ),
                line2: t!(lang, "fish.book.caught", fish = entry.caught),
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
                line1: t!(lang, "fish.book.not-caught"),
                line2: habitat(fish, lang),
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
        lang,
        lang.upper(&t!(lang, "fish.book.title")),
        t!(
            lang,
            "fish.book.subtitle",
            species = species,
            total = content.fish.len(),
            page = page + 1,
            pages = pages
        ),
        match next {
            Some(m) => t!(lang, "fish.book.next", species = m.species),
            None => t!(lang, "fish.book.complete"),
        },
        slots,
    )
    .await?;

    let rewards: Vec<String> = content
        .fishing
        .milestones
        .iter()
        .map(|m| {
            let (key, id) = match &m.reward {
                Reward::Badge { id } => ("fish.book.reward-badge", id),
                Reward::Background { id } => ("fish.book.reward-background", id),
            };
            t!(
                lang,
                key,
                mark = if species >= m.species { "✅" } else { "▫️" },
                species = m.species,
                shells = thousands_in(lang, m.shells),
                name = content.name(id, lang)
            )
        })
        .collect();
    let embed = EmbedBuilder::new()
        .color(BRAND)
        .title(format!("📖 {}", t!(lang, "fish.book.title")))
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
                button(&t!(lang, "customize.prev-page"), page.saturating_sub(1), page == 0),
                button(&t!(lang, "customize.next-page"), page + 1, page + 1 >= pages),
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
        return cx.reply_ephemeral(t!(cx.lang(), "fish.book.not-yours")).await;
    }
    cx.update(page(&cx.app, cx.lang(), owner, target).await?).await
}
