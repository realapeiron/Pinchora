mod contest;
mod customize;
mod economy;
mod fishing;
mod friends;
mod games;
mod giveaways;
mod guestbook;
mod guilds;
mod house;
mod interactions;
mod members;
mod panels;
mod partners;
mod pets;
mod polls;
mod rewards;
mod shop;
mod showcase;
mod social;
mod stalls;
mod suggestions;
mod users;
mod xp;

use std::time::Duration;

use sqlx::{PgPool, postgres::PgPoolOptions};

pub use contest::{ContestEntry, ContestResult};
pub use customize::{CustomizeBuy, MAX_BADGES, PlayerStats};
pub use economy::{DailyOutcome, RepOutcome, TransferOutcome};
pub use fishing::{
    BucketRow, CatchBoardRow, CatchLog, Cosmetic, DEFAULT_ROD, DEFAULT_SPOT, GearBuy, GearRow, MilestonePaid,
    MilestoneReward, NewCatch, Sold,
};
pub use friends::{BefriendOutcome, FriendRow, MAX_FRIENDS};
pub use games::{ACTIVE, DONE, DuelStart, GameSession, GameStat, NewGame, PENDING, Payout, WordleGame};
pub use giveaways::{
    EndedGiveaway, EntryToggle, GiveawayOpen, GiveawayRow, MAX_RUNNING_GIVEAWAYS, NewGiveaway, Reroll,
};
pub use guestbook::{NoteRow, SignOutcome, VisitorRow};
pub use guilds::{GuildSettings, LogKind};
pub use house::{
    HouseRow, InventoryRow, NewRoom, PlaceOutcome, PlacementRow, Refusal, RestoreOutcome, RoomChange, RoomRow,
    UpgradeOutcome,
};
pub use interactions::Tally;
pub use members::{LeaderboardRow, MemberRank, XpGain, XpGrant};
pub use panels::{MAX_PANEL_ROLES, PanelRoleRow, PanelRow};
pub use partners::{PairOutcome, PartnerRow};
pub use pets::{PetRow, TripClaim, TripPayout, Wake};
pub use polls::{MAX_OPEN_POLLS, NewPoll, PollOpen, PollRow, PollVote};
pub use rewards::{LevelRoleRow, MAX_LEVEL_ROLES, MAX_REP_ROLES, MAX_STORE_ITEMS, RepRoleRow, StoreBuy, StoreItem};
pub use shop::{BuyOutcome, Currency, MarketRow, VisitOutcome};
pub use showcase::{FishBookRow, HousePreview};
pub use stalls::{ListingRow, MAX_LISTINGS, STALL_FEE_PERCENT, StallBuy};
pub use suggestions::{
    APPROVED as SUGGESTION_APPROVED, DENIED as SUGGESTION_DENIED, Decision, SuggestionRow, SuggestionVote,
};
pub use users::UserRow;
pub use xp::{MAX_XP_RULES, XpTarget};

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
