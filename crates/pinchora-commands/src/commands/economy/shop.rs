use std::str::Split;

use chrono::Utc;
use pinchora_core::games::items::is_soulbound;
use pinchora_core::infra::i18n::Lang;
use pinchora_core::t;
use pinchora_db::{BuyOutcome, Currency as DbCurrency};
use pinchora_game::{Availability, Category, Content, Currency, market};
use pinchora_render::{GridSlot, ItemGridCard, Theme};
use twilight_interactions::command::{CommandModel, CommandOption, CreateCommand, CreateOption};
use twilight_model::{
    channel::message::{
        Component,
        component::{ActionRow, Button, ButtonStyle, SelectMenu, SelectMenuOption, SelectMenuType},
    },
    http::interaction::InteractionResponseData,
};
use twilight_util::builder::embed::EmbedBuilder;

use crate::App;
use crate::Cx;
use crate::games::game::scene::slot_sprite;
use crate::infra::util::BRAND;
use crate::infra::util::thousands_in;

const PER_PAGE: usize = 8;
/// The most letters a slot's badge fits before it runs out of the frame.
const BADGE_CHARS: usize = 19;

#[derive(CommandModel, CreateCommand)]
#[command(name = "shop", desc = "Buy furniture")]
pub enum ShopCommand {
    #[command(name = "market")]
    Market(ShopMarket),
    #[command(name = "catalog")]
    Catalog(ShopCatalog),
}

#[derive(CommandModel, CreateCommand)]
#[command(name = "market", desc = "The hourly market: rare pieces with limited global stock")]
pub struct ShopMarket;

#[derive(CommandModel, CreateCommand)]
#[command(name = "catalog", desc = "Browse the furniture catalog")]
pub struct ShopCatalog {
    /// Which kind of furniture
    category: Option<CategoryOption>,
}

#[derive(CommandOption, CreateOption, Clone, Copy)]
pub enum CategoryOption {
    #[option(name = "Seating", value = "seating")]
    Seating,
    #[option(name = "Tables", value = "tables")]
    Tables,
    #[option(name = "Beds", value = "beds")]
    Beds,
    #[option(name = "Storage", value = "storage")]
    Storage,
    #[option(name = "Lighting", value = "lighting")]
    Lighting,
    #[option(name = "Kitchen", value = "kitchen")]
    Kitchen,
    #[option(name = "Bathroom", value = "bathroom")]
    Bathroom,
    #[option(name = "Electronics", value = "electronics")]
    Electronics,
    #[option(name = "Rugs", value = "rugs")]
    Rugs,
    #[option(name = "Wall decor", value = "wall_decor")]
    WallDecor,
    #[option(name = "Windows & doors", value = "windows_doors")]
    WindowsDoors,
    #[option(name = "Plants", value = "plants")]
    Plants,
    #[option(name = "Pet items", value = "pet_items")]
    PetItems,
    #[option(name = "Toys & games", value = "toys_games")]
    ToysGames,
    #[option(name = "Music", value = "music")]
    Music,
    #[option(name = "Outdoor", value = "outdoor")]
    Outdoor,
    #[option(name = "Seasonal", value = "seasonal")]
    Seasonal,
    #[option(name = "Office", value = "office")]
    Office,
    #[option(name = "Cafe & shop", value = "cafe")]
    Cafe,
    #[option(name = "Hobbies", value = "hobbies")]
    Hobbies,
    #[option(name = "Fitness", value = "fitness")]
    Fitness,
    #[option(name = "Walls & partitions", value = "partitions")]
    Partitions,
    #[option(name = "Nautical", value = "nautical")]
    Nautical,
}

impl CategoryOption {
    fn category(self) -> Category {
        Category::from_key(self.value()).unwrap_or(Category::Seating)
    }
}

#[derive(CommandModel, CreateCommand)]
#[command(name = "inventory", desc = "Furniture you own but have not placed")]
pub struct Inventory;

