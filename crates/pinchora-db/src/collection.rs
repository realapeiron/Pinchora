//! The collection games: each server's chat-drop settings, the Tide Pool Dex, critter drops and
//! bottles, beachcombs and crab pots.
//!
//! Every payment happens in the same transaction as the thing that earns it (a card taken, a bottle
//! opened, a walk, a haul), so a double click, a race between pods or a restart can never pay twice.
//! The rules (what an entry pays, which entry turned up) belong to `pinchora-game`; this module only
//! applies them.

use chrono::{DateTime, Duration, Utc};
use pinchora_core::items::soulbound;
use sqlx::{Postgres, Transaction};

use crate::{Db, economy::ledger, house::add_inventory, sql_id};

type Tx<'a> = Transaction<'a, Postgres>;

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
    Have { slot: usize },
    /// Somebody else got there first.
    Lost { by: u64 },
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

macro_rules! drop_columns {
    () => {
        "id, kind, guild_id, channel_id, message_id, host_id, cards, takers, status, expires_at"
    };
}

impl Db {
    // /Settings

    pub async fn collection_settings(&self, guild: u64) -> anyhow::Result<CollectionSettings> {
        let row: Option<CollectionSettings> =
            sqlx::query_as("SELECT enabled, channel_id FROM collection_settings WHERE guild_id = $1")
                .bind(sql_id(guild))
                .fetch_optional(self.pool())
                .await?;
        Ok(row.unwrap_or(CollectionSettings {
            enabled: false,
            channel_id: None,
        }))
    }

    /// Changes a server's settings: `enabled` when given, and the channel when `channel` is `Some`
    /// (`Some(None)` clears it).
    pub async fn set_collection_settings(
        &self,
        guild: u64,
        enabled: Option<bool>,
        channel: Option<Option<u64>>,
    ) -> anyhow::Result<CollectionSettings> {
        Ok(sqlx::query_as(
            "INSERT INTO collection_settings (guild_id, enabled, channel_id)
             VALUES ($1, COALESCE($2, FALSE), CASE WHEN $4 THEN $3 END)
             ON CONFLICT (guild_id) DO UPDATE
             SET enabled = COALESCE($2, collection_settings.enabled),
                 channel_id = CASE WHEN $4 THEN $3 ELSE collection_settings.channel_id END
             RETURNING enabled, channel_id",
        )
        .bind(sql_id(guild))
        .bind(enabled)
        .bind(channel.flatten().map(sql_id))
        .bind(channel.is_some())
        .fetch_one(self.pool())
        .await?)
    }

    // /Dex

    /// Every entry the player has, in the order they first got them.
    pub async fn dex(&self, user: u64) -> anyhow::Result<Vec<DexRow>> {
        Ok(sqlx::query_as::<_, DexRow>(
            "SELECT entry_id, caught FROM dex_entries WHERE user_id = $1 ORDER BY first_at, entry_id",
        )
        .bind(sql_id(user))
        .fetch_all(self.pool())
        .await?)
    }

    pub async fn fragments(&self, user: u64) -> anyhow::Result<i64> {
        let row: Option<(i64,)> = sqlx::query_as("SELECT fragments FROM collection_players WHERE user_id = $1")
            .bind(sql_id(user))
            .fetch_optional(self.pool())
            .await?;
        Ok(row.map_or(0, |(n,)| n))
    }

    /// Spends `cost` fragments on the decor piece `item` (an item id) and puts it in the inventory as a
    /// soulbound piece: it can be placed in their own house, but never sold, gifted or donated.
    /// Returns the fragments left, or `None` when the player cannot cover it.
    pub async fn exchange_fragments(&self, user: u64, cost: i64, item: &str) -> anyhow::Result<Option<i64>> {
        let id = sql_id(user);
        let mut tx = self.pool().begin().await?;
        let left: Option<(i64,)> = sqlx::query_as(
            "UPDATE collection_players SET fragments = fragments - $2
             WHERE user_id = $1 AND fragments >= $2 RETURNING fragments",
        )
        .bind(id)
        .bind(cost)
        .fetch_optional(&mut *tx)
        .await?;
        let Some((left,)) = left else {
            return Ok(None);
        };
        add_inventory(&mut tx, id, &soulbound(item), 1).await?;
        tx.commit().await?;
        Ok(Some(left))
    }

    // /Drops and bottles

