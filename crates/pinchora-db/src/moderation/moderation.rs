//! Moderation: numbered cases, warn thresholds, the mod-log settings and the staff action log.

use chrono::{DateTime, Duration, Utc};
use pinchora_core::moderation::moderation::CaseKind;
use pinchora_core::moderation::moderation::MAX_THRESHOLDS;
use pinchora_core::moderation::moderation::Punishment;
use pinchora_core::moderation::moderation::Threshold;
use sqlx::{Postgres, QueryBuilder};

use crate::{Db, sql_id};

/// How long the ticker holds a temporary ban it is lifting before another pod may try.
const LIFT_LEASE_SECS: f64 = 120.0;

/// A server's moderation settings.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct ModSettings {
    /// The mod-log channel; `None` sends cases to the server log's moderation channel.
    pub log_channel_id: Option<i64>,
    /// Whether members are told by direct message.
    pub dm_members: bool,
}

impl Default for ModSettings {
    fn default() -> Self {
        Self {
            log_channel_id: None,
            dm_members: true,
        }
    }
}

/// One case.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct CaseRow {
    pub guild_id: i64,
    pub number: i32,
    pub kind: String,
    pub target_id: i64,
    pub target_name: String,
    pub moderator_id: i64,
    pub moderator_name: String,
    pub reason: Option<String>,
    pub duration_secs: Option<i64>,
    pub expires_at: Option<DateTime<Utc>>,
    pub details: Option<String>,
    pub active: bool,
    pub dm_sent: Option<bool>,
    pub log_channel_id: Option<i64>,
    pub log_message_id: Option<i64>,
    pub created_at: DateTime<Utc>,
    pub updated_at: Option<DateTime<Utc>>,
}

impl CaseRow {
    pub fn kind(&self) -> Option<CaseKind> {
        CaseKind::parse(&self.kind)
    }

    pub fn duration(&self) -> Option<Duration> {
        self.duration_secs.map(Duration::seconds)
    }
}

macro_rules! case_columns {
    () => {
        "guild_id, number, kind, target_id, target_name, moderator_id, moderator_name, reason, duration_secs, \
         expires_at, details, active, dm_sent, log_channel_id, log_message_id, created_at, updated_at"
    };
}

#[path = "reversal.rs"]
mod reversal;
use reversal::open_case_in;

fn push_case_filters(sql: &mut QueryBuilder<Postgres>, filter: &CaseFilter, include_kind: bool) {
    if let Some(target) = filter.target {
        sql.push(" AND target_id = ").push_bind(sql_id(target));
    }
    if let Some(moderator) = filter.moderator {
        sql.push(" AND moderator_id = ").push_bind(sql_id(moderator));
    }
    if let Some(kind) = filter.kind.filter(|_| include_kind) {
        sql.push(" AND kind = ").push_bind(kind.key());
    }
    if filter.active_only {
        sql.push(" AND active AND kind IN ('ban', 'warn', 'timeout')");
    }
    if let Some(query) = filter.query.as_deref().map(str::trim).filter(|q| !q.is_empty()) {
        let like = format!("%{}%", escape_like(query));
        sql.push(" AND (target_name ILIKE ")
            .push_bind(like.clone())
            .push(" OR moderator_name ILIKE ")
            .push_bind(like.clone())
            .push(" OR reason ILIKE ")
            .push_bind(like.clone())
            .push(" OR details ILIKE ")
            .push_bind(like)
            .push(" OR target_id::TEXT = ")
            .push_bind(query.to_owned())
            .push(")");
    }
}

/// A case to open.
#[derive(Debug, Clone)]
pub struct NewCase<'a> {
    pub guild: u64,
    pub kind: CaseKind,
    pub target: u64,
    pub target_name: &'a str,
    pub moderator: u64,
    pub moderator_name: &'a str,
    pub reason: Option<&'a str>,
    pub duration: Option<Duration>,
    pub expires_at: Option<DateTime<Utc>>,
    pub details: Option<&'a str>,
}

