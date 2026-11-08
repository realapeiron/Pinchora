//! Collection depth panels reached from `/dex`: wishes, recycling, shard loot and hourly rolls.
use std::{str::Split, time::Duration};

use chrono::{Datelike, Timelike};
use pinchora_core::{i18n::Lang, t};
use pinchora_db::{RollClaim, RollStart};
use pinchora_game::{
    Content, Rarity,
    collection::{Collection, GLIMMER_SHARE, exchange_pick, recycle_shards, shard_craft_cost},
};
use twilight_model::{
    channel::message::{
        Component,
        component::{ActionRow, ButtonStyle, SelectMenu, SelectMenuOption, SelectMenuType},
    },
    http::interaction::InteractionResponseData,
};
use twilight_util::builder::embed::EmbedBuilder;

use super::{award, dice};
use crate::{
    Cx,
    server_events::{button, row},
    util::BRAND,
};

fn back(owner: u64, lang: Lang) -> Component {
    button(
        format!("bc:d:{owner}:c:0"),
        &t!(lang, "dex.back"),
        None,
        ButtonStyle::Secondary,
        false,
    )
}

fn menu(id: String, label: String, options: Vec<SelectMenuOption>) -> Component {
    Component::ActionRow(ActionRow {
        id: None,
        components: vec![Component::SelectMenu(SelectMenu {
            channel_types: None,
            custom_id: id,
            default_values: None,
            disabled: false,
            id: None,
            kind: SelectMenuType::Text,
            max_values: Some(1),
            min_values: Some(1),
            options: Some(options),
            placeholder: Some(label),
            required: None,
        })],
    })
}

fn panel(title: String, text: String, mut rows: Vec<Component>, owner: u64, lang: Lang) -> InteractionResponseData {
    rows.push(row(vec![back(owner, lang)]));
    InteractionResponseData {
        embeds: Some(vec![
            EmbedBuilder::new().color(BRAND).title(title).description(text).build(),
        ]),
        components: Some(rows),
        attachments: Some(Vec::new()),
        ..Default::default()
    }
}

fn option(entry: &pinchora_game::collection::Entry, lang: Lang) -> SelectMenuOption {
    SelectMenuOption {
        default: false,
        description: Some(entry.rarity.label_in(lang).to_owned()),
        emoji: None,
        label: entry.name_in(lang),
        value: entry.id.clone(),
    }
}

async fn wishlist(cx: &Cx, owner: u64, page: usize, note: Option<String>) -> anyhow::Result<()> {
    let lang = cx.lang();
    let list = cx.app.db.wishlist(owner).await?;
    let catalogue = Collection::get();
    let all: Vec<_> = catalogue.critters.iter().chain(&catalogue.finds).collect();
    let pages = all.len().div_ceil(25).max(1);
    let page = page.min(pages - 1);
    let entries: Vec<_> = all.iter().skip(page * 25).take(25).map(|e| option(e, lang)).collect();
    let ping_entries: Vec<_> = all
        .iter()
        .skip(page * 25)
        .take(25)
        .filter(|e| e.kind == pinchora_game::collection::Kind::Critter)
        .map(|e| option(e, lang))
        .collect();
    let mut rows = Vec::new();
    if !ping_entries.is_empty() {
        rows.push(menu(
            format!("bc:wp:{owner}:{page}"),
            t!(lang, "collection-depth.add-ping"),
            ping_entries,
        ));
    }
    rows.push(menu(
        format!("bc:wq:{owner}:{page}"),
        t!(lang, "collection-depth.add-quiet"),
        entries,
    ));
    if !list.is_empty() {
        rows.push(menu(
            format!("bc:wr:{owner}:{page}"),
            t!(lang, "collection-depth.remove"),
            list.iter()
                .filter_map(|w| catalogue.entry(&w.entry_id).map(|e| option(e, lang)))
                .collect(),
        ));
    }
    rows.push(row(vec![
        button(
            format!("bc:w:{owner}:{}", page.saturating_sub(1)),
            "◀",
            None,
            ButtonStyle::Secondary,
            page == 0,
        ),
        button(
            format!("bc:w:{owner}:{}", page + 1),
            "▶",
            None,
            ButtonStyle::Secondary,
            page + 1 >= pages,
        ),
    ]));
    let mut text = t!(lang, "collection-depth.wishlist-count", count = list.len());
    if let Some(note) = note {
        text = format!("{note}\n\n{text}");
    }
    cx.update(panel(t!(lang, "collection-depth.wishlist"), text, rows, owner, lang))
        .await
}

