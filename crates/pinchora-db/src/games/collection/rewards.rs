use super::*;

/// A server's chat-drop settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, sqlx::FromRow)]
pub struct CollectionSettings {
    pub enabled: bool,
    pub channel_id: Option<i64>,
}

/// One entry of a player's dex.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct DexRow {
    pub entry_id: String,
    pub caught: i32,
    pub kept: i32,
    pub glimmer: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct WishlistRow {
    pub entry_id: String,
    pub ping_on_spawn: bool,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct CollectionRoll {
    pub entry_id: String,
    pub glimmer: bool,
    pub rolled_at: DateTime<Utc>,
    pub claimed_at: Option<DateTime<Utc>>,
}

pub enum RollStart {
    Started(CollectionRoll),
    Wait(CollectionRoll),
    Cooldown,
    Limit,
}

pub enum RollClaim {
    Claimed(Granted),
    Gone,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VoteCard {
    Cast,
    Gone,
    Full,
}

/// What an entry pays when a player gets it, worked out by the game rules.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Award<'a> {
    pub entry: &'a str,
    /// Shells for the first one (the discovery).
    pub new_shells: i64,
    /// Shells for a spare, which is recycled.
    pub dup_shells: i64,
    /// Fragments a recycled spare leaves.
    pub dup_fragments: i64,
    /// Fragments on top, new or not (a salvaged plank).
    pub bonus_fragments: i64,
}

/// What a player got for one entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Granted {
    pub entry: String,
    /// The first of its kind: it is in their dex now.
    pub new: bool,
    /// How many they have met, this one included.
    pub caught: i32,
    pub shells: i64,
    pub fragments: i64,
}

/// A game's daily limit on plays that pay: the first `cap` plays of a UTC `day` pay shells and
/// fragments, later ones still fill the dex.
#[derive(Debug, Clone, Copy)]
pub struct Daily<'a> {
    pub game: &'a str,
    pub day: i32,
    pub cap: i32,
}

pub const CRITTERS: &str = "critters";
pub const BOTTLE: &str = "bottle";
pub const CLAIMS_PER_DAY: i32 = 10;
pub const PROTECTION_SECS: i64 = 15;

/// A critter drop or a bottle on its message.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct DropRow {
    pub id: i64,
    pub kind: String,
    pub guild_id: i64,
    pub channel_id: i64,
    pub message_id: Option<i64>,
    pub host_id: Option<i64>,
    pub cards: Vec<String>,
    /// Who holds each slot; 0 while it is free.
    pub takers: Vec<i64>,
    pub status: String,
    pub expires_at: DateTime<Utc>,
    pub mode: String,
    pub challenge_word: Option<String>,
    pub vote_winner: Option<i16>,
    pub created_at: DateTime<Utc>,
    pub protected_user_id: Option<i64>,
    pub protected_slot: Option<i16>,
    pub protected_until: Option<DateTime<Utc>>,
}

impl DropRow {
    pub fn is_open(&self) -> bool {
        self.status == "open"
    }

    /// Whether `user` holds a slot.
    pub fn taken_by(&self, user: u64) -> bool {
        self.takers.contains(&sql_id(user))
    }
}

#[derive(Debug, Clone, Copy)]
pub struct NewDrop<'a> {
    pub kind: &'a str,
    pub guild_id: u64,
    pub channel_id: u64,
    pub host: Option<u64>,
    pub cards: &'a [String],
    pub expires_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub enum Grab {
    /// The card is theirs.
    Taken {
        granted: Granted,
        drop: Box<DropRow>,
        /// False when the day's paying plays were used up: the card is theirs, nothing was paid.
        paid: bool,
    },
    /// The drop is over, or the slot is not what the message showed.
    Gone,
    /// They already took a card from this drop.
    Have {
        slot: usize,
    },
    /// Somebody else got there first.
    Lost {
        by: u64,
    },
    Protected,
    Limit,
    Voting,
    Challenge,
}

/// What is in a bottle besides its message.
#[derive(Debug, Clone, Copy)]
pub enum BottlePrize<'a> {
    Shells(i64),
    Fragments(i64),
    Treasure(i64),
    Find(Award<'a>),
}

#[derive(Debug, Clone)]
pub enum BottleOpen {
    Opened {
        drop: Box<DropRow>,
        shells: i64,
        fragments: i64,
        /// The find, if it held one.
        granted: Option<Granted>,
        paid: bool,
    },
    Gone,
    /// Somebody opened it first.
    Lost {
        by: u64,
    },
}

#[derive(Debug, Clone)]
pub enum Comb {
    Found {
        granted: Granted,
        paid: bool,
    },
    /// Not yet: they walked the shore a moment ago.
    Wait {
        ready_at: DateTime<Utc>,
    },
}

/// A player's crab pot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PotRow {
    pub level: i16,
    /// The soak the pot was set for, while one is running.
    pub hours: Option<i16>,
    pub ends_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone)]
pub enum PotHaul {
    Hauled {
        granted: Vec<Granted>,
        /// False when the day's paying hauls were used up.
        paid: bool,
        level: i16,
        hours: i16,
    },
    /// Still soaking until `ends_at`.
    Soaking { ends_at: DateTime<Utc> },
    /// Nothing is in the water.
    Empty,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PotUpgrade {
    Upgraded {
        level: i16,
        balance: i64,
    },
    /// The pot is not at the level the caller read (somebody upgraded it already).
    Stale,
    Poor {
        balance: i64,
    },
}
