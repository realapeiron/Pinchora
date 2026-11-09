//! The party games: derby entries, heist crews, storm surge helpers, shell game tables, the
//! gambling switch and the daily limits on what the system pays.
//!
//! Every move of shells here happens in one transaction with the row change that causes it, and a
//! game ends exactly once: whoever gets `Some` back from a settle function owns the ending.
//!
//! The transactions follow the lock rule of the `locks` module: the players' locks first (one player for a
//! join, everyone in the lobby for a settle), then the session's row, then the writes, and a transaction
//! that loses a deadlock anyway runs again.

use chrono::{DateTime, Utc};

use crate::Db;
use crate::economy::legacy::ledger;
use crate::games::games::ACTIVE;
use crate::games::games::DONE;
use crate::games::games::GameSession;
use crate::games::games::NewGame;
use crate::games::games::PENDING;
use crate::infra::locks::lock_users;
use crate::infra::locks::retry;
use crate::infra::locks::try_lock_users;
use crate::sql_id;

/// The columns of a [`GameSession`], for queries that need a literal string.
macro_rules! columns {
    () => {
        "id, kind, guild_id, channel_id, message_id, host_id, guest_id, bet, status, state, expires_at"
    };
}

/// SQL for the day (days since 1970-01-01, UTC) a timestamp falls on, the way [`today`] counts it. Daily rows
/// of a game that holds shells for a while are adjusted on the day the shells were taken, even when the
/// game ends after midnight.
macro_rules! day_of {
    ($timestamp:literal) => {
        concat!("floor(extract(epoch FROM ", $timestamp, ") / 86400)::int")
    };
}

/// Days since 1970-01-01, UTC: what "today" means for the daily limits.
fn today() -> i32 {
    Utc::now().timestamp().div_euclid(86_400) as i32
}

/// The pool split among `n` winners as evenly as whole shells allow: the first few get the odd ones.
fn split(pool: i64, n: usize) -> Vec<i64> {
    let n = n.max(1) as i64;
    (0..n).map(|i| pool / n + i64::from(i < pool % n)).collect()
}

impl Db {
    // /Settings

    /// Whether gambling-style games are on in a server. Off until a manager turns them on.
    pub async fn gambling_enabled(&self, guild: u64) -> anyhow::Result<bool> {
        let row: Option<(bool,)> = sqlx::query_as("SELECT gambling_enabled FROM guilds WHERE id = $1")
            .bind(sql_id(guild))
            .fetch_optional(self.pool())
            .await?;
        Ok(row.is_some_and(|(on,)| on))
    }

    pub async fn set_gambling(&self, guild: u64, on: bool) -> anyhow::Result<()> {
        sqlx::query(
            "INSERT INTO guilds (id, gambling_enabled) VALUES ($1, $2)
             ON CONFLICT (id) DO UPDATE SET gambling_enabled = EXCLUDED.gambling_enabled",
        )
        .bind(sql_id(guild))
        .bind(on)
        .execute(self.pool())
        .await?;
        Ok(())
    }

    /// A server's storm surge settings; the defaults (off) when it never set any.
    pub async fn party_settings(&self, guild: u64) -> anyhow::Result<PartySettings> {
        let row = sqlx::query_as::<_, PartySettings>(
            "SELECT guild_id, surge_enabled, surge_channel_id, surge_every_hours, surge_next_at
             FROM party_settings WHERE guild_id = $1",
        )
        .bind(sql_id(guild))
        .fetch_optional(self.pool())
        .await?;
        Ok(row.unwrap_or(PartySettings {
            guild_id: sql_id(guild),
            surge_enabled: false,
            surge_channel_id: None,
            surge_every_hours: 24,
            surge_next_at: None,
        }))
    }

