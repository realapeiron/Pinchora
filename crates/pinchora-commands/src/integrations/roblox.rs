//! Fixed-host public Roblox API, profile-code checks, and bounded role reconciliation.
use crate::App;
use anyhow::{Context, ensure};
use pinchora_core::admin::modules::Module;
use pinchora_core::admin::settings;
use pinchora_core::admin::settings::RoleFacts;
use pinchora_core::integrations::roblox;
use pinchora_core::integrations::roblox::BindKind;
use pinchora_db::{RobloxBind, RobloxSettings};
use serde_json::{Value, json};
use std::{sync::Arc, time::Duration};
use twilight_http::request::AuditLogReason;
use twilight_model::{channel::message::AllowedMentions, id::Id};

const USERS: &str = "https://users.roblox.com";
const GROUPS: &str = "https://groups.roblox.com";
const THUMBNAILS: &str = "https://thumbnails.roblox.com";
const LIMIT: usize = 64 * 1024;

#[cfg(debug_assertions)]
static TEST_ENDPOINT: std::sync::LazyLock<std::sync::Mutex<Option<String>>> =
    std::sync::LazyLock::new(|| std::sync::Mutex::new(None));
/// Local simulator override; never present in release builds.
#[cfg(debug_assertions)]
pub fn install_test_endpoint(url: Option<&str>) {
    *TEST_ENDPOINT.lock().expect("Roblox test endpoint lock") = url.map(str::to_owned);
}
pub struct RobloxApi {
    client: reqwest::Client,
    cache: Option<pinchora_cache::Cache>,
    users: String,
    groups: String,
    thumbnails: String,
}
impl RobloxApi {
    pub fn new() -> Self {
        let mut api = Self {
            cache: None,
            client: reqwest::Client::builder()
                .dns_resolver(crate::community::feeds::fetch::PublicOnly)
                .no_proxy()
                .timeout(Duration::from_secs(8))
                .redirect(reqwest::redirect::Policy::none())
                .build()
                .expect("Roblox HTTP client"),
            users: USERS.into(),
            groups: GROUPS.into(),
            thumbnails: THUMBNAILS.into(),
        };
        #[cfg(debug_assertions)]
        if let Some(url) = TEST_ENDPOINT.lock().expect("Roblox test endpoint lock").clone() {
            api.client = reqwest::Client::builder()
                .timeout(Duration::from_secs(8))
                .redirect(reqwest::redirect::Policy::none())
                .no_proxy()
                .build()
                .expect("Roblox test client");
            api.users = url.clone();
            api.groups = url.clone();
            api.thumbnails = url;
        }
        api
    }
    pub fn with_cache(cache: &pinchora_cache::Cache) -> Self {
        Self {
            cache: Some(cache.clone()),
            ..Self::new()
        }
    }
    #[cfg(test)]
    fn mock(users: &str, groups: &str) -> Self {
        let mut api = Self::new();
        api.client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .unwrap();
        api.users = users.into();
        api.groups = groups.into();
        api.thumbnails = groups.into();
        api
    }
    async fn json(&self, request: reqwest::RequestBuilder) -> anyhow::Result<Value> {
        let request = request.build()?;
        let origin = request.url().origin().ascii_serialization();
        if let Some(cache) = &self.cache {
            let admitted: bool = redis::Script::new(
                r#"
                if redis.call('EXISTS', KEYS[1]) == 1 then return 0 end
                local count = redis.call('INCR', KEYS[2])
                if count == 1 then redis.call('EXPIRE', KEYS[2], 60) end
                return count <= 60 and 1 or 0
            "#,
            )
            .key(format!("pinchora:roblox:backoff:{origin}"))
            .key(format!("pinchora:roblox:requests:{origin}"))
            .invoke_async(&mut cache.conn())
            .await?;
            ensure!(admitted, "Roblox API backoff active");
        }
        let mut response = self.client.execute(request).await?;
        if response.status() == reqwest::StatusCode::TOO_MANY_REQUESTS
            && let Some(cache) = &self.cache
        {
            let seconds = retry_after(
                response
                    .headers()
                    .get(reqwest::header::RETRY_AFTER)
                    .and_then(|v| v.to_str().ok()),
                chrono::Utc::now(),
            );
            let _: i64 = redis::Script::new(
                r#"
                if redis.call('TTL', KEYS[1]) < tonumber(ARGV[1]) then
                    redis.call('SET', KEYS[1], 1, 'EX', ARGV[1])
                end
                return 1
            "#,
            )
            .key(format!("pinchora:roblox:backoff:{origin}"))
            .arg(seconds)
            .invoke_async(&mut cache.conn())
            .await?;
        }
        if !response.status().is_success() {
            return Err(ApiStatus(response.status()).into());
        }
        if let Some(len) = response.content_length() {
            ensure!(len <= LIMIT as u64, "Roblox response too large");
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await? {
            ensure!(bytes.len() + chunk.len() <= LIMIT, "Roblox response too large");
            bytes.extend_from_slice(&chunk);
        }
        Ok(serde_json::from_slice(&bytes)?)
    }
    pub async fn lookup(&self, name: &str) -> anyhow::Result<(u64, String)> {
        ensure!(
            !name.is_empty() && name.len() <= 20 && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_'),
            "Invalid Roblox username"
        );
        let value = self
            .json(
                self.client
                    .post(format!("{}/v1/usernames/users", self.users))
                    .json(&json!({"usernames":[name],"excludeBannedUsers":true})),
            )
            .await?;
        let user = value["data"]
            .as_array()
            .and_then(|a| a.first())
            .context("Roblox username not found")?;
        let id = user["id"].as_u64().context("missing Roblox id")?;
        let canonical = user["name"].as_str().context("missing Roblox name")?;
        ensure!(
            valid_id(id) && valid_username(canonical) && canonical.eq_ignore_ascii_case(name),
            "Roblox lookup identity mismatch"
        );
        Ok((id, canonical.to_owned()))
    }
    pub async fn profile(&self, id: u64) -> anyhow::Result<Option<RobloxProfile>> {
        ensure!(valid_id(id), "Invalid Roblox id");
        let value = match self
            .json(self.client.get(format!("{}/v1/users/{id}", self.users)))
            .await
        {
            Ok(value) => value,
            Err(error)
                if error
                    .downcast_ref::<ApiStatus>()
                    .is_some_and(|e| e.0 == reqwest::StatusCode::NOT_FOUND) =>
            {
                return Ok(None);
            }
            Err(error) => return Err(error),
        };
        let profile: RobloxProfile = serde_json::from_value(value)?;
        ensure!(
            profile.id == id && (profile.is_banned || valid_username(&profile.name)),
            "Roblox profile identity mismatch"
        );
        Ok(Some(profile))
    }
    pub async fn groups(&self, id: u64) -> anyhow::Result<Vec<(u64, u8)>> {
        let value = self
            .json(self.client.get(format!("{}/v2/users/{id}/groups/roles", self.groups)))
            .await?;
        value["data"]
            .as_array()
            .context("missing groups")?
            .iter()
            .map(|row| {
                let id = row["group"]["id"]
                    .as_u64()
                    .filter(|id| valid_id(*id))
                    .context("invalid group id")?;
                let rank = u8::try_from(row["role"]["rank"].as_u64().context("invalid rank")?)?;
                Ok((id, rank))
            })
            .collect::<anyhow::Result<Vec<_>>>()
    }
    pub async fn headshot(&self, id: u64) -> anyhow::Result<Option<String>> {
        let value = self
            .json(self.client.get(format!(
                "{}/v1/users/avatar-headshot?userIds={id}&size=150x150&format=Png&isCircular=false",
                self.thumbnails
            )))
            .await?;
        let Some(url) = value["data"]
            .as_array()
            .and_then(|v| v.first())
            .and_then(|v| v["imageUrl"].as_str())
        else {
            return Ok(None);
        };
        let parsed = reqwest::Url::parse(url)?;
        let host = parsed.host_str().unwrap_or("");
        Ok(
            (parsed.scheme() == "https" && (host == "rbxcdn.com" || host.ends_with(".rbxcdn.com")))
                .then(|| url.to_owned()),
        )
    }
    pub async fn ranks(&self, id: u64) -> anyhow::Result<Value> {
        self.json(self.client.get(format!("{}/v1/groups/{id}/roles", self.groups)))
            .await
    }
}
impl Default for RobloxApi {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RobloxProfile {
    pub id: u64,
    pub name: String,
    pub description: String,
    pub is_banned: bool,
}
#[derive(Debug)]
struct ApiStatus(reqwest::StatusCode);
impl std::fmt::Display for ApiStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Roblox API returned {}", self.0)
    }
}
impl std::error::Error for ApiStatus {}
fn valid_id(id: u64) -> bool {
    id > 0 && i64::try_from(id).is_ok()
}
fn valid_username(name: &str) -> bool {
    !name.is_empty() && name.len() <= 20 && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
}
fn retry_after(raw: Option<&str>, now: chrono::DateTime<chrono::Utc>) -> u64 {
    raw.and_then(|v| {
        v.parse::<u64>().ok().or_else(|| {
            chrono::DateTime::parse_from_rfc2822(v)
                .ok()
                .map(|date| (date.with_timezone(&chrono::Utc) - now).num_seconds().max(1) as u64)
        })
    })
    .unwrap_or(60)
    .clamp(1, 31_536_000)
}

