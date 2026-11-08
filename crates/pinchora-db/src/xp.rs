//! The leveling toolkit's storage: XP multipliers, weekly XP and voice XP.

use chrono::NaiveDate;
use pinchora_core::leveling::XpRules;

use crate::{
    Db, LeaderboardRow, XpGain,
    locks::{lock_users, retry},
    sql_id,
};

/// Most multiplier rules a server can set up, channels and roles together.
pub const MAX_XP_RULES: i64 = 50;

/// What an XP multiplier applies to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum XpTarget {
    Channel,
    Role,
}

impl XpTarget {
    const fn key(self) -> &'static str {
        match self {
            Self::Channel => "channel",
            Self::Role => "role",
        }
    }
}

impl Db {
    /// The server's multiplier rules.
    pub async fn xp_rules(&self, guild_id: u64) -> anyhow::Result<XpRules> {
        let rows: Vec<(String, i64, f64)> =
            sqlx::query_as("SELECT kind, target_id, multiplier FROM xp_rules WHERE guild_id = $1")
                .bind(sql_id(guild_id))
                .fetch_all(self.pool())
                .await?;
        let mut rules = XpRules {
            guild: guild_id,
            ..Default::default()
        };
        for (kind, target, multiplier) in rows {
            let map = if kind == XpTarget::Channel.key() {
                &mut rules.channels
            } else {
                &mut rules.roles
            };
            map.insert(target as u64, multiplier);
        }
        Ok(rules)
    }

    /// Sets the multiplier of a channel or role, replacing an earlier one. `Ok(false)` when the
    /// server is at its limit of rules.
    pub async fn set_xp_rule(
        &self,
        guild_id: u64,
        target: XpTarget,
        target_id: u64,
        multiplier: f64,
    ) -> anyhow::Result<bool> {
        let set = sqlx::query(
            "INSERT INTO xp_rules (guild_id, kind, target_id, multiplier)
             SELECT $1, $2, $3, $4
             WHERE (SELECT count(*) FROM xp_rules WHERE guild_id = $1) < $5
                OR EXISTS (SELECT 1 FROM xp_rules WHERE guild_id = $1 AND kind = $2 AND target_id = $3)
             ON CONFLICT (guild_id, kind, target_id) DO UPDATE SET multiplier = EXCLUDED.multiplier",
        )
        .bind(sql_id(guild_id))
        .bind(target.key())
        .bind(sql_id(target_id))
        .bind(multiplier)
        .bind(MAX_XP_RULES)
        .execute(self.pool())
        .await?
        .rows_affected();
        Ok(set > 0)
    }

    /// Drops one rule; `Ok(false)` when there was none.
    pub async fn remove_xp_rule(&self, guild_id: u64, target: XpTarget, target_id: u64) -> anyhow::Result<bool> {
        let removed = sqlx::query("DELETE FROM xp_rules WHERE guild_id = $1 AND kind = $2 AND target_id = $3")
            .bind(sql_id(guild_id))
            .bind(target.key())
            .bind(sql_id(target_id))
            .execute(self.pool())
            .await?
            .rows_affected();
        Ok(removed > 0)
    }

    /// Drops every rule of the server; returns how many there were.
    pub async fn clear_xp_rules(&self, guild_id: u64) -> anyhow::Result<u64> {
        Ok(sqlx::query("DELETE FROM xp_rules WHERE guild_id = $1")
            .bind(sql_id(guild_id))
            .execute(self.pool())
            .await?
            .rows_affected())
    }

    /// Adds one minute of voice XP to every member in `grants` (user, server XP), in a single
    /// statement after their locks (see the `locks` module), and counts it toward the week. Each
    /// user may appear once. Returns every member's XP before and after, to spot level-ups.
    pub async fn add_voice_xp(
        &self,
        guild_id: u64,
        grants: &[(u64, i64)],
        week: NaiveDate,
    ) -> anyhow::Result<Vec<(u64, XpGain)>> {
        self.add_voice_xp_mode(guild_id, grants, week, false).await
    }

    /// Voice XP with its own level ledger, leaving message level XP untouched.
    pub async fn add_separate_voice_xp(
        &self,
        guild_id: u64,
        grants: &[(u64, i64)],
        week: NaiveDate,
    ) -> anyhow::Result<Vec<(u64, XpGain)>> {
        self.add_voice_xp_mode(guild_id, grants, week, true).await
    }

    async fn add_voice_xp_mode(
        &self,
        guild_id: u64,
        grants: &[(u64, i64)],
        week: NaiveDate,
        separate: bool,
    ) -> anyhow::Result<Vec<(u64, XpGain)>> {
        if grants.is_empty() {
            return Ok(Vec::new());
        }
        // Voice levels reference the guild row; direct grants may arrive before settings are read.
        self.guild_settings(guild_id).await?;
        let users: Vec<i64> = grants.iter().map(|(user, _)| sql_id(*user)).collect();
        let amounts: Vec<i64> = grants.iter().map(|(_, xp)| *xp).collect();
        let rows = retry("add_voice_xp", || {
            self.add_voice_xp_once(guild_id, &users, &amounts, week, true, separate)
        })
        .await?;
        Ok(rows
            .into_iter()
            .map(|(user, new_xp, amount)| {
                (
                    user as u64,
                    XpGain {
                        old_xp: new_xp - amount,
                        new_xp,
                    },
                )
            })
            .collect())
    }

