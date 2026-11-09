//! Tickets: a server's ticket settings, its panels and their forms, the tickets (numbered per server,
//! closed exactly once) and the transcripts kept when they close.

use chrono::{DateTime, Duration, Utc};
use pinchora_core::community::tickets::Answer;
use pinchora_core::community::tickets::Author;
use pinchora_core::community::tickets::Line;
use pinchora_core::community::tickets::MAX_PANELS;
use pinchora_core::community::tickets::Mode;
use pinchora_core::community::tickets::PanelStyle;
use pinchora_core::community::tickets::Question;
use pinchora_core::community::tickets::Topic;
use sqlx::{Postgres, Transaction};

use crate::Db;
use crate::infra::locks::lock_users;
use crate::infra::locks::retry;
use crate::sql_id;

pub const TICKET_OPEN: &str = "open";
pub const TICKET_CLOSED: &str = "closed";

/// A server's ticket settings; [`Default`] for a server that never changed them.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct TicketSettings {
    pub open_category_id: Option<i64>,
    pub closed_category_id: Option<i64>,
    pub log_channel_id: Option<i64>,
    pub auto_close_hours: i32,
    pub dm_transcript: bool,
    pub rating_enabled: bool,
    pub naming: String,
    pub mode: String,
    pub max_open_per_user: i32,
}

impl Default for TicketSettings {
    fn default() -> Self {
        Self {
            open_category_id: None,
            closed_category_id: None,
            log_channel_id: None,
            auto_close_hours: 0,
            dm_transcript: true,
            rating_enabled: false,
            naming: pinchora_core::community::tickets::DEFAULT_NAMING.to_owned(),
            mode: Mode::Auto.key().to_owned(),
            max_open_per_user: 1,
        }
    }
}

impl TicketSettings {
    pub fn mode(&self) -> Mode {
        Mode::parse(&self.mode).unwrap_or_default()
    }
}

/// A ticket, open or closed.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct Ticket {
    pub id: i64,
    pub guild_id: i64,
    pub panel_id: Option<i64>,
    pub number: i32,
    pub opener_id: i64,
    pub channel_id: Option<i64>,
    pub thread_id: Option<i64>,
    pub topic: String,
    pub status: String,
    pub locked: bool,
    pub claimed_by: Option<i64>,
    pub closed_by: Option<i64>,
    pub close_reason: Option<String>,
    pub opened_at: DateTime<Utc>,
    pub closed_at: Option<DateTime<Utc>>,
    pub last_activity_at: DateTime<Utc>,
    pub first_reply_at: Option<DateTime<Utc>>,
    pub rating: Option<i16>,
    /// What the member answered in the panel's form, as JSON; [`Ticket::answers`] reads it.
    pub form_answers: String,
}

impl Ticket {
    pub fn is_open(&self) -> bool {
        self.status == TICKET_OPEN
    }

    pub fn answers(&self) -> Vec<Answer> {
        serde_json::from_str(&self.form_answers).unwrap_or_default()
    }

    pub fn state(&self) -> pinchora_core::community::tickets::State {
        pinchora_core::community::tickets::State::of(!self.is_open(), self.claimed_by.is_some())
    }

    /// The channel members talk in: the thread, or the ticket's own channel.
    pub fn place(&self) -> Option<u64> {
        self.thread_id.or(self.channel_id).map(|id| id as u64)
    }
}

const TICKET_COLUMNS: &str = "t.id, t.guild_id, t.panel_id, t.number, t.opener_id, t.channel_id, t.thread_id, \
     t.topic, t.status, t.locked, t.claimed_by, t.closed_by, t.close_reason, t.opened_at, t.closed_at, \
     t.last_activity_at, t.first_reply_at, t.rating, t.form_answers::TEXT AS form_answers";

