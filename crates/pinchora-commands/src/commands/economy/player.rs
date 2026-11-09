//! 's internal model is routed into rewards, stall and progression; no new slash root.
use crate::Cx;
use crate::infra::util::BRAND;
use pinchora_core::economy::player::EconomySettings;
use pinchora_core::economy::player::Offer;
use pinchora_core::economy::player::TradeItem;
use pinchora_core::t;
use pinchora_db::economy::player::Action;
use pinchora_db::economy::player::EconomyAudit;
use pinchora_db::economy::player::Rejected;
use serde_json::Value;
use twilight_interactions::command::{CommandModel, CreateCommand, ResolvedUser};
use twilight_model::{
    channel::message::{
        Component, MessageFlags,
        component::{ActionRow, Button, ButtonStyle},
    },
    http::interaction::InteractionResponseData,
};
use twilight_util::builder::embed::EmbedBuilder;

#[derive(CommandModel, CreateCommand)]
#[command(name = "economyv2", desc = "Player economy")]
#[allow(clippy::large_enum_variant)]
pub enum EconomyCommand {
    #[command(name = "jobs")]
    Jobs(Jobs),
    #[command(name = "provisions")]
    Provisions(Provisions),
    #[command(name = "chest")]
    Chest(Chest),
    #[command(name = "season")]
    Season(Season),
    #[command(name = "prestige")]
    Prestige(Prestige),
    #[command(name = "donate")]
    Donate(Donate),
    #[command(name = "auction")]
    Auction(Auction),
    #[command(name = "bid")]
    Bid(Bid),
    #[command(name = "auction-close")]
    Close(Close),
    #[command(name = "trade")]
    Trade(Trade),
    #[command(name = "trade-review")]
    Review(Review),
    #[command(name = "tools")]
    Tools(Tools),
}
#[derive(CommandModel, CreateCommand)]
#[command(name = "jobs", desc = "Reef jobs: three shifts daily, promotions every ten shifts")]
pub struct Jobs {
    /// Start: kelp_keeper, coral_courier or beacon_tender; omit to view
    job: Option<String>,
}
#[derive(CommandModel, CreateCommand)]
#[command(
    name = "provisions",
    desc = "Care for provisions: ten shells per hour, capped at 120"
)]
pub struct Provisions {
    /// tend or claim; omit to view
    action: Option<String>,
}
#[derive(CommandModel, CreateCommand)]
#[command(name = "chest", desc = "Earned chests: 70% 25, 25% 50, 5% 100 shells; never sold")]
pub struct Chest {
    /// Open one earned chest
    open: Option<bool>,
}
#[derive(CommandModel, CreateCommand)]
#[command(name = "season", desc = "Free monthly season: ten tiers, 100 shells each")]
pub struct Season {
    /// Claim the next tier
    #[command(min_value = 1, max_value = 10)]
    tier: Option<i64>,
}
#[derive(CommandModel, CreateCommand)]
#[command(
    name = "prestige",
    desc = "After 40 shifts: spend 10000 shells, reset job rank, earn prestige"
)]
pub struct Prestige {
    /// Confirm spending 10000 shells and resetting job rank
    confirm: Option<bool>,
}
#[derive(CommandModel, CreateCommand)]
#[command(
    name = "donate",
    desc = "Contribute shells to this server's permanent community pool"
)]
pub struct Donate {
    /// Shells to donate; no withdrawals or personal rewards
    #[command(min_value = 1, max_value = 10_000_000)]
    amount: Option<i64>,
}
#[derive(CommandModel, CreateCommand)]
#[command(
    name = "auction",
    desc = "Browse auctions or list one item; 5% sale fee, two-minute anti-snipe"
)]
pub struct Auction {
    /// Inventory item identifier
    item: Option<String>,
    /// Starting price
    #[command(min_value = 1, max_value = 10_000_000)]
    price: Option<i64>,
    /// Duration in hours (default 24)
    #[command(min_value = 1, max_value = 72)]
    hours: Option<i64>,
}
#[derive(CommandModel, CreateCommand)]
#[command(name = "bid", desc = "Reserve a bid; outbid shells return immediately")]
pub struct Bid {
    /// Auction number
    #[command(min_value = 1)]
    id: i64,
    /// Bid in shells (minimum increase 5%, at least one)
    #[command(min_value = 1, max_value = 10_000_000)]
    amount: i64,
}
#[derive(CommandModel, CreateCommand)]
#[command(
    name = "auction-close",
    desc = "Settle an ended auction, or cancel your unbid auction"
)]
pub struct Close {
    /// Auction number
    #[command(min_value = 1)]
    id: i64,
    /// Cancel before the first bid
    cancel: Option<bool>,
}
#[derive(CommandModel, CreateCommand)]
#[command(
    name = "trade",
    desc = "Offer an immutable swap; both players must review and confirm"
)]
pub struct Trade {
    /// Trading partner
    user: ResolvedUser,
    /// Shells you offer
    #[command(min_value = 0, max_value = 10_000_000)]
    give: Option<i64>,
    /// Shells you request
    #[command(min_value = 0, max_value = 10_000_000)]
    receive: Option<i64>,
    /// One inventory item you offer
    item: Option<String>,
    /// One inventory item you request
    request_item: Option<String>,
}
#[derive(CommandModel, CreateCommand)]
#[command(
    name = "trade-review",
    desc = "Review exact items, amounts and partner before confirming a trade"
)]
pub struct Review {
    /// Trade number
    #[command(min_value = 1)]
    id: i64,
}
#[derive(CommandModel, CreateCommand)]
#[command(name = "tools", desc = "Store and points")]
pub struct Tools {
    /// Operation
    action: Option<String>,
    /// Level
    #[command(min_value = 0, max_value = 100)]
    level: Option<i64>,
    /// Days
    #[command(min_value = 0, max_value = 365)]
    days: Option<i64>,
    /// Goal
    #[command(min_value = 100, max_value = 1_000_000_000)]
    goal: Option<i64>,
    /// Open
    enabled: Option<bool>,
    /// Keep %
    #[command(min_value = 0, max_value = 100)]
    percent: Option<i64>,
    /// Preview token
    version: Option<String>,
}
fn button(id: String, label: String) -> Component {
    Component::Button(Button {
        custom_id: Some(id),
        disabled: false,
        emoji: None,
        id: None,
        label: Some(label),
        style: ButtonStyle::Primary,
        url: None,
        sku_id: None,
    })
}
async fn panel(cx: &Cx, title: String, text: String, buttons: Vec<Component>) -> anyhow::Result<()> {
    let color = match cx.guild_id() {
        Some(g) => cx.app.brand_color(g).await?,
        None => BRAND,
    };
    cx.reply(InteractionResponseData {
        embeds: Some(vec![
            EmbedBuilder::new().color(color).title(title).description(text).build(),
        ]),
        flags: Some(MessageFlags::EPHEMERAL),
        components: Some(if buttons.is_empty() {
            vec![]
        } else {
            vec![Component::ActionRow(ActionRow {
                id: None,
                components: buttons,
            })]
        }),
        ..Default::default()
    })
    .await
}
async fn execute(cx: &Cx, action: &Action) -> anyhow::Result<Option<Value>> {
    match cx
        .app
        .db
        .economy_action(
            cx.author()?.id.get(),
            &cx.interaction.id.to_string(),
            action,
            cx.app.now(),
        )
        .await
    {
        Ok(v) => Ok(Some(v)),
        Err(e) => {
            if let Some(r) = e.downcast_ref::<Rejected>() {
                cx.reply_ephemeral(t!(cx.lang(), &format!("eco.error_{}", r.0))).await?;
                Ok(None)
            } else {
                Err(e)
            }
        }
    }
}
async fn done(cx: &Cx, a: Action) -> anyhow::Result<()> {
    if let Some(v) = execute(cx, &a).await? {
        panel(
            cx,
            t!(cx.lang(), "eco.title"),
            t!(
                cx.lang(),
                "eco.done",
                amount = v.get("amount").and_then(Value::as_i64).unwrap_or(0),
                id = v.get("id").and_then(Value::as_i64).unwrap_or(0)
            ),
            vec![],
        )
        .await?;
    }
    Ok(())
}
fn offer(shells: Option<i64>, item: Option<String>) -> Offer {
    Offer {
        shells: shells.unwrap_or(0),
        items: item
            .into_iter()
            .map(|item_id| TradeItem { item_id, quantity: 1 })
            .collect(),
    }
}
impl EconomyCommand {
    pub async fn run(self, cx: &Cx) -> anyhow::Result<()> {
        let user = cx.author()?.id.get();
        let l = cx.lang();
        match self {
            Self::Jobs(c) => jobs(cx, c, user, l).await,
            Self::Provisions(c) => provisions(cx, c, l).await,
            Self::Chest(c) => chest(cx, c, l).await,
            Self::Season(c) => season(cx, c, user, l).await,
            Self::Prestige(c) => prestige(cx, c, user, l).await,
            Self::Donate(c) => donate(cx, c, l).await,
            Self::Auction(c) => auction(cx, c, user, l).await,
            Self::Bid(c) => bid(cx, c).await,
            Self::Close(c) => close_auction(cx, c).await,
            Self::Trade(c) => trade(cx, c, l).await,
            Self::Review(c) => review_trade(cx, c).await,
            Self::Tools(c) => economy_tools(cx, c).await,
        }
    }
}
async fn shift_panel(cx: &Cx, v: &Value) -> anyhow::Result<()> {
    let step = v["step"].as_i64().unwrap_or(3);
    let l = cx.lang();
    if step >= 3 {
        return panel(
            cx,
            t!(l, "eco.jobs"),
            t!(l, "eco.shift_paid", amount = v["amount"]),
            vec![],
        )
        .await;
    }
    let s = cx.app.db.economy_snapshot(cx.author()?.id.get(), cx.app.now()).await?;
    let job = s["player"]["job"].as_str().unwrap_or("kelp_keeper");
    let label = t!(l, &format!("eco.task_{job}_{step}"));
    panel(
        cx,
        t!(l, "eco.jobs"),
        t!(l, "eco.shift_help", step = step + 1, task = &label),
        vec![button(
            format!("sl:eco:step:{}:{}:{step}", cx.author()?.id, v["token"]),
            label,
        )],
    )
    .await
}
fn offer_text(v: &Value) -> String {
    let items = v["items"]
        .as_array()
        .map(|x| {
            x.iter()
                .map(|i| format!("{} × {}", i["quantity"], i["item_id"].as_str().unwrap_or("")))
                .collect::<Vec<_>>()
                .join(", ")
        })
        .unwrap_or_default();
    format!("🐚 {}\n{}", v["shells"], items)
}
async fn review(cx: &Cx, id: i64) -> anyhow::Result<()> {
    let s = cx.app.db.economy_snapshot(cx.author()?.id.get(), cx.app.now()).await?;
    let Some(t) = s["trades"].as_array().unwrap().iter().find(|t| t["id"] == id) else {
        return cx.reply_ephemeral(t!(cx.lang(), "eco.error_gone")).await;
    };
    let l = cx.lang();
    let u = cx.author()?.id;
    let text = t!(
        l,
        "eco.trade_review",
        id = id,
        a = t["user_a"].as_str().unwrap_or(""),
        b = t["user_b"].as_str().unwrap_or(""),
        give = offer_text(&t["offer_a"]),
        receive = offer_text(&t["offer_b"]),
        end = t["expires_at"].as_str().unwrap_or(""),
        confirmed_a = t!(l, if t["confirmed_a"] == true { "eco.yes" } else { "eco.no" }),
        confirmed_b = t!(l, if t["confirmed_b"] == true { "eco.yes" } else { "eco.no" })
    );
    let buttons = if t["status"] == "open" {
        vec![
            button(format!("sl:eco:confirm:{u}:{id}"), t!(l, "eco.confirm")),
            button(format!("sl:eco:cancel:{u}:{id}"), t!(l, "eco.cancel")),
        ]
    } else {
        vec![]
    };
    panel(
        cx,
        t!(l, "eco.trades"),
        format!(
            "{text}\n{}",
            t!(l, &format!("eco.status_{}", t["status"].as_str().unwrap_or("open")))
        ),
        buttons,
    )
    .await
}
pub(crate) async fn on_component(cx: &Cx, mut parts: std::str::Split<'_, char>) -> anyhow::Result<()> {
    let action = parts.next().unwrap_or("");
    let owner = parts.next().and_then(|s| s.parse::<u64>().ok());
    if owner != Some(cx.author()?.id.get()) {
        return cx.reply_ephemeral(t!(cx.lang(), "eco.error_forbidden")).await;
    }
    let id = parts.next().and_then(|s| s.parse::<i64>().ok()).unwrap_or(0);
    match action {
        "step" => {
            let step = parts.next().and_then(|s| s.parse().ok()).unwrap_or(-1);
            if let Some(v) = execute(cx, &Action::Step { token: id, step }).await? {
                shift_panel(cx, &v).await?;
            }
            Ok(())
        }
        "confirm" | "cancel" => {
            if execute(
                cx,
                &if action == "confirm" {
                    Action::ConfirmTrade { id }
                } else {
                    Action::CancelTrade { id }
                },
            )
            .await?
            .is_some()
            {
                review(cx, id).await?;
            }
            Ok(())
        }
        _ => cx.reply_ephemeral(t!(cx.lang(), "eco.error_invalid")).await,
    }
}
async fn admin(cx: &Cx, c: Tools) -> anyhow::Result<()> {
    let l = cx.lang();
    let Some(g) = cx.guild_id().filter(|_| cx.can_manage_guild()) else {
        return cx.reply_ephemeral(t!(l, "eco.error_forbidden")).await;
    };
    let before = if c.action.as_deref() == Some("scale") {
        cx.app.db.economy_points_preview(g).await?
    } else {
        cx.app.db.economy_settings(g).await?
    };
    let actor_name = cx.author_display_name();
    let actor = EconomyAudit {
        actor: cx.author()?.id.get(),
        name: &actor_name,
        source: pinchora_db::AuditSource::Discord,
    };
    let outcome: anyhow::Result<Value> = async {
        match c.action.as_deref().unwrap_or("show") {
            "save" => {
                let mut settings = EconomySettings {
                    pool_goal: before["pool_goal"].as_i64().unwrap_or(10000),
                    shop_min_level: before["shop_min_level"].as_i64().unwrap_or(0) as i32,
                    shop_min_days: before["shop_min_days"].as_i64().unwrap_or(0) as i32,
                    shop_enabled: before["shop_enabled"].as_bool().unwrap_or(true),
                };
                if let Some(v) = c.level {
                    settings.shop_min_level = v as i32;
                }
                if let Some(v) = c.days {
                    settings.shop_min_days = v as i32;
                }
                if let Some(v) = c.goal {
                    settings.pool_goal = v;
                }
                if let Some(v) = c.enabled {
                    settings.shop_enabled = v;
                }
                cx.app
                    .db
                    .save_economy_settings_audited(g, &settings, before["revision"].as_i64().unwrap_or(0), Some(&actor))
                    .await?;
                cx.app.db.economy_settings(g).await
            }
            "scale" => {
                let percent = c.percent.ok_or(Rejected("invalid"))?;
                let version = c.version.ok_or(Rejected("invalid"))?;
                cx.app
                    .db
                    .economy_scale_points_audited(g, percent as i32, &version, Some(&actor))
                    .await?;
                cx.app.db.economy_points_preview(g).await
            }
            "preview" => cx.app.db.economy_points_preview(g).await,
            "show" => Ok(before.clone()),
            _ => Err(Rejected("invalid").into()),
        }
    }
    .await;
    match outcome {
        Ok(after) => {
            let text = if let Some(version) = after["version"].as_str() {
                t!(
                    l,
                    "eco.points_preview",
                    count = after["count"],
                    total = after["total"].as_str().unwrap_or("0"),
                    version = version
                )
            } else {
                t!(
                    l,
                    "eco.settings",
                    goal = after["pool_goal"],
                    level = after["shop_min_level"],
                    days = after["shop_min_days"],
                    enabled = t!(
                        l,
                        if after["shop_enabled"] == true {
                            "eco.yes"
                        } else {
                            "eco.no"
                        }
                    )
                )
            };
            panel(cx, t!(l, "eco.tools"), text, vec![]).await
        }
        Err(e) => {
            if let Some(r) = e.downcast_ref::<Rejected>() {
                cx.reply_ephemeral(t!(l, &format!("eco.error_{}", r.0))).await
            } else {
                Err(e)
            }
        }
    }
}