    pub async fn create_drop(&self, new: NewDrop<'_>) -> anyhow::Result<DropRow> {
        Ok(sqlx::query_as::<_, DropRow>(concat!(
            "INSERT INTO collection_drops (kind, guild_id, channel_id, host_id, cards, takers, expires_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7) RETURNING ",
            drop_columns!()
        ))
        .bind(new.kind)
        .bind(sql_id(new.guild_id))
        .bind(sql_id(new.channel_id))
        .bind(new.host.map(sql_id))
        .bind(new.cards.to_vec())
        .bind(vec![0_i64; new.cards.len()])
        .bind(new.expires_at)
        .fetch_one(self.pool())
        .await?)
    }

    /// Remembers which message shows the drop, so it can be edited when it ends.
    pub async fn set_drop_message(&self, id: i64, channel: u64, message: u64) -> anyhow::Result<()> {
        sqlx::query("UPDATE collection_drops SET channel_id = $2, message_id = $3 WHERE id = $1")
            .bind(id)
            .bind(sql_id(channel))
            .bind(sql_id(message))
            .execute(self.pool())
            .await?;
        Ok(())
    }

    /// Takes a drop back that never made it to its channel.
    pub async fn abort_drop(&self, id: i64) -> anyhow::Result<()> {
        sqlx::query("DELETE FROM collection_drops WHERE id = $1 AND message_id IS NULL")
            .bind(id)
            .execute(self.pool())
            .await?;
        Ok(())
    }

    pub async fn drop_row(&self, id: i64) -> anyhow::Result<Option<DropRow>> {
        Ok(sqlx::query_as::<_, DropRow>(concat!(
            "SELECT ",
            drop_columns!(),
            " FROM collection_drops WHERE id = $1"
        ))
        .bind(id)
        .fetch_optional(self.pool())
        .await?)
    }

