//! Reminders: direct messages that wait for their time, and the notifications a player opted into.

use chrono::{DateTime, Utc};

use crate::{Db, sql_id};

/// Open reminders of the kind a player writes themselves.
pub const MAX_TEXT_REMINDERS: i64 = 25;

/// The kind of a reminder the player wrote.
pub const TEXT: &str = "text";

/// The notifications a player can ask for.
pub const OPT_IN_KINDS: [&str; 5] = ["daily", "weekly", "monthly", "pet", "trip"];

/// How long a claimed reminder stays out of other pods' reach while its message goes out.
const LEASE_SECS: i64 = 120;

/// A reminder as the ticker and the list see it.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct ReminderRow {
    pub id: i64,
    pub user_id: i64,
    pub kind: String,
    pub ref_id: i64,
    pub body: String,
    pub due_at: DateTime<Utc>,
}

impl Db {
    /// The notifications the player turned on.
    pub async fn reminder_optins(&self, user: u64) -> anyhow::Result<Vec<String>> {
        let rows: Vec<(String,)> = sqlx::query_as("SELECT kind FROM reminder_optins WHERE user_id = $1 ORDER BY kind")
            .bind(sql_id(user))
            .fetch_all(self.pool())
            .await?;
        Ok(rows.into_iter().map(|(kind,)| kind).collect())
    }

    pub async fn has_optin(&self, user: u64, kind: &str) -> anyhow::Result<bool> {
        let (on,): (bool,) =
            sqlx::query_as("SELECT EXISTS (SELECT 1 FROM reminder_optins WHERE user_id = $1 AND kind = $2)")
                .bind(sql_id(user))
                .bind(kind)
                .fetch_one(self.pool())
                .await?;
        Ok(on)
    }

    /// Turns a notification on or off. Off also drops what was scheduled for it.
    pub async fn set_optin(&self, user: u64, kind: &str, on: bool) -> anyhow::Result<()> {
        anyhow::ensure!(OPT_IN_KINDS.contains(&kind), "unknown reminder kind {kind}");
        let id = sql_id(user);
        if on {
            sqlx::query("INSERT INTO reminder_optins (user_id, kind) VALUES ($1, $2) ON CONFLICT DO NOTHING")
                .bind(id)
                .bind(kind)
                .execute(self.pool())
                .await?;
        } else {
            let mut tx = self.pool().begin().await?;
            sqlx::query("DELETE FROM reminder_optins WHERE user_id = $1 AND kind = $2")
                .bind(id)
                .bind(kind)
                .execute(&mut *tx)
                .await?;
            sqlx::query("DELETE FROM reminders WHERE user_id = $1 AND kind = $2")
                .bind(id)
                .bind(kind)
                .execute(&mut *tx)
                .await?;
            tx.commit().await?;
        }
        Ok(())
    }