    /// Changes the storm surge settings (what is `None` stays). Saving schedules the next timed storm
    /// `every` hours from `now` when timed storms are on and have a channel, and cancels it otherwise.
    pub async fn set_surge(
        &self,
        guild: u64,
        enabled: Option<bool>,
        channel: Option<u64>,
        every_hours: Option<i32>,
        now: DateTime<Utc>,
    ) -> anyhow::Result<PartySettings> {
        let mut tx = self.pool().begin().await?;
        sqlx::query(
            "INSERT INTO party_settings (guild_id, surge_enabled, surge_channel_id, surge_every_hours)
             VALUES ($1, COALESCE($2, FALSE), $3, COALESCE($4, 24))
             ON CONFLICT (guild_id) DO UPDATE SET
                 surge_enabled = COALESCE($2, party_settings.surge_enabled),
                 surge_channel_id = COALESCE($3, party_settings.surge_channel_id),
                 surge_every_hours = COALESCE($4, party_settings.surge_every_hours)",
        )
        .bind(sql_id(guild))
        .bind(enabled)
        .bind(channel.map(sql_id))
        .bind(every_hours)
        .execute(&mut *tx)
        .await?;
        let settings = sqlx::query_as::<_, PartySettings>(
            "UPDATE party_settings SET surge_next_at = CASE
                 WHEN surge_enabled AND surge_channel_id IS NOT NULL
                 THEN $2 + make_interval(hours => surge_every_hours) END
             WHERE guild_id = $1
             RETURNING guild_id, surge_enabled, surge_channel_id, surge_every_hours, surge_next_at",
        )
        .bind(sql_id(guild))
        .bind(now)
        .fetch_one(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(settings)
    }

    /// Servers whose timed storm is due, oldest first.
    pub async fn due_surges(&self, now: DateTime<Utc>, limit: i64) -> anyhow::Result<Vec<PartySettings>> {
        Ok(sqlx::query_as::<_, PartySettings>(
            "SELECT guild_id, surge_enabled, surge_channel_id, surge_every_hours, surge_next_at
             FROM party_settings
             WHERE surge_enabled AND surge_channel_id IS NOT NULL AND surge_next_at <= $1
             ORDER BY surge_next_at LIMIT $2",
        )
        .bind(now)
        .bind(limit)
        .fetch_all(self.pool())
        .await?)
    }

    /// Takes a server's due storm slot: moves the next one `every` hours past `now`. True for the one
    /// pod that got there first, which then starts the storm.
    pub async fn claim_surge_slot(&self, guild: u64, due: DateTime<Utc>, now: DateTime<Utc>) -> anyhow::Result<bool> {
        let moved = sqlx::query(
            "UPDATE party_settings SET surge_next_at = $3 + make_interval(hours => surge_every_hours)
             WHERE guild_id = $1 AND surge_enabled AND surge_next_at = $2",
        )
        .bind(sql_id(guild))
        .bind(due)
        .bind(now)
        .execute(self.pool())
        .await?
        .rows_affected();
        Ok(moved == 1)
    }

    // /Daily

    /// Pays `want` shells out of thin air for `game`, at most `cap` shells a day to this player in all;
    /// returns what was paid (0 when the day's limit is reached).
    pub async fn grant_capped(&self, user: u64, game: &str, want: i64, cap: i64, reason: &str) -> anyhow::Result<i64> {
        retry("grant_capped", || self.grant_capped_once(user, game, want, cap, reason)).await
    }

    /// One attempt of [`Db::grant_capped`].
    async fn grant_capped_once(&self, user: u64, game: &str, want: i64, cap: i64, reason: &str) -> anyhow::Result<i64> {
        let mut tx = self.pool().begin().await?;
        lock_users(&mut tx, &[sql_id(user)]).await?;
        let granted = grant_capped(&mut tx, sql_id(user), game, want, cap, reason).await?;
        tx.commit().await?;
        Ok(granted)
    }

    /// What a player has been paid for `game` today.
    pub async fn paid_today(&self, user: u64, game: &str) -> anyhow::Result<i64> {
        let row: Option<(i64,)> =
            sqlx::query_as("SELECT paid FROM party_daily WHERE user_id = $1 AND day = $2 AND game = $3")
                .bind(sql_id(user))
                .bind(today())
                .bind(game)
                .fetch_optional(self.pool())
                .await?;
        Ok(row.map_or(0, |(paid,)| paid))
    }

    /// Games started and net shells won today at the gambling table.
    pub async fn table_today(&self, user: u64, game: &str) -> anyhow::Result<(i32, i64)> {
        let row: Option<(i32, i64)> =
            sqlx::query_as("SELECT plays, net FROM party_daily WHERE user_id = $1 AND day = $2 AND game = $3")
                .bind(sql_id(user))
                .bind(today())
                .bind(game)
                .fetch_optional(self.pool())
                .await?;
        Ok(row.unwrap_or((0, 0)))
    }

    // /Lobbies

    /// Opens a derby, heist or storm. `None` when the channel (or, for a storm, the server) already
    /// has one open: a unique index allows only one.
    pub async fn open_party_game(&self, new: NewGame<'_>) -> anyhow::Result<Option<GameSession>> {
        match self.create_game(new).await {
            Ok(session) => Ok(Some(session)),
            Err(error) if is_unique_violation(&error) => Ok(None),
            Err(error) => Err(error),
        }
    }

    /// Everyone in a lobby or an event, in the order they came.
    pub async fn party_members(&self, session: i64) -> anyhow::Result<Vec<PartyMember>> {
        Ok(sqlx::query_as::<_, PartyMember>(
            "SELECT user_id, choice, amount, name FROM party_members
             WHERE session_id = $1 ORDER BY joined_at, user_id",
        )
        .bind(session)
        .fetch_all(self.pool())
        .await?)
    }

    /// Remembers which message shows a party game, unless it already knows one.
    pub async fn remember_game_message(&self, id: i64, channel_id: u64, message_id: u64) -> anyhow::Result<()> {
        sqlx::query(
            "UPDATE game_sessions
             SET message_id = $3, channel_id = CASE WHEN channel_id = 0 THEN $2 ELSE channel_id END
             WHERE id = $1 AND status <> 'done' AND message_id IS NULL",
        )
        .bind(id)
        .bind(sql_id(channel_id))
        .bind(sql_id(message_id))
        .execute(self.pool())
        .await?;
        Ok(())
    }

    /// Moves a session's text state from `from` to `to` without touching its clock; false when it
    /// is not `from` any more.
    pub async fn touch_game_state(&self, id: i64, from: &str, to: &str) -> anyhow::Result<bool> {
        let moved =
            sqlx::query("UPDATE game_sessions SET state = $3 WHERE id = $1 AND status <> 'done' AND state = $2")
                .bind(id)
                .bind(from)
                .bind(to)
                .execute(self.pool())
                .await?
                .rows_affected();
        Ok(moved == 1)
    }

    // /Heist

    /// A player joins an open heist with a role, or changes their role.
    pub async fn heist_join(
        &self,
        session: i64,
        user: u64,
        name: &str,
        role: i16,
        max_crew: i64,
    ) -> anyhow::Result<CrewJoin> {
        retry("heist_join", || {
            self.heist_join_once(session, user, name, role, max_crew)
        })
        .await
    }

    /// One attempt of [`Db::heist_join`].
    async fn heist_join_once(
        &self,
        session: i64,
        user: u64,
        name: &str,
        role: i16,
        max_crew: i64,
    ) -> anyhow::Result<CrewJoin> {
        let mut tx = self.pool().begin().await?;
        lock_users(&mut tx, &[sql_id(user)]).await?;
        if lock_open(&mut tx, session, "heist").await?.is_none() {
            return Ok(CrewJoin::Closed);
        }
        let moved =
            sqlx::query("UPDATE party_members SET choice = $3, name = $4 WHERE session_id = $1 AND user_id = $2")
                .bind(session)
                .bind(sql_id(user))
                .bind(role)
                .bind(name)
                .execute(&mut *tx)
                .await?
                .rows_affected();
        if moved == 1 {
            tx.commit().await?;
            return Ok(CrewJoin::Moved);
        }
        let (crew,): (i64,) = sqlx::query_as("SELECT count(*) FROM party_members WHERE session_id = $1")
            .bind(session)
            .fetch_one(&mut *tx)
            .await?;
        if crew >= max_crew {
            return Ok(CrewJoin::Full);
        }
        sqlx::query("INSERT INTO party_members (session_id, user_id, choice, name) VALUES ($1, $2, $3, $4)")
            .bind(session)
            .bind(sql_id(user))
            .bind(role)
            .bind(name)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(CrewJoin::Joined)
    }

    /// A crew member leaves an open heist. False when they were not in it, or it is closed.
    pub async fn heist_leave(&self, session: i64, user: u64) -> anyhow::Result<bool> {
        retry("heist_leave", || self.heist_leave_once(session, user)).await
    }

    /// One attempt of [`Db::heist_leave`].
    async fn heist_leave_once(&self, session: i64, user: u64) -> anyhow::Result<bool> {
        let mut tx = self.pool().begin().await?;
        lock_users(&mut tx, &[sql_id(user)]).await?;
        if lock_open(&mut tx, session, "heist").await?.is_none() {
            return Ok(false);
        }
        let gone = sqlx::query("DELETE FROM party_members WHERE session_id = $1 AND user_id = $2")
            .bind(session)
            .bind(sql_id(user))
            .execute(&mut *tx)
            .await?
            .rows_affected();
        tx.commit().await?;
        Ok(gone == 1)
    }

    /// Sets the heist off: ends the lobby and pays the crew, in one transaction. `each` is asked for
    /// the share every member earns once the crew is fixed; it is paid up to `daily_cap` a day.
    /// `forced_by` is the host setting off early; without it the lobby must have run out of time.
    pub async fn heist_settle(
        &self,
        session: i64,
        forced_by: Option<u64>,
        min_crew: usize,
        daily_cap: i64,
        each: fn(&GameSession, &[PartyMember]) -> i64,
    ) -> anyhow::Result<Option<HeistSettled>> {
        retry("heist_settle", || {
            self.heist_settle_once(session, forced_by, min_crew, daily_cap, each)
        })
        .await
    }

    /// One attempt of [`Db::heist_settle`]: every crew member's lock, then the lobby's row.
    async fn heist_settle_once(
        &self,
        session: i64,
        forced_by: Option<u64>,
        min_crew: usize,
        daily_cap: i64,
        each: fn(&GameSession, &[PartyMember]) -> i64,
    ) -> anyhow::Result<Option<HeistSettled>> {
        let mut tx = self.pool().begin().await?;
        lock_members(&mut tx, session).await?;
        let Some(lobby) = lock_game(&mut tx, session, "heist").await? else {
            return Ok(None);
        };
        let due = match forced_by {
            Some(host) => lobby.host_id == sql_id(host),
            None => lobby.expired(),
        };
        if lobby.status != PENDING || !due {
            return Ok(None);
        }
        let crew = members_locked(&mut tx, session).await?;
        if crew.len() < min_crew {
            finish(&mut tx, session, "called-off").await?;
            tx.commit().await?;
            return Ok(Some(HeistSettled::CalledOff { session: lobby, crew }));
        }
        let each = each(&lobby, &crew).max(0);
        let mut order: Vec<&PartyMember> = crew.iter().collect();
        order.sort_by_key(|m| m.user_id);
        let mut granted = Vec::with_capacity(crew.len());
        for member in order {
            let paid = if each > 0 {
                grant_capped(&mut tx, member.user_id, "heist", each, daily_cap, "heist_loot").await?
            } else {
                0
            };
            granted.push((member.user_id as u64, paid));
        }
        granted.sort_by_key(|(user, _)| crew.iter().position(|m| m.user_id as u64 == *user));
        finish(&mut tx, session, "done").await?;
        tx.commit().await?;
        Ok(Some(HeistSettled::Done {
            session: lobby,
            crew,
            each,
            paid: granted,
        }))
    }

    // /Share or grab

    /// Ends a share-or-grab game that pays from the system pool: marks it done and, in the same
    /// transaction, gives the host and the guest what `grants` says, each up to `daily_cap` a day.
    /// Like [`Db::settle_game`], returns the session as it was, or `None` when it was already over,
    /// is not in `from_state`, or is not late yet (when `only_if_expired`). Returns what each was paid.
    pub async fn settle_game_granting(
        &self,
        id: i64,
        from_state: Option<&str>,
        final_state: &str,
        grants: [i64; 2],
        daily_cap: i64,
        only_if_expired: bool,
    ) -> anyhow::Result<Option<(GameSession, [i64; 2])>> {
        retry("settle_game_granting", || {
            self.settle_game_granting_once(id, from_state, final_state, grants, daily_cap, only_if_expired)
        })
        .await
    }

    /// One attempt of [`Db::settle_game_granting`]: both players' locks, then the game's row.
    async fn settle_game_granting_once(
        &self,
        id: i64,
        from_state: Option<&str>,
        final_state: &str,
        grants: [i64; 2],
        daily_cap: i64,
        only_if_expired: bool,
    ) -> anyhow::Result<Option<(GameSession, [i64; 2])>> {
        let mut tx = self.pool().begin().await?;
        lock_players(&mut tx, id).await?;
        let Some(game) = lock_by_id(&mut tx, id).await?.filter(|s| {
            s.status != DONE && from_state.is_none_or(|state| s.state == state) && (!only_if_expired || s.expired())
        }) else {
            return Ok(None);
        };
        let players = [Some(game.host_id), game.guest_id];
        let mut paid = [0i64; 2];
        // In id order, so two games sharing a player cannot deadlock.
        let mut order: Vec<usize> = (0..2).filter(|&i| players[i].is_some() && grants[i] > 0).collect();
        order.sort_by_key(|&i| players[i]);
        for i in order {
            if let Some(user) = players[i] {
                paid[i] = grant_capped(&mut tx, user, "sharegrab", grants[i], daily_cap, "sharegrab_pool").await?;
            }
        }
        finish(&mut tx, id, final_state).await?;
        tx.commit().await?;
        Ok(Some((game, paid)))
    }
}

/// Whether an error from [`Db::create_game`] is a unique index saying the slot is taken.
fn is_unique_violation(error: &anyhow::Error) -> bool {
    error
        .downcast_ref::<sqlx::Error>()
        .and_then(|e| e.as_database_error())
        .is_some_and(|e| e.is_unique_violation())
}

/// Takes the locks of a session's players (its host, and its guest when it has one) before the session's
/// row is locked. They are the ones a shell game or a share-or-grab pays; the two never change, so the plain
/// read is good for the locked one that follows.
async fn lock_players(tx: &mut sqlx::Transaction<'_, sqlx::Postgres>, id: i64) -> anyhow::Result<()> {
    let row: Option<(i64, Option<i64>)> = sqlx::query_as("SELECT host_id, guest_id FROM game_sessions WHERE id = $1")
        .bind(id)
        .fetch_optional(&mut **tx)
        .await?;
    if let Some((host, guest)) = row {
        lock_users(tx, &[Some(host), guest].into_iter().flatten().collect::<Vec<_>>()).await?;
    }
    Ok(())
}

/// Takes the locks of everyone in a lobby, as read now, before its row is locked. Somebody who joins in
/// between is caught by [`members_locked`].
async fn lock_members(tx: &mut sqlx::Transaction<'_, sqlx::Postgres>, session: i64) -> anyhow::Result<()> {
    let ids: Vec<(i64,)> = sqlx::query_as("SELECT user_id FROM party_members WHERE session_id = $1")
        .bind(session)
        .fetch_all(&mut **tx)
        .await?;
    lock_users(tx, &ids.into_iter().map(|(id,)| id).collect::<Vec<_>>()).await
}

/// The members of a lobby whose row is locked, with the lock of every one of them: those [`lock_members`]
/// read are held already, and one who joined since is taken without waiting (waiting for a player while
/// holding the lobby's row would be out of order), or the settle starts over.
async fn members_locked(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    session: i64,
) -> anyhow::Result<Vec<PartyMember>> {
    let list = members(tx, session).await?;
    try_lock_users(tx, &list.iter().map(|m| m.user_id).collect::<Vec<_>>()).await?;
    Ok(list)
}

async fn lock_by_id(tx: &mut sqlx::Transaction<'_, sqlx::Postgres>, id: i64) -> anyhow::Result<Option<GameSession>> {
    Ok(sqlx::query_as(concat!(
        "SELECT ",
        columns!(),
        " FROM game_sessions WHERE id = $1 FOR UPDATE"
    ))
    .bind(id)
    .fetch_optional(&mut **tx)
    .await?)
}

/// Locks a session of `kind`, in whatever state.
async fn lock_game(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    id: i64,
    kind: &str,
) -> anyhow::Result<Option<GameSession>> {
    Ok(lock_by_id(tx, id).await?.filter(|s| s.kind == kind))
}

/// Locks a lobby of `kind` that is still open: pending and not out of time.
async fn lock_open(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    id: i64,
    kind: &str,
) -> anyhow::Result<Option<GameSession>> {
    Ok(lock_game(tx, id, kind)
        .await?
        .filter(|s| s.status == PENDING && !s.expired()))
}

pub(crate) async fn members(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    session: i64,
) -> anyhow::Result<Vec<PartyMember>> {
    Ok(sqlx::query_as::<_, PartyMember>(
        "SELECT user_id, choice, amount, name FROM party_members WHERE session_id = $1 ORDER BY joined_at, user_id",
    )
    .bind(session)
    .fetch_all(&mut **tx)
    .await?)
}

async fn totals(tx: &mut sqlx::Transaction<'_, sqlx::Postgres>, session: i64) -> anyhow::Result<(i64, i64)> {
    Ok(
        sqlx::query_as("SELECT COALESCE(SUM(amount), 0)::BIGINT, count(*) FROM party_members WHERE session_id = $1")
            .bind(session)
            .fetch_one(&mut **tx)
            .await?,
    )
}

/// Marks a session done with its final state.
async fn finish(tx: &mut sqlx::Transaction<'_, sqlx::Postgres>, id: i64, state: &str) -> anyhow::Result<()> {
    sqlx::query("UPDATE game_sessions SET status = 'done', state = $2 WHERE id = $1")
        .bind(id)
        .bind(state)
        .execute(&mut **tx)
        .await?;
    Ok(())
}

/// Puts shells on a balance, with a ledger line.
async fn credit(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    user: i64,
    amount: i64,
    reason: &str,
) -> anyhow::Result<()> {
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

/// Gives every racer of derby `session` their stake back, in id order.
pub(crate) async fn refund_all(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    session: i64,
    racers: &[PartyMember],
) -> anyhow::Result<()> {
    let mut owed: Vec<(i64, i64)> = racers
        .iter()
        .filter(|r| r.amount > 0)
        .map(|r| (r.user_id, r.amount))
        .collect();
    owed.sort_unstable();
    for (user, stake) in owed {
        credit(tx, user, stake, "derby_refund").await?;
        derby_net(tx, session, user, stake).await?;
    }
    Ok(())
}

/// Moves a player's net at the derby tables by `delta`, on the day they joined `session` (the day their
/// stake was counted), whatever day it is now. Does nothing for a player who is not in it.
async fn derby_net(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    session: i64,
    user: i64,
    delta: i64,
) -> anyhow::Result<()> {
    sqlx::query(concat!(
        "UPDATE party_daily SET net = net + $3
         WHERE user_id = $1 AND game = 'derby' AND day = (
             SELECT ",
        day_of!("joined_at"),
        " FROM party_members WHERE session_id = $2 AND user_id = $1)"
    ))
    .bind(user)
    .bind(session)
    .bind(delta)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

/// Adds `delta` shells to a shell game's net and takes `plays_back` games off its count, on the day the
/// table started (`game_sessions.created_at`), whatever day it is now. Does nothing when that day has no row.
async fn shell_net(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    session: i64,
    user: i64,
    delta: i64,
    plays_back: i32,
) -> anyhow::Result<()> {
    sqlx::query(concat!(
        "UPDATE party_daily SET plays = GREATEST(plays - $4, 0), net = net + $3
         WHERE user_id = $1 AND game = 'shellgame' AND day = (
             SELECT ",
        day_of!("created_at"),
        " FROM game_sessions WHERE id = $2)"
    ))
    .bind(user)
    .bind(session)
    .bind(delta)
    .bind(plays_back)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

/// The daily-capped grant inside a transaction; see [`Db::grant_capped`].
async fn grant_capped(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    user: i64,
    game: &str,
    want: i64,
    cap: i64,
    reason: &str,
) -> anyhow::Result<i64> {
    let day = today();
    sqlx::query("INSERT INTO party_daily (user_id, day, game) VALUES ($1, $2, $3) ON CONFLICT DO NOTHING")
        .bind(user)
        .bind(day)
        .bind(game)
        .execute(&mut **tx)
        .await?;
    let (paid,): (i64,) =
        sqlx::query_as("SELECT paid FROM party_daily WHERE user_id = $1 AND day = $2 AND game = $3 FOR UPDATE")
            .bind(user)
            .bind(day)
            .bind(game)
            .fetch_one(&mut **tx)
            .await?;
    let granted = want.min(cap - paid).max(0);
    if granted > 0 {
        sqlx::query("UPDATE party_daily SET paid = paid + $4 WHERE user_id = $1 AND day = $2 AND game = $3")
            .bind(user)
            .bind(day)
            .bind(game)
            .bind(granted)
            .execute(&mut **tx)
            .await?;
        credit(tx, user, granted, reason).await?;
    }
    Ok(granted)
}

#[cfg(test)]
mod tests {
    use crate::games::party::split;

    #[test]
    fn a_pool_is_shared_out_to_the_last_shell() {
        assert_eq!(split(100, 4), vec![25; 4]);
        assert_eq!(split(100, 3), vec![34, 33, 33]);
        assert_eq!(split(7, 5), vec![2, 2, 1, 1, 1]);
        assert_eq!(split(5, 1), vec![5]);
        assert_eq!(split(0, 3), vec![0; 3]);
        for pool in [1, 9, 150, 999] {
            for n in 1..=12 {
                assert_eq!(split(pool, n).iter().sum::<i64>(), pool);
            }
        }
    }
}

mod derby;

mod surge;

mod shells;

mod players;
pub use players::{
    CrewJoin, DerbyJoin, DerbyLeave, DerbyOutcome, DerbyResult, HeistSettled, PartyMember, PartySettings, ShellDone,
    ShellLimits, ShellPick, ShellStart, SurgeHelp, SurgeSettled,
};