fn bind_matches(bind: &RobloxBind, verified: bool, ranks: &[(u64, u8)]) -> bool {
    let kind = match bind.kind.as_str() {
        "verified" => BindKind::Verified,
        "group_member" => BindKind::GroupMember,
        _ => BindKind::GroupRank,
    };
    roblox::matches(
        kind,
        bind.group_id.map(|v| v as u64),
        bind.rank_min.map(|v| v as u8),
        bind.rank_max.map(|v| v as u8),
        verified,
        ranks,
    )
}

/// Only configured, safe, below-bot roles are granted; unsafe roles are skipped on every update.
pub async fn sync(app: &App, guild: u64, user: u64) -> anyhow::Result<bool> {
    let lease = app
        .cache
        .verification_lease(
            &format!("roblox:user:{user}"),
            &format!("{:032x}", rand::random::<u128>()),
        )
        .await?
        .context("Roblox member update already running")?;
    let result = sync_locked(app, guild, user).await;
    lease.release().await?;
    result
}

async fn sync_locked(app: &App, guild: u64, user: u64) -> anyhow::Result<bool> {
    let settings = app.db.roblox_settings(guild).await?;
    let active = settings.enabled && app.module_allowed(guild, None, Module::Roblox).await;
    if active
        && app
            .db
            .pending_verification(guild, user)
            .await?
            .is_some_and(|p| p.verified_at.is_none())
    {
        return Ok(false);
    }
    let member = app
        .http
        .guild_member(Id::new(guild), Id::new(user))
        .await?
        .model()
        .await?;
    let bot = app
        .http
        .guild_member(Id::new(guild), app.application_id.cast())
        .await?
        .model()
        .await?;
    let roles = app.http.roles(Id::new(guild)).await?.models().await?;
    let top = roles
        .iter()
        .filter(|r| bot.roles.contains(&r.id))
        .map(|r| r.position)
        .max()
        .unwrap_or(0);
    let link = if active && let Some(mut link) = app.db.roblox_link(user).await? {
        match RobloxApi::with_cache(&app.cache).profile(link.roblox_id as u64).await? {
            Some(profile) if !profile.is_banned => {
                app.db
                    .roblox_refresh_name(user, link.roblox_id as u64, &profile.name)
                    .await?;
                link.roblox_name = profile.name;
                Some(link)
            }
            _ => None,
        }
    } else {
        None
    };
    let binds = app.db.roblox_binds(guild).await?;
    let ranks = if let Some(link) = &link
        && binds.iter().any(|b| b.group_id.is_some())
    {
        let key = format!("groups:{}", link.roblox_id);
        if let Some(v) = app.db.roblox_cache(&key).await? {
            serde_json::from_value(v)?
        } else {
            let result = RobloxApi::with_cache(&app.cache).groups(link.roblox_id as u64).await?;
            app.db.roblox_cache_put(&key, &serde_json::to_value(&result)?).await?;
            result
        }
    } else {
        vec![]
    };
    let mut desired = Vec::new();
    let mut managed = Vec::new();
    let mut keep_if_unmet = Vec::new();
    if let Some(role) = settings.verified_role_id {
        managed.push(role as u64);
        if link.is_some() {
            desired.push(role as u64);
        }
    }
    if let Some(role) = settings.unverified_role_id {
        managed.push(role as u64);
        if active && link.is_none() {
            desired.push(role as u64);
        }
    }
    for bind in binds {
        managed.push(bind.role_id as u64);
        if bind_matches(&bind, link.is_some(), &ranks) {
            desired.push(bind.role_id as u64);
        } else if link.is_some() && !bind.remove_if_unmet {
            keep_if_unmet.push(bind.role_id as u64);
        }
    }
    managed.retain(|id| desired.contains(id) || !keep_if_unmet.contains(id));
    managed.sort_unstable();
    managed.dedup();
    for role_id in managed {
        let Some(role) = roles.iter().find(|r| r.id.get() == role_id) else {
            continue;
        };
        let facts = RoleFacts {
            everyone: role_id == guild,
            managed: role.managed,
            position: role.position,
            permissions: role.permissions.bits(),
        };
        if settings::role_refusal(facts, Some(top), "Roblox bind").is_some() {
            continue;
        }
        let has = member.roles.contains(&role.id);
        let should_have =
            desired.contains(&role_id) && settings::run_time_refusal(facts.permissions, "Roblox bind").is_none();
        if should_have && !has {
            app.http
                .add_guild_member_role(Id::new(guild), Id::new(user), role.id)
                .reason("Roblox bind")
                .await?;
        }
        if !should_have && has {
            app.http
                .remove_guild_member_role(Id::new(guild), Id::new(user), role.id)
                .reason("Roblox bind")
                .await?;
        }
    }
    if let (Some(template), Some(link)) = (&settings.nickname_template, &link)
        && app.guild_facts(guild).await.is_some_and(|facts| facts.owner != user)
        && roles
            .iter()
            .filter(|r| member.roles.contains(&r.id))
            .all(|r| r.position < top)
    {
        let nick = roblox::nickname(template, &member.user.name, &link.roblox_name);
        if !nick.is_empty() && member.nick.as_deref() != Some(&nick) {
            app.http
                .update_guild_member(Id::new(guild), Id::new(user))
                .nick(Some(&nick))
                .await?;
        }
    }
    Ok(link.is_some())
}