async fn jobs(cx: &Cx, c: Jobs, user: u64, l: pinchora_core::infra::i18n::Lang) -> anyhow::Result<()> {
    if let Some(job) = c.job {
        if let Some(v) = execute(cx, &Action::StartShift { job }).await? {
            shift_panel(cx, &v).await?;
        }
        Ok(())
    } else {
        let s = cx.app.db.economy_snapshot(user, cx.app.now()).await?;
        panel(
            cx,
            t!(l, "eco.jobs"),
            t!(
                l,
                "eco.jobs_help",
                shifts = s["player"]["shifts"].as_i64().unwrap_or(0),
                today = s["shifts_today"]
            ),
            vec![],
        )
        .await
    }
}

async fn provisions(cx: &Cx, c: Provisions, l: pinchora_core::infra::i18n::Lang) -> anyhow::Result<()> {
    match c.action.as_deref() {
        Some("tend") => done(cx, Action::Tend).await,
        Some("claim") => done(cx, Action::Provisions).await,
        _ => panel(cx, t!(l, "eco.provisions"), t!(l, "eco.provisions_help"), vec![]).await,
    }
}

async fn chest(cx: &Cx, c: Chest, l: pinchora_core::infra::i18n::Lang) -> anyhow::Result<()> {
    if c.open == Some(true) {
        done(cx, Action::Chest).await
    } else {
        panel(cx, t!(l, "eco.chest"), t!(l, "eco.chest_help"), vec![]).await
    }
}

