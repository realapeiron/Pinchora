//! Birthday dates, opt-in visibility, guild setup and one claim per guild/member/year.

use chrono::{DateTime, Datelike, Duration, Utc};
use pinchora_core::birthdays::BirthdaySettings;
use sqlx::FromRow;

use crate::{Db, locks, sql_id};

#[derive(Clone, Debug, FromRow)]
pub struct BirthdayRow {
    pub user_id: i64,
    pub month: i16,
    pub day: i16,
    pub year: Option<i16>,
    pub tz: String,
    pub public: bool,
    pub show_age: bool,
}

#[derive(Clone, Debug, FromRow)]
pub struct BirthdayCandidate {
    pub guild_id: i64,
    pub user_id: i64,
    pub month: i16,
    pub day: i16,
    pub year: Option<i16>,
    pub tz: String,
    pub show_age: bool,
    pub channel_id: Option<i64>,
    pub role_id: Option<i64>,
    pub message: String,
    pub shells_gift: i16,
    pub allow_age: bool,
    pub badge: bool,
}

#[derive(Clone, Debug, FromRow)]
pub struct BirthdayDelivery {
    #[sqlx(flatten)]
    pub birthday: BirthdayCandidate,
    pub delivery_id: i64,
    pub batch_id: Option<i64>,
    pub role_until: Option<DateTime<Utc>>,
    pub role_done: bool,
}

/// Holds player locks until the Discord attempt finishes, serializing it with opt-out and deletion.
pub struct BirthdayBatchGuard {
    _tx: sqlx::Transaction<'static, sqlx::Postgres>,
    pub ids: Vec<i64>,
}

pub struct BirthdayInput<'a> {
    pub month: i16,
    pub day: i16,
    pub year: Option<i16>,
    pub tz: &'a str,
    pub public: bool,
    pub show_age: bool,
}

#[derive(FromRow)]
struct SettingsRow {
    enabled: bool,
    channel_id: Option<i64>,
    role_id: Option<i64>,
    message: String,
    announce_hour: i16,
    server_tz: String,
    shells_gift: i16,
    allow_age: bool,
    badge: bool,
}

impl Db {
    pub async fn birthday(&self, user: u64) -> anyhow::Result<Option<BirthdayRow>> {
        Ok(
            sqlx::query_as("SELECT user_id, month, day, year, tz, public, show_age FROM birthdays WHERE user_id = $1")
                .bind(sql_id(user))
                .fetch_optional(self.pool())
                .await?,
        )
    }

    /// Profile decoration in one read; the guild controls the marker and private dates stay private.
    pub async fn birthday_profile(&self, guild: u64, user: u64) -> anyhow::Result<Option<(i16, i16, String, bool)>> {
        Ok(sqlx::query_as(
            "SELECT b.month,b.day,b.tz,b.public FROM birthdays b JOIN birthday_settings s ON s.guild_id=$1 AND s.badge WHERE b.user_id=$2 AND b.public AND NOT EXISTS(SELECT 1 FROM user_privacy p WHERE p.user_id=b.user_id AND p.hide_profile)",
        )
        .bind(sql_id(guild))
        .bind(sql_id(user))
        .fetch_optional(self.pool())
        .await?)
    }