/// A case just opened, and how many active warns its member has now (for a warn's thresholds).
#[derive(Debug, Clone)]
pub struct OpenedCase {
    pub case: CaseRow,
    pub active_warns: i64,
}

/// Which cases a list shows.
#[derive(Debug, Clone, Default)]
pub struct CaseFilter {
    pub target: Option<u64>,
    pub moderator: Option<u64>,
    pub kind: Option<CaseKind>,
    pub active_only: bool,
    /// Words to find in the names, the reason or the details.
    pub query: Option<String>,
    /// Cases numbered below this one (paging back).
    pub before: Option<i32>,
    pub limit: i64,
}

/// One entry of the staff action log: a moderation command, a change Discord's audit log reports, or
/// a dashboard change.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct StaffActionRow {
    /// `c:` and a number for commands and Discord's audit log, `w:` and a number for the dashboard.
    pub key: String,
    pub actor_id: i64,
    pub actor_name: String,
    /// `command`, `discord` or `web`.
    pub source: String,
    pub action: String,
    pub target_id: Option<i64>,
    pub target_name: Option<String>,
    pub case_number: Option<i32>,
    pub reason: Option<String>,
    pub details: Option<String>,
    pub created_at: DateTime<Utc>,
}

/// An action to put in the staff log.
#[derive(Debug, Clone)]
pub struct NewStaffAction<'a> {
    pub guild: u64,
    pub actor: u64,
    pub actor_name: &'a str,
    /// `command`, `web` or `discord`.
    pub source: &'a str,
    pub action: &'a str,
    pub target: Option<u64>,
    pub target_name: Option<&'a str>,
    pub case_number: Option<i32>,
    pub reason: Option<&'a str>,
    pub details: Option<&'a str>,
}

/// Which staff actions a list shows.
#[derive(Debug, Clone, Default)]
pub struct StaffFilter {
    pub actor: Option<u64>,
    /// `command`, `discord` or `web`.
    pub source: Option<String>,
    pub query: Option<String>,
    pub before: Option<DateTime<Utc>>,
    pub limit: i64,
}

/// A threshold as stored.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
struct ThresholdRow {
    warns: i32,
    action: String,
    duration_secs: Option<i64>,
}

/// A threshold that could not be added: the ladder is full.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LadderFull;

impl Db {
    pub async fn mod_settings(&self, guild: u64) -> anyhow::Result<ModSettings> {
        Ok(
            sqlx::query_as::<_, ModSettings>("SELECT log_channel_id, dm_members FROM mod_settings WHERE guild_id = $1")
                .bind(sql_id(guild))
                .fetch_optional(self.pool())
                .await?
                .unwrap_or_default(),
        )
    }

    /// Changes the mod-log channel (`Some(None)` clears it) or the direct messages; `None` leaves a
    /// setting alone.
    pub async fn set_mod_settings(
        &self,
        guild: u64,
        log_channel: Option<Option<u64>>,
        dm_members: Option<bool>,
    ) -> anyhow::Result<ModSettings> {
        Ok(sqlx::query_as::<_, ModSettings>(
            "INSERT INTO mod_settings (guild_id, log_channel_id, dm_members) VALUES ($1, $3, COALESCE($4, TRUE))
             ON CONFLICT (guild_id) DO UPDATE SET
                 log_channel_id = CASE WHEN $2 THEN EXCLUDED.log_channel_id ELSE mod_settings.log_channel_id END,
                 dm_members = COALESCE($4, mod_settings.dm_members)
             RETURNING log_channel_id, dm_members",
        )
        .bind(sql_id(guild))
        .bind(log_channel.is_some())
        .bind(log_channel.flatten().map(sql_id))
        .bind(dm_members)
        .fetch_one(self.pool())
        .await?)
    }

