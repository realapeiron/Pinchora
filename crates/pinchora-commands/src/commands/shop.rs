use std::str::Split;

use chrono::Utc;
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

use crate::{
    App, Cx,
    game::scene::slot_sprite,
    util::{BRAND, thousands},
};

const PER_PAGE: usize = 8;

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
                let data = market_card(&cx.app).await?;
                cx.reply(data).await
            }
            Self::Catalog(c) => {
                let category = c.category.map_or(Category::Seating, CategoryOption::category);
                let data = catalog_card(&cx.app, category, 0).await?;
                cx.reply(data).await
            }
        }
    }
}

impl Inventory {
    pub async fn run(self, cx: &Cx) -> anyhow::Result<()> {
        let data = inventory_card(&cx.app, cx.author()?.id.get(), 0).await?;
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

fn buy_menu(custom_id: String, placeholder: &str, options: Vec<SelectMenuOption>) -> Component {
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
                label: "Nothing to buy".to_owned(),
                value: "none".to_owned(),
            }]
        } else {
            options
        }),
        placeholder: Some(placeholder.to_owned()),
        required: None,
    })
}

struct SlotSpec {
    item_id: String,
    line1: String,
    line2: String,
    bar: Option<f32>,
    badge: String,
    shell_price: bool,
}

async fn grid_png(
    app: &App,
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
                    name: item.map_or_else(|| s.item_id.clone(), |i| i.name()),
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
            title,
            subtitle,
            slots,
            footer,
            theme: Theme::reef(),
        })
    })
    .await
}

fn price_label(price: i64, currency: Currency) -> String {
    match currency {
        Currency::Shells => thousands(price),
        Currency::Pearls => format!("{} PEARLS", thousands(price)),
    }
}