/// Hold the identity lease through deletion so a concurrent sync cannot restore removed roles.
pub(crate) async fn before_delete(app: &App, user: u64) -> anyhow::Result<pinchora_cache::VerificationLease> {
    let lease = app
        .cache
        .verification_lease(
            &format!("roblox:user:{user}"),
            &format!("{:032x}", rand::random::<u128>()),
        )
        .await?
        .context("Roblox member update already running")?;
    for guild in app.db.roblox_user_guilds(user).await? {
        let guild = guild as u64;
        let member = match app.http.guild_member(Id::new(guild), Id::new(user)).await {
            Ok(response) => response.model().await?,
            Err(error) if matches!(error.kind(), twilight_http::error::ErrorType::Response { status, .. } if status.get() == 404) =>
            {
                continue;
            }
            Err(error) => return Err(error.into()),
        };
        let settings = app.db.roblox_settings(guild).await?;
        let mut ids: Vec<_> = app
            .db
            .roblox_binds(guild)
            .await?
            .into_iter()
            .map(|bind| bind.role_id as u64)
            .collect();
        ids.extend(
            [settings.verified_role_id, settings.unverified_role_id]
                .into_iter()
                .flatten()
                .map(|id| id as u64),
        );
        ids.sort_unstable();
        ids.dedup();
        for role in member.roles.iter().filter(|role| ids.contains(&role.get())) {
            app.http
                .remove_guild_member_role(Id::new(guild), Id::new(user), *role)
                .reason("Roblox privacy cleanup")
                .await?;
        }
    }
    Ok(lease)
}

