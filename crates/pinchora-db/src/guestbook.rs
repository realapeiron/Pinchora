use chrono::{DateTime, NaiveDate, Utc};

use crate::{Db, sql_id};

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct NoteRow {
    pub id: i64,
    pub author_id: i64,
    pub author_name: String,
    pub body: String,
    pub created_at: DateTime<Utc>,
}

/// Someone who visited a house, and when they last came.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct VisitorRow {
    pub guest_id: i64,
    pub visited_at: DateTime<Utc>,
}

/// Whether a visitor may sign a guestbook, and what happened when they tried.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignOutcome {
    Signed,
    /// One note per visitor, host and day.
    AlreadyToday,
    Locked,
    Blocked,
    /// Only someone who visited in the last day can sign.
    NoVisit,
}

impl Db {
    /// Why `author` cannot sign `host`'s guestbook on `day`, or `None` when they can.
    pub async fn guestbook_refusal(
        &self,
        host: u64,
        author: u64,
        day: NaiveDate,
    ) -> anyhow::Result<Option<SignOutcome>> {
        let (locked, blocked, visited, signed): (bool, bool, bool, bool) = sqlx::query_as(
            "SELECT
                 COALESCE((SELECT locked FROM houses WHERE user_id = $1), FALSE),
                 EXISTS (SELECT 1 FROM guestbook_blocks WHERE host_id = $1 AND blocked_id = $2),
                 EXISTS (SELECT 1 FROM visits WHERE host_id = $1 AND guest_id = $2
                         AND visited_at > now() - interval '1 day'),
                 EXISTS (SELECT 1 FROM guestbook_notes WHERE host_id = $1 AND author_id = $2 AND day = $3)",
        )
        .bind(sql_id(host))
        .bind(sql_id(author))
        .bind(day)
        .fetch_one(self.pool())
        .await?;
        Ok(if locked {
            Some(SignOutcome::Locked)
        } else if blocked {
            Some(SignOutcome::Blocked)
        } else if !visited {
            Some(SignOutcome::NoVisit)
        } else if signed {
            Some(SignOutcome::AlreadyToday)
        } else {
            None
        })
    }

    /// Writes a note (already cleaned) when the rules allow it.
    pub async fn sign_guestbook(
        &self,
        host: u64,
        author: u64,
        author_name: &str,
        body: &str,
        day: NaiveDate,
    ) -> anyhow::Result<SignOutcome> {
        if let Some(refusal) = self.guestbook_refusal(host, author, day).await? {
            return Ok(refusal);
        }
        let written = sqlx::query(
            "INSERT INTO guestbook_notes (host_id, author_id, author_name, day, body) VALUES ($1, $2, $3, $4, $5)
             ON CONFLICT (host_id, author_id, day) DO NOTHING",
        )
        .bind(sql_id(host))
        .bind(sql_id(author))
        .bind(author_name)
        .bind(day)
        .bind(body)
        .execute(self.pool())
        .await?
        .rows_affected();
        Ok(if written == 1 {
            SignOutcome::Signed
        } else {
            SignOutcome::AlreadyToday
        })
    }

    /// One page of a guestbook, newest first, and how many notes it has in all.
    pub async fn guestbook(&self, host: u64, offset: i64, limit: i64) -> anyhow::Result<(Vec<NoteRow>, i64)> {
        let notes = sqlx::query_as::<_, NoteRow>(
            "SELECT id, author_id, author_name, body, created_at FROM guestbook_notes
             WHERE host_id = $1 ORDER BY id DESC LIMIT $2 OFFSET $3",
        )
        .bind(sql_id(host))
        .bind(limit)
        .bind(offset)
        .fetch_all(self.pool())
        .await?;
        let (total,): (i64,) = sqlx::query_as("SELECT count(*) FROM guestbook_notes WHERE host_id = $1")
            .bind(sql_id(host))
            .fetch_one(self.pool())
            .await?;
        Ok((notes, total))
    }

