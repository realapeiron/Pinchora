use std::{collections::HashMap, future::Future, sync::Arc, time::Duration};

use moka::future::Cache as Moka;
use pinchora_cache::Cache;
use pinchora_db::{Db, GuildSettings};
use pinchora_render::{Renderer, placeholder_avatar, prepare_avatar};
use tokio::sync::Semaphore;
use tokio_util::task::TaskTracker;
use twilight_http::{Client, client::InteractionClient};
use twilight_model::{
    id::{Id, marker::ApplicationMarker},
    user::User,
};

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
    avatars: Moka<String, Arc<Vec<u8>>>,
    users: Moka<u64, Arc<User>>,
    tasks: TaskTracker,
    event_slots: Arc<Semaphore>,
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
            avatars: Moka::builder()
                .weigher(|_k: &String, v: &Arc<Vec<u8>>| v.len().try_into().unwrap_or(u32::MAX))
                .max_capacity(64 * 1024 * 1024)
                .time_to_live(Duration::from_secs(600))
                .build(),
            users: Moka::builder()
                .max_capacity(10_000)
                .time_to_live(Duration::from_secs(600))
                .build(),
            tasks: TaskTracker::new(),
            event_slots: Arc::new(Semaphore::new(1024)),
        }
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

    pub async fn forget_guild_settings(&self, guild_id: u64) {
        self.guild_settings.invalidate(&guild_id).await;
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
        let url = match user.avatar {
            Some(hash) => format!("https://cdn.discordapp.com/avatars/{}/{hash}.png?size=256", user.id),
            None => format!(
                "https://cdn.discordapp.com/embed/avatars/{}.png",
                (user.id.get() >> 22) % 6
            ),
        };
        let key = format!("{url}|{size}|{grid:?}");
        let seed = user.id.get();
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
