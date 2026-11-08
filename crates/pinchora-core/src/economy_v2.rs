//! Fixed global reward budgets. Guilds can tune only their own points economy.
use chrono::{DateTime, Datelike, Utc};
use serde::{Deserialize, Serialize};

pub const MAX_PRICE: i64 = 10_000_000;
pub const SHIFTS_PER_DAY: i32 = 3;
pub const SHIFT_COOLDOWN_SECS: i64 = 3600;
pub const PROVISION_CAP: i64 = 120;
pub const PRESTIGE_COST: i64 = 10_000;
pub const CHEST_ODDS: [(i64, u32); 3] = [(25, 70), (50, 25), (100, 5)];
pub const JOBS: [&str; 3] = ["kelp_keeper", "coral_courier", "beacon_tender"];

pub fn rank(shifts: i32) -> i32 {
    (shifts / 10).clamp(0, 4)
}
pub fn wage(shifts: i32) -> i64 {
    100 + i64::from(rank(shifts)) * 25
}
pub fn season(now: DateTime<Utc>) -> i32 {
    now.year() * 100 + now.month() as i32
}
pub fn fee(price: i64) -> i64 {
    (price / 100 * 5 + (price % 100 * 5 + 99) / 100).max(1)
}
pub fn minimum_bid(current: i64) -> i64 {
    current + (current / 20).max(1)
}
pub fn chest_reward(roll: u32) -> i64 {
    if roll < 70 {
        25
    } else if roll < 95 {
        50
    } else {
        100
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct TradeItem {
    pub item_id: String,
    pub quantity: i32,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Offer {
    pub shells: i64,
    pub items: Vec<TradeItem>,
}
impl Offer {
    pub fn valid(&self) -> bool {
        (0..=MAX_PRICE).contains(&self.shells)
            && self.items.len() <= 8
            && self.items.iter().all(|i| {
                (1..=10).contains(&i.quantity)
                    && !i.item_id.is_empty()
                    && i.item_id.len() <= 100
                    && !crate::items::is_soulbound(&i.item_id)
            })
            && self
                .items
                .iter()
                .map(|i| &i.item_id)
                .collect::<std::collections::BTreeSet<_>>()
                .len()
                == self.items.len()
    }
    pub fn empty(&self) -> bool {
        self.shells == 0 && self.items.is_empty()
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct EconomySettings {
    pub pool_goal: i64,
    pub shop_min_level: i32,
    pub shop_min_days: i32,
    pub shop_enabled: bool,
}
impl Default for EconomySettings {
    fn default() -> Self {
        Self {
            pool_goal: 10_000,
            shop_min_level: 0,
            shop_min_days: 0,
            shop_enabled: true,
        }
    }
}
impl EconomySettings {
    pub fn valid(&self) -> bool {
        (100..=1_000_000_000).contains(&self.pool_goal)
            && (0..=100).contains(&self.shop_min_level)
            && (0..=365).contains(&self.shop_min_days)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn budgets_and_rounding() {
        assert_eq!(CHEST_ODDS.iter().map(|x| x.1).sum::<u32>(), 100);
        assert_eq!((0..100).map(chest_reward).sum::<i64>(), 3500);
        assert_eq!(wage(i32::MAX), 200);
        assert_eq!(fee(1), 1);
        assert_eq!(fee(101), 6);
        assert_eq!(minimum_bid(MAX_PRICE), 10_500_000);
        assert_eq!(i64::from(SHIFTS_PER_DAY) * wage(999), 600);
    }
    #[test]
    fn offers_reject_negative_duplicate_and_bound_items() {
        assert!(
            !Offer {
                shells: -1,
                items: vec![]
            }
            .valid()
        );
        let item = TradeItem {
            item_id: "chair".into(),
            quantity: 1,
        };
        assert!(
            !Offer {
                shells: 0,
                items: vec![item.clone(), item]
            }
            .valid()
        );
    }
}
