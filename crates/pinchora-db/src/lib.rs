mod achievements;
mod activity;
mod audit;
mod birthdays;
mod board_games;
mod branding;
mod card_designs;
mod character;
mod collection;
mod contest;
mod customize;
mod economy;
mod events;
mod feeds;
mod fishing;
mod friends;
mod games;
mod giveaways;
mod guestbook;
mod guilds;
mod house;
mod house_read;
mod interactions;
mod invites;
mod language;
mod locks;
mod logs;
mod members;
mod moderation;
mod modules;
mod panels;
mod partners;
mod party;
mod party_games;
mod periodic;
mod pets;
mod polls;
mod privacy;
mod public_stats;
mod reminders;
mod rewards;
mod roles_pack;
mod shop;
mod showcase;
mod social;
mod stalls;
mod starboard;
mod stats;
mod suggestions;
mod tags;
mod temp_voice;
mod tickets;
mod users;
mod utility;
mod xp;
mod xp_admin;

use std::time::Duration;

use sqlx::{PgPool, postgres::PgPoolOptions};

pub use achievements::{AchievementDraft, AchievementRow, AchievementSave};
pub use activity::{Activity, Counter};
pub use audit::{AuditRow, AuditSource, NewAudit};
pub use birthdays::*;
pub use board_games::{PicrossGame, SoloGame, SoloStep};
pub use branding::GuildBranding;
pub use card_designs::{CardDesignRow, CardUploadRow};
pub use character::OutfitRow;
pub use collection::{
    Award, BOTTLE as DROP_BOTTLE, BottleOpen, BottlePrize, CRITTERS as DROP_CRITTERS, CollectionSettings, Comb, Daily,
    DexRow, DropRow, Grab, Granted, NewDrop, PotHaul, PotRow, PotUpgrade,
};
pub use community::*;
pub use contest::{ContestEntry, ContestResult};
pub use customize::{CustomizeBuy, MAX_BADGES};
pub use economy::{DailyOutcome, RepOutcome, TransferOutcome};
pub use events::*;
pub use feeds::*;
pub use fishing::{
    BucketRow, CatchBoardRow, CatchLog, Cosmetic, DEFAULT_ROD, DEFAULT_SPOT, GearBuy, GearRow, MilestonePaid,
    MilestoneReward, NewCatch, Sold,
};
pub use friends::{BefriendOutcome, FriendRow, MAX_FRIENDS};
pub use games::{ACTIVE, DONE, DuelStart, GameSession, GameStat, NewGame, PENDING, Payout, WordleGame};
pub use giveaways::{
    Due, EndedGiveaway, EntryToggle, GiveawayOpen, GiveawayRow, MAX_RUNNING_GIVEAWAYS, NewGiveaway, Reroll,
};
pub use guestbook::{NoteRow, SignOutcome, VisitorRow};
pub use guilds::{GuildSettings, LogKind, PatchRefused};
pub use house::{
    HouseRow, InventoryRow, NewRoom, PlaceOutcome, PlacementRow, Refusal, RestoreOutcome, RoomChange, RoomRow,
    SOULBOUND_GUEST, UpgradeOutcome,
};
pub use interactions::Tally;
pub use invites::{GuildInvite, InviteJoin, InviteObservation, InviteScore, InviteSettings, JoinCredit};
pub use locks::conflict_retries;
pub use members::{LeaderboardRow, MemberRank, XpGain, XpGrant};
pub use moderation::{
    CaseFilter, CaseRow, LadderFull, ModSettings, NewCase, NewStaffAction, OpenedCase, StaffActionRow, StaffFilter,
};
pub use panels::{MAX_PANEL_ROLES, PanelDoc, PanelEntry, PanelPatch, PanelRoleDoc, PanelRoleRow, PanelRow, PanelSaved};
pub use partners::{PairOutcome, PartnerRow};
pub use party::{
    CrewJoin, DerbyJoin, DerbyLeave, DerbyOutcome, DerbyResult, HeistSettled, PartyMember, PartySettings, ShellDone,
    ShellLimits, ShellPick, ShellStart, SurgeHelp, SurgeSettled,
};
pub use party_games::{
    Commit, Committed, LOBBY, NewEntry, NewParty, PLAYING, PartyEntry, PartyGame, PartyJoin, PartyLeave, PartyPlayer,
    PartyVote, Pay, PlayerChange, PollChoice,
};
pub use periodic::PeriodOutcome;
pub use pets::{PetRow, TripClaim, TripPayout, Wake};
pub use polls::{MAX_OPEN_POLLS, NewPoll, PollOpen, PollRow, PollVote};
pub use privacy::{
    ANONYMISED, CLEARED_COLUMNS, DAILY_LIMITS, DELETED_NAME, Deleted, Hide, ID_LISTS, ListEdit, OTHER_TABLES, Privacy,
    TOMBSTONE_DAYS, USER_COLUMNS, ZEROED_COLUMNS,
};
pub use public_stats::PublicCounts;
pub use reminders::{MAX_TEXT_REMINDERS, OPT_IN_KINDS, ReminderRow, TEXT as TEXT_REMINDER};
pub use rewards::{LevelRoleRow, MAX_LEVEL_ROLES, MAX_REP_ROLES, MAX_STORE_ITEMS, RepRoleRow, StoreBuy, StoreItem};
pub use roles_pack::{
    AutoRoleRow, ColorAdded, ColorRoleRow, ColorSettings, DelayedRoleRow, NewAutoRole, StickyRow, TimedRoleRow,
};
pub use shop::{BuyOutcome, Currency, MarketRow, VisitOutcome};
pub use showcase::{FishBookRow, HousePreview};
pub use stalls::{ListingRow, MAX_LISTINGS, STALL_FEE_PERCENT, StallBuy};
pub use starboard::{StarPost, Starboard, VoteChannel};
pub use stats::{ActivityDay, ActivityRank, CounterRow, CounterSave, GlobalDay, GlobalTotals};
pub use suggestions::{
    APPROVED as SUGGESTION_APPROVED, DENIED as SUGGESTION_DENIED, Decision, SuggestionRow, SuggestionVote,
};
pub use tags::{CommandRow, ResponderRow, TagSet, TagWrite};
pub use temp_voice::{CreatorSave, NewTempRoom, RoomMake, TempRoom, VoiceCreator};
pub use tickets::*;
pub use users::UserRow;
pub use utility::{AfkRow, ScheduledRow, UtilityStickyRow};
pub use xp::{MAX_XP_RULES, XpTarget};
pub use xp_admin::XpEdit;

/// Handle to PostgreSQL. Cheap to clone.
#[derive(Clone, Debug)]
pub struct Db {
    pool: PgPool,
}

impl Db {
    pub async fn connect(url: &str, max_connections: u32) -> anyhow::Result<Self> {
        Self::connect_waiting(url, max_connections, Duration::from_secs(5)).await
    }

    /// Like [`Db::connect`], with `acquire_timeout` as the time a query may wait for a free connection
    /// before it fails. The tests give it a minute: on a machine running several test suites at once a
    /// query can wait for its turn that long without anything being wrong.
    pub async fn connect_waiting(url: &str, max_connections: u32, acquire_timeout: Duration) -> anyhow::Result<Self> {
        let pool = PgPoolOptions::new()
            .max_connections(max_connections)
            .acquire_timeout(acquire_timeout)
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

mod automod;
pub use automod::*;

mod verification;
pub use verification::*;
mod anti_nuke;
pub use anti_nuke::*;

mod command_control;
mod community;

mod config_backup;
mod reef_safety;
pub use reef_safety::*;

mod reef_realtime;
pub use reef_realtime::ReefFence;

mod mod_parity;
mod roblox;
pub use roblox::*;
