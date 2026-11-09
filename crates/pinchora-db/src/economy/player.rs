//! transactions shared by Discord and the web. Player locks always precede object rows.
use crate::Db;
use crate::economy::legacy::ledger;
use crate::infra::locks::lock_users;
use crate::infra::locks::retry;
use crate::infra::locks::try_lock_users;
use chrono::{DateTime, Datelike, Duration, Utc};
use pinchora_core::economy::player as rules;
use pinchora_core::economy::player::EconomySettings;
use pinchora_core::economy::player::Offer;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::{Postgres, Transaction};

/// Audit identity for an admin mutation; the change and audit row share one commit.
pub struct EconomyAudit<'a> {
    pub actor: u64,
    pub name: &'a str,
    pub source: crate::AuditSource,
}
async fn admin_audit(
    tx: &mut Tx<'_>,
    guild: u64,
    actor: Option<&EconomyAudit<'_>>,
    action: &str,
    before: Value,
    after: Value,
) -> anyhow::Result<()> {
    if let Some(actor) = actor {
        crate::moderation::audit::insert_in(
            tx,
            &crate::NewAudit {
                guild,
                actor: actor.actor,
                actor_name: actor.name,
                source: actor.source,
                action,
                target: None,
                before: Some(&before.to_string()),
                after: Some(&after.to_string()),
            },
        )
        .await?;
    }
    Ok(())
}

