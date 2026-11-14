mod channels;
mod commands;
mod guild;
mod media;
mod progression;
mod runtime;

use std::{
    collections::HashMap,
    future::Future,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

use chrono::{DateTime, Utc};
use moka::future::Cache as Moka;
use pinchora_cache::Cache;
use pinchora_core::admin::modules::Access;
use pinchora_core::admin::modules::GuildModules;
use pinchora_core::admin::modules::Module;
use pinchora_core::moderation::automod::Config as AutoModConfig;
use pinchora_core::progression::leveling::XpRules;
use pinchora_core::progression::leveling_policy::LevelingPolicy;
use pinchora_db::{Db, GuildSettings};
use pinchora_render::{Renderer, placeholder_avatar, prepare_avatar};
use pinchora_services::{Services, infra::Infrastructure};
use tokio::sync::Semaphore;
use twilight_http::{Client, client::InteractionClient, error::ErrorType};
use twilight_model::{
    channel::permission_overwrite::PermissionOverwrite,
    guild::Role,
    id::{
        Id,
        marker::{ApplicationMarker, ChannelMarker, MessageMarker, UserMarker},
    },
    user::User,
    util::ImageHash,
};

use crate::admin::custom::CustomState;
use crate::commands::moderation::ModState;
use crate::infra::text::access::CommandOverrides;
use crate::moderation::server_log::LogState;

/// Where a channel sits, for XP multipliers that follow a thread to its channel and a channel to its category.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ChannelPlace {
    pub(crate) parent: Option<u64>,
    pub(crate) thread: bool,
}

/// How long a channel's place is kept. A channel rarely moves, and Discord is asked once per channel, so
/// this is hours; GUILD_CREATE fills the places in anyway.
const CHANNEL_PLACE_KEPT: Duration = Duration::from_secs(6 * 3600);
/// How long a channel Discord could not tell us about (deleted, hidden from the bot) is left alone before
/// it is asked about again.
const CHANNEL_MISS_KEPT: Duration = Duration::from_secs(10 * 60);
/// How many guild rebuilds (voice presence from GUILD_CREATE) run at once: a small share of the work, so an
/// identify storm cannot crowd the message and member events out of their slots.
const REBUILD_SLOTS: usize = 16;

pub use pinchora_services::infra::PreviewError;

/// What the voice XP ticker needs about a server, kept for minutes so a tick every minute does not ask
/// the database every time.
pub(crate) struct VoiceGuild {
    pub settings: Arc<GuildSettings>,
    pub rules: Arc<XpRules>,
}

/// What a text command needs about a server to stand in for Discord: who owns it and its roles with
/// their permissions.
pub(crate) struct PermissionFacts {
    pub owner: u64,
    pub roles: Vec<Role>,
}

/// What a text command needs about a channel to stand in for Discord: where command permissions are
/// looked up, and which permission overwrites apply there.
pub(crate) struct ChannelRules {
    /// The channel, followed by its parent when it is a thread (a thread follows its channel).
    pub lineage: Vec<u64>,
    /// The overwrites that count in it: the channel's own, or its parent's for a thread.
    pub overwrites: Vec<PermissionOverwrite>,
}

/// Whether Discord's answer says the application has no command permissions to read in a server (it
/// was added without the commands scope), rather than that Discord could not be asked.
fn is_missing(error: &twilight_http::Error) -> bool {
    matches!(error.kind(), ErrorType::Response { status, .. } if matches!(status.get(), 403 | 404))
}

/// Everything a handler needs. Built once per process and shared as `Arc<App>`.
pub struct App {
    pub services: Arc<Services>,
    /// REST client; goes through the shared http-proxy when one is configured.
    pub http: Arc<Client>,
    /// Direct client for interaction callbacks, which skip the global ratelimit.
    pub interaction_http: Arc<Client>,
    pub application_id: Id<ApplicationMarker>,
    pub db: Db,
    pub cache: Cache,
    /// Shown by /ping, e.g. `gateway-2 (shards 32-47)` or `interactions`.
    pub node: String,
    /// Whether the gateway reads message text (the Message Content intent), so text commands work
    /// with a server's prefix; without it they need an @mention of the bot, keyword auto-responders stay
    /// deaf, the server log keeps no message text and `/purge` filters nothing by what messages say.
    pub message_content: bool,
    /// Public dashboard origin, including when configured through pinchora.toml.
    pub public_url: String,
    pub web: reqwest::Client,
    pub sprites: Arc<crate::games::game::scene::Sprites>,
    guild_settings: Moka<u64, Arc<GuildSettings>>,
    pub(crate) branding_colors: Moka<u64, Option<i32>>,
    guild_roles: Moka<u64, Arc<HashMap<u64, String>>>,
    permission_facts: Moka<u64, Arc<PermissionFacts>>,
    channel_rules: Moka<u64, Arc<ChannelRules>>,
    command_ids: Moka<(), Arc<HashMap<String, u64>>>,
    saved_overrides: Moka<u64, Option<Arc<CommandOverrides>>>,
    command_overrides: Moka<u64, Arc<CommandOverrides>>,
    xp_rules: Moka<u64, Arc<XpRules>>,
    command_control: Moka<u64, Arc<pinchora_core::admin::command_control::Config>>,
    module_rules: Moka<u64, Arc<GuildModules>>,
    automod_configs: Moka<u64, Arc<AutoModConfig>>,
    pub(crate) antinuke_enabled: Moka<u64, bool>,
    voice_guilds: Moka<u64, Arc<VoiceGuild>>,
    channel_places: Moka<u64, ChannelPlace>,
    pub(crate) channel_guilds: Moka<u64, Option<u64>>,
    channel_misses: Moka<u64, ()>,
    avatars: Moka<String, Arc<Vec<u8>>>,
    users: Moka<u64, Arc<User>>,
    pub(crate) languages: crate::infra::lang::Languages,
    event_slots: Arc<Semaphore>,
    rebuild_slots: Arc<Semaphore>,
    invite_baselines: Mutex<HashMap<u64, u64>>,
    invite_epoch: AtomicU64,
    /// Last-seen names and voice channels, so the server log can say what changed.
    pub(crate) log_state: LogState,
    /// Server owners, role positions and the bot's roles, for the moderation hierarchy.
    pub(crate) moderation: ModState,
    /// Custom commands and auto-responders, and what they remember.
    pub(crate) custom: CustomState,
}

