use super::*;

impl Db {
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