pub async fn on_join(app: &Arc<App>, guild: u64, user: u64, already_held: bool) -> bool {
    let settings = match app.db.roblox_settings(guild).await {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!(%e,"Roblox settings failed");
            return false;
        }
    };
    if !settings.enabled || !app.module_allowed(guild, None, Module::Roblox).await {
        return false;
    }
    let verified = app.db.roblox_link(user).await.ok().flatten().is_some();
    let queued = if settings.require_verified && !verified && !already_held {
        app.db.roblox_hold(guild, user).await
    } else {
        app.db.roblox_queue(guild, user).await
    };
    if let Err(error) = &queued {
        tracing::warn!(%error,"Roblox sync queue failed");
    }
    if settings.require_verified
        && !verified
        && app
            .cache
            .try_cooldown(&format!("roblox:join:{guild}:{user}"), Duration::from_secs(60))
            .await
            .is_ok_and(|v| v.is_none())
        && app
            .cache
            .try_cooldown(&format!("roblox:join-prompt:{guild}"), Duration::from_secs(3))
            .await
            .is_ok_and(|v| v.is_none())
        && let Some(channel) = settings.verify_channel_id
    {
        let app = app.clone();
        app.clone().spawn(async move {
            let lang = app.guild_lang(guild).await;
            let content = pinchora_core::t!(lang, "roblox.join-prompt", user = user);
            let mentions = AllowedMentions {
                users: vec![Id::new(user)],
                ..Default::default()
            };
            if let Err(error) = app
                .http
                .create_message(Id::new(channel as u64))
                .content(&content)
                .allowed_mentions(Some(&mentions))
                .await
            {
                tracing::warn!(guild,user,%error,"Roblox join prompt failed");
            }
        });
    }
    queued.is_ok() && settings.require_verified && !verified && !already_held
}

