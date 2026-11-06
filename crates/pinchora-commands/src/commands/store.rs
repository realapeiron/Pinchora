use pinchora_core::settings;
use pinchora_db::{MAX_STORE_ITEMS, StoreBuy, StoreItem};
use twilight_http::request::AuditLogReason;
use twilight_interactions::command::{CommandModel, CreateCommand};
use twilight_model::{
    channel::message::{MessageFlags, component::SelectMenuOption},
    guild::Role,
    http::interaction::InteractionResponseData,
    id::{Id, marker::RoleMarker},
};
use twilight_util::builder::embed::{EmbedBuilder, EmbedFooterBuilder};

use crate::{
    App, Cx,
    commands::{points::capitalized, stall::menu},
    roles,
    util::{BRAND, SUCCESS, thousands},
};

#[derive(CommandModel, CreateCommand)]
#[command(name = "store", desc = "Spend your server points on roles", contexts = "guild")]
pub struct StoreCommand;

#[derive(CommandModel, CreateCommand)]
#[command(name = "store", desc = "Roles members can buy with server points")]
#[allow(clippy::large_enum_variant)] // parsed once per interaction, never stored
pub enum ConfigStore {
    #[command(name = "add")]
    Add(StoreAdd),
    #[command(name = "remove")]
    Remove(StoreRemove),
    #[command(name = "list")]
    List(StoreList),
}

#[derive(CommandModel, CreateCommand)]
#[command(name = "add", desc = "Put a role on sale, or change its price and stock")]
pub struct StoreAdd {
    /// The role to sell
    role: Role,
    /// Price in server points
    #[command(min_value = 1, max_value = 1_000_000_000)]
    price: i64,
    /// How many can be bought (default: unlimited)
    #[command(min_value = 1, max_value = 1_000_000)]
    stock: Option<i64>,
}

#[derive(CommandModel, CreateCommand)]
#[command(name = "remove", desc = "Take a role off sale")]
pub struct StoreRemove {
    /// The role to stop selling
    role: Id<RoleMarker>,
}

#[derive(CommandModel, CreateCommand)]
#[command(name = "list", desc = "Show what is on sale")]
pub struct StoreList;

/// How many are left, as the store words it.
fn stock_text(stock: Option<i32>) -> Option<String> {
    match stock {
        None => None,
        Some(0) => Some("sold out".to_owned()),
        Some(n) => Some(format!("{n} left")),
    }
}

fn line(item: &StoreItem, emoji: &str) -> String {
    let mut line = format!("<@&{}> · **{}** {emoji}", item.role_id, thousands(item.price));
    if let Some(stock) = stock_text(item.stock) {
        line.push_str(" · ");
        line.push_str(&stock);
    }
    line
}

/// The store as a public message: the items and a menu to buy one. `None` when nothing is on sale.
async fn store_message(app: &App, guild: u64) -> anyhow::Result<Option<InteractionResponseData>> {
    let items = app.db.store_items(guild).await?;
    if items.is_empty() {
        return Ok(None);
    }
    let settings = app.guild_settings(guild).await?;
    let names = app.guild_roles(guild).await;
    let options: Vec<SelectMenuOption> = items
        .iter()
        .filter(|i| i.stock != Some(0))
        .map(|i| SelectMenuOption {
            default: false,
            description: Some(format!(
                "{} {}{}",
                thousands(i.price),
                settings.points_name,
                stock_text(i.stock).map_or_else(String::new, |s| format!(" · {s}"))
            )),
            emoji: None,
            label: names
                .as_ref()
                .and_then(|n| n.get(&(i.role_id as u64)))
                .map_or_else(|| "Unknown role".to_owned(), |n| n.chars().take(100).collect()),
            value: i.id.to_string(),
        })
        .collect();
    let lines: Vec<String> = items.iter().map(|i| line(i, &settings.points_emoji)).collect();
    let embed = EmbedBuilder::new()
        .color(BRAND)
        .title("Server store")
        .description(lines.join("\n"))
        .footer(EmbedFooterBuilder::new(format!(
            "Pay with {}. Check your balance with /points show",
            settings.points_name
        )))
        .build();
    Ok(Some(InteractionResponseData {
        embeds: Some(vec![embed]),
        components: Some(menu("st".to_owned(), "Buy a role…", options)),
        ..Default::default()
    }))
}

impl StoreCommand {
    pub async fn run(self, cx: &Cx) -> anyhow::Result<()> {
        let Some(guild) = cx.guild_id() else {
            return cx.reply_ephemeral("The store lives inside a server.").await;
        };
        match store_message(&cx.app, guild).await? {
            Some(message) => cx.reply(message).await,
            None => {
                cx.reply_ephemeral("The store is empty. Server managers can add roles with `/config store add`.")
                    .await
            }
        }
    }
}

