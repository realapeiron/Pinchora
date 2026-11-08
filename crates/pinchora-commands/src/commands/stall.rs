use pinchora_core::t;
use pinchora_db::{MAX_LISTINGS, STALL_FEE_PERCENT, StallBuy};
use pinchora_game::Content;
use twilight_interactions::command::{CommandModel, CreateCommand, ResolvedUser};
use twilight_model::{
    channel::message::{
        Component, MessageFlags,
        component::{ActionRow, SelectMenu, SelectMenuOption, SelectMenuType},
    },
    http::interaction::InteractionResponseData,
    user::User,
};
use twilight_util::builder::embed::EmbedBuilder;

use crate::{
    Cx,
    commands::{
        collection::giveable,
        shop::{SlotSpec, grid_png, soulbound_rule},
        social::ping_only,
    },
    util::{BRAND, mention, resolved_display_name, thousands_in},
};

#[derive(CommandModel, CreateCommand)]
#[command(name = "stall", desc = "Your own little shop: sell furniture to other players")]
#[allow(clippy::large_enum_variant)] // parsed once per interaction, never stored
pub enum StallCommand {
    #[command(name = "view")]
    View(StallView),
    #[command(name = "sell")]
    Sell(StallSell),
    #[command(name = "manage")]
    Manage(StallManage),
}

#[derive(CommandModel, CreateCommand)]
#[command(name = "view", desc = "Browse a player's stall")]
pub struct StallView {
    /// Whose stall (default: yours)
    user: Option<ResolvedUser>,
}

#[derive(CommandModel, CreateCommand)]
#[command(name = "sell", desc = "Put an item from your inventory on your stall")]
pub struct StallSell {
    /// Price in shells (a 5% market fee is taken on sale)
    #[command(min_value = 1, max_value = 10_000_000)]
    price: i64,
}

#[derive(CommandModel, CreateCommand)]
#[command(name = "manage", desc = "Take items back off your stall")]
pub struct StallManage;

pub(crate) fn menu(custom_id: String, placeholder: &str, options: Vec<SelectMenuOption>) -> Vec<Component> {
    if options.is_empty() {
        return Vec::new();
    }
    vec![Component::ActionRow(ActionRow {
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
            placeholder: Some(placeholder.to_owned()),
            required: None,
        })],
    })]
}

impl StallCommand {
    pub async fn run(self, cx: &Cx) -> anyhow::Result<()> {
        let me = cx.author()?.clone();
        match self {
            Self::View(c) => {
                let (seller, name) = match &c.user {
                    Some(r) => (r.resolved.clone(), resolved_display_name(r)),
                    None => (me.clone(), cx.author_display_name()),
                };
                if super::privacy::private_profile(cx, seller.id.get()).await? {
                    return Ok(());
                }
                let data = stall_card(cx, &seller, &name).await?;
                cx.reply(data).await
            }
            Self::Sell(c) => {
                let content = Content::get();
                let lang = cx.lang();
                let (rows, kept_back) = giveable(cx.app.db.inventory(me.id.get()).await?);
                let options: Vec<SelectMenuOption> = rows
                    .into_iter()
                    .filter_map(|row| {
                        let item = content.item(&row.item_id)?;
                        Some(SelectMenuOption {
                            default: false,
                            description: Some(t!(
                                lang,
                                "stall.sell-option",
                                count = row.quantity,
                                price = thousands_in(lang, item.base.price)
                            )),
                            emoji: None,
                            label: item.name_in(lang),
                            value: row.item_id,
                        })
                    })
                    .collect();
                if options.is_empty() {
                    return cx
                        .reply_ephemeral(if kept_back {
                            t!(lang, "stall.nothing-to-sell", rule = soulbound_rule(lang))
                        } else {
                            t!(lang, "inventory.empty")
                        })
                        .await;
                }
                cx.reply(InteractionResponseData {
                    content: Some(t!(
                        lang,
                        "stall.sell-prompt",
                        price = thousands_in(lang, c.price),
                        net = thousands_in(lang, c.price - c.price * STALL_FEE_PERCENT / 100),
                        fee = STALL_FEE_PERCENT
                    )),
                    components: Some(menu(
                        format!("sl:{}", c.price),
                        &t!(lang, "stall.sell-placeholder"),
                        options,
                    )),
                    flags: Some(MessageFlags::EPHEMERAL),
                    ..Default::default()
                })
                .await
            }
            Self::Manage(_) => {
                let content = Content::get();
                let lang = cx.lang();
                let options: Vec<SelectMenuOption> = cx
                    .app
                    .db
                    .listings(me.id.get())
                    .await?
                    .into_iter()
                    .map(|l| SelectMenuOption {
                        default: false,
                        description: Some(t!(lang, "stall.manage-option", price = thousands_in(lang, l.price))),
                        emoji: None,
                        label: content.item(&l.item_id).map_or(l.item_id.clone(), |i| i.name_in(lang)),
                        value: l.id.to_string(),
                    })
                    .collect();
                if options.is_empty() {
                    return cx.reply_ephemeral(t!(lang, "stall.empty")).await;
                }
                cx.reply(InteractionResponseData {
                    content: Some(t!(lang, "stall.manage-prompt")),
                    components: Some(menu("su".to_owned(), &t!(lang, "stall.manage-placeholder"), options)),
                    flags: Some(MessageFlags::EPHEMERAL),
                    ..Default::default()
                })
                .await
            }
        }
    }
}

