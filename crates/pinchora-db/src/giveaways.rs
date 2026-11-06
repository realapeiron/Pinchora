//! Giveaways: entries, the shells prize held from the host, and ending exactly once.

use chrono::{DateTime, Utc};
use pinchora_core::modules::Module;
use sqlx::{Postgres, Transaction};

use crate::{Db, economy::ledger, sql_id};

pub const RUNNING: &str = "running";
pub const ENDED: &str = "ended";

/// Most giveaways one server can run at once.
pub const MAX_RUNNING_GIVEAWAYS: i64 = 20;

/// SQL that holds for a row of `$table` (with `guild_id` and `channel_id`) unless the module named by `$3`
/// is switched off for its server, or its channel is on the module's `except` list.
macro_rules! not_paused {
    ($table:literal) => {
        concat!(
            "NOT EXISTS (SELECT 1 FROM guild_modules m WHERE m.guild_id = ",
            $table,
            ".guild_id AND m.module = $3 AND (NOT m.enabled OR (m.channel_mode = 'except' AND ",
            $table,
            ".channel_id = ANY(m.channels))))"
        )
    };
}
pub(crate) use not_paused;

/// A giveaway or poll whose time is up, and where it lives.
#[derive(Debug, Clone, Copy, sqlx::FromRow)]
pub struct Due {
    pub id: i64,
    pub guild_id: i64,
    pub channel_id: i64,
}

/// A giveaway, running or over.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct GiveawayRow {
    pub id: i64,
    pub guild_id: i64,
    pub channel_id: i64,
    pub message_id: Option<i64>,
    pub host_id: i64,
    pub prize: String,
    /// Shells each winner is paid; 0 when the prize is something else.
    pub shells: i64,
    pub winners: i32,
    pub required_role_id: Option<i64>,
    pub ends_at: DateTime<Utc>,
    /// When it actually ended: sooner than `ends_at` for a giveaway a manager ended early.
    pub ended_at: Option<DateTime<Utc>>,
    pub status: String,
    /// Everyone who has won, rerolls included.
    pub winner_ids: Vec<i64>,
    /// Whether the channel has been told how it ended.
    pub announced: bool,
    pub entries: i64,
}

impl GiveawayRow {
    pub fn is_running(&self) -> bool {
        self.status == RUNNING
    }

    /// Shells held from the host while the giveaway runs.
    pub fn escrow(&self) -> i64 {
        self.shells * i64::from(self.winners)
    }
}

/// A giveaway to start.
#[derive(Debug, Clone, Copy)]
pub struct NewGiveaway<'a> {
    pub guild_id: u64,
    pub channel_id: u64,
    pub host: u64,
    pub prize: &'a str,
    /// Shells each winner is paid, held from the host up front.
    pub shells: i64,
    pub winners: i32,
    pub required_role: Option<u64>,
    pub ends_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub enum GiveawayOpen {
    Started(Box<GiveawayRow>),
    /// The host cannot cover the shells prize.
    Broke {
        balance: i64,
    },
    /// The server already runs [`MAX_RUNNING_GIVEAWAYS`].
    Full,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryToggle {
    /// The member is in; holds the entries now.
    Entered(i64),
    /// The member took their entry back; holds the entries now.
    Left(i64),
    /// Over, or past its end time.
    Closed,
}

/// How a giveaway ended.
#[derive(Debug, Clone)]
pub struct EndedGiveaway {
    pub giveaway: GiveawayRow,
    pub winners: Vec<u64>,
    /// Shells given back to the host: the prize nobody won.
    pub refunded: i64,
}

#[derive(Debug, Clone)]
pub enum Reroll {
    Done(Box<GiveawayRow>, Vec<u64>),
    /// No such giveaway in this server.
    Missing,
    StillRunning,
    /// Every entrant has already won.
    NoOneLeft,
}

macro_rules! columns {
    () => {
        "g.id, g.guild_id, g.channel_id, g.message_id, g.host_id, g.prize, g.shells, g.winners, \
         g.required_role_id, g.ends_at, g.ended_at, g.status, g.winner_ids, g.announced, \
         (SELECT count(*) FROM giveaway_entries e WHERE e.giveaway_id = g.id) AS entries"
    };
}

/// What a picker chose, kept honest: only people from `pool`, each once, at most `max`.
fn chosen(picked: Vec<u64>, pool: &[u64], max: usize) -> Vec<u64> {
    let mut out: Vec<u64> = Vec::with_capacity(max);
    for user in picked {
        if out.len() < max && pool.contains(&user) && !out.contains(&user) {
            out.push(user);
        }
    }
    out
}

/// Adds shells to a balance inside a transaction, with its ledger line.
async fn credit(tx: &mut Transaction<'_, Postgres>, user: i64, amount: i64, reason: &str) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO users (id, credits) VALUES ($1, $2)
         ON CONFLICT (id) DO UPDATE SET credits = users.credits + EXCLUDED.credits",
    )
    .bind(user)
    .bind(amount)
    .execute(&mut **tx)
    .await?;
    ledger(tx, user, amount, reason).await
}

