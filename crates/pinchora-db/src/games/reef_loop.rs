//! Atomic long-loop actions. User advisory locks precede all object and balance locks.
use crate::Db;
use crate::economy::legacy::ledger;
use crate::economy::shop::Currency;
use crate::economy::shop::charge;
use crate::house::house::add_inventory;
use crate::infra::locks::lock_users;
use crate::infra::locks::retry;
use crate::sql_id;
use chrono::{DateTime, Duration, NaiveDate, Utc};
use pinchora_core::games::items::soulbound;
use pinchora_core::games::reef_loop as rules;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ReefState {
    pub seaweed: i32,
    pub aquarium_name: String,
    pub planted_at: Option<DateTime<Utc>>,
    pub watered_at: Option<DateTime<Utc>>,
    pub pet_day: Option<i32>,
    pub pet_streak: i32,
    pub dive_nonce: i64,
    pub dive_depth: i32,
    pub dive_until: Option<DateTime<Utc>>,
}
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct AquariumFish {
    pub slot: i16,
    pub fish_id: String,
    pub size_cm: f32,
}
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ReefStanding {
    pub user_id: i64,
    pub score: f64,
    pub days: i64,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReefOutcome {
    Done,
    Already,
    Funds,
    Ingredients,
    Wait,
    Missing,
    Full,
    Lost,
    Stale,
    Pending,
}
impl ReefOutcome {
    pub fn key(self) -> &'static str {
        match self {
            Self::Done => "reef.done",
            Self::Already => "reef.already",
            Self::Funds => "reef.funds",
            Self::Ingredients => "reef.ingredients",
            Self::Wait => "reef.wait",
            Self::Missing => "reef.missing",
            Self::Full => "reef.full",
            Self::Lost => "reef.lost",
            Self::Stale => "reef.stale",
            Self::Pending => "reef.pending",
        }
    }
}
#[derive(Debug, Clone, Copy)]
pub enum ReefAction<'a> {
    Plant,
    Water,
    Harvest,
    Rename(&'a str),
    AddFish {
        bucket: i64,
        slot: i16,
    },
    RemoveFish(i16),
    Request {
        fish: &'a str,
    },
    Craft {
        recipe: &'a str,
        eligible_fish: &'a [&'a str],
    },
    DiveStart,
    DiveMove {
        nonce: i64,
        depth: i32,
        surface: bool,
        roll: u32,
    },
    SeasonClaim {
        guild: u64,
        season: NaiveDate,
    },
}
async fn ensure(tx: &mut sqlx::Transaction<'_, sqlx::Postgres>, id: i64) -> anyhow::Result<()> {
    sqlx::query("INSERT INTO users(id) VALUES($1) ON CONFLICT DO NOTHING")
        .bind(id)
        .execute(&mut **tx)
        .await?;
    sqlx::query("INSERT INTO reef_loop(user_id) VALUES($1) ON CONFLICT DO NOTHING")
        .bind(id)
        .execute(&mut **tx)
        .await?;
    Ok(())
}
async fn state(tx: &mut sqlx::Transaction<'_, sqlx::Postgres>, id: i64) -> anyhow::Result<ReefState> {
    Ok(sqlx::query_as("SELECT seaweed,aquarium_name,planted_at,watered_at,pet_day,pet_streak,dive_nonce,dive_depth,dive_until FROM reef_loop WHERE user_id=$1")
       .bind(id).fetch_one(&mut **tx).await?)
}
async fn ingredient(tx: &mut sqlx::Transaction<'_, sqlx::Postgres>, id: i64, n: i32) -> anyhow::Result<()> {
    sqlx::query("UPDATE reef_loop SET seaweed=LEAST(999,seaweed+$2) WHERE user_id=$1")
        .bind(id)
        .bind(n)
        .execute(&mut **tx)
        .await?;
    Ok(())
}
async fn daily(tx: &mut sqlx::Transaction<'_, sqlx::Postgres>, id: i64, day: i32) -> anyhow::Result<()> {
    sqlx::query("INSERT INTO reef_loop_daily(user_id,day) VALUES($1,$2) ON CONFLICT DO NOTHING")
        .bind(id)
        .bind(day)
        .execute(&mut **tx)
        .await?;
    Ok(())
}
impl Db {
    /// Read-only snapshot, also used after the house access check on the web.
    pub async fn reef_state_if_any(&self, user: u64) -> anyhow::Result<Option<ReefState>> {
        Ok(sqlx::query_as("SELECT seaweed,aquarium_name,planted_at,watered_at,pet_day,pet_streak,dive_nonce,dive_depth,dive_until FROM reef_loop WHERE user_id=$1").bind(sql_id(user)).fetch_optional(self.pool()).await?)
    }