    /// The newest note the host has read: notes with a higher id are new to them.
    pub async fn guestbook_seen(&self, host: u64) -> anyhow::Result<i64> {
        let row: Option<(i64,)> = sqlx::query_as("SELECT guestbook_seen FROM houses WHERE user_id = $1")
            .bind(sql_id(host))
            .fetch_optional(self.pool())
            .await?;
        Ok(row.map_or(0, |(seen,)| seen))
    }

    /// Notes the host has not read yet.
    pub async fn guestbook_unread(&self, host: u64) -> anyhow::Result<i64> {
        let (n,): (i64,) = sqlx::query_as(
            "SELECT count(*) FROM guestbook_notes n JOIN houses h ON h.user_id = n.host_id
             WHERE n.host_id = $1 AND n.id > h.guestbook_seen",
        )
        .bind(sql_id(host))
        .fetch_one(self.pool())
        .await?;
        Ok(n)
    }

    /// Marks every note so far as read.
    pub async fn mark_guestbook_read(&self, host: u64) -> anyhow::Result<()> {
        sqlx::query(
            "UPDATE houses SET guestbook_seen = GREATEST(guestbook_seen,
                 COALESCE((SELECT max(id) FROM guestbook_notes WHERE host_id = $1), 0))
             WHERE user_id = $1",
        )
        .bind(sql_id(host))
        .execute(self.pool())
        .await?;
        Ok(())
    }

    /// Deletes one note from the host's guestbook; returns it if it was there.
    pub async fn delete_note(&self, host: u64, note: i64) -> anyhow::Result<Option<NoteRow>> {
        Ok(sqlx::query_as::<_, NoteRow>(
            "DELETE FROM guestbook_notes WHERE id = $1 AND host_id = $2
             RETURNING id, author_id, author_name, body, created_at",
        )
        .bind(note)
        .bind(sql_id(host))
        .fetch_optional(self.pool())
        .await?)
    }

    /// Blocks a signer and removes their notes; returns how many notes went.
    pub async fn block_signer(&self, host: u64, signer: u64, name: &str) -> anyhow::Result<u64> {
        let (host, signer) = (sql_id(host), sql_id(signer));
        let mut tx = self.pool().begin().await?;
        sqlx::query(
            "INSERT INTO guestbook_blocks (host_id, blocked_id, blocked_name) VALUES ($1, $2, $3)
             ON CONFLICT (host_id, blocked_id) DO NOTHING",
        )
        .bind(host)
        .bind(signer)
        .bind(name)
        .execute(&mut *tx)
        .await?;
        let removed = sqlx::query("DELETE FROM guestbook_notes WHERE host_id = $1 AND author_id = $2")
            .bind(host)
            .bind(signer)
            .execute(&mut *tx)
            .await?
            .rows_affected();
        tx.commit().await?;
        Ok(removed)
    }

    /// Lets a blocked signer sign again; false if they were not blocked.
    pub async fn unblock_signer(&self, host: u64, signer: u64) -> anyhow::Result<bool> {
        let n = sqlx::query("DELETE FROM guestbook_blocks WHERE host_id = $1 AND blocked_id = $2")
            .bind(sql_id(host))
            .bind(sql_id(signer))
            .execute(self.pool())
            .await?
            .rows_affected();
        Ok(n == 1)
    }

    /// Signers the host blocked, as (id, name), most recent first.
    pub async fn guestbook_blocks(&self, host: u64) -> anyhow::Result<Vec<(i64, String)>> {
        Ok(sqlx::query_as(
            "SELECT blocked_id, blocked_name FROM guestbook_blocks WHERE host_id = $1 ORDER BY blocked_at DESC",
        )
        .bind(sql_id(host))
        .fetch_all(self.pool())
        .await?)
    }

    /// The last `limit` different visitors of a house, most recent first.
    pub async fn recent_visitors(&self, host: u64, limit: i64) -> anyhow::Result<Vec<VisitorRow>> {
        Ok(sqlx::query_as::<_, VisitorRow>(
            "SELECT guest_id, max(visited_at) AS visited_at FROM visits WHERE host_id = $1
             GROUP BY guest_id ORDER BY max(visited_at) DESC LIMIT $2",
        )
        .bind(sql_id(host))
        .bind(limit)
        .fetch_all(self.pool())
        .await?)
    }
}
