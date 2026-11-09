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