async fn stall_card(cx: &Cx, seller: &User, name: &str) -> anyhow::Result<InteractionResponseData> {
    let content = Content::get();
    let lang = cx.lang();
    let listings = cx.app.db.listings(seller.id.get()).await?;
    let specs = listings
        .iter()
        .map(|l| SlotSpec {
            item_id: l.item_id.clone(),
            line1: thousands_in(lang, l.price),
            line2: content.item(&l.item_id).map_or_else(String::new, |i| {
                t!(lang, "stall.catalog-price", price = thousands_in(lang, i.base.price))
            }),
            bar: None,
            badge: String::new(),
            shell_price: true,
        })
        .collect();
    let png = grid_png(
        &cx.app,
        lang,
        t!(lang, "stall.title", name = name),
        t!(lang, "stall.listings", count = listings.len(), max = MAX_LISTINGS),
        t!(lang, "stall.footer", fee = STALL_FEE_PERCENT),
        specs,
    )
    .await?;
    let options = listings
        .iter()
        .map(|l| SelectMenuOption {
            default: false,
            description: Some(t!(lang, "stall.option-price", price = thousands_in(lang, l.price))),
            emoji: None,
            label: content.item(&l.item_id).map_or(l.item_id.clone(), |i| i.name_in(lang)),
            value: l.id.to_string(),
        })
        .collect();
    let embed = EmbedBuilder::new()
        .color(BRAND)
        .title(t!(lang, "stall.title", name = name))
        .description(if listings.is_empty() {
            t!(lang, "stall.nothing-for-sale")
        } else {
            t!(lang, "stall.pick-to-buy")
        });
    Ok(Cx::card(
        embed,
        "stall.png",
        png,
        menu(format!("sb:{}", seller.id), &t!(lang, "stall.buy-placeholder"), options),
        false,
    ))
}

/// `sl:<price>` select: list the chosen item.
pub(crate) async fn on_list(cx: &Cx, mut args: std::str::Split<'_, char>, value: Option<&str>) -> anyhow::Result<()> {
    if args.clone().next() == Some("eco") {
        args.next();
        return super::economy_v2::on_component(cx, args).await;
    }
    let price: i64 = args
        .next()
        .and_then(|v| v.parse().ok())
        .unwrap_or(0)
        .clamp(1, 10_000_000);
    let lang = cx.lang();
    let Some(item) = value.and_then(|v| Content::get().item(v)) else {
        return cx.reply_ephemeral(t!(lang, "stall.pick-item")).await;
    };
    if item.bound {
        return cx
            .reply_ephemeral(t!(
                lang,
                "stall.cant-list",
                name = item.name_in(lang),
                rule = soulbound_rule(lang)
            ))
            .await;
    }
    let text = if cx.app.db.list_item(cx.author()?.id.get(), &item.id(), price).await? {
        t!(
            lang,
            "stall.listed",
            name = item.name_in(lang),
            price = thousands_in(lang, price)
        )
    } else {
        t!(lang, "stall.list-failed", max = MAX_LISTINGS)
    };
    cx.update(InteractionResponseData {
        content: Some(text),
        components: Some(Vec::new()),
        ..Default::default()
    })
    .await
}

/// `su` select: take a listing back.
pub(crate) async fn on_unlist(cx: &Cx, value: Option<&str>) -> anyhow::Result<()> {
    let listing: i64 = value.and_then(|v| v.parse().ok()).unwrap_or(0);
    let lang = cx.lang();
    let text = match cx.app.db.unlist_item(cx.author()?.id.get(), listing).await? {
        Some(item) => t!(
            lang,
            "stall.took-back",
            name = Content::get().item(&item).map_or(item.clone(), |i| i.name_in(lang))
        ),
        None => t!(lang, "stall.listing-gone"),
    };
    cx.update(InteractionResponseData {
        content: Some(text),
        components: Some(Vec::new()),
        ..Default::default()
    })
    .await
}

/// `sb:<seller>` select: buy a listing.
pub(crate) async fn on_buy(cx: &Cx, value: Option<&str>) -> anyhow::Result<()> {
    let listing: i64 = value.and_then(|v| v.parse().ok()).unwrap_or(0);
    let buyer = cx.author()?.id.get();
    let lang = cx.lang();
    let text = match cx.app.db.buy_listing(buyer, listing).await? {
        StallBuy::Bought {
            item_id,
            price,
            seller,
            balance,
        } => {
            cx.app.quest(buyer, pinchora_game::quests::QuestKind::Buy).await;
            let name = Content::get()
                .item(&item_id)
                .map_or(item_id.clone(), |i| i.name_in(lang));
            // The seller hears of the sale; the buyer is not pinged for their own purchase.
            return cx
                .reply(InteractionResponseData {
                    content: Some(t!(
                        lang,
                        "stall.bought",
                        buyer = mention(buyer),
                        name = name,
                        seller = mention(seller),
                        price = thousands_in(lang, price),
                        balance = thousands_in(lang, balance)
                    )),
                    allowed_mentions: ping_only(seller as u64),
                    ..Default::default()
                })
                .await;
        }
        StallBuy::Gone => t!(lang, "stall.sold-already"),
        StallBuy::OwnListing => t!(lang, "stall.own-listing"),
        StallBuy::Insufficient { price, balance } => {
            t!(
                lang,
                "stall.insufficient",
                price = thousands_in(lang, price),
                balance = thousands_in(lang, balance)
            )
        }
    };
    cx.reply_ephemeral(text).await
}