async fn recycle(cx: &Cx, owner: u64, page: usize, note: Option<String>) -> anyhow::Result<()> {
    let lang = cx.lang();
    let dex = cx.app.db.dex(owner).await?;
    let all: Vec<_> = dex
        .iter()
        .filter(|d| d.kept > d.glimmer.max(1))
        .filter_map(|d| Collection::get().entry(&d.entry_id))
        .collect();
    let pages = all.len().div_ceil(25).max(1);
    let page = page.min(pages - 1);
    let mut rows = Vec::new();
    if !all.is_empty() {
        rows.push(menu(
            format!("bc:rs:{owner}:{page}"),
            t!(lang, "collection-depth.pick-spare"),
            all.iter().skip(page * 25).take(25).map(|e| option(e, lang)).collect(),
        ));
        rows.push(row(vec![
            button(
                format!("bc:r:{owner}:{}", page.saturating_sub(1)),
                "◀",
                None,
                ButtonStyle::Secondary,
                page == 0,
            ),
            button(
                format!("bc:r:{owner}:{}", page + 1),
                "▶",
                None,
                ButtonStyle::Secondary,
                page + 1 >= pages,
            ),
        ]));
    }
    let mut text = t!(
        lang,
        "collection-depth.recycle-text",
        shards = cx.app.db.shards(owner).await?,
        spares = all.len()
    );
    if let Some(note) = note {
        text = format!("{note}\n\n{text}");
    }
    cx.update(panel(t!(lang, "collection-depth.recycle"), text, rows, owner, lang))
        .await
}

async fn craft(cx: &Cx, owner: u64) -> anyhow::Result<()> {
    let lang = cx.lang();
    let owned: std::collections::HashSet<String> =
        cx.app.db.dex(owner).await?.into_iter().map(|d| d.entry_id).collect();
    let cost = shard_craft_cost(&owned);
    let Some(item) = exchange_pick(Content::get(), Rarity::Common, rand::random()) else {
        return cx.reply_ephemeral(t!(lang, "dex.nothing-yet")).await;
    };
    let text = if let Some(left) = cx.app.db.craft_shard_loot(owner, cost, &item).await? {
        t!(
            lang,
            "collection-depth.crafted",
            name = Content::get().name(&item, lang),
            left = left
        )
    } else {
        t!(lang, "collection-depth.not-enough-shards", cost = cost)
    };
    cx.update(panel(t!(lang, "collection-depth.craft"), text, Vec::new(), owner, lang))
        .await
}

async fn roll(cx: &Cx, owner: u64) -> anyhow::Result<()> {
    let lang = cx.lang();
    let now = cx.app.now();
    let Some(entry) = pinchora_game::collection::drop_cards(now.hour(), now.month(), &mut dice())
        .into_iter()
        .next()
    else {
        return cx.reply_ephemeral(t!(lang, "drop.gone")).await;
    };
    let result = cx
        .app
        .db
        .start_collection_roll(owner, &entry.id, rand::random::<f64>() < GLIMMER_SHARE, now)
        .await?;
    let (text, claim) = match result {
        RollStart::Started(r) => (
            t!(
                lang,
                "collection-depth.rolled",
                name = entry.name_in(lang),
                glimmer = if r.glimmer { "✦" } else { "" }
            ),
            true,
        ),
        RollStart::Wait(r) if r.claimed_at.is_none() && r.rolled_at + chrono::Duration::minutes(10) > now => (
            t!(
                lang,
                "collection-depth.rolled",
                name = Collection::get()
                    .entry(&r.entry_id)
                    .map(|e| e.name_in(lang))
                    .unwrap_or_default(),
                glimmer = if r.glimmer { "✦" } else { "" }
            ),
            true,
        ),
        RollStart::Wait(_) => (t!(lang, "collection-depth.roll-wait"), false),
        RollStart::Limit => (t!(lang, "collection-depth.roll-limit"), false),
    };
    let buttons = if claim {
        vec![row(vec![button(
            format!("bc:cl:{owner}"),
            &t!(lang, "collection-depth.claim"),
            None,
            ButtonStyle::Success,
            false,
        )])]
    } else {
        Vec::new()
    };
    cx.update(panel(t!(lang, "collection-depth.roll"), text, buttons, owner, lang))
        .await
}