    /// The warn ladder, fewest warns first.
    pub async fn warn_thresholds(&self, guild: u64) -> anyhow::Result<Vec<Threshold>> {
        let rows = sqlx::query_as::<_, ThresholdRow>(
            "SELECT warns, action, duration_secs FROM mod_warn_thresholds WHERE guild_id = $1 ORDER BY warns",
        )
        .bind(sql_id(guild))
        .fetch_all(self.pool())
        .await?;
        Ok(rows
            .into_iter()
            .filter_map(|r| {
                Some(Threshold {
                    warns: r.warns,
                    action: Punishment::parse(&r.action)?,
                    duration: r.duration_secs.map(Duration::seconds),
                })
            })
            .collect())
    }

    /// Adds a rung, or changes the one at the same warn count. Refused when the ladder is full.
    pub async fn put_warn_threshold(
        &self,
        guild: u64,
        threshold: Threshold,
    ) -> anyhow::Result<Result<Vec<Threshold>, LadderFull>> {
        let mut tx = self.pool().begin().await?;
        // One server's ladder is written by one transaction at a time, so the count below holds.
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended('warn-ladder:' || $1, 0))")
            .bind(sql_id(guild))
            .execute(&mut *tx)
            .await?;
        let (others,): (i64,) =
            sqlx::query_as("SELECT count(*) FROM mod_warn_thresholds WHERE guild_id = $1 AND warns <> $2")
                .bind(sql_id(guild))
                .bind(threshold.warns)
                .fetch_one(&mut *tx)
                .await?;
        if others as usize >= MAX_THRESHOLDS {
            return Ok(Err(LadderFull));
        }
        sqlx::query(
            "INSERT INTO mod_warn_thresholds (guild_id, warns, action, duration_secs) VALUES ($1, $2, $3, $4)
             ON CONFLICT (guild_id, warns) DO UPDATE SET action = EXCLUDED.action, duration_secs = EXCLUDED.duration_secs",
        )
        .bind(sql_id(guild))
        .bind(threshold.warns)
        .bind(threshold.action.key())
        .bind(threshold.duration.map(|d| d.num_seconds()))
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(Ok(self.warn_thresholds(guild).await?))
    }

    /// Replaces the whole ladder (the dashboard saves it at once). The caller checked its length.
    pub async fn replace_warn_thresholds(&self, guild: u64, ladder: &[Threshold]) -> anyhow::Result<Vec<Threshold>> {
        let mut tx = self.pool().begin().await?;
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended('warn-ladder:' || $1, 0))")
            .bind(sql_id(guild))
            .execute(&mut *tx)
            .await?;
        sqlx::query("DELETE FROM mod_warn_thresholds WHERE guild_id = $1")
            .bind(sql_id(guild))
            .execute(&mut *tx)
            .await?;
        for rung in ladder {
            sqlx::query(
                "INSERT INTO mod_warn_thresholds (guild_id, warns, action, duration_secs) VALUES ($1, $2, $3, $4)",
            )
            .bind(sql_id(guild))
            .bind(rung.warns)
            .bind(rung.action.key())
            .bind(rung.duration.map(|d| d.num_seconds()))
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        self.warn_thresholds(guild).await
    }

    /// Takes the rung at `warns` off; whether there was one.
    pub async fn delete_warn_threshold(&self, guild: u64, warns: i32) -> anyhow::Result<bool> {
        Ok(
            sqlx::query("DELETE FROM mod_warn_thresholds WHERE guild_id = $1 AND warns = $2")
                .bind(sql_id(guild))
                .bind(warns)
                .execute(self.pool())
                .await?
                .rows_affected()
                > 0,
        )
    }

    /// Opens a case with the server's next number and writes it to the staff log, in one transaction.
    /// A ban or timeout replaces the member's earlier one (its end no longer counts), an unban or a
    /// removed timeout ends them.
    pub async fn open_case(&self, new: &NewCase<'_>) -> anyhow::Result<OpenedCase> {
        self.open_case_from(new, "command").await
    }

    pub async fn open_case_from(&self, new: &NewCase<'_>, source: &str) -> anyhow::Result<OpenedCase> {
        let mut tx = self.pool().begin().await?;
        let opened = open_case_in(&mut tx, new, source).await?;
        tx.commit().await?;
        Ok(opened)
    }

    /// Takes a warn back: the warn stops counting and an `unwarn` case records who did it. `None`
    /// when `number` is no warn of this server that still counts.
    pub async fn remove_warn(&self, number: i32, new: &NewCase<'_>) -> anyhow::Result<Option<(CaseRow, OpenedCase)>> {
        self.remove_warn_from(number, new, "command").await
    }

    pub async fn remove_warn_from(
        &self,
        number: i32,
        new: &NewCase<'_>,
        source: &str,
    ) -> anyhow::Result<Option<(CaseRow, OpenedCase)>> {
        let mut tx = self.pool().begin().await?;
        let warn = sqlx::query_as::<_, CaseRow>(concat!(
            "UPDATE mod_cases SET active = FALSE, updated_at = now()
             WHERE guild_id = $1 AND number = $2 AND kind = 'warn' AND active RETURNING ",
            case_columns!()
        ))
        .bind(sql_id(new.guild))
        .bind(number)
        .fetch_optional(&mut *tx)
        .await?;
        let Some(warn) = warn else {
            return Ok(None);
        };
        let unwarn = NewCase {
            target: warn.target_id as u64,
            target_name: &warn.target_name,
            ..new.clone()
        };
        let opened = open_case_in(&mut tx, &unwarn, source).await?;
        tx.commit().await?;
        Ok(Some((warn, opened)))
    }

    pub async fn case(&self, guild: u64, number: i32) -> anyhow::Result<Option<CaseRow>> {
        Ok(sqlx::query_as::<_, CaseRow>(concat!(
            "SELECT ",
            case_columns!(),
            " FROM mod_cases WHERE guild_id = $1 AND number = $2"
        ))
        .bind(sql_id(guild))
        .bind(number)
        .fetch_optional(self.pool())
        .await?)
    }

    /// Remembers where the case was posted, so a new reason can edit the post.
    pub async fn set_case_post(&self, guild: u64, number: i32, channel: u64, message: u64) -> anyhow::Result<()> {
        sqlx::query(
            "UPDATE mod_cases SET log_channel_id = $3, log_message_id = $4 WHERE guild_id = $1 AND number = $2",
        )
        .bind(sql_id(guild))
        .bind(number)
        .bind(sql_id(channel))
        .bind(sql_id(message))
        .execute(self.pool())
        .await?;
        Ok(())
    }

    /// Notes whether the member got the direct message.
    pub async fn set_case_dm(&self, guild: u64, number: i32, sent: bool) -> anyhow::Result<()> {
        sqlx::query("UPDATE mod_cases SET dm_sent = $3 WHERE guild_id = $1 AND number = $2")
            .bind(sql_id(guild))
            .bind(number)
            .bind(sent)
            .execute(self.pool())
            .await?;
        Ok(())
    }

    /// Gives a case a new reason, and notes in the staff log who did. `None` when there is no such case.
    pub async fn set_case_reason(
        &self,
        guild: u64,
        number: i32,
        reason: Option<&str>,
        editor: u64,
        editor_name: &str,
        source: &str,
    ) -> anyhow::Result<Option<CaseRow>> {
        let mut tx = self.pool().begin().await?;
        let case = sqlx::query_as::<_, CaseRow>(concat!(
            "UPDATE mod_cases SET reason = $3, updated_at = now() WHERE guild_id = $1 AND number = $2 RETURNING ",
            case_columns!()
        ))
        .bind(sql_id(guild))
        .bind(number)
        .bind(reason)
        .fetch_optional(&mut *tx)
        .await?;
        if let Some(case) = &case {
            record_staff_in(
                &mut tx,
                &NewStaffAction {
                    guild,
                    actor: editor,
                    actor_name: editor_name,
                    source,
                    action: "reason",
                    target: Some(case.target_id as u64),
                    target_name: Some(&case.target_name),
                    case_number: Some(number),
                    reason,
                    details: None,
                },
            )
            .await?;
        }
        tx.commit().await?;
        Ok(case)
    }

    /// A member's warns that still count, newest first.
    pub async fn active_warns(&self, guild: u64, target: u64) -> anyhow::Result<Vec<CaseRow>> {
        Ok(sqlx::query_as::<_, CaseRow>(concat!(
            "SELECT ",
            case_columns!(),
            " FROM mod_cases WHERE guild_id = $1 AND target_id = $2 AND kind = 'warn' AND active ORDER BY number DESC"
        ))
        .bind(sql_id(guild))
        .bind(sql_id(target))
        .fetch_all(self.pool())
        .await?)
    }

    /// A page of cases, newest first.
    pub async fn cases(&self, guild: u64, filter: &CaseFilter) -> anyhow::Result<Vec<CaseRow>> {
        let mut sql: QueryBuilder<Postgres> =
            QueryBuilder::new(concat!("SELECT ", case_columns!(), " FROM mod_cases WHERE guild_id = "));
        sql.push_bind(sql_id(guild));
        push_case_filters(&mut sql, filter, true);
        if let Some(before) = filter.before {
            sql.push(" AND number < ").push_bind(before);
        }
        sql.push(" ORDER BY number DESC LIMIT ")
            .push_bind(filter.limit.clamp(1, 100));
        Ok(sql.build_query_as::<CaseRow>().fetch_all(self.pool()).await?)
    }

    /// Counts for every kind under the list's search, member and active filters.
    pub async fn case_kind_counts(&self, guild: u64, filter: &CaseFilter) -> anyhow::Result<Vec<(String, i64)>> {
        let mut sql: QueryBuilder<Postgres> =
            QueryBuilder::new("SELECT kind, count(*) AS count FROM mod_cases WHERE guild_id = ");
        sql.push_bind(sql_id(guild));
        push_case_filters(&mut sql, filter, false);
        sql.push(" GROUP BY kind");
        Ok(sql.build_query_as::<(String, i64)>().fetch_all(self.pool()).await?)
    }

    /// Serializes Discord ban changes for a member across pods. Hold through the HTTP call and case write.
    pub async fn lock_ban(&self, guild: u64, target: u64) -> anyhow::Result<sqlx::Transaction<'_, Postgres>> {
        let mut tx = self.pool().begin().await?;
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
            .bind(format!("moderation-ban:{guild}:{target}"))
            .execute(&mut *tx)
            .await?;
        Ok(tx)
    }

    /// Takes temporary bans whose time is up, leasing each for two minutes so no other pod lifts it.
    pub async fn claim_expired_bans(&self, now: DateTime<Utc>, limit: i64) -> anyhow::Result<Vec<CaseRow>> {
        Ok(sqlx::query_as::<_, CaseRow>(concat!(
            "UPDATE mod_cases SET expire_lease = now() + make_interval(secs => $3)
             WHERE (guild_id, number) IN (
                 SELECT guild_id, number FROM mod_cases
                 WHERE kind = 'ban' AND active AND expires_at IS NOT NULL AND expires_at <= $1
                   AND (expire_lease IS NULL OR expire_lease < now())
                 ORDER BY expires_at LIMIT $2
                 FOR UPDATE SKIP LOCKED
             )
             RETURNING ",
            case_columns!()
        ))
        .bind(now)
        .bind(limit)
        .bind(LIFT_LEASE_SECS)
        .fetch_all(self.pool())
        .await?)
    }

    /// Ends a temporary ban the ticker is about to lift, right before it asks Discord: `false` when the
    /// ban no longer stands (lifted by someone, or replaced by a newer ban), so it is left alone.
    pub async fn close_ban(&self, ban: &CaseRow) -> anyhow::Result<bool> {
        let closed = sqlx::query(
            "UPDATE mod_cases SET active = FALSE, updated_at = now()
             WHERE guild_id = $1 AND number = $2 AND created_at = $3 AND kind = 'ban' AND active
               AND NOT EXISTS (SELECT 1 FROM mod_cases newer
                   WHERE newer.guild_id = $1 AND newer.target_id = $4
                     AND newer.kind = 'ban' AND newer.active AND newer.number > $2)
             RETURNING number",
        )
        .bind(ban.guild_id)
        .bind(ban.number)
        .bind(ban.created_at)
        .bind(ban.target_id)
        .fetch_optional(self.pool())
        .await?;
        Ok(closed.is_some())
    }

    /// Puts back a temporary ban Discord would not lift, to be tried again in an hour: unless a newer
    /// ban or unban of the member came since, which decides instead.
    pub async fn postpone_lift(&self, ban: &CaseRow) -> anyhow::Result<()> {
        sqlx::query(
            "UPDATE mod_cases m SET active = TRUE, updated_at = now(), expire_lease = now() + interval '1 hour'
             WHERE m.guild_id = $1 AND m.number = $2 AND m.kind = 'ban' AND m.created_at = $3
               AND NOT EXISTS (
                   SELECT 1 FROM mod_cases n
                   WHERE n.guild_id = m.guild_id AND n.target_id = m.target_id AND n.number > m.number
                     AND n.kind IN ('ban', 'unban')
               )",
        )
        .bind(ban.guild_id)
        .bind(ban.number)
        .bind(ban.created_at)
        .execute(self.pool())
        .await?;
        Ok(())
    }

    /// Remembers what @everyone's overwrite allowed and denied of the locked permissions before the
    /// lock of case `number`, for the unlock to put back.
    pub async fn set_lock_state(&self, guild: u64, number: i32, allowed: u64, denied: u64) -> anyhow::Result<()> {
        sqlx::query("UPDATE mod_cases SET lock_allow = $3, lock_deny = $4 WHERE guild_id = $1 AND number = $2")
            .bind(sql_id(guild))
            .bind(number)
            .bind(allowed as i64)
            .bind(denied as i64)
            .execute(self.pool())
            .await?;
        Ok(())
    }

    /// What the channel's last lock took, as (allowed, denied) before it, when the channel's last lock
    /// or unlock through Pinchora is a lock that remembered it.
    pub async fn lock_state(&self, guild: u64, channel: u64) -> anyhow::Result<Option<(u64, u64)>> {
        let row: Option<(String, Option<i64>, Option<i64>)> = sqlx::query_as(
            "SELECT kind, lock_allow, lock_deny FROM mod_cases
             WHERE guild_id = $1 AND target_id = $2 AND kind IN ('lock', 'unlock')
             ORDER BY number DESC LIMIT 1",
        )
        .bind(sql_id(guild))
        .bind(sql_id(channel))
        .fetch_optional(self.pool())
        .await?;
        Ok(match row {
            Some((kind, Some(allowed), Some(denied))) if kind == "lock" => Some((allowed as u64, denied as u64)),
            _ => None,
        })
    }

    /// Ends the member's bans opened strictly before the audit entry's timestamp: someone
    /// lifted their ban on Discord, so none of them stands any more and the ticker must not lift a ban
    /// made since. How many ended.
    pub async fn end_bans(&self, guild: u64, target: u64, until: DateTime<Utc>) -> anyhow::Result<u64> {
        let ended = sqlx::query(
            "WITH candidates AS (
                 SELECT guild_id, number, created_at FROM mod_cases
                 WHERE guild_id = $1 AND target_id = $2 AND kind = 'ban' AND active AND created_at < $3
             )
             UPDATE mod_cases c SET active = FALSE, updated_at = now()
             FROM candidates b
             WHERE c.guild_id = b.guild_id AND c.number = b.number AND c.created_at = b.created_at AND c.active
             RETURNING c.number",
        )
        .bind(sql_id(guild))
        .bind(sql_id(target))
        .bind(until)
        .fetch_all(self.pool())
        .await?;
        Ok(ended.len() as u64)
    }

    /// Puts an action in the staff log.
    pub async fn record_staff_action(&self, action: &NewStaffAction<'_>) -> anyhow::Result<()> {
        let mut tx = self.pool().begin().await?;
        record_staff_in(&mut tx, action).await?;
        tx.commit().await?;
        Ok(())
    }

    /// A page of the staff log, newest first: moderation commands, Discord's audit log and the
    /// dashboard's changes together.
    pub async fn staff_actions(&self, guild: u64, filter: &StaffFilter) -> anyhow::Result<Vec<StaffActionRow>> {
        let mut sql: QueryBuilder<Postgres> = QueryBuilder::new(
            "SELECT * FROM (
                 SELECT 'c:' || id AS key, actor_id, actor_name, source, action, target_id, target_name, case_number,
                        reason, details, created_at
                 FROM staff_actions WHERE guild_id = ",
        );
        sql.push_bind(sql_id(guild));
        sql.push(
            " UNION ALL
                 SELECT 'w:' || id, actor_id, actor_name, 'web', action, NULL::BIGINT, target, NULL::INTEGER,
                        NULL::TEXT, after::text, created_at
                 FROM dashboard_audit WHERE source = 'web' AND guild_id = ",
        );
        sql.push_bind(sql_id(guild));
        sql.push(") log WHERE TRUE");
        if let Some(actor) = filter.actor {
            sql.push(" AND actor_id = ").push_bind(sql_id(actor));
        }
        if let Some(source) = &filter.source {
            sql.push(" AND source = ").push_bind(source.clone());
        }
        if let Some(query) = filter.query.as_deref().map(str::trim).filter(|q| !q.is_empty()) {
            let like = format!("%{}%", escape_like(query));
            sql.push(" AND (actor_name ILIKE ")
                .push_bind(like.clone())
                .push(" OR target_name ILIKE ")
                .push_bind(like.clone())
                .push(" OR action ILIKE ")
                .push_bind(like.clone())
                .push(" OR reason ILIKE ")
                .push_bind(like)
                .push(" OR actor_id::TEXT = ")
                .push_bind(query.to_owned())
                .push(" OR target_id::TEXT = ")
                .push_bind(query.to_owned())
                .push(")");
        }
        if let Some(before) = filter.before {
            sql.push(" AND created_at < ").push_bind(before);
        }
        sql.push(" ORDER BY created_at DESC, key DESC LIMIT ")
            .push_bind(filter.limit.clamp(1, 100));
        Ok(sql.build_query_as::<StaffActionRow>().fetch_all(self.pool()).await?)
    }

    /// Forgets staff actions made before `cutoff`; returns how many went.
    pub async fn forget_staff_actions_before(&self, cutoff: DateTime<Utc>) -> anyhow::Result<u64> {
        Ok(sqlx::query("DELETE FROM staff_actions WHERE created_at < $1")
            .bind(cutoff)
            .execute(self.pool())
            .await?
            .rows_affected())
    }
}

/// Opens a case inside `tx`. The counter row taken first is held to the end of the transaction, so
/// cases of one server are numbered one after the other, and the warn count read after it sees every
/// warn opened before.
async fn record_staff_in(tx: &mut sqlx::Transaction<'_, Postgres>, a: &NewStaffAction<'_>) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO staff_actions (guild_id, actor_id, actor_name, source, action, target_id, target_name,
                                    case_number, reason, details)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)",
    )
    .bind(sql_id(a.guild))
    .bind(sql_id(a.actor))
    .bind(a.actor_name)
    .bind(a.source)
    .bind(a.action)
    .bind(a.target.map(sql_id))
    .bind(a.target_name)
    .bind(a.case_number)
    .bind(a.reason)
    .bind(a.details)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

/// `text` for a LIKE pattern, its wildcards taken literally.
fn escape_like(text: &str) -> String {
    text.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_")
}

#[cfg(test)]
mod tests {
    use crate::moderation::moderation::*;

    #[test]
    fn wildcards_are_taken_literally() {
        assert_eq!(escape_like("50%_off\\"), "50\\%\\_off\\\\");
    }
}
