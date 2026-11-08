use chrono::NaiveDate;
use pinchora_core::leveling_parity::LevelingParity;

use crate::{Db, LeaderboardRow, locks::lock_users, sql_id};

#[derive(Debug, Clone, Copy, sqlx::FromRow)]
pub struct VoiceLevelRole {
    pub role_id: i64,
    pub level: i32,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct PublicLevelRow {
    pub user_id: i64,
    pub value: i64,
    pub name: Option<String>,
}

impl Db {
    /// Import and its receipt/audit commit together. A repeated preview never overwrites newer XP.
    pub async fn apply_level_import(
        &self,
        rows: &[(u64, i64)],
        replace: bool,
        audit: &crate::NewAudit<'_>,
    ) -> anyhow::Result<(u64, bool)> {
        anyhow::ensure!(
            !rows.is_empty()
                && rows.len() <= 10_000
                && rows
                    .iter()
                    .all(|(id, xp)| *id > 0 && *id <= i64::MAX as u64 && (0..=1_000_000_000).contains(xp)),
            "invalid import"
        );
        anyhow::ensure!(
            audit.action == "xp.import.offline" && audit.target.is_some(),
            "missing import receipt"
        );
        let users: Vec<i64> = rows.iter().map(|r| sql_id(r.0)).collect();
        let amounts: Vec<i64> = rows.iter().map(|r| r.1).collect();
        let mut tx = self.pool().begin().await?;
        lock_users(&mut tx, &users).await?;
        // Receipt requests share the same user locks, including across pods.
        let old: Option<String> = sqlx::query_scalar("SELECT after::text FROM dashboard_audit WHERE guild_id=$1 AND actor_id=$2 AND action='xp.import.offline' AND target=$3 LIMIT 1")
            .bind(sql_id(audit.guild)).bind(sql_id(audit.actor)).bind(audit.target).fetch_optional(&mut *tx).await?;
        if let Some(old) = old {
            let old: serde_json::Value = serde_json::from_str(&old)?;
            tx.commit().await?;
            return Ok((old["updated"].as_u64().unwrap_or(0), false));
        }
        let updated = sqlx::query("INSERT INTO members (guild_id,user_id,xp) SELECT $1, user_id, xp FROM UNNEST($2::BIGINT[],$3::BIGINT[]) p(user_id,xp)
            ON CONFLICT (guild_id,user_id) DO UPDATE SET xp=CASE WHEN $4 THEN EXCLUDED.xp ELSE GREATEST(members.xp,EXCLUDED.xp) END
            WHERE CASE WHEN $4 THEN members.xp <> EXCLUDED.xp ELSE members.xp < EXCLUDED.xp END")
            .bind(sql_id(audit.guild)).bind(&users).bind(&amounts).bind(replace).execute(&mut *tx).await?.rows_affected();
        let after = serde_json::json!({"mode": if replace { "replace" } else { "merge" }, "checked": rows.len(), "updated": updated}).to_string();
        crate::audit::insert_in(
            &mut tx,
            &crate::NewAudit {
                after: Some(&after),
                ..audit.clone()
            },
        )
        .await?;
        tx.commit().await?;
        Ok((updated, true))
    }

    pub async fn preview_member_xp_import(
        &self,
        guild: u64,
        rows: &[(u64, i64)],
        replace: bool,
    ) -> anyhow::Result<Vec<(i64, i64, i64, i64)>> {
        let users: Vec<i64> = rows.iter().map(|r| sql_id(r.0)).collect();
        let amounts: Vec<i64> = rows.iter().map(|r| r.1).collect();
        Ok(sqlx::query_as(
            "WITH incoming AS (
                SELECT user_id, xp FROM UNNEST($2::BIGINT[], $3::BIGINT[]) AS t(user_id, xp)
            ) SELECT i.user_id, i.xp, COALESCE(m.xp, 0) AS current_xp,
                CASE WHEN $4 THEN i.xp ELSE GREATEST(COALESCE(m.xp, 0), i.xp) END AS result_xp
                FROM incoming i LEFT JOIN members m ON m.guild_id = $1 AND m.user_id = i.user_id
                ORDER BY i.user_id",
        )
        .bind(sql_id(guild))
        .bind(&users)
        .bind(&amounts)
        .bind(replace)
        .fetch_all(self.pool())
        .await?)
    }
    pub async fn voice_level_roles(&self, guild: u64) -> anyhow::Result<Vec<VoiceLevelRole>> {
        Ok(
            sqlx::query_as("SELECT role_id, level FROM voice_level_roles WHERE guild_id = $1 ORDER BY level, role_id")
                .bind(sql_id(guild))
                .fetch_all(self.pool())
                .await?,
        )
    }

