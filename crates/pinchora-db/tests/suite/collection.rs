//! Runs against a real PostgreSQL when `PINCHORA_TEST_DATABASE_URL` is set; skipped otherwise.

use chrono::{Duration, Utc};
use pinchora_db::{
    Award, BottleOpen, BottlePrize, Comb, DROP_BOTTLE, DROP_CRITTERS, Daily, Db, Grab, NewDrop, PotHaul, PotUpgrade,
    RollClaim, RollStart, VoteCard,
};

use crate::support::db;

/// Ids unique per run, so the suite can run repeatedly against one database.
use crate::support::ids;

fn award(entry: &str) -> Award<'_> {
    Award {
        entry,
        new_shells: 15,
        dup_shells: 6,
        dup_fragments: 1,
        bonus_fragments: 0,
    }
}

fn daily(game: &'static str, cap: i32) -> Daily<'static> {
    Daily { game, day: 20_000, cap }
}

use crate::support::shells;

/// What the ledger says the given players hold in total, which must match their balances.
use crate::support::ledger_total;

async fn critter_drop(db: &Db, guild: u64, cards: &[&str], lasts: Duration) -> pinchora_db::DropRow {
    let cards: Vec<String> = cards.iter().map(|c| (*c).to_owned()).collect();
    db.create_drop(NewDrop {
        kind: DROP_CRITTERS,
        guild_id: guild,
        channel_id: guild + 1,
        host: None,
        cards: &cards,
        expires_at: Utc::now() + lasts,
    })
    .await
    .unwrap()
}

async fn bottle(db: &Db, guild: u64, lasts: Duration) -> pinchora_db::DropRow {
    db.create_drop(NewDrop {
        kind: DROP_BOTTLE,
        guild_id: guild,
        channel_id: guild + 1,
        host: None,
        cards: &["bottle_01".to_owned()],
        expires_at: Utc::now() + lasts,
    })
    .await
    .unwrap()
}

/// A starter room for `user`, so there is a house to place pieces in.
async fn house_of(db: &Db, user: u64) {
    let starter = pinchora_db::NewRoom {
        room_type: "living".into(),
        wallpaper: "w".into(),
        floor: "f".into(),
    };
    db.house(user, &starter).await.unwrap();
}

async fn pieces(db: &Db, user: u64, item: &str) -> i32 {
    db.inventory(user)
        .await
        .unwrap()
        .into_iter()
        .find(|r| r.item_id == item)
        .map_or(0, |r| r.quantity)
}

mod depth;
mod drops;
mod inventory;
