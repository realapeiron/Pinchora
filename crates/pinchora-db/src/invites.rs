//! Invite snapshots and join accounting. A guild advisory lock serializes reconciliation across pods.

use chrono::{DateTime, Utc};
use sqlx::FromRow;

use crate::{
    Db,
    locks::{lock_users, retry, try_lock_users},
    sql_id,
};

#[derive(Debug, Clone, FromRow)]
pub struct InviteSettings {
    pub enabled: bool,
    pub baseline_required: bool,
    pub log_channel_id: Option<i64>,
    pub fake_days: i32,
    pub temp_enabled: bool,
    pub temp_channel_id: Option<i64>,
    pub temp_max_age_secs: i32,
    pub temp_max_uses: i32,
    pub temp_per_user_daily: i32,
    pub temp_message_id: Option<i64>,
}

impl Default for InviteSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            baseline_required: false,
            log_channel_id: None,
            fake_days: 7,
            temp_enabled: false,
            temp_channel_id: None,
            temp_max_age_secs: 172800,
            temp_max_uses: 1,
            temp_per_user_daily: 3,
            temp_message_id: None,
        }
    }
}

#[derive(Debug, Clone, FromRow)]
pub struct GuildInvite {
    pub code: String,
    pub inviter_id: Option<i64>,
    pub uses: i32,
    pub max_uses: i32,
    pub max_age: i32,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, FromRow)]
pub struct InviteJoin {
    pub user_id: i64,
    pub inviter_id: Option<i64>,
    pub code: Option<String>,
    pub joined_at: DateTime<Utc>,
    pub left_at: Option<DateTime<Utc>>,
    pub fake: bool,
    pub source: String,
}

#[derive(Debug, Clone, FromRow)]
pub struct InviteScore {
    pub user_id: i64,
    pub regular: i64,
    pub left: i64,
    pub fake: i64,
    pub bonus: i32,
    pub total: i64,
}

/// What Discord showed at the time of a join, and whether the list could be read.
pub struct InviteObservation<'a> {
    pub invites: &'a [GuildInvite],
    pub vanity_changed: bool,
    pub list_available: bool,
    /// A hint from the REST snapshot; ambiguous changes are never attributed using this hint.
    pub preferred_code: Option<&'a str>,
}

#[derive(FromRow)]
struct ExistingJoin {
    id: i64,
    inviter_id: Option<i64>,
    code: Option<String>,
    left_at: Option<DateTime<Utc>>,
    source: String,
}

#[derive(Debug, Clone)]
pub struct JoinCredit {
    pub inviter_id: Option<u64>,
    pub source: &'static str,
    pub code: Option<String>,
    pub total: i64,
    pub fresh: bool,
}

