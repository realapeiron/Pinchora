use super::*;

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
