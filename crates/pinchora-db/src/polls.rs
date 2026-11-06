//! Polls: one vote per member, closed once by the ticker.

use chrono::{DateTime, Utc};

use pinchora_core::modules::Module;

use crate::{
    Db,
    giveaways::{Due, not_paused},
    locks::{lock_users, retry},
    sql_id,
};

pub const OPEN: &str = "open";

/// Most polls one member can have open in a server.
pub const MAX_OPEN_POLLS: i64 = 3;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct PollRow {
    pub id: i64,
    pub guild_id: i64,
    pub channel_id: i64,
    pub message_id: Option<i64>,
    pub author_id: i64,
    pub question: String,
    pub options: Vec<String>,
    pub ends_at: DateTime<Utc>,
    pub status: String,
    pub announced: bool,
}

impl PollRow {
    pub fn is_open(&self) -> bool {
        self.status == OPEN
    }
}

#[derive(Debug, Clone, Copy)]
pub struct NewPoll<'a> {
    pub guild_id: u64,
    pub channel_id: u64,
    pub author: u64,
    pub question: &'a str,
    pub options: &'a [String],
    pub ends_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub enum PollOpen {
    Started(PollRow),
    /// The author already has [`MAX_OPEN_POLLS`] open here.
    TooMany,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PollVote {
    /// A first vote.
    Cast,
    /// The member moved their vote to another option.
    Changed,
    /// The member clicked their own option again and took the vote back.
    Withdrawn,
    /// Over, or past its end time.
    Closed,
}

macro_rules! columns {
    () => {
        "id, guild_id, channel_id, message_id, author_id, question, options, ends_at, status, announced"
    };
}

impl Db {
    /// Starts a poll, unless its author already has [`MAX_OPEN_POLLS`] open in this server.
    pub async fn create_poll(&self, new: NewPoll<'_>) -> anyhow::Result<PollOpen> {
        retry("create_poll", || self.create_poll_once(new)).await
    }

    /// One attempt of [`Db::create_poll`].
    async fn create_poll_once(&self, new: NewPoll<'_>) -> anyhow::Result<PollOpen> {
        let mut tx = self.pool().begin().await?;
        // Two polls started at once by one member must not both slip under the limit: the author's
        // lock makes them take turns.
        lock_users(&mut tx, &[sql_id(new.author)]).await?;
        let (open,): (i64,) =
            sqlx::query_as("SELECT count(*) FROM polls WHERE guild_id = $1 AND author_id = $2 AND status = 'open'")
                .bind(sql_id(new.guild_id))
                .bind(sql_id(new.author))
                .fetch_one(&mut *tx)
                .await?;
        if open >= MAX_OPEN_POLLS {
            return Ok(PollOpen::TooMany);
        }
        let row = sqlx::query_as::<_, PollRow>(concat!(
            "INSERT INTO polls (guild_id, channel_id, author_id, question, options, ends_at)
             VALUES ($1, $2, $3, $4, $5, $6) RETURNING ",
            columns!()
        ))
        .bind(sql_id(new.guild_id))
        .bind(sql_id(new.channel_id))
        .bind(sql_id(new.author))
        .bind(new.question)
        .bind(new.options)
        .bind(new.ends_at)
        .fetch_one(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(PollOpen::Started(row))
    }

    pub async fn poll(&self, id: i64) -> anyhow::Result<Option<PollRow>> {
        Ok(
            sqlx::query_as::<_, PollRow>(concat!("SELECT ", columns!(), " FROM polls WHERE id = $1"))
                .bind(id)
                .fetch_optional(self.pool())
                .await?,
        )
    }

    pub async fn set_poll_message(&self, id: i64, channel_id: u64, message_id: u64) -> anyhow::Result<()> {
        sqlx::query("UPDATE polls SET channel_id = $2, message_id = $3 WHERE id = $1")
            .bind(id)
            .bind(sql_id(channel_id))
            .bind(sql_id(message_id))
            .execute(self.pool())
            .await?;
        Ok(())
    }

    /// Takes back a poll whose message could not be posted.
    pub async fn abort_poll(&self, id: i64) -> anyhow::Result<()> {
        sqlx::query("DELETE FROM polls WHERE id = $1 AND status = 'open'")
            .bind(id)
            .execute(self.pool())
            .await?;
        Ok(())
    }

    /// Records a vote for option `choice`: a first vote, a move to another option, or, when it is
    /// the member's current option, taking the vote back. The poll row is share-locked, so a vote
    /// never lands after the poll closed.
    pub async fn vote_poll(&self, id: i64, user: u64, choice: usize, now: DateTime<Utc>) -> anyhow::Result<PollVote> {
        retry("vote_poll", || self.vote_poll_once(id, user, choice, now)).await
    }

    /// One attempt of [`Db::vote_poll`].
    async fn vote_poll_once(&self, id: i64, user: u64, choice: usize, now: DateTime<Utc>) -> anyhow::Result<PollVote> {
        let mut tx = self.pool().begin().await?;
        lock_users(&mut tx, &[sql_id(user)]).await?;
        let open: Option<(bool,)> =
            sqlx::query_as("SELECT status = 'open' AND ends_at > $2 FROM polls WHERE id = $1 FOR SHARE")
                .bind(id)
                .bind(now)
                .fetch_optional(&mut *tx)
                .await?;
        if open != Some((true,)) {
            return Ok(PollVote::Closed);
        }
        let choice = choice as i16;
        let before: Option<(i16,)> =
            sqlx::query_as("SELECT choice FROM poll_votes WHERE poll_id = $1 AND user_id = $2")
                .bind(id)
                .bind(sql_id(user))
                .fetch_optional(&mut *tx)
                .await?;
        let outcome = match before {
            Some((old,)) if old == choice => {
                sqlx::query("DELETE FROM poll_votes WHERE poll_id = $1 AND user_id = $2")
                    .bind(id)
                    .bind(sql_id(user))
                    .execute(&mut *tx)
                    .await?;
                PollVote::Withdrawn
            }
            other => {
                sqlx::query(
                    "INSERT INTO poll_votes (poll_id, user_id, choice) VALUES ($1, $2, $3)
                     ON CONFLICT (poll_id, user_id) DO UPDATE SET choice = EXCLUDED.choice",
                )
                .bind(id)
                .bind(sql_id(user))
                .bind(choice)
                .execute(&mut *tx)
                .await?;
                if other.is_some() {
                    PollVote::Changed
                } else {
                    PollVote::Cast
                }
            }
        };
        tx.commit().await?;
        Ok(outcome)
    }

    /// Votes per option, in option order, for a poll of `options` options.
    pub async fn poll_tally(&self, id: i64, options: usize) -> anyhow::Result<Vec<i64>> {
        let rows: Vec<(i16, i64)> =
            sqlx::query_as("SELECT choice, count(*) FROM poll_votes WHERE poll_id = $1 GROUP BY choice")
                .bind(id)
                .fetch_all(self.pool())
                .await?;
        let mut tally = vec![0; options];
        for (choice, votes) in rows {
            if let Some(slot) = usize::try_from(choice).ok().and_then(|c| tally.get_mut(c)) {
                *slot = votes;
            }
        }
        Ok(tally)
    }

    /// The option the member voted for, if they did.
    pub async fn poll_choice(&self, id: i64, user: u64) -> anyhow::Result<Option<usize>> {
        let row: Option<(i16,)> = sqlx::query_as("SELECT choice FROM poll_votes WHERE poll_id = $1 AND user_id = $2")
            .bind(id)
            .bind(sql_id(user))
            .fetch_optional(self.pool())
            .await?;
        Ok(row.and_then(|(c,)| usize::try_from(c).ok()))
    }

    /// Open polls whose time is up at `now`, soonest first, with the server and channel they live in. Ones
    /// paused by their module are left out, as in [`Db::due_giveaways`].
    pub async fn due_polls(&self, now: DateTime<Utc>, limit: i64) -> anyhow::Result<Vec<Due>> {
        Ok(sqlx::query_as(concat!(
            "SELECT p.id, p.guild_id, p.channel_id FROM polls p
             WHERE p.status = 'open' AND p.ends_at <= $1 AND ",
            not_paused!("p"),
            " ORDER BY p.ends_at LIMIT $2"
        ))
        .bind(now)
        .bind(limit)
        .bind(Module::Polls.key())
        .fetch_all(self.pool())
        .await?)
    }

    /// Closes a poll that is due at `now`, once; `None` when somebody closed it first.
    pub async fn close_poll(&self, id: i64, now: DateTime<Utc>) -> anyhow::Result<Option<PollRow>> {
        Ok(sqlx::query_as::<_, PollRow>(concat!(
            "UPDATE polls SET status = 'closed', closed_at = $2
             WHERE id = $1 AND status = 'open' AND ends_at <= $2 RETURNING ",
            columns!()
        ))
        .bind(id)
        .bind(now)
        .fetch_optional(self.pool())
        .await?)
    }

    /// Closed polls whose channel has not been told yet, oldest first.
    pub async fn unannounced_polls(&self, limit: i64) -> anyhow::Result<Vec<PollRow>> {
        Ok(sqlx::query_as::<_, PollRow>(concat!(
            "SELECT ",
            columns!(),
            " FROM polls WHERE status = 'closed' AND NOT announced ORDER BY id LIMIT $1"
        ))
        .bind(limit)
        .fetch_all(self.pool())
        .await?)
    }

    /// Takes the job of telling the channel how poll `id` closed, like
    /// [`Db::claim_giveaway_announcement`].
    pub async fn claim_poll_announcement(&self, id: i64, now: DateTime<Utc>) -> anyhow::Result<bool> {
        Ok(sqlx::query(
            "UPDATE polls SET announce_claimed = $2
             WHERE id = $1 AND status = 'closed' AND NOT announced
               AND (announce_claimed IS NULL OR announce_claimed < $2 - interval '2 minutes')",
        )
        .bind(id)
        .bind(now)
        .execute(self.pool())
        .await?
        .rows_affected()
            > 0)
    }

    pub async fn mark_poll_announced(&self, id: i64) -> anyhow::Result<()> {
        sqlx::query("UPDATE polls SET announced = TRUE WHERE id = $1")
            .bind(id)
            .execute(self.pool())
            .await?;
        Ok(())
    }

    /// Drops polls that closed before `before` (and their votes); returns how many. Rows somebody
    /// holds are left for the next sweep (see the `locks` module).
    pub async fn forget_old_polls(&self, before: DateTime<Utc>) -> anyhow::Result<u64> {
        retry("forget_old_polls", || async move {
            Ok(sqlx::query(
                "WITH old AS MATERIALIZED (
                     SELECT id FROM polls WHERE status = 'closed' AND announced AND closed_at < $1
                     FOR UPDATE SKIP LOCKED)
                 DELETE FROM polls WHERE id IN (SELECT id FROM old)",
            )
            .bind(before)
            .execute(self.pool())
            .await?
            .rows_affected())
        })
        .await
    }
}
