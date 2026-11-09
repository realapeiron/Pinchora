//! `/dex`: the Tide Pool Dex, a paged picture book of every critter and find, and where fragments are
//! spent on decor.

use std::{collections::HashMap, str::Split};

use pinchora_core::infra::i18n::Lang;
use pinchora_core::t;
use pinchora_game::{
    Content, Rarity,
    collection::{Collection, Kind, exchange_cost, exchange_pick, exchange_pool},
    lang as letters,
};
use pinchora_render::{DexCard, DexCell, Theme};
use twilight_interactions::command::{CommandModel, CommandOption, CreateCommand, CreateOption, ResolvedUser};
use twilight_model::{
    channel::message::{
        Component,
        component::{ActionRow, ButtonStyle, SelectMenu, SelectMenuOption, SelectMenuType},
    },
    http::interaction::InteractionResponseData,
};
use twilight_util::builder::embed::EmbedBuilder;

use crate::App;
use crate::Cx;
use crate::commands::games::collection::soulbound_rule;
use crate::community::server_events::button;
use crate::community::server_events::row;
use crate::games::game::scene::slot_sprite;
use crate::infra::util::BRAND;
use crate::infra::util::SUCCESS;
use crate::infra::util::display_name;
use crate::infra::util::resolved_display_name;
use crate::infra::util::thousands_in;

#[derive(CommandModel, CreateCommand)]
#[command(name = "dex", desc = "Your Tide Pool Dex: every critter and find you have collected")]
pub struct Dex {
    /// Whose dex (default: yours)
    user: Option<ResolvedUser>,
    /// Which half of the book to open (default: critters)
    tab: Option<DexTab>,
}

#[derive(CommandOption, CreateOption, Clone, Copy, Debug, PartialEq, Eq)]
pub enum DexTab {
    #[option(name = "Critters", value = "critters")]
    Critters,
    #[option(name = "Finds", value = "finds")]
    Finds,
}

impl Dex {
    pub async fn run(self, cx: &Cx) -> anyhow::Result<()> {
        let me = cx.author()?.id.get();
        let (owner, name) = match &self.user {
            Some(u) if u.resolved.bot => return cx.reply_ephemeral(t!(cx.lang(), "dex.bot")).await,
            Some(u) => (u.resolved.id.get(), resolved_display_name(u)),
            None => (me, cx.author_display_name()),
        };
        if crate::commands::admin::privacy::private_profile(cx, owner).await? {
            return Ok(());
        }
        let tab = match self.tab {
            Some(DexTab::Finds) => Kind::Find,
            _ => Kind::Critter,
        };
        let data = view(cx, owner, &name, tab, 0, true).await?;
        cx.reply(data).await
    }
}

fn tab_key(kind: Kind) -> char {
    match kind {
        Kind::Critter => 'c',
        Kind::Find => 'f',
    }
}

fn parse_tab(key: &str) -> Kind {
    if key == "f" { Kind::Find } else { Kind::Critter }
}

fn page_id(owner: u64, kind: Kind, page: usize) -> String {
    format!("bc:d:{owner}:{}:{page}", tab_key(kind))
}