impl Db {
    /// Holds a player's reward ordering lock until the returned transaction is committed
    /// or dropped. The user lock comes first, as required by `locks.rs`.
    pub async fn invite_reward_guard(
        &self,
        guild: u64,
        user: u64,
    ) -> anyhow::Result<sqlx::Transaction<'_, sqlx::Postgres>> {
        let mut tx = self.pool().begin().await?;
        lock_users(&mut tx, &[sql_id(user)]).await?;
        lock_reward_in_tx(&mut tx, guild, sql_id(user)).await?;
        Ok(tx)
    }

    pub async fn invite_settings(&self, guild: u64) -> anyhow::Result<InviteSettings> {
        Ok(sqlx::query_as("SELECT enabled, baseline_required, log_channel_id, fake_days, temp_enabled, temp_channel_id, \
            temp_max_age_secs, temp_max_uses, temp_per_user_daily, temp_message_id FROM invite_settings WHERE guild_id=$1").bind(sql_id(guild)).fetch_optional(self.pool()).await?.unwrap_or_default())
    }

    pub async fn save_invite_settings(&self, guild: u64, settings: &InviteSettings) -> anyhow::Result<()> {
        self.guild_settings(guild).await?;
        sqlx::query(
            "INSERT INTO invite_settings (guild_id, enabled, log_channel_id, fake_days, temp_enabled, \
            temp_channel_id, temp_max_age_secs, temp_max_uses, temp_per_user_daily, temp_message_id) \
            VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10) ON CONFLICT (guild_id) DO UPDATE SET \
            baseline_required=invite_settings.baseline_required OR invite_settings.enabled IS DISTINCT FROM $2, \
            enabled=$2, log_channel_id=$3, fake_days=$4, temp_enabled=$5, temp_channel_id=$6, \
            temp_max_age_secs=$7, temp_max_uses=$8, temp_per_user_daily=$9, temp_message_id=$10",
        )
        .bind(sql_id(guild))
        .bind(settings.enabled)
        .bind(settings.log_channel_id)
        .bind(settings.fake_days)
        .bind(settings.temp_enabled)
        .bind(settings.temp_channel_id)
        .bind(settings.temp_max_age_secs)
        .bind(settings.temp_max_uses)
        .bind(settings.temp_per_user_daily)
        .bind(settings.temp_message_id)
        .execute(self.pool())
        .await?;
        Ok(())
    }

    pub async fn invite_rewards(&self, guild: u64) -> anyhow::Result<Vec<(i32, i64)>> {
        Ok(
            sqlx::query_as("SELECT invites, role_id FROM invite_rewards WHERE guild_id=$1 ORDER BY invites")
                .bind(sql_id(guild))
                .fetch_all(self.pool())
                .await?,
        )
    }

    pub async fn invite_rewards_with_grantor(&self, guild: u64) -> anyhow::Result<Vec<(i32, i64, Option<i64>)>> {
        Ok(
            sqlx::query_as("SELECT invites,role_id,grantor_id FROM invite_rewards WHERE guild_id=$1 ORDER BY invites")
                .bind(sql_id(guild))
                .fetch_all(self.pool())
                .await?,
        )
    }

    pub async fn put_invite_reward(&self, guild: u64, count: i32, role: u64) -> anyhow::Result<()> {
        self.put_invite_reward_by(guild, count, role, None).await
    }

    pub async fn put_invite_reward_by(
        &self,
        guild: u64,
        count: i32,
        role: u64,
        grantor: Option<u64>,
    ) -> anyhow::Result<()> {
        self.guild_settings(guild).await?;
        sqlx::query(
            "INSERT INTO invite_rewards(guild_id,invites,role_id,grantor_id) VALUES($1,$2,$3,$4) \
            ON CONFLICT(guild_id,invites) DO UPDATE SET role_id=$3,grantor_id=$4",
        )
        .bind(sql_id(guild))
        .bind(count)
        .bind(sql_id(role))
        .bind(grantor.map(sql_id))
        .execute(self.pool())
        .await?;
        Ok(())
    }

    pub async fn invite_reward_authority(&self, guild: u64, role: u64) -> anyhow::Result<Vec<Option<u64>>> {
        Ok(sqlx::query_as::<_, (Option<i64>,)>(
            "SELECT grantor_id FROM invite_rewards WHERE guild_id=$1 AND role_id=$2",
        )
        .bind(sql_id(guild))
        .bind(sql_id(role))
        .fetch_all(self.pool())
        .await?
        .into_iter()
        .map(|(id,)| id.map(|n| n as u64))
        .collect())
    }

    pub async fn invite_granted_roles(&self, guild: u64, user: u64) -> anyhow::Result<Vec<u64>> {
        Ok(
            sqlx::query_as::<_, (i64,)>("SELECT role_id FROM invite_role_grants WHERE guild_id=$1 AND user_id=$2")
                .bind(sql_id(guild))
                .bind(sql_id(user))
                .fetch_all(self.pool())
                .await?
                .into_iter()
                .map(|(id,)| id as u64)
                .collect(),
        )
    }

    pub async fn mark_invite_role(&self, guild: u64, user: u64, role: u64, granted: bool) -> anyhow::Result<()> {
        if granted {
            sqlx::query(
                "INSERT INTO invite_role_grants(guild_id,user_id,role_id) VALUES($1,$2,$3) ON CONFLICT DO NOTHING",
            )
            .bind(sql_id(guild))
            .bind(sql_id(user))
            .bind(sql_id(role))
            .execute(self.pool())
            .await?;
        } else {
            sqlx::query("DELETE FROM invite_role_grants WHERE guild_id=$1 AND user_id=$2 AND role_id=$3")
                .bind(sql_id(guild))
                .bind(sql_id(user))
                .bind(sql_id(role))
                .execute(self.pool())
                .await?;
        }
        Ok(())
    }

    pub async fn invite_inviter_for_member(&self, guild: u64, user: u64) -> anyhow::Result<Option<u64>> {
        let row: Option<(Option<i64>,)> = sqlx::query_as(
            "SELECT inviter_id FROM invite_joins WHERE guild_id=$1 AND user_id=$2 ORDER BY joined_at DESC LIMIT 1",
        )
        .bind(sql_id(guild))
        .bind(sql_id(user))
        .fetch_optional(self.pool())
        .await?;
        Ok(row.and_then(|(id,)| id.map(|n| n as u64)))
    }

    pub async fn invite_reward_candidates(&self, guild: u64) -> anyhow::Result<Vec<u64>> {
        Ok(sqlx::query_as::<_, (i64,)>("SELECT DISTINCT user_id FROM invite_role_grants WHERE guild_id=$1 UNION SELECT DISTINCT inviter_id FROM invite_joins WHERE guild_id=$1 AND inviter_id IS NOT NULL UNION SELECT user_id FROM invite_bonus WHERE guild_id=$1")
            .bind(sql_id(guild)).fetch_all(self.pool()).await?
            .into_iter().map(|(id,)| id as u64).collect())
    }

    pub async fn invite_reward_guilds(&self) -> anyhow::Result<Vec<u64>> {
        Ok(sqlx::query_as::<_, (i64,)>(
            "SELECT guild_id FROM invite_rewards UNION SELECT guild_id FROM invite_role_grants",
        )
        .fetch_all(self.pool())
        .await?
        .into_iter()
        .map(|(id,)| id as u64)
        .collect())
    }

    pub async fn remove_invite_reward(&self, guild: u64, count: i32) -> anyhow::Result<()> {
        sqlx::query("DELETE FROM invite_rewards WHERE guild_id=$1 AND invites=$2")
            .bind(sql_id(guild))
            .bind(count)
            .execute(self.pool())
            .await?;
        Ok(())
    }

    pub async fn invite_snapshot(&self, guild: u64) -> anyhow::Result<Vec<GuildInvite>> {
        Ok(sqlx::query_as(
            "SELECT code, inviter_id, uses, max_uses, max_age, created_at \
            FROM guild_invites WHERE guild_id=$1",
        )
        .bind(sql_id(guild))
        .fetch_all(self.pool())
        .await?)
    }

    /// Add newly observed codes without consuming pending uses on existing codes.
    pub async fn baseline_invites(&self, guild: u64, invites: &[GuildInvite]) -> anyhow::Result<()> {
        self.guild_settings(guild).await?;
        let mut tx = self.pool().begin().await?;
        let ids = invites.iter().filter_map(|i| i.inviter_id).collect::<Vec<_>>();
        lock_users(&mut tx, &ids).await?;
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended('invites:' || $1::text, 0))")
            .bind(sql_id(guild))
            .execute(&mut *tx)
            .await?;
        for invite in invites {
            let old: Option<(i32,)> = sqlx::query_as("SELECT uses FROM guild_invites WHERE guild_id=$1 AND code=$2")
                .bind(sql_id(guild))
                .bind(&invite.code)
                .fetch_optional(&mut *tx)
                .await?;
            upsert_invite(&mut tx, guild, invite, old.map_or(invite.uses, |(n,)| n)).await?;
        }
        tx.commit().await?;
        Ok(())
    }

    /// A gateway connection epoch starts from Discord's current counts. Uses accumulated
    /// while offline belong to unknown joins and must not be assigned after reconnect.
    pub async fn reset_invite_baseline(&self, guild: u64, invites: &[GuildInvite]) -> anyhow::Result<()> {
        self.guild_settings(guild).await?;
        let mut tx = self.pool().begin().await?;
        let ids = invites.iter().filter_map(|i| i.inviter_id).collect::<Vec<_>>();
        lock_users(&mut tx, &ids).await?;
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended('invites:' || $1::text, 0))")
            .bind(sql_id(guild))
            .execute(&mut *tx)
            .await?;
        for invite in invites {
            upsert_invite(&mut tx, guild, invite, invite.uses).await?;
        }
        sqlx::query("UPDATE invite_settings SET baseline_required=FALSE WHERE guild_id=$1")
            .bind(sql_id(guild))
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(())
    }

    /// The first join after re-enabling claims the new baseline. Concurrent joins cannot
    /// each reset it and erase uses observed after the first reset.
    pub async fn reset_required_invite_baseline(&self, guild: u64, invites: &[GuildInvite]) -> anyhow::Result<bool> {
        let mut tx = self.pool().begin().await?;
        let ids = invites.iter().filter_map(|i| i.inviter_id).collect::<Vec<_>>();
        lock_users(&mut tx, &ids).await?;
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended('invites:' || $1::text, 0))")
            .bind(sql_id(guild))
            .execute(&mut *tx)
            .await?;
        let changed = sqlx::query(
            "UPDATE invite_settings SET baseline_required=FALSE WHERE guild_id=$1 AND baseline_required=TRUE",
        )
        .bind(sql_id(guild))
        .execute(&mut *tx)
        .await?;
        if changed.rows_affected() == 0 {
            tx.commit().await?;
            return Ok(false);
        }
        for invite in invites {
            upsert_invite(&mut tx, guild, invite, invite.uses).await?;
        }
        tx.commit().await?;
        Ok(true)
    }

    pub async fn require_invite_baseline(&self, guild: u64) -> anyhow::Result<()> {
        sqlx::query("UPDATE invite_settings SET baseline_required=TRUE WHERE guild_id=$1")
            .bind(sql_id(guild))
            .execute(self.pool())
            .await?;
        Ok(())
    }

    /// Consume at most one new use per join, leaving further increments for another join in the same second.
    /// The guild transaction lock makes two pods unable to credit the same use.
    pub async fn record_invite_join(
        &self,
        guild: u64,
        user: u64,
        now: DateTime<Utc>,
        fake: bool,
        observation: InviteObservation<'_>,
    ) -> anyhow::Result<JoinCredit> {
        self.guild_settings(guild).await?;
        let prior: Option<(Option<i64>,)> = sqlx::query_as(
            "SELECT inviter_id FROM invite_joins WHERE guild_id=$1 AND user_id=$2 ORDER BY joined_at DESC LIMIT 1",
        )
        .bind(sql_id(guild))
        .bind(sql_id(user))
        .fetch_optional(self.pool())
        .await?;
        let mut ids = observation
            .invites
            .iter()
            .filter_map(|i| i.inviter_id)
            .collect::<Vec<_>>();
        ids.push(sql_id(user));
        if let Some((Some(inviter),)) = prior {
            ids.push(inviter);
        }
        let mut tx = self.pool().begin().await?;
        lock_users(&mut tx, &ids).await?;
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended('invites:' || $1::text, 0))")
            .bind(sql_id(guild))
            .execute(&mut *tx)
            .await?;
        let existing: Option<ExistingJoin> = sqlx::query_as(
            "SELECT id, inviter_id, code, left_at, source FROM invite_joins WHERE guild_id=$1 AND user_id=$2 ORDER BY joined_at DESC LIMIT 1")
            .bind(sql_id(guild)).bind(sql_id(user)).fetch_optional(&mut *tx).await?;
        if let Some(existing) = existing {
            // A gateway replay must not spend the next member's pending use. A leave recorded
            // before a slow join is a tombstone, not a rejoin.
            if let Some(inviter) = existing.inviter_id {
                if !ids.contains(&inviter) {
                    try_lock_users(&mut tx, &[inviter]).await?;
                }
                lock_reward_in_tx(&mut tx, guild, inviter).await?;
            }
            if existing.code.as_deref() == Some("__leave_before_join__") {
                sqlx::query("UPDATE invite_joins SET code='__ignored_join__' WHERE id=$1")
                    .bind(existing.id)
                    .execute(&mut *tx)
                    .await?;
            } else if existing.left_at.is_some_and(|left| now > left) {
                sqlx::query("UPDATE invite_joins SET left_at=NULL WHERE id=$1")
                    .bind(existing.id)
                    .execute(&mut *tx)
                    .await?;
            }
            let total = if let Some(inviter) = existing.inviter_id {
                score_in_tx(&mut tx, guild, inviter, None).await?.total
            } else {
                0
            };
            tx.commit().await?;
            return Ok(JoinCredit {
                inviter_id: existing.inviter_id.map(|n| n as u64),
                source: match existing.source.as_str() {
                    "invite" => "invite",
                    "vanity" => "vanity",
                    "temporary" => "temporary",
                    "widget" => "widget",
                    _ => "unknown",
                },
                code: None,
                total,
                fresh: false,
            });
        }
        let old: Vec<(String, i32)> = sqlx::query_as("SELECT code, uses FROM guild_invites WHERE guild_id=$1")
            .bind(sql_id(guild))
            .fetch_all(&mut *tx)
            .await?;
        let observed = observation.invites;
        let changed = observed
            .iter()
            .filter(|i| {
                old.iter()
                    .find(|(code, _)| code == &i.code)
                    .is_some_and(|(_, uses)| i.uses > *uses)
            })
            .collect::<Vec<_>>();
        // Two changed codes (or a code plus vanity) contain no evidence of which one
        // this member used. Retire all those deltas so a later join cannot inherit them.
        let ambiguous = changed.len() > 1 || (observation.vanity_changed && !changed.is_empty());
        let selected = if ambiguous { None } else { changed.first().copied() };
        let codes = observed.iter().map(|i| i.code.clone()).collect::<Vec<_>>();
        let (recent_missing,): (bool,) = sqlx::query_as(
            "SELECT EXISTS (SELECT 1 FROM guild_invites WHERE guild_id=$1 \
            AND max_uses=1 AND seen_at > $3::timestamptz - interval '2 minutes' AND NOT (code = ANY($2::text[])))",
        )
        .bind(sql_id(guild))
        .bind(&codes)
        .bind(now)
        .fetch_one(&mut *tx)
        .await?;
        let (inviter, code, source) = match selected {
            Some(i) => (i.inviter_id, Some(i.code.clone()), "invite"),
            None if observation.vanity_changed && !ambiguous => (None, None, "vanity"),
            None if observation.list_available && recent_missing => (None, None, "temporary"),
            None => (None, None, "unknown"),
        };
        if let Some(inviter) = inviter {
            lock_reward_in_tx(&mut tx, guild, inviter).await?;
        }
        for invite in observed {
            let used = old
                .iter()
                .find(|(code, _)| code == &invite.code)
                .map_or(invite.uses, |(_, n)| {
                    if selected.is_some_and(|selected| selected.code == invite.code) {
                        *n + 1
                    } else if ambiguous && changed.iter().any(|i| i.code == invite.code) {
                        invite.uses
                    } else {
                        *n
                    }
                });
            upsert_invite(
                &mut tx,
                guild,
                invite,
                used.max(
                    *old.iter()
                        .find(|(code, _)| code == &invite.code)
                        .map(|(_, n)| n)
                        .unwrap_or(&0),
                ),
            )
            .await?;
        }
        sqlx::query(
            "INSERT INTO invite_joins(guild_id,user_id,inviter_id,code,joined_at,fake,source) \
                VALUES($1,$2,$3,$4,$5,$6,$7)",
        )
        .bind(sql_id(guild))
        .bind(sql_id(user))
        .bind(inviter)
        .bind(&code)
        .bind(now)
        .bind(fake)
        .bind(source)
        .execute(&mut *tx)
        .await?;
        let total = if let Some(inviter) = inviter {
            score_in_tx(&mut tx, guild, inviter, None).await?.total
        } else {
            0
        };
        tx.commit().await?;
        Ok(JoinCredit {
            inviter_id: inviter.map(|n| n as u64),
            source,
            code,
            total,
            fresh: true,
        })
    }

    pub async fn leave_invite(&self, guild: u64, user: u64) -> anyhow::Result<()> {
        retry("invite leave", || self.leave_invite_once(guild, user)).await
    }

    async fn leave_invite_once(&self, guild: u64, user: u64) -> anyhow::Result<()> {
        self.guild_settings(guild).await?;
        let prior: Option<(Option<i64>,)> = sqlx::query_as(
            "SELECT inviter_id FROM invite_joins WHERE guild_id=$1 AND user_id=$2 AND left_at IS NULL ORDER BY joined_at DESC LIMIT 1")
            .bind(sql_id(guild)).bind(sql_id(user)).fetch_optional(self.pool()).await?;
        let mut ids = vec![sql_id(user)];
        if let Some((Some(inviter),)) = prior {
            ids.push(inviter);
        }
        let mut tx = self.pool().begin().await?;
        lock_users(&mut tx, &ids).await?;
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended('invites:' || $1::text, 0))")
            .bind(sql_id(guild))
            .execute(&mut *tx)
            .await?;
        let active_inviter: Option<(Option<i64>,)> = sqlx::query_as(
            "SELECT inviter_id FROM invite_joins WHERE guild_id=$1 AND user_id=$2 AND left_at IS NULL ORDER BY joined_at DESC LIMIT 1")
            .bind(sql_id(guild)).bind(sql_id(user)).fetch_optional(&mut *tx).await?;
        if let Some((Some(inviter),)) = active_inviter {
            if !ids.contains(&inviter) {
                try_lock_users(&mut tx, &[inviter]).await?;
            }
            lock_reward_in_tx(&mut tx, guild, inviter).await?;
        }
        let changed = sqlx::query(
            "UPDATE invite_joins SET left_at=now() WHERE id=(SELECT id FROM invite_joins \
            WHERE guild_id=$1 AND user_id=$2 AND left_at IS NULL ORDER BY joined_at DESC LIMIT 1)",
        )
        .bind(sql_id(guild))
        .bind(sql_id(user))
        .execute(&mut *tx)
        .await?;
        if changed.rows_affected() == 0 {
            let prior: (bool,) =
                sqlx::query_as("SELECT EXISTS(SELECT 1 FROM invite_joins WHERE guild_id=$1 AND user_id=$2)")
                    .bind(sql_id(guild))
                    .bind(sql_id(user))
                    .fetch_one(&mut *tx)
                    .await?;
            if !prior.0 {
                sqlx::query("INSERT INTO invite_joins(guild_id,user_id,code,joined_at,left_at,source) VALUES($1,$2,'__leave_before_join__',now(),now(),'unknown')")
                    .bind(sql_id(guild)).bind(sql_id(user)).execute(&mut *tx).await?;
            }
        }
        tx.commit().await?;
        Ok(())
    }

    pub async fn invite_score(&self, guild: u64, user: u64) -> anyhow::Result<InviteScore> {
        let mut tx = self.pool().begin().await?;
        let score = score_in_tx(&mut tx, guild, sql_id(user), None).await?;
        tx.commit().await?;
        Ok(score)
    }

    pub async fn invite_leaders(
        &self,
        guild: u64,
        since: Option<DateTime<Utc>>,
        limit: i64,
        offset: i64,
    ) -> anyhow::Result<Vec<InviteScore>> {
        Ok(sqlx::query_as(
            "WITH invited AS (
            SELECT inviter_id AS user_id, count(*) AS regular,
              count(*) FILTER (WHERE left_at IS NOT NULL) AS left_count,
              count(*) FILTER (WHERE fake) AS fake,
              count(*) FILTER (WHERE left_at IS NULL AND NOT fake) AS eligible
            FROM invite_joins WHERE guild_id=$1 AND inviter_id IS NOT NULL
              AND ($2::timestamptz IS NULL OR joined_at >= $2) GROUP BY inviter_id
          ), candidates AS (
            SELECT user_id FROM invited UNION SELECT user_id FROM invite_bonus
            WHERE guild_id=$1 AND $2::timestamptz IS NULL
          )
          SELECT c.user_id, COALESCE(i.regular,0) AS regular, COALESCE(i.left_count,0) AS \"left\",
            COALESCE(i.fake,0) AS fake,
            CASE WHEN $2::timestamptz IS NULL THEN COALESCE(b.bonus,0) ELSE 0 END AS bonus,
            COALESCE(i.eligible,0) + CASE WHEN $2::timestamptz IS NULL THEN COALESCE(b.bonus,0) ELSE 0 END AS total
          FROM candidates c LEFT JOIN invited i ON i.user_id=c.user_id
          LEFT JOIN invite_bonus b ON b.guild_id=$1 AND b.user_id=c.user_id
          LEFT JOIN user_privacy p ON p.user_id=c.user_id
          WHERE NOT COALESCE(p.hide_leaderboard,FALSE)
            AND COALESCE(i.eligible,0) + CASE WHEN $2::timestamptz IS NULL THEN COALESCE(b.bonus,0) ELSE 0 END <> 0
          ORDER BY total DESC, c.user_id LIMIT $3 OFFSET $4",
        )
        .bind(sql_id(guild))
        .bind(since)
        .bind(limit)
        .bind(offset)
        .fetch_all(self.pool())
        .await?)
    }

    pub async fn invite_joins(&self, guild: u64, inviter: u64, limit: i64) -> anyhow::Result<Vec<InviteJoin>> {
        Ok(sqlx::query_as(
            "SELECT user_id, inviter_id, code, joined_at, left_at, fake, source \
            FROM invite_joins WHERE guild_id=$1 AND inviter_id=$2 ORDER BY joined_at DESC LIMIT $3",
        )
        .bind(sql_id(guild))
        .bind(sql_id(inviter))
        .bind(limit)
        .fetch_all(self.pool())
        .await?)
    }

    pub async fn change_invite_bonus(&self, guild: u64, user: u64, by: i32) -> anyhow::Result<i32> {
        self.guild_settings(guild).await?;
        let mut tx = self.invite_reward_guard(guild, user).await?;
        let (bonus,): (i32,) = sqlx::query_as(
            "INSERT INTO invite_bonus(guild_id,user_id,bonus) VALUES($1,$2,$3) \
            ON CONFLICT(guild_id,user_id) DO UPDATE SET bonus=invite_bonus.bonus+$3 RETURNING bonus",
        )
        .bind(sql_id(guild))
        .bind(sql_id(user))
        .bind(by)
        .fetch_one(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(bonus)
    }
}