async fn season(cx: &Cx, c: Season, user: u64, l: pinchora_core::infra::i18n::Lang) -> anyhow::Result<()> {
    if let Some(tier) = c.tier {
        done(cx, Action::Season { tier: tier as i32 }).await
    } else {
        let s = cx.app.db.economy_snapshot(user, cx.app.now()).await?;
        let xp = if s["player"]["season"] == s["season"] {
            s["player"]["season_xp"].as_i64().unwrap_or(0)
        } else {
            0
        };
        panel(
            cx,
            t!(l, "eco.season"),
            t!(
                l,
                "eco.season_help",
                xp = xp,
                claimed = s["season_claimed"],
                end = s["season_ends"].as_str().unwrap_or("")
            ),
            vec![],
        )
        .await
    }
}

async fn prestige(cx: &Cx, c: Prestige, user: u64, l: pinchora_core::infra::i18n::Lang) -> anyhow::Result<()> {
    if c.confirm == Some(true) {
        let s = cx.app.db.economy_snapshot(user, cx.app.now()).await?;
        done(
            cx,
            Action::Prestige {
                expected: s["player"]["prestige"].as_i64().unwrap_or(0) as i32,
            },
        )
        .await
    } else {
        panel(cx, t!(l, "eco.prestige"), t!(l, "eco.prestige_help"), vec![]).await
    }
}