impl ConfigStore {
    pub async fn run(self, cx: &Cx, guild: u64) -> anyhow::Result<()> {
        let db = &cx.app.db;
        match self {
            Self::Add(c) => {
                let checked = settings::check_price(c.price)
                    .and_then(|price| Ok((price, c.stock.map(settings::check_stock).transpose()?)));
                let (price, stock) = match checked {
                    Ok(values) => values,
                    Err(error) => return cx.reply_ephemeral(error.message).await,
                };
                if let Some(why) = roles::unassignable(cx, guild, &c.role).await {
                    return cx.reply_ephemeral(why).await;
                }
                let Some(item) = db.add_store_item(guild, c.role.id.get(), price, stock).await? else {
                    return cx
                        .reply_ephemeral(format!(
                            "A store holds at most {MAX_STORE_ITEMS} roles. Remove one with `/config store remove` first."
                        ))
                        .await;
                };
                let settings = cx.app.guild_settings(guild).await?;
                cx.reply_ephemeral(format!("On sale now: {}", line(&item, &settings.points_emoji)))
                    .await
            }
            Self::Remove(c) => {
                let text = if db.remove_store_item(guild, c.role.get()).await? {
                    format!("<@&{}> is off sale. Members who bought it keep it.", c.role)
                } else {
                    format!("<@&{}> isn't in the store.", c.role)
                };
                cx.reply_ephemeral(text).await
            }
            Self::List(_) => {
                let items = db.store_items(guild).await?;
                if items.is_empty() {
                    return cx
                        .reply_ephemeral("Nothing on sale. Add a role with `/config store add`.")
                        .await;
                }
                let settings = cx.app.guild_settings(guild).await?;
                let lines: Vec<String> = items.iter().map(|i| line(i, &settings.points_emoji)).collect();
                let embed = EmbedBuilder::new()
                    .color(BRAND)
                    .title(format!("{} store", capitalized(&settings.points_name)))
                    .description(lines.join("\n"))
                    .footer(EmbedFooterBuilder::new(format!(
                        "{}/{MAX_STORE_ITEMS} roles",
                        items.len()
                    )))
                    .build();
                cx.reply(InteractionResponseData {
                    embeds: Some(vec![embed]),
                    flags: Some(MessageFlags::EPHEMERAL),
                    ..Default::default()
                })
                .await
            }
        }
    }
}

/// `st` select: buy the chosen role. The points are taken first; if Discord then refuses the
/// role they are given back.
pub(crate) async fn on_buy(cx: &Cx, value: Option<&str>) -> anyhow::Result<()> {
    let Some(guild) = cx.guild_id() else {
        return cx.reply_ephemeral("The store lives inside a server.").await;
    };
    let Some(item_id) = value.and_then(|v| v.parse::<i64>().ok()) else {
        return cx.reply_ephemeral("Pick something to buy.").await;
    };
    let buyer = cx.author()?.id.get();
    let db = &cx.app.db;
    let Some(item) = db.store_item(guild, item_id).await? else {
        return cx.reply_ephemeral("That role isn't for sale any more.").await;
    };
    let role = format!("<@&{}>", item.role_id);
    let has_it = cx
        .interaction
        .member
        .as_ref()
        .is_some_and(|m| m.roles.iter().any(|r| r.get() == item.role_id as u64));
    if has_it {
        return cx
            .reply_ephemeral(format!("You already have {role}, so you weren't charged."))
            .await;
    }
    let settings = cx.app.guild_settings(guild).await?;
    let (emoji, name) = (&settings.points_emoji, &settings.points_name);
    let (price, balance) = match db.buy_store_item(guild, buyer, item_id).await? {
        StoreBuy::Bought { price, balance, .. } => (price, balance),
        StoreBuy::Gone => return cx.reply_ephemeral("That role isn't for sale any more.").await,
        StoreBuy::SoldOut => return cx.reply_ephemeral(format!("{role} is sold out.")).await,
        StoreBuy::Insufficient { price, balance } => {
            return cx
                .reply_ephemeral(format!(
                    "{role} costs **{}** {emoji} {name}; you have **{}**.",
                    thousands(price),
                    thousands(balance)
                ))
                .await;
        }
    };

    let given = cx
        .app
        .http
        .add_guild_member_role(Id::new(guild), Id::new(buyer), Id::new(item.role_id as u64))
        .reason("Server store purchase")
        .await;
    if let Err(error) = given {
        tracing::warn!(guild, buyer, role = item.role_id, %error, "could not give a store role");
        let balance = db.refund_store_purchase(guild, buyer, item_id, price).await?;
        let mut text = format!(
            "I couldn't give you {role}: {}. You weren't charged; you still have **{}** {emoji} {name}.",
            roles::refusal(&error),
            thousands(balance)
        );
        if roles::role_is_gone(&error) {
            db.forget_role(guild, item.role_id as u64).await?;
            text.push_str(" The role was taken off the store.");
        }
        return cx.reply_ephemeral(text).await;
    }

    // Show the new stock to everyone, then tell the buyer privately.
    if let Some(message) = store_message(&cx.app, guild).await?
        && let Err(error) = cx.update(message).await
    {
        tracing::debug!(%error, "could not refresh the store message");
    }
    cx.reply(InteractionResponseData {
        embeds: Some(vec![
            EmbedBuilder::new()
                .color(SUCCESS)
                .description(format!(
                    "You bought {role} for **{}** {emoji} {name}. You have **{}** left.",
                    thousands(price),
                    thousands(balance)
                ))
                .build(),
        ]),
        flags: Some(MessageFlags::EPHEMERAL),
        ..Default::default()
    })
    .await
}
