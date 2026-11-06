//! The party games: derby entries, heist crews, storm surge helpers, shell game tables, the
//! gambling switch and the daily limits on what the system pays.
//!
//! Every move of shells here happens in one transaction with the row change that causes it, and a
//! game ends exactly once: whoever gets `Some` back from a settle function owns the ending.

use chrono::{DateTime, Utc};

use crate::{
    Db,
    economy::ledger,
    games::{ACTIVE, DONE, GameSession, NewGame, PENDING},
    sql_id,
};

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

/// Someone in a lobby or an event: a derby racer, a heist crew member or a surge helper.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct PartyMember {
    pub user_id: i64,
    /// The crab (derby) or the role (heist).
    pub choice: i16,
    /// The stake held (derby) or the sandbags stacked (surge).
    pub amount: i64,
    pub name: String,
}

/// A server's storm surge settings.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct PartySettings {
    pub guild_id: i64,
    pub surge_enabled: bool,
    pub surge_channel_id: Option<i64>,
    pub surge_every_hours: i32,
    pub surge_next_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DerbyJoin {
    Joined,
    /// The player was in already and picked another crab; nothing was charged again.
    Moved,
    /// The lobby is closed, or gone.
    Closed,
    Broke {
        balance: i64,
    },
    Full,
    /// This stake would take the player past the day's derby loss limit; `net` is where they stand
    /// (shells won less shells staked and not given back).
    Down {
        net: i64,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DerbyLeave {
    /// The player left; `refund` is the stake they got back.
    Left {
        refund: i64,
    },
    NotIn,
    Closed,
}

/// What a derby came to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DerbyOutcome {
    /// Nobody (or too few for a race with shells at stake) joined; every stake is back.
    Cancelled,
    /// Nobody backed the winner; every stake is back.
    Refunded,
    /// Those who backed the winner shared the pool: each as (player, shells paid).
    Paid(Vec<(u64, i64)>),
    /// A race for fun: nothing was staked. The players who backed the winner.
    Friendly(Vec<u64>),
}

#[derive(Debug, Clone)]
pub struct DerbyResult {
    /// The lobby as it was before the race.
    pub session: GameSession,
    pub racers: Vec<PartyMember>,
    /// The winning crab.
    pub winner: i16,
    pub outcome: DerbyOutcome,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CrewJoin {
    Joined,
    /// Already in the crew; the role changed.
    Moved,
    Closed,
    Full,
}

#[derive(Debug, Clone)]
pub enum HeistSettled {
    /// Too few joined: the crew stays home, nothing is paid.
    CalledOff {
        session: GameSession,
        crew: Vec<PartyMember>,
    },
    /// The heist ran: `each` is the share the crew earned, `paid` what each member actually got
    /// after their daily limit.
    Done {
        session: GameSession,
        crew: Vec<PartyMember>,
        each: i64,
        paid: Vec<(u64, i64)>,
    },
}

#[derive(Debug, Clone)]
pub enum SurgeHelp {
    /// A sandbag was stacked: the helper's own count, the total stacked and how many have helped.
    Helped {
        session: GameSession,
        mine: i64,
        stacked: i64,
        helpers: i64,
    },
    /// The storm is over, or not running.
    Over,
    /// This helper has stacked all they may.
    Capped,
}

#[derive(Debug, Clone)]
pub struct SurgeSettled {
    pub session: GameSession,
    pub held: bool,
    /// Every helper with the sandbags they stacked and the shells they got.
    pub helpers: Vec<(PartyMember, i64)>,
}

/// What a player may do at the shell game table in a day.
#[derive(Debug, Clone, Copy)]
pub struct ShellLimits {
    pub plays: i32,
    pub loss: i64,
    pub win: i64,
}

#[derive(Debug, Clone)]
pub enum ShellStart {
    Started(GameSession),
    Broke {
        balance: i64,
    },
    /// Today's games are used up.
    TooMany,
    /// This stake would take the player past the day's loss limit; `net` is where they stand.
    Down {
        net: i64,
    },
    /// The player is up as far as the table lets them be today.
    Up {
        net: i64,
    },
}

#[derive(Debug, Clone)]
pub enum ShellPick {
    Done(ShellDone),
    NotYours,
    /// Over already, or out of time.
    Gone,
}

#[derive(Debug, Clone)]
pub struct ShellDone {
    pub session: GameSession,
    pub pearl: u8,
    pub picked: u8,
    /// Shells paid back; 0 for a wrong guess.
    pub paid: i64,
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
        let mut tx = self.pool().begin().await?;
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

    // /Derby

    /// A player picks a crab in an open derby. With a stake the stake comes off their balance, once, and
    /// counts against `loss_limit` shells a day; picking again moves them to the other crab for free.
    pub async fn derby_join(
        &self,
        session: i64,
        user: u64,
        name: &str,
        crab: i16,
        max_racers: i64,
        loss_limit: i64,
    ) -> anyhow::Result<DerbyJoin> {
        let mut tx = self.pool().begin().await?;
        let Some(lobby) = lock_open(&mut tx, session, "derby").await? else {
            return Ok(DerbyJoin::Closed);
        };
        let user = sql_id(user);
        let moved =
            sqlx::query("UPDATE party_members SET choice = $3, name = $4 WHERE session_id = $1 AND user_id = $2")
                .bind(session)
                .bind(user)
                .bind(crab)
                .bind(name)
                .execute(&mut *tx)
                .await?
                .rows_affected();
        if moved == 1 {
            tx.commit().await?;
            return Ok(DerbyJoin::Moved);
        }
        let (racers,): (i64,) = sqlx::query_as("SELECT count(*) FROM party_members WHERE session_id = $1")
            .bind(session)
            .fetch_one(&mut *tx)
            .await?;
        if racers >= max_racers {
            return Ok(DerbyJoin::Full);
        }
        if lobby.bet > 0 {
            sqlx::query("INSERT INTO users (id) VALUES ($1) ON CONFLICT (id) DO NOTHING")
                .bind(user)
                .execute(&mut *tx)
                .await?;
            let (balance,): (i64,) = sqlx::query_as("SELECT credits FROM users WHERE id = $1 FOR UPDATE")
                .bind(user)
                .fetch_one(&mut *tx)
                .await?;
            if balance < lobby.bet {
                return Ok(DerbyJoin::Broke { balance });
            }
            // The day's row is the one of the transaction's own clock, which stamps `joined_at` too: that
            // is the row a refund or a payout finds again later.
            sqlx::query(concat!(
                "INSERT INTO party_daily (user_id, day, game) VALUES ($1, ",
                day_of!("now()"),
                ", 'derby') ON CONFLICT DO NOTHING"
            ))
            .bind(user)
            .execute(&mut *tx)
            .await?;
            let (net,): (i64,) = sqlx::query_as(concat!(
                "SELECT net FROM party_daily WHERE user_id = $1 AND day = ",
                day_of!("now()"),
                " AND game = 'derby' FOR UPDATE"
            ))
            .bind(user)
            .fetch_one(&mut *tx)
            .await?;
            if net - lobby.bet < -loss_limit {
                return Ok(DerbyJoin::Down { net });
            }
            sqlx::query("UPDATE users SET credits = credits - $2 WHERE id = $1")
                .bind(user)
                .bind(lobby.bet)
                .execute(&mut *tx)
                .await?;
            ledger(&mut tx, user, -lobby.bet, "derby_bet").await?;
            sqlx::query(concat!(
                "UPDATE party_daily SET net = net - $2 WHERE user_id = $1 AND day = ",
                day_of!("now()"),
                " AND game = 'derby'"
            ))
            .bind(user)
            .bind(lobby.bet)
            .execute(&mut *tx)
            .await?;
        }
        sqlx::query(
            "INSERT INTO party_members (session_id, user_id, choice, amount, name) VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(session)
        .bind(user)
        .bind(crab)
        .bind(lobby.bet)
        .bind(name)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(DerbyJoin::Joined)
    }

    /// A racer leaves an open derby and gets their stake back.
    pub async fn derby_leave(&self, session: i64, user: u64) -> anyhow::Result<DerbyLeave> {
        let mut tx = self.pool().begin().await?;
        if lock_open(&mut tx, session, "derby").await?.is_none() {
            return Ok(DerbyLeave::Closed);
        }
        let user = sql_id(user);
        let held: Option<(i64,)> =
            sqlx::query_as("SELECT amount FROM party_members WHERE session_id = $1 AND user_id = $2 FOR UPDATE")
                .bind(session)
                .bind(user)
                .fetch_optional(&mut *tx)
                .await?;
        let Some((refund,)) = held else {
            return Ok(DerbyLeave::NotIn);
        };
        if refund > 0 {
            credit(&mut tx, user, refund, "derby_refund").await?;
            derby_net(&mut tx, session, user, refund).await?;
        }
        sqlx::query("DELETE FROM party_members WHERE session_id = $1 AND user_id = $2")
            .bind(session)
            .bind(user)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(DerbyLeave::Left { refund })
    }

    /// Runs the derby: ends the lobby and pays, in one transaction. `winner` is the crab that won the
    /// race the lobby's seed says. `forced_by` is the host starting the race early; without it the
    /// lobby must have run out of time. `None` when somebody else ended it first.
    pub async fn derby_settle(
        &self,
        session: i64,
        forced_by: Option<u64>,
        min_staked: usize,
        winner: i16,
    ) -> anyhow::Result<Option<DerbyResult>> {
        let mut tx = self.pool().begin().await?;
        let Some(lobby) = lock_game(&mut tx, session, "derby").await? else {
            return Ok(None);
        };
        let due = match forced_by {
            Some(host) => lobby.host_id == sql_id(host),
            None => lobby.expired(),
        };
        if lobby.status != PENDING || !due {
            return Ok(None);
        }
        let racers = members(&mut tx, session).await?;
        let staked = lobby.bet > 0;
        let outcome = if racers.is_empty() || (staked && racers.len() < min_staked) {
            refund_all(&mut tx, session, &racers).await?;
            DerbyOutcome::Cancelled
        } else if !staked {
            let winners = racers.iter().filter(|r| r.choice == winner).map(|r| r.user_id as u64);
            DerbyOutcome::Friendly(winners.collect())
        } else {
            let winners: Vec<&PartyMember> = racers.iter().filter(|r| r.choice == winner).collect();
            if winners.is_empty() {
                refund_all(&mut tx, session, &racers).await?;
                DerbyOutcome::Refunded
            } else {
                let pool: i64 = racers.iter().map(|r| r.amount).sum();
                let shares = split(pool, winners.len());
                let mut paid: Vec<(u64, i64)> =
                    winners.iter().zip(shares).map(|(w, s)| (w.user_id as u64, s)).collect();
                // Pay in id order, like every multi-player payout, so two settles cannot deadlock.
                paid.sort_unstable();
                for (user, share) in &paid {
                    credit(&mut tx, sql_id(*user), *share, "derby_win").await?;
                    derby_net(&mut tx, session, sql_id(*user), *share).await?;
                }
                paid.sort_by_key(|(user, _)| racers.iter().position(|r| r.user_id as u64 == *user));
                DerbyOutcome::Paid(paid)
            }
        };
        finish(&mut tx, session, "done").await?;
        tx.commit().await?;
        Ok(Some(DerbyResult {
            session: lobby,
            racers,
            winner,
            outcome,
        }))
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
        let mut tx = self.pool().begin().await?;
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
        let mut tx = self.pool().begin().await?;
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
        let mut tx = self.pool().begin().await?;
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
        let crew = members(&mut tx, session).await?;
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

    // /Surge

    /// A player stacks a sandbag on a running storm. Counts one sandbag for them, up to `max_helps`.
    pub async fn surge_help(&self, session: i64, user: u64, name: &str, max_helps: i64) -> anyhow::Result<SurgeHelp> {
        let mut tx = self.pool().begin().await?;
        let Some(storm) = lock_game(&mut tx, session, "surge")
            .await?
            .filter(|s| s.status == ACTIVE && !s.expired())
        else {
            return Ok(SurgeHelp::Over);
        };
        let mine: Option<(i64,)> = sqlx::query_as(
            "INSERT INTO party_members (session_id, user_id, amount, name) VALUES ($1, $2, 1, $3)
             ON CONFLICT (session_id, user_id) DO UPDATE
                 SET amount = party_members.amount + 1, name = EXCLUDED.name
                 WHERE party_members.amount < $4
             RETURNING amount",
        )
        .bind(session)
        .bind(sql_id(user))
        .bind(name)
        .bind(max_helps)
        .fetch_optional(&mut *tx)
        .await?;
        let Some((mine,)) = mine else {
            return Ok(SurgeHelp::Capped);
        };
        let (stacked, helpers) = totals(&mut tx, session).await?;
        tx.commit().await?;
        Ok(SurgeHelp::Helped {
            session: storm,
            mine,
            stacked,
            helpers,
        })
    }

    /// Storms that are running and not out of time, oldest first, for keeping their card fresh.
    pub async fn running_surges(&self, limit: i64) -> anyhow::Result<Vec<GameSession>> {
        Ok(sqlx::query_as::<_, GameSession>(concat!(
            "SELECT ",
            columns!(),
            " FROM game_sessions
             WHERE kind = 'surge' AND status = 'active' AND expires_at > now() ORDER BY id LIMIT $1"
        ))
        .bind(limit)
        .fetch_all(self.pool())
        .await?)
    }

    /// Sandbags stacked and how many have helped so far.
    pub async fn surge_totals(&self, session: i64) -> anyhow::Result<(i64, i64)> {
        let mut tx = self.pool().begin().await?;
        let counts = totals(&mut tx, session).await?;
        tx.commit().await?;
        Ok(counts)
    }

    /// Ends a storm: marks it done and pays every helper, in one transaction. `held` says whether the
    /// wall held; a storm that did not hold is only ended once its time is up (`only_if_expired`).
    /// `reward` is what a helper gets for the sandbags they stacked, paid up to `daily_cap` a day.
    pub async fn surge_settle(
        &self,
        session: i64,
        held: bool,
        only_if_expired: bool,
        daily_cap: i64,
        reward: fn(bool, i64) -> i64,
    ) -> anyhow::Result<Option<SurgeSettled>> {
        let mut tx = self.pool().begin().await?;
        let Some(storm) = lock_game(&mut tx, session, "surge")
            .await?
            .filter(|s| s.status == ACTIVE && (!only_if_expired || s.expired()))
        else {
            return Ok(None);
        };
        let helpers = members(&mut tx, session).await?;
        let mut order: Vec<&PartyMember> = helpers.iter().collect();
        order.sort_by_key(|m| m.user_id);
        let mut paid = Vec::with_capacity(helpers.len());
        for helper in order {
            let want = reward(held, helper.amount);
            let got = if want > 0 {
                grant_capped(&mut tx, helper.user_id, "surge", want, daily_cap, "surge_reward").await?
            } else {
                0
            };
            paid.push((helper.user_id, got));
        }
        finish(&mut tx, session, if held { "held" } else { "broken" }).await?;
        tx.commit().await?;
        let helpers = helpers
            .into_iter()
            .map(|h| {
                let got = paid
                    .iter()
                    .find(|(user, _)| *user == h.user_id)
                    .map_or(0, |(_, got)| *got);
                (h, got)
            })
            .collect();
        Ok(Some(SurgeSettled {
            session: storm,
            held,
            helpers,
        }))
    }

    // /Shells

    /// A player sits down at the shell game: the stake comes off their balance and the pearl is put
    /// under shell `pearl`. Refused when they cannot cover it or have used the day's allowance.
    pub async fn shell_start(&self, new: NewGame<'_>, limits: ShellLimits) -> anyhow::Result<ShellStart> {
        let user = sql_id(new.host);
        let stake = new.bet;
        let mut tx = self.pool().begin().await?;
        sqlx::query("INSERT INTO users (id) VALUES ($1) ON CONFLICT (id) DO NOTHING")
            .bind(user)
            .execute(&mut *tx)
            .await?;
        let (balance,): (i64,) = sqlx::query_as("SELECT credits FROM users WHERE id = $1 FOR UPDATE")
            .bind(user)
            .fetch_one(&mut *tx)
            .await?;
        if balance < stake {
            return Ok(ShellStart::Broke { balance });
        }
        sqlx::query("INSERT INTO party_daily (user_id, day, game) VALUES ($1, $2, 'shellgame') ON CONFLICT DO NOTHING")
            .bind(user)
            .bind(today())
            .execute(&mut *tx)
            .await?;
        let (plays, net): (i32, i64) = sqlx::query_as(
            "SELECT plays, net FROM party_daily WHERE user_id = $1 AND day = $2 AND game = 'shellgame' FOR UPDATE",
        )
        .bind(user)
        .bind(today())
        .fetch_one(&mut *tx)
        .await?;
        if plays >= limits.plays {
            return Ok(ShellStart::TooMany);
        }
        if net - stake < -limits.loss {
            return Ok(ShellStart::Down { net });
        }
        if net >= limits.win {
            return Ok(ShellStart::Up { net });
        }
        sqlx::query("UPDATE users SET credits = credits - $2 WHERE id = $1")
            .bind(user)
            .bind(stake)
            .execute(&mut *tx)
            .await?;
        ledger(&mut tx, user, -stake, "shells_bet").await?;
        sqlx::query(
            "UPDATE party_daily SET plays = plays + 1, net = net - $4
             WHERE user_id = $1 AND day = $2 AND game = $3",
        )
        .bind(user)
        .bind(today())
        .bind("shellgame")
        .bind(stake)
        .execute(&mut *tx)
        .await?;
        let session: GameSession = sqlx::query_as(concat!(
            "INSERT INTO game_sessions (kind, guild_id, channel_id, host_id, guest_id, bet, status, state, expires_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, now() + make_interval(secs => $9))
             RETURNING ",
            columns!()
        ))
        .bind(new.kind)
        .bind(sql_id(new.guild_id))
        .bind(sql_id(new.channel_id))
        .bind(user)
        .bind(new.guest.map(sql_id))
        .bind(stake)
        .bind(new.status)
        .bind(new.state)
        .bind(new.ttl_secs as f64)
        .fetch_one(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(ShellStart::Started(session))
    }

    /// The player picks a shell: the game ends, and a right guess is paid `payout(stake)`.
    pub async fn shell_pick(
        &self,
        session: i64,
        user: u64,
        pick: u8,
        payout: fn(i64) -> i64,
    ) -> anyhow::Result<ShellPick> {
        let mut tx = self.pool().begin().await?;
        let Some(game) = lock_game(&mut tx, session, "shellgame")
            .await?
            .filter(|s| s.status == ACTIVE)
        else {
            return Ok(ShellPick::Gone);
        };
        if game.host_id != sql_id(user) {
            return Ok(ShellPick::NotYours);
        }
        if game.expired() {
            return Ok(ShellPick::Gone);
        }
        let pearl: u8 = game
            .state
            .parse()
            .map_err(|_| anyhow::anyhow!("bad shell game state {:?}", game.state))?;
        let paid = if pick == pearl { payout(game.bet) } else { 0 };
        if paid > 0 {
            credit(&mut tx, game.host_id, paid, "shells_win").await?;
            shell_net(&mut tx, session, game.host_id, paid, 0).await?;
        }
        finish(&mut tx, session, &format!("{pearl}:{pick}")).await?;
        tx.commit().await?;
        Ok(ShellPick::Done(ShellDone {
            session: game,
            pearl,
            picked: pick,
            paid,
        }))
    }

    /// Gives the stake back for a shell game nobody finished, once its time is up. `None` when it
    /// is over already or not late yet.
    pub async fn shell_refund(&self, session: i64) -> anyhow::Result<Option<GameSession>> {
        let mut tx = self.pool().begin().await?;
        let Some(game) = lock_game(&mut tx, session, "shellgame")
            .await?
            .filter(|s| s.status == ACTIVE && s.expired())
        else {
            return Ok(None);
        };
        credit(&mut tx, game.host_id, game.bet, "shells_refund").await?;
        shell_net(&mut tx, session, game.host_id, game.bet, 1).await?;
        finish(&mut tx, session, "timeout").await?;
        tx.commit().await?;
        Ok(Some(game))
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
        let mut tx = self.pool().begin().await?;
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

async fn members(tx: &mut sqlx::Transaction<'_, sqlx::Postgres>, session: i64) -> anyhow::Result<Vec<PartyMember>> {
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
async fn refund_all(
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
    use super::split;

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