    pub async fn reef_state(&self, user: u64) -> anyhow::Result<ReefState> {
        let mut tx = self.pool().begin().await?;
        let id = sql_id(user);
        lock_users(&mut tx, &[id]).await?;
        ensure(&mut tx, id).await?;
        let row = state(&mut tx, id).await?;
        tx.commit().await?;
        Ok(row)
    }
    pub async fn reef_aquarium(&self, user: u64) -> anyhow::Result<Vec<AquariumFish>> {
        Ok(
            sqlx::query_as("SELECT slot,fish_id,size_cm FROM reef_aquarium WHERE user_id=$1 ORDER BY slot")
                .bind(sql_id(user))
                .fetch_all(self.pool())
                .await?,
        )
    }
    pub async fn reef_action(
        &self,
        user: u64,
        now: DateTime<Utc>,
        action: ReefAction<'_>,
    ) -> anyhow::Result<ReefOutcome> {
        retry("reef_action", || self.reef_action_once(user, now, action)).await
    }
    async fn reef_action_once(
        &self,
        user: u64,
        now: DateTime<Utc>,
        action: ReefAction<'_>,
    ) -> anyhow::Result<ReefOutcome> {
        use ReefOutcome::*;
        let id = sql_id(user);
        let day = now.timestamp().div_euclid(86400) as i32;
        let mut tx = self.pool().begin().await?;
        lock_users(&mut tx, &[id]).await?;
        ensure(&mut tx, id).await?;
        let s = state(&mut tx, id).await?;
        daily(&mut tx, id, day).await?;
        let mut outcome = Done;
        match action {
            ReefAction::Plant => {
                if s.planted_at.is_some() {
                    return Ok(Already);
                }
                if s.seaweed > rules::INGREDIENT_CAP - rules::HARVEST {
                    return Ok(Full);
                }
                if charge(&mut tx, id, rules::SEED_COST, Currency::Shells).await?.is_none() {
                    return Ok(Funds);
                }
                ledger(&mut tx, id, -rules::SEED_COST, "reef_seed").await?;
                sqlx::query("UPDATE reef_loop SET planted_at=$2,watered_at=NULL WHERE user_id=$1")
                    .bind(id)
                    .bind(now)
                    .execute(&mut *tx)
                    .await?;
            }
            ReefAction::Water => {
                let Some(planted) = s.planted_at else {
                    return Ok(Missing);
                };
                if s.watered_at.is_some() {
                    return Ok(Already);
                }
                if now < planted + Duration::seconds(rules::WATER_SECS) {
                    return Ok(Wait);
                }
                sqlx::query("UPDATE reef_loop SET watered_at=$2 WHERE user_id=$1")
                    .bind(id)
                    .bind(now)
                    .execute(&mut *tx)
                    .await?;
            }
            ReefAction::Harvest => {
                let Some(planted) = s.planted_at else {
                    return Ok(Missing);
                };
                if !s.watered_at.is_some_and(|watered| watered <= now)
                    || now < planted + Duration::seconds(rules::GROW_SECS)
                {
                    return Ok(Wait);
                }
                if s.seaweed > rules::INGREDIENT_CAP - rules::HARVEST {
                    return Ok(Full);
                }
                ingredient(&mut tx, id, rules::HARVEST).await?;
                sqlx::query("UPDATE reef_loop SET planted_at=NULL,watered_at=NULL WHERE user_id=$1")
                    .bind(id)
                    .execute(&mut *tx)
                    .await?;
            }
            ReefAction::Rename(name) => {
                let name = name.trim();
                if name.is_empty() || name.chars().count() > 24 || name.chars().any(char::is_control) {
                    return Ok(Missing);
                }
                sqlx::query("UPDATE reef_loop SET aquarium_name=$2 WHERE user_id=$1")
                    .bind(id)
                    .bind(name)
                    .execute(&mut *tx)
                    .await?;
            }
            ReefAction::AddFish { bucket, slot } => {
                if !(1..=6).contains(&slot) {
                    return Ok(Missing);
                }
                let exists: bool =
                    sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM reef_aquarium WHERE user_id=$1 AND slot=$2)")
                        .bind(id)
                        .bind(slot)
                        .fetch_one(&mut *tx)
                        .await?;
                if exists {
                    return Ok(Full);
                }
                let fish: Option<(String, f32)> =
                    sqlx::query_as("DELETE FROM fish_bucket WHERE user_id=$1 AND id=$2 RETURNING fish_id,size_cm")
                        .bind(id)
                        .bind(bucket)
                        .fetch_optional(&mut *tx)
                        .await?;
                let Some((fish, size)) = fish else { return Ok(Missing) };
                sqlx::query("INSERT INTO reef_aquarium(user_id,slot,fish_id,size_cm) VALUES($1,$2,$3,$4)")
                    .bind(id)
                    .bind(slot)
                    .bind(fish)
                    .bind(size)
                    .execute(&mut *tx)
                    .await?;
            }
            ReefAction::RemoveFish(slot) => {
                let count: i64 = sqlx::query_scalar("SELECT count(*) FROM fish_bucket WHERE user_id=$1")
                    .bind(id)
                    .fetch_one(&mut *tx)
                    .await?;
                if count >= 60 {
                    return Ok(Full);
                }
                let fish: Option<(String, f32)> =
                    sqlx::query_as("DELETE FROM reef_aquarium WHERE user_id=$1 AND slot=$2 RETURNING fish_id,size_cm")
                        .bind(id)
                        .bind(slot)
                        .fetch_optional(&mut *tx)
                        .await?;
                let Some((fish, size)) = fish else { return Ok(Missing) };
                sqlx::query("INSERT INTO fish_bucket(user_id,fish_id,size_cm) VALUES($1,$2,$3)")
                    .bind(id)
                    .bind(fish)
                    .bind(size)
                    .execute(&mut *tx)
                    .await?;
            }
            ReefAction::Request { fish } => {
                let rotation = now.timestamp().div_euclid(rules::REQUEST_SECS);
                if s.seaweed > 997 {
                    return Ok(Full);
                }
                if sqlx::query("INSERT INTO reef_requests(user_id,rotation) VALUES($1,$2) ON CONFLICT DO NOTHING")
                    .bind(id)
                    .bind(rotation)
                    .execute(&mut *tx)
                    .await?
                    .rows_affected()
                    == 0
                {
                    return Ok(Already);
                }
                if sqlx::query(
                    "UPDATE reef_loop_daily SET requests=requests+1 WHERE user_id=$1 AND day=$2 AND requests<3",
                )
                .bind(id)
                .bind(day)
                .execute(&mut *tx)
                .await?
                .rows_affected()
                    == 0
                {
                    return Ok(Already);
                }
                let removed=sqlx::query("DELETE FROM fish_bucket WHERE id IN (SELECT id FROM fish_bucket WHERE user_id=$1 AND fish_id=$2 ORDER BY size_cm,id LIMIT 2)").bind(id).bind(fish).execute(&mut *tx).await?.rows_affected();
                if removed != 2 {
                    return Ok(Ingredients);
                }
                ingredient(&mut tx, id, 2).await?;
            }
            ReefAction::Craft { recipe, eligible_fish } => {
                let Some(r) = rules::recipe(recipe) else {
                    return Ok(Missing);
                };
                if s.seaweed < r.seaweed {
                    return Ok(Ingredients);
                }
                if charge(&mut tx, id, r.shells, Currency::Shells).await?.is_none()
                    || charge(&mut tx, id, r.pearls, Currency::Pearls).await?.is_none()
                {
                    return Ok(Funds);
                }
                let removed=sqlx::query("DELETE FROM fish_bucket WHERE id IN (SELECT id FROM fish_bucket WHERE user_id=$1 AND fish_id=ANY($2) ORDER BY size_cm,id LIMIT $3)").bind(id).bind(eligible_fish).bind(r.fish).execute(&mut *tx).await?.rows_affected();
                if removed != r.fish as u64 {
                    return Ok(Ingredients);
                }
                sqlx::query("UPDATE reef_loop SET seaweed=seaweed-$2 WHERE user_id=$1")
                    .bind(id)
                    .bind(r.seaweed)
                    .execute(&mut *tx)
                    .await?;
                ledger(&mut tx, id, -r.shells, "reef_craft").await?;
                add_inventory(&mut tx, id, &soulbound(r.item), 1).await?;
            }
            ReefAction::DiveStart => {
                if s.dive_until.is_some_and(|until| until > now) {
                    return Ok(Already);
                }
                if sqlx::query("UPDATE reef_loop_daily SET dives=1 WHERE user_id=$1 AND day=$2 AND dives=0")
                    .bind(id)
                    .bind(day)
                    .execute(&mut *tx)
                    .await?
                    .rows_affected()
                    == 0
                {
                    return Ok(Already);
                }
                if charge(&mut tx, id, rules::DIVE_COST, Currency::Shells).await?.is_none() {
                    return Ok(Funds);
                }
                ledger(&mut tx, id, -rules::DIVE_COST, "reef_dive").await?;
                sqlx::query("UPDATE reef_loop SET dive_nonce=$3,dive_depth=0,dive_until=$2 WHERE user_id=$1")
                    .bind(id)
                    .bind(now + Duration::seconds(rules::DIVE_SECS))
                    .bind(now.timestamp_micros())
                    .execute(&mut *tx)
                    .await?;
            }
            ReefAction::DiveMove {
                nonce,
                depth,
                surface,
                roll,
            } => {
                if s.dive_nonce != nonce || s.dive_depth != depth || now.timestamp_micros() < nonce {
                    return Ok(Stale);
                }
                let Some(until) = s.dive_until else { return Ok(Stale) };
                if until <= now {
                    // Persist an observed timeout so an older queued click cannot revive it.
                    sqlx::query("UPDATE reef_loop SET dive_until=NULL,dive_depth=0 WHERE user_id=$1")
                        .bind(id)
                        .execute(&mut *tx)
                        .await?;
                    tx.commit().await?;
                    return Ok(Stale);
                }
                if surface {
                    if sqlx::query("UPDATE users SET pearls=pearls+$2 WHERE id=$1 AND pearls <= $3")
                        .bind(id)
                        .bind(i64::from(depth))
                        .bind(i64::MAX - i64::from(depth))
                        .execute(&mut *tx)
                        .await?
                        .rows_affected()
                        == 0
                    {
                        return Ok(Full);
                    }
                    sqlx::query("UPDATE reef_loop SET dive_until=NULL WHERE user_id=$1")
                        .bind(id)
                        .execute(&mut *tx)
                        .await?;
                } else {
                    if depth >= 3 || roll >= 100 {
                        return Ok(Stale);
                    }
                    if roll < rules::dive_risk(depth) {
                        sqlx::query("UPDATE reef_loop SET dive_until=NULL,dive_depth=0 WHERE user_id=$1")
                            .bind(id)
                            .execute(&mut *tx)
                            .await?;
                        outcome = Lost;
                    } else {
                        sqlx::query("UPDATE reef_loop SET dive_depth=dive_depth+1 WHERE user_id=$1")
                            .bind(id)
                            .execute(&mut *tx)
                            .await?;
                    }
                }
            }
            ReefAction::SeasonClaim { guild, season } => {
                if rules::season_start(season) != season || rules::next_season(season) > now.date_naive() {
                    return Ok(Wait);
                }
                let days: i64 = sqlx::query_scalar(
                    "SELECT count(*) FROM reef_catches WHERE user_id=$1 AND guild_id=$2 AND day >= $3 AND day < $4",
                )
                .bind(id)
                .bind(sql_id(guild))
                .bind(season)
                .bind(rules::next_season(season))
                .fetch_one(&mut *tx)
                .await?;
                if days < 3 {
                    return Ok(Ingredients);
                }
                if sqlx::query("INSERT INTO reef_season_claims(user_id,season) VALUES($1,$2) ON CONFLICT DO NOTHING")
                    .bind(id)
                    .bind(season)
                    .execute(&mut *tx)
                    .await?
                    .rows_affected()
                    == 0
                {
                    return Ok(Already);
                }
                add_inventory(&mut tx, id, &soulbound("reefbubble_lamp__sky"), 1).await?;
            }
        }
        tx.commit().await?;
        Ok(outcome)
    }
    /// Daily best size/typical-size; seasons sum daily bests. Privacy-hidden players never appear.
    pub async fn reef_board(&self, guild: u64, start: NaiveDate, end: NaiveDate) -> anyhow::Result<Vec<ReefStanding>> {
        Ok(sqlx::query_as("SELECT c.user_id,sum(c.score)::DOUBLE PRECISION AS score,count(*) AS days FROM reef_catches c LEFT JOIN user_privacy p ON p.user_id=c.user_id WHERE guild_id=$1 AND day >= $2 AND day < $3 AND NOT COALESCE(p.hide_leaderboard,FALSE) AND NOT COALESCE(p.hide_profile,FALSE) GROUP BY c.user_id ORDER BY score DESC,min(c.caught_at),c.user_id LIMIT 10")
            .bind(sql_id(guild)).bind(start).bind(end).fetch_all(self.pool()).await?)
    }
    /// Both partners must check in on the same UTC day. Locks cover both, then recheck the relationship.
    pub async fn reef_partner_checkin(&self, user: u64, now: DateTime<Utc>) -> anyhow::Result<(ReefOutcome, i32)> {
        retry("reef_partner", || self.reef_partner_once(user, now)).await
    }
    async fn reef_partner_once(&self, user: u64, now: DateTime<Utc>) -> anyhow::Result<(ReefOutcome, i32)> {
        let Some(p) = self.partner(user).await? else {
            return Ok((ReefOutcome::Missing, 0));
        };
        let a = sql_id(user.min(p.partner_id));
        let b = sql_id(user.max(p.partner_id));
        let mut tx = self.pool().begin().await?;
        lock_users(&mut tx, &[a, b]).await?;
        let valid: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM partners WHERE user_a=$1 AND user_b=$2 AND since=$3 FOR UPDATE)",
        )
        .bind(a)
        .bind(b)
        .bind(p.since)
        .fetch_one(&mut *tx)
        .await?;
        if !valid {
            return Ok((ReefOutcome::Stale, 0));
        }
        ensure(&mut tx, a).await?;
        ensure(&mut tx, b).await?;
        sqlx::query("INSERT INTO reef_partner_streaks(user_a,user_b,since) VALUES($1,$2,$3) ON CONFLICT(user_a) DO UPDATE SET user_b=EXCLUDED.user_b,since=EXCLUDED.since,a_day=NULL,b_day=NULL,completed_day=NULL,streak=0 WHERE reef_partner_streaks.since<>EXCLUDED.since OR reef_partner_streaks.user_b<>EXCLUDED.user_b").bind(a).bind(b).bind(p.since).execute(&mut *tx).await?;
        let day = now.timestamp().div_euclid(86400) as i32;
        let (a_day, b_day, completed, value): (Option<i32>, Option<i32>, Option<i32>, i32) =
            sqlx::query_as("SELECT a_day,b_day,completed_day,streak FROM reef_partner_streaks WHERE user_a=$1")
                .bind(a)
                .fetch_one(&mut *tx)
                .await?;
        // Requests can acquire the pair's locks out of arrival order around UTC midnight.
        // Never erase a newer check-in or reopen an already completed day.
        if completed == Some(day) || [a_day, b_day, completed].into_iter().flatten().any(|d| d > day) {
            return Ok((ReefOutcome::Already, value));
        }
        let (ad,bd,last,value):(Option<i32>,Option<i32>,Option<i32>,i32)=sqlx::query_as("UPDATE reef_partner_streaks SET a_day=CASE WHEN user_a=$2 THEN $3 ELSE a_day END,b_day=CASE WHEN user_b=$2 THEN $3 ELSE b_day END WHERE user_a=$1 RETURNING a_day,b_day,completed_day,streak").bind(a).bind(sql_id(user)).bind(day).fetch_one(&mut *tx).await?;
        let mut streak = value;
        let outcome = if last == Some(day) {
            ReefOutcome::Already
        } else if ad == Some(day) && bd == Some(day) {
            streak = rules::streak(last, day, value);
            sqlx::query("UPDATE reef_partner_streaks SET completed_day=$2,streak=$3 WHERE user_a=$1")
                .bind(a)
                .bind(day)
                .bind(streak)
                .execute(&mut *tx)
                .await?;
            for id in [a, b] {
                daily(&mut tx, id, day).await?;
                if sqlx::query(
                    "UPDATE reef_loop_daily SET partner_reward=TRUE WHERE user_id=$1 AND day=$2 AND NOT partner_reward",
                )
                .bind(id)
                .bind(day)
                .execute(&mut *tx)
                .await?
                .rows_affected()
                    == 1
                {
                    ingredient(&mut tx, id, 1).await?;
                }
            }
            ReefOutcome::Done
        } else {
            ReefOutcome::Pending
        };
        tx.commit().await?;
        Ok((outcome, streak))
    }
}
/// Called inside the successful pet-care transaction, never for merely opening a status card.
pub(crate) async fn reef_pet_care(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    id: i64,
    now: DateTime<Utc>,
) -> anyhow::Result<()> {
    ensure(tx, id).await?;
    let day = now.timestamp().div_euclid(86400) as i32;
    let s = state(tx, id).await?;
    if s.pet_day.is_some_and(|last| last >= day) {
        return Ok(());
    }
    let streak = rules::streak(s.pet_day, day, s.pet_streak);
    sqlx::query("UPDATE reef_loop SET pet_day=$2,pet_streak=$3 WHERE user_id=$1")
        .bind(id)
        .bind(day)
        .bind(streak)
        .execute(&mut **tx)
        .await?;
    daily(tx, id, day).await?;
    if sqlx::query("UPDATE reef_loop_daily SET pet_reward=TRUE WHERE user_id=$1 AND day=$2 AND NOT pet_reward")
        .bind(id)
        .bind(day)
        .execute(&mut **tx)
        .await?
        .rows_affected()
        == 1
    {
        ingredient(tx, id, 1).await?;
    }
    Ok(())
}