async fn lock_reward_in_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    guild: u64,
    user: i64,
) -> anyhow::Result<()> {
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended('invite-reward:' || $1::text || ':' || $2::text, 0))")
        .bind(sql_id(guild))
        .bind(user)
        .execute(&mut **tx)
        .await?;
    Ok(())
}

async fn upsert_invite(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    guild: u64,
    invite: &GuildInvite,
    uses: i32,
) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO guild_invites(guild_id,code,inviter_id,uses,max_uses,max_age,created_at) \
        VALUES($1,$2,$3,$4,$5,$6,$7) ON CONFLICT(guild_id,code) DO UPDATE SET \
        inviter_id=$3,uses=$4,max_uses=$5,max_age=$6,seen_at=now()",
    )
    .bind(sql_id(guild))
    .bind(&invite.code)
    .bind(invite.inviter_id)
    .bind(uses)
    .bind(invite.max_uses)
    .bind(invite.max_age)
    .bind(invite.created_at)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn score_in_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    guild: u64,
    user: i64,
    since: Option<DateTime<Utc>>,
) -> anyhow::Result<InviteScore> {
    let (regular, left, fake): (i64, i64, i64) = sqlx::query_as(
        "SELECT count(*), \
        count(*) FILTER (WHERE left_at IS NOT NULL), count(*) FILTER (WHERE fake) \
        FROM invite_joins WHERE guild_id=$1 AND inviter_id=$2 AND ($3::timestamptz IS NULL OR joined_at >= $3)",
    )
    .bind(sql_id(guild))
    .bind(user)
    .bind(since)
    .fetch_one(&mut **tx)
    .await?;
    let bonus: Option<(i32,)> = sqlx::query_as("SELECT bonus FROM invite_bonus WHERE guild_id=$1 AND user_id=$2")
        .bind(sql_id(guild))
        .bind(user)
        .fetch_optional(&mut **tx)
        .await?;
    let bonus = if since.is_none() { bonus.map_or(0, |(n,)| n) } else { 0 };
    // A fake member who left is excluded once, never twice.
    let (eligible,): (i64,) = sqlx::query_as(
        "SELECT count(*) FROM invite_joins WHERE guild_id=$1 AND inviter_id=$2 \
        AND left_at IS NULL AND NOT fake AND ($3::timestamptz IS NULL OR joined_at >= $3)",
    )
    .bind(sql_id(guild))
    .bind(user)
    .bind(since)
    .fetch_one(&mut **tx)
    .await?;
    Ok(InviteScore {
        user_id: user,
        regular,
        left,
        fake,
        bonus,
        total: eligible + i64::from(bonus),
    })
}