pub async fn release_hold(app: &Arc<App>, guild: u64, user: u64) -> anyhow::Result<()> {
    if app
        .db
        .pending_verification(guild, user)
        .await?
        .is_some_and(|p| p.verified_at.is_none())
    {
        return Ok(());
    }
    if app.db.roblox_claim_hold(guild, user).await?
        && let Err(error) = crate::moderation::verification::resume_roblox(app, guild, user).await
    {
        app.db.roblox_hold(guild, user).await?;
        return Err(error);
    }
    Ok(())
}

pub async fn tick(app: &Arc<App>) -> anyhow::Result<usize> {
    let scope = format!("roblox:tick:{}", app.application_id);
    let Some(_lease) = app
        .cache
        .verification_lease(&scope, &format!("{:032x}", rand::random::<u128>()))
        .await?
    else {
        return Ok(0);
    };
    if app.cache.try_cooldown(&scope, Duration::from_secs(60)).await?.is_some() {
        return Ok(0);
    }
    let due = app.db.roblox_due().await?;
    let count = due.len();
    for (guild, user) in due {
        // A renewable fleet lease and shared cooldown admit at most 20 scheduled users/minute.
        let result = sync(app, guild as u64, user as u64).await;
        if result.as_ref().is_ok_and(|verified| *verified)
            && let Err(error) = release_hold(app, guild as u64, user as u64).await
        {
            tracing::warn!(guild,user,%error,"Roblox held member release failed");
        }
        app.db
            .roblox_reschedule(guild as u64, user as u64, if result.is_ok() { 60 } else { 10 })
            .await?;
        if let Err(error) = result {
            tracing::warn!(guild,user,%error,"Roblox scheduled sync failed");
        }
    }
    Ok(count)
}
pub fn spawn_ticker(app: Arc<App>) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(60));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            interval.tick().await;
            if let Err(error) = tick(&app).await {
                tracing::warn!(%error,"Roblox ticker failed");
            }
        }
    });
}