impl Db {
    /// Starts a giveaway: holds `winners` x `shells` from the host and creates the row, in one
    /// transaction. Nothing changes when the host cannot cover it or the server is full.
    pub async fn create_giveaway(&self, new: NewGiveaway<'_>) -> anyhow::Result<GiveawayOpen> {
        let mut tx = self.pool().begin().await?;
        // Two giveaways started at once must not both slip under the limit.
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended('giveaways:' || $1::text, 0))")
            .bind(sql_id(new.guild_id))
            .execute(&mut *tx)
            .await?;
        let (running,): (i64,) =
            sqlx::query_as("SELECT count(*) FROM giveaways WHERE guild_id = $1 AND status = 'running'")
                .bind(sql_id(new.guild_id))
                .fetch_one(&mut *tx)
                .await?;
        if running >= MAX_RUNNING_GIVEAWAYS {
            return Ok(GiveawayOpen::Full);
        }
        let escrow = new.shells * i64::from(new.winners);
        if escrow > 0 {
            let host = sql_id(new.host);
            sqlx::query("INSERT INTO users (id) VALUES ($1) ON CONFLICT (id) DO NOTHING")
                .bind(host)
                .execute(&mut *tx)
                .await?;
            let held: Option<(i64,)> = sqlx::query_as(
                "UPDATE users SET credits = credits - $2 WHERE id = $1 AND credits >= $2 RETURNING credits",
            )
            .bind(host)
            .bind(escrow)
            .fetch_optional(&mut *tx)
            .await?;
            if held.is_none() {
                let (balance,): (i64,) = sqlx::query_as("SELECT credits FROM users WHERE id = $1")
                    .bind(host)
                    .fetch_one(&mut *tx)
                    .await?;
                return Ok(GiveawayOpen::Broke { balance });
            }
            ledger(&mut tx, host, -escrow, "giveaway_escrow").await?;
        }
        let (id,): (i64,) = sqlx::query_as(
            "INSERT INTO giveaways (guild_id, channel_id, host_id, prize, shells, winners, required_role_id, ends_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8) RETURNING id",
        )
        .bind(sql_id(new.guild_id))
        .bind(sql_id(new.channel_id))
        .bind(sql_id(new.host))
        .bind(new.prize)
        .bind(new.shells)
        .bind(new.winners)
        .bind(new.required_role.map(sql_id))
        .bind(new.ends_at)
        .fetch_one(&mut *tx)
        .await?;
        let row = sqlx::query_as::<_, GiveawayRow>(concat!("SELECT ", columns!(), " FROM giveaways g WHERE g.id = $1"))
            .bind(id)
            .fetch_one(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(GiveawayOpen::Started(Box::new(row)))
    }

    pub async fn giveaway(&self, id: i64) -> anyhow::Result<Option<GiveawayRow>> {
        Ok(
            sqlx::query_as::<_, GiveawayRow>(concat!("SELECT ", columns!(), " FROM giveaways g WHERE g.id = $1"))
                .bind(id)
                .fetch_optional(self.pool())
                .await?,
        )
    }

