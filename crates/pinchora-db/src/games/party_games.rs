//! Party games: lobbies, games and polls with their players, typed entries and votes. Every change to a
//! running game goes through [`Db::commit_party`], which only applies to the game as the caller saw it
//! (`seq`), so two clicks, or a click and the clock, can never both move it. A game's payout rides in the
//! same transaction as the move that ends it.
//!
//! The transactions follow the lock rule of the `locks` module: the locks of the players they touch first,
//! then the game's row, then the writes, and a transaction that loses a deadlock anyway runs again.

use chrono::{DateTime, Utc};

use crate::Db;
use crate::economy::legacy::ledger;
use crate::games::games::DONE;
use crate::infra::locks::lock_users;
use crate::infra::locks::retry;
use crate::sql_id;

type Tx<'a> = sqlx::Transaction<'a, sqlx::Postgres>;

pub const LOBBY: &str = "lobby";
pub const PLAYING: &str = "playing";

/// The columns of a [`PartyGame`], for queries that need a literal string.
macro_rules! game_columns {
    () => {
        "id, kind, guild_id, channel_id, message_id, host_id, status, phase, round, rounds, seq, state, deadline"
    };
}

macro_rules! player_columns {
    () => {
        "user_id, name, seat, lives, score, eliminated, guesses"
    };
}

/// A lobby, a game under way, a finished game or a poll.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct PartyGame {
    pub id: i64,
    pub kind: String,
    pub guild_id: i64,
    pub channel_id: i64,
    pub message_id: Option<i64>,
    pub host_id: i64,
    /// `lobby`, `playing` or `done`.
    pub status: String,
    /// What the game waits for; its own vocabulary.
    pub phase: String,
    pub round: i32,
    pub rounds: i32,
    /// Goes up with every change; [`Db::commit_party`] refuses a stale one.
    pub seq: i32,
    pub state: String,
    pub deadline: DateTime<Utc>,
}

impl PartyGame {
    pub fn in_lobby(&self) -> bool {
        self.status == LOBBY
    }

    pub fn is_playing(&self) -> bool {
        self.status == PLAYING
    }

    pub fn is_done(&self) -> bool {
        self.status == DONE
    }

    /// Whether the wait for what the game is on has run out at `now`.
    pub fn late(&self, now: DateTime<Utc>) -> bool {
        self.deadline <= now
    }

    pub fn host(&self) -> u64 {
        self.host_id as u64
    }

    /// The channel and message the game is shown in, once it is known.
    pub fn location(&self) -> Option<(u64, u64)> {
        self.message_id.map(|m| (self.channel_id as u64, m as u64))
    }
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct PartyPlayer {
    pub user_id: i64,
    pub name: String,
    pub seat: i32,
    pub lives: i32,
    pub score: i32,
    pub eliminated: bool,
    pub guesses: Vec<String>,
}

impl PartyPlayer {
    pub fn id(&self) -> u64 {
        self.user_id as u64
    }
}

/// A word, phrase or caption somebody typed.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct PartyEntry {
    pub round: i32,
    pub user_id: i64,
    pub text: String,
}

impl PartyEntry {
    pub fn author(&self) -> u64 {
        self.user_id as u64
    }
}

/// A game to open: the host is its first player.
#[derive(Debug, Clone, Copy)]
pub struct NewParty<'a> {
    pub kind: &'a str,
    pub guild_id: u64,
    pub channel_id: u64,
    pub host: u64,
    pub host_name: &'a str,
    pub rounds: i32,
    /// `lobby` for a game that waits for players, `playing` for a poll.
    pub status: &'a str,
    pub phase: &'a str,
    pub state: &'a str,
    pub deadline: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PartyJoin {
    Joined,
    Already,
    Full,
    /// Not a lobby any more, or its time is up.
    Closed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PartyLeave {
    Left,
    NotIn,
    /// The host cannot leave their own lobby, only cancel it.
    Host,
    Closed,
}

/// How a vote in a written game landed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PartyVote {
    Counted,
    /// The voter had picked another entry; the vote moved.
    Changed,
    Same,
    /// Not the voting phase of that round, or the clock ran out.
    Closed,
    NotPlayer,
    OwnEntry,
    NoEntry,
}

/// How a vote in a poll landed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PollChoice {
    First,
    Changed,
    Same,
    Closed,
}

/// A change to one player, applied with a move.
#[derive(Debug, Clone, Copy, Default)]
pub struct PlayerChange {
    pub user: u64,
    pub seat: Option<i32>,
    /// Sets the lives to this.
    pub lives: Option<i32>,
    /// Added to the score.
    pub score: i32,
    pub eliminated: Option<bool>,
}

/// An entry to file with a move.
#[derive(Debug, Clone, Copy)]
pub struct NewEntry<'a> {
    pub round: i32,
    pub user: u64,
    pub text: &'a str,
}

/// What a finished game pays. A player is paid for at most `cap` games of `game` a day.
#[derive(Debug, Clone, Copy)]
pub struct Pay<'a> {
    pub game: &'a str,
    pub day: i32,
    pub cap: i32,
    pub reason: &'a str,
    /// Who earns what; players with nothing to earn are left out, so they do not use up their day.
    pub shells: &'a [(u64, i64)],
}

