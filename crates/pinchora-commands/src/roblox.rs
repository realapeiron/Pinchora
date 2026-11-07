//! Fixed-host public Roblox API, profile-code checks, and bounded role reconciliation.
use crate::App;
use anyhow::{Context, ensure};
use pinchora_core::{
    modules::Module,
    roblox::{self, BindKind},
    settings::{self, RoleFacts},
};
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
    users: String,
    groups: String,
    thumbnails: String,
}
impl RobloxApi {
    pub fn new() -> Self {
        let mut api = Self {
            client: reqwest::Client::builder()
                .dns_resolver(crate::feeds::fetch::PublicOnly)
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
        let mut response = request.send().await?;
        ensure!(
            response.status().is_success(),
            "Roblox API returned {}",
            response.status()
        );
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
        Ok((
            user["id"].as_u64().context("missing Roblox id")?,
            user["name"].as_str().context("missing Roblox name")?.to_owned(),
        ))
    }
    pub async fn profile(&self, id: u64) -> anyhow::Result<Value> {
        self.json(self.client.get(format!("{}/v1/users/{id}", self.users)))
            .await
    }
    pub async fn groups(&self, id: u64) -> anyhow::Result<Vec<(u64, u8)>> {
        let value = self
            .json(self.client.get(format!("{}/v2/users/{id}/groups/roles", self.groups)))
            .await?;
        Ok(value["data"]
            .as_array()
            .context("missing groups")?
            .iter()
            .filter_map(|row| {
                Some((
                    row["group"]["id"].as_u64()?,
                    u8::try_from(row["role"]["rank"].as_u64()?).ok()?,
                ))
            })
            .collect())
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
pub async fn sync(app: &App, guild: u64, user: u64) -> anyhow::Result<()> {
    if !app.module_allowed(guild, None, Module::Roblox).await {
        return Ok(());
    }
    let settings = app.db.roblox_settings(guild).await?;
    if !settings.enabled || !app.module_allowed(guild, None, Module::Roblox).await {
        return Ok(());
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
    let link = app.db.roblox_link(user).await?;
    let binds = app.db.roblox_binds(guild).await?;
    let ranks = if let Some(link) = &link
        && binds.iter().any(|b| b.group_id.is_some())
    {
        let key = format!("groups:{}", link.roblox_id);
        if let Some(v) = app.db.roblox_cache(&key).await? {
            serde_json::from_value(v)?
        } else {
            let result = RobloxApi::new().groups(link.roblox_id as u64).await?;
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
        if link.is_none() {
            desired.push(role as u64);
        }
    }
    for bind in binds {
        managed.push(bind.role_id as u64);
        if bind_matches(&bind, link.is_some(), &ranks) {
            desired.push(bind.role_id as u64);
        } else if !bind.remove_if_unmet {
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
        if settings::role_refusal(facts, Some(top), "Roblox bind").is_some()
            || settings::run_time_refusal(facts.permissions, "Roblox bind").is_some()
        {
            continue;
        }
        let has = member.roles.contains(&role.id);
        if desired.contains(&role_id) && !has {
            app.http
                .add_guild_member_role(Id::new(guild), Id::new(user), role.id)
                .reason("Roblox bind")
                .await?;
        }
        if !desired.contains(&role_id) && has {
            app.http
                .remove_guild_member_role(Id::new(guild), Id::new(user), role.id)
                .reason("Roblox bind")
                .await?;
        }
    }
    if let (Some(template), Some(link)) = (&settings.nickname_template, &link) {
        let nick = roblox::nickname(template, &member.user.name, &link.roblox_name);
        if member.nick.as_deref() != Some(&nick) {
            app.http
                .update_guild_member(Id::new(guild), Id::new(user))
                .nick(Some(&nick))
                .await?;
        }
    }
    Ok(())
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
    if app.db.roblox_claim_hold(guild, user).await?
        && let Err(error) = crate::verification::resume_roblox(app, guild, user).await
    {
        app.db.roblox_hold(guild, user).await?;
        return Err(error);
    }
    Ok(())
}

pub async fn tick(app: &Arc<App>) -> anyhow::Result<usize> {
    let due = app.db.roblox_due().await?;
    let count = due.len();
    for (guild, user) in due {
        // At most 20 users/minute across this process; database scheduling prevents a hot loop.
        let result = sync(app, guild as u64, user as u64).await;
        if result.is_ok()
            && app.db.roblox_link(user as u64).await?.is_some()
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
    use super::*;
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
                get(|| async { Json(json!({"description":"pinchora-crab-ab12"})) }),
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
            api.profile(17).await.unwrap()["description"].as_str().unwrap(),
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
                get(|| async { (StatusCode::FOUND, [("location", "http://127.0.0.1:9/private")]) }),
            )
            .route("/v1/users/3", get(|| async { StatusCode::TOO_MANY_REQUESTS }));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let api = RobloxApi::mock(&url, &url);
        assert!(api.profile(1).await.unwrap_err().to_string().contains("too large"));
        assert!(api.profile(2).await.is_err());
        assert!(api.profile(3).await.unwrap_err().to_string().contains("429"));
    }
}