impl ShopCommand {
    pub async fn run(self, cx: &Cx) -> anyhow::Result<()> {
        match self {
            Self::Market(_) => {
                let lang = cx.lang();
                let data = market_card(&cx.app, lang).await?;
                cx.reply(data).await
            }
            Self::Catalog(c) => {
                let category = c.category.map_or(Category::Seating, CategoryOption::category);
                let lang = cx.lang();
                let data = catalog_card(&cx.app, lang, category, 0).await?;
                cx.reply(data).await
            }
        }
    }
}

impl Inventory {
    pub async fn run(self, cx: &Cx) -> anyhow::Result<()> {
        let lang = cx.lang();
        let data = inventory_card(&cx.app, lang, cx.author()?.id.get(), 0).await?;
        cx.reply(data).await
    }
}

fn row(components: Vec<Component>) -> Component {
    Component::ActionRow(ActionRow { id: None, components })
}

fn button(custom_id: String, label: &str, disabled: bool) -> Component {
    Component::Button(Button {
        custom_id: Some(custom_id),
        disabled,
        emoji: None,
        id: None,
        label: Some(label.to_owned()),
        sku_id: None,
        style: ButtonStyle::Secondary,
        url: None,
    })
}

fn buy_menu(lang: Lang, custom_id: String, placeholder: &str, options: Vec<SelectMenuOption>) -> Component {
    let disabled = options.is_empty();
    Component::SelectMenu(SelectMenu {
        channel_types: None,
        custom_id,
        default_values: None,
        disabled,
        id: None,
        kind: SelectMenuType::Text,
        max_values: Some(1),
        min_values: Some(1),
        options: Some(if disabled {
            vec![SelectMenuOption {
                default: false,
                description: None,
                emoji: None,
                label: t!(lang, "shop.nothing-to-buy"),
                value: "none".to_owned(),
            }]
        } else {
            options
        }),
        placeholder: Some(placeholder.to_owned()),
        required: None,
    })
}

pub(crate) struct SlotSpec {
    pub(crate) item_id: String,
    pub(crate) line1: String,
    pub(crate) line2: String,
    pub(crate) bar: Option<f32>,
    pub(crate) badge: String,
    pub(crate) shell_price: bool,
}

pub(crate) async fn grid_png(
    app: &App,
    lang: Lang,
    title: String,
    subtitle: String,
    footer: String,
    specs: Vec<SlotSpec>,
) -> anyhow::Result<Vec<u8>> {
    let sprites = app.sprites.clone();
    app.render(move |r| {
        let content = Content::get();
        let (bw, bh) = ItemGridCard::SPRITE_BOX;
        let slots = specs
            .into_iter()
            .map(|s| {
                let item = content.item(&s.item_id);
                let (png, w, h) = slot_sprite(&sprites, content, &s.item_id, bw, bh);
                GridSlot {
                    sprite_png: png,
                    sprite_w: w,
                    sprite_h: h,
                    name: item.map_or_else(|| s.item_id.clone(), |i| i.name_in(lang)),
                    line1: s.line1,
                    line2: s.line2,
                    bar: s.bar,
                    frame: item.map_or(0x8fa0d8, |i| i.base.rarity.color()),
                    badge: s.badge,
                    shell_price: s.shell_price,
                }
            })
            .collect();
        r.item_grid(&ItemGridCard {
            lang,
            title,
            subtitle,
            slots,
            footer,
            theme: Theme::reef(),
        })
    })
    .await
}

/// `text` cut back by whole words until it fits a slot's badge (a Turkish category name can be longer than the frame).
fn fit_badge(mut text: String) -> String {
    while text.chars().count() > BADGE_CHARS {
        match text.rfind(' ') {
            Some(at) => text.truncate(at),
            None => return text.chars().take(BADGE_CHARS).collect(),
        }
    }
    text
}

/// A price as the cards draw it: shells are a bare number next to the shell icon.
fn price_label(lang: Lang, price: i64, currency: Currency) -> String {
    match currency {
        Currency::Shells => thousands_in(lang, price),
        Currency::Pearls => t!(lang, "shop.price-pearls", price = thousands_in(lang, price)),
    }
}

