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
        collection::{SOULBOUND_RULE, giveable},
        shop::{SlotSpec, grid_png},
        social::ping_only,
    },
    util::{BRAND, mention, resolved_display_name, thousands},
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
                let (rows, kept_back) = giveable(cx.app.db.inventory(me.id.get()).await?);
                let options: Vec<SelectMenuOption> = rows
                    .into_iter()
                    .filter_map(|row| {
                        let item = content.item(&row.item_id)?;
                        Some(SelectMenuOption {
                            default: false,
                            description: Some(format!(
                                "You have {} · catalog price {}",
                                row.quantity,
                                thousands(item.base.price)
                            )),
                            emoji: None,
                            label: item.name(),
                            value: row.item_id,
                        })
                    })
                    .collect();
                if options.is_empty() {
                    return cx
                        .reply_ephemeral(if kept_back {
                            format!("Nothing in your inventory can go on a stall. {SOULBOUND_RULE}")
                        } else {
                            "Your inventory is empty.".to_owned()
                        })
                        .await;
                }
                cx.reply(InteractionResponseData {
                    content: Some(format!(
                        "Which item do you want to sell for **{}** shells? (You receive {} after the {STALL_FEE_PERCENT}% fee.)",
                        thousands(c.price),
                        thousands(c.price - c.price * STALL_FEE_PERCENT / 100)
                    )),
                    components: Some(menu(format!("sl:{}", c.price), "Choose an item…", options)),
                    flags: Some(MessageFlags::EPHEMERAL),
                    ..Default::default()
                })
                .await
            }
            Self::Manage(_) => {
                let content = Content::get();
                let options: Vec<SelectMenuOption> = cx
                    .app
                    .db
                    .listings(me.id.get())
                    .await?
                    .into_iter()
                    .map(|l| SelectMenuOption {
                        default: false,
                        description: Some(format!("Listed for {} shells", thousands(l.price))),
                        emoji: None,
                        label: content.item(&l.item_id).map_or(l.item_id.clone(), |i| i.name()),
                        value: l.id.to_string(),
                    })
                    .collect();
                if options.is_empty() {
                    return cx
                        .reply_ephemeral("Your stall is empty. Add items with `/stall sell`.")
                        .await;
                }
                cx.reply(InteractionResponseData {
                    content: Some("Pick a listing to take back into your inventory.".to_owned()),
                    components: Some(menu("su".to_owned(), "Take back…", options)),
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
    let listings = cx.app.db.listings(seller.id.get()).await?;
    let specs = listings
        .iter()
        .map(|l| SlotSpec {
            item_id: l.item_id.clone(),
            line1: thousands(l.price),
            line2: content
                .item(&l.item_id)
                .map_or_else(String::new, |i| format!("CATALOG {}", thousands(i.base.price))),
            bar: None,
            badge: String::new(),
            shell_price: true,
        })
        .collect();
    let png = grid_png(
        &cx.app,
        format!("{name}'s stall"),
        format!("{}/{MAX_LISTINGS} LISTINGS", listings.len()),
        "PLAYER SHOP · 5% MARKET FEE".to_owned(),
        specs,
    )
    .await?;
    let options = listings
        .iter()
        .map(|l| SelectMenuOption {
            default: false,
            description: Some(format!("{} shells", thousands(l.price))),
            emoji: None,
            label: content.item(&l.item_id).map_or(l.item_id.clone(), |i| i.name()),
            value: l.id.to_string(),
        })
        .collect();
    let embed = EmbedBuilder::new()
        .color(BRAND)
        .title(format!("{name}'s stall"))
        .description(if listings.is_empty() {
            "Nothing for sale right now.".to_owned()
        } else {
            "Pick something below to buy it.".to_owned()
        });
    Ok(Cx::card(
        embed,
        "stall.png",
        png,
        menu(format!("sb:{}", seller.id), "Buy from this stall…", options),
        false,
    ))
}

/// `sl:<price>` select: list the chosen item.
pub(crate) async fn on_list(cx: &Cx, mut args: std::str::Split<'_, char>, value: Option<&str>) -> anyhow::Result<()> {
    let price: i64 = args
        .next()
        .and_then(|v| v.parse().ok())
        .unwrap_or(0)
        .clamp(1, 10_000_000);
    let Some(item) = value.and_then(|v| Content::get().item(v)) else {
        return cx.reply_ephemeral("Pick an item.").await;
    };
    if item.bound {
        return cx
            .reply_ephemeral(format!("**{}** can't go on a stall. {SOULBOUND_RULE}", item.name()))
            .await;
    }
    let text = if cx.app.db.list_item(cx.author()?.id.get(), &item.id(), price).await? {
        format!(
            "🏷️ **{}** is on your stall for {} shells.",
            item.name(),
            thousands(price)
        )
    } else {
        format!("Couldn't list it: either your stall is full ({MAX_LISTINGS} items) or you no longer have one.")
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
    let text = match cx.app.db.unlist_item(cx.author()?.id.get(), listing).await? {
        Some(item) => format!(
            "Took **{}** back into your inventory.",
            Content::get().item(&item).map_or(item.clone(), |i| i.name())
        ),
        None => "That listing is gone.".to_owned(),
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
    let text = match cx.app.db.buy_listing(buyer, listing).await? {
        StallBuy::Bought {
            item_id,
            price,
            seller,
            balance,
        } => {
            cx.app.quest(buyer, pinchora_game::quests::QuestKind::Buy).await;
            let name = Content::get().item(&item_id).map_or(item_id.clone(), |i| i.name());
            // The seller hears of the sale; the buyer is not pinged for their own purchase.
            return cx
                .reply(InteractionResponseData {
                    content: Some(format!(
                        "🤝 {} bought **{name}** from {}'s stall for {} shells. ({} left)",
                        mention(buyer),
                        mention(seller),
                        thousands(price),
                        thousands(balance)
                    )),
                    allowed_mentions: ping_only(seller as u64),
                    ..Default::default()
                })
                .await;
        }
        StallBuy::Gone => "Someone was faster: that item is already sold.".to_owned(),
        StallBuy::OwnListing => "That's your own listing. Use `/stall manage` to take it back.".to_owned(),
        StallBuy::Insufficient { price, balance } => {
            format!("It costs {} shells; you have {}.", thousands(price), thousands(balance))
        }
    };
    cx.reply_ephemeral(text).await
}