/// One page of `owner`'s dex as the author of `cx` sees it, in their language: the picture, page and tab
/// buttons, and for the owner a button to spend their fragments.
pub(crate) async fn view(
    cx: &Cx,
    owner: u64,
    name: &str,
    kind: Kind,
    page: usize,
    ephemeral: bool,
) -> anyhow::Result<InteractionResponseData> {
    let (app, lang, viewer) = (&cx.app, cx.lang(), cx.author()?.id.get());
    let collection = Collection::get();
    let list = collection.list(kind);
    let pages = list.len().div_ceil(DexCard::PER_PAGE).max(1);
    let page = page.min(pages - 1);
    let caught: HashMap<String, pinchora_db::DexRow> = app
        .db
        .dex(owner)
        .await?
        .into_iter()
        .map(|row| (row.entry_id.clone(), row))
        .collect();
    let found = |k: Kind| collection.list(k).iter().filter(|e| caught.contains_key(&e.id)).count();
    let (found_here, found_all) = (found(kind), found(Kind::Critter) + found(Kind::Find));
    let cells: Vec<DexCell> = list
        .iter()
        .enumerate()
        .skip(page * DexCard::PER_PAGE)
        .take(DexCard::PER_PAGE)
        .map(|(i, entry)| DexCell {
            number: i + 1,
            name: entry.name_in(lang),
            art: if caught.get(&entry.id).is_some_and(|row| row.glimmer > 0) {
                crate::commands::games::collection::glimmer_picture(app, entry, DexCard::ART_BOX)
            } else {
                crate::commands::games::collection::picture(app, entry, DexCard::ART_BOX)
            },
            frame: if caught.get(&entry.id).is_some_and(|row| row.glimmer > 0) {
                0x9af5ec
            } else {
                entry.rarity.color()
            },
            owned: caught.contains_key(&entry.id),
            caught: caught.get(&entry.id).map_or(0, |row| row.caught),
        })
        .collect();
    let mine = owner == viewer;
    let footer = if mine {
        let fragments = thousands_in(lang, app.db.fragments(owner).await?);
        t!(
            lang,
            "dex.footer-mine",
            found = found_all,
            total = collection.len(),
            fragments = fragments
        )
    } else {
        t!(lang, "dex.footer-other", found = found_all, total = collection.len())
    };
    let card = DexCard {
        title: t!(lang, "dex.title"),
        tab: lang.upper(kind.label_in(lang)),
        owner: name.to_owned(),
        found: found_here,
        total: list.len(),
        page,
        pages,
        cells,
        footer,
        lang,
        theme: Theme::reef(),
    };
    let png = app.render(move |r| r.dex_card(&card)).await?;

    let nav = |label: &str, to: usize, disabled: bool| {
        button(page_id(owner, kind, to), label, None, ButtonStyle::Secondary, disabled)
    };
    let tab = |label: &str, to: Kind| {
        let style = if to == kind {
            ButtonStyle::Primary
        } else {
            ButtonStyle::Secondary
        };
        button(page_id(owner, to, 0), label, None, style, to == kind)
    };
    let mut rows = vec![row(vec![
        nav("◀", page.saturating_sub(1), page == 0),
        nav("▶", page + 1, page + 1 >= pages),
        tab(Kind::Critter.label_in(lang), Kind::Critter),
        tab(Kind::Find.label_in(lang), Kind::Find),
    ])];
    if mine {
        rows.push(row(vec![
            button(
                format!("bc:x:{owner}"),
                &t!(lang, "dex.spend"),
                Some("🧩"),
                ButtonStyle::Success,
                false,
            ),
            button(
                format!("bc:w:{owner}:0"),
                &t!(lang, "collection-depth.wishlist"),
                None,
                ButtonStyle::Secondary,
                false,
            ),
            button(
                format!("bc:r:{owner}:0"),
                &t!(lang, "collection-depth.recycle"),
                None,
                ButtonStyle::Secondary,
                false,
            ),
            button(
                format!("bc:c:{owner}:0:{}", cx.interaction.id),
                &t!(lang, "collection-depth.craft"),
                None,
                ButtonStyle::Secondary,
                false,
            ),
            button(
                format!("bc:rl:{owner}"),
                &t!(lang, "collection-depth.roll"),
                None,
                ButtonStyle::Primary,
                false,
            ),
        ]));
    }
    let embed = EmbedBuilder::new().color(BRAND).description(t!(
        lang,
        "dex.summary",
        name = name,
        tab = letters::lower(lang, kind.label_in(lang)),
        found = found_here,
        count = list.len(),
        all = found_all,
        total = collection.len()
    ));
    Ok(Cx::card(embed, "dex.png", png, rows, ephemeral))
}

/// What the fragment exchange offers, as a select menu, with a way back.
async fn exchange_panel(
    app: &App,
    lang: Lang,
    owner: u64,
    note: Option<&str>,
) -> anyhow::Result<InteractionResponseData> {
    let fragments = app.db.fragments(owner).await?;
    let content = Content::get();
    let options: Vec<SelectMenuOption> = Rarity::ALL
        .into_iter()
        .map(|rarity| {
            let cost = exchange_cost(rarity);
            SelectMenuOption {
                default: false,
                description: Some(t!(
                    lang,
                    "dex.option-desc",
                    cost = thousands_in(lang, cost),
                    pieces = exchange_pool(content, rarity).len()
                )),
                emoji: None,
                label: t!(lang, "dex.option-label", rarity = rarity.label_in(lang)),
                value: rarity.label().to_lowercase(),
            }
        })
        .collect();
    let menu = Component::ActionRow(ActionRow {
        id: None,
        components: vec![Component::SelectMenu(SelectMenu {
            channel_types: None,
            custom_id: format!("bc:xs:{owner}"),
            default_values: None,
            disabled: false,
            id: None,
            kind: SelectMenuType::Text,
            max_values: Some(1),
            min_values: Some(1),
            options: Some(options),
            placeholder: Some(t!(lang, "dex.pick-piece")),
            required: None,
        })],
    });
    let back = row(vec![button(
        page_id(owner, Kind::Critter, 0),
        &t!(lang, "dex.back"),
        None,
        ButtonStyle::Secondary,
        false,
    )]);
    let mut text = t!(
        lang,
        "dex.exchange-text",
        fragments = thousands_in(lang, fragments),
        rule = soulbound_rule(lang)
    );
    if let Some(note) = note {
        text = format!("{note}\n\n{text}");
    }
    Ok(InteractionResponseData {
        embeds: Some(vec![
            EmbedBuilder::new()
                .color(BRAND)
                .title(t!(lang, "dex.exchange-title"))
                .description(text)
                .build(),
        ]),
        components: Some(vec![menu, back]),
        attachments: Some(Vec::new()),
        ..Default::default()
    })
}