/// A price as a menu description writes it.
fn option_price(lang: Lang, price: i64, currency: Currency) -> String {
    match currency {
        Currency::Shells => t!(lang, "shop.option-price-shells", price = thousands_in(lang, price)),
        Currency::Pearls => t!(lang, "shop.option-price-pearls", price = thousands_in(lang, price)),
    }
}

/// The rule about soulbound pieces, which gifts, stalls and the plaza all repeat.
pub(crate) fn soulbound_rule(lang: Lang) -> String {
    t!(lang, "collect.soulbound-rule")
}

async fn market_card(app: &App, lang: Lang) -> anyhow::Result<InteractionResponseData> {
    let content = Content::get();
    let now = Utc::now().timestamp();
    let hour = market::hour_of(now);
    let rows = app.db.market(hour, &market::offers(content, hour)).await?;
    let minutes_left = (3600 - now.rem_euclid(3600)) / 60 + 1;
    let specs: Vec<SlotSpec> = rows
        .iter()
        .enumerate()
        .map(|(i, r)| {
            let left = (r.stock - r.sold).max(0);
            SlotSpec {
                item_id: r.item_id.clone(),
                line1: thousands_in(lang, r.price),
                line2: if left == 0 {
                    t!(lang, "shop.sold-out-line")
                } else {
                    t!(lang, "shop.stock-left", left = left, stock = r.stock)
                },
                bar: Some(left as f32 / r.stock.max(1) as f32),
                badge: format!("#{}", i + 1),
                shell_price: true,
            }
        })
        .collect();
    let options = rows
        .iter()
        .filter(|r| r.sold < r.stock)
        .filter_map(|r| {
            let item = content.item(&r.item_id)?;
            Some(SelectMenuOption {
                default: false,
                description: Some(t!(
                    lang,
                    "shop.market-option",
                    price = thousands_in(lang, r.price),
                    rarity = item.base.rarity.label_in(lang),
                    left = r.stock - r.sold
                )),
                emoji: None,
                label: item.name_in(lang),
                value: r.item_id.clone(),
            })
        })
        .collect();
    let png = grid_png(
        app,
        lang,
        t!(lang, "shop.market-title"),
        t!(lang, "shop.market-restocks", minutes = minutes_left),
        t!(lang, "shop.market-footer"),
        specs,
    )
    .await?;
    let embed = EmbedBuilder::new()
        .color(BRAND)
        .title(t!(lang, "shop.market-title"))
        .description(t!(lang, "shop.market-intro"));
    Ok(Cx::card(
        embed,
        "market.png",
        png,
        vec![row(vec![buy_menu(
            lang,
            format!("mk:{hour}"),
            &t!(lang, "shop.market-pick"),
            options,
        )])],
        false,
    ))
}

fn catalog_items(content: &Content, category: Category) -> Vec<pinchora_game::Item<'_>> {
    let mut items: Vec<_> = content
        .items()
        .filter(|i| i.base.category == category && i.base.availability == Availability::Permanent)
        .collect();
    // Shell items first (cheapest first), premium pearl items after them.
    items.sort_by_key(|i| (i.base.currency == Currency::Pearls, i.base.price, i.id()));
    items
}

