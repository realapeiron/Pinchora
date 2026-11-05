//! Fishing: the gear on the line, the fish bucket, book milestones and the weekly catch board.

use chrono::NaiveDate;

use crate::{
    Db,
    economy::ledger,
    shop::{Currency, balance_of, charge},
    sql_id,
};

/// The spot a player fishes before they pick another, as the table's default.
pub const DEFAULT_SPOT: &str = "shore";
/// The rod everybody owns, as the table's default.
pub const DEFAULT_ROD: &str = "driftwood_rod";

// /Gear

/// What a player has on the line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GearRow {
    /// The last spot they fished.
    pub spot: String,
    pub rod: String,
    /// `None` is plain bait.
    pub bait: Option<String>,
}

impl Default for GearRow {
    fn default() -> Self {
        Self {
            spot: DEFAULT_SPOT.to_owned(),
            rod: DEFAULT_ROD.to_owned(),
            bait: None,
        }
    }
}

/// How buying a rod or a pack of bait went.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GearBuy {
    /// `owned` is how many pieces of bait they have now (1 for a rod).
    Bought {
        balance: i64,
        owned: i32,
    },
    AlreadyOwned,
    Insufficient {
        balance: i64,
    },
}

// /Catches

/// A fish waiting in the bucket.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct BucketRow {
    pub id: i64,
    pub fish_id: String,
    pub size_cm: f32,
}

/// What a sale paid.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Sold {
    pub fish: i64,
    pub shells: i64,
    pub balance: i64,
}

/// A cosmetic a milestone gives, by its id in `badges.json` or `backgrounds.json`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Cosmetic {
    Badge(String),
    Background(String),
}

/// A fish-book milestone to pay.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MilestoneReward {
    /// The book size it is for.
    pub species: i32,
    pub shells: i64,
    pub cosmetic: Cosmetic,
}

/// A milestone that paid out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MilestonePaid {
    pub reward: MilestoneReward,
    /// False when they already owned the cosmetic, bought earlier.
    pub cosmetic_new: bool,
}

/// A catch to put in the book and the bucket.
#[derive(Debug, Clone, Copy)]
pub struct NewCatch<'a> {
    pub fish_id: &'a str,
    pub size_cm: f32,
    /// How big it is for its species: 1.0 is a usual size.
    pub ratio: f64,
    /// The server and the week (its Monday) to count the catch in for the weekly board.
    pub weekly: Option<(u64, NaiveDate)>,
    /// Every milestone there is; the ones the book has reached and not been paid are paid now.
    pub milestones: &'a [MilestoneReward],
}

/// What putting a catch away changed.
#[derive(Debug, Clone, PartialEq)]
pub struct CatchLog {
    /// The first of its species in their book.
    pub first: bool,
    /// The record size before this catch; 0 for a first catch.
    pub previous_best: f32,
    /// A bigger one than they ever caught of it (never true for a first catch).
    pub record: bool,
    /// Species in the book now.
    pub species: i64,
    /// Fish in the bucket now.
    pub bucket: i64,
    /// This is their best catch of the week in that server.
    pub week_best: bool,
    pub paid: Vec<MilestonePaid>,
}

/// One place on the weekly catch board.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct CatchBoardRow {
    pub user_id: i64,
    pub fish_id: String,
    pub size_cm: f32,
    /// Size over the species' usual size.
    pub ratio: f64,
}

impl Db {
    /// What the player has on the line; the shore, the starter rod and plain bait until they change it.
    pub async fn fishing_gear(&self, user: u64) -> anyhow::Result<GearRow> {
        let row: Option<(String, String, Option<String>)> =
            sqlx::query_as("SELECT spot, rod, bait FROM fishing_gear WHERE user_id = $1")
                .bind(sql_id(user))
                .fetch_optional(self.pool())
                .await?;
        Ok(row.map_or_else(GearRow::default, |(spot, rod, bait)| GearRow { spot, rod, bait }))
    }

    /// Remembers the spot as the one they fish next.
    pub async fn set_fishing_spot(&self, user: u64, spot: &str) -> anyhow::Result<()> {
        sqlx::query(
            "INSERT INTO fishing_gear (user_id, spot) VALUES ($1, $2)
             ON CONFLICT (user_id) DO UPDATE SET spot = EXCLUDED.spot",
        )
        .bind(sql_id(user))
        .bind(spot)
        .execute(self.pool())
        .await?;
        Ok(())
    }

    /// Rods they bought, oldest first. The starter rod is not among them.
    pub async fn owned_rods(&self, user: u64) -> anyhow::Result<Vec<String>> {
        let rows: Vec<(String,)> =
            sqlx::query_as("SELECT rod_id FROM user_rods WHERE user_id = $1 ORDER BY bought_at, rod_id")
                .bind(sql_id(user))
                .fetch_all(self.pool())
                .await?;
        Ok(rows.into_iter().map(|(id,)| id).collect())
    }

