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

mod accounts;
mod ledger_entries;
mod model;
mod points;
mod privacy;

pub(crate) use ledger_entries::movement;
use ledger_entries::{auction, balance, give_offer, inventory_credit, money, money_record, take_offer, trade};
pub use model::{Action, Auction, EconomyAudit, Player, Rejected, Trade};
use model::{Tx, admin_audit, ensure_player, id, require};
pub(crate) use privacy::privacy_refund;

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
