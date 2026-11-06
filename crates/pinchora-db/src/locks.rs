//! The order transactions take their locks in, and what a transaction that loses a deadlock anyway
//! does.
//!
//! # The rule
//!
//! Every transaction that writes or locks rows of a player (their `users` row and anything a table
//! of [`crate::USER_COLUMNS`] keys by their id: shells, the ledger, the monthly numbers, the weekly
//! board, members, houses, inventory, pets, likes, visits, entries, votes) does it in this order:
//!
//! 1. It takes the lock of every player it is going to touch, all of them at once and in id order,
//!    with [`lock_users`], before it holds any row.
//! 2. Then it locks the object it is about, if there is one (a game session, a giveaway, a pet, a
//!    stall listing, a poll), and checks it is still in the state it expects.
//! 3. Then it writes, the players' rows included. Nothing writes a player's activity, weekly XP,
//!    ledger or balance without holding that player's lock.
//!
//! When it has to read the object to learn whose it is (the two players of a duel, a giveaway's
//! winners, the seller of a listing), it reads it plainly first, takes the locks, and only then
//! locks it and checks again. "Delete my data" follows the rule too: it takes the locks of the
//! player and of everyone it pays back or takes a like from. A player it learns about only after
//! it holds its first locks (a like that landed in between) is never waited for, which would be
//! out of order: [`try_lock_users`] gets that lock or the delete starts over, with the full set
//! read again.
//!
//! So a transaction never waits for a player's lock while holding a row, and the rows of a player
//! are only reached by whoever holds that player's lock: no two transactions can each hold what the
//! other waits for. The same lock is what keeps a claim from overlapping a delete of the same player.
//!
//! A statement on its own that writes one player's rows starting with `users` counts as taking the
//! lock first: the trigger on `users` takes it before the statement touches anything (chat XP relies
//! on this to stay one round trip). A statement or transaction that locks a single row and nothing
//! else (a pet renamed, points taken) cannot be part of a deadlock and needs no lock, and neither
//! does a lone statement that holds nothing while it waits: a monthly counter bumped by one upsert.
//! The sweeps that forget old rows of many players at once cannot take locks first, so they lock the
//! rows they delete with `SKIP LOCKED`: a row somebody else holds is left for the next sweep
//! instead of being waited for (only the rows a cascade deletes along with it can still make a
//! sweep wait, and [`retry`] covers that).
//!
//! The games follow it too. A move of one player (a grab, a pick, a join, a poll vote) takes that
//! player's lock before the drop, session or game row; a transaction that pays everyone in a lobby
//! (a derby, a heist, a storm, a party game's last move) reads the players, takes all their locks,
//! and only then locks the session; one who joined in between is [`try_lock_users`]ed, never waited for.
//! The collection games, the party games, the word and party games, the contest and fishing do this;
//! the board games and the calm games hold no transaction of their own beyond those of `games.rs`.
//!
//! # When it happens anyway
//!
//! Postgres catches a deadlock the rule does not prevent (two friends who delete their data at once
//! each remove the visits and gifts the other one has, from opposite ends; a query that breaks the
//! rule) and rolls one side back. [`retry`] runs a transaction that
//! lost like that again, a few times, after a short random wait. That is safe because the lost
//! attempt left nothing behind, and every claim and payment checks inside its own transaction what
//! was already used up or paid.

use std::{
    collections::hash_map::RandomState,
    hash::{BuildHasher, Hasher},
    sync::atomic::{AtomicU64, Ordering},
    time::Duration,
};

/// How many times in all a transaction that keeps losing deadlocks runs.
const ATTEMPTS: u32 = 3;

/// How many times a transaction may start over because it met a player's lock out of order
/// ([`LockBusy`]), besides the deadlocks it lost. Each of those cost it nothing but a read, and in a
/// busy server a player's lock is queued behind several transactions of theirs, so the next attempt
/// can find a late player again.
const BUSY_ATTEMPTS: u32 = 12;

/// The error of a transaction that gave up on purpose because it could not take a player's lock
/// without waiting (see [`try_lock_users`]). Nothing it did stays; running it again is all there is
/// to do, and [`retry`] does.
#[derive(Debug)]
pub(crate) struct LockBusy;

impl std::fmt::Display for LockBusy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("a player's lock was busy and could not be taken out of order")
    }
}

impl std::error::Error for LockBusy {}

static RETRIED: AtomicU64 = AtomicU64::new(0);

