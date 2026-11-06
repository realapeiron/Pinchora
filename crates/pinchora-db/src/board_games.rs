//! What the board games keep: pixel picross boards, Tide 2048 games, the best scores and the Sea Fleet
//! layouts captains are choosing.

use crate::{Db, sql_id};

/// A pixel picross board.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct PicrossGame {
    pub user_id: i64,
    pub mode: String,
    pub day: i32,
    pub picture: i16,
    pub variant: i16,
    /// The player's marks: 100 characters of `#` and `.`.
    pub grid: String,
    /// How many times the player asked how many of their marks are wrong.
    pub checks: i16,
    pub status: String,
}

impl PicrossGame {
    pub fn solved(&self) -> bool {
        self.status == "solved"
    }
}

/// A Tide 2048 game.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct SoloGame {
    pub user_id: i64,
    pub game: String,
    pub state: String,
    pub score: i32,
    pub moves: i32,
    pub status: String,
}

impl SoloGame {
    pub fn over(&self) -> bool {
        self.status == "over"
    }
}

/// The columns of a [`PicrossGame`] and of a [`SoloGame`], for queries that need a literal string.
macro_rules! picross_columns {
    () => {
        "user_id, mode, day, picture, variant, grid, checks, status"
    };
}
macro_rules! solo_columns {
    () => {
        "user_id, game, state, score, moves, status"
    };
}

impl Db {
    /// Counts a finished game for a player and keeps their highest score.
    pub async fn record_game_score(&self, user: u64, game: &str, won: bool, score: i32) -> anyhow::Result<()> {
        sqlx::query(
            "INSERT INTO game_stats (user_id, game, played, won, best) VALUES ($1, $2, 1, $3, $4)
             ON CONFLICT (user_id, game) DO UPDATE
             SET played = game_stats.played + 1, won = game_stats.won + EXCLUDED.won,
                 best = GREATEST(game_stats.best, EXCLUDED.best)",
        )
        .bind(sql_id(user))
        .bind(game)
        .bind(i32::from(won))
        .bind(score)
        .execute(self.pool())
        .await?;
        Ok(())
    }

    pub async fn picross_game(&self, user: u64, mode: &str, day: i32) -> anyhow::Result<Option<PicrossGame>> {
        Ok(sqlx::query_as::<_, PicrossGame>(concat!(
            "SELECT ",
            picross_columns!(),
            " FROM picross_games WHERE user_id = $1 AND mode = $2 AND day = $3"
        ))
        .bind(sql_id(user))
        .bind(mode)
        .bind(day)
        .fetch_optional(self.pool())
        .await?)
    }

    /// Starts a board on `picture` in `variant`. With `replace` an existing board for the same slot is
    /// thrown away (the next practice puzzle); without it an existing board is kept and returned (the
    /// daily puzzle is played once a day, and a second `/play picross` picks up where the first left off).
    pub async fn start_picross(
        &self,
        user: u64,
        mode: &str,
        day: i32,
        (picture, variant): (u8, u8),
        replace: bool,
    ) -> anyhow::Result<PicrossGame> {
        let sql = if replace {
            "INSERT INTO picross_games (user_id, mode, day, picture, variant, grid)
             VALUES ($1, $2, $3, $4, $5, $6) ON CONFLICT (user_id, mode, day)
             DO UPDATE SET picture = EXCLUDED.picture, variant = EXCLUDED.variant, grid = EXCLUDED.grid,
                 checks = 0, status = 'playing', started_at = now(), solved_at = NULL"
        } else {
            "INSERT INTO picross_games (user_id, mode, day, picture, variant, grid)
             VALUES ($1, $2, $3, $4, $5, $6) ON CONFLICT (user_id, mode, day) DO NOTHING"
        };
        sqlx::query(sql)
            .bind(sql_id(user))
            .bind(mode)
            .bind(day)
            .bind(i16::from(picture))
            .bind(i16::from(variant))
            .bind(".".repeat(100))
            .execute(self.pool())
            .await?;
        self.picross_game(user, mode, day)
            .await?
            .ok_or_else(|| anyhow::anyhow!("picross board vanished"))
    }

    /// Replaces a board's marks, if they are still `from` (nobody marked in between) and the board is
    /// still being played. With `solved` the board becomes solved in the same update. `None` when the
    /// marks moved on or the board is finished, so a double click can neither lose a mark nor pay twice.
    pub async fn picross_marks(
        &self,
        user: u64,
        mode: &str,
        day: i32,
        from: &str,
        to: &str,
        solved: bool,
    ) -> anyhow::Result<Option<PicrossGame>> {
        Ok(sqlx::query_as::<_, PicrossGame>(concat!(
            "UPDATE picross_games
             SET grid = $5,
                 status = CASE WHEN $6 THEN 'solved' ELSE status END,
                 solved_at = CASE WHEN $6 THEN now() ELSE solved_at END
             WHERE user_id = $1 AND mode = $2 AND day = $3 AND grid = $4 AND status = 'playing'
             RETURNING ",
            picross_columns!()
        ))
        .bind(sql_id(user))
        .bind(mode)
        .bind(day)
        .bind(from)
        .bind(to)
        .bind(solved)
        .fetch_optional(self.pool())
        .await?)
    }

