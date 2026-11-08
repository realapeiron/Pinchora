use pinchora_core::{i18n::Lang, settings, t};
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
    util::{BRAND, SUCCESS, thousands_in},
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
fn stock_text(lang: Lang, stock: Option<i32>) -> Option<String> {
    match stock {
        None => None,
        Some(0) => Some(t!(lang, "store.sold-out")),
        Some(n) => Some(t!(lang, "store.left", count = n)),
    }
}

fn line(lang: Lang, item: &StoreItem, emoji: &str) -> String {
    let mut line = format!("<@&{}> · **{}** {emoji}", item.role_id, thousands_in(lang, item.price));
    if let Some(stock) = stock_text(lang, item.stock) {
        line.push_str(" · ");
        line.push_str(&stock);
    }
    line
}

/// The store as a public message: the items and a menu to buy one. `None` when nothing is on sale.
async fn store_message(app: &App, guild: u64, lang: Lang) -> anyhow::Result<Option<InteractionResponseData>> {
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
                "{}{}",
                t!(
                    lang,
                    "store.option",
                    price = thousands_in(lang, i.price),
                    name = settings.points_name
                ),
                stock_text(lang, i.stock).map_or_else(String::new, |s| format!(" · {s}"))
            )),
            emoji: None,
            label: names
                .as_ref()
                .and_then(|n| n.get(&(i.role_id as u64)))
                .map_or_else(|| t!(lang, "store.unknown-role"), |n| n.chars().take(100).collect()),
            value: i.id.to_string(),
        })
        .collect();
    let lines: Vec<String> = items.iter().map(|i| line(lang, i, &settings.points_emoji)).collect();
    let embed = EmbedBuilder::new()
        .color(BRAND)
        .title(t!(lang, "store.title"))
        .description(lines.join("\n"))
        .footer(EmbedFooterBuilder::new(t!(
            lang,
            "store.footer",
            name = settings.points_name
        )))
        .build();
    Ok(Some(InteractionResponseData {
        embeds: Some(vec![embed]),
        components: Some(menu("st".to_owned(), &t!(lang, "store.buy-placeholder"), options)),
        ..Default::default()
    }))
}

impl StoreCommand {
    pub async fn run(self, cx: &Cx) -> anyhow::Result<()> {
        let lang = cx.lang();
        let Some(guild) = cx.guild_id() else {
            return cx.reply_ephemeral(t!(lang, "store.guild-only")).await;
        };
        match store_message(&cx.app, guild, cx.app.guild_lang(guild).await).await? {
            Some(message) => cx.reply(message).await,
            None => cx.reply_ephemeral(t!(lang, "store.empty")).await,
        }
    }
}