/// `bc:d:<owner>:<tab>:<page>` pages and tabs, `bc:x:<owner>` opens the exchange and `bc:xs:<owner>` picks
/// a tier in it.
pub(crate) async fn on_component(cx: &Cx, mut parts: Split<'_, char>, value: Option<&str>) -> anyhow::Result<()> {
    let action = parts.next().unwrap_or_default();
    let owner: u64 = parts.next().and_then(|v| v.parse().ok()).unwrap_or(0);
    let me = cx.author()?.id.get();
    let lang = cx.lang();
    match action {
        "d" => {
            let kind = parse_tab(parts.next().unwrap_or_default());
            let page: usize = parts.next().and_then(|v| v.parse().ok()).unwrap_or(0);
            let name = if owner == me {
                cx.author_display_name()
            } else {
                match cx.app.user(owner).await {
                    Some(user) => display_name(&user, None),
                    None => t!(lang, "dex.collector"),
                }
            };
            let data = view(cx, owner, &name, kind, page, false).await?;
            cx.update(data).await
        }
        "x" | "xs" if owner != me => cx.reply_ephemeral(t!(lang, "dex.not-yours")).await,
        "x" => {
            let data = exchange_panel(&cx.app, lang, owner, None).await?;
            cx.update(data).await
        }
        "xs" => {
            let Some(rarity) = value.and_then(|v| Rarity::ALL.into_iter().find(|r| r.label().to_lowercase() == v))
            else {
                return cx.reply_ephemeral(t!(lang, "dex.pick-tier")).await;
            };
            let cost = exchange_cost(rarity);
            let content = Content::get();
            let Some(item) = exchange_pick(content, rarity, rand::random()) else {
                let note = t!(lang, "dex.nothing-yet");
                let data = exchange_panel(&cx.app, lang, owner, Some(&note)).await?;
                return cx.update(data).await;
            };
            let Some(left) = cx.app.db.exchange_fragments(me, cost, &item).await? else {
                let note = t!(
                    lang,
                    "dex.costs",
                    rarity = letters::lower(lang, rarity.label_in(lang)),
                    cost = thousands_in(lang, cost)
                );
                let data = exchange_panel(&cx.app, lang, owner, Some(&note)).await?;
                return cx.update(data).await;
            };
            let name = content.item(&item).map_or_else(|| item.clone(), |i| i.name_in(lang));
            let (png, _, _) = slot_sprite(&cx.app.sprites, content, &item, 200, 160);
            let embed = EmbedBuilder::new()
                .color(SUCCESS)
                .title(t!(lang, "dex.got", name = name))
                .description(t!(
                    lang,
                    "dex.got-text",
                    rarity = letters::lower(lang, rarity.label_in(lang)),
                    left = thousands_in(lang, left)
                ));
            let back = row(vec![
                button(
                    format!("bc:x:{owner}"),
                    &t!(lang, "dex.trade-again"),
                    Some("🧩"),
                    ButtonStyle::Success,
                    false,
                ),
                button(
                    page_id(owner, Kind::Critter, 0),
                    &t!(lang, "dex.back"),
                    None,
                    ButtonStyle::Secondary,
                    false,
                ),
            ]);
            cx.update(Cx::card(embed, "piece.png", png, vec![back], false)).await
        }
        "w" | "wp" | "wq" | "wr" | "r" | "rs" | "rc" | "c" | "rl" | "cl" => {
            crate::commands::games::collection::depth::on_component(cx, action, owner, parts, value).await
        }
        _ => cx.reply_ephemeral(t!(lang, "collect.button-expired")).await,
    }
}