async fn claim(cx: &Cx, owner: u64) -> anyhow::Result<()> {
    let lang = cx.lang();
    let Some(r) = cx.app.db.collection_roll(owner).await? else {
        return cx.reply_ephemeral(t!(lang, "collection-depth.roll-gone")).await;
    };
    let Some(entry) = Collection::get().entry(&r.entry_id) else {
        return cx.reply_ephemeral(t!(lang, "collection-depth.roll-gone")).await;
    };
    let text = match cx
        .app
        .db
        .claim_collection_roll(owner, cx.app.now(), award(entry))
        .await?
    {
        RollClaim::Claimed(_) => t!(
            lang,
            "collection-depth.claimed",
            name = entry.name_in(lang),
            glimmer = if r.glimmer { "✦" } else { "" }
        ),
        RollClaim::Gone => t!(lang, "collection-depth.roll-gone"),
    };
    cx.update(panel(t!(lang, "collection-depth.roll"), text, Vec::new(), owner, lang))
        .await
}

pub(crate) async fn on_component(
    cx: &Cx,
    action: &str,
    owner: u64,
    mut parts: Split<'_, char>,
    value: Option<&str>,
) -> anyhow::Result<()> {
    let lang = cx.lang();
    if owner != cx.author()?.id.get() {
        return cx.reply_ephemeral(t!(lang, "dex.not-yours")).await;
    }
    if matches!(action, "wp" | "wq" | "wr" | "rs" | "c" | "rl" | "cl")
        && cx
            .app
            .cache
            .try_cooldown(&format!("collection-depth:{owner}:{action}"), Duration::from_secs(1))
            .await?
            .is_some()
    {
        return cx.reply_ephemeral(t!(lang, "collection-depth.claim-slow")).await;
    }
    let page = parts.next().and_then(|v| v.parse::<usize>().ok()).unwrap_or(0);
    match action {
        "w" => wishlist(cx, owner, page, None).await,
        "wp" | "wq" => {
            let Some(entry) = value.filter(|id| {
                Collection::get()
                    .entry(id)
                    .is_some_and(|entry| action != "wp" || entry.kind == pinchora_game::collection::Kind::Critter)
            }) else {
                return cx.reply_ephemeral(t!(lang, "collect.button-expired")).await;
            };
            let added = cx.app.db.set_wish(owner, entry, action == "wp").await?;
            wishlist(
                cx,
                owner,
                page,
                Some(if added {
                    t!(lang, "collection-depth.wish-saved")
                } else {
                    t!(lang, "collection-depth.wishlist-full")
                }),
            )
            .await
        }
        "wr" => {
            if let Some(entry) = value {
                cx.app.db.remove_wish(owner, entry).await?;
            }
            wishlist(cx, owner, page, Some(t!(lang, "collection-depth.wish-removed"))).await
        }
        "r" => recycle(cx, owner, page, None).await,
        "rs" => {
            let Some(entry) = value.and_then(|id| Collection::get().entry(id)) else {
                return cx.reply_ephemeral(t!(lang, "collect.button-expired")).await;
            };
            let earned = recycle_shards(entry.rarity);
            let text = if cx.app.db.recycle_critter(owner, &entry.id, earned).await?.is_some() {
                t!(
                    lang,
                    "collection-depth.recycled",
                    name = entry.name_in(lang),
                    shards = earned
                )
            } else {
                t!(lang, "collection-depth.no-spare")
            };
            recycle(cx, owner, page, Some(text)).await
        }
        "c" => craft(cx, owner).await,
        "rl" => roll(cx, owner).await,
        "cl" => claim(cx, owner).await,
        _ => cx.reply_ephemeral(t!(lang, "collect.button-expired")).await,
    }
}
