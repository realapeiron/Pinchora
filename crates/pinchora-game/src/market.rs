//! The hourly market: a deterministic draw per UTC hour, so every pod shows the same shop.

use crate::{Availability, Content, Currency, Rarity};

pub const OFFERS_PER_HOUR: usize = 8;

pub fn stock_for(rarity: Rarity) -> i32 {
    match rarity {
        Rarity::Common => 300,
        Rarity::Uncommon => 120,
        Rarity::Rare => 40,
        Rarity::Epic => 12,
        Rarity::Legendary => 3,
    }
}

fn weight_for(rarity: Rarity) -> u64 {
    match rarity {
        Rarity::Common => 50,
        Rarity::Uncommon => 28,
        Rarity::Rare => 14,
        Rarity::Epic => 6,
        Rarity::Legendary => 2,
    }
}

/// Hour number since the Unix epoch.
pub fn hour_of(unix_secs: i64) -> i64 {
    unix_secs.div_euclid(3600)
}

fn splitmix(mut x: u64) -> u64 {
    x = x.wrapping_add(0x9e37_79b9_7f4a_7c15);
    x = (x ^ (x >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    x ^ (x >> 31)
}

/// `(item_id, price, stock)` for the given hour. Draws distinct base designs from the
/// rotating shell-priced pool, weighted by rarity.
pub fn offers(content: &Content, hour: i64) -> Vec<(String, i64, i32)> {
    let pool: Vec<_> = content
        .items()
        .filter(|i| i.base.availability == Availability::Rotating && i.base.currency == Currency::Shells)
        .collect();
    let mut state = splitmix(hour as u64 ^ 0x5049_4e43_484f_5241);
    let mut out: Vec<(String, i64, i32)> = Vec::new();
    let mut used_bases: Vec<&str> = Vec::new();
    let total: u64 = pool.iter().map(|i| weight_for(i.base.rarity)).sum();
    if total == 0 {
        return out;
    }
    let mut attempts = 0;
    while out.len() < OFFERS_PER_HOUR && attempts < 500 {
        attempts += 1;
        state = splitmix(state);
        let mut roll = state % total;
        let pick = pool.iter().find(|i| {
            let w = weight_for(i.base.rarity);
            if roll < w {
                true
            } else {
                roll -= w;
                false
            }
        });
        let Some(item) = pick else { continue };
        if used_bases.contains(&item.base.id.as_str()) {
            continue;
        }
        used_bases.push(&item.base.id);
        out.push((item.id(), item.base.price, stock_for(item.base.rarity)));
    }
    out
}
