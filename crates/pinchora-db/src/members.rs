use chrono::{NaiveDate, Utc};
use pinchora_core::leveling::week_start;

use crate::{Db, sql_id};

#[derive(Debug, Clone, Copy)]
pub struct XpGain {
    pub old_xp: i64,
    pub new_xp: i64,
}

/// What one chat tick earns: server XP and points, and the share that goes to the user's global XP.
#[derive(Debug, Clone, Copy)]
pub struct XpGrant {
    pub server_xp: i64,
    pub global_xp: i64,
    pub points: i64,
}

#[derive(Debug, Clone, Copy, sqlx::FromRow)]
pub struct MemberRank {
    pub xp: i64,
    pub messages: i64,
    pub rank: i64,
}

/// A member's server points and their place on the points board.
#[derive(Debug, Clone, Copy, sqlx::FromRow)]
pub struct PointsRank {
    pub points: i64,
    pub rank: i64,
}

#[derive(Debug, Clone, Copy, sqlx::FromRow)]
pub struct LeaderboardRow {
    pub user_id: i64,
    pub value: i64,
}

impl Db {
    /// Adds XP and server points to the member, and the XP to the user's global total, in one round trip.
    pub async fn add_message_xp(
        &self,
        guild_id: u64,
        user_id: u64,
        amount: i64,
        points: i64,
    ) -> anyhow::Result<XpGain> {
        let grant = XpGrant {
            server_xp: amount,
            global_xp: amount,
            points,
        };
        self.add_xp(guild_id, user_id, grant, week_start(Utc::now())).await
    }

