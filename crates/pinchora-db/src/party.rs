//! Party games: lobbies, games and polls with their players, typed entries and votes. Every change to a
//! running game goes through [`Db::commit_party`], which only applies to the game as the caller saw it
//! (`seq`), so two clicks, or a click and the clock, can never both move it. A game's payout rides in the
//! same transaction as the move that ends it.

use chrono::{DateTime, Utc};

use crate::{Db, economy::ledger, games::DONE, sql_id};

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

impl Db {
    /// Opens a game with its host as the first player.
    pub async fn create_party(&self, new: NewParty<'_>) -> anyhow::Result<PartyGame> {
        let mut tx = self.pool().begin().await?;
        let game: PartyGame = sqlx::query_as(concat!(
            "INSERT INTO party_games (kind, guild_id, channel_id, host_id, status, phase, rounds, state, deadline)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9) RETURNING ",
            game_columns!()
        ))
        .bind(new.kind)
        .bind(sql_id(new.guild_id))
        .bind(sql_id(new.channel_id))
        .bind(sql_id(new.host))
        .bind(new.status)
        .bind(new.phase)
        .bind(new.rounds)
        .bind(new.state)
        .bind(new.deadline)
        .fetch_one(&mut *tx)
        .await?;
        sqlx::query("INSERT INTO party_players (game_id, user_id, name, seat) VALUES ($1, $2, $3, 0)")
            .bind(game.id)
            .bind(sql_id(new.host))
            .bind(new.host_name)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(game)
    }

    pub async fn party(&self, id: i64) -> anyhow::Result<Option<PartyGame>> {
        Ok(
            sqlx::query_as::<_, PartyGame>(concat!("SELECT ", game_columns!(), " FROM party_games WHERE id = $1"))
                .bind(id)
                .fetch_optional(self.pool())
                .await?,
        )
    }

    /// The players of a game in seat order.
    pub async fn party_players(&self, id: i64) -> anyhow::Result<Vec<PartyPlayer>> {
        Ok(sqlx::query_as::<_, PartyPlayer>(concat!(
            "SELECT ",
            player_columns!(),
            " FROM party_players WHERE game_id = $1 ORDER BY seat, user_id"
        ))
        .bind(id)
        .fetch_all(self.pool())
        .await?)
    }

    /// A lobby or game still going in `channel`, which keeps another from starting there. Polls do not
    /// count, and neither does a game more than five minutes past its deadline: the ticker will close it.
    pub async fn party_in_channel(&self, channel: u64, now: DateTime<Utc>) -> anyhow::Result<Option<PartyGame>> {
        Ok(sqlx::query_as::<_, PartyGame>(concat!(
            "SELECT ",
            game_columns!(),
            " FROM party_games
             WHERE channel_id = $1 AND status <> 'done' AND kind <> 'wouldyourrather'
               AND deadline > $2 - interval '5 minutes'
             ORDER BY id DESC LIMIT 1"
        ))
        .bind(sql_id(channel))
        .bind(now)
        .fetch_optional(self.pool())
        .await?)
    }

    /// Remembers which message shows the game, so the ticker can edit it.
    pub async fn set_party_message(&self, id: i64, channel: u64, message: u64) -> anyhow::Result<()> {
        sqlx::query("UPDATE party_games SET channel_id = $2, message_id = $3 WHERE id = $1 AND status <> 'done'")
            .bind(id)
            .bind(sql_id(channel))
            .bind(sql_id(message))
            .execute(self.pool())
            .await?;
        Ok(())
    }

    /// Adds a player to a lobby that has room.
    pub async fn join_party(
        &self,
        id: i64,
        user: u64,
        name: &str,
        max: usize,
        now: DateTime<Utc>,
    ) -> anyhow::Result<PartyJoin> {
        let mut tx = self.pool().begin().await?;
        let Some(game) = lock_game(&mut tx, id).await? else {
            return Ok(PartyJoin::Closed);
        };
        if !game.in_lobby() || game.late(now) {
            return Ok(PartyJoin::Closed);
        }
        let (seats, next, me): (i64, i32, i64) = sqlx::query_as(
            "SELECT COUNT(*), COALESCE(MAX(seat) + 1, 0)::INTEGER,
                    COUNT(*) FILTER (WHERE user_id = $2)
             FROM party_players WHERE game_id = $1",
        )
        .bind(id)
        .bind(sql_id(user))
        .fetch_one(&mut *tx)
        .await?;
        if me > 0 {
            return Ok(PartyJoin::Already);
        }
        if seats as usize >= max {
            return Ok(PartyJoin::Full);
        }
        sqlx::query("INSERT INTO party_players (game_id, user_id, name, seat) VALUES ($1, $2, $3, $4)")
            .bind(id)
            .bind(sql_id(user))
            .bind(name)
            .bind(next)
            .execute(&mut *tx)
            .await?;
        sqlx::query("UPDATE party_games SET seq = seq + 1 WHERE id = $1")
            .bind(id)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(PartyJoin::Joined)
    }