    pub async fn add_voice_level_role(&self, guild: u64, role: u64, level: i32) -> anyhow::Result<bool> {
        anyhow::ensure!((1..=1000).contains(&level), "invalid level");
        self.guild_settings(guild).await?;
        let mut tx = self.pool().begin().await?;
        sqlx::query("SELECT id FROM guilds WHERE id=$1 FOR UPDATE")
            .bind(sql_id(guild))
            .execute(&mut *tx)
            .await?;
        let n = sqlx::query(
            "INSERT INTO voice_level_roles (guild_id, role_id, level)
            SELECT $1, $2, $3 WHERE (SELECT count(*) FROM voice_level_roles WHERE guild_id = $1) < 25
            OR EXISTS (SELECT 1 FROM voice_level_roles WHERE guild_id = $1 AND role_id = $2)
            ON CONFLICT (guild_id, role_id) DO UPDATE SET level = EXCLUDED.level",
        )
        .bind(sql_id(guild))
        .bind(sql_id(role))
        .bind(level)
        .execute(&mut *tx)
        .await?
        .rows_affected();
        tx.commit().await?;
        Ok(n > 0)
    }

    pub async fn remove_voice_level_role(&self, guild: u64, role: u64) -> anyhow::Result<bool> {
        Ok(
            sqlx::query("DELETE FROM voice_level_roles WHERE guild_id = $1 AND role_id = $2")
                .bind(sql_id(guild))
                .bind(sql_id(role))
                .execute(self.pool())
                .await?
                .rows_affected()
                > 0,
        )
    }
    pub async fn due_leveling_highlights(&self, week: NaiveDate) -> anyhow::Result<Vec<(u64, u64)>> {
        let rows: Vec<(i64, String)> = sqlx::query_as("SELECT p.guild_id, p.config->>'highlights_channel_id'
            FROM guild_leveling_parity p WHERE p.config->>'highlights_channel_id' IS NOT NULL
            AND NOT EXISTS (SELECT 1 FROM guild_leveling_highlight_posts h WHERE h.guild_id = p.guild_id AND h.week = $1)
            ORDER BY p.guild_id LIMIT 1000")
            .bind(week).fetch_all(self.pool()).await?;
        Ok(rows
            .into_iter()
            .filter_map(|(g, ch)| ch.parse().ok().map(|ch| (g as u64, ch)))
            .collect())
    }

    pub async fn claim_leveling_highlight(&self, guild: u64, week: NaiveDate) -> anyhow::Result<bool> {
        Ok(sqlx::query(
            "INSERT INTO guild_leveling_highlight_posts (guild_id, week) VALUES ($1, $2) ON CONFLICT DO NOTHING",
        )
        .bind(sql_id(guild))
        .bind(week)
        .execute(self.pool())
        .await?
        .rows_affected()
            > 0)
    }

    pub async fn retry_leveling_highlight(&self, guild: u64, week: NaiveDate) -> anyhow::Result<()> {
        sqlx::query("DELETE FROM guild_leveling_highlight_posts WHERE guild_id = $1 AND week = $2")
            .bind(sql_id(guild))
            .bind(week)
            .execute(self.pool())
            .await?;
        Ok(())
    }

    pub async fn leveling_parity(&self, guild: u64) -> anyhow::Result<LevelingParity> {
        let value: Option<serde_json::Value> =
            sqlx::query_scalar("SELECT config FROM guild_leveling_parity WHERE guild_id = $1")
                .bind(sql_id(guild))
                .fetch_optional(self.pool())
                .await?;
        Ok(match value {
            Some(value) => serde_json::from_value(value)?,
            None => LevelingParity::default(),
        })
    }

    pub async fn save_leveling_parity(&self, guild: u64, config: &LevelingParity) -> anyhow::Result<()> {
        config.validate().map_err(anyhow::Error::msg)?;
        self.guild_settings(guild).await?;
        sqlx::query(
            "INSERT INTO guild_leveling_parity (guild_id, config) VALUES ($1, $2)
            ON CONFLICT (guild_id) DO UPDATE SET config = EXCLUDED.config",
        )
        .bind(sql_id(guild))
        .bind(serde_json::to_value(config)?)
        .execute(self.pool())
        .await?;
        Ok(())
    }

    pub async fn vanity_guild(&self, slug: &str) -> anyhow::Result<Option<u64>> {
        let id: Option<i64> =
            sqlx::query_scalar("SELECT guild_id FROM guild_leveling_parity WHERE config->>'vanity_slug' = $1")
                .bind(slug)
                .fetch_optional(self.pool())
                .await?;
        Ok(id.map(|id| id as u64))
    }

    pub async fn voice_level_xp(&self, guild: u64, user: u64) -> anyhow::Result<i64> {
        Ok(
            sqlx::query_scalar("SELECT xp FROM member_voice_levels WHERE guild_id = $1 AND user_id = $2")
                .bind(sql_id(guild))
                .bind(sql_id(user))
                .fetch_optional(self.pool())
                .await?
                .unwrap_or(0),
        )
    }

    pub async fn voice_level_rank(&self, guild: u64, user: u64) -> anyhow::Result<(i64, i64)> {
        let row: Option<(i64, i64)> = sqlx::query_as(
            "SELECT v.xp,
            (SELECT count(*) FROM member_voice_levels o WHERE o.guild_id = v.guild_id AND o.xp > v.xp) + 1
            FROM member_voice_levels v WHERE v.guild_id = $1 AND v.user_id = $2",
        )
        .bind(sql_id(guild))
        .bind(sql_id(user))
        .fetch_optional(self.pool())
        .await?;
        if let Some(row) = row {
            return Ok(row);
        }
        let count: i64 = sqlx::query_scalar("SELECT count(*) FROM member_voice_levels WHERE guild_id = $1 AND xp > 0")
            .bind(sql_id(guild))
            .fetch_one(self.pool())
            .await?;
        Ok((0, count + 1))
    }

    pub async fn voice_level_board(&self, guild: u64, limit: i64, offset: i64) -> anyhow::Result<Vec<LeaderboardRow>> {
        Ok(sqlx::query_as(
            "SELECT user_id, xp AS value FROM member_voice_levels
            WHERE guild_id = $1 AND xp > 0 ORDER BY xp DESC, user_id LIMIT $2 OFFSET $3",
        )
        .bind(sql_id(guild))
        .bind(limit)
        .bind(offset)
        .fetch_all(self.pool())
        .await?)
    }

    pub async fn public_level_board(
        &self,
        guild: u64,
        week: Option<NaiveDate>,
        limit: i64,
    ) -> anyhow::Result<Vec<PublicLevelRow>> {
        let rows = if let Some(week) = week {
            sqlx::query_as("SELECT w.user_id, w.xp AS value, n.name FROM weekly_xp w
                LEFT JOIN social_names n ON n.user_id = w.user_id
                WHERE w.guild_id = $1 AND w.week = $2 AND w.xp > 0
                AND NOT EXISTS (SELECT 1 FROM user_privacy p WHERE p.user_id = w.user_id AND (p.hide_profile OR p.hide_leaderboard))
                ORDER BY w.xp DESC, w.user_id LIMIT $3")
                .bind(sql_id(guild)).bind(week).bind(limit).fetch_all(self.pool()).await?
        } else {
            sqlx::query_as("SELECT m.user_id, m.xp AS value, n.name FROM members m
                LEFT JOIN social_names n ON n.user_id = m.user_id
                WHERE m.guild_id = $1 AND m.xp > 0
                AND NOT EXISTS (SELECT 1 FROM user_privacy p WHERE p.user_id = m.user_id AND (p.hide_profile OR p.hide_leaderboard))
                ORDER BY m.xp DESC, m.user_id LIMIT $2")
                .bind(sql_id(guild)).bind(limit).fetch_all(self.pool()).await?
        };
        Ok(rows)
    }

    pub async fn schedule_level_reset(
        &self,
        guild: u64,
        user: u64,
        at: chrono::DateTime<chrono::Utc>,
    ) -> anyhow::Result<()> {
        let mut tx = self.pool().begin().await?;
        lock_users(&mut tx, &[sql_id(user)]).await?;
        sqlx::query("INSERT INTO leveling_departures (guild_id,user_id,reset_at) VALUES ($1,$2,$3) ON CONFLICT (guild_id,user_id) DO NOTHING")
            .bind(sql_id(guild)).bind(sql_id(user)).bind(at).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(())
    }

    /// A rejoin cancels a pending reset; an already expired grace period still resets XP.
    pub async fn finish_level_departure(
        &self,
        guild: u64,
        user: u64,
        now: chrono::DateTime<chrono::Utc>,
        rejoined: bool,
    ) -> anyhow::Result<bool> {
        let mut tx = self.pool().begin().await?;
        lock_users(&mut tx, &[sql_id(user)]).await?;
        let due: Option<bool> = sqlx::query_scalar("DELETE FROM leveling_departures WHERE guild_id=$1 AND user_id=$2 AND ($4 OR reset_at <= $3) RETURNING reset_at <= $3")
            .bind(sql_id(guild)).bind(sql_id(user)).bind(now).bind(rejoined).fetch_optional(&mut *tx).await?;
        let enabled: Option<bool> = sqlx::query_scalar(
            "SELECT COALESCE((config->>'reset_on_leave')::boolean, false) FROM guild_leveling_parity WHERE guild_id=$1",
        )
        .bind(sql_id(guild))
        .fetch_optional(&mut *tx)
        .await?;
        let reset = due == Some(true) && enabled == Some(true);
        if reset {
            reset_in(&mut tx, guild, user).await?;
        }
        tx.commit().await?;
        Ok(reset)
    }

    pub async fn due_level_departures(&self, now: chrono::DateTime<chrono::Utc>) -> anyhow::Result<Vec<(i64, i64)>> {
        Ok(sqlx::query_as(
            "SELECT guild_id,user_id FROM leveling_departures WHERE reset_at <= $1 ORDER BY reset_at LIMIT 1000",
        )
        .bind(now)
        .fetch_all(self.pool())
        .await?)
    }

    pub async fn reset_departed_level(&self, guild: u64, user: u64) -> anyhow::Result<()> {
        let mut tx = self.pool().begin().await?;
        lock_users(&mut tx, &[sql_id(user)]).await?;
        reset_in(&mut tx, guild, user).await?;
        tx.commit().await?;
        Ok(())
    }
}

async fn reset_in(tx: &mut sqlx::Transaction<'_, sqlx::Postgres>, guild: u64, user: u64) -> anyhow::Result<()> {
    sqlx::query("UPDATE members SET xp = 0 WHERE guild_id = $1 AND user_id = $2")
        .bind(sql_id(guild))
        .bind(sql_id(user))
        .execute(&mut **tx)
        .await?;
    sqlx::query("DELETE FROM weekly_xp WHERE guild_id = $1 AND user_id = $2")
        .bind(sql_id(guild))
        .bind(sql_id(user))
        .execute(&mut **tx)
        .await?;
    sqlx::query("DELETE FROM member_voice_levels WHERE guild_id = $1 AND user_id = $2")
        .bind(sql_id(guild))
        .bind(sql_id(user))
        .execute(&mut **tx)
        .await?;
    Ok(())
}