/// A ticket about to be opened.
#[derive(Debug, Clone)]
pub struct NewTicket<'a> {
    pub guild_id: u64,
    pub panel_id: Option<i64>,
    pub opener: u64,
    pub topic: &'a str,
    pub answers: &'a [Answer],
    /// How many open tickets one member may have in the server.
    pub max_open: i32,
    pub at: DateTime<Utc>,
}

/// How opening went.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TicketOpen {
    /// The ticket has its number; its channel comes next ([`Db::set_ticket_place`]).
    Opened(Box<Ticket>),
    /// The member already has `open` tickets open, the most the server allows.
    TooMany {
        open: i64,
    },
    Cooldown {
        seconds: i64,
    },
}

/// Which tickets a list shows.
#[derive(Debug, Clone, Default)]
pub struct TicketFilter {
    /// `open`, `claimed` or `closed`; anything else is all of them.
    pub state: Option<String>,
    pub staff: Option<u64>,
    pub opener: Option<u64>,
    /// Words in the topic, the close reason or the number.
    pub text: Option<String>,
    /// Only tickets older than this one, for the next page.
    pub before: Option<i64>,
}

/// A week's or a month's tickets, for the dashboard's summary.
#[derive(Debug, Clone, Default, PartialEq, sqlx::FromRow)]
pub struct TicketSummary {
    pub opened: i64,
    pub closed: i64,
    pub open_now: i64,
    pub median_first_reply_secs: Option<f64>,
    pub ratings: i64,
    pub rating_avg: Option<f64>,
}

/// An open ticket that may have gone quiet for longer than its server allows.
#[derive(Debug, Clone, Copy, sqlx::FromRow)]
pub struct IdleTicket {
    pub id: i64,
    pub guild_id: i64,
    pub place: i64,
    pub last_activity_at: DateTime<Utc>,
    pub auto_close_hours: i32,
}

/// A closed ticket's transcript as kept.
#[derive(Debug, Clone)]
pub struct StoredTranscript {
    pub lines: Vec<Line>,
    pub authors: Vec<Author>,
    /// Retained author ids by line index. Delete-my-data removes the user's row from this map.
    pub author_ids: Vec<(u16, u64)>,
    pub with_content: bool,
    pub messages: i32,
}

impl Db {
    // /Settings

    pub async fn ticket_settings(&self, guild: u64) -> anyhow::Result<TicketSettings> {
        let row = sqlx::query_as::<_, TicketSettings>(
            "SELECT open_category_id, closed_category_id, log_channel_id, auto_close_hours, dm_transcript,
                    rating_enabled, naming, mode, max_open_per_user
             FROM ticket_settings WHERE guild_id = $1",
        )
        .bind(sql_id(guild))
        .fetch_optional(self.pool())
        .await?;
        Ok(row.unwrap_or_default())
    }