    /// Adds a reminder the player wrote; `None` when they already have [`MAX_TEXT_REMINDERS`] open.
    pub async fn add_text_reminder(&self, user: u64, body: &str, due_at: DateTime<Utc>) -> anyhow::Result<Option<i64>> {
        let id = sql_id(user);
        let mut tx = self.pool().begin().await?;
        // Two reminders added at once count each other.
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended('reminders:' || $1::text, 0))")
            .bind(id)
            .execute(&mut *tx)
            .await?;
        let (open,): (i64,) = sqlx::query_as("SELECT count(*) FROM reminders WHERE user_id = $1 AND kind = $2")
            .bind(id)
            .bind(TEXT)
            .fetch_one(&mut *tx)
            .await?;
        if open >= MAX_TEXT_REMINDERS {
            return Ok(None);
        }
        let (reminder,): (i64,) =
            sqlx::query_as("INSERT INTO reminders (user_id, kind, body, due_at) VALUES ($1, $2, $3, $4) RETURNING id")
                .bind(id)
                .bind(TEXT)
                .bind(body)
                .bind(due_at)
                .fetch_one(&mut *tx)
                .await?;
        tx.commit().await?;
        Ok(Some(reminder))
    }

    /// The reminders the player wrote, soonest first.
    pub async fn text_reminders(&self, user: u64) -> anyhow::Result<Vec<ReminderRow>> {
        Ok(sqlx::query_as::<_, ReminderRow>(
            "SELECT id, user_id, kind, ref_id, body, due_at FROM reminders
             WHERE user_id = $1 AND kind = $2 ORDER BY due_at, id",
        )
        .bind(sql_id(user))
        .bind(TEXT)
        .fetch_all(self.pool())
        .await?)
    }

    /// Cancels a reminder the player wrote; false when they have none with this id.
    pub async fn cancel_text_reminder(&self, user: u64, reminder: i64) -> anyhow::Result<bool> {
        let n = sqlx::query("DELETE FROM reminders WHERE id = $1 AND user_id = $2 AND kind = $3")
            .bind(reminder)
            .bind(sql_id(user))
            .bind(TEXT)
            .execute(self.pool())
            .await?
            .rows_affected();
        Ok(n == 1)
    }

    /// Schedules the message of a notification (`daily`, `pet`, ...), replacing the one already
    /// waiting for the same thing.
    pub async fn schedule_reminder(
        &self,
        user: u64,
        kind: &str,
        ref_id: i64,
        body: &str,
        due_at: DateTime<Utc>,
    ) -> anyhow::Result<()> {
        anyhow::ensure!(kind != TEXT, "text reminders are added with add_text_reminder");
        sqlx::query(
            "INSERT INTO reminders (user_id, kind, ref_id, body, due_at) VALUES ($1, $2, $3, $4, $5)
             ON CONFLICT (user_id, kind, ref_id) WHERE kind <> 'text'
             DO UPDATE SET body = EXCLUDED.body, due_at = EXCLUDED.due_at, lease_until = NULL",
        )
        .bind(sql_id(user))
        .bind(kind)
        .bind(ref_id)
        .bind(body)
        .bind(due_at)
        .execute(self.pool())
        .await?;
        Ok(())
    }

    /// Forgets what was scheduled for a notification.
    pub async fn unschedule_reminder(&self, user: u64, kind: &str, ref_id: i64) -> anyhow::Result<()> {
        sqlx::query("DELETE FROM reminders WHERE user_id = $1 AND kind = $2 AND ref_id = $3")
            .bind(sql_id(user))
            .bind(kind)
            .bind(ref_id)
            .execute(self.pool())
            .await?;
        Ok(())
    }

    /// When the notification of this kind is due, if one is scheduled.
    pub async fn scheduled_reminder(
        &self,
        user: u64,
        kind: &str,
        ref_id: i64,
    ) -> anyhow::Result<Option<DateTime<Utc>>> {
        let row: Option<(DateTime<Utc>,)> =
            sqlx::query_as("SELECT due_at FROM reminders WHERE user_id = $1 AND kind = $2 AND ref_id = $3")
                .bind(sql_id(user))
                .bind(kind)
                .bind(ref_id)
                .fetch_optional(self.pool())
                .await?;
        Ok(row.map(|(t,)| t))
    }

    /// Takes up to `limit` reminders that are due at `now` (oldest first) for this pod to send.
    /// They stay in the table, leased for two minutes: other pods skip them meanwhile, and a pod
    /// that dies before it finishes only delays them.
    pub async fn claim_due_reminders(&self, now: DateTime<Utc>, limit: i64) -> anyhow::Result<Vec<ReminderRow>> {
        let mut rows = sqlx::query_as::<_, ReminderRow>(
            "UPDATE reminders SET lease_until = now() + make_interval(secs => $3)
             WHERE id IN (
                 SELECT id FROM reminders
                 WHERE due_at <= $1 AND (lease_until IS NULL OR lease_until < now())
                 ORDER BY due_at, id LIMIT $2
                 FOR UPDATE SKIP LOCKED
             )
             RETURNING id, user_id, kind, ref_id, body, due_at",
        )
        .bind(now)
        .bind(limit)
        .bind(LEASE_SECS)
        .fetch_all(self.pool())
        .await?;
        rows.sort_by_key(|r| (r.due_at, r.id));
        Ok(rows)
    }

    /// Removes a reminder that was sent (or given up on). A reminder that was moved to another
    /// time in the meantime, because the thing it is about happened again, stays.
    pub async fn finish_reminder(&self, reminder: &ReminderRow) -> anyhow::Result<()> {
        sqlx::query("DELETE FROM reminders WHERE id = $1 AND due_at = $2")
            .bind(reminder.id)
            .bind(reminder.due_at)
            .execute(self.pool())
            .await?;
        Ok(())
    }

    /// Moves a claimed reminder to a later time and gives it back to every pod.
    pub async fn postpone_reminder(&self, reminder: &ReminderRow, due_at: DateTime<Utc>) -> anyhow::Result<()> {
        sqlx::query("UPDATE reminders SET due_at = $3, lease_until = NULL WHERE id = $1 AND due_at = $2")
            .bind(reminder.id)
            .bind(reminder.due_at)
            .bind(due_at)
            .execute(self.pool())
            .await?;
        Ok(())
    }
}
