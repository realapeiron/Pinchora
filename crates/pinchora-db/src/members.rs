use crate::{Db, sql_id};

#[derive(Debug, Clone, Copy)]
pub struct XpGain {
    pub old_xp: i64,
    pub new_xp: i64,
}

#[derive(Debug, Clone, Copy, sqlx::FromRow)]
pub struct MemberRank {
    pub xp: i64,
    pub messages: i64,
    pub rank: i64,
}

#[derive(Debug, Clone, Copy, sqlx::FromRow)]
pub struct LeaderboardRow {
    pub user_id: i64,
    pub value: i64,
}

impl Db {
    /// Adds XP to the member and to the user's global total in one round trip.
    pub async fn add_message_xp(&self, guild_id: u64, user_id: u64, amount: i64) -> anyhow::Result<XpGain> {
        let (new_xp,): (i64,) = sqlx::query_as(
            "WITH m AS (
                 INSERT INTO members (guild_id, user_id, xp, messages, last_xp_at)
                 VALUES ($1, $2, $3, 1, now())
                 ON CONFLICT (guild_id, user_id) DO UPDATE
                 SET xp = members.xp + EXCLUDED.xp,
                     messages = members.messages + 1,
                     last_xp_at = now()
                 RETURNING xp
             ), u AS (
                 INSERT INTO users (id, xp) VALUES ($2, $3)
                 ON CONFLICT (id) DO UPDATE SET xp = users.xp + EXCLUDED.xp
             )
             SELECT xp FROM m",
        )
        .bind(sql_id(guild_id))
        .bind(sql_id(user_id))
        .bind(amount)
        .fetch_one(self.pool())
        .await?;
        Ok(XpGain {
            old_xp: new_xp - amount,
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
             WHERE guild_id = $1
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
        let (n,): (i64,) = sqlx::query_as("SELECT count(*) FROM members WHERE guild_id = $1")
            .bind(sql_id(guild_id))
            .fetch_one(self.pool())
            .await?;
        Ok(n)
    }

    pub async fn global_xp_leaderboard(&self, limit: i64, offset: i64) -> anyhow::Result<Vec<LeaderboardRow>> {
        Ok(sqlx::query_as::<_, LeaderboardRow>(
            "SELECT id AS user_id, xp AS value FROM users ORDER BY xp DESC, id LIMIT $1 OFFSET $2",
        )
        .bind(limit)
        .bind(offset)
        .fetch_all(self.pool())
        .await?)
    }

    pub async fn global_credits_leaderboard(&self, limit: i64, offset: i64) -> anyhow::Result<Vec<LeaderboardRow>> {
        Ok(sqlx::query_as::<_, LeaderboardRow>(
            "SELECT id AS user_id, credits AS value FROM users ORDER BY credits DESC, id LIMIT $1 OFFSET $2",
        )
        .bind(limit)
        .bind(offset)
        .fetch_all(self.pool())
        .await?)
    }
}