type Tx<'a> = Transaction<'a, Postgres>;
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum Action {
    StartShift { job: String },
    Step { token: i64, step: i32 },
    Tend,
    Provisions,
    Chest,
    Season { tier: i32 },
    Prestige { expected: i32 },
    Auction { item: String, price: i64, hours: i32 },
    Bid { id: i64, amount: i64 },
    CancelAuction { id: i64 },
    SettleAuction { id: i64 },
    Trade { to: String, give: Offer, receive: Offer },
    ConfirmTrade { id: i64 },
    CancelTrade { id: i64 },
    Donate { guild: String, amount: i64 },
}
#[derive(Debug)]
pub struct Rejected(pub &'static str);
impl std::fmt::Display for Rejected {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.0)
    }
}
impl std::error::Error for Rejected {}
fn require(ok: bool, key: &'static str) -> anyhow::Result<()> {
    if ok { Ok(()) } else { Err(Rejected(key).into()) }
}
fn id(raw: &str) -> anyhow::Result<i64> {
    raw.parse::<i64>()
        .ok()
        .filter(|x| *x > 0)
        .ok_or_else(|| Rejected("invalid").into())
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Player {
    pub job: String,
    pub shifts: i32,
    pub prestige: i32,
    pub chests: i32,
    pub provision_at: Option<DateTime<Utc>>,
    pub provision_claimed: i32,
    pub shift_token: i64,
    pub shift_step: i32,
    pub shift_started: Option<DateTime<Utc>>,
    pub season: i32,
    pub season_xp: i32,
}
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Auction {
    pub id: i64,
    pub seller_id: i64,
    pub item_id: String,
    pub start_price: i64,
    pub bid: i64,
    pub bidder_id: Option<i64>,
    pub ends_at: DateTime<Utc>,
    pub hard_end: DateTime<Utc>,
    pub status: String,
}
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Trade {
    pub id: i64,
    pub user_a: i64,
    pub user_b: i64,
    pub offer_a: Value,
    pub offer_b: Value,
    pub confirmed_a: bool,
    pub confirmed_b: bool,
    pub held_b: bool,
    pub expires_at: DateTime<Utc>,
    pub status: String,
}
async fn ensure_player(tx: &mut Tx<'_>, user: i64) -> anyhow::Result<()> {
    sqlx::query("INSERT INTO users(id) VALUES($1) ON CONFLICT DO NOTHING")
        .bind(user)
        .execute(&mut **tx)
        .await?;
    sqlx::query("INSERT INTO economy_players(user_id) VALUES($1) ON CONFLICT DO NOTHING")
        .bind(user)
        .execute(&mut **tx)
        .await?;
    Ok(())
}
/// The two sides are inserted together; every balance/inventory write and its pair commit together.
pub(crate) async fn movement(tx: &mut Tx<'_>, user: i64, asset: &str, delta: i64, reason: &str) -> anyhow::Result<()> {
    if delta == 0 {
        return Ok(());
    }
    let account = match reason {
        "job_shift" | "provisions" | "earned_chest" | "season_pass" | "job_chest" | "collection_reward" => "source",
        "donation_pool" | "prestige" | "auction_fee" | "chest_open" | "collection_upgrade" => "sink",
        "collection_recycle" | "collection_craft" | "collection_exchange" if delta > 0 => "source",
        "collection_recycle" | "collection_craft" | "collection_exchange" => "sink",
        _ => "escrow",
    };
    sqlx::query("WITH m AS MATERIALIZED (SELECT nextval('economy_movement_ids') AS id) INSERT INTO economy_ledger(movement,user_id,account,asset,delta,reason) SELECT id,$1,'wallet',$2,$3,$4 FROM m UNION ALL SELECT id,$1,$5,$2,-$3,$4 FROM m")
        .bind(user).bind(asset).bind(delta).bind(reason).bind(account).execute(&mut **tx).await?;
    Ok(())
}
async fn money_record(tx: &mut Tx<'_>, user: i64, delta: i64, reason: &str) -> anyhow::Result<()> {
    if delta == 0 {
        return Ok(());
    }
    ledger(tx, user, delta, reason).await?;
    movement(tx, user, "shells", delta, reason).await
}
/// Numeric comparison prevents SQL overflow before evaluating the guarded addition.
async fn balance(tx: &mut Tx<'_>, user: i64, delta: i64) -> anyhow::Result<()> {
    let n = sqlx::query(
        "UPDATE users SET credits=credits+$2 WHERE id=$1 AND credits::numeric+$2 BETWEEN 0 AND 9223372036854775807",
    )
    .bind(user)
    .bind(delta)
    .execute(&mut **tx)
    .await?
    .rows_affected();
    require(n == 1, "balance")
}
async fn money(tx: &mut Tx<'_>, user: i64, delta: i64, reason: &str) -> anyhow::Result<()> {
    if delta == 0 {
        return Ok(());
    }
    balance(tx, user, delta).await?;
    money_record(tx, user, delta, reason).await
}
async fn inventory_credit(tx: &mut Tx<'_>, user: i64, item: &str, quantity: i32, reason: &str) -> anyhow::Result<()> {
    let n = sqlx::query("INSERT INTO inventory(user_id,item_id,quantity) VALUES($1,$2,$3) ON CONFLICT(user_id,item_id) DO UPDATE SET quantity=inventory.quantity+EXCLUDED.quantity,acquired_at=now() WHERE inventory.quantity::bigint+EXCLUDED.quantity<=2147483647")
        .bind(user).bind(item).bind(quantity).execute(&mut **tx).await?.rows_affected();
    require(n == 1, "inventory")?;
    movement(tx, user, &format!("item:{item}"), i64::from(quantity), reason).await
}
async fn take_offer(tx: &mut Tx<'_>, user: i64, offer: &Offer, reason: &str) -> anyhow::Result<()> {
    require(offer.valid(), "invalid")?;
    money(tx, user, -offer.shells, reason).await?;
    for i in &offer.items {
        let n =
            sqlx::query("UPDATE inventory SET quantity=quantity-$3 WHERE user_id=$1 AND item_id=$2 AND quantity >= $3")
                .bind(user)
                .bind(&i.item_id)
                .bind(i.quantity)
                .execute(&mut **tx)
                .await?
                .rows_affected();
        require(n == 1, "inventory")?;
        movement(tx, user, &format!("item:{}", i.item_id), -i64::from(i.quantity), reason).await?;
    }
    Ok(())
}
async fn give_offer(tx: &mut Tx<'_>, user: i64, offer: &Offer, reason: &str) -> anyhow::Result<()> {
    money(tx, user, offer.shells, reason).await?;
    for i in &offer.items {
        inventory_credit(tx, user, &i.item_id, i.quantity, reason).await?;
    }
    Ok(())
}
async fn auction(tx: &mut Tx<'_>, id: i64) -> anyhow::Result<Auction> {
    sqlx::query_as("SELECT * FROM economy_auctions WHERE id=$1 FOR UPDATE")
        .bind(id)
        .fetch_optional(&mut **tx)
        .await?
        .ok_or_else(|| Rejected("gone").into())
}
async fn trade(tx: &mut Tx<'_>, id: i64) -> anyhow::Result<Trade> {
    sqlx::query_as("SELECT * FROM economy_trades WHERE id=$1 FOR UPDATE")
        .bind(id)
        .fetch_optional(&mut **tx)
        .await?
        .ok_or_else(|| Rejected("gone").into())
}
impl Db {
    pub async fn economy_action(
        &self,
        user: u64,
        request: &str,
        action: &Action,
        now: DateTime<Utc>,
    ) -> anyhow::Result<Value> {
        require(
            user > 0
                && user <= i64::MAX as u64
                && !request.is_empty()
                && request.len() <= 100
                && !request.starts_with("worker:"),
            "invalid",
        )?;
        retry("economy_v2", || {
            self.economy_action_once(user as i64, request, action, now)
        })
        .await
    }
    async fn economy_action_once(
        &self,
        user: i64,
        request: &str,
        action: &Action,
        now: DateTime<Utc>,
    ) -> anyhow::Result<Value> {
        let mut tx = self.pool().begin().await?;
        let mut users = vec![user];
        match action {
            Action::Trade { to, .. } => users.push(id(to)?),
            Action::Bid { id, .. } | Action::CancelAuction { id } | Action::SettleAuction { id } => {
                if let Some(a) = sqlx::query_as::<_, Auction>("SELECT * FROM economy_auctions WHERE id=$1")
                    .bind(id)
                    .fetch_optional(&mut *tx)
                    .await?
                {
                    users.push(a.seller_id);
                    users.extend(a.bidder_id);
                }
            }
            Action::ConfirmTrade { id } | Action::CancelTrade { id } => {
                if let Some(t) = sqlx::query_as::<_, Trade>("SELECT * FROM economy_trades WHERE id=$1")
                    .bind(id)
                    .fetch_optional(&mut *tx)
                    .await?
                {
                    users.extend([t.user_a, t.user_b]);
                }
            }
            _ => {}
        }
        lock_users(&mut tx, &users).await?;
        // Keep an action fingerprint, not another player's ID in the receipt JSON.
        let fingerprint: String = sqlx::query_scalar("SELECT encode(sha256(convert_to($1,'UTF8')),'hex')")
            .bind(serde_json::to_string(action)?)
            .fetch_one(&mut *tx)
            .await?;
        if let Some(saved) =
            sqlx::query_scalar::<_, Value>("SELECT result FROM economy_requests WHERE user_id=$1 AND request_id=$2")
                .bind(user)
                .bind(request)
                .fetch_optional(&mut *tx)
                .await?
        {
            require(saved["request"] == fingerprint, "invalid")?;
            return Ok(saved["response"].clone());
        }
        ensure_player(&mut tx, user).await?;
        let mut p: Player = sqlx::query_as("SELECT * FROM economy_players WHERE user_id=$1 FOR UPDATE")
            .bind(user)
            .fetch_one(&mut *tx)
            .await?;
        let season = rules::season(now);
        if p.season != season {
            p.season = season;
            p.season_xp = 0;
            sqlx::query("UPDATE economy_players SET season=$2,season_xp=0 WHERE user_id=$1")
                .bind(user)
                .bind(season)
                .execute(&mut *tx)
                .await?;
        }
        let day = (now.timestamp().div_euclid(86400)) as i32;
        sqlx::query("INSERT INTO economy_limits(user_id,day) VALUES($1,$2) ON CONFLICT DO NOTHING")
            .bind(user)
            .bind(day)
            .execute(&mut *tx)
            .await?;
        let (shifts, last): (i32, Option<DateTime<Utc>>) =
            sqlx::query_as("SELECT shifts,last_shift FROM economy_limits WHERE user_id=$1 AND day=$2 FOR UPDATE")
                .bind(user)
                .bind(day)
                .fetch_one(&mut *tx)
                .await?;
        let response = match action {
            Action::StartShift { job } => {
                require(rules::JOBS.contains(&job.as_str()), "invalid")?;
                require(shifts < rules::SHIFTS_PER_DAY, "cap")?;
                let latest: Option<DateTime<Utc>> =
                    sqlx::query_scalar("SELECT max(last_shift) FROM economy_limits WHERE user_id=$1")
                        .bind(user)
                        .fetch_one(&mut *tx)
                        .await?;
                require(
                    latest.is_none_or(|t| now >= t + Duration::seconds(rules::SHIFT_COOLDOWN_SECS)),
                    "cooldown",
                )?;
                if p.shift_step < 3 && p.shift_started.is_some_and(|t| now < t + Duration::minutes(5)) {
                    json!({"token":p.shift_token,"step":p.shift_step,"job":p.job})
                } else {
                    let token:i64=sqlx::query_scalar("UPDATE economy_players SET job=$2,shift_started=$3,shift_step=0,shift_token=nextval('economy_shift_tokens') WHERE user_id=$1 RETURNING shift_token")
      .bind(user).bind(job).bind(now).fetch_one(&mut *tx).await?;
                    json!({"token":token,"step":0,"job":job})
                }
            }
            Action::Step { token, step } => {
                require(
                    *token == p.shift_token && *step == p.shift_step && (0..3).contains(step),
                    "changed",
                )?;
                require(
                    p.shift_started
                        .is_some_and(|t| now >= t && now < t + Duration::minutes(5)),
                    "expired",
                )?;
                let mut amount = 0;
                if *step == 2 {
                    require(
                        shifts < rules::SHIFTS_PER_DAY && last.is_none_or(|t| now >= t + Duration::hours(1)),
                        "cap",
                    )?;
                    amount = rules::wage(p.shifts);
                    money(&mut tx, user, amount, "job_shift").await?;
                    sqlx::query("UPDATE economy_limits SET shifts=shifts+1,last_shift=$3 WHERE user_id=$1 AND day=$2")
                        .bind(user)
                        .bind(day)
                        .bind(now)
                        .execute(&mut *tx)
                        .await?;
                    sqlx::query("UPDATE economy_players SET shifts=LEAST(shifts,999999)+1,season_xp=LEAST(1000,season_xp+10),chests=LEAST(10,chests+CASE WHEN (shifts+1)%5=0 THEN 1 ELSE 0 END) WHERE user_id=$1").bind(user).execute(&mut *tx).await?;
                    if p.shifts % 5 == 4 && p.chests < 10 {
                        movement(&mut tx, user, "chests", 1, "job_chest").await?;
                    }
                }
                sqlx::query("UPDATE economy_players SET shift_step=shift_step+1 WHERE user_id=$1")
                    .bind(user)
                    .execute(&mut *tx)
                    .await?;
                json!({"token":token,"step":step+1,"amount":amount})
            }
            Action::Tend => {
                require(
                    p.provision_at.is_none_or(|t| now >= t + Duration::hours(12))
                        && (p.provision_at.is_none() || p.provision_claimed == 120),
                    "provisions_pending",
                )?;
                sqlx::query("UPDATE economy_players SET provision_at=$2,provision_claimed=0 WHERE user_id=$1")
                    .bind(user)
                    .bind(now)
                    .execute(&mut *tx)
                    .await?;
                json!({"amount":0})
            }
            Action::Provisions => {
                let accrued = p
                    .provision_at
                    .map_or(0, |t| ((now - t).num_hours() * 10).clamp(0, rules::PROVISION_CAP));
                let amount = accrued - i64::from(p.provision_claimed);
                require(amount > 0, "cooldown")?;
                money(&mut tx, user, amount, "provisions").await?;
                sqlx::query("UPDATE economy_players SET provision_claimed=$2 WHERE user_id=$1")
                    .bind(user)
                    .bind(accrued as i32)
                    .execute(&mut *tx)
                    .await?;
                json!({"amount":amount})
            }
            Action::Chest => {
                require(p.chests > 0, "chest_empty")?;
                let roll: i32 = sqlx::query_scalar("SELECT floor(random()*100)::integer")
                    .fetch_one(&mut *tx)
                    .await?;
                let amount = rules::chest_reward(roll as u32);
                money(&mut tx, user, amount, "earned_chest").await?;
                sqlx::query("UPDATE economy_players SET chests=chests-1 WHERE user_id=$1")
                    .bind(user)
                    .execute(&mut *tx)
                    .await?;
                movement(&mut tx, user, "chests", -1, "chest_open").await?;
                json!({"amount":amount})
            }
            Action::Season { tier } => {
                require((1..=10).contains(tier) && p.season_xp >= tier * 50, "season_locked")?;
                let end = now
                    .date_naive()
                    .with_day(1)
                    .unwrap()
                    .checked_add_months(chrono::Months::new(1))
                    .unwrap()
                    .and_hms_opt(0, 0, 0)
                    .unwrap()
                    .and_utc()
                    .timestamp()
                    / 86400
                    - 1;
                sqlx::query("INSERT INTO economy_limits(user_id,day) VALUES($1,$2) ON CONFLICT DO NOTHING")
                    .bind(user)
                    .bind(end as i32)
                    .execute(&mut *tx)
                    .await?;
                let claimed: i32 = sqlx::query_scalar(
                    "SELECT season_claimed FROM economy_limits WHERE user_id=$1 AND day=$2 FOR UPDATE",
                )
                .bind(user)
                .bind(end as i32)
                .fetch_one(&mut *tx)
                .await?;
                require(*tier == claimed + 1, "claimed")?;
                money(&mut tx, user, 100, "season_pass").await?;
                sqlx::query("UPDATE economy_limits SET season_claimed=$3 WHERE user_id=$1 AND day=$2")
                    .bind(user)
                    .bind(end as i32)
                    .bind(tier)
                    .execute(&mut *tx)
                    .await?;
                json!({"amount":100,"tier":tier})
            }
            Action::Prestige { expected } => {
                require(
                    *expected == p.prestige && p.prestige < 100 && p.shifts >= 40,
                    "prestige_locked",
                )?;
                money(&mut tx, user, -rules::PRESTIGE_COST, "prestige").await?;
                sqlx::query("UPDATE economy_players SET prestige=prestige+1,shifts=0,shift_step=3 WHERE user_id=$1")
                    .bind(user)
                    .execute(&mut *tx)
                    .await?;
                json!({"prestige":p.prestige+1,"amount":-rules::PRESTIGE_COST})
            }
            Action::Auction { item, price, hours } => {
                require(
                    (1..=rules::MAX_PRICE).contains(price) && (1..=72).contains(hours),
                    "invalid",
                )?;
                let count: i64 =
                    sqlx::query_scalar("SELECT count(*) FROM economy_auctions WHERE seller_id=$1 AND status='open'")
                        .bind(user)
                        .fetch_one(&mut *tx)
                        .await?;
                require(count < 8, "cap")?;
                take_offer(
                    &mut tx,
                    user,
                    &Offer {
                        shells: 0,
                        items: vec![rules::TradeItem {
                            item_id: item.clone(),
                            quantity: 1,
                        }],
                    },
                    "auction_item",
                )
                .await?;
                let end = now + Duration::hours(i64::from(*hours));
                let id:i64=sqlx::query_scalar("INSERT INTO economy_auctions(seller_id,item_id,start_price,ends_at,hard_end) VALUES($1,$2,$3,$4,$5) RETURNING id")
     .bind(user).bind(item).bind(price).bind(end).bind(end+Duration::minutes(30)).fetch_one(&mut *tx).await?;
                json!({"id":id})
            }
            Action::Bid { id, amount } => {
                let a = auction(&mut tx, *id).await?;
                require(a.status == "open" && now < a.ends_at && a.seller_id != user, "gone")?;
                let minimum = if a.bidder_id.is_none() {
                    a.start_price
                } else {
                    rules::minimum_bid(a.bid)
                };
                require((minimum..=rules::MAX_PRICE).contains(amount), "bid_low")?;
                if let Some(old) = a.bidder_id {
                    if !users.contains(&old) {
                        try_lock_users(&mut tx, &[old]).await?;
                    }
                    money(&mut tx, old, a.bid, "auction_refund").await?;
                }
                money(&mut tx, user, -amount, "auction_escrow").await?;
                let end = if a.ends_at - now <= Duration::minutes(2) {
                    (now + Duration::minutes(2)).min(a.hard_end)
                } else {
                    a.ends_at
                };
                sqlx::query("UPDATE economy_auctions SET bid=$2,bidder_id=$3,ends_at=$4 WHERE id=$1")
                    .bind(id)
                    .bind(amount)
                    .bind(user)
                    .bind(end)
                    .execute(&mut *tx)
                    .await?;
                json!({"id":id,"bid":amount,"ends_at":end})
            }
            Action::CancelAuction { id } | Action::SettleAuction { id } => {
                let a = auction(&mut tx, *id).await?;
                require(a.status == "open", "gone")?;
                if let Some(b) = a.bidder_id
                    && !users.contains(&b)
                {
                    try_lock_users(&mut tx, &[b]).await?;
                }
                let cancel = matches!(action, Action::CancelAuction { .. });
                if cancel {
                    require(
                        a.seller_id == user && a.bidder_id.is_none() && now < a.ends_at,
                        "auction_cancel",
                    )?;
                } else {
                    require(now >= a.ends_at, "cooldown")?;
                }
                let status = if cancel {
                    "cancelled"
                } else if a.bidder_id.is_some() {
                    "sold"
                } else {
                    "expired"
                };
                if let Some(b) = a.bidder_id {
                    // Gross proceeds and the fee are separate entries so the sink is inspectable.
                    balance(&mut tx, a.seller_id, a.bid - rules::fee(a.bid)).await?;
                    money_record(&mut tx, a.seller_id, a.bid, "auction_sale").await?;
                    money_record(&mut tx, a.seller_id, -rules::fee(a.bid), "auction_fee").await?;
                    inventory_credit(&mut tx, b, &a.item_id, 1, "auction_delivery").await?;
                } else {
                    inventory_credit(&mut tx, a.seller_id, &a.item_id, 1, "auction_return").await?;
                }
                sqlx::query("UPDATE economy_auctions SET status=$2 WHERE id=$1")
                    .bind(id)
                    .bind(status)
                    .execute(&mut *tx)
                    .await?;
                json!({"id":id,"status":status})
            }
            Action::Trade { to, give, receive } => {
                let other = id(to)?;
                require(
                    other != user && give.valid() && receive.valid() && !(give.empty() && receive.empty()),
                    "invalid",
                )?;
                let count: i64 = sqlx::query_scalar(
                    "SELECT count(*) FROM economy_trades WHERE status='open' AND (user_a=ANY($1) OR user_b=ANY($1))",
                )
                .bind(vec![user, other])
                .fetch_one(&mut *tx)
                .await?;
                require(count < 8, "cap")?;
                ensure_player(&mut tx, other).await?;
                take_offer(&mut tx, user, give, "trade_escrow").await?;
                let id:i64=sqlx::query_scalar("INSERT INTO economy_trades(user_a,user_b,offer_a,offer_b,expires_at) VALUES($1,$2,$3,$4,$5) RETURNING id")
    .bind(user).bind(other).bind(serde_json::to_value(give)?).bind(serde_json::to_value(receive)?).bind(now+Duration::hours(24)).fetch_one(&mut *tx).await?;
                json!({"id":id})
            }
            Action::ConfirmTrade { id } | Action::CancelTrade { id } => {
                let t = trade(&mut tx, *id).await?;
                require(user == t.user_a || user == t.user_b, "forbidden")?;
                require(t.status == "open", "gone")?;
                let give: Offer = serde_json::from_value(t.offer_a.clone())?;
                let receive: Offer = serde_json::from_value(t.offer_b.clone())?;
                let cancel = matches!(action, Action::CancelTrade { .. });
                if cancel || now >= t.expires_at {
                    give_offer(&mut tx, t.user_a, &give, "trade_refund").await?;
                    if t.held_b {
                        give_offer(&mut tx, t.user_b, &receive, "trade_refund").await?;
                    }
                    let status = if now >= t.expires_at { "expired" } else { "cancelled" };
                    sqlx::query("UPDATE economy_trades SET status=$2 WHERE id=$1")
                        .bind(id)
                        .bind(status)
                        .execute(&mut *tx)
                        .await?;
                    json!({"id":id,"status":status})
                } else {
                    if user == t.user_b && !t.held_b {
                        take_offer(&mut tx, t.user_b, &receive, "trade_escrow").await?;
                    }
                    let a = t.confirmed_a || user == t.user_a;
                    let b = t.confirmed_b || user == t.user_b;
                    let status = if a && b { "completed" } else { "open" };
                    if a && b {
                        give_offer(&mut tx, t.user_b, &give, "trade_received").await?;
                        give_offer(&mut tx, t.user_a, &receive, "trade_received").await?;
                    }
                    sqlx::query(
                        "UPDATE economy_trades SET confirmed_a=$2,confirmed_b=$3,held_b=$3,status=$4 WHERE id=$1",
                    )
                    .bind(id)
                    .bind(a)
                    .bind(b)
                    .bind(status)
                    .execute(&mut *tx)
                    .await?;
                    json!({"id":id,"status":status})
                }
            }
            Action::Donate { guild, amount } => {
                let guild = id(guild)?;
                require((1..=rules::MAX_PRICE).contains(amount), "invalid")?;
                sqlx::query("INSERT INTO economy_settings(guild_id) VALUES($1) ON CONFLICT DO NOTHING")
                    .bind(guild)
                    .execute(&mut *tx)
                    .await?;
                money(&mut tx, user, -amount, "donation_pool").await?;
                let total:Option<i64>=sqlx::query_scalar("UPDATE economy_settings SET pool_total=pool_total+$2 WHERE guild_id=$1 AND pool_total::numeric+$2<=9223372036854775807 RETURNING pool_total").bind(guild).bind(amount).fetch_optional(&mut *tx).await?;
                require(total.is_some(), "cap")?;
                json!({"amount":-amount,"pool_total":total})
            }
        };
        sqlx::query("INSERT INTO economy_requests(user_id,request_id,result) VALUES($1,$2,$3)")
            .bind(user)
            .bind(request)
            .bind(json!({"request":fingerprint,"response":response}))
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(response)
    }
    pub async fn economy_snapshot(&self, user: u64, now: DateTime<Utc>) -> anyhow::Result<Value> {
        let p: Option<Player> = sqlx::query_as("SELECT * FROM economy_players WHERE user_id=$1")
            .bind(user as i64)
            .fetch_optional(self.pool())
            .await?;
        let trades: Vec<Trade> = sqlx::query_as(
            "SELECT * FROM economy_trades WHERE user_a=$1 OR user_b=$1 ORDER BY (status='open') DESC,id DESC LIMIT 50",
        )
        .bind(user as i64)
        .fetch_all(self.pool())
        .await?;
        let auctions:Vec<Auction>=sqlx::query_as("SELECT * FROM economy_auctions WHERE status='open' OR seller_id=$1 OR bidder_id=$1 ORDER BY (status='open') DESC,(seller_id=$1 OR bidder_id=$1) DESC NULLS LAST,id DESC LIMIT 100").bind(user as i64).fetch_all(self.pool()).await?;
        let inventory: Vec<(String, i32)> = sqlx::query_as(
            "SELECT item_id,quantity FROM inventory WHERE user_id=$1 AND quantity>0 ORDER BY item_id LIMIT 2000",
        )
        .bind(user as i64)
        .fetch_all(self.pool())
        .await?;
        let day = (now.timestamp() / 86400) as i32;
        let shifts: i32 =
            sqlx::query_scalar("SELECT COALESCE((SELECT shifts FROM economy_limits WHERE user_id=$1 AND day=$2),0)")
                .bind(user as i64)
                .bind(day)
                .fetch_one(self.pool())
                .await?;
        let last: Option<DateTime<Utc>> =
            sqlx::query_scalar("SELECT max(last_shift) FROM economy_limits WHERE user_id=$1")
                .bind(user as i64)
                .fetch_one(self.pool())
                .await?;
        let end = now
            .date_naive()
            .with_day(1)
            .unwrap()
            .checked_add_months(chrono::Months::new(1))
            .unwrap()
            .and_hms_opt(0, 0, 0)
            .unwrap()
            .and_utc();
        let claimed: i32 = sqlx::query_scalar(
            "SELECT COALESCE((SELECT season_claimed FROM economy_limits WHERE user_id=$1 AND day=$2),0)",
        )
        .bind(user as i64)
        .bind((end.timestamp() / 86400 - 1) as i32)
        .fetch_one(self.pool())
        .await?;
        // Snowflakes must cross JSON as strings (JavaScript numbers cannot represent them).
        let trades = trades
            .into_iter()
            .map(|x| {
                let mut v = serde_json::to_value(&x).unwrap();
                v["user_a"] = json!(x.user_a.to_string());
                v["user_b"] = json!(x.user_b.to_string());
                v
            })
            .collect::<Vec<_>>();
        let auctions = auctions
            .into_iter()
            .map(|x| {
                let mut v = serde_json::to_value(&x).unwrap();
                v["seller_id"] = json!(x.seller_id.to_string());
                v["bidder_id"] = json!(x.bidder_id.map(|i| i.to_string()));
                v["minimum_bid"] = json!(if x.bidder_id.is_none() {
                    x.start_price
                } else {
                    rules::minimum_bid(x.bid)
                });
                v
            })
            .collect::<Vec<_>>();
        Ok(
            json!({"player":p,"user_id":user.to_string(),"season":rules::season(now),"season_ends":end,"season_claimed":claimed,"shifts_today":shifts,"next_shift":last.map(|t|t+Duration::hours(1)),"trades":trades,"auctions":auctions,"inventory":inventory,"odds":rules::CHEST_ODDS}),
        )
    }
    pub async fn economy_settings(&self, guild: u64) -> anyhow::Result<Value> {
        let value: Option<Value> =
            sqlx::query_scalar("SELECT to_jsonb(s) - 'guild_id' FROM economy_settings s WHERE guild_id=$1")
                .bind(guild as i64)
                .fetch_optional(self.pool())
                .await?;
        Ok(value.unwrap_or_else(||json!({"pool_goal":10000,"pool_total":0,"shop_min_level":0,"shop_min_days":0,"shop_enabled":true,"revision":0})))
    }
    pub async fn save_economy_settings(
        &self,
        guild: u64,
        settings: &EconomySettings,
        expected: i64,
    ) -> anyhow::Result<bool> {
        self.save_economy_settings_audited(guild, settings, expected, None)
            .await
    }
    pub async fn save_economy_settings_audited(
        &self,
        guild: u64,
        settings: &EconomySettings,
        expected: i64,
        actor: Option<&EconomyAudit<'_>>,
    ) -> anyhow::Result<bool> {
        require(settings.valid(), "invalid")?;
        let mut tx = self.pool().begin().await?;
        if let Some(actor) = actor {
            lock_users(&mut tx, &[actor.actor as i64]).await?;
        }
        sqlx::query("INSERT INTO economy_settings(guild_id) VALUES($1) ON CONFLICT DO NOTHING")
            .bind(guild as i64)
            .execute(&mut *tx)
            .await?;
        let current: Value =
            sqlx::query_scalar("SELECT to_jsonb(s) FROM economy_settings s WHERE guild_id=$1 FOR UPDATE")
                .bind(guild as i64)
                .fetch_one(&mut *tx)
                .await?;
        require(current["revision"].as_i64() == Some(expected), "changed")?;
        let unchanged = current["pool_goal"] == settings.pool_goal
            && current["shop_min_level"] == settings.shop_min_level
            && current["shop_min_days"] == settings.shop_min_days
            && current["shop_enabled"] == settings.shop_enabled;
        if unchanged {
            tx.commit().await?;
            return Ok(false);
        }
        let n=sqlx::query("UPDATE economy_settings SET pool_goal=$2,shop_min_level=$3,shop_min_days=$4,shop_enabled=$5,revision=revision+1 WHERE guild_id=$1 AND revision=$6")
   .bind(guild as i64).bind(settings.pool_goal).bind(settings.shop_min_level).bind(settings.shop_min_days).bind(settings.shop_enabled).bind(expected).execute(&mut *tx).await?.rows_affected();
        require(n == 1, "changed")?;
        let after: Value = sqlx::query_scalar("SELECT to_jsonb(s) FROM economy_settings s WHERE guild_id=$1")
            .bind(guild as i64)
            .fetch_one(&mut *tx)
            .await?;
        admin_audit(&mut tx, guild, actor, "economy.settings", current, after).await?;
        tx.commit().await?;
        Ok(true)
    }
    /// Bounded work per tick; every settlement uses the same idempotent transaction as member actions.
    /// Privacy tombstones expire after 35 days; future month-end claim guards stay.
    pub async fn forget_economy_limits(&self, now: DateTime<Utc>) -> anyhow::Result<u64> {
        Ok(sqlx::query("WITH old AS MATERIALIZED (SELECT user_id,day FROM economy_limits WHERE day<$1 LIMIT 1000 FOR UPDATE SKIP LOCKED) DELETE FROM economy_limits d USING old WHERE d.user_id=old.user_id AND d.day=old.day")
            .bind((now.timestamp()/86400) as i32 - 35).execute(self.pool()).await?.rows_affected())
    }
    pub async fn sweep_economy(&self, now: DateTime<Utc>) -> anyhow::Result<()> {
        // Transaction-scoped leadership releases on cancellation, crash or connection loss.
        let mut leader = self.pool().begin().await?;
        let result = async {
            let acquired: bool =
                sqlx::query_scalar("SELECT pg_try_advisory_xact_lock(hashtextextended('economy:sweep',0))")
                    .fetch_one(&mut *leader)
                    .await?;
            if !acquired {
                return Ok(false);
            }
            let auctions: Vec<(i64, i64)> = sqlx::query_as(
            "SELECT id,seller_id FROM economy_auctions WHERE status='open' AND ends_at<=$1 ORDER BY ends_at LIMIT 100",
        )
        .bind(now)
        .fetch_all(self.pool())
        .await?;
            for (a, u) in auctions {
                if let Err(e) = retry("economy_settle", || async move {
                    self.economy_action_once(u, &format!("worker:settle:{a}"), &Action::SettleAuction { id: a }, now)
                        .await
                })
                .await
                    && e.downcast_ref::<Rejected>().is_none()
                {
                    return Err(e);
                }
            }
            let trades: Vec<(i64, i64)> = sqlx::query_as(
            "SELECT id,user_a FROM economy_trades WHERE status='open' AND expires_at<=$1 ORDER BY expires_at LIMIT 100",
        )
        .bind(now)
        .fetch_all(self.pool())
        .await?;
            for (t, u) in trades {
                if let Err(e) = retry("economy_expire", || async move {
                    self.economy_action_once(u, &format!("worker:expire:{t}"), &Action::CancelTrade { id: t }, now)
                        .await
                })
                .await
                    && e.downcast_ref::<Rejected>().is_none()
                {
                    return Err(e);
                }
            }
            anyhow::Ok(true)
        }
        .await;
        match result {
            Ok(true) => {
                leader.commit().await?;
                Ok(())
            }
            Ok(false) => {
                leader.rollback().await?;
                Ok(())
            }
            Err(error) => {
                if let Err(release_error) = leader.rollback().await {
                    tracing::warn!(%release_error, "market leadership rollback failed after sweep error");
                }
                Err(error)
            }
        }
    }
}

/// All counterpart locks were collected by privacy::others_written before object rows.
pub(crate) async fn privacy_refund(tx: &mut Tx<'_>, user: i64, now: DateTime<Utc>) -> anyhow::Result<()> {
    sqlx::query("INSERT INTO economy_limits(user_id,day,last_shift) SELECT $1,$2,max(last_shift) FROM economy_limits WHERE user_id=$1 ON CONFLICT(user_id,day) DO UPDATE SET last_shift=GREATEST(economy_limits.last_shift,EXCLUDED.last_shift)").bind(user).bind((now.timestamp()/86400) as i32).execute(&mut **tx).await?;
    let rows: Vec<Auction> = sqlx::query_as(
        "SELECT * FROM economy_auctions WHERE status='open' AND (seller_id=$1 OR bidder_id=$1) ORDER BY id FOR UPDATE",
    )
    .bind(user)
    .fetch_all(&mut **tx)
    .await?;
    for a in rows {
        if let Some(b) = a.bidder_id
            && b != user
        {
            money(tx, b, a.bid, "auction_refund").await?;
        }
        if a.seller_id != user {
            inventory_credit(tx, a.seller_id, &a.item_id, 1, "auction_return").await?;
        }
    }
    let rows: Vec<Trade> = sqlx::query_as(
        "SELECT * FROM economy_trades WHERE status='open' AND (user_a=$1 OR user_b=$1) ORDER BY id FOR UPDATE",
    )
    .bind(user)
    .fetch_all(&mut **tx)
    .await?;
    for t in rows {
        if t.user_a != user {
            give_offer(tx, t.user_a, &serde_json::from_value(t.offer_a)?, "trade_refund").await?;
        }
        if t.user_b != user && t.held_b {
            give_offer(tx, t.user_b, &serde_json::from_value(t.offer_b)?, "trade_refund").await?;
        }
    }
    Ok(())
}
impl Db {
    /// Preview is a revision-bound confirmation: concurrent members or point changes invalidate it.
    pub async fn economy_points_preview(&self, guild: u64) -> anyhow::Result<Value> {
        let (count,total,version):(i64,String,String)=sqlx::query_as("SELECT count(*),COALESCE(sum(points),0)::text,md5($1::bigint::text||':'||COALESCE(string_agg(user_id::text||':'||points::text,',' ORDER BY user_id),'')) FROM members WHERE guild_id=$1").bind(guild as i64).fetch_one(self.pool()).await?;
        Ok(json!({"count":count,"total":total,"version":version}))
    }
    pub async fn economy_scale_points(&self, guild: u64, percent: i32, version: &str) -> anyhow::Result<u64> {
        self.economy_scale_points_audited(guild, percent, version, None).await
    }
    pub async fn economy_scale_points_audited(
        &self,
        guild: u64,
        percent: i32,
        version: &str,
        actor: Option<&EconomyAudit<'_>>,
    ) -> anyhow::Result<u64> {
        require((0..=100).contains(&percent), "invalid")?;
        retry("economy_scale_points",||async {
   let mut tx=self.pool().begin().await?;
   let users:Vec<i64>=sqlx::query_scalar("SELECT user_id FROM members WHERE guild_id=$1 ORDER BY user_id").bind(guild as i64).fetch_all(&mut *tx).await?;
   require(users.len()<=10000,"cap")?;
   let mut locked = users.clone();
   if let Some(actor) = actor { locked.push(actor.actor as i64); }
   lock_users(&mut tx,&locked).await?;
   // A late member must not be waited on out of order.
   let late:Vec<i64>=sqlx::query_scalar("SELECT user_id FROM members WHERE guild_id=$1 AND NOT(user_id=ANY($2))").bind(guild as i64).bind(&users).fetch_all(&mut *tx).await?;
   require(late.is_empty(),"changed")?;
   let current:String=sqlx::query_scalar("SELECT md5($1::bigint::text||':'||COALESCE(string_agg(user_id::text||':'||points::text,',' ORDER BY user_id),'')) FROM members WHERE guild_id=$1").bind(guild as i64).fetch_one(&mut *tx).await?;
   require(current==version,"changed")?;
   let before: String=sqlx::query_scalar("SELECT COALESCE(sum(points),0)::text FROM members WHERE guild_id=$1 AND user_id=ANY($2)").bind(guild as i64).bind(&users).fetch_one(&mut *tx).await?;
   let n=sqlx::query("UPDATE members SET points=floor(points::numeric*$2/100)::bigint WHERE guild_id=$1 AND user_id=ANY($3)").bind(guild as i64).bind(percent).bind(&users).execute(&mut *tx).await?.rows_affected();
   let after: String=sqlx::query_scalar("SELECT COALESCE(sum(points),0)::text FROM members WHERE guild_id=$1 AND user_id=ANY($2)").bind(guild as i64).bind(&users).fetch_one(&mut *tx).await?;
   if before != after {
       admin_audit(&mut tx,guild,actor,"economy.points_scale",json!({"count":users.len(),"total":before,"version":version}),json!({"count":n,"total":after,"percent":percent})).await?;
   }
   tx.commit().await?;Ok(n)
  }).await
    }
}
pub(crate) async fn store_allowed(tx: &mut Tx<'_>, guild: i64, user: i64) -> anyhow::Result<bool> {
    let s: Option<(bool, i32, i32)> = sqlx::query_as(
        "SELECT shop_enabled,shop_min_level,shop_min_days FROM economy_settings WHERE guild_id=$1 FOR SHARE",
    )
    .bind(guild)
    .fetch_optional(&mut **tx)
    .await?;
    let Some((enabled, level, days)) = s else {
        return Ok(true);
    };
    if !enabled {
        return Ok(false);
    }
    if level == 0 && days == 0 {
        return Ok(true);
    }
    let m: Option<(i64, DateTime<Utc>)> =
        sqlx::query_as("SELECT xp,economy_joined_at FROM members WHERE guild_id=$1 AND user_id=$2")
            .bind(guild)
            .bind(user)
            .fetch_optional(&mut **tx)
            .await?;
    let config: Option<Value> = sqlx::query_scalar("SELECT config FROM guild_leveling_parity WHERE guild_id=$1")
        .bind(guild)
        .fetch_optional(&mut **tx)
        .await?;
    let config: pinchora_core::progression::leveling_policy::LevelingPolicy =
        config.map(serde_json::from_value).transpose()?.unwrap_or_default();
    Ok(m.is_some_and(|(xp, joined)| {
        config.level_of(xp) >= level as u32 && Utc::now() - joined >= Duration::days(i64::from(days))
    }))
}