    /// Grants event XP without counting a message, global XP, or voice time.
    pub async fn add_event_xp(&self, guild: u64, user: u64, amount: i64, week: NaiveDate) -> anyhow::Result<XpGain> {
        anyhow::ensure!((1..=25_000).contains(&amount), "event XP out of range");
        let users = [sql_id(user)];
        let amounts = [amount];
        let rows = retry("add_event_xp", || {
            self.add_voice_xp_once(guild, &users, &amounts, week, false, false)
        })
        .await?;
        let (_, new_xp, _) = rows.into_iter().next().expect("one positive grant");
        Ok(XpGain {
            old_xp: new_xp - amount,
            new_xp,
        })
    }

    /// One attempt of [`Db::add_voice_xp`]: (user, XP now, XP added) per member.
    async fn add_voice_xp_once(
        &self,
        guild_id: u64,
        users: &[i64],
        amounts: &[i64],
        week: NaiveDate,
        voice: bool,
        separate: bool,
    ) -> anyhow::Result<Vec<(i64, i64, i64)>> {
        let mut tx = self.pool().begin().await?;
        lock_users(&mut tx, users).await?;
        sqlx::query("INSERT INTO users (id) SELECT UNNEST($1::BIGINT[]) ON CONFLICT DO NOTHING")
            .bind(users)
            .execute(&mut *tx)
            .await?;
        let rows: Vec<(i64, i64, i64)> = sqlx::query_as(
            "WITH g AS (
                 SELECT * FROM UNNEST($2::BIGINT[], $3::BIGINT[]) AS t(user_id, amount) WHERE amount > 0
             ), m AS (
                 INSERT INTO members (guild_id, user_id, xp, last_xp_at)
                 SELECT $1, user_id, amount, now() FROM g WHERE NOT $6::BOOLEAN
                 ON CONFLICT (guild_id, user_id) DO UPDATE
                 SET xp = members.xp + EXCLUDED.xp, last_xp_at = now()
                 RETURNING user_id, xp
             ), w AS (
                 INSERT INTO weekly_xp (guild_id, week, user_id, xp)
                 SELECT $1, $4, user_id, amount FROM g
                 ON CONFLICT (guild_id, week, user_id) DO UPDATE SET xp = weekly_xp.xp + EXCLUDED.xp
             ), a AS (
                 INSERT INTO activity_monthly (user_id, month, xp)
                 SELECT user_id, date_trunc('month', now() AT TIME ZONE 'UTC')::date, amount FROM g
                 ON CONFLICT (user_id, month) DO UPDATE SET xp = activity_monthly.xp + EXCLUDED.xp
             ), v AS (
                 INSERT INTO member_voice_levels (guild_id, user_id, xp)
                 SELECT $1, user_id, amount FROM g WHERE $5::BOOLEAN
                 ON CONFLICT (guild_id, user_id) DO UPDATE SET xp = member_voice_levels.xp + EXCLUDED.xp
                 RETURNING user_id, xp
             )
             SELECT g.user_id, CASE WHEN $6 THEN v.xp ELSE m.xp END, g.amount
             FROM g LEFT JOIN m USING (user_id) LEFT JOIN v USING (user_id)",
        )
        .bind(sql_id(guild_id))
        .bind(users)
        .bind(amounts)
        .bind(week)
        .bind(voice)
        .bind(separate)
        .fetch_all(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(rows)
    }

    /// The server's weekly XP board for the week starting `week`, with the XP as `value`.
    pub async fn weekly_leaderboard(
        &self,
        guild_id: u64,
        week: NaiveDate,
        limit: i64,
        offset: i64,
    ) -> anyhow::Result<Vec<LeaderboardRow>> {
        Ok(sqlx::query_as::<_, LeaderboardRow>(
            "SELECT user_id, xp AS value FROM weekly_xp
             WHERE guild_id = $1 AND week = $2 AND xp > 0
             ORDER BY xp DESC, user_id
             LIMIT $3 OFFSET $4",
        )
        .bind(sql_id(guild_id))
        .bind(week)
        .bind(limit)
        .bind(offset)
        .fetch_all(self.pool())
        .await?)
    }

    /// Forgets the weekly XP of weeks that started before `week`; returns how many rows went. Rows
    /// somebody holds are left for the next sweep (see the `locks` module).
    pub async fn forget_weeks_before(&self, week: NaiveDate) -> anyhow::Result<u64> {
        retry("forget_weeks_before", || async move {
            Ok(sqlx::query(
                "WITH old AS MATERIALIZED (
                     SELECT guild_id, week, user_id FROM weekly_xp WHERE week < $1 FOR UPDATE SKIP LOCKED)
                 DELETE FROM weekly_xp x USING old
                 WHERE x.guild_id = old.guild_id AND x.week = old.week AND x.user_id = old.user_id",
            )
            .bind(week)
            .execute(self.pool())
            .await?
            .rows_affected())
        })
        .await
    }
}