    /// Like [`Db::add_message_xp`] with the server and global shares apart, and the server XP counted
    /// toward the weekly board of the week starting `week`.
    pub async fn add_xp(&self, guild_id: u64, user_id: u64, grant: XpGrant, week: NaiveDate) -> anyhow::Result<XpGain> {
        let (new_xp,): (i64,) = sqlx::query_as(
            "WITH m AS (
                 INSERT INTO members (guild_id, user_id, xp, messages, points, last_xp_at)
                 VALUES ($1, $2, $3, 1, $4, now())
                 ON CONFLICT (guild_id, user_id) DO UPDATE
                 SET xp = members.xp + EXCLUDED.xp,
                     messages = members.messages + 1,
                     points = members.points + EXCLUDED.points,
                     last_xp_at = now()
                 RETURNING xp
             ), u AS (
                 INSERT INTO users (id, xp) VALUES ($2, $5)
                 ON CONFLICT (id) DO UPDATE SET xp = users.xp + EXCLUDED.xp
             ), w AS (
                 INSERT INTO weekly_xp (guild_id, week, user_id, xp)
                 SELECT $1, $6, $2, $3::BIGINT WHERE $3::BIGINT > 0
                 ON CONFLICT (guild_id, week, user_id) DO UPDATE SET xp = weekly_xp.xp + EXCLUDED.xp
             ), a AS (
                 INSERT INTO activity_monthly (user_id, month, messages, xp)
                 VALUES ($2, date_trunc('month', now() AT TIME ZONE 'UTC')::date, 1, $5)
                 ON CONFLICT (user_id, month) DO UPDATE
                 SET messages = activity_monthly.messages + 1, xp = activity_monthly.xp + EXCLUDED.xp
             )
             SELECT xp FROM m",
        )
        .bind(sql_id(guild_id))
        .bind(sql_id(user_id))
        .bind(grant.server_xp)
        .bind(grant.points)
        .bind(grant.global_xp)
        .bind(week)
        .fetch_one(self.pool())
        .await?;
        Ok(XpGain {
            old_xp: new_xp - grant.server_xp,
            new_xp,
        })
    }

    pub async fn member_rank(&self, guild_id: u64, user_id: u64) -> anyhow::Result<Option<MemberRank>> {
        Ok(sqlx::query_as::<_, MemberRank>(
            "SELECT m.xp, m.messages,
                    (SELECT count(*) FROM members o WHERE o.guild_id = m.guild_id AND o.xp > m.xp) + 1 AS rank
             FROM members m
             WHERE m.guild_id = $1 AND m.user_id = $2",
        )
        .bind(sql_id(guild_id))
        .bind(sql_id(user_id))
        .fetch_optional(self.pool())
        .await?)
    }

    pub async fn guild_leaderboard(
        &self,
        guild_id: u64,
        limit: i64,
        offset: i64,
    ) -> anyhow::Result<Vec<LeaderboardRow>> {
        Ok(sqlx::query_as::<_, LeaderboardRow>(
            "SELECT user_id, xp AS value FROM members
             WHERE guild_id = $1 AND xp > 0
             ORDER BY xp DESC, user_id
             LIMIT $2 OFFSET $3",
        )
        .bind(sql_id(guild_id))
        .bind(limit)
        .bind(offset)
        .fetch_all(self.pool())
        .await?)
    }

    pub async fn guild_member_count(&self, guild_id: u64) -> anyhow::Result<i64> {
        let (n,): (i64,) = sqlx::query_as("SELECT count(*) FROM members WHERE guild_id = $1 AND xp > 0")
            .bind(sql_id(guild_id))
            .fetch_one(self.pool())
            .await?;
        Ok(n)
    }

    pub async fn global_xp_leaderboard(&self, limit: i64, offset: i64) -> anyhow::Result<Vec<LeaderboardRow>> {
        Ok(sqlx::query_as::<_, LeaderboardRow>(
            "SELECT id AS user_id, xp AS value FROM users u
             WHERE NOT EXISTS (SELECT 1 FROM user_privacy p WHERE p.user_id = u.id AND p.hide_leaderboard)
             ORDER BY xp DESC, id LIMIT $1 OFFSET $2",
        )
        .bind(limit)
        .bind(offset)
        .fetch_all(self.pool())
        .await?)
    }

    pub async fn global_credits_leaderboard(&self, limit: i64, offset: i64) -> anyhow::Result<Vec<LeaderboardRow>> {
        Ok(sqlx::query_as::<_, LeaderboardRow>(
            "SELECT id AS user_id, credits AS value FROM users u
             WHERE NOT EXISTS (SELECT 1 FROM user_privacy p WHERE p.user_id = u.id AND p.hide_leaderboard)
             ORDER BY credits DESC, id LIMIT $1 OFFSET $2",
        )
        .bind(limit)
        .bind(offset)
        .fetch_all(self.pool())
        .await?)
    }

    /// Adds server points, creating the member row if they have none yet. Returns the new balance.
    pub async fn give_points(&self, guild_id: u64, user_id: u64, amount: i64) -> anyhow::Result<i64> {
        anyhow::ensure!(amount > 0, "invalid amount");
        let (balance,): (i64,) = sqlx::query_as(
            "INSERT INTO members (guild_id, user_id, points) VALUES ($1, $2, $3)
             ON CONFLICT (guild_id, user_id) DO UPDATE SET points = members.points + EXCLUDED.points
             RETURNING points",
        )
        .bind(sql_id(guild_id))
        .bind(sql_id(user_id))
        .bind(amount)
        .fetch_one(self.pool())
        .await?;
        Ok(balance)
    }

    /// Removes up to `amount` server points. Returns `(taken, balance)`; a member never goes below zero.
    pub async fn take_points(&self, guild_id: u64, user_id: u64, amount: i64) -> anyhow::Result<(i64, i64)> {
        anyhow::ensure!(amount > 0, "invalid amount");
        let mut tx = self.pool().begin().await?;
        let row: Option<(i64,)> =
            sqlx::query_as("SELECT points FROM members WHERE guild_id = $1 AND user_id = $2 FOR UPDATE")
                .bind(sql_id(guild_id))
                .bind(sql_id(user_id))
                .fetch_optional(&mut *tx)
                .await?;
        let Some((old,)) = row else {
            return Ok((0, 0));
        };
        let taken = old.min(amount);
        sqlx::query("UPDATE members SET points = points - $3 WHERE guild_id = $1 AND user_id = $2")
            .bind(sql_id(guild_id))
            .bind(sql_id(user_id))
            .bind(taken)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok((taken, old - taken))
    }

    /// The member's server points and rank; `None` rank until they hold any.
    pub async fn member_points(&self, guild_id: u64, user_id: u64) -> anyhow::Result<(i64, Option<i64>)> {
        let row: Option<PointsRank> = sqlx::query_as(
            "SELECT m.points,
                    (SELECT count(*) FROM members o WHERE o.guild_id = m.guild_id AND o.points > m.points) + 1 AS rank
             FROM members m
             WHERE m.guild_id = $1 AND m.user_id = $2",
        )
        .bind(sql_id(guild_id))
        .bind(sql_id(user_id))
        .fetch_optional(self.pool())
        .await?;
        Ok(match row {
            Some(r) if r.points > 0 => (r.points, Some(r.rank)),
            Some(r) => (r.points, None),
            None => (0, None),
        })
    }

    pub async fn guild_points_leaderboard(
        &self,
        guild_id: u64,
        limit: i64,
        offset: i64,
    ) -> anyhow::Result<Vec<LeaderboardRow>> {
        Ok(sqlx::query_as::<_, LeaderboardRow>(
            "SELECT user_id, points AS value FROM members
             WHERE guild_id = $1 AND points > 0
             ORDER BY points DESC, user_id
             LIMIT $2 OFFSET $3",
        )
        .bind(sql_id(guild_id))
        .bind(limit)
        .bind(offset)
        .fetch_all(self.pool())
        .await?)
    }

    /// Members with XP here, most recently active first, with their XP as `value`.
    pub async fn recent_xp_members(&self, guild_id: u64, limit: i64) -> anyhow::Result<Vec<LeaderboardRow>> {
        Ok(sqlx::query_as::<_, LeaderboardRow>(
            "SELECT user_id, xp AS value FROM members
             WHERE guild_id = $1 AND xp > 0
             ORDER BY last_xp_at DESC NULLS LAST, user_id
             LIMIT $2",
        )
        .bind(sql_id(guild_id))
        .bind(limit)
        .fetch_all(self.pool())
        .await?)
    }
}