    /// Uses one of a board's checks, if it still has one under `limit`, and returns the board as it is
    /// then. `None` when none are left or the board is not being played, so a double click cannot spend
    /// more than the limit.
    pub async fn picross_check(
        &self,
        user: u64,
        mode: &str,
        day: i32,
        limit: i16,
    ) -> anyhow::Result<Option<PicrossGame>> {
        Ok(sqlx::query_as::<_, PicrossGame>(concat!(
            "UPDATE picross_games SET checks = checks + 1
             WHERE user_id = $1 AND mode = $2 AND day = $3 AND status = 'playing' AND checks < $4
             RETURNING ",
            picross_columns!()
        ))
        .bind(sql_id(user))
        .bind(mode)
        .bind(day)
        .bind(limit)
        .fetch_optional(self.pool())
        .await?)
    }

    /// The days (since 1970-01-01) a player solved the daily puzzle, newest first, at most `limit`.
    pub async fn picross_days_solved(&self, user: u64, limit: i64) -> anyhow::Result<Vec<i32>> {
        let rows: Vec<(i32,)> = sqlx::query_as(
            "SELECT day FROM picross_games WHERE user_id = $1 AND mode = 'daily' AND status = 'solved'
             ORDER BY day DESC LIMIT $2",
        )
        .bind(sql_id(user))
        .bind(limit)
        .fetch_all(self.pool())
        .await?;
        Ok(rows.into_iter().map(|(day,)| day).collect())
    }

    /// Keeps a layout for a captain's placement in a game, in place of the one they had, and returns its
    /// revision (it goes up by one every time).
    pub async fn set_fleet_draft(&self, session: i64, user: u64, layout: &str) -> anyhow::Result<i64> {
        let (rev,): (i64,) = sqlx::query_as(
            "INSERT INTO fleet_drafts (session_id, user_id, layout) VALUES ($1, $2, $3)
             ON CONFLICT (session_id, user_id)
             DO UPDATE SET layout = EXCLUDED.layout, rev = fleet_drafts.rev + 1
             RETURNING rev",
        )
        .bind(session)
        .bind(sql_id(user))
        .bind(layout)
        .fetch_one(self.pool())
        .await?;
        Ok(rev)
    }

    /// The layout a captain is looking at, with its revision.
    pub async fn fleet_draft(&self, session: i64, user: u64) -> anyhow::Result<Option<(String, i64)>> {
        Ok(
            sqlx::query_as("SELECT layout, rev FROM fleet_drafts WHERE session_id = $1 AND user_id = $2")
                .bind(session)
                .bind(sql_id(user))
                .fetch_optional(self.pool())
                .await?,
        )
    }

    /// Takes a captain's layout out of the drafts, if it is still at `rev`. `None` when they have a newer
    /// one or none, so a click on an older message locks in nothing, and a double click locks in once.
    pub async fn take_fleet_draft(&self, session: i64, user: u64, rev: i64) -> anyhow::Result<Option<String>> {
        let row: Option<(String,)> = sqlx::query_as(
            "DELETE FROM fleet_drafts WHERE session_id = $1 AND user_id = $2 AND rev = $3 RETURNING layout",
        )
        .bind(session)
        .bind(sql_id(user))
        .bind(rev)
        .fetch_optional(self.pool())
        .await?;
        Ok(row.map(|(layout,)| layout))
    }

    pub async fn solo_game(&self, user: u64, game: &str) -> anyhow::Result<Option<SoloGame>> {
        Ok(sqlx::query_as::<_, SoloGame>(concat!(
            "SELECT ",
            solo_columns!(),
            " FROM solo_games WHERE user_id = $1 AND game = $2"
        ))
        .bind(sql_id(user))
        .bind(game)
        .fetch_optional(self.pool())
        .await?)
    }

    /// Starts a solo game at `state`. With `replace` the player's previous one is thrown away; without
    /// it an unfinished one is kept and returned.
    pub async fn start_solo(&self, user: u64, game: &str, state: &str, replace: bool) -> anyhow::Result<SoloGame> {
        let sql = if replace {
            "INSERT INTO solo_games (user_id, game, state) VALUES ($1, $2, $3)
             ON CONFLICT (user_id, game)
             DO UPDATE SET state = EXCLUDED.state, score = 0, moves = 0, status = 'playing', updated_at = now()"
        } else {
            "INSERT INTO solo_games (user_id, game, state) VALUES ($1, $2, $3)
             ON CONFLICT (user_id, game) DO NOTHING"
        };
        sqlx::query(sql)
            .bind(sql_id(user))
            .bind(game)
            .bind(state)
            .execute(self.pool())
            .await?;
        self.solo_game(user, game)
            .await?
            .ok_or_else(|| anyhow::anyhow!("solo game vanished"))
    }

    /// Moves a solo game on from the move count the player saw to the next state. False when the game
    /// is no longer at `from_moves` (another click moved first) or is over; so two clicks at once make
    /// one move, and only one of them ends the game.
    pub async fn advance_solo(
        &self,
        user: u64,
        game: &str,
        from_moves: i32,
        next: &SoloStep<'_>,
    ) -> anyhow::Result<bool> {
        let done = sqlx::query(
            "UPDATE solo_games
             SET state = $4, score = $5, moves = $3 + 1,
                 status = CASE WHEN $6 THEN 'over' ELSE 'playing' END, updated_at = now()
             WHERE user_id = $1 AND game = $2 AND moves = $3 AND status = 'playing'",
        )
        .bind(sql_id(user))
        .bind(game)
        .bind(from_moves)
        .bind(next.state)
        .bind(next.score)
        .bind(next.over)
        .execute(self.pool())
        .await?
        .rows_affected();
        Ok(done == 1)
    }
}

/// What a solo game becomes after a move.
#[derive(Debug, Clone, Copy)]
pub struct SoloStep<'a> {
    pub state: &'a str,
    pub score: i32,
    /// Whether this move ended the game.
    pub over: bool,
}