pub fn valid_settings(s: &RobloxSettings) -> bool {
    s.nickname_template.as_deref().is_none_or(roblox::valid_template)
}

#[cfg(test)]
mod tests {
    use crate::integrations::roblox::*;
    #[tokio::test]
    async fn mock_public_api() {
        use axum::{
            Json, Router,
            routing::{get, post},
        };
        let app = Router::new()
            .route(
                "/v1/usernames/users",
                post(|| async { Json(json!({"data":[{"id":17,"name":"Crab"}]})) }),
            )
            .route(
                "/v1/users/17",
                get(|| async {
                    Json(json!({"id":17,"name":"Crab","isBanned":false,"description":"pinchora-crab-ab12"}))
                }),
            )
            .route(
                "/v2/users/17/groups/roles",
                get(|| async { Json(json!({"data":[{"group":{"id":9},"role":{"rank":5}}]})) }),
            )
            .route(
                "/v1/users/avatar-headshot",
                get(|| async { Json(json!({"data":[{"imageUrl":"https://tr.rbxcdn.com/example.png"}]})) }),
            );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        let api = RobloxApi::mock(&url, &url);
        assert_eq!(api.lookup("Crab").await.unwrap().0, 17);
        assert_eq!(api.groups(17).await.unwrap(), vec![(9, 5)]);
        assert_eq!(
            api.headshot(17).await.unwrap().as_deref(),
            Some("https://tr.rbxcdn.com/example.png")
        );
        assert!(roblox::description_has_code(
            &api.profile(17).await.unwrap().unwrap().description,
            "pinchora-crab-ab12"
        ));
    }
    #[tokio::test]
    async fn response_bound_redirect_and_rate_limit() {
        use axum::{Router, http::StatusCode, routing::get};
        let app = Router::new()
            .route("/v1/users/1", get(|| async { vec![b'x'; LIMIT + 1] }))
            .route(
                "/v1/users/2",
                get(|| async { (StatusCode::FOUND, [("location", "/private")]) }),
            )
            .route("/v1/users/3", get(|| async { StatusCode::TOO_MANY_REQUESTS }))
            .route(
                "/private",
                get(|| async { axum::Json(json!({"id":2,"name":"Crab","description":"","isBanned":false})) }),
            );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let api = RobloxApi::mock(&url, &url);
        assert!(api.profile(1).await.unwrap_err().to_string().contains("too large"));
        assert!(api.profile(2).await.is_err());
        assert!(api.profile(3).await.unwrap_err().to_string().contains("429"));
    }
    #[test]
    fn retry_after_supports_seconds_dates_and_invalid_headers() {
        let now = chrono::DateTime::parse_from_rfc2822("Sun, 08 Nov 2026 10:00:00 GMT")
            .unwrap()
            .with_timezone(&chrono::Utc);
        assert_eq!(retry_after(Some("120"), now), 120);
        assert_eq!(retry_after(Some("Sun, 08 Nov 2026 10:02:00 GMT"), now), 120);
        assert_eq!(retry_after(Some("garbage"), now), 60);
        assert_eq!(retry_after(None, now), 60);
        assert_eq!(retry_after(Some("0"), now), 1);
    }