async fn market_card(app: &App) -> anyhow::Result<InteractionResponseData> {
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
                line1: thousands(r.price),
                line2: if left == 0 {
                    "SOLD OUT".to_owned()
                } else {
                    format!("{left}/{} LEFT", r.stock)
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
                description: Some(format!(
                    "{} shells · {} · {} left",
                    thousands(r.price),
                    item.base.rarity.label(),
                    r.stock - r.sold
                )),
                emoji: None,
                label: item.name(),
                value: r.item_id.clone(),
            })
        })
        .collect();
    let png = grid_png(
        app,
        "Hourly market".to_owned(),
        format!("RESTOCKS IN {minutes_left} MIN"),
        "STOCK IS SHARED BY EVERY PLAYER".to_owned(),
        specs,
    )
    .await?;
    let embed = EmbedBuilder::new()
        .color(BRAND)
        .title("Hourly market")
        .description("Eight pieces, fresh every hour, while stock lasts. Pick one below to buy it.");
    Ok(Cx::card(
        embed,
        "market.png",
        png,
        vec![row(vec![buy_menu(
            format!("mk:{hour}"),
            "Buy from the market…",
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
    items.sort_by(|a, b| a.base.price.cmp(&b.base.price).then_with(|| a.id().cmp(&b.id())));
    items
}

async fn catalog_card(app: &App, category: Category, page: usize) -> anyhow::Result<InteractionResponseData> {
    let content = Content::get();
    let items = catalog_items(content, category);
    let pages = items.len().div_ceil(PER_PAGE).max(1);
    let page = page.min(pages - 1);
    let shown: Vec<_> = items.iter().skip(page * PER_PAGE).take(PER_PAGE).collect();
    let specs = shown
        .iter()
        .map(|i| SlotSpec {
            item_id: i.id(),
            line1: price_label(i.base.price, i.base.currency),
            line2: format!(
                "{} · {}×{}",
                i.base.rarity.label().to_uppercase(),
                i.base.footprint[0],
                i.base.footprint[1]
            ),
            bar: None,
            badge: i.base.category.label().to_uppercase(),
            shell_price: i.base.currency == Currency::Shells,
        })
        .collect();
    let options = shown
        .iter()
        .map(|i| SelectMenuOption {
            default: false,
            description: Some(format!(
                "{} · {}",
                price_label(i.base.price, i.base.currency).to_lowercase(),
                i.base.description.chars().take(80).collect::<String>()
            )),
            emoji: None,
            label: i.name(),
            value: i.id(),
        })
        .collect();
    let key = category.key();
    let png = grid_png(
        app,
        format!("{} catalog", category.label()),
        format!("PAGE {}/{pages}", page + 1),
        format!("{} ITEMS IN THIS AISLE", items.len()),
        specs,
    )
    .await?;
    let embed = EmbedBuilder::new()
        .color(BRAND)
        .title(format!("{} catalog", category.label()))
        .description(
            "Pick an item below to buy one. Bought furniture goes to `/inventory`; place it with `/house edit`.",
        );
    Ok(Cx::card(
        embed,
        "catalog.png",
        png,
        vec![
            row(vec![buy_menu(format!("cb:{key}"), "Buy from this page…", options)]),
            row(vec![
                button(format!("cp:{key}:{}", page.saturating_sub(1)), "◀ Page", page == 0),
                button(format!("cp:{key}:{}", page + 1), "Page ▶", page + 1 >= pages),
            ]),
        ],
        false,
    ))
}

async fn inventory_card(app: &App, user_id: u64, page: usize) -> anyhow::Result<InteractionResponseData> {
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
                .map_or_else(String::new, |i| i.base.category.label().to_uppercase()),
            bar: None,
            badge: String::new(),
            shell_price: false,
        })
        .collect();
    let png = grid_png(
        app,
        "Inventory".to_owned(),
        format!("PAGE {}/{pages}", page + 1),
        format!("{total} PIECES READY TO PLACE"),
        specs,
    )
    .await?;
    let embed = EmbedBuilder::new()
        .color(BRAND)
        .title("Your inventory")
        .description("Place these with `/house edit`. Buy more with `/shop catalog` or `/shop market`.");
    let components = if pages > 1 {
        vec![row(vec![
            button(format!("iv:{}", page.saturating_sub(1)), "◀ Page", page == 0),
            button(format!("iv:{}", page + 1), "Page ▶", page + 1 >= pages),
        ])]
    } else {
        Vec::new()
    };
    Ok(Cx::card(embed, "inventory.png", png, components, true))
}

fn buy_text(outcome: BuyOutcome, name: &str, price: i64, currency: Currency) -> String {
    let unit = match currency {
        Currency::Shells => "shells",
        Currency::Pearls => "pearls",
    };
    match outcome {
        BuyOutcome::Bought { balance } => format!(
            "🛍️ Bought **{name}** for {} {unit}. {} {unit} left. Place it with `/house edit`.",
            thousands(price),
            thousands(balance)
        ),
        BuyOutcome::Insufficient { balance } => {
            format!(
                "**{name}** costs {} {unit}; you have {}.",
                thousands(price),
                thousands(balance)
            )
        }
        BuyOutcome::SoldOut => format!("**{name}** just sold out. Check back next hour!"),
    }
}

/// `mk:<hour>` select: buy one unit from that hour's market.
pub(crate) async fn on_market_buy(cx: &Cx, mut args: Split<'_, char>, value: Option<&str>) -> anyhow::Result<()> {
    let hour: i64 = args.next().and_then(|h| h.parse().ok()).unwrap_or(0);
    let Some(item_id) = value.filter(|v| *v != "none") else {
        return cx.reply_ephemeral("Pick an item to buy.").await;
    };
    if hour != market::hour_of(Utc::now().timestamp()) {
        return cx
            .reply_ephemeral("This market has closed. Open the new one with `/shop market`.")
            .await;
    }
    let content = Content::get();
    let Some(item) = content.item(item_id) else {
        return cx.reply_ephemeral("That item no longer exists.").await;
    };
    let outcome = cx.app.db.market_buy(cx.author()?.id.get(), hour, item_id).await?;
    let price = cx
        .app
        .db
        .market(hour, &[])
        .await?
        .into_iter()
        .find(|r| r.item_id == item_id)
        .map_or(item.base.price, |r| r.price);
    cx.reply_ephemeral(buy_text(outcome, &item.name(), price, Currency::Shells))
        .await
}

/// `cb:<category>` select: buy one at the catalog price. Price and availability come
/// from content, never from the client.
pub(crate) async fn on_catalog_buy(cx: &Cx, value: Option<&str>) -> anyhow::Result<()> {
    let Some(item) = value.and_then(|v| Content::get().item(v)) else {
        return cx.reply_ephemeral("Pick an item to buy.").await;
    };
    if item.base.availability != Availability::Permanent {
        return cx.reply_ephemeral("That item isn't sold in the catalog.").await;
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
    cx.reply_ephemeral(buy_text(outcome, &item.name(), item.base.price, item.base.currency))
        .await
}

/// `cp:<category>:<page>` buttons.
pub(crate) async fn on_catalog_page(cx: &Cx, mut args: Split<'_, char>) -> anyhow::Result<()> {
    let category = args.next().and_then(Category::from_key).unwrap_or(Category::Seating);
    let page = args.next().and_then(|p| p.parse().ok()).unwrap_or(0);
    let data = catalog_card(&cx.app, category, page).await?;
    cx.update(data).await
}

/// `iv:<page>` buttons.
pub(crate) async fn on_inventory_page(cx: &Cx, mut args: Split<'_, char>) -> anyhow::Result<()> {
    let page = args.next().and_then(|p| p.parse().ok()).unwrap_or(0);
    let data = inventory_card(&cx.app, cx.author()?.id.get(), page).await?;
    cx.update(data).await
}