    /// Whether something still floats or lies in the channel, waiting for somebody.
    pub async fn drop_waiting_in(&self, channel: u64, now: DateTime<Utc>) -> anyhow::Result<bool> {
        let (waiting,): (bool,) = sqlx::query_as(
            "SELECT EXISTS (SELECT 1 FROM collection_drops
                            WHERE channel_id = $1 AND status = 'open' AND expires_at > $2)",
        )
        .bind(sql_id(channel))
        .bind(now)
        .fetch_one(self.pool())
        .await?;
        Ok(waiting)
    }

    /// `user` takes card `slot` of the critter drop. `award` is what the card's critter pays; the drop's
    /// card in that slot must be `award.entry`. Takes the card, files it in the dex and pays, all
    /// or nothing.
    pub async fn grab_card(
        &self,
        drop_id: i64,
        slot: usize,
        user: u64,
        now: DateTime<Utc>,
        award: Award<'_>,
        daily: Daily<'_>,
    ) -> anyhow::Result<Grab> {
        let mut tx = self.pool().begin().await?;
        let drop: Option<DropRow> = sqlx::query_as(concat!(
            "SELECT ",
            drop_columns!(),
            " FROM collection_drops WHERE id = $1 FOR UPDATE"
        ))
        .bind(drop_id)
        .fetch_optional(&mut *tx)
        .await?;
        let Some(mut drop) = drop.filter(|d| d.kind == CRITTERS && d.is_open() && d.expires_at > now) else {
            return Ok(Grab::Gone);
        };
        if drop.cards.get(slot).map(String::as_str) != Some(award.entry) {
            return Ok(Grab::Gone);
        }
        if let Some(have) = drop.takers.iter().position(|t| *t == sql_id(user)) {
            return Ok(Grab::Have { slot: have });
        }
        if drop.takers[slot] != 0 {
            return Ok(Grab::Lost {
                by: drop.takers[slot] as u64,
            });
        }
        drop.takers[slot] = sql_id(user);
        sqlx::query("UPDATE collection_drops SET takers = $2 WHERE id = $1")
            .bind(drop_id)
            .bind(&drop.takers)
            .execute(&mut *tx)
            .await?;
        let paid = paid_play(&mut tx, user, daily).await?;
        let granted = grant(&mut tx, user, award, paid, "critter_drop").await?;
        tx.commit().await?;
        Ok(Grab::Taken {
            granted,
            drop: Box::new(drop),
            paid,
        })
    }

    /// `user` opens the bottle and finds `prize`. The prize pays in the same transaction that marks the
    /// bottle opened, so two openers never both get it.
    pub async fn open_bottle(
        &self,
        drop_id: i64,
        user: u64,
        now: DateTime<Utc>,
        prize: BottlePrize<'_>,
        daily: Daily<'_>,
    ) -> anyhow::Result<BottleOpen> {
        let mut tx = self.pool().begin().await?;
        let drop: Option<DropRow> = sqlx::query_as(concat!(
            "SELECT ",
            drop_columns!(),
            " FROM collection_drops WHERE id = $1 FOR UPDATE"
        ))
        .bind(drop_id)
        .fetch_optional(&mut *tx)
        .await?;
        let Some(mut drop) = drop.filter(|d| d.kind == BOTTLE) else {
            return Ok(BottleOpen::Gone);
        };
        // Somebody opened it first: opening closes the bottle, so this must be told before it is called over.
        if drop.takers.first().is_some_and(|t| *t != 0) {
            return Ok(BottleOpen::Lost {
                by: drop.takers[0] as u64,
            });
        }
        if !drop.is_open() || drop.expires_at <= now {
            return Ok(BottleOpen::Gone);
        }
        drop.takers[0] = sql_id(user);
        sqlx::query("UPDATE collection_drops SET takers = $2, status = 'closed' WHERE id = $1")
            .bind(drop_id)
            .bind(&drop.takers)
            .execute(&mut *tx)
            .await?;
        drop.status = "closed".to_owned();
        let paid = paid_play(&mut tx, user, daily).await?;
        let (mut shells, mut fragments, mut granted) = (0, 0, None);
        match prize {
            BottlePrize::Shells(n) | BottlePrize::Treasure(n) if paid => shells = n,
            BottlePrize::Fragments(n) if paid => fragments = n,
            BottlePrize::Find(award) => granted = Some(grant(&mut tx, user, award, paid, "bottle").await?),
            _ => {}
        }
        if shells > 0 {
            earn(&mut tx, user, shells, "bottle").await?;
        }
        if fragments > 0 {
            add_fragments(&mut tx, user, fragments).await?;
        }
        tx.commit().await?;
        Ok(BottleOpen::Opened {
            drop: Box::new(drop),
            shells,
            fragments,
            granted,
            paid,
        })
    }

    /// Closes the drops whose time is up (once each: whoever gets the row back tells the channel).
    pub async fn close_due_drops(&self, now: DateTime<Utc>, limit: i64) -> anyhow::Result<Vec<DropRow>> {
        Ok(sqlx::query_as::<_, DropRow>(concat!(
            "UPDATE collection_drops SET status = 'closed'
             WHERE id IN (SELECT id FROM collection_drops WHERE status = 'open' AND expires_at <= $1
                          ORDER BY expires_at LIMIT $2 FOR UPDATE SKIP LOCKED)
             RETURNING ",
            drop_columns!()
        ))
        .bind(now)
        .bind(limit)
        .fetch_all(self.pool())
        .await?)
    }

    /// Forgets drops that closed before `before`, and play counts of days before `day`.
    pub async fn forget_old_collection(&self, before: DateTime<Utc>, day: i32) -> anyhow::Result<u64> {
        let drops = sqlx::query("DELETE FROM collection_drops WHERE status = 'closed' AND expires_at < $1")
            .bind(before)
            .execute(self.pool())
            .await?
            .rows_affected();
        let days = sqlx::query("DELETE FROM collection_daily WHERE day < $1")
            .bind(day)
            .execute(self.pool())
            .await?
            .rows_affected();
        Ok(drops + days)
    }

    // /Beachcomb

    /// A walk along the shore: unless the player walked less than `wait` ago, files `award`'s find
    /// in their dex, pays it and starts the wait.
    pub async fn beachcomb(
        &self,
        user: u64,
        now: DateTime<Utc>,
        wait: Duration,
        award: Award<'_>,
        daily: Daily<'_>,
    ) -> anyhow::Result<Comb> {
        let mut tx = self.pool().begin().await?;
        let ready: Option<DateTime<Utc>> = lock_player(&mut tx, user)
            .await?
            .and_then(|row: (Option<DateTime<Utc>>,)| row.0);
        if let Some(ready) = ready.filter(|r| *r > now) {
            return Ok(Comb::Wait { ready_at: ready });
        }
        sqlx::query("UPDATE collection_players SET comb_ready_at = $2 WHERE user_id = $1")
            .bind(sql_id(user))
            .bind(now + wait)
            .execute(&mut *tx)
            .await?;
        let paid = paid_play(&mut tx, user, daily).await?;
        let granted = grant(&mut tx, user, award, paid, "beachcomb").await?;
        tx.commit().await?;
        Ok(Comb::Found { granted, paid })
    }

    // /Crab pot

    pub async fn pot(&self, user: u64) -> anyhow::Result<PotRow> {
        let row: Option<(i16, Option<i16>, Option<DateTime<Utc>>)> =
            sqlx::query_as("SELECT pot_level, pot_hours, pot_ends_at FROM collection_players WHERE user_id = $1")
                .bind(sql_id(user))
                .fetch_optional(self.pool())
                .await?;
        Ok(row.map_or(
            PotRow {
                level: 1,
                hours: None,
                ends_at: None,
            },
            |(level, hours, ends_at)| PotRow { level, hours, ends_at },
        ))
    }

    /// Sets the pot for `hours`. False when it already has a soak running or waiting to be hauled.
    pub async fn start_pot(&self, user: u64, hours: i16, now: DateTime<Utc>) -> anyhow::Result<bool> {
        sqlx::query("INSERT INTO collection_players (user_id) VALUES ($1) ON CONFLICT (user_id) DO NOTHING")
            .bind(sql_id(user))
            .execute(self.pool())
            .await?;
        let set = sqlx::query(
            "UPDATE collection_players SET pot_hours = $2, pot_ends_at = $3
             WHERE user_id = $1 AND pot_ends_at IS NULL",
        )
        .bind(sql_id(user))
        .bind(hours)
        .bind(now + Duration::hours(i64::from(hours)))
        .execute(self.pool())
        .await?
        .rows_affected();
        Ok(set == 1)
    }

    /// Hauls the pot in once its soak is done: `catches` works out what it brought from the pot's level, the
    /// soak's hours and the time the soak ended (the pot is locked meanwhile), and the haul is filed, paid and
    /// cleared in one transaction, so a second haul finds [`PotHaul::Empty`].
    pub async fn haul_pot<'a, F>(
        &self,
        user: u64,
        now: DateTime<Utc>,
        daily: Daily<'_>,
        catches: F,
    ) -> anyhow::Result<PotHaul>
    where
        F: FnOnce(i16, i16, DateTime<Utc>) -> Vec<Award<'a>>,
    {
        let mut tx = self.pool().begin().await?;
        let row: Option<(i16, Option<i16>, Option<DateTime<Utc>>)> = sqlx::query_as(
            "SELECT pot_level, pot_hours, pot_ends_at FROM collection_players WHERE user_id = $1 FOR UPDATE",
        )
        .bind(sql_id(user))
        .fetch_optional(&mut *tx)
        .await?;
        let Some((level, Some(hours), Some(ends_at))) = row else {
            return Ok(PotHaul::Empty);
        };
        if ends_at > now {
            return Ok(PotHaul::Soaking { ends_at });
        }
        sqlx::query("UPDATE collection_players SET pot_hours = NULL, pot_ends_at = NULL WHERE user_id = $1")
            .bind(sql_id(user))
            .execute(&mut *tx)
            .await?;
        let paid = paid_play(&mut tx, user, daily).await?;
        let mut granted = Vec::new();
        for award in catches(level, hours, ends_at) {
            granted.push(grant(&mut tx, user, award, paid, "crab_pot").await?);
        }
        tx.commit().await?;
        Ok(PotHaul::Hauled {
            granted,
            paid,
            level,
            hours,
        })
    }

    /// Takes the pot from level `from` to the next for `cost` shells.
    pub async fn upgrade_pot(&self, user: u64, from: i16, cost: i64) -> anyhow::Result<PotUpgrade> {
        let id = sql_id(user);
        let mut tx = self.pool().begin().await?;
        sqlx::query("INSERT INTO collection_players (user_id) VALUES ($1) ON CONFLICT (user_id) DO NOTHING")
            .bind(id)
            .execute(&mut *tx)
            .await?;
        let (level,): (i16,) = sqlx::query_as("SELECT pot_level FROM collection_players WHERE user_id = $1 FOR UPDATE")
            .bind(id)
            .fetch_one(&mut *tx)
            .await?;
        if level != from || level >= 5 {
            return Ok(PotUpgrade::Stale);
        }
        let paid: Option<(i64,)> =
            sqlx::query_as("UPDATE users SET credits = credits - $2 WHERE id = $1 AND credits >= $2 RETURNING credits")
                .bind(id)
                .bind(cost)
                .fetch_optional(&mut *tx)
                .await?;
        let Some((balance,)) = paid else {
            let (balance,): (i64,) = sqlx::query_as("SELECT COALESCE((SELECT credits FROM users WHERE id = $1), 0)")
                .bind(id)
                .fetch_one(&mut *tx)
                .await?;
            return Ok(PotUpgrade::Poor { balance });
        };
        ledger(&mut tx, id, -cost, "crab_pot_upgrade").await?;
        sqlx::query("UPDATE collection_players SET pot_level = pot_level + 1 WHERE user_id = $1")
            .bind(id)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(PotUpgrade::Upgraded {
            level: level + 1,
            balance,
        })
    }
}

