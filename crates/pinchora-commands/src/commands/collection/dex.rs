//! `/dex`: the Tide Pool Dex, a paged picture book of every critter and find, and where fragments are
//! spent on decor.

use std::{collections::HashMap, str::Split, sync::Arc};

use pinchora_game::{
    Content, Rarity,
    collection::{Collection, Kind, exchange_cost, exchange_pick, exchange_pool},
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

use crate::{
    App, Cx,
    game::scene::slot_sprite,
    server_events::{button, row},
    util::{BRAND, SUCCESS, display_name, resolved_display_name, thousands},
};

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
            Some(u) if u.resolved.bot => return cx.reply_ephemeral("Bots don't collect. They just count.").await,
            Some(u) => (u.resolved.id.get(), resolved_display_name(u)),
            None => (me, cx.author_display_name()),
        };
        let tab = match self.tab {
            Some(DexTab::Finds) => Kind::Find,
            _ => Kind::Critter,
        };
        let data = view(&cx.app, owner, &name, me, tab, 0, true).await?;
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

/// One page of `owner`'s dex as `viewer` sees it: the picture, page and tab buttons, and for the owner a
/// button to spend their fragments.
pub(crate) async fn view(
    app: &Arc<App>,
    owner: u64,
    name: &str,
    viewer: u64,
    kind: Kind,
    page: usize,
    ephemeral: bool,
) -> anyhow::Result<InteractionResponseData> {
    let collection = Collection::get();
    let list = collection.list(kind);
    let pages = list.len().div_ceil(DexCard::PER_PAGE).max(1);
    let page = page.min(pages - 1);
    let caught: HashMap<String, i32> = app
        .db
        .dex(owner)
        .await?
        .into_iter()
        .map(|row| (row.entry_id, row.caught))
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
            name: entry.name.clone(),
            art: super::picture(app, entry, DexCard::ART_BOX),
            frame: entry.rarity.color(),
            owned: caught.contains_key(&entry.id),
            caught: caught.get(&entry.id).copied().unwrap_or(0),
        })
        .collect();
    let mine = owner == viewer;
    let footer = if mine {
        format!(
            "{found_all}/{} IN ALL · FRAGMENTS {}",
            collection.len(),
            thousands(app.db.fragments(owner).await?)
        )
    } else {
        format!("{found_all}/{} IN ALL", collection.len())
    };
    let card = DexCard {
        title: "Tide pool dex".to_owned(),
        tab: kind.label().to_uppercase(),
        owner: name.to_owned(),
        found: found_here,
        total: list.len(),
        page,
        pages,
        cells,
        footer,
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
        tab("Critters", Kind::Critter),
        tab("Finds", Kind::Find),
    ])];
    if mine {
        rows.push(row(vec![button(
            format!("bc:x:{owner}"),
            "Spend fragments",
            Some("🧩"),
            ButtonStyle::Success,
            false,
        )]));
    }
    let embed = EmbedBuilder::new().color(BRAND).description(format!(
        "**{name}'s Tide Pool Dex** · {} {found_here}/{} · {found_all} of {} found",
        kind.label().to_lowercase(),
        list.len(),
        collection.len()
    ));
    Ok(Cx::card(embed, "dex.png", png, rows, ephemeral))
}

/// What the fragment exchange offers, as a select menu, with a way back.
async fn exchange_panel(app: &App, owner: u64, note: Option<&str>) -> anyhow::Result<InteractionResponseData> {
    let fragments = app.db.fragments(owner).await?;
    let content = Content::get();
    let options: Vec<SelectMenuOption> = Rarity::ALL
        .into_iter()
        .map(|rarity| {
            let cost = exchange_cost(rarity);
            SelectMenuOption {
                default: false,
                description: Some(format!(
                    "{} fragments · {} pieces to find",
                    thousands(cost),
                    exchange_pool(content, rarity).len()
                )),
                emoji: None,
                label: format!("{} decor piece", rarity.label()),
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
            placeholder: Some("Pick a piece to trade for…".to_owned()),
            required: None,
        })],
    });
    let back = row(vec![button(
        page_id(owner, Kind::Critter, 0),
        "Back to the dex",
        None,
        ButtonStyle::Secondary,
        false,
    )]);
    let mut text = format!(
        "You have **{}** fragments. Spares of critters and finds leave fragments behind; trade them for a \
         free-to-earn decor piece of the tier you pick, chosen at random.",
        thousands(fragments)
    );
    if let Some(note) = note {
        text = format!("{note}\n\n{text}");
    }
    Ok(InteractionResponseData {
        embeds: Some(vec![
            EmbedBuilder::new()
                .color(BRAND)
                .title("Fragment exchange")
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
    match action {
        "d" => {
            let kind = parse_tab(parts.next().unwrap_or_default());
            let page: usize = parts.next().and_then(|v| v.parse().ok()).unwrap_or(0);
            let name = if owner == me {
                cx.author_display_name()
            } else {
                match cx.app.user(owner).await {
                    Some(user) => display_name(&user, None),
                    None => "A collector".to_owned(),
                }
            };
            let data = view(&cx.app, owner, &name, me, kind, page, false).await?;
            cx.update(data).await
        }
        "x" | "xs" if owner != me => {
            cx.reply_ephemeral("That is not your dex. Open yours with `/dex`.")
                .await
        }
        "x" => {
            let data = exchange_panel(&cx.app, owner, None).await?;
            cx.update(data).await
        }
        "xs" => {
            let Some(rarity) = value.and_then(|v| Rarity::ALL.into_iter().find(|r| r.label().to_lowercase() == v))
            else {
                return cx.reply_ephemeral("Pick a tier from the menu.").await;
            };
            let cost = exchange_cost(rarity);
            let content = Content::get();
            let Some(item) = exchange_pick(content, rarity, rand::random()) else {
                let data =
                    exchange_panel(&cx.app, owner, Some("There is nothing to trade for at that tier yet.")).await?;
                return cx.update(data).await;
            };
            let Some(left) = cx.app.db.exchange_fragments(me, cost, &item).await? else {
                let note = format!(
                    "A {} piece costs **{}** fragments.",
                    rarity.label().to_lowercase(),
                    thousands(cost)
                );
                let data = exchange_panel(&cx.app, owner, Some(&note)).await?;
                return cx.update(data).await;
            };
            let name = content.item(&item).map_or_else(|| item.clone(), |i| i.name());
            let (png, _, _) = slot_sprite(&cx.app.sprites, content, &item, 200, 160);
            let embed = EmbedBuilder::new()
                .color(SUCCESS)
                .title(format!("You got a {name}!"))
                .description(format!(
                    "A {} decor piece, now in your inventory: place it in `/house edit`.\nFragments left: **{}**.",
                    rarity.label().to_lowercase(),
                    thousands(left)
                ));
            let back = row(vec![
                button(
                    format!("bc:x:{owner}"),
                    "Trade again",
                    Some("🧩"),
                    ButtonStyle::Success,
                    false,
                ),
                button(
                    page_id(owner, Kind::Critter, 0),
                    "Back to the dex",
                    None,
                    ButtonStyle::Secondary,
                    false,
                ),
            ]);
            cx.update(Cx::card(embed, "piece.png", png, vec![back], false)).await
        }
        _ => cx.reply_ephemeral("This button has expired.").await,
    }
}
