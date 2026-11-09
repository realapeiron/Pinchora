//! Runs against a real PostgreSQL when `PINCHORA_TEST_DATABASE_URL` is set; skipped otherwise.

use pinchora_core::admin::stats::PlayerStats;
use pinchora_db::{
    BefriendOutcome, Cosmetic, CustomizeBuy, DEFAULT_ROD, DEFAULT_SPOT, DailyOutcome, Db, GearBuy, GearRow, LogKind,
    MAX_BADGES, MAX_FRIENDS, MAX_PANEL_ROLES, MAX_REP_ROLES, MAX_XP_RULES, MilestoneReward, NewCatch, OutfitRow,
    PairOutcome, RepOutcome, Tally, TransferOutcome, TripClaim, TripPayout, Wake, XpGrant, XpTarget,
};

use crate::support::db;

/// Ids unique per run, so the suite can run repeatedly against one database.
use crate::support::ids;

/// The pet as stored now, as a care click reads it before it writes.
async fn read_pet(db: &Db, owner: u64, id: i64) -> pinchora_db::PetRow {
    db.pets(owner)
        .await
        .unwrap()
        .into_iter()
        .find(|p| p.id == id)
        .expect("the pet")
}

/// A day of January 2001, whose first day was a Monday: weeks far in the past, so that forgetting old
/// ones in this test cannot touch anything another test keeps.
fn week(day: u32) -> chrono::NaiveDate {
    chrono::NaiveDate::from_ymd_opt(2001, 1, day).unwrap()
}

mod cosmetics;
mod economy;
mod fishing;
mod guilds;
mod leveling;
mod pets;
mod social;
