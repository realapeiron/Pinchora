//! Share or grab: two players secretly pick, and the pool goes by what they picked together.

/// Shells the sea puts in the pool of a game nobody wagered on.
pub const POOL: i64 = 60;
/// Seconds a pair of players has to pick.
pub const PICK_SECS: i64 = 120;
/// A pair is paid from the system pool at most once in this many seconds.
pub const PAIR_COOLDOWN_SECS: u64 = 3_600;
/// Shells the system pays one player per day across all their share-or-grab games.
pub const DAILY_SHELLS: i64 = 180;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pick {
    Share,
    Grab,
}

impl Pick {
    pub const ALL: [Pick; 2] = [Self::Share, Self::Grab];

    /// One letter, as kept in custom ids and the database.
    pub fn key(self) -> char {
        match self {
            Self::Share => 's',
            Self::Grab => 'g',
        }
    }

    pub fn from_key(key: char) -> Option<Self> {
        Self::ALL.into_iter().find(|p| p.key() == key)
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Share => "Share",
            Self::Grab => "Grab",
        }
    }

    pub fn emoji(self) -> &'static str {
        match self {
            Self::Share => "🤝",
            Self::Grab => "🦀",
        }
    }
}

/// How a game ended, with the players numbered 0 (host) and 1 (guest).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// Both shared: the pool is split evenly.
    BothShare,
    /// One grabbed while the other shared: the grabber takes the whole pool.
    Grabbed(usize),
    /// Both grabbed: nobody gets anything.
    BothGrab,
}

pub fn outcome(host: Pick, guest: Pick) -> Outcome {
    match (host, guest) {
        (Pick::Share, Pick::Share) => Outcome::BothShare,
        (Pick::Grab, Pick::Share) => Outcome::Grabbed(0),
        (Pick::Share, Pick::Grab) => Outcome::Grabbed(1),
        (Pick::Grab, Pick::Grab) => Outcome::BothGrab,
    }
}

impl Outcome {
    /// What each player takes from a pool of `pool` shells (the host's share first).
    pub fn split(self, pool: i64) -> [i64; 2] {
        match self {
            Self::BothShare => [pool / 2, pool - pool / 2],
            Self::Grabbed(0) => [pool, 0],
            Self::Grabbed(_) => [0, pool],
            Self::BothGrab => [0, 0],
        }
    }
}

/// The two picks of a running game, kept in its state: `<host>:<guest>`, each `s`, `g` or `-`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Picks(pub [Option<Pick>; 2]);

impl Picks {
    pub fn encode(self) -> String {
        let one = |p: Option<Pick>| p.map_or('-', Pick::key);
        format!("{}:{}", one(self.0[0]), one(self.0[1]))
    }

    pub fn decode(text: &str) -> Option<Self> {
        let (a, b) = text.split_once(':')?;
        let one = |s: &str| match s {
            "-" => Some(None),
            other if other.chars().count() == 1 => other.chars().next().and_then(Pick::from_key).map(Some),
            _ => None,
        };
        Some(Self([one(a)?, one(b)?]))
    }

    /// The outcome once both have picked.
    pub fn outcome(self) -> Option<Outcome> {
        match self.0 {
            [Some(a), Some(b)] => Some(outcome(a, b)),
            _ => None,
        }
    }

    /// Who has not picked yet (0 host, 1 guest).
    pub fn waiting(self) -> Vec<usize> {
        (0..2).filter(|&i| self.0[i].is_none()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_four_outcomes_split_the_pool_and_never_create_shells() {
        use Pick::{Grab, Share};
        assert_eq!(outcome(Share, Share), Outcome::BothShare);
        assert_eq!(outcome(Grab, Share), Outcome::Grabbed(0));
        assert_eq!(outcome(Share, Grab), Outcome::Grabbed(1));
        assert_eq!(outcome(Grab, Grab), Outcome::BothGrab);
        for pool in [0, 1, 59, 60, 101, 1_000] {
            for o in [
                Outcome::BothShare,
                Outcome::Grabbed(0),
                Outcome::Grabbed(1),
                Outcome::BothGrab,
            ] {
                let [a, b] = o.split(pool);
                assert!(a >= 0 && b >= 0 && a + b <= pool, "{o:?} of {pool}: {a} + {b}");
            }
            let [a, b] = Outcome::BothShare.split(pool);
            assert_eq!(a + b, pool, "an odd shell goes to the guest, not the sea");
        }
        assert_eq!(Outcome::BothShare.split(60), [30, 30]);
        assert_eq!(Outcome::Grabbed(1).split(60), [0, 60]);
    }

    #[test]
    fn picks_round_trip_through_the_state() {
        let mut picks = Picks::default();
        assert_eq!(picks.encode(), "-:-");
        assert_eq!(picks.waiting(), vec![0, 1]);
        picks.0[1] = Some(Pick::Grab);
        assert_eq!(picks.encode(), "-:g");
        assert_eq!(Picks::decode("-:g"), Some(picks));
        assert_eq!(picks.waiting(), vec![0]);
        assert_eq!(picks.outcome(), None);
        picks.0[0] = Some(Pick::Share);
        assert_eq!(picks.outcome(), Some(Outcome::Grabbed(1)));
        for bad in ["", "s", "s:", "x:s", "ss:s", "s:s:s"] {
            assert_eq!(Picks::decode(bad), None, "{bad:?}");
        }
    }
}