async fn catalog_card(
    app: &App,
    lang: Lang,
    category: Category,
    page: usize,
) -> anyhow::Result<InteractionResponseData> {
    let content = Content::get();
    let items = catalog_items(content, category);
    let pages = items.len().div_ceil(PER_PAGE).max(1);
    let page = page.min(pages - 1);
    let shown: Vec<_> = items.iter().skip(page * PER_PAGE).take(PER_PAGE).collect();
    let specs = shown
        .iter()
        .map(|i| SlotSpec {
            item_id: i.id(),
            line1: price_label(lang, i.base.price, i.base.currency),
            line2: format!(
                "{} · {}×{}",
                lang.upper(i.base.rarity.label_in(lang)),
                i.base.footprint[0],
                i.base.footprint[1]
            ),
            bar: None,
            badge: fit_badge(lang.upper(i.base.category.label_in(lang))),
            shell_price: i.base.currency == Currency::Shells,
        })
        .collect();
    let options = shown
        .iter()
        .map(|i| SelectMenuOption {
            default: false,
            description: Some(format!(
                "{} · {}",
                option_price(lang, i.base.price, i.base.currency),
                content
                    .description(&i.base.id, lang)
                    .unwrap_or_default()
                    .chars()
                    .take(80)
                    .collect::<String>()
            )),
            emoji: None,
            label: i.name_in(lang),
            value: i.id(),
        })
        .collect();
    let key = category.key();
    let png = grid_png(
        app,
        lang,
        t!(lang, "shop.catalog-title", category = category.label_in(lang)),
        t!(lang, "card.page", page = page + 1, pages = pages),
        t!(lang, "shop.catalog-footer", count = items.len()),
        specs,
    )
    .await?;
    let embed = EmbedBuilder::new()
        .color(BRAND)
        .title(t!(lang, "shop.catalog-title", category = category.label_in(lang)))
        .description(t!(lang, "shop.catalog-intro"));
    Ok(Cx::card(
        embed,
        "catalog.png",
        png,
        vec![
            row(vec![buy_menu(
                lang,
                format!("cb:{key}"),
                &t!(lang, "shop.catalog-pick"),
                options,
            )]),
            row(vec![
                button(
                    format!("cp:{key}:{}", page.saturating_sub(1)),
                    &t!(lang, "customize.prev-page"),
                    page == 0,
                ),
                button(
                    format!("cp:{key}:{}", page + 1),
                    &t!(lang, "customize.next-page"),
                    page + 1 >= pages,
                ),
            ]),
        ],
        false,
    ))
}

async fn inventory_card(app: &App, lang: Lang, user_id: u64, page: usize) -> anyhow::Result<InteractionResponseData> {
    let content = Content::get();
    let rows = app.db.inventory(user_id).await?;
    let pages = rows.len().div_ceil(PER_PAGE).max(1);
    let page = page.min(pages - 1);
    let total: i64 = rows.iter().map(|r| i64::from(r.quantity)).sum();
    let specs = rows
        .iter()
        .skip(page * PER_PAGE)
        .take(PER_PAGE)
        .map(|r| SlotSpec {
            item_id: r.item_id.clone(),
            line1: format!("X{}", r.quantity),
            line2: content
                .item(&r.item_id)
                .map_or_else(String::new, |i| lang.upper(i.base.category.label_in(lang))),
            bar: None,
            badge: if is_soulbound(&r.item_id) {
                t!(lang, "inventory.soulbound")
            } else {
                String::new()
            },
            shell_price: false,
        })
        .collect();
    let png = grid_png(
        app,
        lang,
        t!(lang, "inventory.card-title"),
        t!(lang, "card.page", page = page + 1, pages = pages),
        t!(lang, "inventory.footer", total = total),
        specs,
    )
    .await?;
    let embed = EmbedBuilder::new()
        .color(BRAND)
        .title(t!(lang, "inventory.title"))
        .description(t!(lang, "inventory.intro"));
    let components = if pages > 1 {
        vec![row(vec![
            button(
                format!("iv:{}", page.saturating_sub(1)),
                &t!(lang, "customize.prev-page"),
                page == 0,
            ),
            button(
                format!("iv:{}", page + 1),
                &t!(lang, "customize.next-page"),
                page + 1 >= pages,
            ),
        ])]
    } else {
        Vec::new()
    };
    Ok(Cx::card(embed, "inventory.png", png, components, true))
}