    /// Bait pieces they still have, by kind; kinds that ran out are left out.
    pub async fn bait_stock(&self, user: u64) -> anyhow::Result<Vec<(String, i32)>> {
        Ok(
            sqlx::query_as("SELECT bait_id, qty FROM user_baits WHERE user_id = $1 AND qty > 0 ORDER BY bait_id")
                .bind(sql_id(user))
                .fetch_all(self.pool())
                .await?,
        )
    }

    /// Buys a rod at `price`: the shells, the ledger entry and the rod happen together or not at all.
    pub async fn buy_rod(&self, user: u64, rod: &str, price: i64) -> anyhow::Result<GearBuy> {
        anyhow::ensure!(price > 0, "a purchase needs a price");
        let id = sql_id(user);
        let mut tx = self.pool().begin().await?;
        sqlx::query("INSERT INTO users (id) VALUES ($1) ON CONFLICT (id) DO NOTHING")
            .bind(id)
            .execute(&mut *tx)
            .await?;
        // The ownership row goes first: of two clicks at once, the second waits here and finds it taken.
        let fresh = sqlx::query("INSERT INTO user_rods (user_id, rod_id) VALUES ($1, $2) ON CONFLICT DO NOTHING")
            .bind(id)
            .bind(rod)
            .execute(&mut *tx)
            .await?
            .rows_affected()
            == 1;
        if !fresh {
            return Ok(GearBuy::AlreadyOwned);
        }
        let Some(balance) = charge(&mut tx, id, price, Currency::Shells).await? else {
            // Dropping the transaction takes the rod back.
            let balance = balance_of(&mut tx, id, Currency::Shells).await?;
            return Ok(GearBuy::Insufficient { balance });
        };
        ledger(&mut tx, id, -price, "fishing_rod").await?;
        tx.commit().await?;
        Ok(GearBuy::Bought { balance, owned: 1 })
    }

    /// Buys a pack of `pack` bait for `price`, added to what they have.
    pub async fn buy_bait(&self, user: u64, bait: &str, pack: i32, price: i64) -> anyhow::Result<GearBuy> {
        anyhow::ensure!(price > 0 && pack > 0, "a purchase needs a price and a pack");
        let id = sql_id(user);
        let mut tx = self.pool().begin().await?;
        sqlx::query("INSERT INTO users (id) VALUES ($1) ON CONFLICT (id) DO NOTHING")
            .bind(id)
            .execute(&mut *tx)
            .await?;
        let Some(balance) = charge(&mut tx, id, price, Currency::Shells).await? else {
            let balance = balance_of(&mut tx, id, Currency::Shells).await?;
            return Ok(GearBuy::Insufficient { balance });
        };
        let (owned,): (i32,) = sqlx::query_as(
            "INSERT INTO user_baits (user_id, bait_id, qty) VALUES ($1, $2, $3)
             ON CONFLICT (user_id, bait_id) DO UPDATE SET qty = user_baits.qty + EXCLUDED.qty RETURNING qty",
        )
        .bind(id)
        .bind(bait)
        .bind(pack)
        .fetch_one(&mut *tx)
        .await?;
        ledger(&mut tx, id, -price, "fishing_bait").await?;
        tx.commit().await?;
        Ok(GearBuy::Bought { balance, owned })
    }

    /// Puts a rod on the line. False when it is not theirs; the starter rod (`starter`) always is.
    pub async fn equip_rod(&self, user: u64, rod: &str, starter: bool) -> anyhow::Result<bool> {
        let set = sqlx::query(
            "INSERT INTO fishing_gear (user_id, rod)
             SELECT $1, $2 WHERE $3 OR EXISTS (SELECT 1 FROM user_rods WHERE user_id = $1 AND rod_id = $2)
             ON CONFLICT (user_id) DO UPDATE SET rod = EXCLUDED.rod",
        )
        .bind(sql_id(user))
        .bind(rod)
        .bind(starter)
        .execute(self.pool())
        .await?
        .rows_affected();
        Ok(set == 1)
    }

    /// Puts a bait on the hook, or plain bait for `None`. False when they have none of it.
    pub async fn equip_bait(&self, user: u64, bait: Option<&str>) -> anyhow::Result<bool> {
        let set = sqlx::query(
            "INSERT INTO fishing_gear (user_id, bait)
             SELECT $1, $2 WHERE $2::TEXT IS NULL
                 OR EXISTS (SELECT 1 FROM user_baits WHERE user_id = $1 AND bait_id = $2 AND qty > 0)
             ON CONFLICT (user_id) DO UPDATE SET bait = EXCLUDED.bait",
        )
        .bind(sql_id(user))
        .bind(bait)
        .execute(self.pool())
        .await?
        .rows_affected();
        Ok(set == 1)
    }