async fn donate(cx: &Cx, c: Donate, l: pinchora_core::infra::i18n::Lang) -> anyhow::Result<()> {
    let Some(g) = cx.guild_id() else {
        return cx.reply_ephemeral(t!(l, "eco.error_forbidden")).await;
    };
    if let Some(amount) = c.amount {
        done(
            cx,
            Action::Donate {
                guild: g.to_string(),
                amount,
            },
        )
        .await
    } else {
        let s = cx.app.db.economy_settings(g).await?;
        panel(
            cx,
            t!(l, "eco.pool"),
            t!(l, "eco.pool_help", total = s["pool_total"], goal = s["pool_goal"]),
            vec![],
        )
        .await
    }
}

async fn auction(cx: &Cx, c: Auction, user: u64, l: pinchora_core::infra::i18n::Lang) -> anyhow::Result<()> {
    if let (Some(item), Some(price)) = (c.item, c.price) {
        done(
            cx,
            Action::Auction {
                item,
                price,
                hours: c.hours.unwrap_or(24) as i32,
            },
        )
        .await
    } else {
        let s = cx.app.db.economy_snapshot(user, cx.app.now()).await?;
        let rows = s["auctions"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|a| a["status"] == "open")
            .take(10)
            .map(|a| {
                t!(
                    l,
                    "eco.auction_row",
                    id = a["id"],
                    item = a["item_id"].as_str().unwrap_or(""),
                    price = a["minimum_bid"],
                    end = a["ends_at"].as_str().unwrap_or("")
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
        panel(
            cx,
            t!(l, "eco.auctions"),
            format!("{}\n{}", t!(l, "eco.auction_help"), rows),
            vec![],
        )
        .await
    }
}

async fn bid(cx: &Cx, c: Bid) -> anyhow::Result<()> {
    done(
        cx,
        Action::Bid {
            id: c.id,
            amount: c.amount,
        },
    )
    .await
}

async fn close_auction(cx: &Cx, c: Close) -> anyhow::Result<()> {
    done(
        cx,
        if c.cancel == Some(true) {
            Action::CancelAuction { id: c.id }
        } else {
            Action::SettleAuction { id: c.id }
        },
    )
    .await
}

async fn trade(cx: &Cx, c: Trade, l: pinchora_core::infra::i18n::Lang) -> anyhow::Result<()> {
    if c.user.resolved.bot {
        return cx.reply_ephemeral(t!(l, "eco.error_invalid")).await;
    }
    if let Some(v) = execute(
        cx,
        &Action::Trade {
            to: c.user.resolved.id.to_string(),
            give: offer(c.give, c.item),
            receive: offer(c.receive, c.request_item),
        },
    )
    .await?
    {
        review(cx, v["id"].as_i64().unwrap()).await?;
    }
    Ok(())
}

async fn review_trade(cx: &Cx, c: Review) -> anyhow::Result<()> {
    review(cx, c.id).await
}

async fn economy_tools(cx: &Cx, c: Tools) -> anyhow::Result<()> {
    admin(cx, c).await
}