fn buy_text(lang: Lang, outcome: BuyOutcome, name: &str, price: i64, currency: Currency) -> String {
    let unit = match currency {
        Currency::Shells => t!(lang, "shop.unit-shells"),
        Currency::Pearls => t!(lang, "shop.unit-pearls"),
    };
    match outcome {
        BuyOutcome::Bought { balance } => t!(
            lang,
            "shop.bought",
            name = name,
            price = thousands_in(lang, price),
            unit = unit,
            balance = thousands_in(lang, balance)
        ),
        BuyOutcome::Insufficient { balance } => t!(
            lang,
            "shop.insufficient",
            name = name,
            price = thousands_in(lang, price),
            unit = unit,
            balance = thousands_in(lang, balance)
        ),
        BuyOutcome::SoldOut => t!(lang, "shop.sold-out", name = name),
    }
}

/// `mk:<hour>` select: buy one unit from that hour's market.
pub(crate) async fn on_market_buy(cx: &Cx, mut args: Split<'_, char>, value: Option<&str>) -> anyhow::Result<()> {
    let hour: i64 = args.next().and_then(|h| h.parse().ok()).unwrap_or(0);
    let lang = cx.lang();
    let Some(item_id) = value.filter(|v| *v != "none") else {
        return cx.reply_ephemeral(t!(lang, "shop.pick-item")).await;
    };
    if hour != market::hour_of(Utc::now().timestamp()) {
        return cx.reply_ephemeral(t!(lang, "shop.market-closed")).await;
    }
    let content = Content::get();
    let Some(item) = content.item(item_id) else {
        return cx.reply_ephemeral(t!(lang, "shop.item-gone")).await;
    };
    let outcome = cx.app.db.market_buy(cx.author()?.id.get(), hour, item_id).await?;
    if matches!(outcome, BuyOutcome::Bought { .. }) {
        cx.app
            .quest(cx.author()?.id.get(), pinchora_game::quests::QuestKind::Buy)
            .await;
    }
    let price = cx
        .app
        .db
        .market(hour, &[])
        .await?
        .into_iter()
        .find(|r| r.item_id == item_id)
        .map_or(item.base.price, |r| r.price);
    cx.reply_ephemeral(buy_text(lang, outcome, &item.name_in(lang), price, Currency::Shells))
        .await
}

/// `cb:<category>` select: buy one at the catalog price. Price and availability come
/// from content, never from the client.
pub(crate) async fn on_catalog_buy(cx: &Cx, value: Option<&str>) -> anyhow::Result<()> {
    let lang = cx.lang();
    let Some(item) = value.and_then(|v| Content::get().item(v)) else {
        return cx.reply_ephemeral(t!(lang, "shop.pick-item")).await;
    };
    if item.base.availability != Availability::Permanent {
        return cx.reply_ephemeral(t!(lang, "shop.not-in-catalog")).await;
    }
    let currency = match item.base.currency {
        Currency::Shells => DbCurrency::Shells,
        Currency::Pearls => DbCurrency::Pearls,
    };
    let outcome = cx
        .app
        .db
        .buy_item(cx.author()?.id.get(), &item.id(), item.base.price, currency, 1)
        .await?;
    if matches!(outcome, BuyOutcome::Bought { .. }) {
        cx.app
            .quest(cx.author()?.id.get(), pinchora_game::quests::QuestKind::Buy)
            .await;
    }
    cx.reply_ephemeral(buy_text(
        lang,
        outcome,
        &item.name_in(lang),
        item.base.price,
        item.base.currency,
    ))
    .await
}

/// `cp:<category>:<page>` buttons.
pub(crate) async fn on_catalog_page(cx: &Cx, mut args: Split<'_, char>) -> anyhow::Result<()> {
    let category = args.next().and_then(Category::from_key).unwrap_or(Category::Seating);
    let page = args.next().and_then(|p| p.parse().ok()).unwrap_or(0);
    let lang = cx.lang();
    let data = catalog_card(&cx.app, lang, category, page).await?;
    cx.update(data).await
}

/// `iv:<page>` buttons.
pub(crate) async fn on_inventory_page(cx: &Cx, mut args: Split<'_, char>) -> anyhow::Result<()> {
    let page = args.next().and_then(|p| p.parse().ok()).unwrap_or(0);
    let lang = cx.lang();
    let data = inventory_card(&cx.app, lang, cx.author()?.id.get(), page).await?;
    cx.update(data).await
}