    pub async fn save_ticket_settings(&self, guild: u64, settings: &TicketSettings) -> anyhow::Result<()> {
        sqlx::query(
            "INSERT INTO ticket_settings (guild_id, open_category_id, closed_category_id, log_channel_id,
                 auto_close_hours, dm_transcript, rating_enabled, naming, mode, max_open_per_user, updated_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, now())
             ON CONFLICT (guild_id) DO UPDATE SET open_category_id = EXCLUDED.open_category_id,
                 closed_category_id = EXCLUDED.closed_category_id, log_channel_id = EXCLUDED.log_channel_id,
                 auto_close_hours = EXCLUDED.auto_close_hours, dm_transcript = EXCLUDED.dm_transcript,
                 rating_enabled = EXCLUDED.rating_enabled, naming = EXCLUDED.naming, mode = EXCLUDED.mode,
                 max_open_per_user = EXCLUDED.max_open_per_user, updated_at = now()",
        )
        .bind(sql_id(guild))
        .bind(settings.open_category_id)
        .bind(settings.closed_category_id)
        .bind(settings.log_channel_id)
        .bind(settings.auto_close_hours)
        .bind(settings.dm_transcript)
        .bind(settings.rating_enabled)
        .bind(&settings.naming)
        .bind(&settings.mode)
        .bind(settings.max_open_per_user)
        .execute(self.pool())
        .await?;
        Ok(())
    }

    // /Tickets

    /// Gives the next number of the server to a new ticket of `opener`, unless they already have as many
    /// open as the server allows. Two members opening at the same moment get different numbers; one member
    /// opening twice at once gets one ticket when only one is allowed.
    pub async fn open_ticket(&self, new: &NewTicket<'_>) -> anyhow::Result<TicketOpen> {
        retry("open_ticket", || self.open_ticket_once(new)).await
    }

    async fn open_ticket_once(&self, new: &NewTicket<'_>) -> anyhow::Result<TicketOpen> {
        let mut tx = self.pool().begin().await?;
        lock_users(&mut tx, &[sql_id(new.opener)]).await?;
        let guild = sql_id(new.guild_id);
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended('ticket-open:' || $1::text, 0))")
            .bind(guild)
            .execute(&mut *tx)
            .await?;
        let (open,): (i64,) =
            sqlx::query_as("SELECT count(*) FROM tickets WHERE guild_id = $1 AND opener_id = $2 AND status = 'open'")
                .bind(guild)
                .bind(sql_id(new.opener))
                .fetch_one(&mut *tx)
                .await?;
        if open >= i64::from(new.max_open) {
            return Ok(TicketOpen::TooMany { open });
        }
        if let Some(refusal) = crate::community::ticket_limits::check_open_limits(&mut tx, new).await? {
            return Ok(refusal);
        }
        let (number,): (i32,) = sqlx::query_as(
            "INSERT INTO ticket_counters (guild_id, next) VALUES ($1, 2)
             ON CONFLICT (guild_id) DO UPDATE SET next = ticket_counters.next + 1
             RETURNING next - 1",
        )
        .bind(guild)
        .fetch_one(&mut *tx)
        .await?;
        let sql = format!(
            "INSERT INTO tickets AS t (guild_id, panel_id, number, opener_id, topic, form_answers, opened_at,
                 last_activity_at)
             VALUES ($1, $2, $3, $4, $5, $6::jsonb, $7, $7) RETURNING {TICKET_COLUMNS}"
        );
        let ticket = sqlx::query_as::<_, Ticket>(sqlx::AssertSqlSafe(sql))
            .bind(guild)
            .bind(new.panel_id)
            .bind(number)
            .bind(sql_id(new.opener))
            .bind(new.topic)
            .bind(serde_json::to_string(new.answers)?)
            .bind(new.at)
            .fetch_one(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(TicketOpen::Opened(Box::new(ticket)))
    }

    /// Where a ticket lives once its channel or thread exists.
    pub async fn set_ticket_place(&self, id: i64, channel: u64, thread: Option<u64>) -> anyhow::Result<()> {
        sqlx::query("UPDATE tickets SET channel_id = $2, thread_id = $3 WHERE id = $1")
            .bind(id)
            .bind(sql_id(channel))
            .bind(thread.map(sql_id))
            .execute(self.pool())
            .await?;
        Ok(())
    }

    /// Takes back a ticket whose channel could not be made: as if it had never been opened (its number
    /// stays used).
    pub async fn abort_ticket(&self, id: i64) -> anyhow::Result<()> {
        sqlx::query("DELETE FROM tickets WHERE id = $1 AND channel_id IS NULL")
            .bind(id)
            .execute(self.pool())
            .await?;
        Ok(())
    }

    pub async fn ticket(&self, id: i64) -> anyhow::Result<Option<Ticket>> {
        let sql = format!("SELECT {TICKET_COLUMNS} FROM tickets t WHERE t.id = $1");
        Ok(sqlx::query_as::<_, Ticket>(sqlx::AssertSqlSafe(sql))
            .bind(id)
            .fetch_optional(self.pool())
            .await?)
    }

    /// The ticket a channel or thread of the server belongs to.
    pub async fn ticket_at(&self, guild: u64, place: u64) -> anyhow::Result<Option<Ticket>> {
        let sql = format!(
            "SELECT {TICKET_COLUMNS} FROM tickets t
             WHERE COALESCE(t.thread_id, t.channel_id) = $2 AND t.guild_id = $1 ORDER BY t.id DESC LIMIT 1"
        );
        Ok(sqlx::query_as::<_, Ticket>(sqlx::AssertSqlSafe(sql))
            .bind(sql_id(guild))
            .bind(sql_id(place))
            .fetch_optional(self.pool())
            .await?)
    }

    /// Open tickets belonging to a member, for checking whether Discord still has their places.
    pub async fn open_tickets_for_user(&self, guild: u64, opener: u64) -> anyhow::Result<Vec<Ticket>> {
        let sql = format!(
            "SELECT {TICKET_COLUMNS} FROM tickets t WHERE t.guild_id = $1 AND t.opener_id = $2 AND t.status = 'open'"
        );
        Ok(sqlx::query_as::<_, Ticket>(sqlx::AssertSqlSafe(sql))
            .bind(sql_id(guild))
            .bind(sql_id(opener))
            .fetch_all(self.pool())
            .await?)
    }

    /// `staff` takes an open ticket nobody has claimed; `false` when it is claimed or closed already.
    pub async fn claim_ticket(&self, id: i64, staff: u64) -> anyhow::Result<bool> {
        let mut tx = self.pool().begin().await?;
        lock_users(&mut tx, &[sql_id(staff)]).await?;
        let limit: Option<(i32,)> = sqlx::query_as(
            "SELECT o.staff_open_limit FROM tickets t JOIN ticket_panel_operations o ON o.panel_id=t.panel_id
             WHERE t.id=$1",
        )
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?;
        if let Some((limit,)) = limit
            && limit > 0
        {
            let count: i64 = sqlx::query_scalar(
                "SELECT count(*) FROM tickets WHERE guild_id=(SELECT guild_id FROM tickets WHERE id=$1)
                 AND claimed_by=$2 AND status='open'",
            )
            .bind(id)
            .bind(sql_id(staff))
            .fetch_one(&mut *tx)
            .await?;
            if count >= i64::from(limit) {
                return Ok(false);
            }
        }
        let done =
            sqlx::query("UPDATE tickets SET claimed_by = $2 WHERE id = $1 AND status = 'open' AND claimed_by IS NULL")
                .bind(id)
                .bind(sql_id(staff))
                .execute(&mut *tx)
                .await?;
        tx.commit().await?;
        Ok(done.rows_affected() == 1)
    }

    pub async fn unclaim_ticket_by(&self, id: i64, staff: u64) -> anyhow::Result<bool> {
        let done = sqlx::query(
            "UPDATE tickets t SET claimed_by=NULL WHERE t.id=$1 AND t.status='open' AND t.claimed_by IS NOT NULL
             AND (NOT EXISTS(SELECT 1 FROM ticket_panel_operations o WHERE o.panel_id=t.panel_id AND o.claim_mode='exclusive')
                  OR t.claimed_by=$2)",
        ).bind(id).bind(sql_id(staff)).execute(self.pool()).await?;
        Ok(done.rows_affected() == 1)
    }

    pub async fn unclaim_ticket(&self, id: i64) -> anyhow::Result<bool> {
        let done = sqlx::query(
            "UPDATE tickets SET claimed_by = NULL WHERE id = $1 AND status = 'open' AND claimed_by IS NOT NULL",
        )
        .bind(id)
        .execute(self.pool())
        .await?;
        Ok(done.rows_affected() == 1)
    }

    /// Closes an open ticket, once: whoever gets here first gets the ticket back and does what closing does
    /// (the transcript, the log); everyone after gets `None`. `idle_before`, for the auto-close, only closes
    /// it if nobody wrote since then.
    pub async fn close_ticket(
        &self,
        id: i64,
        by: Option<u64>,
        reason: Option<&str>,
        at: DateTime<Utc>,
        idle_before: Option<DateTime<Utc>>,
    ) -> anyhow::Result<Option<Ticket>> {
        self.close_ticket_guarded(id, by, reason, at, idle_before, false).await
    }

    /// The deadline is checked again under the ticket row lock; stale ticker work cannot close
    /// a reopened ticket or a cancelled request.
    pub async fn close_requested_ticket(
        &self,
        id: i64,
        reason: &str,
        at: DateTime<Utc>,
    ) -> anyhow::Result<Option<Ticket>> {
        self.close_ticket_guarded(id, None, Some(reason), at, None, true).await
    }

    async fn close_ticket_guarded(
        &self,
        id: i64,
        by: Option<u64>,
        reason: Option<&str>,
        at: DateTime<Utc>,
        idle_before: Option<DateTime<Utc>>,
        require_request: bool,
    ) -> anyhow::Result<Option<Ticket>> {
        let mut tx = self.pool().begin().await?;
        sqlx::query("SELECT id FROM tickets WHERE id=$1 FOR UPDATE")
            .bind(id)
            .execute(&mut *tx)
            .await?;
        if require_request {
            let due: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM ticket_operations
                WHERE ticket_id=$1 AND close_request_expires_at <= $2)",
            )
            .bind(id)
            .bind(at)
            .fetch_one(&mut *tx)
            .await?;
            if !due {
                return Ok(None);
            }
        }
        let sql = format!(
            "UPDATE tickets t SET status = 'closed', closed_by = $2, close_reason = $3, closed_at = $4, locked = FALSE
             WHERE t.id = $1 AND t.status = 'open' AND ($5::timestamptz IS NULL OR t.last_activity_at < $5)
             RETURNING {TICKET_COLUMNS}"
        );
        let closed = sqlx::query_as::<_, Ticket>(sqlx::AssertSqlSafe(sql))
            .bind(id)
            .bind(by.map(sql_id))
            .bind(reason)
            .bind(at)
            .bind(idle_before)
            .fetch_optional(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(closed)
    }

    /// Opens a closed ticket again, unclaimed, its transcript and rating forgotten (a new one is kept when
    /// it closes again).
    pub async fn reopen_ticket(&self, id: i64, at: DateTime<Utc>) -> anyhow::Result<Option<Ticket>> {
        retry("reopen_ticket", || self.reopen_ticket_once(id, at, None)).await
    }

    pub async fn force_reopen_ticket(&self, id: i64, at: DateTime<Utc>, staff: u64) -> anyhow::Result<Option<Ticket>> {
        retry("force_reopen_ticket", || self.reopen_ticket_once(id, at, Some(staff))).await
    }

    async fn reopen_ticket_once(
        &self,
        id: i64,
        at: DateTime<Utc>,
        staff: Option<u64>,
    ) -> anyhow::Result<Option<Ticket>> {
        let mut ids: Vec<i64> = sqlx::query_scalar(
            "SELECT opener_id FROM tickets WHERE id = $1 UNION SELECT user_id FROM ticket_transcript_authors WHERE ticket_id = $1",
        )
        .bind(id)
        .fetch_all(self.pool())
        .await?;
        ids.extend(staff.map(sql_id));
        let mut tx = self.pool().begin().await?;
        lock_users(&mut tx, &ids).await?;
        let sql = format!(
            "UPDATE tickets t SET status = 'open', claimed_by = NULL, closed_by = NULL, close_reason = NULL,
                 closed_at = NULL, rating = NULL, last_activity_at = $2
             WHERE t.id = $1 AND t.status = 'closed' RETURNING {TICKET_COLUMNS}"
        );
        let ticket = sqlx::query_as::<_, Ticket>(sqlx::AssertSqlSafe(sql))
            .bind(id)
            .bind(at)
            .fetch_optional(&mut *tx)
            .await?;
        if ticket.is_some() {
            if let Some(staff) = staff {
                sqlx::query(
                    "INSERT INTO ticket_operations(ticket_id,force_opened_by) VALUES($1,$2)
                    ON CONFLICT(ticket_id) DO UPDATE SET force_opened_by=$2,updated_at=now()",
                )
                .bind(id)
                .bind(sql_id(staff))
                .execute(&mut *tx)
                .await?;
            }
            sqlx::query(
                "UPDATE ticket_operations SET close_requested_by=NULL,
                close_requested_at=NULL, close_request_expires_at=NULL WHERE ticket_id=$1",
            )
            .bind(id)
            .execute(&mut *tx)
            .await?;
            let current: Vec<i64> =
                sqlx::query_scalar("SELECT user_id FROM ticket_transcript_authors WHERE ticket_id = $1")
                    .bind(id)
                    .fetch_all(&mut *tx)
                    .await?;
            let late: Vec<i64> = current.into_iter().filter(|user| !ids.contains(user)).collect();
            crate::infra::locks::try_lock_users(&mut tx, &late).await?;
            sqlx::query("DELETE FROM ticket_transcript_authors WHERE ticket_id = $1")
                .bind(id)
                .execute(&mut *tx)
                .await?;
            sqlx::query("DELETE FROM ticket_transcripts WHERE ticket_id = $1")
                .bind(id)
                .execute(&mut *tx)
                .await?;
        }
        tx.commit().await?;
        Ok(ticket)
    }

    pub async fn set_ticket_locked(&self, id: i64, locked: bool) -> anyhow::Result<bool> {
        let done = sqlx::query("UPDATE tickets SET locked = $2 WHERE id = $1 AND status = 'open' AND locked <> $2")
            .bind(id)
            .bind(locked)
            .execute(self.pool())
            .await?;
        Ok(done.rows_affected() == 1)
    }

    pub async fn set_ticket_topic(&self, id: i64, topic: &str) -> anyhow::Result<()> {
        sqlx::query("UPDATE tickets SET topic = $2 WHERE id = $1")
            .bind(id)
            .bind(topic)
            .execute(self.pool())
            .await?;
        Ok(())
    }

    /// Somebody wrote in the ticket at `at` (read from the channel's last message): it is not idle.
    pub async fn touch_ticket(&self, id: i64, at: DateTime<Utc>) -> anyhow::Result<()> {
        sqlx::query("UPDATE tickets SET last_activity_at = GREATEST(last_activity_at, $2) WHERE id = $1")
            .bind(id)
            .bind(at)
            .execute(self.pool())
            .await?;
        Ok(())
    }

    /// The opener's 1 to 5 stars for a closed ticket, once.
    pub async fn rate_ticket(&self, id: i64, opener: u64, stars: i16) -> anyhow::Result<bool> {
        let done = sqlx::query(
            "UPDATE tickets SET rating = $3 WHERE id = $1 AND opener_id = $2 AND status = 'closed' AND rating IS NULL",
        )
        .bind(id)
        .bind(sql_id(opener))
        .bind(stars.clamp(1, 5))
        .execute(self.pool())
        .await?;
        Ok(done.rows_affected() == 1)
    }

    /// Keeps the transcript of a closed ticket: the lines, who wrote them (as `(user, name, bot)` in the
    /// order of their index) and when the first answer came.
    pub async fn save_transcript(
        &self,
        ticket: i64,
        lines: &[Line],
        authors: &[(u64, String, bool)],
        with_content: bool,
        first_reply: Option<DateTime<Utc>>,
    ) -> anyhow::Result<()> {
        let json = serde_json::to_string(lines)?;
        retry("save_transcript", || async {
            let mut tx = self.pool().begin().await?;
            let ids: Vec<i64> = authors.iter().map(|(id, _, _)| sql_id(*id)).collect();
            lock_users(&mut tx, &ids).await?;
            sqlx::query("DELETE FROM ticket_transcript_authors WHERE ticket_id = $1")
                .bind(ticket)
                .execute(&mut *tx)
                .await?;
            sqlx::query(
                "INSERT INTO ticket_transcripts (ticket_id, json, messages, with_content) VALUES ($1, $2::jsonb, $3, $4)
                 ON CONFLICT (ticket_id) DO UPDATE SET json = EXCLUDED.json, messages = EXCLUDED.messages,
                     with_content = EXCLUDED.with_content, created_at = now()",
            )
            .bind(ticket)
            .bind(&json)
            .bind(lines.len() as i32)
            .bind(with_content)
            .execute(&mut *tx)
            .await?;
            let names: Vec<&str> = authors.iter().map(|(_, name, _)| name.as_str()).collect();
            let bots: Vec<bool> = authors.iter().map(|(_, _, bot)| *bot).collect();
            sqlx::query(
                "INSERT INTO ticket_transcript_authors (ticket_id, idx, user_id, name, bot)
                 SELECT $1, (n - 1)::SMALLINT, u, name, bot
                 FROM unnest($2::BIGINT[], $3::TEXT[], $4::BOOLEAN[]) WITH ORDINALITY AS a(u, name, bot, n)",
            )
            .bind(ticket)
            .bind(&ids)
            .bind(&names)
            .bind(&bots)
            .execute(&mut *tx)
            .await?;
            sqlx::query("UPDATE tickets SET first_reply_at = $2 WHERE id = $1")
                .bind(ticket)
                .bind(first_reply)
                .execute(&mut *tx)
                .await?;
            tx.commit().await?;
            Ok(())
        })
        .await
    }

    /// A closed ticket's transcript; the authors who deleted their data are there without a name.
    pub async fn transcript(&self, ticket: i64) -> anyhow::Result<Option<StoredTranscript>> {
        let row: Option<(String, i32, bool)> =
            sqlx::query_as("SELECT json::TEXT, messages, with_content FROM ticket_transcripts WHERE ticket_id = $1")
                .bind(ticket)
                .fetch_optional(self.pool())
                .await?;
        let Some((json, messages, with_content)) = row else {
            return Ok(None);
        };
        let lines: Vec<Line> = serde_json::from_str(&json)?;
        let rows: Vec<(i16, i64, String, bool)> = sqlx::query_as(
            "SELECT idx, user_id, name, bot FROM ticket_transcript_authors WHERE ticket_id = $1 ORDER BY idx",
        )
        .bind(ticket)
        .fetch_all(self.pool())
        .await?;
        let author_ids = rows.iter().map(|(idx, id, _, _)| (*idx as u16, *id as u64)).collect();
        let mut authors: Vec<Author> = rows
            .into_iter()
            .map(|(idx, _, name, bot)| Author {
                idx: idx as u16,
                name: Some(name),
                bot,
            })
            .collect();
        for line in &lines {
            if !authors.iter().any(|a| a.idx == line.a) {
                authors.push(Author {
                    idx: line.a,
                    name: None,
                    bot: false,
                });
            }
        }
        Ok(Some(StoredTranscript {
            lines,
            authors,
            author_ids,
            with_content,
            messages,
        }))
    }

    /// The server's tickets, newest first, `limit` at a time.
    pub async fn ticket_list(&self, guild: u64, filter: &TicketFilter, limit: i64) -> anyhow::Result<Vec<Ticket>> {
        let text = filter
            .text
            .as_deref()
            .map(str::trim)
            .filter(|t| !t.is_empty())
            .map(|t| format!("%{}%", t.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_")));
        let sql = format!(
            "SELECT {TICKET_COLUMNS} FROM tickets t
             WHERE t.guild_id = $1
               AND ($2::TEXT IS NULL
                    OR ($2 = 'open' AND t.status = 'open' AND t.claimed_by IS NULL)
                    OR ($2 = 'claimed' AND t.status = 'open' AND t.claimed_by IS NOT NULL)
                    OR ($2 = 'closed' AND t.status = 'closed')
                    OR $2 NOT IN ('open', 'claimed', 'closed'))
               AND ($3::BIGINT IS NULL OR t.claimed_by = $3 OR t.closed_by = $3)
               AND ($4::BIGINT IS NULL OR t.opener_id = $4)
               AND ($5::TEXT IS NULL OR t.topic ILIKE $5 OR t.close_reason ILIKE $5 OR t.number::TEXT = $6)
               AND ($7::BIGINT IS NULL OR t.id < $7)
             ORDER BY t.id DESC LIMIT $8"
        );
        Ok(sqlx::query_as::<_, Ticket>(sqlx::AssertSqlSafe(sql))
            .bind(sql_id(guild))
            .bind(filter.state.as_deref())
            .bind(filter.staff.map(sql_id))
            .bind(filter.opener.map(sql_id))
            .bind(text)
            .bind(filter.text.as_deref().map(|t| t.trim().trim_start_matches('#')))
            .bind(filter.before)
            .bind(limit)
            .fetch_all(self.pool())
            .await?)
    }

    /// Tickets opened and closed since `since`, how soon the first answers came and the ratings.
    pub async fn ticket_summary(&self, guild: u64, since: DateTime<Utc>) -> anyhow::Result<TicketSummary> {
        Ok(sqlx::query_as::<_, TicketSummary>(
            "SELECT count(*) FILTER (WHERE opened_at >= $2) AS opened,
                    count(*) FILTER (WHERE closed_at >= $2) AS closed,
                    count(*) FILTER (WHERE status = 'open') AS open_now,
                    (percentile_cont(0.5) WITHIN GROUP (ORDER BY extract(epoch FROM first_reply_at - opened_at))
                        FILTER (WHERE opened_at >= $2 AND first_reply_at IS NOT NULL))::FLOAT8
                        AS median_first_reply_secs,
                    count(rating) FILTER (WHERE closed_at >= $2) AS ratings,
                    (avg(rating) FILTER (WHERE closed_at >= $2))::FLOAT8 AS rating_avg
             FROM tickets WHERE guild_id = $1",
        )
        .bind(sql_id(guild))
        .bind(since)
        .fetch_one(self.pool())
        .await?)
    }

    pub async fn open_ticket_count(&self, guild: u64) -> anyhow::Result<i64> {
        let (n,): (i64,) = sqlx::query_as("SELECT count(*) FROM tickets WHERE guild_id = $1 AND status = 'open'")
            .bind(sql_id(guild))
            .fetch_one(self.pool())
            .await?;
        Ok(n)
    }

    /// Open tickets whose last known activity is older than their server's auto-close allows, oldest first.
    pub async fn idle_tickets(&self, now: DateTime<Utc>, limit: i64) -> anyhow::Result<Vec<IdleTicket>> {
        Ok(sqlx::query_as::<_, IdleTicket>(
            "SELECT t.id, t.guild_id, COALESCE(t.thread_id, t.channel_id) AS place, t.last_activity_at,
                    s.auto_close_hours
             FROM tickets t JOIN ticket_settings s ON s.guild_id = t.guild_id
             WHERE t.status = 'open' AND s.auto_close_hours > 0 AND COALESCE(t.thread_id, t.channel_id) IS NOT NULL
               AND t.last_activity_at < $1 - make_interval(hours => s.auto_close_hours)
             ORDER BY t.last_activity_at LIMIT $2",
        )
        .bind(now)
        .bind(limit)
        .fetch_all(self.pool())
        .await?)
    }
}

/// How long ago a ticket counts as idle, for a server's auto-close hours.
pub fn idle_cutoff(now: DateTime<Utc>, hours: i32) -> DateTime<Utc> {
    now - Duration::hours(i64::from(hours))
}

mod panels;

mod panel_records;
pub use panel_records::{PanelWrite, TicketPanel, TicketPanelSaved};

use panel_records::{PANEL_COLUMNS, RawPanel};