impl App {
    pub fn new(
        http: Arc<Client>,
        interaction_http: Arc<Client>,
        application_id: Id<ApplicationMarker>,
        db: Db,
        cache: Cache,
        node: String,
    ) -> Self {
        let infrastructure = Arc::new(Infrastructure::new(http, interaction_http, db, cache));
        let services = Arc::new(Services::new(infrastructure.clone()));
        Self {
            http: infrastructure.http.clone(),
            interaction_http: infrastructure.interaction_http.clone(),
            application_id,
            db: infrastructure.db.clone(),
            cache: infrastructure.cache.clone(),
            services,
            node,
            message_content: false,
            public_url: pinchora_core::infra::config::WebConfig::default().public_url,
            web: infrastructure.web.clone(),
            sprites: Arc::new(crate::games::game::scene::Sprites::default()),
            // Other pods see a settings change after at most this TTL.
            guild_settings: Moka::builder()
                .max_capacity(50_000)
                .time_to_live(Duration::from_secs(60))
                .build(),
            branding_colors: Moka::builder()
                .max_capacity(50_000)
                .time_to_live(Duration::from_secs(60))
                .build(),
            guild_roles: Moka::builder()
                .max_capacity(10_000)
                .time_to_live(Duration::from_secs(60))
                .build(),
            permission_facts: Moka::builder()
                .max_capacity(10_000)
                .time_to_live(Duration::from_secs(60))
                .build(),
            channel_rules: Moka::builder()
                .max_capacity(50_000)
                .time_to_live(Duration::from_secs(60))
                .build(),
            saved_overrides: Moka::builder()
                .max_capacity(10_000)
                .time_to_live(Duration::from_secs(60))
                .build(),
            command_ids: Moka::builder()
                .max_capacity(1)
                .time_to_live(Duration::from_secs(600))
                .build(),
            command_overrides: Moka::builder()
                .max_capacity(10_000)
                .time_to_live(Duration::from_secs(300))
                .build(),
            xp_rules: Moka::builder()
                .max_capacity(50_000)
                .time_to_live(Duration::from_secs(60))
                .build(),
            command_control: Moka::builder()
                .max_capacity(50_000)
                .time_to_live(Duration::from_secs(60))
                .build(),
            module_rules: Moka::builder()
                .max_capacity(50_000)
                .time_to_live(Duration::from_secs(60))
                .build(),
            antinuke_enabled: Moka::builder()
                .max_capacity(50_000)
                .time_to_live(Duration::from_secs(60))
                .build(),
            automod_configs: Moka::builder()
                .max_capacity(50_000)
                .time_to_live(Duration::from_secs(60))
                .build(),
            // Longer than the minute between ticks; a change made on another pod waits at most this long.
            voice_guilds: Moka::builder()
                .max_capacity(50_000)
                .time_to_live(Duration::from_secs(300))
                .build(),
            channel_places: Moka::builder()
                .max_capacity(500_000)
                .time_to_live(CHANNEL_PLACE_KEPT)
                .build(),
            channel_guilds: Moka::builder()
                .max_capacity(100_000)
                .time_to_live(CHANNEL_PLACE_KEPT)
                .build(),
            channel_misses: Moka::builder()
                .max_capacity(100_000)
                .time_to_live(CHANNEL_MISS_KEPT)
                .build(),
            avatars: Moka::builder()
                .weigher(|_k: &String, v: &Arc<Vec<u8>>| v.len().try_into().unwrap_or(u32::MAX))
                .max_capacity(64 * 1024 * 1024)
                .time_to_live(Duration::from_secs(600))
                .build(),
            users: Moka::builder()
                .max_capacity(10_000)
                .time_to_live(Duration::from_secs(600))
                .build(),
            languages: crate::infra::lang::Languages::new(),
            event_slots: Arc::new(Semaphore::new(1024)),
            rebuild_slots: Arc::new(Semaphore::new(REBUILD_SLOTS)),
            invite_baselines: Mutex::new(HashMap::new()),
            invite_epoch: AtomicU64::new(0),
            log_state: LogState::new(),
            moderation: ModState::new(),
            custom: CustomState::new(),
        }
    }
}