/// Makes sure the player has a row and locks it; the first column is `comb_ready_at`.
async fn lock_player(tx: &mut Tx<'_>, user: u64) -> anyhow::Result<Option<(Option<DateTime<Utc>>,)>> {
    sqlx::query("INSERT INTO collection_players (user_id) VALUES ($1) ON CONFLICT (user_id) DO NOTHING")
        .bind(sql_id(user))
        .execute(&mut **tx)
        .await?;
    Ok(
        sqlx::query_as("SELECT comb_ready_at FROM collection_players WHERE user_id = $1 FOR UPDATE")
            .bind(sql_id(user))
            .fetch_optional(&mut **tx)
            .await?,
    )
}

/// Counts a play of a game today and says whether it still pays.
async fn paid_play(tx: &mut Tx<'_>, user: u64, daily: Daily<'_>) -> anyhow::Result<bool> {
    let (plays,): (i32,) = sqlx::query_as(
        "INSERT INTO collection_daily (user_id, day, game, plays) VALUES ($1, $2, $3, 1)
         ON CONFLICT (user_id, day, game) DO UPDATE SET plays = collection_daily.plays + 1
         RETURNING plays",
    )
    .bind(sql_id(user))
    .bind(daily.day)
    .bind(daily.game)
    .fetch_one(&mut **tx)
    .await?;
    Ok(plays <= daily.cap)
}

