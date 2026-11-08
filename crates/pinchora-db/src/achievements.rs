//! Server achievement definitions, progress, and atomic one-time grants.

use chrono::{DateTime, Utc};
use pinchora_core::achievements::{Kind, MAX_ACHIEVEMENTS, valid_description, valid_icon, valid_name};

use crate::{
    Db,
    locks::{lock_users, retry, try_lock_users},
    sql_id,
};

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct AchievementRow {
    pub id: i64,
    pub guild_id: i64,
    pub name: String,
    pub description: String,
    pub icon: String,
    pub kind: String,
    pub target: i64,
    pub reward_role_id: Option<i64>,
    pub reward_points: i64,
    pub announce_channel_id: Option<i64>,
    pub secret: bool,
    pub enabled: bool,
    pub created_at: DateTime<Utc>,
}

pub struct AchievementDraft<'a> {
    pub name: &'a str,
    pub description: &'a str,
    pub icon: &'a str,
    pub kind: Kind,
    pub target: i64,
    pub reward_role_id: Option<u64>,
    pub reward_points: i64,
    pub announce_channel_id: Option<u64>,
    pub secret: bool,
    pub enabled: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AchievementSave {
    Saved(i64),
    Full,
    NameTaken,
    Missing,
}

const COLUMNS: &str = "id,guild_id,name,description,icon,kind,target,reward_role_id,reward_points,announce_channel_id,secret,enabled,created_at";

impl Db {
    pub async fn achievements(&self, guild: u64) -> anyhow::Result<Vec<AchievementRow>> {
        Ok(sqlx::query_as(sqlx::AssertSqlSafe(format!(
            "SELECT {COLUMNS} FROM server_achievements WHERE guild_id=$1 ORDER BY id"
        )))
        .bind(sql_id(guild))
        .fetch_all(self.pool())
        .await?)
    }

    pub async fn achievement(&self, guild: u64, id: i64) -> anyhow::Result<Option<AchievementRow>> {
        Ok(sqlx::query_as(sqlx::AssertSqlSafe(format!(
            "SELECT {COLUMNS} FROM server_achievements WHERE guild_id=$1 AND id=$2"
        )))
        .bind(sql_id(guild))
        .bind(id)
        .fetch_optional(self.pool())
        .await?)
    }