    #[tokio::test]
    async fn malformed_profiles_groups_lookup_and_timeout_fail_closed() {
        use axum::{
            Json, Router,
            routing::{get, post},
        };
        let app = Router::new()
            .route(
                "/v1/users/{id}",
                get(|axum::extract::Path(id): axum::extract::Path<u64>| async move {
                    match id {
                        1 => Json(json!({"id":2,"name":"Crab","description":"code","isBanned":false})),
                        2 => Json(json!({"id":2,"name":"Crab","description":"code"})),
                        3 => Json(json!({"id":3,"name":"Crab","description":"code","isBanned":true})),
                        _ => {
                            tokio::time::sleep(Duration::from_millis(100)).await;
                            Json(json!({}))
                        }
                    }
                }),
            )
            .route(
                "/v2/users/1/groups/roles",
                get(|| async { Json(json!({"data":[{"group":{"id":7},"role":{"rank":256}}]})) }),
            )
            .route("/v2/users/2/groups/roles", get(|| async { "not json" }))
            .route(
                "/v1/usernames/users",
                post(|| async { Json(json!({"data":[{"id":0,"name":"Wrong"}]})) }),
            );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let mut api = RobloxApi::mock(&url, &url);
        api.client = reqwest::Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_millis(50))
            .build()
            .unwrap();
        assert!(api.profile(0).await.is_err());
        assert!(api.profile(u64::MAX).await.is_err());
        assert!(api.profile(1).await.is_err());
        assert!(api.profile(2).await.is_err());
        assert!(api.profile(3).await.unwrap().unwrap().is_banned);
        assert!(api.profile(4).await.is_err());
        assert!(api.groups(1).await.is_err());
        assert!(api.groups(2).await.is_err());
        assert!(api.lookup("Crab").await.is_err());
        for invalid in ["https://evil.test", "123/../456", "", "a?b", "a#b"] {
            assert!(api.lookup(invalid).await.is_err());
        }
    }

    #[tokio::test]
    async fn backoff_and_request_budget_are_shared_between_clients() {
        let Ok(redis_url) = std::env::var("PINCHORA_TEST_REDIS_URL") else {
            return;
        };
        let cache = pinchora_cache::Cache::connect(&redis_url).await.unwrap();
        let hits = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let requests = hits.clone();
        let router = axum::Router::new().route(
            "/v1/users/{id}",
            axum::routing::get(move || {
                requests.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                async { (axum::http::StatusCode::TOO_MANY_REQUESTS, [("retry-after", "120")]) }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
        let mut first = RobloxApi::mock(&url, &url);
        first.cache = Some(cache.clone());
        let mut second = RobloxApi::mock(&url, &url);
        second.cache = Some(cache.clone());
        assert!(first.profile(1).await.unwrap_err().to_string().contains("429"));
        assert!(second.profile(2).await.unwrap_err().to_string().contains("backoff"));
        assert_eq!(hits.load(std::sync::atomic::Ordering::SeqCst), 1);
        let ttl: i64 = redis::cmd("TTL")
            .arg(format!("pinchora:roblox:backoff:{url}"))
            .query_async(&mut cache.conn())
            .await
            .unwrap();
        assert!((115..=120).contains(&ttl));
        let _: i64 = redis::cmd("DEL")
            .arg(format!("pinchora:roblox:backoff:{url}"))
            .query_async(&mut cache.conn())
            .await
            .unwrap();
        let _: String = redis::cmd("SET")
            .arg(format!("pinchora:roblox:requests:{url}"))
            .arg(60)
            .arg("EX")
            .arg(60)
            .query_async(&mut cache.conn())
            .await
            .unwrap();
        assert!(second.profile(2).await.unwrap_err().to_string().contains("backoff"));
        assert_eq!(hits.load(std::sync::atomic::Ordering::SeqCst), 1);
    }
}
