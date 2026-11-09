pub mod admin;
pub mod community;
pub mod economy;
pub mod games;
pub mod house;
pub mod infra;
pub mod integrations;
pub mod moderation;
pub mod progression;
pub mod roles;
pub mod social;
pub use crate::house::house::HouseEditorGuard;
pub use crate::house::house_settings::HouseUpdate;
pub use crate::house::house_settings::HouseUpdateOutcome;

/// Truncate an application timestamp to PostgreSQL's microsecond precision before
/// comparing it in Rust with a stored timestamp. SQLx already does this for SQL binds.
pub fn timestamp(at: chrono::DateTime<chrono::Utc>) -> chrono::DateTime<chrono::Utc> {
    use chrono::Timelike;
    at.with_nanosecond(at.nanosecond() / 1_000 * 1_000)
        .expect("truncating nanoseconds keeps a valid timestamp")
}

use std::time::Duration;

use sqlx::{PgPool, postgres::PgPoolOptions};

pub use crate::admin::afk_sticky_schedule::AfkRow;
pub use crate::admin::afk_sticky_schedule::ScheduledRow;
pub use crate::admin::afk_sticky_schedule::UtilityStickyRow;
pub use crate::admin::branding::GuildBranding;
pub use crate::admin::card_designs::CardDesignRow;
pub use crate::admin::card_designs::CardUploadRow;
pub use crate::admin::guilds::GuildSettings;
pub use crate::admin::guilds::LogKind;
pub use crate::admin::guilds::PatchRefused;
pub use crate::admin::members::LeaderboardRow;
pub use crate::admin::members::MemberRank;
pub use crate::admin::members::XpGain;
pub use crate::admin::members::XpGrant;
pub use crate::admin::public_stats::PublicCounts;
pub use crate::admin::temp_voice::CreatorSave;
pub use crate::admin::temp_voice::NewTempRoom;
pub use crate::admin::temp_voice::RoomMake;
pub use crate::admin::temp_voice::TempRoom;
pub use crate::admin::temp_voice::VoiceCreator;
pub use crate::community::birthdays::*;
pub use crate::community::community::*;
pub use crate::community::contest::ContestEntry;
pub use crate::community::contest::ContestResult;
pub use crate::community::events::*;
pub use crate::community::feeds::*;
pub use crate::community::giveaways::Due;
pub use crate::community::giveaways::EndedGiveaway;
pub use crate::community::giveaways::EntryToggle;
pub use crate::community::giveaways::GiveawayOpen;
pub use crate::community::giveaways::GiveawayRow;
pub use crate::community::giveaways::MAX_RUNNING_GIVEAWAYS;
pub use crate::community::giveaways::NewGiveaway;
pub use crate::community::giveaways::Reroll;
pub use crate::community::guestbook::NoteRow;
pub use crate::community::guestbook::SignOutcome;
pub use crate::community::guestbook::VisitorRow;
pub use crate::community::invites::GuildInvite;
pub use crate::community::invites::InviteJoin;
pub use crate::community::invites::InviteObservation;
pub use crate::community::invites::InviteScore;
pub use crate::community::invites::InviteSettings;
pub use crate::community::invites::JoinCredit;
pub use crate::community::partners::PairOutcome;
pub use crate::community::partners::PartnerRow;
pub use crate::community::periodic::PeriodOutcome;
pub use crate::community::polls::MAX_OPEN_POLLS;
pub use crate::community::polls::NewPoll;
pub use crate::community::polls::PollOpen;
pub use crate::community::polls::PollRow;
pub use crate::community::polls::PollVote;
pub use crate::community::reminders::MAX_TEXT_REMINDERS;
pub use crate::community::reminders::OPT_IN_KINDS;
pub use crate::community::reminders::ReminderRow;
pub use crate::community::reminders::TEXT as TEXT_REMINDER;
pub use crate::community::starboard::StarPost;
pub use crate::community::starboard::Starboard;
pub use crate::community::starboard::VoteChannel;
pub use crate::community::suggestions::APPROVED as SUGGESTION_APPROVED;
pub use crate::community::suggestions::DENIED as SUGGESTION_DENIED;
pub use crate::community::suggestions::Decision;
pub use crate::community::suggestions::SuggestionRow;
pub use crate::community::suggestions::SuggestionVote;
pub use crate::community::tags::CommandRow;
pub use crate::community::tags::ResponderRow;
pub use crate::community::tags::TagSet;
pub use crate::community::tags::TagWrite;
pub use crate::community::ticket_ops::*;
pub use crate::community::tickets::*;
pub use crate::community::trackers::RiotLink;
pub use crate::community::trackers::TrackerSettings;
pub use crate::economy::legacy::DailyOutcome;
pub use crate::economy::legacy::RepOutcome;
pub use crate::economy::legacy::TransferOutcome;
pub use crate::economy::rewards::LevelRoleRow;
pub use crate::economy::rewards::MAX_LEVEL_ROLES;
pub use crate::economy::rewards::MAX_REP_ROLES;
pub use crate::economy::rewards::MAX_STORE_ITEMS;
pub use crate::economy::rewards::RepRoleRow;
pub use crate::economy::rewards::StoreBuy;
pub use crate::economy::rewards::StoreItem;
pub use crate::economy::shop::BuyOutcome;
pub use crate::economy::shop::Currency;
pub use crate::economy::shop::MarketRow;
pub use crate::economy::shop::VisitOutcome;
pub use crate::economy::stalls::ListingRow;
pub use crate::economy::stalls::MAX_LISTINGS;
pub use crate::economy::stalls::STALL_FEE_PERCENT;
pub use crate::economy::stalls::StallBuy;
pub use crate::games::board_games::PicrossGame;
pub use crate::games::board_games::SoloGame;
pub use crate::games::board_games::SoloStep;
pub use crate::games::collection::Award;
pub use crate::games::collection::BOTTLE as DROP_BOTTLE;
pub use crate::games::collection::BottleOpen;
pub use crate::games::collection::BottlePrize;
pub use crate::games::collection::CRITTERS as DROP_CRITTERS;
pub use crate::games::collection::CollectionRoll;
pub use crate::games::collection::CollectionSettings;
pub use crate::games::collection::Comb;
pub use crate::games::collection::Daily;
pub use crate::games::collection::DexRow;
pub use crate::games::collection::DropRow;
pub use crate::games::collection::Grab;
pub use crate::games::collection::Granted;
pub use crate::games::collection::NewDrop;
pub use crate::games::collection::PotHaul;
pub use crate::games::collection::PotRow;
pub use crate::games::collection::PotUpgrade;
pub use crate::games::collection::RollClaim;
pub use crate::games::collection::RollStart;
pub use crate::games::collection::VoteCard;
pub use crate::games::collection::WishlistRow;
pub use crate::games::fishing::BucketRow;
pub use crate::games::fishing::CatchBoardRow;
pub use crate::games::fishing::CatchLog;
pub use crate::games::fishing::Cosmetic;
pub use crate::games::fishing::DEFAULT_ROD;
pub use crate::games::fishing::DEFAULT_SPOT;
pub use crate::games::fishing::GearBuy;
pub use crate::games::fishing::GearRow;
pub use crate::games::fishing::MilestonePaid;
pub use crate::games::fishing::MilestoneReward;
pub use crate::games::fishing::NewCatch;
pub use crate::games::fishing::Sold;
pub use crate::games::games::ACTIVE;
pub use crate::games::games::DONE;
pub use crate::games::games::DuelStart;
pub use crate::games::games::GameSession;
pub use crate::games::games::GameStat;
pub use crate::games::games::NewGame;
pub use crate::games::games::PENDING;
pub use crate::games::games::Payout;
pub use crate::games::games::WordleGame;
pub use crate::games::party::CrewJoin;
pub use crate::games::party::DerbyJoin;
pub use crate::games::party::DerbyLeave;
pub use crate::games::party::DerbyOutcome;
pub use crate::games::party::DerbyResult;
pub use crate::games::party::HeistSettled;
pub use crate::games::party::PartyMember;
pub use crate::games::party::PartySettings;
pub use crate::games::party::ShellDone;
pub use crate::games::party::ShellLimits;
pub use crate::games::party::ShellPick;
pub use crate::games::party::ShellStart;
pub use crate::games::party::SurgeHelp;
pub use crate::games::party::SurgeSettled;
pub use crate::games::party_games::Commit;
pub use crate::games::party_games::Committed;
pub use crate::games::party_games::LOBBY;
pub use crate::games::party_games::NewEntry;
pub use crate::games::party_games::NewParty;
pub use crate::games::party_games::PLAYING;
pub use crate::games::party_games::PartyEntry;
pub use crate::games::party_games::PartyGame;
pub use crate::games::party_games::PartyJoin;
pub use crate::games::party_games::PartyLeave;
pub use crate::games::party_games::PartyPlayer;
pub use crate::games::party_games::PartyVote;
pub use crate::games::party_games::Pay;
pub use crate::games::party_games::PlayerChange;
pub use crate::games::party_games::PollChoice;
pub use crate::house::house::HouseRow;
pub use crate::house::house::InventoryRow;
pub use crate::house::house::NewRoom;
pub use crate::house::house::PlaceOutcome;
pub use crate::house::house::PlacementRow;
pub use crate::house::house::Refusal;
pub use crate::house::house::RestoreOutcome;
pub use crate::house::house::RoomChange;
pub use crate::house::house::RoomRow;
pub use crate::house::house::SOULBOUND_GUEST;
pub use crate::house::house::UpgradeOutcome;
pub use crate::infra::locks::conflict_retries;
pub use crate::moderation::audit::AuditRow;
pub use crate::moderation::audit::AuditSource;
pub use crate::moderation::audit::NewAudit;
pub use crate::moderation::moderation::CaseFilter;
pub use crate::moderation::moderation::CaseRow;
pub use crate::moderation::moderation::LadderFull;
pub use crate::moderation::moderation::ModSettings;
pub use crate::moderation::moderation::NewCase;
pub use crate::moderation::moderation::NewStaffAction;
pub use crate::moderation::moderation::OpenedCase;
pub use crate::moderation::moderation::StaffActionRow;
pub use crate::moderation::moderation::StaffFilter;
pub use crate::moderation::privacy::ANONYMISED;
pub use crate::moderation::privacy::CLEARED_COLUMNS;
pub use crate::moderation::privacy::DAILY_LIMITS;
pub use crate::moderation::privacy::DELETED_NAME;
pub use crate::moderation::privacy::Deleted;
pub use crate::moderation::privacy::Hide;
pub use crate::moderation::privacy::ID_LISTS;
pub use crate::moderation::privacy::ListEdit;
pub use crate::moderation::privacy::OTHER_TABLES;
pub use crate::moderation::privacy::Privacy;
pub use crate::moderation::privacy::TOMBSTONE_DAYS;
pub use crate::moderation::privacy::USER_COLUMNS;
pub use crate::moderation::privacy::ZEROED_COLUMNS;
pub use crate::progression::achievements::AchievementDraft;
pub use crate::progression::achievements::AchievementRow;
pub use crate::progression::achievements::AchievementSave;
pub use crate::progression::activity::Activity;
pub use crate::progression::activity::Counter;
pub use crate::progression::leveling_policy::VoiceLevelRole;
pub use crate::progression::stats::ActivityDay;
pub use crate::progression::stats::ActivityRank;
pub use crate::progression::stats::CounterRow;
pub use crate::progression::stats::CounterSave;
pub use crate::progression::stats::GlobalDay;
pub use crate::progression::stats::GlobalTotals;
pub use crate::progression::xp::MAX_XP_RULES;
pub use crate::progression::xp::XpTarget;
pub use crate::progression::xp_admin::XpEdit;
pub use crate::roles::automation::AutoRoleRow;
pub use crate::roles::automation::ColorAdded;
pub use crate::roles::automation::ColorRoleRow;
pub use crate::roles::automation::ColorSettings;
pub use crate::roles::automation::DelayedRoleRow;
pub use crate::roles::automation::NewAutoRole;
pub use crate::roles::automation::StickyRow;
pub use crate::roles::automation::TimedRoleRow;
pub use crate::roles::panels::MAX_PANEL_ROLES;
pub use crate::roles::panels::PanelDoc;
pub use crate::roles::panels::PanelEntry;
pub use crate::roles::panels::PanelPatch;
pub use crate::roles::panels::PanelRoleDoc;
pub use crate::roles::panels::PanelRoleRow;
pub use crate::roles::panels::PanelRow;
pub use crate::roles::panels::PanelSaved;
pub use crate::social::character::OutfitRow;
pub use crate::social::customize::CustomizeBuy;
pub use crate::social::customize::MAX_BADGES;
pub use crate::social::friends::BefriendOutcome;
pub use crate::social::friends::FriendRow;
pub use crate::social::friends::MAX_FRIENDS;
pub use crate::social::interactions::Tally;
pub use crate::social::pets::PetCare;
pub use crate::social::pets::PetRow;
pub use crate::social::pets::TripClaim;
pub use crate::social::pets::TripPayout;
pub use crate::social::pets::Wake;
pub use crate::social::showcase::FishBookRow;
pub use crate::social::showcase::HousePreview;
pub use crate::social::users::UserRow;

/// Handle to PostgreSQL. Cheap to clone.
#[derive(Clone, Debug)]
pub struct Db {
    pool: PgPool,
    editor_slots: std::sync::Arc<tokio::sync::Semaphore>,
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
        Ok(Self {
            pool,
            editor_slots: std::sync::Arc::new(tokio::sync::Semaphore::new((max_connections / 2) as usize)),
        })
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

pub use crate::moderation::automod::*;

pub use crate::moderation::anti_nuke::*;
pub use crate::moderation::verification::*;

pub use crate::games::reef_safety::*;

pub use crate::games::reef_realtime::ReefFence;

pub use crate::integrations::roblox::*;

pub use crate::games::web_player::*;

pub use crate::games::reef_loop::*;

pub use crate::admin::server_tools::AuditFilter;
pub use crate::admin::server_tools::DASHBOARD_AREAS;
pub use crate::admin::server_tools::DashboardManager;
pub use crate::admin::server_tools::MAX_DASHBOARD_MANAGERS;
pub use crate::games::reef_party::ReefMove;

pub use crate::games::reef_spaces::*;

pub use crate::admin::workflows::*;
pub use crate::games::reef_games::*;