    pub async fn save_achievement(
        &self,
        guild: u64,
        id: Option<i64>,
        draft: &AchievementDraft<'_>,
    ) -> anyhow::Result<AchievementSave> {
        anyhow::ensure!(
            valid_name(draft.name) && valid_description(draft.description) && valid_icon(draft.icon),
            "invalid achievement text or icon"
        );
        anyhow::ensure!((1..=1_000_000_000).contains(&draft.target), "invalid target");
        anyhow::ensure!((0..=1_000_000_000).contains(&draft.reward_points), "invalid points");
        self.guild_settings(guild).await?;
        let mut tx = self.pool().begin().await?;
        sqlx::query("SELECT pg_advisory_xact_lock($1)")
            .bind(sql_id(guild))
            .execute(&mut *tx)
            .await?;
        if id.is_none() {
            let (count,): (i64,) = sqlx::query_as("SELECT count(*) FROM server_achievements WHERE guild_id=$1")
                .bind(sql_id(guild))
                .fetch_one(&mut *tx)
                .await?;
            if count >= MAX_ACHIEVEMENTS as i64 {
                return Ok(AchievementSave::Full);
            }
        } else if sqlx::query("SELECT id FROM server_achievements WHERE guild_id=$1 AND id=$2 FOR UPDATE")
            .bind(sql_id(guild))
            .bind(id)
            .fetch_optional(&mut *tx)
            .await?
            .is_none()
        {
            return Ok(AchievementSave::Missing);
        }
        let result = sqlx::query_scalar::<_, i64>("INSERT INTO server_achievements (id,guild_id,name,description,icon,kind,target,reward_role_id,reward_points,announce_channel_id,secret,enabled)
            VALUES (COALESCE($1,nextval('server_achievements_id_seq')),$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)
            ON CONFLICT (id) DO UPDATE SET name=$3,description=$4,icon=$5,kind=$6,target=$7,reward_role_id=$8,reward_points=$9,announce_channel_id=$10,secret=$11,enabled=$12
            WHERE server_achievements.guild_id=$2 RETURNING id")
            .bind(id).bind(sql_id(guild)).bind(draft.name.trim()).bind(draft.description.trim()).bind(draft.icon)
            .bind(draft.kind.key()).bind(draft.target).bind(draft.reward_role_id.map(sql_id)).bind(draft.reward_points)
            .bind(draft.announce_channel_id.map(sql_id)).bind(draft.secret).bind(draft.enabled)
            .fetch_optional(&mut *tx).await;
        match result {
            Ok(Some(id)) => {
                tx.commit().await?;
                Ok(AchievementSave::Saved(id))
            }
            Ok(None) => Ok(AchievementSave::Missing),
            Err(sqlx::Error::Database(ref e)) if e.constraint() == Some("server_achievements_guild_id_name_key") => {
                Ok(AchievementSave::NameTaken)
            }
            Err(e) => Err(e.into()),
        }
    }

    pub async fn remove_achievement(&self, guild: u64, id: i64) -> anyhow::Result<bool> {
        retry("remove_achievement", || async {
            let users: Vec<i64> =
                sqlx::query_scalar("SELECT user_id FROM member_achievements WHERE guild_id=$1 AND achievement_id=$2")
                    .bind(sql_id(guild))
                    .bind(id)
                    .fetch_all(self.pool())
                    .await?;
            let mut tx = self.pool().begin().await?;
            lock_users(&mut tx, &users).await?;
            let exists = sqlx::query("SELECT id FROM server_achievements WHERE guild_id=$1 AND id=$2 FOR UPDATE")
                .bind(sql_id(guild))
                .bind(id)
                .fetch_optional(&mut *tx)
                .await?
                .is_some();
            if !exists {
                return Ok(false);
            }
            let late: Vec<i64> =
                sqlx::query_scalar("SELECT user_id FROM member_achievements WHERE guild_id=$1 AND achievement_id=$2")
                    .bind(sql_id(guild))
                    .bind(id)
                    .fetch_all(&mut *tx)
                    .await?;
            try_lock_users(&mut tx, &late).await?;
            sqlx::query("DELETE FROM server_achievements WHERE guild_id=$1 AND id=$2")
                .bind(sql_id(guild))
                .bind(id)
                .execute(&mut *tx)
                .await?;
            tx.commit().await?;
            Ok(true)
        })
        .await
    }

    pub async fn earned_achievements(&self, guild: u64, user: u64) -> anyhow::Result<Vec<(i64, DateTime<Utc>)>> {
        Ok(
            sqlx::query_as("SELECT achievement_id,earned_at FROM member_achievements WHERE guild_id=$1 AND user_id=$2")
                .bind(sql_id(guild))
                .bind(sql_id(user))
                .fetch_all(self.pool())
                .await?,
        )
    }

    pub async fn achievement_holders(
        &self,
        guild: u64,
        id: i64,
        limit: i64,
    ) -> anyhow::Result<Vec<(i64, DateTime<Utc>)>> {
        Ok(sqlx::query_as("SELECT m.user_id,m.earned_at FROM member_achievements m JOIN server_achievements a ON a.id=m.achievement_id AND a.guild_id=m.guild_id WHERE m.guild_id=$1 AND m.achievement_id=$2 AND NOT EXISTS (SELECT 1 FROM user_privacy p WHERE p.user_id=m.user_id AND p.hide_profile) ORDER BY m.earned_at DESC LIMIT $3")
            .bind(sql_id(guild)).bind(id).bind(limit).fetch_all(self.pool()).await?)
    }

    /// Only callers with a configured kind ask for progress. Old stats are retained for 90 days.
    pub async fn achievement_progress(&self, guild: u64, user: u64, kind: Kind) -> anyhow::Result<i64> {
        let mut conn = self.pool().acquire().await?;
        achievement_progress_on(&mut conn, guild, user, kind).await
    }

    /// Inserts the grant and its server points in one transaction. A duplicate gives nothing.
    pub async fn grant_achievement(
        &self,
        guild: u64,
        user: u64,
        id: i64,
        expected_kind: Kind,
    ) -> anyhow::Result<Option<AchievementRow>> {
        retry("grant_achievement", || async {
            let mut tx = self.pool().begin().await?;
            lock_users(&mut tx, &[sql_id(user)]).await?;
            let rule: Option<AchievementRow> = sqlx::query_as(sqlx::AssertSqlSafe(format!("SELECT {COLUMNS} FROM server_achievements WHERE guild_id=$1 AND id=$2 AND enabled FOR UPDATE")))
                .bind(sql_id(guild)).bind(id).fetch_optional(&mut *tx).await?;
            let Some(rule) = rule else { return Ok(None) };
            if rule.kind != expected_kind.key() { return Ok(None) }
            // A cached rule may have been edited while evaluation was in flight. Check the
            // locked definition's kind and target before any reward is written.
            if expected_kind != Kind::Custom {
                let Some(kind) = Kind::parse(&rule.kind) else { return Ok(None) };
                if achievement_progress_on(&mut tx, guild, user, kind).await? < rule.target { return Ok(None) }
            }
            let fresh = sqlx::query("INSERT INTO member_achievements(guild_id,user_id,achievement_id) SELECT $1,$2,$3 WHERE NOT EXISTS (SELECT 1 FROM deleted_users WHERE user_id=$2) ON CONFLICT DO NOTHING")
                .bind(sql_id(guild)).bind(sql_id(user)).bind(id).execute(&mut *tx).await?.rows_affected() == 1;
            if !fresh { return Ok(None) }
            if rule.reward_points > 0 {
                crate::members::give_points_in(&mut tx, guild, user, rule.reward_points).await?;
            }
            tx.commit().await?;
            Ok(Some(rule))
        }).await
    }
}

async fn achievement_progress_on(
    conn: &mut sqlx::PgConnection,
    guild: u64,
    user: u64,
    kind: Kind,
) -> anyhow::Result<i64> {
    let g = sql_id(guild);
    let u = sql_id(user);
    let value = match kind {
            Kind::Messages | Kind::VoiceMinutes | Kind::DaysActive => {
                let column = match kind { Kind::Messages => "sum(messages)::BIGINT", Kind::VoiceMinutes => "sum(voice_minutes)::BIGINT", _ => "count(*) FILTER (WHERE messages > 0 OR voice_minutes > 0)" };
                sqlx::query_scalar::<_, Option<i64>>(sqlx::AssertSqlSafe(format!("SELECT {column} FROM member_activity_daily WHERE guild_id=$1 AND user_id=$2")))
                    .bind(g).bind(u).fetch_one(&mut *conn).await?.unwrap_or(0)
            }
            Kind::Level => {
                let xp: Option<i64> = sqlx::query_scalar("SELECT xp FROM members WHERE guild_id=$1 AND user_id=$2")
                    .bind(g).bind(u).fetch_optional(&mut *conn).await?;
                let config: Option<serde_json::Value> = sqlx::query_scalar("SELECT config FROM guild_leveling_parity WHERE guild_id=$1")
                    .bind(g).fetch_optional(&mut *conn).await?;
                let policy: pinchora_core::leveling_parity::LevelingParity = config.map(serde_json::from_value).transpose()?.unwrap_or_default();
                i64::from(policy.level_of(xp.unwrap_or(0)))
            }
            Kind::Invites => {
                let regular: i64 = sqlx::query_scalar("SELECT count(DISTINCT user_id) FROM invite_joins WHERE guild_id=$1 AND inviter_id=$2 AND user_id<>$2 AND left_at IS NULL AND NOT fake AND source IN ('invite','temporary') AND to_timestamp(((user_id >> 22) + 1420070400000)::DOUBLE PRECISION / 1000) <= joined_at - interval '7 days'")
                    .bind(g).bind(u).fetch_one(&mut *conn).await?;
                let bonus: Option<i32> = sqlx::query_scalar("SELECT bonus FROM invite_bonus WHERE guild_id=$1 AND user_id=$2")
                    .bind(g).bind(u).fetch_optional(&mut *conn).await?;
                (regular + i64::from(bonus.unwrap_or(0))).max(0)
            }
            Kind::StarsReceived => sqlx::query_scalar::<_, i64>("SELECT count(DISTINCT (s.message_id,s.user_id)) FROM starboard_stars s JOIN starboard_posts p USING(starboard_id,message_id) JOIN starboards b ON b.id=p.starboard_id WHERE b.guild_id=$1 AND p.author_id=$2 AND s.user_id<>$2 AND b.enabled AND NOT b.allow_bots AND p.sync_due IS NULL AND to_timestamp(((s.user_id >> 22) + 1420070400000)::DOUBLE PRECISION / 1000) <= p.created_at - interval '7 days'")
                .bind(g).bind(u).fetch_one(&mut *conn).await?,
            Kind::TicketsClosed => sqlx::query_scalar::<_, i64>("SELECT count(*) FROM tickets WHERE guild_id=$1 AND closed_by=$2 AND opener_id<>$2 AND status='closed'")
                .bind(g).bind(u).fetch_one(&mut *conn).await?,
            Kind::Reputation => i64::from(sqlx::query_scalar::<_, i32>("SELECT reputation FROM users WHERE id=$1")
                .bind(u).fetch_optional(&mut *conn).await?.unwrap_or(0)),
            Kind::GamesWon => sqlx::query_scalar::<_, Option<i64>>("SELECT sum(won)::BIGINT FROM game_stats WHERE user_id=$1")
                .bind(u).fetch_one(&mut *conn).await?.unwrap_or(0),
            Kind::FishCaught => sqlx::query_scalar::<_, Option<i64>>("SELECT sum(caught)::BIGINT FROM fish_book WHERE user_id=$1")
                .bind(u).fetch_one(&mut *conn).await?.unwrap_or(0),
            Kind::Custom => 0,
        };
    Ok(value.max(0))
}