/// Files `award.entry` in the dex and, when `paid`, pays what it is worth: the discovery bonus the first
/// time, the recycled spare after that.
async fn grant(tx: &mut Tx<'_>, user: u64, award: Award<'_>, paid: bool, reason: &str) -> anyhow::Result<Granted> {
    let (caught,): (i32,) = sqlx::query_as(
        "INSERT INTO dex_entries (user_id, entry_id) VALUES ($1, $2)
         ON CONFLICT (user_id, entry_id) DO UPDATE SET caught = dex_entries.caught + 1
         RETURNING caught",
    )
    .bind(sql_id(user))
    .bind(award.entry)
    .fetch_one(&mut **tx)
    .await?;
    let new = caught == 1;
    let (shells, fragments) = match (paid, new) {
        (false, _) => (0, 0),
        (true, true) => (award.new_shells, award.bonus_fragments),
        (true, false) => (award.dup_shells, award.dup_fragments + award.bonus_fragments),
    };
    if shells > 0 {
        earn(tx, user, shells, reason).await?;
    }
    if fragments > 0 {
        add_fragments(tx, user, fragments).await?;
    }
    Ok(Granted {
        entry: award.entry.to_owned(),
        new,
        caught,
        shells,
        fragments,
    })
}

async fn earn(tx: &mut Tx<'_>, user: u64, shells: i64, reason: &str) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO users (id, credits) VALUES ($1, $2)
         ON CONFLICT (id) DO UPDATE SET credits = users.credits + EXCLUDED.credits",
    )
    .bind(sql_id(user))
    .bind(shells)
    .execute(&mut **tx)
    .await?;
    ledger(tx, sql_id(user), shells, reason).await
}

async fn add_fragments(tx: &mut Tx<'_>, user: u64, fragments: i64) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO collection_players (user_id, fragments) VALUES ($1, $2)
         ON CONFLICT (user_id) DO UPDATE SET fragments = collection_players.fragments + EXCLUDED.fragments",
    )
    .bind(sql_id(user))
    .bind(fragments)
    .execute(&mut **tx)
    .await?;
    Ok(())
}
