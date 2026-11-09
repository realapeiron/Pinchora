use std::{
    collections::HashMap,
    future::Future,
    sync::{
        Arc, Mutex,
        atomic::{AtomicI64, AtomicU64, Ordering},
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
use tokio::sync::{OwnedSemaphorePermit, Semaphore};
use tokio_util::{sync::CancellationToken, task::TaskTracker};
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

/// How many dashboard previews are drawn (or waiting for a render slot) at once, at most: a small share of the
/// render slots, so an editor being dragged around cannot crowd the member events' cards out.
const PREVIEW_SLOTS: usize = 2;
/// How long a preview waits for its turn before the dashboard is told the bot is busy.
const PREVIEW_WAIT: Duration = Duration::from_secs(3);
/// How long a preview may take from its turn to its picture. A card is within the card design's cost cap, which
/// draws in well under a second; this is for a starved or stuck pod.
const PREVIEW_BUDGET: Duration = Duration::from_secs(6);

/// Why a dashboard preview was not drawn.
#[derive(Debug)]
pub enum PreviewError {
    /// All the preview slots stayed taken for [`PREVIEW_WAIT`].
    Busy,
    /// The drawing did not finish within [`PREVIEW_BUDGET`]. The job still runs to its end (it cannot be
    /// interrupted) and keeps its slots until then.
    TooSlow,
    Failed(anyhow::Error),
}

impl std::fmt::Display for PreviewError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Busy => f.write_str("the preview slots are all taken"),
            Self::TooSlow => f.write_str("the preview took too long to draw"),
            Self::Failed(error) => write!(f, "{error}"),
        }
    }
}

impl std::error::Error for PreviewError {}

