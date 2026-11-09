//! Daily quests: three per player per UTC day, drawn from a fixed pool.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuestKind {
    Chat,
    Fish,
    Visit,
    Care,
    Place,
    Buy,
    Like,
    Daily,
}

#[derive(Debug, Clone, Copy)]
pub struct Quest {
    pub kind: QuestKind,
    pub target: i64,
    pub reward: i64,
    pub text: &'static str,
}

pub const POOL: [Quest; 8] = [
    Quest {
        kind: QuestKind::Chat,
        target: 10,
        reward: 150,
        text: "Chat and earn XP 10 times",
    },
    Quest {
        kind: QuestKind::Fish,
        target: 3,
        reward: 120,
        text: "Catch 3 fish",
    },
    Quest {
        kind: QuestKind::Visit,
        target: 2,
        reward: 100,
        text: "Visit 2 houses",
    },
    Quest {
        kind: QuestKind::Care,
        target: 3,
        reward: 100,
        text: "Look after your pet 3 times",
    },
    Quest {
        kind: QuestKind::Place,
        target: 2,
        reward: 80,
        text: "Place 2 pieces of furniture",
    },
    Quest {
        kind: QuestKind::Buy,
        target: 1,
        reward: 60,
        text: "Buy something in the shop",
    },
    Quest {
        kind: QuestKind::Like,
        target: 1,
        reward: 50,
        text: "Like someone's house",
    },
    Quest {
        kind: QuestKind::Daily,
        target: 1,
        reward: 50,
        text: "Collect your daily shells",
    },
];

/// Pearls for finishing all three quests of a day.
pub const ALL_CLEAR_PEARLS: i64 = 5;

impl QuestKind {
    pub fn key(self) -> &'static str {
        match self {
            Self::Chat => "chat",
            Self::Fish => "fish",
            Self::Visit => "visit",
            Self::Care => "care",
            Self::Place => "place",
            Self::Buy => "buy",
            Self::Like => "like",
            Self::Daily => "daily",
        }
    }
}

/// The player's three quests for `day` (days since the Unix epoch).
pub fn for_day(user: u64, day: i64) -> [Quest; 3] {
    let mut state = user ^ (day as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15);
    let mut picked: Vec<usize> = Vec::with_capacity(3);
    while picked.len() < 3 {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        let i = (state % POOL.len() as u64) as usize;
        if !picked.contains(&i) {
            picked.push(i);
        }
    }
    [POOL[picked[0]], POOL[picked[1]], POOL[picked[2]]]
}

#[cfg(test)]
mod tests {
    use crate::quests::*;

    #[test]
    fn three_distinct_quests_that_change_daily() {
        let a = for_day(42, 20_000);
        assert_ne!(a[0].kind, a[1].kind);
        assert_ne!(a[1].kind, a[2].kind);
        assert_ne!(a[0].kind, a[2].kind);
        assert_eq!(a.map(|q| q.kind), for_day(42, 20_000).map(|q| q.kind));
        let changes = (0..10)
            .filter(|d| for_day(42, *d).map(|q| q.kind) != a.map(|q| q.kind))
            .count();
        assert!(changes > 5);
    }
}