    /// Takes a player out of a lobby.
    pub async fn leave_party(&self, id: i64, user: u64, now: DateTime<Utc>) -> anyhow::Result<PartyLeave> {
        let mut tx = self.pool().begin().await?;
        let Some(game) = lock_game(&mut tx, id).await? else {
            return Ok(PartyLeave::Closed);
        };
        if !game.in_lobby() || game.late(now) {
            return Ok(PartyLeave::Closed);
        }
        if game.host_id == sql_id(user) {
            return Ok(PartyLeave::Host);
        }
        let gone = sqlx::query("DELETE FROM party_players WHERE game_id = $1 AND user_id = $2")
            .bind(id)
            .bind(sql_id(user))
            .execute(&mut *tx)
            .await?
            .rows_affected();
        if gone == 0 {
            return Ok(PartyLeave::NotIn);
        }
        sqlx::query("UPDATE party_games SET seq = seq + 1 WHERE id = $1")
            .bind(id)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(PartyLeave::Left)
    }

    /// Moves a game on, if it is still at `expect_seq` and not over: sets its phase, round, state and
    /// deadline, applies the player changes, files the entries and pays what a finished game pays, all
    /// in one transaction. `None` when somebody moved first; nothing has changed then, and the caller
    /// has nothing to announce.
    pub async fn commit_party(
        &self,
        id: i64,
        expect_seq: i32,
        commit: Commit<'_>,
    ) -> anyhow::Result<Option<Committed>> {
        let mut tx = self.pool().begin().await?;
        let moved: Option<PartyGame> = sqlx::query_as(concat!(
            "UPDATE party_games
             SET phase = $3, round = $4, state = $5, deadline = $6, status = $7, seq = seq + 1
             WHERE id = $1 AND seq = $2 AND status <> 'done'
             RETURNING ",
            game_columns!()
        ))
        .bind(id)
        .bind(expect_seq)
        .bind(commit.phase)
        .bind(commit.round)
        .bind(commit.state)
        .bind(commit.deadline)
        .bind(commit.status)
        .fetch_optional(&mut *tx)
        .await?;
        let Some(game) = moved else {
            return Ok(None);
        };
        for change in commit.players {
            sqlx::query(
                "UPDATE party_players
                 SET seat = COALESCE($3, seat), lives = COALESCE($4, lives), score = score + $5,
                     eliminated = COALESCE($6, eliminated)
                 WHERE game_id = $1 AND user_id = $2",
            )
            .bind(id)
            .bind(sql_id(change.user))
            .bind(change.seat)
            .bind(change.lives)
            .bind(change.score)
            .bind(change.eliminated)
            .execute(&mut *tx)
            .await?;
        }
        for entry in commit.entries {
            sqlx::query(
                "INSERT INTO party_entries (game_id, round, user_id, text) VALUES ($1, $2, $3, $4)
                 ON CONFLICT (game_id, round, user_id) DO UPDATE SET text = EXCLUDED.text",
            )
            .bind(id)
            .bind(entry.round)
            .bind(sql_id(entry.user))
            .bind(entry.text)
            .execute(&mut *tx)
            .await?;
        }
        let mut paid = Vec::new();
        if let Some(pay) = commit.pay {
            // In user order, so two games ending together cannot wait on each other's rows.
            let mut shells = pay.shells.to_vec();
            shells.sort_unstable();
            for (user, amount) in shells.into_iter().filter(|(_, amount)| *amount > 0) {
                let (plays,): (i32,) = sqlx::query_as(
                    "INSERT INTO party_daily (user_id, day, game, plays) VALUES ($1, $2, $3, 1)
                     ON CONFLICT (user_id, day, game) DO UPDATE SET plays = party_daily.plays + 1
                     RETURNING plays",
                )
                .bind(sql_id(user))
                .bind(pay.day)
                .bind(pay.game)
                .fetch_one(&mut *tx)
                .await?;
                if plays <= pay.cap {
                    earn(&mut tx, user, amount, pay.reason).await?;
                    paid.push((user, amount));
                } else {
                    paid.push((user, 0));
                }
            }
        }
        tx.commit().await?;
        Ok(Some(Committed { game, paid }))
    }

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

