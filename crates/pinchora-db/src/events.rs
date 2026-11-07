//! Events and RSVP writes. A locked event serializes capacity checks, promotion and recurrence.

use chrono::{DateTime, Utc};
use pinchora_core::events::{Recurrence, RsvpOption, check_options, next_occurrence};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

use crate::{
    Db,
    locks::{lock_users, retry},
    sql_id,
};

#[derive(Clone, Debug, Serialize, Deserialize, FromRow)]
pub struct EventRow {
    pub id: i64,
    pub guild_id: i64,
    pub title: String,
    pub description: String,
    pub starts_at: DateTime<Utc>,
    pub ends_at: DateTime<Utc>,
    pub tz: String,
    pub location: String,
    pub channel_id: Option<i64>,
    pub message_id: Option<i64>,
    pub host_id: Option<i64>,
    pub options: serde_json::Value,
    pub required_role_id: Option<i64>,
    pub reminders: Vec<i32>,
    pub discord_event_id: Option<i64>,
    pub recurrence: Option<serde_json::Value>,
    pub parent_id: Option<i64>,
    pub status: String,
}

impl EventRow {
    pub fn options(&self) -> Vec<RsvpOption> {
        serde_json::from_value(self.options.clone()).unwrap_or_default()
    }
    pub fn recurrence(&self) -> Option<Recurrence> {
        self.recurrence.clone().and_then(|v| serde_json::from_value(v).ok())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EventDraft {
    pub title: String,
    pub description: String,
    pub starts_at: DateTime<Utc>,
    pub ends_at: DateTime<Utc>,
    pub tz: String,
    pub location: String,
    pub channel_id: Option<i64>,
    pub host_id: Option<i64>,
    pub options: Vec<RsvpOption>,
    pub required_role_id: Option<i64>,
    pub reminders: Vec<i32>,
    pub recurrence: Option<Recurrence>,
}

impl EventDraft {
    pub fn validate(&self) -> anyhow::Result<()> {
        anyhow::ensure!(
            !self.title.trim().is_empty() && self.title.chars().count() <= 100,
            "title must be 1–100 characters"
        );
        anyhow::ensure!(
            self.description.chars().count() <= 1000,
            "description must be at most 1000 characters"
        );
        anyhow::ensure!(self.ends_at > self.starts_at, "event must end after it starts");
        anyhow::ensure!(
            self.location.chars().count() <= 100,
            "location must be at most 100 characters"
        );
        anyhow::ensure!(self.tz.parse::<chrono_tz::Tz>().is_ok(), "unknown time zone");
        anyhow::ensure!(check_options(&self.options), "invalid RSVP options");
        anyhow::ensure!(
            self.reminders.iter().all(|m| *m == 15 || *m == 60),
            "reminders must be 15 or 60 minutes"
        );
        Ok(())
    }
}

#[derive(Clone, Debug, FromRow, Serialize)]
pub struct EventRsvp {
    pub event_id: i64,
    pub user_id: i64,
    pub option: String,
    pub waitlisted: bool,
    pub responded_at: DateTime<Utc>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RsvpResult {
    Confirmed,
    Waitlisted,
    Removed,
    Closed,
    Invalid,
}

const COLUMNS: &str = "id, guild_id, title, description, starts_at, ends_at, tz, location, channel_id, message_id, host_id, options, required_role_id, reminders, discord_event_id, recurrence, parent_id, status";

impl Db {
    pub async fn event_timezone(&self, guild: u64) -> anyhow::Result<String> {
        let tz: Option<(String,)> = sqlx::query_as("SELECT timezone FROM event_settings WHERE guild_id=$1")
            .bind(sql_id(guild))
            .fetch_optional(self.pool())
            .await?;
        if let Some((tz,)) = tz {
            return Ok(tz);
        }
        let (birthdays_available,): (bool,) = sqlx::query_as("SELECT to_regclass('birthday_settings') IS NOT NULL")
            .fetch_one(self.pool())
            .await?;
        if birthdays_available {
            let birthday: Option<(Option<String>,)> =
                sqlx::query_as("SELECT server_tz FROM birthday_settings WHERE guild_id=$1")
                    .bind(sql_id(guild))
                    .fetch_optional(self.pool())
                    .await?;
            if let Some(tz) = birthday.and_then(|(tz,)| tz)
                && tz.parse::<chrono_tz::Tz>().is_ok()
            {
                return Ok(tz);
            }
        }
        Ok("UTC".into())
    }
    /// Uses a member's `/birthday` time zone when that package is installed; otherwise the server's.
    pub async fn event_timezone_for(&self, guild: u64, user: u64) -> anyhow::Result<String> {
        let (birthdays_available,): (bool,) = sqlx::query_as("SELECT to_regclass('birthdays') IS NOT NULL")
            .fetch_one(self.pool())
            .await?;
        if birthdays_available {
            let birthday: Option<(Option<String>,)> = sqlx::query_as("SELECT tz FROM birthdays WHERE user_id=$1")
                .bind(sql_id(user))
                .fetch_optional(self.pool())
                .await?;
            if let Some(tz) = birthday.and_then(|(tz,)| tz)
                && tz.parse::<chrono_tz::Tz>().is_ok()
            {
                return Ok(tz);
            }
        }
        self.event_timezone(guild).await
    }
    pub async fn set_event_timezone(&self, guild: u64, tz: &str) -> anyhow::Result<()> {
        anyhow::ensure!(tz.parse::<chrono_tz::Tz>().is_ok(), "unknown time zone");
        sqlx::query("INSERT INTO event_settings (guild_id, timezone) VALUES ($1,$2) ON CONFLICT (guild_id) DO UPDATE SET timezone=$2")
            .bind(sql_id(guild)).bind(tz).execute(self.pool()).await?;
        Ok(())
    }
    pub async fn create_event(&self, guild: u64, draft: &EventDraft) -> anyhow::Result<EventRow> {
        draft.validate()?;
        let sql = format!(
            "INSERT INTO events (guild_id,title,description,starts_at,ends_at,tz,location,channel_id,host_id,options,required_role_id,reminders,recurrence) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13) RETURNING {COLUMNS}"
        );
        Ok(sqlx::query_as::<_, EventRow>(sqlx::AssertSqlSafe(sql))
            .bind(sql_id(guild))
            .bind(draft.title.trim())
            .bind(&draft.description)
            .bind(draft.starts_at)
            .bind(draft.ends_at)
            .bind(&draft.tz)
            .bind(&draft.location)
            .bind(draft.channel_id)
            .bind(draft.host_id)
            .bind(serde_json::to_value(&draft.options)?)
            .bind(draft.required_role_id)
            .bind(&draft.reminders)
            .bind(draft.recurrence.as_ref().map(serde_json::to_value).transpose()?)
            .fetch_one(self.pool())
            .await?)
    }
    pub async fn event(&self, guild: u64, id: i64) -> anyhow::Result<Option<EventRow>> {
        let sql = format!("SELECT {COLUMNS} FROM events WHERE guild_id=$1 AND id=$2");
        Ok(sqlx::query_as(sqlx::AssertSqlSafe(sql))
            .bind(sql_id(guild))
            .bind(id)
            .fetch_optional(self.pool())
            .await?)
    }
    pub async fn events(&self, guild: u64, include_past: bool) -> anyhow::Result<Vec<EventRow>> {
        let sql = format!(
            "SELECT {COLUMNS} FROM events WHERE guild_id=$1 AND status='upcoming' AND ends_at>now() ORDER BY starts_at ASC LIMIT 100"
        );
        let mut rows: Vec<EventRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
            .bind(sql_id(guild))
            .fetch_all(self.pool())
            .await?;
        if include_past {
            let sql = format!(
                "SELECT {COLUMNS} FROM events WHERE guild_id=$1 AND (status<>'upcoming' OR ends_at<=now()) ORDER BY starts_at DESC LIMIT 10"
            );
            rows.extend(
                sqlx::query_as(sqlx::AssertSqlSafe(sql))
                    .bind(sql_id(guild))
                    .fetch_all(self.pool())
                    .await?,
            );
        }
        Ok(rows)
    }
    pub async fn update_event(&self, guild: u64, id: i64, draft: &EventDraft) -> anyhow::Result<Option<EventRow>> {
        draft.validate()?;
        let sql = format!(
            "UPDATE events SET title=$3,description=$4,starts_at=$5,ends_at=$6,tz=$7,location=$8,channel_id=$9,message_id=CASE WHEN channel_id IS DISTINCT FROM $9 THEN NULL ELSE message_id END,options=$10,required_role_id=$11,reminders=$12,recurrence=$13 WHERE guild_id=$1 AND id=$2 AND status='upcoming' RETURNING {COLUMNS}"
        );
        Ok(sqlx::query_as(sqlx::AssertSqlSafe(sql))
            .bind(sql_id(guild))
            .bind(id)
            .bind(draft.title.trim())
            .bind(&draft.description)
            .bind(draft.starts_at)
            .bind(draft.ends_at)
            .bind(&draft.tz)
            .bind(&draft.location)
            .bind(draft.channel_id)
            .bind(serde_json::to_value(&draft.options)?)
            .bind(draft.required_role_id)
            .bind(&draft.reminders)
            .bind(draft.recurrence.as_ref().map(serde_json::to_value).transpose()?)
            .fetch_optional(self.pool())
            .await?)
    }
    pub async fn cancel_event(&self, guild: u64, id: i64) -> anyhow::Result<Option<EventRow>> {
        let sql = format!(
            "UPDATE events SET status='cancelled' WHERE guild_id=$1 AND id=$2 AND status='upcoming' RETURNING {COLUMNS}"
        );
        let row: Option<EventRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
            .bind(sql_id(guild))
            .bind(id)
            .fetch_optional(self.pool())
            .await?;
        if row.is_some() {
            sqlx::query("DELETE FROM reminders WHERE kind IN ('event_60','event_15') AND ref_id=$1")
                .bind(id)
                .execute(self.pool())
                .await?;
        }
        Ok(row)
    }
    pub async fn set_event_message(&self, id: i64, channel: u64, message: u64) -> anyhow::Result<()> {
        sqlx::query("UPDATE events SET channel_id=$2,message_id=$3 WHERE id=$1")
            .bind(id)
            .bind(sql_id(channel))
            .bind(sql_id(message))
            .execute(self.pool())
            .await?;
        Ok(())
    }
    pub async fn set_discord_event(&self, id: i64, discord_id: Option<u64>) -> anyhow::Result<()> {
        sqlx::query("UPDATE events SET discord_event_id=$2 WHERE id=$1")
            .bind(id)
            .bind(discord_id.map(sql_id))
            .execute(self.pool())
            .await?;
        Ok(())
    }
    pub async fn event_rsvps(&self, id: i64, offset: i64, limit: i64) -> anyhow::Result<Vec<EventRsvp>> {
        Ok(sqlx::query_as("SELECT event_id,user_id,option,waitlisted,responded_at FROM event_rsvps WHERE event_id=$1 ORDER BY option,waitlisted,responded_at,user_id OFFSET $2 LIMIT $3")
            .bind(id).bind(offset).bind(limit.min(25)).fetch_all(self.pool()).await?)
    }
    pub async fn event_rsvp_count(&self, id: i64) -> anyhow::Result<i64> {
        let (n,): (i64,) = sqlx::query_as("SELECT count(*) FROM event_rsvps WHERE event_id=$1")
            .bind(id)
            .fetch_one(self.pool())
            .await?;
        Ok(n)
    }
    pub async fn rsvp_event(&self, guild: u64, event: i64, user: u64, option: &str) -> anyhow::Result<RsvpResult> {
        retry("rsvp_event", || self.rsvp_event_once(guild, event, user, option)).await
    }
    async fn rsvp_event_once(&self, guild: u64, event: i64, user: u64, option: &str) -> anyhow::Result<RsvpResult> {
        let mut tx = self.pool().begin().await?;
        lock_users(&mut tx, &[sql_id(user)]).await?;
        let row: Option<(serde_json::Value, String)> =
            sqlx::query_as("SELECT options,status FROM events WHERE guild_id=$1 AND id=$2 FOR UPDATE")
                .bind(sql_id(guild))
                .bind(event)
                .fetch_optional(&mut *tx)
                .await?;
        let Some((raw, status)) = row else {
            return Ok(RsvpResult::Closed);
        };
        if status != "upcoming" {
            return Ok(RsvpResult::Closed);
        }
        let options: Vec<RsvpOption> = serde_json::from_value(raw)?;
        let old: Option<(String, bool)> =
            sqlx::query_as("SELECT option,waitlisted FROM event_rsvps WHERE event_id=$1 AND user_id=$2")
                .bind(event)
                .bind(sql_id(user))
                .fetch_optional(&mut *tx)
                .await?;
        if old.as_ref().is_some_and(|(choice, _)| choice == option) {
            sqlx::query("DELETE FROM event_rsvps WHERE event_id=$1 AND user_id=$2")
                .bind(event)
                .bind(sql_id(user))
                .execute(&mut *tx)
                .await?;
            if let Some((previous, false)) = old {
                let capacity = options
                    .iter()
                    .find(|o| o.key.as_str() == previous.as_str())
                    .map_or(0, |o| o.capacity);
                promote(&mut tx, event, &previous, capacity).await?;
            }
            tx.commit().await?;
            return Ok(RsvpResult::Removed);
        }
        let Some(selected) = options.iter().find(|o| o.key == option) else {
            return Ok(RsvpResult::Invalid);
        };
        let (count,): (i64,) =
            sqlx::query_as("SELECT count(*) FROM event_rsvps WHERE event_id=$1 AND option=$2 AND NOT waitlisted")
                .bind(event)
                .bind(option)
                .fetch_one(&mut *tx)
                .await?;
        let waitlisted = selected.capacity > 0 && count >= i64::from(selected.capacity);
        sqlx::query("INSERT INTO event_rsvps(event_id,user_id,option,waitlisted) VALUES ($1,$2,$3,$4) ON CONFLICT(event_id,user_id) DO UPDATE SET option=$3,waitlisted=$4,responded_at=now()")
            .bind(event).bind(sql_id(user)).bind(option).bind(waitlisted).execute(&mut *tx).await?;
        if let Some((previous, false)) = &old {
            let capacity = options
                .iter()
                .find(|o| o.key.as_str() == previous.as_str())
                .map_or(0, |o| o.capacity);
            promote(&mut tx, event, previous, capacity).await?;
        }
        tx.commit().await?;
        Ok(if waitlisted {
            RsvpResult::Waitlisted
        } else {
            RsvpResult::Confirmed
        })
    }
    /// Materialises the next occurrence with a unique parent/time key while holding the old row.
    pub async fn advance_events(&self, now: DateTime<Utc>) -> anyhow::Result<Vec<EventRow>> {
        let mut tx = self.pool().begin().await?;
        let due: Vec<EventRow> = sqlx::query_as(sqlx::AssertSqlSafe(format!("SELECT {COLUMNS} FROM events WHERE status='upcoming' AND ends_at <= $1 ORDER BY ends_at LIMIT 25 FOR UPDATE SKIP LOCKED")))
            .bind(now).fetch_all(&mut *tx).await?;
        let mut new_rows = Vec::new();
        for old in due {
            sqlx::query("UPDATE events SET status='ended' WHERE id=$1")
                .bind(old.id)
                .execute(&mut *tx)
                .await?;
            let Some(rule) = old.recurrence() else { continue };
            let tz: chrono_tz::Tz = old.tz.parse()?;
            let Some(start) = next_occurrence(old.starts_at, tz, &rule) else {
                continue;
            };
            let end = start + (old.ends_at - old.starts_at);
            let row: Option<EventRow> = sqlx::query_as(sqlx::AssertSqlSafe(format!("INSERT INTO events (guild_id,title,description,starts_at,ends_at,tz,location,channel_id,host_id,options,required_role_id,reminders,recurrence,parent_id) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14) ON CONFLICT(parent_id,starts_at) DO NOTHING RETURNING {COLUMNS}")))
                .bind(old.guild_id).bind(&old.title).bind(&old.description).bind(start).bind(end).bind(&old.tz)
                .bind(&old.location).bind(old.channel_id).bind(old.host_id).bind(&old.options)
                .bind(old.required_role_id).bind(&old.reminders).bind(&old.recurrence).bind(old.id)
                .fetch_optional(&mut *tx).await?;
            if let Some(row) = row {
                new_rows.push(row);
            }
        }
        tx.commit().await?;
        Ok(new_rows)
    }
}

async fn promote(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    event: i64,
    option: &str,
    capacity: i32,
) -> anyhow::Result<()> {
    if capacity > 0 {
        let (confirmed,): (i64,) =
            sqlx::query_as("SELECT count(*) FROM event_rsvps WHERE event_id=$1 AND option=$2 AND NOT waitlisted")
                .bind(event)
                .bind(option)
                .fetch_one(&mut **tx)
                .await?;
        if confirmed >= i64::from(capacity) {
            return Ok(());
        }
    }
    let promoted: Option<(i64,)> = sqlx::query_as(
        "UPDATE event_rsvps SET waitlisted=FALSE WHERE (event_id,user_id) IN
         (SELECT event_id,user_id FROM event_rsvps WHERE event_id=$1 AND option=$2 AND waitlisted
          ORDER BY responded_at,user_id LIMIT 1) RETURNING user_id",
    )
    .bind(event)
    .bind(option)
    .fetch_optional(&mut **tx)
    .await?;
    if let Some((user,)) = promoted {
        let (start, reminders): (DateTime<Utc>, Vec<i32>) =
            sqlx::query_as("SELECT starts_at, reminders FROM events WHERE id=$1")
                .bind(event)
                .fetch_one(&mut **tx)
                .await?;
        let (off,): (bool,) = sqlx::query_as("SELECT COALESCE((SELECT dm_opt_out FROM users WHERE id=$1), FALSE)")
            .bind(user)
            .fetch_one(&mut **tx)
            .await?;
        if !off && option != "declined" {
            let (language,): (String,) = sqlx::query_as(
                "SELECT COALESCE((SELECT g.language FROM guilds g JOIN events e ON e.guild_id=g.id WHERE e.id=$1), 'en')",
            )
            .bind(event)
            .fetch_one(&mut **tx)
            .await?;
            for minutes in reminders {
                let due = start - chrono::Duration::minutes(i64::from(minutes));
                if due > Utc::now() {
                    sqlx::query(
                        "INSERT INTO reminders (user_id,kind,ref_id,body,due_at) VALUES ($1,$2,$3,$4,$5)
                         ON CONFLICT (user_id,kind,ref_id) WHERE kind <> 'text'
                         DO UPDATE SET body=EXCLUDED.body, due_at=EXCLUDED.due_at, lease_until=NULL",
                    )
                    .bind(user)
                    .bind(format!("event_{minutes}"))
                    .bind(event)
                    .bind(&language)
                    .bind(due)
                    .execute(&mut **tx)
                    .await?;
                }
            }
        }
    }
    Ok(())
}

impl Db {
    pub async fn event_by_id(&self, id: i64) -> anyhow::Result<Option<EventRow>> {
        Ok(
            sqlx::query_as::<_, EventRow>(sqlx::AssertSqlSafe(format!("SELECT {COLUMNS} FROM events WHERE id=$1")))
                .bind(id)
                .fetch_optional(self.pool())
                .await?,
        )
    }
    pub async fn event_reminder_users(&self, id: i64) -> anyhow::Result<Vec<u64>> {
        let rows: Vec<(i64,)> = sqlx::query_as(
            "SELECT user_id FROM event_rsvps WHERE event_id=$1 AND option <> 'declined' AND NOT waitlisted",
        )
        .bind(id)
        .fetch_all(self.pool())
        .await?;
        Ok(rows.into_iter().map(|(id,)| id as u64).collect())
    }
}

impl Db {
    pub async fn event_counts(&self, id: i64) -> anyhow::Result<Vec<(String, i64)>> {
        Ok(sqlx::query_as(
            "SELECT option,count(*) FROM event_rsvps WHERE event_id=$1 AND NOT waitlisted GROUP BY option",
        )
        .bind(id)
        .fetch_all(self.pool())
        .await?)
    }
}

impl Db {
    pub async fn update_scheduled_event_status(
        &self,
        guild: u64,
        discord_id: u64,
        status: &str,
    ) -> anyhow::Result<Option<EventRow>> {
        let row = sqlx::query_as::<_, EventRow>(sqlx::AssertSqlSafe(format!(
            "UPDATE events SET status=$3 WHERE guild_id=$1 AND discord_event_id=$2 AND status='upcoming' RETURNING {COLUMNS}"
        )))
            .bind(sql_id(guild))
            .bind(sql_id(discord_id))
            .bind(status)
            .fetch_optional(self.pool())
            .await?;
        Ok(row)
    }
}