    /// Returns false while the 30-day date/zone lock is active. Privacy choices remain editable.
    pub async fn set_birthday(&self, user: u64, b: &BirthdayInput<'_>) -> anyhow::Result<bool> {
        anyhow::ensure!(
            pinchora_core::birthdays::valid_date(b.month as u32, b.day as u32, b.year.map(i32::from))
                && b.year.is_none_or(|y| (1900..=9999).contains(&y))
                && pinchora_core::birthdays::timezone(b.tz).is_some(),
            "invalid birthday"
        );
        locks::retry("set birthday", || async {
            let mut tx = self.pool().begin().await?;
            locks::lock_users(&mut tx, &[sql_id(user)]).await?;
            let changed: bool = sqlx::query_scalar("SELECT NOT EXISTS(SELECT 1 FROM birthdays WHERE user_id=$1 AND month=$2 AND day=$3 AND year IS NOT DISTINCT FROM $4 AND tz=$5)")
                .bind(sql_id(user)).bind(b.month).bind(b.day).bind(b.year).bind(b.tz).fetch_one(&mut *tx).await?;
            if changed {
                let locked: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM birthday_gift_years WHERE user_id=$1 AND year=0 AND locked_until>now()) OR EXISTS(SELECT 1 FROM deleted_users WHERE user_id=$1 AND birthday_until>now())")
                    .bind(sql_id(user)).fetch_one(&mut *tx).await?;
                if locked { return Ok(false); }
                sqlx::query("INSERT INTO birthday_gift_years (user_id,year,locked_until) VALUES ($1,0,now()+interval '30 days') ON CONFLICT (user_id,year) DO UPDATE SET locked_until=EXCLUDED.locked_until")
                    .bind(sql_id(user)).execute(&mut *tx).await?;
            }
            sqlx::query("INSERT INTO birthdays (user_id, month, day, year, tz, public, show_age) VALUES ($1,$2,$3,$4,$5,$6,$7) ON CONFLICT (user_id) DO UPDATE SET month=$2, day=$3, year=$4, tz=$5, public=$6, show_age=$7, updated_at=now()")
                .bind(sql_id(user)).bind(b.month).bind(b.day).bind(b.year).bind(b.tz).bind(b.public).bind(b.show_age).execute(&mut *tx).await?;
            tx.commit().await?;
            Ok(true)
        }).await
    }

    pub async fn remove_birthday(&self, user: u64) -> anyhow::Result<bool> {
        locks::retry("remove birthday", || async {
            let mut tx = self.pool().begin().await?;
            locks::lock_users(&mut tx, &[sql_id(user)]).await?;
            let removed = sqlx::query("DELETE FROM birthdays WHERE user_id=$1")
                .bind(sql_id(user))
                .execute(&mut *tx)
                .await?
                .rows_affected()
                > 0;
            tx.commit().await?;
            Ok(removed)
        })
        .await
    }

    pub async fn birthday_settings(&self, guild: u64) -> anyhow::Result<BirthdaySettings> {
        self.guild_settings(guild).await?;
        let row: Option<SettingsRow> = sqlx::query_as("SELECT enabled, channel_id, role_id, message, announce_hour, server_tz, shells_gift, allow_age, badge FROM birthday_settings WHERE guild_id=$1")
            .bind(sql_id(guild)).fetch_optional(self.pool()).await?;
        Ok(row.map_or_else(BirthdaySettings::default, |r| BirthdaySettings {
            enabled: r.enabled,
            channel_id: r.channel_id.map(|n| n as u64),
            role_id: r.role_id.map(|n| n as u64),
            message: r.message,
            announce_hour: r.announce_hour,
            server_tz: r.server_tz,
            shells_gift: r.shells_gift,
            allow_age: r.allow_age,
            badge: r.badge,
        }))
    }

    pub async fn save_birthday_settings(&self, guild: u64, s: &BirthdaySettings) -> anyhow::Result<()> {
        pinchora_core::birthdays::check_settings(s).map_err(anyhow::Error::msg)?;
        self.guild_settings(guild).await?;
        sqlx::query("INSERT INTO birthday_settings (guild_id,enabled,channel_id,role_id,message,announce_hour,server_tz,shells_gift,allow_age,badge) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10) ON CONFLICT (guild_id) DO UPDATE SET enabled=$2,channel_id=$3,role_id=$4,message=$5,announce_hour=$6,server_tz=$7,shells_gift=$8,allow_age=$9,badge=$10")
            .bind(sql_id(guild)).bind(s.enabled).bind(s.channel_id.map(sql_id)).bind(s.role_id.map(sql_id)).bind(&s.message).bind(s.announce_hour).bind(&s.server_tz).bind(s.shells_gift).bind(s.allow_age).bind(s.badge).execute(self.pool()).await?;
        Ok(())
    }

    /// Public upcoming birthdays in a guild. A member who has never earned XP has no members row and is absent.
    pub async fn public_birthdays(&self, guild: u64) -> anyhow::Result<Vec<BirthdayRow>> {
        Ok(sqlx::query_as("SELECT b.user_id,b.month,b.day,b.year,b.tz,b.public,b.show_age FROM birthdays b JOIN members m ON m.user_id=b.user_id WHERE m.guild_id=$1 AND b.public AND NOT EXISTS(SELECT 1 FROM user_privacy p WHERE p.user_id=b.user_id AND p.hide_profile) ORDER BY b.month,b.day,b.user_id")
            .bind(sql_id(guild)).fetch_all(self.pool()).await?)
    }

    /// Dates within a day of UTC today cover every IANA local date, including the leap-day fallback.
    pub async fn birthday_candidates(&self, now: DateTime<Utc>) -> anyhow::Result<Vec<BirthdayCandidate>> {
        let today = now.date_naive();
        let mut dates = Vec::new();
        for shift in -1..=1 {
            let day = today + Duration::days(shift);
            dates.push((day.month() * 100 + day.day()) as i16);
            if day.month() == 2 && day.day() == 28 {
                dates.push(229);
            }
        }
        Ok(sqlx::query_as("SELECT s.guild_id,b.user_id,b.month,b.day,b.year,b.tz,b.show_age,s.channel_id,s.role_id,s.message,s.shells_gift,s.allow_age,s.badge FROM birthdays b JOIN members m ON m.user_id=b.user_id JOIN birthday_settings s ON s.guild_id=m.guild_id AND s.enabled AND s.channel_id IS NOT NULL WHERE b.public AND NOT EXISTS(SELECT 1 FROM user_privacy p WHERE p.user_id=b.user_id AND p.hide_profile) AND (b.month*100+b.day)=ANY($1::smallint[]) AND NOT EXISTS(SELECT 1 FROM birthday_sent d WHERE d.guild_id=s.guild_id AND d.user_id=b.user_id AND d.utc_year=EXTRACT(YEAR FROM now() AT TIME ZONE 'UTC')) ORDER BY b.user_id,s.guild_id")
            .bind(&dates).fetch_all(self.pool()).await?)
    }

    /// Birthday-owned timers only: never remove a pre-existing or subsequently renewed role.
    pub async fn birthday_cleanup_roles(&self, user: u64) -> anyhow::Result<Vec<(i64, i64)>> {
        Ok(sqlx::query_as("SELECT DISTINCT d.guild_id,d.role_id FROM birthday_sent d JOIN timed_roles r ON r.guild_id=d.guild_id AND r.user_id=d.user_id AND r.role_id=d.role_id AND r.remove_at=d.role_until WHERE d.user_id=$1")
            .bind(sql_id(user)).fetch_all(self.pool()).await?)
    }

    /// One dispatcher at a time across pods; the caller must await commit or rollback before returning.
    pub async fn birthday_dispatch_lock(&self) -> anyhow::Result<Option<sqlx::Transaction<'static, sqlx::Postgres>>> {
        let mut tx = self.pool().begin().await?;
        let taken =
            sqlx::query_scalar::<_, bool>("SELECT pg_try_advisory_xact_lock(hashtextextended('birthday-dispatch',0))")
                .fetch_one(&mut *tx)
                .await;
        match taken {
            Ok(true) => Ok(Some(tx)),
            result => {
                let released = tx.rollback().await;
                result?;
                released?;
                Ok(None)
            }
        }
    }

    /// Undelivered claims survive process restarts. Re-read visibility and settings before delivery.
    pub async fn birthday_deliveries(&self) -> anyhow::Result<Vec<BirthdayDelivery>> {
        Ok(sqlx::query_as("SELECT s.guild_id,b.user_id,b.month,b.day,b.year,b.tz,b.show_age,s.channel_id,d.role_id,s.message,s.shells_gift,s.allow_age,s.badge,d.delivery_id,d.batch_id,d.role_until,d.role_done FROM birthday_sent d JOIN birthdays b ON b.user_id=d.user_id JOIN birthday_settings s ON s.guild_id=d.guild_id AND s.enabled WHERE d.announced_at IS NULL AND d.sent_at>now()-interval '24 hours' AND b.public AND NOT EXISTS(SELECT 1 FROM user_privacy p WHERE p.user_id=b.user_id AND p.hide_profile) ORDER BY s.guild_id,d.batch_id NULLS LAST,d.delivery_id")
            .fetch_all(self.pool()).await?)
    }

    pub async fn lock_birthday_batch(&self, ids: &[i64]) -> anyhow::Result<BirthdayBatchGuard> {
        let users: Vec<i64> = sqlx::query_scalar("SELECT user_id FROM birthday_sent WHERE delivery_id=ANY($1)")
            .bind(ids)
            .fetch_all(self.pool())
            .await?;
        let mut tx = self.pool().begin().await?;
        locks::lock_users(&mut tx, &users).await?;
        // Privacy toggles are single-statement upserts; lock their row as well as the player.
        sqlx::query("INSERT INTO user_privacy (user_id) SELECT DISTINCT user_id FROM birthday_sent WHERE delivery_id=ANY($1) ON CONFLICT DO NOTHING")
            .bind(ids).execute(&mut *tx).await?;
        sqlx::query("SELECT user_id FROM user_privacy WHERE user_id=ANY($1) ORDER BY user_id FOR UPDATE")
            .bind(&users)
            .execute(&mut *tx)
            .await?;
        let ids = sqlx::query_scalar("SELECT d.delivery_id FROM birthday_sent d JOIN birthdays b ON b.user_id=d.user_id JOIN birthday_settings s ON s.guild_id=d.guild_id AND s.enabled WHERE d.delivery_id=ANY($1) AND d.announced_at IS NULL AND b.public AND NOT EXISTS(SELECT 1 FROM user_privacy p WHERE p.user_id=b.user_id AND p.hide_profile)")
            .bind(ids).fetch_all(&mut *tx).await?;
        Ok(BirthdayBatchGuard { _tx: tx, ids })
    }

    pub async fn birthday_batch(&self, ids: &[i64], batch: i64) -> anyhow::Result<()> {
        sqlx::query("UPDATE birthday_sent SET batch_id=$2 WHERE delivery_id=ANY($1) AND batch_id IS NULL")
            .bind(ids)
            .bind(batch)
            .execute(self.pool())
            .await?;
        Ok(())
    }

    pub async fn finish_birthday_batch(&self, ids: &[i64]) -> anyhow::Result<()> {
        sqlx::query("UPDATE birthday_sent SET announced_at=now() WHERE delivery_id=ANY($1)")
            .bind(ids)
            .execute(self.pool())
            .await?;
        Ok(())
    }

    pub async fn finish_birthday_role(&self, id: i64) -> anyhow::Result<()> {
        sqlx::query("UPDATE birthday_sent SET role_done=true WHERE delivery_id=$1")
            .bind(id)
            .execute(self.pool())
            .await?;
        Ok(())
    }

    /// Claim a delivery before an external Discord request; `SKIP LOCKED` and the primary key
    /// ensure parallel pods cannot claim the same guild/member/year. A failed send is recorded for audit.
    /// The gift is system paid once globally per year under the player's lock.
    pub async fn claim_birthday(
        &self,
        guild: u64,
        user: u64,
        year: i16,
        gift: i16,
        badge: bool,
        role_until: Option<(u64, DateTime<Utc>)>,
    ) -> anyhow::Result<Option<i16>> {
        anyhow::ensure!(
            (0..=pinchora_core::birthdays::MAX_GIFT).contains(&gift),
            "birthday gift out of range"
        );
        locks::retry("claim birthday", || async {
            let mut tx = self.pool().begin().await?;
            locks::lock_users(&mut tx, &[sql_id(user)]).await?;
            let available: Option<(i64,)> = sqlx::query_as("SELECT user_id FROM birthdays WHERE user_id=$1 AND public AND NOT EXISTS(SELECT 1 FROM deleted_users WHERE user_id=$1 AND birthday_until>now()) AND NOT EXISTS(SELECT 1 FROM user_privacy p WHERE p.user_id=$1 AND p.hide_profile) FOR UPDATE SKIP LOCKED")
                .bind(sql_id(user)).fetch_optional(&mut *tx).await?;
            if available.is_none() { return Ok(None); }
            let inserted = sqlx::query("INSERT INTO birthday_sent (guild_id,user_id,year,role_until,role_id) VALUES ($1,$2,$3,$4,$5) ON CONFLICT DO NOTHING")
                .bind(sql_id(guild)).bind(sql_id(user)).bind(year).bind(role_until.map(|(_, until)| until)).bind(role_until.map(|(role, _)| sql_id(role))).execute(&mut *tx).await?.rows_affected() > 0;
            if !inserted { return Ok(None); }
            // Persist removal in the same transaction as the claim, before any Discord grant.
            sqlx::query("INSERT INTO timed_roles (guild_id,user_id,role_id,remove_at) SELECT guild_id,user_id,role_id,role_until FROM birthday_sent WHERE guild_id=$1 AND user_id=$2 AND year=$3 AND role_id IS NOT NULL AND role_until IS NOT NULL ON CONFLICT (guild_id,user_id,role_id) DO UPDATE SET remove_at=EXCLUDED.remove_at,lease_until=NULL")
                .bind(sql_id(guild)).bind(sql_id(user)).bind(year).execute(&mut *tx).await?;
            if badge {
                sqlx::query("INSERT INTO user_badges (user_id,badge_id) VALUES ($1,'cake_day') ON CONFLICT DO NOTHING")
                    .bind(sql_id(user)).execute(&mut *tx).await?;
            }
            let mut paid = 0;
            if gift > 0 {
                let first = sqlx::query("INSERT INTO birthday_gift_years (user_id,year) VALUES ($1,EXTRACT(YEAR FROM now() AT TIME ZONE 'UTC')) ON CONFLICT DO NOTHING")
                    .bind(sql_id(user)).execute(&mut *tx).await?.rows_affected() > 0;
                if first {
                    sqlx::query("INSERT INTO users (id,credits) VALUES ($1,$2) ON CONFLICT (id) DO UPDATE SET credits=users.credits+$2")
                        .bind(sql_id(user)).bind(i64::from(gift)).execute(&mut *tx).await?;
                    crate::economy::ledger(&mut tx, sql_id(user), i64::from(gift), "birthday_gift").await?;
                    paid = gift;
                }
            }
            tx.commit().await?;
            Ok(Some(paid))
        }).await
    }
}