/// A move: where the game goes next, and what changes with it.
#[derive(Debug, Clone, Copy)]
pub struct Commit<'a> {
    pub phase: &'a str,
    pub round: i32,
    pub state: &'a str,
    pub deadline: DateTime<Utc>,
    /// `playing`, or `done` to end the game.
    pub status: &'a str,
    pub players: &'a [PlayerChange],
    pub entries: &'a [NewEntry<'a>],
    pub pay: Option<Pay<'a>>,
}

/// A move that went through: the game as it now is, and what each paid player was given (0 for one
/// whose day was already full).
#[derive(Debug, Clone)]
pub struct Committed {
    pub game: PartyGame,
    pub paid: Vec<(u64, i64)>,
}

mod entries;
mod lobby;
mod moves;
mod polls;

impl Db {
    /// Games and polls whose wait ran out and that nobody has moved on, the most overdue first.
    pub async fn due_parties(&self, now: DateTime<Utc>, limit: i64) -> anyhow::Result<Vec<PartyGame>> {
        Ok(sqlx::query_as::<_, PartyGame>(concat!(
            "SELECT ",
            game_columns!(),
            " FROM party_games WHERE status <> 'done' AND deadline <= $1 ORDER BY deadline LIMIT $2"
        ))
        .bind(now)
        .bind(limit)
        .fetch_all(self.pool())
        .await?)
    }

    /// Adds a guess to a player's Wordle Race board, if the game is on, the board has room and the
    /// word was not already found. The board as it now is; `None` when it was refused, so a double
    /// click adds nothing.
    pub async fn race_guess(
        &self,
        id: i64,
        user: u64,
        guess: &str,
        word: &str,
        now: DateTime<Utc>,
    ) -> anyhow::Result<Option<PartyPlayer>> {
        Ok(sqlx::query_as::<_, PartyPlayer>(
            "UPDATE party_players p SET guesses = array_append(p.guesses, $3)
             FROM party_games g
             WHERE p.game_id = $1 AND p.user_id = $2 AND g.id = p.game_id
               AND g.status = 'playing' AND g.phase = 'race' AND g.deadline > $5
               AND cardinality(p.guesses) < 6 AND NOT ($4 = ANY(p.guesses))
             RETURNING p.user_id, p.name, p.seat, p.lives, p.score, p.eliminated, p.guesses",
        )
        .bind(id)
        .bind(sql_id(user))
        .bind(guess)
        .bind(word)
        .bind(now)
        .fetch_optional(self.pool())
        .await?)
    }

    /// Whether a server wants the strict word filter in the party games.
    pub async fn party_strict(&self, guild: u64) -> anyhow::Result<bool> {
        let row: Option<(bool,)> = sqlx::query_as("SELECT strict FROM party_settings WHERE guild_id = $1")
            .bind(sql_id(guild))
            .fetch_optional(self.pool())
            .await?;
        Ok(row.is_some_and(|(strict,)| strict))
    }

    pub async fn set_party_strict(&self, guild: u64, strict: bool) -> anyhow::Result<()> {
        sqlx::query(
            "INSERT INTO party_settings (guild_id, strict) VALUES ($1, $2)
             ON CONFLICT (guild_id) DO UPDATE SET strict = EXCLUDED.strict",
        )
        .bind(sql_id(guild))
        .bind(strict)
        .execute(self.pool())
        .await?;
        Ok(())
    }

    /// Forgets games that ended before `before`, with their players, entries and votes, and daily
    /// counts from before `day`. Returns how many games went.
    ///
    /// A sweep over the rows of many players cannot take their locks first, so it locks the rows it deletes
    /// with `SKIP LOCKED`: one somebody holds is left for the next sweep (see the `locks` module).
    pub async fn forget_old_parties(&self, before: DateTime<Utc>, day: i32) -> anyhow::Result<u64> {
        retry("forget_old_parties", || async move {
            sqlx::query(
                "WITH old AS MATERIALIZED (
                     SELECT user_id, day, game FROM party_daily WHERE day < $1 FOR UPDATE SKIP LOCKED)
                 DELETE FROM party_daily d USING old
                 WHERE d.user_id = old.user_id AND d.day = old.day AND d.game = old.game",
            )
            .bind(day)
            .execute(self.pool())
            .await?;
            Ok(sqlx::query(
                "WITH old AS MATERIALIZED (
                     SELECT id FROM party_games WHERE status = 'done' AND created_at < $1
                     FOR UPDATE SKIP LOCKED)
                 DELETE FROM party_games WHERE id IN (SELECT id FROM old)",
            )
            .bind(before)
            .execute(self.pool())
            .await?
            .rows_affected())
        })
        .await
    }
}
/// The game row, locked for the rest of the transaction.
async fn lock_game(tx: &mut Tx<'_>, id: i64) -> anyhow::Result<Option<PartyGame>> {
    Ok(sqlx::query_as::<_, PartyGame>(concat!(
        "SELECT ",
        game_columns!(),
        " FROM party_games WHERE id = $1 FOR UPDATE"
    ))
    .bind(id)
    .fetch_optional(&mut **tx)
    .await?)
}

async fn earn(tx: &mut Tx<'_>, user: u64, shells: i64, reason: &str) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO users (id, credits) VALUES ($1, $2)
         ON CONFLICT (id) DO UPDATE SET credits = users.credits + EXCLUDED.credits",
    )
    .bind(sql_id(user))
    .bind(shells)
    .execute(&mut **tx)
    .await?;
    ledger(tx, sql_id(user), shells, reason).await
}
