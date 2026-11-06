//! Game sessions with escrowed bets, trivia answers, wordle boards and win counts.

use chrono::{DateTime, Utc};

use crate::{
    Db,
    economy::ledger,
    locks::{lock_users, retry},
    sql_id,
};

pub const PENDING: &str = "pending";
pub const ACTIVE: &str = "active";
pub const DONE: &str = "done";

/// A duel, hangman game or trivia round. `state` is the owning game's own text.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct GameSession {
    pub id: i64,
    pub kind: String,
    pub guild_id: i64,
    pub channel_id: i64,
    pub message_id: Option<i64>,
    pub host_id: i64,
    pub guest_id: Option<i64>,
    /// Shells each player put up; held while the session is `active`.
    pub bet: i64,
    pub status: String,
    pub state: String,
    pub expires_at: DateTime<Utc>,
}

impl GameSession {
    pub fn is_active(&self) -> bool {
        self.status == ACTIVE
    }

    pub fn expired(&self) -> bool {
        self.expires_at <= Utc::now()
    }

    /// Whether `user` is one of the two players.
    pub fn has_player(&self, user: u64) -> bool {
        let user = sql_id(user);
        self.host_id == user || self.guest_id == Some(user)
    }
}

/// A session to open.
#[derive(Debug, Clone, Copy)]
pub struct NewGame<'a> {
    pub kind: &'a str,
    pub guild_id: u64,
    pub channel_id: u64,
    pub host: u64,
    pub guest: Option<u64>,
    /// Shells each player puts up when the guest accepts; 0 plays for fun.
    pub bet: i64,
    pub status: &'a str,
    pub state: &'a str,
    /// Seconds until the session's clock runs out.
    pub ttl_secs: i64,
}

/// What happens to the held bets when a session ends.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Payout {
    /// Nothing is held, or nothing moves.
    None,
    /// The winner takes both bets.
    Winner(u64),
    /// Both players get their bet back.
    Refund,
}

