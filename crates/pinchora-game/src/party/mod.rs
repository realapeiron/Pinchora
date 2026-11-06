//! The party games: pure rules and content for Hot Pearl, Acronym Tide, Caption the Postcard, Wordle
//! Race, Would You Rather and Truth or Dare. Nothing here touches the database or Discord.

pub mod acronym;
pub mod filter;
pub mod hotpearl;
pub mod postcard;
pub mod prompts;
pub mod race;

use crate::play::Game;

/// Which party game a row of `party_games` is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Party {
    HotPearl,
    Acronym,
    Postcard,
    WordRace,
    TruthOrDare,
    WouldYouRather,
}

impl Party {
    pub const ALL: [Party; 6] = [
        Self::HotPearl,
        Self::Acronym,
        Self::Postcard,
        Self::WordRace,
        Self::TruthOrDare,
        Self::WouldYouRather,
    ];

    /// What the database keeps in `party_games.kind`.
    pub fn key(self) -> &'static str {
        match self {
            Self::HotPearl => "hotpearl",
            Self::Acronym => "acronym",
            Self::Postcard => "postcard",
            Self::WordRace => "wordrace",
            Self::TruthOrDare => "truthordare",
            Self::WouldYouRather => "wouldyourrather",
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|p| p.key() == key)
    }

    /// The game whose custom ids start with `prefix`.
    pub fn from_prefix(prefix: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|p| p.prefix() == prefix)
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::HotPearl => "Hot Pearl",
            Self::Acronym => "Acronym Tide",
            Self::Postcard => "Caption the Postcard",
            Self::WordRace => "Wordle Race",
            Self::TruthOrDare => "Truth or Dare",
            Self::WouldYouRather => "Would You Rather",
        }
    }

    pub fn emoji(self) -> &'static str {
        match self {
            Self::HotPearl => "🦪",
            Self::Acronym => "🌊",
            Self::Postcard => "💌",
            Self::WordRace => "🏁",
            Self::TruthOrDare => "🎲",
            Self::WouldYouRather => "⚖️",
        }
    }

    /// The prefix of the game's custom ids.
    pub fn prefix(self) -> &'static str {
        match self {
            Self::HotPearl => "hl",
            Self::Acronym => "ac",
            Self::Postcard => "cn",
            Self::WordRace => "wx",
            Self::TruthOrDare => "td",
            Self::WouldYouRather => "wy",
        }
    }

    /// The counter `/play stats` keeps the game's plays in.
    pub fn game(self) -> Game {
        match self {
            Self::HotPearl => Game::HotPearl,
            Self::Acronym => Game::Acronym,
            Self::Postcard => Game::Postcard,
            Self::WordRace => Game::WordRace,
            Self::TruthOrDare => Game::TruthOrDare,
            Self::WouldYouRather => Game::WouldYouRather,
        }
    }

    /// Fewest and most players a lobby takes. A poll has no lobby.
    pub fn players(self) -> (usize, usize) {
        match self {
            Self::HotPearl => (2, 8),
            Self::Acronym | Self::Postcard => (3, 10),
            Self::WordRace => (2, 4),
            Self::TruthOrDare => (2, 10),
            Self::WouldYouRather => (1, usize::MAX),
        }
    }

    /// How long a lobby waits for a start before it is given up.
    pub fn lobby_secs(self) -> i64 {
        180
    }
}

/// Points for one vote in a written round.
pub const VOTE_POINTS: i32 = 1;
/// Extra points for a round's winner, each of them on a tie.
pub const WIN_BONUS: i32 = 2;
/// The most shells one written game pays one player.
pub const WRITING_REWARD_CAP: i64 = 60;

/// The authors with the most votes of a round, if anybody got any: more than one on a tie.
pub fn round_winners(ballots: &[(u64, u32)]) -> Vec<u64> {
    let best = ballots.iter().map(|(_, votes)| *votes).max().unwrap_or(0);
    if best == 0 {
        return Vec::new();
    }
    ballots
        .iter()
        .filter(|(_, votes)| *votes == best)
        .map(|(author, _)| *author)
        .collect()
}

/// What a round is worth to the author of an entry with `votes` votes.
pub fn round_points(votes: u32, won: bool) -> i32 {
    votes as i32 * VOTE_POINTS + if won { WIN_BONUS } else { 0 }
}

