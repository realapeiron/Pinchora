/// The numbers achievement badges are earned from. All zero for a player never seen before.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PlayerStats {
    /// From their total XP.
    pub level: i64,
    /// Different fish in the fish book.
    pub fish_species: i64,
    /// Likes their house has received.
    pub house_likes: i64,
    pub pets: i64,
    /// The daily streak as it stands now: 0 once they missed the window to extend it.
    pub streak: i64,
    /// Furniture standing in their rooms.
    pub placed: i64,
    pub reputation: i64,
    pub shells: i64,
}

/// What the rule for wearing an accessory needs to know about a player (see
/// `pinchora_game::character::Shelf`): their numbers (the level and what achievement badges count)
/// and what they bought.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WardrobeFacts {
    pub stats: PlayerStats,
    /// Shop badge ids they bought.
    pub bought_badges: Vec<String>,
    /// Accessory ids they bought.
    pub bought_accessories: Vec<String>,
}
