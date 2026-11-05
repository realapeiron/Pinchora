mod customize;
mod economy;
mod guilds;
mod house;
mod members;
mod pets;
mod rewards;
mod shop;
mod social;
mod stalls;
mod users;

use std::time::Duration;

use sqlx::{PgPool, postgres::PgPoolOptions};

pub use customize::{CustomizeBuy, MAX_BADGES, PlayerStats};
pub use economy::{DailyOutcome, RepOutcome, TransferOutcome};
pub use guilds::GuildSettings;
pub use house::{HouseRow, InventoryRow, NewRoom, PlaceOutcome, PlacementRow, RoomRow, UpgradeOutcome};
pub use members::{LeaderboardRow, MemberRank, XpGain};
pub use pets::PetRow;
pub use rewards::{LevelRoleRow, MAX_LEVEL_ROLES, MAX_STORE_ITEMS, StoreBuy, StoreItem};
pub use shop::{BuyOutcome, Currency, MarketRow, VisitOutcome};
pub use stalls::{ListingRow, MAX_LISTINGS, STALL_FEE_PERCENT, StallBuy};
pub use users::UserRow;

/// Handle to PostgreSQL. Cheap to clone.
#[derive(Clone, Debug)]
pub struct Db {
    pool: PgPool,
}

impl Db {
    pub async fn connect(url: &str, max_connections: u32) -> anyhow::Result<Self> {
        let pool = PgPoolOptions::new()
            .max_connections(max_connections)
            .acquire_timeout(Duration::from_secs(5))
            .connect(url)
            .await?;
        Ok(Self { pool })
    }

    pub async fn migrate(&self) -> anyhow::Result<()> {
        sqlx::migrate!("../../migrations").run(&self.pool).await?;
        Ok(())
    }

    pub async fn ping(&self) -> anyhow::Result<()> {
        sqlx::query("SELECT 1").execute(&self.pool).await?;
        Ok(())
    }

    pub fn pool(&self) -> &PgPool {
        &self.pool
    }
}

/// Discord ids are u64 but always below 2^63, so they map losslessly to BIGINT.
pub(crate) fn sql_id(id: u64) -> i64 {
    id as i64
}