/// What a written game pays a player who wrote at least once and ended on `score` points.
pub fn writing_reward(score: i32) -> i64 {
    (4 + 3 * i64::from(score.max(0))).min(WRITING_REWARD_CAP)
}

/// Shells for the last player standing in Hot Pearl: more for a fuller table.
pub fn pearl_reward(players: usize) -> i64 {
    20 + 4 * players.saturating_sub(1).min(7) as i64
}

/// Shells for a Wordle Race win: fewer tries pay more.
pub fn race_reward(tries: usize) -> i64 {
    15 + 4 * race::MAX_TRIES.saturating_sub(tries.max(1)) as i64
}

/// Shells for a vote in Would You Rather.
pub const POLL_VOTE_REWARD: i64 = 1;
/// Shells for finishing a dare or a truth.
pub const TURN_REWARD: i64 = 4;

/// A tiny deterministic generator for picking things from a seed: splitmix64.
#[derive(Debug, Clone)]
pub struct Dice(u64);

impl Dice {
    pub fn new(seed: u64) -> Self {
        Self(seed)
    }

    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    /// A number in `0..n`; `n` must not be 0.
    pub fn below(&mut self, n: usize) -> usize {
        (self.next_u64() % n as u64) as usize
    }

    /// Shuffles `items` in place.
    pub fn shuffle<T>(&mut self, items: &mut [T]) {
        for i in (1..items.len()).rev() {
            items.swap(i, self.below(i + 1));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn party_keys_and_prefixes_are_unique_and_round_trip() {
        for (i, party) in Party::ALL.iter().enumerate() {
            assert_eq!(Party::from_key(party.key()), Some(*party));
            for other in &Party::ALL[i + 1..] {
                assert_ne!(party.key(), other.key());
                assert_ne!(party.prefix(), other.prefix());
            }
            assert!(party.prefix().len() == 2);
        }
        assert_eq!(Party::from_key("chess"), None);
    }

    #[test]
    fn ties_share_the_round_and_no_votes_means_no_winner() {
        assert_eq!(round_winners(&[(1, 2), (2, 3), (3, 1)]), vec![2]);
        assert_eq!(round_winners(&[(1, 3), (2, 3), (3, 1)]), vec![1, 2]);
        assert!(round_winners(&[(1, 0), (2, 0)]).is_empty());
        assert!(round_winners(&[]).is_empty());
        assert_eq!(round_points(3, true), 5);
        assert_eq!(round_points(0, false), 0);
    }

    #[test]
    fn rewards_stay_well_under_the_hourly_ceiling() {
        // A player is paid for at most `daily_cap` games a day, so a day's best case bounds an hour's.
        let best = |game: Game, per_play: i64| per_play * game.daily_cap();
        assert!(best(Game::HotPearl, pearl_reward(8)) <= 600);
        assert!(best(Game::Acronym, WRITING_REWARD_CAP) <= 600);
        assert!(best(Game::Postcard, WRITING_REWARD_CAP) <= 600);
        assert!(best(Game::WordRace, race_reward(1)) <= 600);
        assert!(best(Game::WouldYouRather, POLL_VOTE_REWARD) <= 600);
        assert!(best(Game::TruthOrDare, TURN_REWARD) <= 600);
        assert_eq!(pearl_reward(2), 24);
        assert_eq!(pearl_reward(8), 48);
        assert_eq!(race_reward(1), 35);
        assert_eq!(race_reward(6), 15);
        assert_eq!(writing_reward(0), 4);
        assert_eq!(writing_reward(100), WRITING_REWARD_CAP);
    }

    #[test]
    fn the_dice_are_repeatable_and_shuffle_keeps_every_item() {
        let mut a = Dice::new(7);
        let mut b = Dice::new(7);
        assert_eq!(a.next_u64(), b.next_u64());
        let mut items: Vec<u32> = (0..20).collect();
        a.shuffle(&mut items);
        let mut sorted = items.clone();
        sorted.sort_unstable();
        assert_eq!(sorted, (0..20).collect::<Vec<_>>());
        assert_ne!(
            items, sorted,
            "twenty items stay in order with odds of one in 2.4 quintillion"
        );
        assert!((0..100).all(|_| a.below(5) < 5));
    }
}
