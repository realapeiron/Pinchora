use std::{
    collections::HashMap,
    future::Future,
    sync::{
        Arc,
        atomic::{AtomicI64, Ordering},
    },
    time::Duration,
};

use chrono::{DateTime, Utc};
use moka::future::Cache as Moka;
use pinchora_cache::Cache;
use pinchora_core::{
    leveling::XpRules,
    modules::{Access, GuildModules, Module},
};
use pinchora_db::{Db, GuildSettings};
use pinchora_render::{Renderer, placeholder_avatar, prepare_avatar};
use tokio::sync::Semaphore;
use tokio_util::task::TaskTracker;
use twilight_http::{Client, client::InteractionClient};
use twilight_model::{
    id::{
        Id,
        marker::{ApplicationMarker, UserMarker},
    },
    user::User,
    util::ImageHash,
};

use crate::{commands::moderation::ModState, custom::CustomState, server_log::LogState};

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

/// What the voice XP ticker needs about a server, kept for minutes so a tick every minute does not ask
/// the database every time.
pub(crate) struct VoiceGuild {
    pub settings: Arc<GuildSettings>,
    pub rules: Arc<XpRules>,
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
    pub web: reqwest::Client,
    pub sprites: Arc<crate::game::scene::Sprites>,
    renderer: Arc<Renderer>,
    render_slots: Arc<Semaphore>,
    guild_settings: Moka<u64, Arc<GuildSettings>>,
    guild_roles: Moka<u64, Arc<HashMap<u64, String>>>,
    xp_rules: Moka<u64, Arc<XpRules>>,
    module_rules: Moka<u64, Arc<GuildModules>>,
    voice_guilds: Moka<u64, Arc<VoiceGuild>>,
    channel_places: Moka<u64, ChannelPlace>,
    channel_misses: Moka<u64, ()>,
    avatars: Moka<String, Arc<Vec<u8>>>,
    users: Moka<u64, Arc<User>>,
    pub(crate) languages: crate::lang::Languages,
    tasks: TaskTracker,
    event_slots: Arc<Semaphore>,
    rebuild_slots: Arc<Semaphore>,
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
            web: reqwest::Client::builder()
                .timeout(Duration::from_secs(5))
                .user_agent("Pinchora (https://github.com/realapeiron/Pinchora)")
                .build()
                .expect("reqwest client"),
            sprites: Arc::new(crate::game::scene::Sprites::default()),
            renderer: Arc::new(Renderer::new()),
            render_slots: Arc::new(Semaphore::new(cpus)),
            // Other pods see a settings change after at most this TTL.
            guild_settings: Moka::builder()
                .max_capacity(50_000)
                .time_to_live(Duration::from_secs(60))
                .build(),
            guild_roles: Moka::builder()
                .max_capacity(10_000)
                .time_to_live(Duration::from_secs(60))
                .build(),
            xp_rules: Moka::builder()
                .max_capacity(50_000)
                .time_to_live(Duration::from_secs(60))
                .build(),
            module_rules: Moka::builder()
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
            languages: crate::lang::Languages::new(),
            tasks: TaskTracker::new(),
            event_slots: Arc::new(Semaphore::new(1024)),
            rebuild_slots: Arc::new(Semaphore::new(REBUILD_SLOTS)),
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

    /// Waits up to `limit` for spawned tasks to finish.
    pub async fn drain(&self, limit: Duration) {
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
        self.guild_settings.invalidate(&guild_id).await;
        self.log_state.forget_config(guild_id).await;
        self.module_rules.invalidate(&guild_id).await;
        self.voice_guilds.invalidate(&guild_id).await;
        self.custom.forget(guild_id).await;
    }

    /// Runs a render job on the blocking pool, at most one per CPU at a time.
    pub async fn render<T, F>(&self, job: F) -> anyhow::Result<T>
    where
        T: Send + 'static,
        F: FnOnce(&Renderer) -> anyhow::Result<T> + Send + 'static,
    {
        let _permit = self.render_slots.acquire().await?;
        let renderer = self.renderer.clone();
        let started = std::time::Instant::now();
        let out = tokio::task::spawn_blocking(move || job(&renderer)).await??;
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