    /// Files what a player typed for a round, while the game is in `phase` of that round and the clock
    /// has not run out; a later entry replaces an earlier one. `Some(true)` for a first entry,
    /// `Some(false)` for a replacement, `None` when it was too late or the player is not in the game.
    pub async fn put_entry(
        &self,
        id: i64,
        round: i32,
        phase: &str,
        user: u64,
        text: &str,
        now: DateTime<Utc>,
    ) -> anyhow::Result<Option<bool>> {
        let row: Option<(bool,)> = sqlx::query_as(
            "INSERT INTO party_entries (game_id, round, user_id, text)
             SELECT g.id, $2, $4, $5 FROM party_games g
             WHERE g.id = $1 AND g.status = 'playing' AND g.phase = $3 AND g.round = $2 AND g.deadline > $6
               AND EXISTS (SELECT 1 FROM party_players p WHERE p.game_id = g.id AND p.user_id = $4)
             ON CONFLICT (game_id, round, user_id) DO UPDATE SET text = EXCLUDED.text
             RETURNING (xmax = 0)",
        )
        .bind(id)
        .bind(round)
        .bind(phase)
        .bind(sql_id(user))
        .bind(text)
        .bind(now)
        .fetch_optional(self.pool())
        .await?;
        Ok(row.map(|(fresh,)| fresh))
    }

    /// The entries of a round, in an order that does not tell who wrote what (it is stable, so numbers
    /// shown to voters mean the same thing every time).
    pub async fn party_entries(&self, id: i64, round: i32) -> anyhow::Result<Vec<PartyEntry>> {
        Ok(sqlx::query_as::<_, PartyEntry>(
            "SELECT round, user_id, text FROM party_entries WHERE game_id = $1 AND round = $2
             ORDER BY md5(game_id::text || ':' || round::text || ':' || user_id::text)",
        )
        .bind(id)
        .bind(round)
        .fetch_all(self.pool())
        .await?)
    }

    /// Every entry of a game, oldest round first: the words Hot Pearl has used, the phrases written.
    pub async fn party_all_entries(&self, id: i64) -> anyhow::Result<Vec<PartyEntry>> {
        Ok(sqlx::query_as::<_, PartyEntry>(
            "SELECT round, user_id, text FROM party_entries WHERE game_id = $1 ORDER BY round, user_id",
        )
        .bind(id)
        .fetch_all(self.pool())
        .await?)
    }

    /// Votes for the entry of `pick` in the voting phase of `round`.
    pub async fn put_vote(
        &self,
        id: i64,
        round: i32,
        voter: u64,
        pick: u64,
        now: DateTime<Utc>,
    ) -> anyhow::Result<PartyVote> {
        let Some(game) = self.party(id).await? else {
            return Ok(PartyVote::Closed);
        };
        if !game.is_playing() || game.phase != "vote" || game.round != round || game.late(now) {
            return Ok(PartyVote::Closed);
        }
        let (me, entry, previous): (bool, bool, Option<i64>) = sqlx::query_as(
            "SELECT EXISTS (SELECT 1 FROM party_players WHERE game_id = $1 AND user_id = $3),
                    EXISTS (SELECT 1 FROM party_entries WHERE game_id = $1 AND round = $2 AND user_id = $4),
                    (SELECT pick FROM party_votes WHERE game_id = $1 AND round = $2 AND voter_id = $3)",
        )
        .bind(id)
        .bind(round)
        .bind(sql_id(voter))
        .bind(sql_id(pick))
        .fetch_one(self.pool())
        .await?;
        if !me {
            return Ok(PartyVote::NotPlayer);
        }
        if voter == pick {
            return Ok(PartyVote::OwnEntry);
        }
        if !entry {
            return Ok(PartyVote::NoEntry);
        }
        if previous == Some(sql_id(pick)) {
            return Ok(PartyVote::Same);
        }
        sqlx::query(
            "INSERT INTO party_votes (game_id, round, voter_id, pick) VALUES ($1, $2, $3, $4)
             ON CONFLICT (game_id, round, voter_id) DO UPDATE SET pick = EXCLUDED.pick",
        )
        .bind(id)
        .bind(round)
        .bind(sql_id(voter))
        .bind(sql_id(pick))
        .execute(self.pool())
        .await?;
        Ok(if previous.is_some() {
            PartyVote::Changed
        } else {
            PartyVote::Counted
        })
    }