    /// Remembers which message shows the giveaway.
    pub async fn set_giveaway_message(&self, id: i64, channel_id: u64, message_id: u64) -> anyhow::Result<()> {
        sqlx::query("UPDATE giveaways SET channel_id = $2, message_id = $3 WHERE id = $1")
            .bind(id)
            .bind(sql_id(channel_id))
            .bind(sql_id(message_id))
            .execute(self.pool())
            .await?;
        Ok(())
    }

    /// Takes back a giveaway whose message could not be posted: the row goes and the host gets the
    /// held shells back. False when it was no longer running.
    pub async fn abort_giveaway(&self, id: i64) -> anyhow::Result<bool> {
        let mut tx = self.pool().begin().await?;
        let row = sqlx::query_as::<_, GiveawayRow>(concat!(
            "SELECT ",
            columns!(),
            " FROM giveaways g WHERE g.id = $1 AND g.status = 'running' FOR UPDATE OF g"
        ))
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?;
        let Some(row) = row else {
            return Ok(false);
        };
        if row.escrow() > 0 {
            credit(&mut tx, row.host_id, row.escrow(), "giveaway_refund").await?;
        }
        sqlx::query("DELETE FROM giveaways WHERE id = $1")
            .bind(id)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(true)
    }

    /// Enters the member, or takes their entry back when they are in. The giveaway row is
    /// share-locked, so an entry never lands after the giveaway ended.
    pub async fn toggle_giveaway_entry(&self, id: i64, user: u64, now: DateTime<Utc>) -> anyhow::Result<EntryToggle> {
        let mut tx = self.pool().begin().await?;
        let open: Option<(bool,)> =
            sqlx::query_as("SELECT status = 'running' AND ends_at > $2 FROM giveaways WHERE id = $1 FOR SHARE")
                .bind(id)
                .bind(now)
                .fetch_optional(&mut *tx)
                .await?;
        if open != Some((true,)) {
            return Ok(EntryToggle::Closed);
        }
        let left = sqlx::query("DELETE FROM giveaway_entries WHERE giveaway_id = $1 AND user_id = $2")
            .bind(id)
            .bind(sql_id(user))
            .execute(&mut *tx)
            .await?
            .rows_affected()
            > 0;
        if !left {
            sqlx::query("INSERT INTO giveaway_entries (giveaway_id, user_id) VALUES ($1, $2) ON CONFLICT DO NOTHING")
                .bind(id)
                .bind(sql_id(user))
                .execute(&mut *tx)
                .await?;
        }
        let (entries,): (i64,) = sqlx::query_as("SELECT count(*) FROM giveaway_entries WHERE giveaway_id = $1")
            .bind(id)
            .fetch_one(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(if left {
            EntryToggle::Left(entries)
        } else {
            EntryToggle::Entered(entries)
        })
    }

    /// Running giveaways whose time is up at `now`, soonest first, with the server and channel they live in.
    /// Ones paused by their module are left out: in a server that switched giveaways off, or in a channel
    /// its rule names as excluded (a rule that reaches the channel through its category is the caller's to
    /// check).
    pub async fn due_giveaways(&self, now: DateTime<Utc>, limit: i64) -> anyhow::Result<Vec<Due>> {
        Ok(sqlx::query_as(concat!(
            "SELECT g.id, g.guild_id, g.channel_id FROM giveaways g
             WHERE g.status = 'running' AND g.ends_at <= $1 AND ",
            not_paused!("g"),
            " ORDER BY g.ends_at LIMIT $2"
        ))
        .bind(now)
        .bind(limit)
        .bind(Module::Giveaways.key())
        .fetch_all(self.pool())
        .await?)
    }

    /// Ends a running giveaway, once: `pick(entrants, count)` chooses the winners, each is paid
    /// the shells prize and the host gets back the prize nobody won, all in one transaction with the
    /// status change. `None` when it is not running (somebody ended it first) or, unless `early`,
    /// not due at `now`.
    pub async fn end_giveaway(
        &self,
        id: i64,
        now: DateTime<Utc>,
        early: bool,
        pick: impl FnOnce(&[u64], usize) -> Vec<u64>,
    ) -> anyhow::Result<Option<EndedGiveaway>> {
        let mut tx = self.pool().begin().await?;
        let row = sqlx::query_as::<_, GiveawayRow>(concat!(
            "SELECT ",
            columns!(),
            " FROM giveaways g WHERE g.id = $1 AND g.status = 'running' AND ($3 OR g.ends_at <= $2) FOR UPDATE OF g"
        ))
        .bind(id)
        .bind(now)
        .bind(early)
        .fetch_optional(&mut *tx)
        .await?;
        let Some(mut giveaway) = row else {
            return Ok(None);
        };
        let entrants: Vec<(i64,)> = sqlx::query_as("SELECT user_id FROM giveaway_entries WHERE giveaway_id = $1")
            .bind(id)
            .fetch_all(&mut *tx)
            .await?;
        let entrants: Vec<u64> = entrants.into_iter().map(|(user,)| user as u64).collect();
        let winners = chosen(
            pick(&entrants, giveaway.winners as usize),
            &entrants,
            giveaway.winners as usize,
        );

        let refunded = giveaway.shells * (i64::from(giveaway.winners) - winners.len() as i64);
        // Balances are locked in id order, like every other shell movement, so a payout cannot
        // deadlock with a transfer between two of the winners.
        let mut payouts: Vec<(i64, i64, &str)> = Vec::with_capacity(winners.len() + 1);
        if giveaway.shells > 0 {
            payouts.extend(winners.iter().map(|w| (sql_id(*w), giveaway.shells, "giveaway_win")));
        }
        if refunded > 0 {
            payouts.push((giveaway.host_id, refunded, "giveaway_refund"));
        }
        payouts.sort_by_key(|(user, _, _)| *user);
        for (user, amount, reason) in payouts {
            credit(&mut tx, user, amount, reason).await?;
        }
        let ids: Vec<i64> = winners.iter().map(|w| sql_id(*w)).collect();
        sqlx::query("UPDATE giveaways SET status = 'ended', ended_at = $2, winner_ids = $3 WHERE id = $1")
            .bind(id)
            .bind(now)
            .bind(&ids)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        giveaway.status = ENDED.to_owned();
        giveaway.ended_at = Some(now);
        giveaway.winner_ids = ids;
        Ok(Some(EndedGiveaway {
            giveaway,
            winners,
            refunded,
        }))
    }

    /// Picks `count` new winners of an ended giveaway from the entrants who have not won yet. No
    /// shells move: the prize was paid when it ended.
    pub async fn reroll_giveaway(
        &self,
        guild_id: u64,
        id: i64,
        count: usize,
        pick: impl FnOnce(&[u64], usize) -> Vec<u64>,
    ) -> anyhow::Result<Reroll> {
        let mut tx = self.pool().begin().await?;
        let row = sqlx::query_as::<_, GiveawayRow>(concat!(
            "SELECT ",
            columns!(),
            " FROM giveaways g WHERE g.id = $1 AND g.guild_id = $2 FOR UPDATE OF g"
        ))
        .bind(id)
        .bind(sql_id(guild_id))
        .fetch_optional(&mut *tx)
        .await?;
        let Some(mut giveaway) = row else {
            return Ok(Reroll::Missing);
        };
        if giveaway.is_running() {
            return Ok(Reroll::StillRunning);
        }
        let candidates: Vec<(i64,)> =
            sqlx::query_as("SELECT user_id FROM giveaway_entries WHERE giveaway_id = $1 AND NOT (user_id = ANY($2))")
                .bind(id)
                .bind(&giveaway.winner_ids)
                .fetch_all(&mut *tx)
                .await?;
        let candidates: Vec<u64> = candidates.into_iter().map(|(user,)| user as u64).collect();
        let picked = chosen(pick(&candidates, count), &candidates, count);
        if picked.is_empty() {
            return Ok(Reroll::NoOneLeft);
        }
        let ids: Vec<i64> = picked.iter().map(|w| sql_id(*w)).collect();
        sqlx::query("UPDATE giveaways SET winner_ids = winner_ids || $2 WHERE id = $1")
            .bind(id)
            .bind(&ids)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        giveaway.winner_ids.extend(ids);
        Ok(Reroll::Done(Box::new(giveaway), picked))
    }

    /// Ended giveaways whose channel has not been told yet, oldest first.
    pub async fn unannounced_giveaways(&self, limit: i64) -> anyhow::Result<Vec<GiveawayRow>> {
        Ok(sqlx::query_as::<_, GiveawayRow>(concat!(
            "SELECT ",
            columns!(),
            " FROM giveaways g WHERE g.status = 'ended' AND NOT g.announced ORDER BY g.id LIMIT $1"
        ))
        .bind(limit)
        .fetch_all(self.pool())
        .await?)
    }

    /// Takes the job of telling the channel how giveaway `id` ended; false when it was told already
    /// or another pod claimed it in the last two minutes (after that the claim lapses, so an
    /// announcement lost to a crash is tried again).
    pub async fn claim_giveaway_announcement(&self, id: i64, now: DateTime<Utc>) -> anyhow::Result<bool> {
        Ok(sqlx::query(
            "UPDATE giveaways SET announce_claimed = $2
             WHERE id = $1 AND status = 'ended' AND NOT announced
               AND (announce_claimed IS NULL OR announce_claimed < $2 - interval '2 minutes')",
        )
        .bind(id)
        .bind(now)
        .execute(self.pool())
        .await?
        .rows_affected()
            > 0)
    }

    pub async fn mark_giveaway_announced(&self, id: i64) -> anyhow::Result<()> {
        sqlx::query("UPDATE giveaways SET announced = TRUE WHERE id = $1")
            .bind(id)
            .execute(self.pool())
            .await?;
        Ok(())
    }

    /// The server's running giveaways, ending soonest first.
    pub async fn running_giveaways(&self, guild_id: u64) -> anyhow::Result<Vec<GiveawayRow>> {
        Ok(sqlx::query_as::<_, GiveawayRow>(concat!(
            "SELECT ",
            columns!(),
            " FROM giveaways g WHERE g.guild_id = $1 AND g.status = 'running' ORDER BY g.ends_at LIMIT $2"
        ))
        .bind(sql_id(guild_id))
        .bind(MAX_RUNNING_GIVEAWAYS)
        .fetch_all(self.pool())
        .await?)
    }

    /// Drops giveaways that ended before `before` (and their entries); returns how many.
    pub async fn forget_old_giveaways(&self, before: DateTime<Utc>) -> anyhow::Result<u64> {
        Ok(
            sqlx::query("DELETE FROM giveaways WHERE status = 'ended' AND announced AND ended_at < $1")
                .bind(before)
                .execute(self.pool())
                .await?
                .rows_affected(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pick_is_kept_inside_the_pool_once_and_within_the_limit() {
        assert_eq!(chosen(vec![3, 3, 9, 1, 2], &[1, 2, 3], 2), [3, 1]);
        assert_eq!(chosen(vec![], &[1], 3), Vec::<u64>::new());
        assert_eq!(chosen(vec![5], &[1], 3), Vec::<u64>::new());
    }

    #[test]
    fn the_escrow_is_the_prize_for_every_winner() {
        let row = GiveawayRow {
            id: 1,
            guild_id: 2,
            channel_id: 3,
            message_id: None,
            host_id: 4,
            prize: "Shells".to_owned(),
            shells: 50,
            winners: 3,
            required_role_id: None,
            ends_at: Utc::now(),
            ended_at: None,
            status: RUNNING.to_owned(),
            winner_ids: Vec::new(),
            announced: false,
            entries: 0,
        };
        assert_eq!(row.escrow(), 150);
        assert!(row.is_running());
    }
}