    /// Uses up one piece of `bait` for a cast and returns how many are left; `None` when they have
    /// none. The last piece also takes the bait off the hook.
    pub async fn take_bait(&self, user: u64, bait: &str) -> anyhow::Result<Option<i32>> {
        let id = sql_id(user);
        let mut tx = self.pool().begin().await?;
        let left: Option<(i32,)> = sqlx::query_as(
            "UPDATE user_baits SET qty = qty - 1 WHERE user_id = $1 AND bait_id = $2 AND qty >= 1 RETURNING qty",
        )
        .bind(id)
        .bind(bait)
        .fetch_optional(&mut *tx)
        .await?;
        let Some((left,)) = left else {
            return Ok(None);
        };
        if left == 0 {
            sqlx::query("UPDATE fishing_gear SET bait = NULL WHERE user_id = $1 AND bait = $2")
                .bind(id)
                .bind(bait)
                .execute(&mut *tx)
                .await?;
        }
        tx.commit().await?;
        Ok(Some(left))
    }

    // /Catches

    /// Puts a catch in the fish book (counting it, and the record size if it beats the last), the
    /// bucket and the weekly board, and pays the book milestones it reached, all together.
    pub async fn record_catch(&self, user: u64, catch: NewCatch<'_>) -> anyhow::Result<CatchLog> {
        let id = sql_id(user);
        let mut tx = self.pool().begin().await?;
        sqlx::query("INSERT INTO users (id) VALUES ($1) ON CONFLICT (id) DO NOTHING")
            .bind(id)
            .execute(&mut *tx)
            .await?;
        let before: Option<(f32,)> =
            sqlx::query_as("SELECT best_cm FROM fish_book WHERE user_id = $1 AND fish_id = $2 FOR UPDATE")
                .bind(id)
                .bind(catch.fish_id)
                .fetch_optional(&mut *tx)
                .await?;
        sqlx::query(
            "INSERT INTO fish_book (user_id, fish_id, caught, best_cm) VALUES ($1, $2, 1, $3)
             ON CONFLICT (user_id, fish_id) DO UPDATE
             SET caught = fish_book.caught + 1, best_cm = GREATEST(fish_book.best_cm, EXCLUDED.best_cm)",
        )
        .bind(id)
        .bind(catch.fish_id)
        .bind(catch.size_cm)
        .execute(&mut *tx)
        .await?;
        let first = before.is_none();
        let previous_best = before.map_or(0.0, |(best,)| best);
        sqlx::query("INSERT INTO fish_bucket (user_id, fish_id, size_cm) VALUES ($1, $2, $3)")
            .bind(id)
            .bind(catch.fish_id)
            .bind(catch.size_cm)
            .execute(&mut *tx)
            .await?;
        let week_best = match catch.weekly {
            Some((guild, week)) => {
                sqlx::query(
                    "INSERT INTO weekly_catch (guild_id, week, user_id, fish_id, size_cm, ratio)
                     VALUES ($1, $2, $3, $4, $5, $6)
                     ON CONFLICT (guild_id, week, user_id) DO UPDATE
                     SET fish_id = EXCLUDED.fish_id, size_cm = EXCLUDED.size_cm, ratio = EXCLUDED.ratio,
                         caught_at = now()
                     WHERE EXCLUDED.ratio > weekly_catch.ratio",
                )
                .bind(sql_id(guild))
                .bind(week)
                .bind(id)
                .bind(catch.fish_id)
                .bind(catch.size_cm)
                .bind(catch.ratio)
                .execute(&mut *tx)
                .await?
                .rows_affected()
                    == 1
            }
            None => false,
        };
        let (species,): (i64,) = sqlx::query_as("SELECT count(*) FROM fish_book WHERE user_id = $1")
            .bind(id)
            .fetch_one(&mut *tx)
            .await?;
        let mut paid = Vec::new();
        for reward in catch.milestones.iter().filter(|m| i64::from(m.species) <= species) {
            // The claim row goes first: of two catches at once, the second finds it taken.
            let claimed =
                sqlx::query("INSERT INTO fish_milestones (user_id, species) VALUES ($1, $2) ON CONFLICT DO NOTHING")
                    .bind(id)
                    .bind(reward.species)
                    .execute(&mut *tx)
                    .await?
                    .rows_affected()
                    == 1;
            if !claimed {
                continue;
            }
            sqlx::query("UPDATE users SET credits = credits + $2 WHERE id = $1")
                .bind(id)
                .bind(reward.shells)
                .execute(&mut *tx)
                .await?;
            ledger(&mut tx, id, reward.shells, "fish_milestone").await?;
            let (insert, cosmetic) = match &reward.cosmetic {
                Cosmetic::Badge(badge) => (
                    "INSERT INTO user_badges (user_id, badge_id) VALUES ($1, $2) ON CONFLICT DO NOTHING",
                    badge,
                ),
                Cosmetic::Background(background) => (
                    "INSERT INTO user_backgrounds (user_id, background_id) VALUES ($1, $2) ON CONFLICT DO NOTHING",
                    background,
                ),
            };
            let cosmetic_new = sqlx::query(insert)
                .bind(id)
                .bind(cosmetic)
                .execute(&mut *tx)
                .await?
                .rows_affected()
                == 1;
            paid.push(MilestonePaid {
                reward: reward.clone(),
                cosmetic_new,
            });
        }
        let (bucket,): (i64,) = sqlx::query_as("SELECT count(*) FROM fish_bucket WHERE user_id = $1")
            .bind(id)
            .fetch_one(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(CatchLog {
            first,
            previous_best,
            record: !first && catch.size_cm > previous_best,
            species,
            bucket,
            week_best,
            paid,
        })
    }

    /// The fish in the bucket, species by species, the biggest of each first.
    pub async fn bucket(&self, user: u64) -> anyhow::Result<Vec<BucketRow>> {
        Ok(sqlx::query_as::<_, BucketRow>(
            "SELECT id, fish_id, size_cm FROM fish_bucket WHERE user_id = $1 ORDER BY fish_id, size_cm DESC, id",
        )
        .bind(sql_id(user))
        .fetch_all(self.pool())
        .await?)
    }

    pub async fn bucket_count(&self, user: u64) -> anyhow::Result<i64> {
        let (n,): (i64,) = sqlx::query_as("SELECT count(*) FROM fish_bucket WHERE user_id = $1")
            .bind(sql_id(user))
            .fetch_one(self.pool())
            .await?;
        Ok(n)
    }

    /// Sells the fish `(id, value)` pairs name and pays their value. Only fish still in their bucket
    /// are sold and paid for, so a second click on the same sale pays nothing. The fish book is not
    /// touched.
    pub async fn sell_fish(&self, user: u64, sales: &[(i64, i64)]) -> anyhow::Result<Sold> {
        let id = sql_id(user);
        let ids: Vec<i64> = sales.iter().map(|(fish, _)| *fish).collect();
        let mut tx = self.pool().begin().await?;
        sqlx::query("INSERT INTO users (id) VALUES ($1) ON CONFLICT (id) DO NOTHING")
            .bind(id)
            .execute(&mut *tx)
            .await?;
        let gone: Vec<(i64,)> =
            sqlx::query_as("DELETE FROM fish_bucket WHERE user_id = $1 AND id = ANY($2) RETURNING id")
                .bind(id)
                .bind(&ids)
                .fetch_all(&mut *tx)
                .await?;
        let shells: i64 = sales
            .iter()
            .filter(|(fish, _)| gone.iter().any(|(sold,)| sold == fish))
            .map(|(_, value)| value)
            .sum();
        let balance = if shells > 0 {
            let (balance,): (i64,) =
                sqlx::query_as("UPDATE users SET credits = credits + $2 WHERE id = $1 RETURNING credits")
                    .bind(id)
                    .bind(shells)
                    .fetch_one(&mut *tx)
                    .await?;
            ledger(&mut tx, id, shells, "fish_sale").await?;
            balance
        } else {
            balance_of(&mut tx, id, Currency::Shells).await?
        };
        tx.commit().await?;
        Ok(Sold {
            fish: gone.len() as i64,
            shells,
            balance,
        })
    }

    /// The server's weekly catch board for the week starting `week`: each player's biggest catch for
    /// its species, biggest first.
    pub async fn weekly_catch_board(
        &self,
        guild_id: u64,
        week: NaiveDate,
        limit: i64,
        offset: i64,
    ) -> anyhow::Result<Vec<CatchBoardRow>> {
        Ok(sqlx::query_as::<_, CatchBoardRow>(
            "SELECT user_id, fish_id, size_cm, ratio FROM weekly_catch
             WHERE guild_id = $1 AND week = $2
             ORDER BY ratio DESC, caught_at, user_id
             LIMIT $3 OFFSET $4",
        )
        .bind(sql_id(guild_id))
        .bind(week)
        .bind(limit)
        .bind(offset)
        .fetch_all(self.pool())
        .await?)
    }

    /// Forgets the weekly catches of weeks that started before `week`; returns how many rows went.
    pub async fn forget_catch_weeks_before(&self, week: NaiveDate) -> anyhow::Result<u64> {
        Ok(sqlx::query("DELETE FROM weekly_catch WHERE week < $1")
            .bind(week)
            .execute(self.pool())
            .await?
            .rows_affected())
    }
}
