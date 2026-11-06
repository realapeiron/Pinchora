//! Suggestions: the channel they go to, the votes and the managers' decisions.

use chrono::{DateTime, Utc};

use crate::{
    Db,
    locks::{lock_users, retry},
    sql_id,
};

pub const OPEN: &str = "open";
pub const APPROVED: &str = "approved";
pub const DENIED: &str = "denied";

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct SuggestionRow {
    pub id: i64,
    pub guild_id: i64,
    /// The server's own count, from 1.
    pub number: i32,
    pub author_id: i64,
    pub body: String,
    pub channel_id: Option<i64>,
    pub message_id: Option<i64>,
    pub status: String,
    pub reason: Option<String>,
    pub decided_by: Option<i64>,
    pub decided_at: Option<DateTime<Utc>>,
    pub up: i64,
    pub down: i64,
}

impl SuggestionRow {
    pub fn is_open(&self) -> bool {
        self.status == OPEN
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SuggestionVote {
    /// A first vote.
    Voted,
    /// The member moved to the other side.
    Switched,
    /// The member clicked their own side again and took the vote back.
    Withdrawn,
    /// Approved or denied already: no more votes.
    Closed,
}

#[derive(Debug, Clone)]
pub enum Decision {
    Done(SuggestionRow),
    /// No such suggestion in this server.
    Missing,
    /// Decided before; holds the status it has.
    Already(String),
}

macro_rules! columns {
    () => {
        "s.id, s.guild_id, s.number, s.author_id, s.body, s.channel_id, s.message_id, s.status, s.reason, \
         s.decided_by, s.decided_at, \
         (SELECT count(*) FROM suggestion_votes v WHERE v.suggestion_id = s.id AND v.up) AS up, \
         (SELECT count(*) FROM suggestion_votes v WHERE v.suggestion_id = s.id AND NOT v.up) AS down"
    };
}

impl Db {
    /// The channel `/suggest` posts to, if the server set one.
    pub async fn suggestion_channel(&self, guild_id: u64) -> anyhow::Result<Option<u64>> {
        let row: Option<(Option<i64>,)> =
            sqlx::query_as("SELECT channel_id FROM suggestion_settings WHERE guild_id = $1")
                .bind(sql_id(guild_id))
                .fetch_optional(self.pool())
                .await?;
        Ok(row.and_then(|(channel,)| channel).map(|c| c as u64))
    }

    /// Sets or clears (`None`) the suggestion channel. The server's numbering is kept.
    pub async fn set_suggestion_channel(&self, guild_id: u64, channel_id: Option<u64>) -> anyhow::Result<()> {
        sqlx::query(
            "INSERT INTO suggestion_settings (guild_id, channel_id) VALUES ($1, $2)
             ON CONFLICT (guild_id) DO UPDATE SET channel_id = EXCLUDED.channel_id",
        )
        .bind(sql_id(guild_id))
        .bind(channel_id.map(sql_id))
        .execute(self.pool())
        .await?;
        Ok(())
    }

    /// Files a suggestion under the server's next number.
    pub async fn create_suggestion(&self, guild_id: u64, author: u64, body: &str) -> anyhow::Result<SuggestionRow> {
        retry("create_suggestion", || {
            self.create_suggestion_once(guild_id, author, body)
        })
        .await
    }

    /// One attempt of [`Db::create_suggestion`].
    async fn create_suggestion_once(&self, guild_id: u64, author: u64, body: &str) -> anyhow::Result<SuggestionRow> {
        let mut tx = self.pool().begin().await?;
        lock_users(&mut tx, &[sql_id(author)]).await?;
        let (number,): (i32,) = sqlx::query_as(
            "INSERT INTO suggestion_settings (guild_id, last_number) VALUES ($1, 1)
             ON CONFLICT (guild_id) DO UPDATE SET last_number = suggestion_settings.last_number + 1
             RETURNING last_number",
        )
        .bind(sql_id(guild_id))
        .fetch_one(&mut *tx)
        .await?;
        let (id,): (i64,) = sqlx::query_as(
            "INSERT INTO suggestions (guild_id, number, author_id, body) VALUES ($1, $2, $3, $4) RETURNING id",
        )
        .bind(sql_id(guild_id))
        .bind(number)
        .bind(sql_id(author))
        .bind(body)
        .fetch_one(&mut *tx)
        .await?;
        let row =
            sqlx::query_as::<_, SuggestionRow>(concat!("SELECT ", columns!(), " FROM suggestions s WHERE s.id = $1"))
                .bind(id)
                .fetch_one(&mut *tx)
                .await?;
        tx.commit().await?;
        Ok(row)
    }

    pub async fn suggestion(&self, id: i64) -> anyhow::Result<Option<SuggestionRow>> {
        Ok(
            sqlx::query_as::<_, SuggestionRow>(concat!("SELECT ", columns!(), " FROM suggestions s WHERE s.id = $1"))
                .bind(id)
                .fetch_optional(self.pool())
                .await?,
        )
    }

    pub async fn set_suggestion_message(&self, id: i64, channel_id: u64, message_id: u64) -> anyhow::Result<()> {
        sqlx::query("UPDATE suggestions SET channel_id = $2, message_id = $3 WHERE id = $1")
            .bind(id)
            .bind(sql_id(channel_id))
            .bind(sql_id(message_id))
            .execute(self.pool())
            .await?;
        Ok(())
    }

    /// Takes back a suggestion whose message could not be posted.
    pub async fn abort_suggestion(&self, id: i64) -> anyhow::Result<()> {
        sqlx::query("DELETE FROM suggestions WHERE id = $1 AND message_id IS NULL")
            .bind(id)
            .execute(self.pool())
            .await?;
        Ok(())
    }

    /// Records a thumbs-up (`up`) or thumbs-down: a first vote, a switch to the other side, or, when
    /// it is the member's current side, taking the vote back. The row is share-locked, so a vote
    /// never lands after a manager decided.
    pub async fn vote_suggestion(&self, id: i64, user: u64, up: bool) -> anyhow::Result<SuggestionVote> {
        retry("vote_suggestion", || self.vote_suggestion_once(id, user, up)).await
    }

    /// One attempt of [`Db::vote_suggestion`].
    async fn vote_suggestion_once(&self, id: i64, user: u64, up: bool) -> anyhow::Result<SuggestionVote> {
        let mut tx = self.pool().begin().await?;
        lock_users(&mut tx, &[sql_id(user)]).await?;
        let open: Option<(bool,)> = sqlx::query_as("SELECT status = 'open' FROM suggestions WHERE id = $1 FOR SHARE")
            .bind(id)
            .fetch_optional(&mut *tx)
            .await?;
        if open != Some((true,)) {
            return Ok(SuggestionVote::Closed);
        }
        let before: Option<(bool,)> =
            sqlx::query_as("SELECT up FROM suggestion_votes WHERE suggestion_id = $1 AND user_id = $2")
                .bind(id)
                .bind(sql_id(user))
                .fetch_optional(&mut *tx)
                .await?;
        let outcome = match before {
            Some((side,)) if side == up => {
                sqlx::query("DELETE FROM suggestion_votes WHERE suggestion_id = $1 AND user_id = $2")
                    .bind(id)
                    .bind(sql_id(user))
                    .execute(&mut *tx)
                    .await?;
                SuggestionVote::Withdrawn
            }
            other => {
                sqlx::query(
                    "INSERT INTO suggestion_votes (suggestion_id, user_id, up) VALUES ($1, $2, $3)
                     ON CONFLICT (suggestion_id, user_id) DO UPDATE SET up = EXCLUDED.up",
                )
                .bind(id)
                .bind(sql_id(user))
                .bind(up)
                .execute(&mut *tx)
                .await?;
                if other.is_some() {
                    SuggestionVote::Switched
                } else {
                    SuggestionVote::Voted
                }
            }
        };
        tx.commit().await?;
        Ok(outcome)
    }

    /// Approves or denies the server's suggestion number `number`, once.
    pub async fn decide_suggestion(
        &self,
        guild_id: u64,
        number: i32,
        approve: bool,
        by: u64,
        reason: Option<&str>,
    ) -> anyhow::Result<Decision> {
        retry("decide_suggestion", || {
            self.decide_suggestion_once(guild_id, number, approve, by, reason)
        })
        .await
    }

    /// One attempt of [`Db::decide_suggestion`].
    async fn decide_suggestion_once(
        &self,
        guild_id: u64,
        number: i32,
        approve: bool,
        by: u64,
        reason: Option<&str>,
    ) -> anyhow::Result<Decision> {
        let mut tx = self.pool().begin().await?;
        // The deciding manager is named on the row (a delete of their data clears that).
        lock_users(&mut tx, &[sql_id(by)]).await?;
        let found: Option<(i64, String)> =
            sqlx::query_as("SELECT id, status FROM suggestions WHERE guild_id = $1 AND number = $2 FOR UPDATE")
                .bind(sql_id(guild_id))
                .bind(number)
                .fetch_optional(&mut *tx)
                .await?;
        let Some((id, status)) = found else {
            return Ok(Decision::Missing);
        };
        if status != OPEN {
            return Ok(Decision::Already(status));
        }
        sqlx::query(
            "UPDATE suggestions SET status = $2, reason = $3, decided_by = $4, decided_at = now() WHERE id = $1",
        )
        .bind(id)
        .bind(if approve { APPROVED } else { DENIED })
        .bind(reason)
        .bind(sql_id(by))
        .execute(&mut *tx)
        .await?;
        let row =
            sqlx::query_as::<_, SuggestionRow>(concat!("SELECT ", columns!(), " FROM suggestions s WHERE s.id = $1"))
                .bind(id)
                .fetch_one(&mut *tx)
                .await?;
        tx.commit().await?;
        Ok(Decision::Done(row))
    }
}