impl From<anyhow::Error> for PreviewError {
    fn from(error: anyhow::Error) -> Self {
        Self::Failed(error)
    }
}

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
    renderer: Arc<Renderer>,
    render_slots: Arc<Semaphore>,
    preview_slots: Arc<Semaphore>,
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
    tasks: TaskTracker,
    /// Cancelled when shutdown begins, so tasks that wait for a while can finish at once.
    shutdown: CancellationToken,
    event_slots: Arc<Semaphore>,
    rebuild_slots: Arc<Semaphore>,
    invite_baselines: Mutex<HashMap<u64, u64>>,
    invite_epoch: AtomicU64,
    /// Last-seen names and voice channels, so the server log can say what changed.
    pub(crate) log_state: LogState,
    /// Server owners, role positions and the bot's roles, for the moderation hierarchy.
    pub(crate) moderation: ModState,
    /// Seconds added to the clock by [`App::shift_clock`].
    clock_shift: AtomicI64,
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
        let cpus = std::thread::available_parallelism().map_or(2, usize::from);
        Self {
            http,
            interaction_http,
            application_id,
            db,
            cache,
            node,
            message_content: false,
            public_url: pinchora_core::infra::config::WebConfig::default().public_url,
            web: reqwest::Client::builder()
                .timeout(Duration::from_secs(5))
                .user_agent("Pinchora (https://github.com/realapeiron/Pinchora)")
                .build()
                .expect("reqwest client"),
            sprites: Arc::new(crate::games::game::scene::Sprites::default()),
            renderer: Arc::new(Renderer::new()),
            render_slots: Arc::new(Semaphore::new(cpus)),
            preview_slots: Arc::new(Semaphore::new(PREVIEW_SLOTS.min(cpus))),
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
            tasks: TaskTracker::new(),
            shutdown: CancellationToken::new(),
            event_slots: Arc::new(Semaphore::new(1024)),
            rebuild_slots: Arc::new(Semaphore::new(REBUILD_SLOTS)),
            invite_baselines: Mutex::new(HashMap::new()),
            invite_epoch: AtomicU64::new(0),
            log_state: LogState::new(),
            moderation: ModState::new(),
            clock_shift: AtomicI64::new(0),
            custom: CustomState::new(),
        }
    }

    /// The time as game rules that span days and weeks see it (the room contest).
    pub fn now(&self) -> DateTime<Utc> {
        Utc::now() + chrono::Duration::seconds(self.clock_shift.load(Ordering::Relaxed))
    }

    /// A gateway epoch must have a fresh invite baseline before joins can be attributed.
    pub fn begin_invite_baseline(&self, guild: u64) -> u64 {
        let mut pending = self.invite_baselines.lock().unwrap();
        let epoch = self.invite_epoch.fetch_add(1, Ordering::Relaxed).wrapping_add(1);
        pending.insert(guild, epoch);
        epoch
    }

    pub fn invite_baseline_current(&self, guild: u64, epoch: u64) -> bool {
        self.invite_baselines.lock().unwrap().get(&guild).copied() == Some(epoch)
    }

    pub fn finish_invite_baseline(&self, guild: u64, epoch: u64) {
        let mut pending = self.invite_baselines.lock().unwrap();
        if pending.get(&guild).copied() == Some(epoch) {
            pending.remove(&guild);
        }
    }

    pub fn invite_baseline_pending(&self, guild: u64) -> bool {
        self.invite_baselines.lock().unwrap().contains_key(&guild)
    }

    /// Moves [`App::now`] forward, so tests can end a contest week without waiting for it.
    pub fn shift_clock(&self, by: chrono::Duration) {
        self.clock_shift.fetch_add(by.num_seconds(), Ordering::Relaxed);
    }

    pub fn interaction(&self) -> InteractionClient<'_> {
        self.interaction_http.interaction(self.application_id)
    }

    /// Spawns a task that shutdown waits for.
    pub fn spawn<F>(&self, future: F)
    where
        F: Future<Output = ()> + Send + 'static,
    {
        self.tasks.spawn(future);
    }

    /// Deletes a message of ours after `after`, or as soon as shutdown begins, whichever comes
    /// first: a private answer given in a channel must not outlive the process. The wait is a
    /// tracked task, and a delete that fails is logged.
    pub fn delete_later(&self, channel: Id<ChannelMarker>, message: Id<MessageMarker>, after: Duration) {
        let (http, shutdown) = (self.http.clone(), self.shutdown.clone());
        self.spawn(async move {
            tokio::select! {
                () = tokio::time::sleep(after) => {}
                () = shutdown.cancelled() => {}
            }
            if let Err(error) = http.delete_message(channel, message).await {
                tracing::warn!(%error, channel = channel.get(), message = message.get(), "private answer not deleted");
            }
        });
    }

    /// Spawns a gateway event handler, or drops the event when 1024 are already
    /// running. Dropping XP ticks under overload beats unbounded memory.
    pub fn spawn_event<F>(&self, kind: &'static str, future: F)
    where
        F: Future<Output = ()> + Send + 'static,
    {
        match self.event_slots.clone().try_acquire_owned() {
            Ok(permit) => {
                self.tasks.spawn(async move {
                    future.await;
                    drop(permit);
                });
            }
            Err(_) => metrics::counter!("pinchora_events_dropped_total", "kind" => kind).increment(1),
        }
    }

    /// Spawns a guild rebuild: it waits for one of a few slots of its own (the event slots are for events), so
    /// a storm of GUILD_CREATEs at identify queues up here, as small snapshots, instead of using every slot
    /// the message and member events need.
    pub fn spawn_rebuild<F>(&self, future: F)
    where
        F: Future<Output = ()> + Send + 'static,
    {
        let slots = self.rebuild_slots.clone();
        self.tasks.spawn(async move {
            let Ok(_slot) = slots.acquire_owned().await else {
                return;
            };
            future.await;
        });
    }

    /// Waits up to `limit` for spawned tasks to finish. Tasks that are only waiting out a delay
    /// ([`App::delete_later`]) are told to stop waiting.
    pub async fn drain(&self, limit: Duration) {
        self.shutdown.cancel();
        self.tasks.close();
        let _ = tokio::time::timeout(limit, self.tasks.wait()).await;
    }

    pub async fn guild_settings(&self, guild_id: u64) -> anyhow::Result<Arc<GuildSettings>> {
        self.guild_settings
            .try_get_with(guild_id, async { self.db.guild_settings(guild_id).await.map(Arc::new) })
            .await
            .map_err(|e| anyhow::anyhow!("{e:#}"))
    }

    /// A server's XP multipliers, cached for a minute like its settings.
    pub async fn xp_rules(&self, guild_id: u64) -> anyhow::Result<Arc<XpRules>> {
        self.xp_rules
            .try_get_with(guild_id, async { self.db.xp_rules(guild_id).await.map(Arc::new) })
            .await
            .map_err(|e| anyhow::anyhow!("{e:#}"))
    }

    /// Read the authoritative policy so curve changes are consistent across pods.
    pub async fn leveling_policy(&self, guild_id: u64) -> anyhow::Result<Arc<LevelingPolicy>> {
        self.db.leveling_policy(guild_id).await.map(Arc::new)
    }

    pub async fn forget_xp_rules(&self, guild_id: u64) {
        self.xp_rules.invalidate(&guild_id).await;
        self.voice_guilds.invalidate(&guild_id).await;
    }

    /// A server's settings changed here: forgets this pod's copy and tells every other pod through Redis to
    /// forget theirs, so the change counts from their next message (and voice minute) instead of after their
    /// caches run out. A failed announcement is only logged: the caches run out within minutes.
    pub async fn announce_guild_change(&self, guild_id: u64) {
        self.forget_guild_settings(guild_id).await;
        self.publish(pinchora_cache::Invalidation::Guild(guild_id)).await;
    }

    /// [`App::announce_guild_change`] for a server's XP multipliers.
    pub async fn announce_xp_change(&self, guild_id: u64) {
        self.forget_xp_rules(guild_id).await;
        self.publish(pinchora_cache::Invalidation::XpRules(guild_id)).await;
    }

    async fn publish(&self, what: pinchora_cache::Invalidation) {
        if let Err(error) = self.cache.publish_invalidation(what).await {
            tracing::warn!(%error, ?what, "invalidation not published; other pods catch up as their caches run out");
        }
    }

    /// The settings and multipliers voice XP is paid by, cached for five minutes.
    pub(crate) async fn voice_guild(&self, guild_id: u64) -> anyhow::Result<Arc<VoiceGuild>> {
        self.voice_guilds
            .try_get_with(guild_id, async {
                let settings = self.db.guild_settings(guild_id).await?;
                let rules = self.db.xp_rules(guild_id).await?;
                anyhow::Ok(Arc::new(VoiceGuild {
                    settings: Arc::new(settings),
                    rules: Arc::new(rules),
                }))
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e:#}"))
    }

    pub(crate) async fn forget_voice_guild(&self, guild_id: u64) {
        self.voice_guilds.invalidate(&guild_id).await;
    }

    /// `channel` followed by what it sits under, nearest first: a thread or forum post's channel, then
    /// that channel's category. Only looked up as far as `rules` needs: nothing when the server has no
    /// channel rules or the channel has its own. Discord is asked once per channel and the answer
    /// kept; a lookup that fails just leaves the lineage short.
    pub async fn xp_lineage(&self, rules: &XpRules, channel: u64) -> Vec<u64> {
        self.lineage(channel, rules.channels.is_empty(), |id| {
            rules.channels.contains_key(&id)
        })
        .await
    }

    /// `channel` followed by everything it sits under (a thread's channel, a channel's category), for
    /// the server log's channel ignore list.
    pub(crate) async fn place_lineage(&self, channel: u64) -> Vec<u64> {
        self.lineage(channel, false, |_| false).await
    }

    /// Complete ancestry for exclusion checks; an unknown parent fails closed.
    pub(crate) async fn automod_lineage(&self, channel: u64) -> Option<Vec<u64>> {
        let mut ids = vec![channel];
        let place = self.channel_place(channel).await?;
        if let Some(parent) = place.parent {
            ids.push(parent);
            if place.thread {
                let parent_place = self.channel_place(parent).await?;
                if let Some(category) = parent_place.parent {
                    ids.push(category);
                }
            }
        }
        Some(ids)
    }

    pub async fn automod_config(&self, guild: u64) -> anyhow::Result<Arc<AutoModConfig>> {
        self.automod_configs
            .try_get_with(guild, async { self.db.automod_config(guild).await.map(Arc::new) })
            .await
            .map_err(|e| anyhow::anyhow!("{e:#}"))
    }

    pub async fn forget_automod_config(&self, guild: u64) {
        self.automod_configs.invalidate(&guild).await;
    }

    /// [`App::xp_lineage`] for any rule: `none` says the server has no channel rules at all, `has_rule`
    /// whether a channel has its own.
    pub(crate) async fn lineage(&self, channel: u64, none: bool, has_rule: impl Fn(u64) -> bool) -> Vec<u64> {
        let mut lineage = vec![channel];
        if none || has_rule(channel) {
            return lineage;
        }
        let Some(place) = self.channel_place(channel).await else {
            return lineage;
        };
        let Some(parent) = place.parent else {
            return lineage;
        };
        lineage.push(parent);
        if place.thread
            && !has_rule(parent)
            && let Some(category) = self.channel_place(parent).await.and_then(|p| p.parent)
        {
            lineage.push(category);
        }
        lineage
    }

    async fn channel_place(&self, id: u64) -> Option<ChannelPlace> {
        if let Some(place) = self.channel_places.get(&id).await {
            return Some(place);
        }
        // A channel Discord could not tell us about a moment ago is not asked about again on every message.
        if self.channel_misses.contains_key(&id) {
            return None;
        }
        let asked = self
            .channel_places
            .try_get_with(id, async {
                let channel = self.http.channel(Id::new(id.max(1))).await?.model().await?;
                anyhow::Ok(ChannelPlace {
                    parent: channel.parent_id.map(|p| p.get()),
                    thread: channel.kind.is_thread(),
                })
            })
            .await;
        match asked {
            Ok(place) => Some(place),
            Err(error) => {
                tracing::debug!(%error, id, "channel lookup failed");
                self.channel_misses.insert(id, ()).await;
                None
            }
        }
    }

    /// Remembers where channels and threads sit, as GUILD_CREATE lists them, so Discord is never asked
    /// about them.
    pub(crate) async fn prefill_channel_places(&self, places: Vec<(u64, ChannelPlace)>) {
        for (id, place) in places {
            self.channel_places.insert(id, place).await;
            self.channel_misses.invalidate(&id).await;
        }
    }

    /// Whether `id`'s place is known without asking Discord, for tests.
    pub fn knows_channel_place(&self, id: u64) -> bool {
        self.channel_places.contains_key(&id)
    }

    /// A server's role names by id, cached for a minute; `None` when Discord cannot be asked.
    pub async fn guild_roles(&self, guild_id: u64) -> Option<Arc<HashMap<u64, String>>> {
        self.guild_roles
            .try_get_with(guild_id, async {
                let roles = self.http.roles(Id::new(guild_id.max(1))).await?.models().await?;
                anyhow::Ok(Arc::new(roles.into_iter().map(|r| (r.id.get(), r.name)).collect()))
            })
            .await
            .map_err(|error| tracing::debug!(%error, guild_id, "role lookup failed"))
            .ok()
    }

    /// A server's owner and roles, cached for a minute; asked for by text commands only.
    pub(crate) async fn permission_facts(&self, guild_id: u64) -> anyhow::Result<Arc<PermissionFacts>> {
        self.permission_facts
            .try_get_with(guild_id, async {
                let id = Id::new(guild_id.max(1));
                let guild = self.http.guild(id).await?.model().await?;
                let roles = self.http.roles(id).await?.models().await?;
                anyhow::Ok(Arc::new(PermissionFacts {
                    owner: guild.owner_id.get(),
                    roles,
                }))
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e:#}"))
    }

    /// Forgets a server's cached owner and roles, when either changed.
    pub async fn forget_permission_facts(&self, guild_id: u64) {
        self.permission_facts.invalidate(&guild_id).await;
    }

    /// Where command permissions are looked up in a channel and which overwrites apply there, cached
    /// for a minute. Fails when Discord cannot say, which a text command answers by refusing.
    pub(crate) async fn channel_rules(&self, channel: u64) -> anyhow::Result<Arc<ChannelRules>> {
        if self.channel_misses.contains_key(&channel) {
            anyhow::bail!("channel lookup recently failed");
        }
        let result = self
            .channel_rules
            .try_get_with(channel, async {
                let response = match self.http.channel(Id::new(channel.max(1))).await {
                    Ok(response) => response,
                    Err(error) => {
                        if crate::roles::roles::error_code(&error) == Some(10003) {
                            self.channel_misses.insert(channel, ()).await;
                        }
                        return Err(error.into());
                    }
                };
                let found = response.model().await?;
                self.channel_places
                    .insert(
                        channel,
                        ChannelPlace {
                            parent: found.parent_id.map(|id| id.get()),
                            thread: found.kind.is_thread(),
                        },
                    )
                    .await;
                let mut lineage = vec![channel];
                let mut overwrites = found.permission_overwrites.unwrap_or_default();
                if found.kind.is_thread()
                    && let Some(parent) = found.parent_id
                {
                    lineage.push(parent.get());
                    let parent = self.http.channel(parent).await?.model().await?;
                    overwrites = parent.permission_overwrites.unwrap_or_default();
                }
                anyhow::Ok(Arc::new(ChannelRules { lineage, overwrites }))
            })
            .await;
        result.map_err(|error| anyhow::anyhow!("{error:#}"))
    }

    /// Forgets what a channel's permissions were, when they changed.
    pub async fn forget_channel_rules(&self, channel: u64) {
        self.channel_rules.invalidate(&channel).await;
        self.channel_misses.invalidate(&channel).await;
    }

    /// What a server set for the bot's commands under Server Settings, Integrations, cached for five
    /// minutes. A server that cannot have any (Discord says the application is not there, as for a bot
    /// added without the commands scope) has none; any other failure is an error, which a text
    /// command answers by refusing rather than guessing.
    pub(crate) async fn command_overrides(&self, guild_id: u64) -> anyhow::Result<Arc<CommandOverrides>> {
        self.command_overrides
            .try_get_with(guild_id, async {
                let entries = match self
                    .interaction()
                    .guild_command_permissions(Id::new(guild_id.max(1)))
                    .await
                {
                    Ok(response) => response.models().await?,
                    Err(error) if is_missing(&error) => Vec::new(),
                    Err(error) => return Err(error.into()),
                };
                if entries.is_empty() {
                    return anyhow::Ok(Arc::new(CommandOverrides::default()));
                }
                let ids = self.command_ids().await?;
                anyhow::Ok(Arc::new(CommandOverrides::new(
                    entries,
                    self.application_id.get(),
                    &ids,
                )))
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e:#}"))
    }

    /// Registration invalidates both caches together, on every serving process.
    pub async fn forget_command_registration(&self) {
        self.command_ids.invalidate_all();
        self.command_overrides.invalidate_all();
        self.saved_overrides.invalidate_all();
    }

    pub(crate) async fn saved_command_overrides(&self, guild: u64) -> anyhow::Result<Option<Arc<CommandOverrides>>> {
        self.saved_overrides
            .try_get_with(guild, async {
                let Some(value) = self
                    .db
                    .saved_command_permissions(guild, self.application_id.get())
                    .await?
                else {
                    return anyhow::Ok(None);
                };
                let entries = serde_json::from_value(value["entries"].clone())?;
                let ids = serde_json::from_value(value["ids"].clone())?;
                Ok(Some(Arc::new(CommandOverrides::new(
                    entries,
                    self.application_id.get(),
                    &ids,
                ))))
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e:#}"))
    }

    /// Forgets a server's command overrides, when they changed.
    pub async fn forget_command_overrides(&self, guild_id: u64) {
        self.command_overrides.invalidate(&guild_id).await;
        self.command_ids.invalidate_all();
    }

    /// The ids of the bot's commands by name, which tell the entries of Discord's command permissions
    /// apart; cached for ten minutes.
    async fn command_ids(&self) -> anyhow::Result<Arc<HashMap<String, u64>>> {
        self.command_ids
            .try_get_with((), async {
                let commands = match self.interaction().global_commands().await {
                    Ok(response) => response.models().await?,
                    Err(error) if is_missing(&error) => Vec::new(),
                    Err(error) => return Err(error.into()),
                };
                anyhow::Ok(Arc::new(
                    commands
                        .into_iter()
                        .filter_map(|c| Some((c.name, c.id?.get())))
                        .collect(),
                ))
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e:#}"))
    }

    /// Counts progress on today's quest of `kind`, if the player has that quest today.
    /// Failures are logged and swallowed: quest progress never breaks a command.
    pub async fn quest(&self, user: u64, kind: pinchora_game::quests::QuestKind) {
        let day = chrono::Utc::now().timestamp().div_euclid(86_400);
        if !pinchora_game::quests::for_day(user, day).iter().any(|q| q.kind == kind) {
            return;
        }
        let key = format!("quest:{day}:{user}:{}", kind.key());
        if let Err(error) = self.cache.incr(&key, Duration::from_secs(2 * 86_400)).await {
            tracing::debug!(%error, "quest progress not saved");
        }
    }

    /// What a server switched off, and where: its module rules, cached for a minute like its settings.
    pub async fn guild_modules(&self, guild_id: u64) -> anyhow::Result<Arc<GuildModules>> {
        self.module_rules
            .try_get_with(guild_id, async { self.db.guild_modules(guild_id).await.map(Arc::new) })
            .await
            .map_err(|e| anyhow::anyhow!("{e:#}"))
    }

    /// Command policies share the guild invalidation bus and a bounded fallback TTL.
    pub async fn command_control(
        &self,
        guild: u64,
    ) -> anyhow::Result<Arc<pinchora_core::admin::command_control::Config>> {
        self.command_control
            .try_get_with(guild, async { self.db.command_control(guild).await.map(Arc::new) })
            .await
            .map_err(|e| anyhow::anyhow!("{e:#}"))
    }

    /// Whether `module` may act in `channel` of a server, and if not, why. A thread follows its
    /// channel and a channel its category, like XP multipliers; a place that cannot be found (`None`,
    /// or a lookup that failed) is not on any list.
    pub async fn module_access(&self, guild_id: u64, channel: Option<u64>, module: Module) -> anyhow::Result<Access> {
        let rule = self.guild_modules(guild_id).await?.rule(module);
        if !rule.needs_lineage() {
            return Ok(rule.access(&[]));
        }
        let lineage = match channel {
            Some(channel) => self.lineage(channel, false, |id| rule.names(id)).await,
            None => Vec::new(),
        };
        Ok(rule.access(&lineage))
    }

    /// [`App::module_access`] for events, which have nobody to answer: allowed unless the server
    /// switched the module off here. A lookup that fails reads as allowed, so a database hiccup
    /// never silences a server.
    pub async fn module_allowed(&self, guild_id: u64, channel: Option<u64>, module: Module) -> bool {
        match self.module_access(guild_id, channel, module).await {
            Ok(access) => access == Access::Allowed,
            Err(error) => {
                tracing::warn!(%error, guild = guild_id, module = module.key(), "module rules unavailable");
                true
            }
        }
    }

    /// Counts something the player did for their `/wrapped`. Failures are logged and swallowed: the
    /// numbers never break a command.
    pub async fn count(&self, user: u64, counter: pinchora_db::Counter, n: i64) {
        if let Err(error) = self.db.bump_activity(user, counter, n).await {
            tracing::debug!(%error, user, "activity not counted");
        }
    }

    pub async fn forget_guild_settings(&self, guild_id: u64) {
        self.antinuke_enabled.invalidate(&guild_id).await;
        self.branding_colors.invalidate(&guild_id).await;
        self.forget_automod_config(guild_id).await;
        self.guild_settings.invalidate(&guild_id).await;
        self.log_state.forget_config(guild_id).await;
        self.module_rules.invalidate(&guild_id).await;
        self.command_control.invalidate(&guild_id).await;
        self.voice_guilds.invalidate(&guild_id).await;
        self.custom.forget(guild_id).await;
    }

    /// Runs a render job on the blocking pool, at most one per CPU at a time.
    pub async fn render<T, F>(&self, job: F) -> anyhow::Result<T>
    where
        T: Send + 'static,
        F: FnOnce(&Renderer) -> anyhow::Result<T> + Send + 'static,
    {
        self.render_holding(None, job).await
    }

    /// [`App::render`] for the dashboard's previews: they share a few preview slots, so however many are asked
    /// for, at most that many wait among the member events' cards for a render slot; one that waits too long
    /// for its turn, or takes too long to draw, gives up with an error the dashboard can show.
    pub async fn render_preview<T, F>(&self, job: F) -> Result<T, PreviewError>
    where
        T: Send + 'static,
        F: FnOnce(&Renderer) -> anyhow::Result<T> + Send + 'static,
    {
        let turn = match tokio::time::timeout(PREVIEW_WAIT, self.preview_slots.clone().acquire_owned()).await {
            Ok(turn) => turn.map_err(anyhow::Error::from)?,
            Err(_) => {
                metrics::counter!("pinchora_preview_refused_total", "why" => "busy").increment(1);
                return Err(PreviewError::Busy);
            }
        };
        match tokio::time::timeout(PREVIEW_BUDGET, self.render_holding(Some(turn), job)).await {
            Ok(drawn) => Ok(drawn?),
            Err(_) => {
                metrics::counter!("pinchora_preview_refused_total", "why" => "slow").increment(1);
                Err(PreviewError::TooSlow)
            }
        }
    }

    /// The slots (`turn`, if any, and a render slot) go with the job to the blocking pool and are given back
    /// when it ends, even if whoever waits for it has given up.
    async fn render_holding<T, F>(&self, turn: Option<OwnedSemaphorePermit>, job: F) -> anyhow::Result<T>
    where
        T: Send + 'static,
        F: FnOnce(&Renderer) -> anyhow::Result<T> + Send + 'static,
    {
        let slot = self.render_slots.clone().acquire_owned().await?;
        let renderer = self.renderer.clone();
        let started = std::time::Instant::now();
        let out = tokio::task::spawn_blocking(move || {
            let out = job(&renderer);
            drop((turn, slot));
            out
        })
        .await??;
        metrics::histogram!("pinchora_render_seconds").record(started.elapsed().as_secs_f64());
        Ok(out)
    }

    /// A user's name and avatar as Discord has them, cached for ten minutes; `None` when
    /// Discord cannot find them (deleted accounts) or is unreachable.
    pub async fn user(&self, id: u64) -> Option<Arc<User>> {
        self.users
            .try_get_with(id, async {
                let user = self.http.user(Id::new(id.max(1))).await?.model().await?;
                anyhow::Ok(Arc::new(user))
            })
            .await
            .map_err(|error| tracing::debug!(%error, id, "user lookup failed"))
            .ok()
    }

    /// The user's avatar as a `size`² PNG, pixelated to `grid` when given. Falls back
    /// to a generated pixel creature when the CDN fails.
    pub async fn avatar(&self, user: &User, size: u32, grid: Option<u32>) -> Arc<Vec<u8>> {
        self.avatar_of(user.id, user.avatar, size, grid).await
    }

    /// [`App::avatar`] by id and avatar hash, for callers without a [`User`] (the dashboard).
    pub async fn avatar_of(
        &self,
        id: Id<UserMarker>,
        hash: Option<ImageHash>,
        size: u32,
        grid: Option<u32>,
    ) -> Arc<Vec<u8>> {
        let url = match hash {
            Some(hash) => format!("https://cdn.discordapp.com/avatars/{id}/{hash}.png?size=256"),
            None => format!("https://cdn.discordapp.com/embed/avatars/{}.png", (id.get() >> 22) % 6),
        };
        let key = format!("{url}|{size}|{grid:?}");
        let seed = id.get();
        self.avatars
            .get_with(key, async move {
                let processed = async {
                    let bytes = self.web.get(&url).send().await?.error_for_status()?.bytes().await?;
                    let png = self.render(move |_| prepare_avatar(&bytes, size, grid)).await?;
                    anyhow::Ok(png)
                }
                .await;
                Arc::new(processed.unwrap_or_else(|error| {
                    tracing::debug!(%error, "avatar fetch failed, using placeholder");
                    placeholder_avatar(seed, size)
                }))
            })
            .await
    }
}