impl Db {
    /// Hourly janitor: retain a year of results, 35 days of tombstones, and recent requests.
    pub async fn prune_reef_loop(&self, now: DateTime<Utc>) -> anyhow::Result<u64> {
        let day = now.timestamp().div_euclid(86400) as i32;
        let mut n = 0;
        n+=sqlx::query("WITH old AS MATERIALIZED (SELECT user_id,day FROM reef_loop_daily WHERE day<$1 FOR UPDATE SKIP LOCKED) DELETE FROM reef_loop_daily d USING old WHERE d.user_id=old.user_id AND d.day=old.day").bind(day-35).execute(self.pool()).await?.rows_affected();
        n+=sqlx::query("WITH old AS MATERIALIZED (SELECT user_id,rotation FROM reef_requests WHERE rotation<$1 FOR UPDATE SKIP LOCKED) DELETE FROM reef_requests d USING old WHERE d.user_id=old.user_id AND d.rotation=old.rotation").bind(now.timestamp().div_euclid(rules::REQUEST_SECS)-6).execute(self.pool()).await?.rows_affected();
        n+=sqlx::query("WITH old AS MATERIALIZED (SELECT guild_id,user_id,day FROM reef_catches WHERE day<$1 FOR UPDATE SKIP LOCKED) DELETE FROM reef_catches d USING old WHERE d.guild_id=old.guild_id AND d.user_id=old.user_id AND d.day=old.day").bind(now.date_naive()-Duration::days(366)).execute(self.pool()).await?.rows_affected();
        Ok(n)
    }
}