    /// Who voted for whose entry in a round, as (voter, author).
    pub async fn party_votes(&self, id: i64, round: i32) -> anyhow::Result<Vec<(u64, u64)>> {
        let rows: Vec<(i64, i64)> = sqlx::query_as(
            "SELECT voter_id, pick FROM party_votes WHERE game_id = $1 AND round = $2 ORDER BY voter_id",
        )
        .bind(id)
        .bind(round)
        .fetch_all(self.pool())
        .await?;
        Ok(rows.into_iter().map(|(v, p)| (v as u64, p as u64)).collect())
    }

    /// A vote in a would-you-rather poll for side 0 or 1; changing it is allowed until the poll closes.
    pub async fn put_poll_vote(
        &self,
        id: i64,
        voter: u64,
        side: i64,
        now: DateTime<Utc>,
    ) -> anyhow::Result<PollChoice> {
        let mut tx = self.pool().begin().await?;
        let Some(game) = lock_game(&mut tx, id).await? else {
            return Ok(PollChoice::Closed);
        };
        if game.kind != "wouldyourrather" || !game.is_playing() || game.late(now) {
            return Ok(PollChoice::Closed);
        }
        let previous: Option<(i64,)> =
            sqlx::query_as("SELECT pick FROM party_votes WHERE game_id = $1 AND round = 0 AND voter_id = $2")
                .bind(id)
                .bind(sql_id(voter))
                .fetch_optional(&mut *tx)
                .await?;
        if previous.is_some_and(|(pick,)| pick == side) {
            return Ok(PollChoice::Same);
        }
        sqlx::query(
            "INSERT INTO party_votes (game_id, round, voter_id, pick) VALUES ($1, 0, $2, $3)
             ON CONFLICT (game_id, round, voter_id) DO UPDATE SET pick = EXCLUDED.pick",
        )
        .bind(id)
        .bind(sql_id(voter))
        .bind(side)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(if previous.is_some() {
            PollChoice::Changed
        } else {
            PollChoice::First
        })
    }

    /// Votes for side 0 and side 1 of a poll.
    pub async fn poll_counts(&self, id: i64) -> anyhow::Result<(i64, i64)> {
        let (a, b): (i64, i64) = sqlx::query_as(
            "SELECT COUNT(*) FILTER (WHERE pick = 0), COUNT(*) FILTER (WHERE pick = 1)
             FROM party_votes WHERE game_id = $1 AND round = 0",
        )
        .bind(id)
        .fetch_one(self.pool())
        .await?;
        Ok((a, b))
    }

    /// Everyone who voted in a poll and the side they ended on.
    pub async fn poll_voters(&self, id: i64) -> anyhow::Result<Vec<(u64, i64)>> {
        let rows: Vec<(i64, i64)> =
            sqlx::query_as("SELECT voter_id, pick FROM party_votes WHERE game_id = $1 AND round = 0")
                .bind(id)
                .fetch_all(self.pool())
                .await?;
        Ok(rows.into_iter().map(|(v, p)| (v as u64, p)).collect())
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
    pub async fn forget_old_parties(&self, before: DateTime<Utc>, day: i32) -> anyhow::Result<u64> {
        sqlx::query("DELETE FROM party_daily WHERE day < $1")
            .bind(day)
            .execute(self.pool())
            .await?;
        Ok(
            sqlx::query("DELETE FROM party_games WHERE status = 'done' AND created_at < $1")
                .bind(before)
                .execute(self.pool())
                .await?
                .rows_affected(),
        )
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