impl ConfigStore {
    pub async fn run(self, cx: &Cx, guild: u64) -> anyhow::Result<()> {
        let db = &cx.app.db;
        let lang = cx.lang();
        match self {
            Self::Add(c) => {
                let checked = settings::check_price(c.price)
                    .and_then(|price| Ok((price, c.stock.map(settings::check_stock).transpose()?)));
                let (price, stock) = match checked {
                    Ok(values) => values,
                    Err(error) => return cx.reply_ephemeral(error.text(lang)).await,
                };
                if let Some(why) = roles::unassignable(cx, guild, &c.role).await {
                    return cx.reply_ephemeral(why).await;
                }
                let Some(item) = db.add_store_item(guild, c.role.id.get(), price, stock).await? else {
                    return cx
                        .reply_ephemeral(t!(lang, "store.max-roles", max = MAX_STORE_ITEMS))
                        .await;
                };
                let settings = cx.app.guild_settings(guild).await?;
                cx.reply_ephemeral(t!(
                    lang,
                    "store.on-sale",
                    line = line(lang, &item, &settings.points_emoji)
                ))
                .await
            }
            Self::Remove(c) => {
                let role = format!("<@&{}>", c.role);
                let text = if db.remove_store_item(guild, c.role.get()).await? {
                    t!(lang, "store.removed", role = role)
                } else {
                    t!(lang, "store.not-in-store", role = role)
                };
                cx.reply_ephemeral(text).await
            }
            Self::List(_) => {
                let items = db.store_items(guild).await?;
                if items.is_empty() {
                    return cx.reply_ephemeral(t!(lang, "store.list-empty")).await;
                }
                let settings = cx.app.guild_settings(guild).await?;
                let lines: Vec<String> = items.iter().map(|i| line(lang, i, &settings.points_emoji)).collect();
                let embed = EmbedBuilder::new()
                    .color(BRAND)
                    .title(t!(
                        lang,
                        "store.list-title",
                        name = capitalized(&settings.points_name, lang)
                    ))
                    .description(lines.join("\n"))
                    .footer(EmbedFooterBuilder::new(t!(
                        lang,
                        "store.list-footer",
                        count = items.len(),
                        max = MAX_STORE_ITEMS
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
    let lang = cx.lang();
    let Some(guild) = cx.guild_id() else {
        return cx.reply_ephemeral(t!(lang, "store.guild-only")).await;
    };
    let Some(item_id) = value.and_then(|v| v.parse::<i64>().ok()) else {
        return cx.reply_ephemeral(t!(lang, "store.pick")).await;
    };
    let buyer = cx.author()?.id.get();
    let db = &cx.app.db;
    let Some(item) = db.store_item(guild, item_id).await? else {
        return cx.reply_ephemeral(t!(lang, "store.gone")).await;
    };
    let role = format!("<@&{}>", item.role_id);
    let has_it = cx
        .interaction
        .member
        .as_ref()
        .is_some_and(|m| m.roles.iter().any(|r| r.get() == item.role_id as u64));
    if has_it {
        return cx.reply_ephemeral(t!(lang, "store.already-have", role = role)).await;
    }
    let settings = cx.app.guild_settings(guild).await?;
    let (emoji, name) = (&settings.points_emoji, &settings.points_name);
    let (price, balance) = match db.buy_store_item(guild, buyer, item_id).await? {
        StoreBuy::Bought { price, balance, .. } => (price, balance),
        StoreBuy::Conditions => return cx.reply_ephemeral(t!(lang, "eco.store_conditions")).await,
        StoreBuy::Gone => return cx.reply_ephemeral(t!(lang, "store.gone")).await,
        StoreBuy::SoldOut => return cx.reply_ephemeral(t!(lang, "store.role-sold-out", role = role)).await,
        StoreBuy::Insufficient { price, balance } => {
            return cx
                .reply_ephemeral(t!(
                    lang,
                    "store.insufficient",
                    role = role,
                    price = thousands_in(lang, price),
                    emoji = emoji,
                    name = name,
                    balance = thousands_in(lang, balance)
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
        let mut text = t!(
            lang,
            "store.couldnt-give",
            role = role,
            why = roles::refusal_in(lang, &error),
            balance = thousands_in(lang, balance),
            emoji = emoji,
            name = name
        );
        if roles::role_is_gone(&error) {
            db.forget_role(guild, item.role_id as u64).await?;
            text.push_str(&t!(lang, "store.role-removed"));
        }
        return cx.reply_ephemeral(text).await;
    }

    // Show the new stock to everyone, in the server's language since the message is nobody's in particular,
    // then tell the buyer privately in theirs.
    let shared = cx.app.guild_lang(guild).await;
    if let Some(message) = store_message(&cx.app, guild, shared).await?
        && let Err(error) = cx.update(message).await
    {
        tracing::debug!(%error, "could not refresh the store message");
    }
    cx.reply(InteractionResponseData {
        embeds: Some(vec![
            EmbedBuilder::new()
                .color(SUCCESS)
                .description(t!(
                    lang,
                    "store.bought",
                    role = role,
                    price = thousands_in(lang, price),
                    emoji = emoji,
                    name = name,
                    balance = thousands_in(lang, balance)
                ))
                .build(),
        ]),
        flags: Some(MessageFlags::EPHEMERAL),
        ..Default::default()
    })
    .await
}