#[derive(Debug, Clone)]
pub enum DuelStart {
    /// The challenge was accepted and any bets are now held. Holds the session as it now is.
    Started(GameSession),
    /// Not pending any more: declined, taken back, timed out or already accepted.
    Gone,
    /// Somebody else was challenged.
    NotInvited,
    /// This player cannot cover the bet.
    Broke { user: u64, balance: i64 },
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct GameStat {
    pub game: String,
    pub played: i32,
    pub won: i32,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct WordleGame {
    pub user_id: i64,
    pub mode: String,
    pub day: i32,
    pub word: String,
    pub guesses: Vec<String>,
    pub status: String,
}

/// The columns of a [`GameSession`], for queries that need a literal string.
macro_rules! columns {
    () => {
        "id, kind, guild_id, channel_id, message_id, host_id, guest_id, bet, status, state, expires_at"
    };
}

impl Db {
    /// Opens a session. Its players' locks come first (see the `locks` module), so a session is
    /// never opened for a player in the middle of deleting their data, and the delete finds it.
    pub async fn create_game(&self, new: NewGame<'_>) -> anyhow::Result<GameSession> {
        retry("create_game", || self.create_game_once(new)).await
    }

    /// One attempt of [`Db::create_game`].
    async fn create_game_once(&self, new: NewGame<'_>) -> anyhow::Result<GameSession> {
        let mut tx = self.pool().begin().await?;
        lock_users(
            &mut tx,
            &[Some(new.host), new.guest]
                .into_iter()
                .flatten()
                .map(sql_id)
                .collect::<Vec<_>>(),
        )
        .await?;
        let session = sqlx::query_as::<_, GameSession>(concat!(
            "INSERT INTO game_sessions (kind, guild_id, channel_id, host_id, guest_id, bet, status, state, expires_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, now() + make_interval(secs => $9))
             RETURNING ",
            columns!()
        ))
        .bind(new.kind)
        .bind(sql_id(new.guild_id))
        .bind(sql_id(new.channel_id))
        .bind(sql_id(new.host))
        .bind(new.guest.map(sql_id))
        .bind(new.bet)
        .bind(new.status)
        .bind(new.state)
        .bind(new.ttl_secs as f64)
        .fetch_one(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(session)
    }

    pub async fn game(&self, id: i64) -> anyhow::Result<Option<GameSession>> {
        Ok(
            sqlx::query_as::<_, GameSession>(concat!("SELECT ", columns!(), " FROM game_sessions WHERE id = $1"))
                .bind(id)
                .fetch_optional(self.pool())
                .await?,
        )
    }

    /// Remembers which message shows the session, so a timeout can edit it later.
    pub async fn set_game_message(&self, id: i64, channel_id: u64, message_id: u64) -> anyhow::Result<()> {
        sqlx::query("UPDATE game_sessions SET channel_id = $2, message_id = $3 WHERE id = $1 AND status <> 'done'")
            .bind(id)
            .bind(sql_id(channel_id))
            .bind(sql_id(message_id))
            .execute(self.pool())
            .await?;
        Ok(())
    }

    /// Moves an active session on from `from` to `to` and restarts its clock. False when the
    /// state is no longer `from` (somebody else moved first) or the session is over.
    pub async fn advance_game(&self, id: i64, from: &str, to: &str, ttl_secs: i64) -> anyhow::Result<bool> {
        let moved = sqlx::query(
            "UPDATE game_sessions SET state = $3, expires_at = now() + make_interval(secs => $4)
             WHERE id = $1 AND status = 'active' AND state = $2 AND expires_at > now()",
        )
        .bind(id)
        .bind(from)
        .bind(to)
        .bind(ttl_secs as f64)
        .execute(self.pool())
        .await?
        .rows_affected();
        Ok(moved == 1)
    }

    /// The challenged player accepts on `message` (channel and message id): holds both bets (all or
    /// nothing, in one transaction through the ledger) and makes the session active.
    pub async fn start_duel(
        &self,
        id: i64,
        guest: u64,
        message: (u64, u64),
        state: &str,
        ttl_secs: i64,
    ) -> anyhow::Result<DuelStart> {
        retry("start_duel", || {
            self.start_duel_once(id, guest, message, state, ttl_secs)
        })
        .await
    }

    /// One attempt of [`Db::start_duel`].
    async fn start_duel_once(
        &self,
        id: i64,
        guest: u64,
        message: (u64, u64),
        state: &str,
        ttl_secs: i64,
    ) -> anyhow::Result<DuelStart> {
        let mut tx = self.pool().begin().await?;
        lock_players(&mut tx, id).await?;
        let session: Option<GameSession> = sqlx::query_as(concat!(
            "SELECT ",
            columns!(),
            " FROM game_sessions WHERE id = $1 FOR UPDATE"
        ))
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?;
        let Some(session) = session.filter(|s| s.status == PENDING && !s.expired()) else {
            return Ok(DuelStart::Gone);
        };
        if session.guest_id != Some(sql_id(guest)) {
            return Ok(DuelStart::NotInvited);
        }
        if session.bet > 0 {
            let players = [session.host_id, sql_id(guest)];
            sqlx::query("INSERT INTO users (id) VALUES ($1), ($2) ON CONFLICT (id) DO NOTHING")
                .bind(players[0])
                .bind(players[1])
                .execute(&mut *tx)
                .await?;
            // Lock both rows in id order, like a transfer, so two duels cannot deadlock.
            let balances: Vec<(i64, i64)> =
                sqlx::query_as("SELECT id, credits FROM users WHERE id = ANY($1) ORDER BY id FOR UPDATE")
                    .bind(players.to_vec())
                    .fetch_all(&mut *tx)
                    .await?;
            for player in players {
                let balance = balances.iter().find(|(user, _)| *user == player).map_or(0, |(_, c)| *c);
                if balance < session.bet {
                    return Ok(DuelStart::Broke {
                        user: player as u64,
                        balance,
                    });
                }
            }
            for player in players {
                sqlx::query("UPDATE users SET credits = credits - $2 WHERE id = $1")
                    .bind(player)
                    .bind(session.bet)
                    .execute(&mut *tx)
                    .await?;
                ledger(&mut tx, player, -session.bet, "duel_bet").await?;
            }
        }
        let started: GameSession = sqlx::query_as(concat!(
            "UPDATE game_sessions
             SET status = 'active', channel_id = $5, message_id = $2, state = $3,
                 expires_at = now() + make_interval(secs => $4)
             WHERE id = $1 RETURNING ",
            columns!()
        ))
        .bind(id)
        .bind(sql_id(message.1))
        .bind(state)
        .bind(ttl_secs as f64)
        .bind(sql_id(message.0))
        .fetch_one(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(DuelStart::Started(started))
    }

    /// Ends a session exactly once: marks it done and, in the same transaction, pays out any held
    /// bets. Returns the session as it was, or `None` when it was already over, its state is not
    /// `from_state` (if given: somebody moved first), or it is not yet late (when
    /// `only_if_expired`). Whoever gets `Some` owns the ending; nobody else will.
    pub async fn settle_game(
        &self,
        id: i64,
        from_state: Option<&str>,
        final_state: &str,
        payout: Payout,
        only_if_expired: bool,
    ) -> anyhow::Result<Option<GameSession>> {
        retry("settle_game", || {
            self.settle_game_once(id, from_state, final_state, payout, only_if_expired)
        })
        .await
    }

    /// One attempt of [`Db::settle_game`].
    async fn settle_game_once(
        &self,
        id: i64,
        from_state: Option<&str>,
        final_state: &str,
        payout: Payout,
        only_if_expired: bool,
    ) -> anyhow::Result<Option<GameSession>> {
        let mut tx = self.pool().begin().await?;
        lock_players(&mut tx, id).await?;
        let session: Option<GameSession> = sqlx::query_as(concat!(
            "SELECT ",
            columns!(),
            " FROM game_sessions WHERE id = $1 FOR UPDATE"
        ))
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?;
        let Some(session) = session.filter(|s| {
            s.status != DONE && from_state.is_none_or(|state| s.state == state) && (!only_if_expired || s.expired())
        }) else {
            return Ok(None);
        };
        if session.status == ACTIVE && session.bet > 0 {
            match payout {
                Payout::None => {}
                Payout::Winner(winner) => {
                    let winner = sql_id(winner);
                    anyhow::ensure!(
                        winner == session.host_id || Some(winner) == session.guest_id,
                        "the winner is not in the game"
                    );
                    pay(&mut tx, winner, session.bet * 2, "duel_pot").await?;
                }
                Payout::Refund => {
                    pay(&mut tx, session.host_id, session.bet, "duel_refund").await?;
                    if let Some(guest) = session.guest_id {
                        pay(&mut tx, guest, session.bet, "duel_refund").await?;
                    }
                }
            }
        }
        sqlx::query("UPDATE game_sessions SET status = 'done', state = $2 WHERE id = $1")
            .bind(id)
            .bind(final_state)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(Some(session))
    }

    /// Sessions whose clock ran out and that nobody ended, oldest first.
    pub async fn late_games(&self, limit: i64) -> anyhow::Result<Vec<GameSession>> {
        Ok(sqlx::query_as::<_, GameSession>(concat!(
            "SELECT ",
            columns!(),
            " FROM game_sessions
             WHERE status <> 'done' AND expires_at < now() ORDER BY expires_at LIMIT $1"
        ))
        .bind(limit)
        .fetch_all(self.pool())
        .await?)
    }

    /// Drops finished sessions (and their trivia answers) after two days; returns how many. Rows
    /// somebody holds are left for the next sweep (see the `locks` module).
    pub async fn forget_old_games(&self) -> anyhow::Result<u64> {
        retry("forget_old_games", || async move {
            Ok(sqlx::query(
                "WITH old AS MATERIALIZED (
                     SELECT id FROM game_sessions WHERE status = 'done' AND created_at < now() - interval '2 days'
                     FOR UPDATE SKIP LOCKED)
                 DELETE FROM game_sessions WHERE id IN (SELECT id FROM old)",
            )
            .execute(self.pool())
            .await?
            .rows_affected())
        })
        .await
    }

    /// Records a trivia answer; false when this player already answered the round.
    pub async fn add_game_answer(&self, session: i64, user: u64, choice: i16) -> anyhow::Result<bool> {
        let added = sqlx::query(
            "INSERT INTO game_answers (session_id, user_id, choice) VALUES ($1, $2, $3) ON CONFLICT DO NOTHING",
        )
        .bind(session)
        .bind(sql_id(user))
        .bind(choice)
        .execute(self.pool())
        .await?
        .rows_affected();
        Ok(added == 1)
    }

    /// Who answered a trivia round with which slot, in the order they answered.
    pub async fn game_answers(&self, session: i64) -> anyhow::Result<Vec<(u64, i16)>> {
        let rows: Vec<(i64, i16)> =
            sqlx::query_as("SELECT user_id, choice FROM game_answers WHERE session_id = $1 ORDER BY ctid")
                .bind(session)
                .fetch_all(self.pool())
                .await?;
        Ok(rows.into_iter().map(|(user, choice)| (user as u64, choice)).collect())
    }

    /// Counts a finished game for a player.
    pub async fn record_game(&self, user: u64, game: &str, won: bool) -> anyhow::Result<()> {
        sqlx::query(
            "INSERT INTO game_stats (user_id, game, played, won) VALUES ($1, $2, 1, $3)
             ON CONFLICT (user_id, game) DO UPDATE
             SET played = game_stats.played + 1, won = game_stats.won + EXCLUDED.won",
        )
        .bind(sql_id(user))
        .bind(game)
        .bind(i32::from(won))
        .execute(self.pool())
        .await?;
        Ok(())
    }

    pub async fn game_stats(&self, user: u64) -> anyhow::Result<Vec<GameStat>> {
        Ok(
            sqlx::query_as::<_, GameStat>("SELECT game, played, won FROM game_stats WHERE user_id = $1")
                .bind(sql_id(user))
                .fetch_all(self.pool())
                .await?,
        )
    }

    pub async fn wordle_game(&self, user: u64, mode: &str, day: i32) -> anyhow::Result<Option<WordleGame>> {
        Ok(sqlx::query_as::<_, WordleGame>(
            "SELECT user_id, mode, day, word, guesses, status FROM wordle_games
             WHERE user_id = $1 AND mode = $2 AND day = $3",
        )
        .bind(sql_id(user))
        .bind(mode)
        .bind(day)
        .fetch_optional(self.pool())
        .await?)
    }

    /// Starts a board with `word`. With `replace` an existing board for the same slot is thrown
    /// away (the next practice word); without it an existing board is kept and returned (the daily
    /// word can be played once a day).
    pub async fn start_wordle(
        &self,
        user: u64,
        mode: &str,
        day: i32,
        word: &str,
        replace: bool,
    ) -> anyhow::Result<WordleGame> {
        let sql = if replace {
            "INSERT INTO wordle_games (user_id, mode, day, word) VALUES ($1, $2, $3, $4)
             ON CONFLICT (user_id, mode, day) DO UPDATE
             SET word = EXCLUDED.word, guesses = '{}', status = 'playing', updated_at = now()"
        } else {
            "INSERT INTO wordle_games (user_id, mode, day, word) VALUES ($1, $2, $3, $4)
             ON CONFLICT (user_id, mode, day) DO NOTHING"
        };
        sqlx::query(sql)
            .bind(sql_id(user))
            .bind(mode)
            .bind(day)
            .bind(word)
            .execute(self.pool())
            .await?;
        self.wordle_game(user, mode, day)
            .await?
            .ok_or_else(|| anyhow::anyhow!("wordle board vanished"))
    }

    /// Adds a guess to a board still being played; the status becomes `won`, `lost` (sixth guess)
    /// or stays `playing`. `None` when the board is finished or missing, so a double click cannot
    /// add two guesses or pay twice.
    pub async fn wordle_guess(
        &self,
        user: u64,
        mode: &str,
        day: i32,
        guess: &str,
    ) -> anyhow::Result<Option<WordleGame>> {
        Ok(sqlx::query_as::<_, WordleGame>(
            "UPDATE wordle_games
             SET guesses = array_append(guesses, $4),
                 status = CASE WHEN $4 = word THEN 'won'
                               WHEN cardinality(guesses) + 1 >= 6 THEN 'lost'
                               ELSE 'playing' END,
                 updated_at = now()
             WHERE user_id = $1 AND mode = $2 AND day = $3 AND status = 'playing' AND cardinality(guesses) < 6
             RETURNING user_id, mode, day, word, guesses, status",
        )
        .bind(sql_id(user))
        .bind(mode)
        .bind(day)
        .bind(guess)
        .fetch_optional(self.pool())
        .await?)
    }
}

/// Takes the locks of the session's players when it holds (or will hold) shells, before the session's
/// row is locked (see the `locks` module). The players and the bet of a session never change, so the
/// plain read is good for the locked one that follows.
async fn lock_players(tx: &mut sqlx::Transaction<'_, sqlx::Postgres>, id: i64) -> anyhow::Result<()> {
    let row: Option<(i64, Option<i64>, i64)> =
        sqlx::query_as("SELECT host_id, guest_id, bet FROM game_sessions WHERE id = $1")
            .bind(id)
            .fetch_optional(&mut **tx)
            .await?;
    if let Some((host, guest, bet)) = row
        && bet > 0
    {
        lock_users(tx, &[Some(host), guest].into_iter().flatten().collect::<Vec<_>>()).await?;
    }
    Ok(())
}

async fn pay(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    user: i64,
    amount: i64,
    reason: &str,
) -> anyhow::Result<()> {
    sqlx::query("UPDATE users SET credits = credits + $2 WHERE id = $1")
        .bind(user)
        .bind(amount)
        .execute(&mut **tx)
        .await?;
    ledger(tx, user, amount, reason).await
}
