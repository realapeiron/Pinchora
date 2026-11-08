//! Ticket operation settings, private notes, snippets and atomic state changes.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::types::Json;

use crate::{Db, locks::lock_users, sql_id};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SupportWindow {
    /// Weekday in the configured timezone, Monday = 1.
    pub day: u8,
    /// Minutes after midnight in the configured IANA timezone.
    pub start: u16,
    pub end: u16,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TicketFaq {
    pub question: String,
    pub answer: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TicketOperationSettings {
    pub support_timezone: String,
    pub support_hours: Vec<SupportWindow>,
    pub out_of_hours_message: String,
    pub status_channel_id: Option<i64>,
    pub status_message_id: Option<i64>,
    pub close_timeout_hours: i32,
}

impl Default for TicketOperationSettings {
    fn default() -> Self {
        Self {
            support_timezone: "UTC".into(),
            support_hours: Vec::new(),
            out_of_hours_message: String::new(),
            status_channel_id: None,
            status_message_id: None,
            close_timeout_hours: 24,
        }
    }
}

#[derive(sqlx::FromRow)]
struct TicketOperationSettingsRow {
    support_timezone: String,
    support_hours: Json<Vec<SupportWindow>>,
    out_of_hours_message: String,
    status_channel_id: Option<i64>,
    status_message_id: Option<i64>,
    close_timeout_hours: i32,
}

impl From<TicketOperationSettingsRow> for TicketOperationSettings {
    fn from(row: TicketOperationSettingsRow) -> Self {
        Self {
            support_timezone: row.support_timezone,
            support_hours: row.support_hours.0,
            out_of_hours_message: row.out_of_hours_message,
            status_channel_id: row.status_channel_id,
            status_message_id: row.status_message_id,
            close_timeout_hours: row.close_timeout_hours,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TicketPanelOperations {
    pub claim_mode: String,
    pub max_open_per_panel: i32,
    pub max_open_per_category: i32,
    pub cooldown_seconds: i32,
    pub minimum_account_age_days: i32,
    pub minimum_membership_days: i32,
    pub bypass_roles: Vec<i64>,
    pub staff_open_limit: i32,
    pub overflow_message: String,
    pub faq: Vec<TicketFaq>,
}

impl Default for TicketPanelOperations {
    fn default() -> Self {
        Self {
            claim_mode: "optional".into(),
            max_open_per_panel: 0,
            max_open_per_category: 0,
            cooldown_seconds: 60,
            minimum_account_age_days: 0,
            minimum_membership_days: 0,
            bypass_roles: Vec::new(),
            staff_open_limit: 0,
            overflow_message: String::new(),
            faq: Vec::new(),
        }
    }
}

#[derive(sqlx::FromRow)]
struct TicketPanelOperationsRow {
    claim_mode: String,
    max_open_per_panel: i32,
    max_open_per_category: i32,
    cooldown_seconds: i32,
    minimum_account_age_days: i32,
    minimum_membership_days: i32,
    bypass_roles: Vec<i64>,
    staff_open_limit: i32,
    overflow_message: String,
    faq: Json<Vec<TicketFaq>>,
}

impl From<TicketPanelOperationsRow> for TicketPanelOperations {
    fn from(row: TicketPanelOperationsRow) -> Self {
        Self {
            claim_mode: row.claim_mode,
            max_open_per_panel: row.max_open_per_panel,
            max_open_per_category: row.max_open_per_category,
            cooldown_seconds: row.cooldown_seconds,
            minimum_account_age_days: row.minimum_account_age_days,
            minimum_membership_days: row.minimum_membership_days,
            bypass_roles: row.bypass_roles,
            staff_open_limit: row.staff_open_limit,
            overflow_message: row.overflow_message,
            faq: row.faq.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow, Serialize)]
pub struct TicketOperations {
    pub priority: String,
    pub escalated_at: Option<DateTime<Utc>>,
    pub transferred_to_user: Option<i64>,
    pub transferred_to_role: Option<i64>,
    pub close_requested_by: Option<i64>,
    pub close_requested_at: Option<DateTime<Utc>>,
    pub close_request_expires_at: Option<DateTime<Utc>>,
    pub force_opened_by: Option<i64>,
    pub last_reminder_at: Option<DateTime<Utc>>,
}

impl Default for TicketOperations {
    fn default() -> Self {
        Self {
            priority: "normal".into(),
            escalated_at: None,
            transferred_to_user: None,
            transferred_to_role: None,
            close_requested_by: None,
            close_requested_at: None,
            close_request_expires_at: None,
            force_opened_by: None,
            last_reminder_at: None,
        }
    }
}

#[derive(Debug, Clone, sqlx::FromRow, Serialize)]
pub struct TicketStaffNote {
    pub id: i64,
    pub author_id: i64,
    pub body: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, sqlx::FromRow, Serialize)]
pub struct TicketSnippet {
    pub id: i64,
    pub name: String,
    pub body: String,
}

#[derive(Debug, Clone, sqlx::FromRow, Serialize)]
pub struct TicketStaffScore {
    pub user_id: i64,
    pub claimed: i64,
    pub closed: i64,
}

impl Db {
    pub async fn purge_expired_ticket_form_drafts(&self) -> anyhow::Result<u64> {
        Ok(sqlx::query("DELETE FROM ticket_form_drafts WHERE expires_at <= now()")
            .execute(self.pool())
            .await?
            .rows_affected())
    }
    pub async fn ticket_staff_leaderboard(&self, guild: u64) -> anyhow::Result<Vec<TicketStaffScore>> {
        Ok(sqlx::query_as(
            "SELECT user_id, SUM(claimed)::bigint AS claimed, SUM(closed)::bigint AS closed
            FROM (SELECT claimed_by AS user_id, 1 AS claimed, 0 AS closed FROM tickets
                  WHERE guild_id=$1 AND claimed_by IS NOT NULL
                  UNION ALL
                  SELECT closed_by AS user_id, 0 AS claimed, 1 AS closed FROM tickets
                  WHERE guild_id=$1 AND closed_by IS NOT NULL) s
            GROUP BY user_id ORDER BY closed DESC, claimed DESC LIMIT 20",
        )
        .bind(sql_id(guild))
        .fetch_all(self.pool())
        .await?)
    }
    pub async fn ticket_status_boards(&self) -> anyhow::Result<Vec<(i64, i64, Option<i64>)>> {
        Ok(sqlx::query_as(
            "SELECT guild_id,status_channel_id,status_message_id
            FROM ticket_operation_settings WHERE status_channel_id IS NOT NULL",
        )
        .fetch_all(self.pool())
        .await?)
    }

    pub async fn set_ticket_status_message(&self, guild: u64, channel: u64, message: u64) -> anyhow::Result<bool> {
        let done = sqlx::query(
            "UPDATE ticket_operation_settings SET status_message_id=$3
            WHERE guild_id=$1 AND status_channel_id=$2",
        )
        .bind(sql_id(guild))
        .bind(sql_id(channel))
        .bind(sql_id(message))
        .execute(self.pool())
        .await?;
        Ok(done.rows_affected() == 1)
    }

    pub async fn ticket_status_counts(&self, guild: u64) -> anyhow::Result<(i64, i64, i64)> {
        Ok(sqlx::query_as(
            "SELECT count(*) FILTER (WHERE status='open'),
                count(*) FILTER (WHERE status='open' AND claimed_by IS NOT NULL),
                count(*) FILTER (WHERE status='closed' AND closed_at>now()-interval '24 hours')
            FROM tickets WHERE guild_id=$1",
        )
        .bind(sql_id(guild))
        .fetch_one(self.pool())
        .await?)
    }
    pub async fn ticket_operation_settings(&self, guild: u64) -> anyhow::Result<TicketOperationSettings> {
        let row: Option<TicketOperationSettingsRow> = sqlx::query_as(
            "SELECT support_timezone, support_hours, out_of_hours_message, status_channel_id,
                    status_message_id, close_timeout_hours FROM ticket_operation_settings WHERE guild_id = $1",
        )
        .bind(sql_id(guild))
        .fetch_optional(self.pool())
        .await?;
        Ok(row.map(Into::into).unwrap_or_default())
    }

    pub async fn save_ticket_operation_settings(
        &self,
        guild: u64,
        settings: &TicketOperationSettings,
    ) -> anyhow::Result<()> {
        sqlx::query(
            "INSERT INTO ticket_operation_settings (guild_id, support_timezone, support_hours, out_of_hours_message,
                    status_channel_id, status_message_id, close_timeout_hours) VALUES ($1,$2,$3,$4,$5,$6,$7)
             ON CONFLICT (guild_id) DO UPDATE SET support_timezone=$2, support_hours=$3,
                    out_of_hours_message=$4, status_channel_id=$5, status_message_id=$6,
                    close_timeout_hours=$7, updated_at=now()",
        )
        .bind(sql_id(guild))
        .bind(&settings.support_timezone)
        .bind(Json(&settings.support_hours))
        .bind(&settings.out_of_hours_message)
        .bind(settings.status_channel_id)
        .bind(settings.status_message_id)
        .bind(settings.close_timeout_hours)
        .execute(self.pool())
        .await?;
        Ok(())
    }

    pub async fn ticket_panel_operations(&self, panel: i64) -> anyhow::Result<TicketPanelOperations> {
        let row: Option<TicketPanelOperationsRow> = sqlx::query_as(
            "SELECT claim_mode, max_open_per_panel, max_open_per_category, cooldown_seconds,
                    minimum_account_age_days, minimum_membership_days, bypass_roles, staff_open_limit,
                    overflow_message, faq FROM ticket_panel_operations WHERE panel_id=$1",
        )
        .bind(panel)
        .fetch_optional(self.pool())
        .await?;
        Ok(row.map(Into::into).unwrap_or_default())
    }

    pub async fn save_ticket_panel_operations(&self, panel: i64, ops: &TicketPanelOperations) -> anyhow::Result<()> {
        sqlx::query(
            "INSERT INTO ticket_panel_operations (panel_id, claim_mode, max_open_per_panel, max_open_per_category,
                cooldown_seconds, minimum_account_age_days, minimum_membership_days, bypass_roles,
                staff_open_limit, overflow_message, faq) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)
             ON CONFLICT (panel_id) DO UPDATE SET claim_mode=$2, max_open_per_panel=$3, max_open_per_category=$4,
                cooldown_seconds=$5, minimum_account_age_days=$6, minimum_membership_days=$7, bypass_roles=$8,
                staff_open_limit=$9, overflow_message=$10, faq=$11, updated_at=now()",
        )
        .bind(panel)
        .bind(&ops.claim_mode)
        .bind(ops.max_open_per_panel)
        .bind(ops.max_open_per_category)
        .bind(ops.cooldown_seconds)
        .bind(ops.minimum_account_age_days)
        .bind(ops.minimum_membership_days)
        .bind(&ops.bypass_roles)
        .bind(ops.staff_open_limit)
        .bind(&ops.overflow_message)
        .bind(Json(&ops.faq))
        .execute(self.pool())
        .await?;
        Ok(())
    }

    pub async fn ticket_operations(&self, ticket: i64) -> anyhow::Result<TicketOperations> {
        Ok(sqlx::query_as(
            "SELECT priority, escalated_at, transferred_to_user, transferred_to_role,
                    close_requested_by, close_requested_at, close_request_expires_at, force_opened_by,
                    last_reminder_at
                FROM ticket_operations WHERE ticket_id=$1",
        )
        .bind(ticket)
        .fetch_optional(self.pool())
        .await?
        .unwrap_or_default())
    }

    pub async fn due_ticket_reminders(&self, now: DateTime<Utc>, limit: i64) -> anyhow::Result<Vec<(i64, i64, i64)>> {
        Ok(sqlx::query_as(
            "SELECT t.id,t.guild_id,COALESCE(t.thread_id,t.channel_id)
            FROM tickets t JOIN ticket_settings s ON s.guild_id=t.guild_id
            LEFT JOIN ticket_operations o ON o.ticket_id=t.id
            WHERE t.status='open' AND s.auto_close_hours>0 AND COALESCE(t.thread_id,t.channel_id) IS NOT NULL
              AND t.last_activity_at + (s.auto_close_hours * interval '30 minutes') <= $1
              AND t.last_activity_at + (s.auto_close_hours * interval '1 hour') > $1
              AND (o.last_reminder_at IS NULL OR o.last_reminder_at < t.last_activity_at)
            ORDER BY t.last_activity_at LIMIT $2",
        )
        .bind(now)
        .bind(limit)
        .fetch_all(self.pool())
        .await?)
    }

    pub async fn claim_ticket_reminder(&self, ticket: i64) -> anyhow::Result<bool> {
        let done = sqlx::query(
            "INSERT INTO ticket_operations(ticket_id,last_reminder_at)
            SELECT id,now() FROM tickets WHERE id=$1 AND status='open'
            ON CONFLICT(ticket_id) DO UPDATE SET last_reminder_at=now()
            WHERE ticket_operations.last_reminder_at IS NULL OR ticket_operations.last_reminder_at <
                (SELECT last_activity_at FROM tickets WHERE id=$1)",
        )
        .bind(ticket)
        .execute(self.pool())
        .await?;
        Ok(done.rows_affected() == 1)
    }

    pub async fn set_ticket_priority(&self, ticket: i64, priority: &str, escalate: bool) -> anyhow::Result<bool> {
        let done = sqlx::query(
            "INSERT INTO ticket_operations(ticket_id, priority, escalated_at) 
             SELECT id, $2, CASE WHEN $3 THEN now() END FROM tickets WHERE id=$1 AND status='open'
             ON CONFLICT (ticket_id) DO UPDATE SET priority=$2,
                escalated_at=CASE WHEN $3 THEN now() ELSE ticket_operations.escalated_at END, updated_at=now()",
        )
        .bind(ticket)
        .bind(priority)
        .bind(escalate)
        .execute(self.pool())
        .await?;
        Ok(done.rows_affected() == 1)
    }

    pub async fn mark_ticket_force_opened(&self, ticket: i64, by: u64) -> anyhow::Result<()> {
        sqlx::query(
            "INSERT INTO ticket_operations(ticket_id,force_opened_by) VALUES($1,$2)
            ON CONFLICT(ticket_id) DO UPDATE SET force_opened_by=$2,updated_at=now()",
        )
        .bind(ticket)
        .bind(sql_id(by))
        .execute(self.pool())
        .await?;
        Ok(())
    }

    pub async fn transfer_ticket(&self, ticket: i64, user: Option<u64>, role: Option<u64>) -> anyhow::Result<bool> {
        let mut tx = self.pool().begin().await?;
        if let Some(user) = user {
            lock_users(&mut tx, &[sql_id(user)]).await?;
            let limit: Option<(i32,)> = sqlx::query_as(
                "SELECT o.staff_open_limit FROM tickets t JOIN ticket_panel_operations o ON o.panel_id=t.panel_id
                 WHERE t.id=$1",
            )
            .bind(ticket)
            .fetch_optional(&mut *tx)
            .await?;
            if let Some((limit,)) = limit
                && limit > 0
            {
                let count: i64 = sqlx::query_scalar(
                    "SELECT count(*) FROM tickets WHERE guild_id=(SELECT guild_id FROM tickets WHERE id=$1)
                     AND claimed_by=$2 AND status='open' AND id<>$1",
                )
                .bind(ticket)
                .bind(sql_id(user))
                .fetch_one(&mut *tx)
                .await?;
                if count >= i64::from(limit) {
                    return Ok(false);
                }
            }
        }
        let done = sqlx::query("UPDATE tickets SET claimed_by=$2 WHERE id=$1 AND status='open'")
            .bind(ticket)
            .bind(user.map(sql_id))
            .execute(&mut *tx)
            .await?;
        if done.rows_affected() == 0 {
            return Ok(false);
        }
        sqlx::query(
            "INSERT INTO ticket_operations(ticket_id, transferred_to_user, transferred_to_role)
            VALUES ($1,$2,$3) ON CONFLICT (ticket_id) DO UPDATE SET transferred_to_user=$2,
            transferred_to_role=$3, updated_at=now()",
        )
        .bind(ticket)
        .bind(user.map(sql_id))
        .bind(role.map(sql_id))
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(true)
    }

    pub async fn request_ticket_close(&self, ticket: i64, by: u64, timeout_hours: i32) -> anyhow::Result<bool> {
        let done = sqlx::query(
            "INSERT INTO ticket_operations(ticket_id, close_requested_by, close_requested_at, close_request_expires_at)
             SELECT id,$2,now(),now()+($3::integer * interval '1 hour') FROM tickets
             WHERE id=$1 AND status='open'
             ON CONFLICT (ticket_id) DO UPDATE SET close_requested_by=$2, close_requested_at=now(),
                close_request_expires_at=now()+($3::integer * interval '1 hour'), updated_at=now()
             WHERE ticket_operations.close_request_expires_at IS NULL",
        )
        .bind(ticket)
        .bind(sql_id(by))
        .bind(timeout_hours)
        .execute(self.pool())
        .await?;
        Ok(done.rows_affected() == 1)
    }

    pub async fn cancel_ticket_close_request(&self, ticket: i64) -> anyhow::Result<bool> {
        let done = sqlx::query(
            "UPDATE ticket_operations SET close_requested_by=NULL, close_requested_at=NULL,
            close_request_expires_at=NULL WHERE ticket_id=$1 AND close_request_expires_at IS NOT NULL",
        )
        .bind(ticket)
        .execute(self.pool())
        .await?;
        Ok(done.rows_affected() == 1)
    }

    pub async fn due_ticket_close_requests(&self, now: DateTime<Utc>, limit: i64) -> anyhow::Result<Vec<i64>> {
        Ok(sqlx::query_scalar(
            "SELECT o.ticket_id FROM ticket_operations o JOIN tickets t ON t.id=o.ticket_id
            WHERE t.status='open' AND o.close_request_expires_at <= $1 ORDER BY o.close_request_expires_at LIMIT $2",
        )
        .bind(now)
        .bind(limit)
        .fetch_all(self.pool())
        .await?)
    }

    pub async fn add_ticket_staff_note(
        &self,
        ticket: i64,
        author: u64,
        body: &str,
    ) -> anyhow::Result<Option<TicketStaffNote>> {
        Ok(sqlx::query_as(
            "INSERT INTO ticket_staff_notes(ticket_id,author_id,body)
            SELECT id,$2,$3 FROM tickets WHERE id=$1 RETURNING id,author_id,body,created_at",
        )
        .bind(ticket)
        .bind(sql_id(author))
        .bind(body)
        .fetch_optional(self.pool())
        .await?)
    }

    pub async fn ticket_staff_notes(&self, ticket: i64) -> anyhow::Result<Vec<TicketStaffNote>> {
        Ok(sqlx::query_as(
            "SELECT id,author_id,body,created_at FROM ticket_staff_notes
            WHERE ticket_id=$1 ORDER BY id DESC LIMIT 100",
        )
        .bind(ticket)
        .fetch_all(self.pool())
        .await?)
    }

    pub async fn ticket_snippets(&self, guild: u64) -> anyhow::Result<Vec<TicketSnippet>> {
        Ok(
            sqlx::query_as("SELECT id,name,body FROM ticket_snippets WHERE guild_id=$1 ORDER BY name")
                .bind(sql_id(guild))
                .fetch_all(self.pool())
                .await?,
        )
    }

    pub async fn save_ticket_snippet(&self, guild: u64, name: &str, body: &str) -> anyhow::Result<TicketSnippet> {
        Ok(sqlx::query_as(
            "INSERT INTO ticket_snippets(guild_id,name,body) VALUES($1,$2,$3)
            ON CONFLICT(guild_id,name) DO UPDATE SET body=$3, updated_at=now() RETURNING id,name,body",
        )
        .bind(sql_id(guild))
        .bind(name)
        .bind(body)
        .fetch_one(self.pool())
        .await?)
    }

    pub async fn delete_ticket_snippet(&self, guild: u64, id: i64) -> anyhow::Result<bool> {
        Ok(sqlx::query("DELETE FROM ticket_snippets WHERE guild_id=$1 AND id=$2")
            .bind(sql_id(guild))
            .bind(id)
            .execute(self.pool())
            .await?
            .rows_affected()
            == 1)
    }

    pub async fn save_ticket_form_draft(
        &self,
        guild: u64,
        user: u64,
        panel: i64,
        topic: i32,
        answers: &serde_json::Value,
        step: i32,
    ) -> anyhow::Result<()> {
        sqlx::query(
            "INSERT INTO ticket_form_drafts(guild_id,user_id,panel_id,topic_index,answers,step,expires_at)
            VALUES($1,$2,$3,$4,$5,$6,now()+interval '30 minutes')
            ON CONFLICT(guild_id,user_id,panel_id,topic_index) DO UPDATE SET answers=$5,step=$6,
            expires_at=now()+interval '30 minutes'",
        )
        .bind(sql_id(guild))
        .bind(sql_id(user))
        .bind(panel)
        .bind(topic)
        .bind(answers)
        .bind(step)
        .execute(self.pool())
        .await?;
        Ok(())
    }

    pub async fn ticket_form_draft(
        &self,
        guild: u64,
        user: u64,
        panel: i64,
        topic: i32,
    ) -> anyhow::Result<Option<(serde_json::Value, i32)>> {
        Ok(sqlx::query_as(
            "SELECT answers,step FROM ticket_form_drafts WHERE guild_id=$1 AND user_id=$2
            AND panel_id=$3 AND topic_index=$4 AND expires_at>now()",
        )
        .bind(sql_id(guild))
        .bind(sql_id(user))
        .bind(panel)
        .bind(topic)
        .fetch_optional(self.pool())
        .await?)
    }

    pub async fn delete_ticket_form_draft(&self, guild: u64, user: u64, panel: i64, topic: i32) -> anyhow::Result<()> {
        sqlx::query(
            "DELETE FROM ticket_form_drafts WHERE guild_id=$1 AND user_id=$2 AND panel_id=$3 AND topic_index=$4",
        )
        .bind(sql_id(guild))
        .bind(sql_id(user))
        .bind(panel)
        .bind(topic)
        .execute(self.pool())
        .await?;
        Ok(())
    }
}