/// How many transactions were run again since start because they lost a deadlock or a serialization
/// conflict.
pub fn conflict_retries() -> u64 {
    RETRIED.load(Ordering::Relaxed)
}

/// Takes the lock of each player in `ids` (the one `delete_user_data` and the trigger that creates a
/// player's row take) in id order, in one round trip, and keeps them until the transaction ends.
/// See the module's rule: this comes before any row is locked.
pub(crate) async fn lock_users(tx: &mut sqlx::Transaction<'_, sqlx::Postgres>, ids: &[i64]) -> anyhow::Result<()> {
    let mut ids = ids.to_vec();
    ids.sort_unstable();
    ids.dedup();
    // `unnest` hands the ids out in their order and each lock is taken as its row comes.
    sqlx::query("SELECT lock_user(id) FROM unnest($1::BIGINT[]) AS id")
        .bind(&ids)
        .execute(&mut **tx)
        .await?;
    Ok(())
}

/// Tries to take the lock of each player in `ids` without waiting for any of them, and fails with
/// [`LockBusy`] if one is held by somebody else. For a transaction that already holds locks and
/// learns of more players: waiting for those would be taking locks out of order. The transaction
/// ends in the error and is run again, this time asking for all of them in order from the start.
pub(crate) async fn try_lock_users(tx: &mut sqlx::Transaction<'_, sqlx::Postgres>, ids: &[i64]) -> anyhow::Result<()> {
    let mut ids = ids.to_vec();
    ids.sort_unstable();
    ids.dedup();
    // The same key as `lock_user` (migration 0023), taken without waiting.
    let got: Vec<(bool,)> = sqlx::query_as(
        "SELECT pg_try_advisory_xact_lock(hashtextextended('user:' || id, 0)) FROM unnest($1::BIGINT[]) AS id",
    )
    .bind(&ids)
    .fetch_all(&mut **tx)
    .await?;
    if got.iter().all(|(taken,)| *taken) {
        Ok(())
    } else {
        Err(LockBusy.into())
    }
}

/// Runs `attempt` until it does not lose a deadlock, at most [`ATTEMPTS`] times.
pub(crate) async fn retry<T, F, Fut>(what: &'static str, mut attempt: F) -> anyhow::Result<T>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = anyhow::Result<T>>,
{
    let mut retry = Retry::new(what);
    loop {
        let result = attempt().await;
        if !retry.again(&result).await {
            return result;
        }
    }
}

/// [`retry`] spelled out, for an attempt that borrows something mutably (a caller's closure).
pub(crate) struct Retry {
    what: &'static str,
    attempt: u32,
    busy: u32,
}

impl Retry {
    pub(crate) fn new(what: &'static str) -> Self {
        Self {
            what,
            attempt: 1,
            busy: 0,
        }
    }

    /// Whether `result` lost a deadlock or a serialization conflict (or gave up with [`LockBusy`])
    /// and may run again. If so, waits a few random milliseconds first, so the two sides do not meet
    /// again straight away.
    pub(crate) async fn again<T>(&mut self, result: &anyhow::Result<T>) -> bool {
        let Err(error) = result else {
            return false;
        };
        let noise = RandomState::new().build_hasher().finish();
        let wait = if error.chain().any(|cause| cause.is::<LockBusy>()) {
            if self.busy >= BUSY_ATTEMPTS {
                return false;
            }
            self.busy += 1;
            5 + noise % 20
        } else if lost_a_conflict(error) && self.attempt < ATTEMPTS {
            let wait = 5 * u64::from(self.attempt) + noise % (20 * u64::from(self.attempt));
            self.attempt += 1;
            wait
        } else {
            return false;
        };
        RETRIED.fetch_add(1, Ordering::Relaxed);
        tracing::debug!(%error, what = self.what, "transaction lost a deadlock, running it again");
        tokio::time::sleep(Duration::from_millis(wait)).await;
        true
    }
}

/// Whether Postgres rolled the transaction back to break a deadlock (`40P01`) or a serialization
/// conflict (`40001`): nothing it did stays, and running it again is all there is to do.
fn lost_a_conflict(error: &anyhow::Error) -> bool {
    error
        .chain()
        .filter_map(|cause| cause.downcast_ref::<sqlx::Error>())
        .filter_map(|cause| cause.as_database_error())
        .any(|cause| matches!(cause.code().as_deref(), Some("40P01" | "40001")))
}
