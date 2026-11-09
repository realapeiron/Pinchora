//! Shared five-minute Minecraft and patch polling. One Redis lease covers every pod.
use std::{sync::Arc, time::Duration};

use pinchora_core::{modules::Module, t, trackers};
use twilight_model::id::Id;
use twilight_util::builder::embed::EmbedBuilder;

use crate::App;

const MANIFEST: &str = "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json";
const LAUNCHER_NEWS: &str = "https://launchercontent.mojang.com/v2/news.json";

/// No redirects or ambient proxies, particularly when carrying Riot credentials.
pub(crate) fn http_client() -> anyhow::Result<reqwest::Client> {
    Ok(reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .no_proxy()
        .timeout(Duration::from_secs(5))
        .build()?)
}

pub(crate) async fn bounded_body(mut response: reqwest::Response, cap: usize) -> anyhow::Result<Vec<u8>> {
    anyhow::ensure!(
        response.content_length().is_none_or(|n| n <= cap as u64),
        "response too large"
    );
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await? {
        anyhow::ensure!(chunk.len() <= cap - bytes.len(), "response too large");
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

pub(crate) async fn mc_status(app: &App, input: &str) -> anyhow::Result<trackers::McStatus> {
    let (host, port) = trackers::parse_host(input)?;
    let key = format!("minecraft:status:{host}:{port}");
    if let Some(value) = app.db.tracker_cached(&key, 300).await? {
        return serde_json::from_value(value).map_err(Into::into);
    }
    // Cache failures too; one unreachable host watched by many guilds gets one probe.
    anyhow::ensure!(
        app.cache.try_cooldown(&key, Duration::from_secs(300)).await?.is_none(),
        "ping pending"
    );
    let result = trackers::ping(input).await;
    let value = result
        .as_ref()
        .ok()
        .map(serde_json::to_value)
        .transpose()?
        .unwrap_or(serde_json::Value::Null);
    app.db.tracker_cache_put(&key, &value).await?;
    result
}

fn plain_text(input: &str, limit: usize) -> String {
    input
        .chars()
        .filter(|c| {
            (!c.is_control() || *c == '\n')
                && !matches!(c,
        '<' | '>' | '&' | '[' | ']' | '(' | ')' | '*' | '_' | '~' | '`' | '@' | '\\'
        | '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}')
        })
        .take(limit)
        .collect()
}

pub async fn manifest(app: &App) -> anyhow::Result<serde_json::Value> {
    if let Some(cached) = app.db.tracker_cached("minecraft:manifest", 300).await? {
        return Ok(cached);
    }
    let bytes = bounded_body(
        http_client()?.get(MANIFEST).send().await?.error_for_status()?,
        1024 * 1024,
    )
    .await?;
    let value = serde_json::from_slice(&bytes)?;
    app.db.tracker_cache_put("minecraft:manifest", &value).await?;
    Ok(value)
}

pub async fn launcher_news(app: &App) -> anyhow::Result<serde_json::Value> {
    if let Some(cached) = app.db.tracker_cached("minecraft:news", 300).await? {
        return Ok(cached);
    }
    let bytes = bounded_body(
        http_client()?.get(LAUNCHER_NEWS).send().await?.error_for_status()?,
        1024 * 1024,
    )
    .await?;
    let value = serde_json::from_slice(&bytes)?;
    app.db.tracker_cache_put("minecraft:news", &value).await?;
    Ok(value)
}

pub fn java_news(value: &serde_json::Value) -> Vec<(String, String)> {
    value["entries"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|entry| {
            entry["newsType"]
                .as_array()
                .is_some_and(|tags| tags.iter().any(|tag| tag == "Java"))
        })
        .filter_map(|entry| {
            let title = plain_text(entry["title"].as_str()?, 90);
            let link = entry["readMoreLink"].as_str()?;
            let url = reqwest::Url::parse(link).ok()?;
            (url.scheme() == "https"
                && url.username().is_empty()
                && url.password().is_none()
                && url.port().is_none()
                && link.len() <= 2048
                && matches!(url.host_str(), Some("www.minecraft.net" | "minecraft.net")))
            .then_some((title, link.to_owned()))
        })
        .take(3)
        .collect()
}

/// The first official article whose path is a patch note, without accepting external links.
pub fn patch_link(html: &str, domain: &str) -> Option<String> {
    for chunk in html.split("href=\"").skip(1) {
        let href = chunk.split('"').next()?;
        let path = if let Some(path) = href.strip_prefix(&format!("https://{domain}")) {
            path
        } else {
            href
        };
        if path.strip_prefix("/en-us/news/game-updates/").is_some_and(|slug| {
            let slug = slug.trim_end_matches('/');
            !slug.is_empty()
                && slug.len() <= 180
                && slug.contains("patch")
                && slug.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
        }) {
            return Some(format!("https://{domain}{path}"));
        }
    }
    None
}

async fn post(
    app: &App,
    guild: u64,
    channel: u64,
    title: &str,
    description: &str,
    url: Option<&str>,
    previous: Option<u64>,
) -> anyhow::Result<Option<u64>> {
    let mut embed = EmbedBuilder::new()
        .title(plain_text(title, 256))
        .description(plain_text(description, 4096))
        .color(0x53c7c0);
    if let Some(url) = url {
        embed = embed.url(url);
    }
    let embed = app.brand_embed(guild, embed.build()).await;
    let body = serde_json::to_vec(&serde_json::json!({"embeds":[embed],"allowed_mentions":{"parse":[]}}))?;
    if let Some(message) = previous {
        match app
            .http
            .update_message(Id::new(channel), Id::new(message))
            .payload_json(&body)
            .await
        {
            Ok(_) => return Ok(None),
            Err(error) => {
                // A timeout/403/429 is not evidence that the old message is gone.
                if !matches!(error.kind(), twilight_http::error::ErrorType::Response { status, .. } if status.get() == 404)
                {
                    return Err(error.into());
                }
            }
        }
    }
    let response = app.http.create_message(Id::new(channel)).payload_json(&body).await?;
    Ok(Some(response.model().await?.id.get()))
}

pub async fn tick(app: &Arc<App>) -> anyhow::Result<usize> {
    if app
        .cache
        .try_cooldown("trackers:poll", Duration::from_secs(300))
        .await?
        .is_some()
    {
        return Ok(0);
    }
    // A stalled pass cannot outlive its shared lease and overlap the next pod.
    tokio::time::timeout(Duration::from_secs(240), tick_leased(app)).await?
}

async fn tick_leased(app: &Arc<App>) -> anyhow::Result<usize> {
    let settings = app.db.tracker_settings_all().await?;
    app.db.tracker_prune().await?;
    let mut done = 0;
    for config in &settings {
        let guild = &config.guild_id;
        if !app.module_allowed(*guild as u64, None, Module::GameTrackers).await {
            continue;
        }
        for item in config.mc_watch.as_array().into_iter().flatten().take(20) {
            let (Some(host), Some(channel)) = (
                item["host"].as_str(),
                item["channel_id"].as_str().and_then(|s| s.parse::<u64>().ok()),
            ) else {
                continue;
            };
            let status = match mc_status(app, host).await {
                Ok(status) => status,
                Err(error) => {
                    tracing::debug!(%error,host,"Minecraft watch ping failed");
                    continue;
                }
            };
            let lang = app.guild_lang(*guild as u64).await;
            let title = format!("{} · {host}", t!(lang, "trackers.minecraft"));
            let detail = format!(
                "{}: {}\n{}: {} / {}\n{}",
                t!(lang, "trackers.version"),
                status.version,
                t!(lang, "trackers.players"),
                status.online,
                status.max,
                status.motd
            );
            let previous = item["message_id"].as_str().and_then(|s| s.parse().ok());
            match post(app, *guild as u64, channel, &title, &detail, None, previous).await {
                Ok(Some(message)) => {
                    app.db
                        .tracker_watch_message(*guild as u64, host, channel, message)
                        .await?;
                    done += 1;
                }
                Ok(None) => done += 1,
                Err(error) => tracing::warn!(%error,guild,host,"Minecraft watch post failed"),
            }
        }
    }
    // Minecraft Java releases come from Mojang's manifest; Riot patch links are from their own news pages.
    for (game, item) in [
        (
            "mc",
            manifest(app).await.ok().and_then(|v| {
                trackers::release_ids(&v).into_iter().next().map(|id| {
                    (
                        id.clone(),
                        format!("Minecraft Java {id}"),
                        "https://www.minecraft.net/en-us/articles".to_owned(),
                    )
                })
            }),
        ),
        (
            "mc-news",
            launcher_news(app).await.ok().and_then(|v| {
                let (title, url) = java_news(&v).into_iter().next()?;
                let id = v["entries"]
                    .as_array()?
                    .iter()
                    .find(|entry| entry["readMoreLink"].as_str() == Some(url.as_str()))?["id"]
                    .as_str()?
                    .to_owned();
                (id.len() <= 256 && !id.is_empty()).then_some((id, title, url))
            }),
        ),
        ("lol", official_patch("www.leagueoflegends.com").await),
        ("valorant", official_patch("playvalorant.com").await),
    ] {
        let Some((id, title, url)) = item else {
            continue;
        };
        if app.db.patch_is_seen(game, &id).await? {
            continue;
        }
        for config in &settings {
            let guild = &config.guild_id;
            if !app.module_allowed(*guild as u64, None, Module::GameTrackers).await {
                continue;
            }
            let channel = if game == "mc-news" {
                config.mc_news_channel_id.map(|id| id as u64)
            } else {
                config.patch_channels[game].as_str().and_then(|s| s.parse::<u64>().ok())
            };
            let Some(channel) = channel else {
                continue;
            };
            // Persist each destination before dispatch. Discord has no durable idempotency
            // key: after an ambiguous send/crash we prefer a missed post to duplicate spam.
            if !app
                .db
                .patch_mark_seen(&format!("{game}:{guild}:{channel}"), &id)
                .await?
            {
                continue;
            }
            let lang = app.guild_lang(*guild as u64).await;
            match post(
                app,
                *guild as u64,
                channel,
                &title,
                &t!(lang, "trackers.new-patch"),
                Some(&url),
                None,
            )
            .await
            {
                Ok(_) => {
                    done += 1;
                }
                Err(error) => tracing::warn!(%error,guild,game,"patch post failed"),
            }
        }
    }
    Ok(done)
}

async fn official_patch(domain: &str) -> Option<(String, String, String)> {
    let url = format!("https://{domain}/en-us/news/tags/patch-notes/");
    let response = http_client()
        .ok()?
        .get(url)
        .send()
        .await
        .ok()?
        .error_for_status()
        .ok()?;
    let bytes = bounded_body(response, 2 * 1024 * 1024).await.ok()?;
    let link = patch_link(std::str::from_utf8(&bytes).ok()?, domain)?;
    let slug = link.trim_end_matches('/').rsplit('/').next()?.to_owned();
    Some((slug.clone(), slug.replace('-', " "), link))
}

pub fn spawn_ticker(app: Arc<App>) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(300));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            interval.tick().await;
            if let Err(error) = tick(&app).await {
                tracing::warn!(%error,"trackers tick failed");
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn patch_links_and_embed_text_reject_injected_markup() {
        for path in [
            "/en-us/news/game-updates/../evil/patch-notes",
            "/en-us/news/game-updates/<svg>/patch-notes",
            "/en-us/news/game-updates/patch-notes?x",
            "/en-us/news/game-updates/patch-notes#x",
        ] {
            assert!(patch_link(&format!("<a href=\"{path}\">"), "playvalorant.com").is_none());
        }
        let text = plain_text("<img> [click](https://evil.test) @everyone\u{202e}", 256);
        assert!(!text.contains(['<', '>', '[', ']', '@', '\u{202e}']));
        assert_eq!(plain_text(&"猫".repeat(1000), 256).chars().count(), 256);
    }

    #[tokio::test]
    async fn http_bodies_are_capped_before_buffering() {
        use axum::{Router, routing::get};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            axum::serve(listener, Router::new().route("/", get(|| async { "x".repeat(2048) })))
                .await
                .unwrap();
        });
        let response = http_client()
            .unwrap()
            .get(format!("http://{addr}/"))
            .send()
            .await
            .unwrap();
        assert!(bounded_body(response, 100).await.is_err());
        server.abort();
    }

    #[tokio::test]
    async fn branding_watch_updates_and_posts_preserve_color_without_duplicate_messages() {
        use axum::{
            Router,
            extract::Request,
            http::{Method, StatusCode},
        };
        use std::sync::atomic::{AtomicU16, AtomicUsize, Ordering};
        let (Ok(database), Ok(redis)) = (
            std::env::var("PINCHORA_TEST_DATABASE_URL"),
            std::env::var("PINCHORA_TEST_REDIS_URL"),
        ) else {
            return;
        };
        crate::fixture_isolation::shared().await;
        let db = pinchora_db::Db::connect_waiting(&database, 2, Duration::from_secs(10))
            .await
            .unwrap();
        db.migrate().await.unwrap();
        let guild = chrono::Utc::now().timestamp_nanos_opt().unwrap() as u64;
        assert!(db.claim_branding_apply(guild, Some(0x31b8d4)).await.unwrap());
        let cache = pinchora_cache::Cache::connect(&redis).await.unwrap();
        let bodies = Arc::new(std::sync::Mutex::new(Vec::<serde_json::Value>::new()));
        let status = Arc::new(AtomicU16::new(403));
        let creates = Arc::new(AtomicUsize::new(0));
        let router = Router::new().fallback({
            let status = status.clone();
            let creates = creates.clone();
            let bodies = bodies.clone();
            move |request: Request| {
                let status = status.clone();
                let creates = creates.clone();
                let bodies = bodies.clone();
                async move {
                    if request.method() == Method::POST {
                        creates.fetch_add(1, Ordering::SeqCst);
                    }
                    let bytes = axum::body::to_bytes(request.into_body(), 16_384).await.unwrap();
                    bodies.lock().unwrap().push(serde_json::from_slice(&bytes).unwrap());
                    (
                        StatusCode::from_u16(status.load(Ordering::SeqCst)).unwrap(),
                        axum::Json(serde_json::json!({"code":10008,"message":"test error"})),
                    )
                }
            }
        });
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            axum::serve(listener, router).await.unwrap();
        });
        let http = Arc::new(
            twilight_http::Client::builder()
                .token("Bot test".into())
                .proxy(address.to_string(), true)
                .ratelimiter(None)
                .build(),
        );
        let app = App::new(http.clone(), http, Id::new(1), db, cache, "tracker-test".into());
        assert!(
            post(&app, guild, 123, "status", "online", None, Some(456))
                .await
                .is_err()
        );
        assert_eq!(creates.load(Ordering::SeqCst), 0, "403 is not a deleted message");
        status.store(404, Ordering::SeqCst);
        assert!(
            post(&app, guild, 123, "status", "online", None, Some(456))
                .await
                .is_err()
        );
        assert_eq!(creates.load(Ordering::SeqCst), 1, "only 404 allows replacement");
        let bodies = bodies.lock().unwrap();
        assert_eq!(bodies.len(), 3, "two edits and one replacement post");
        for body in bodies.iter() {
            assert_eq!(body["embeds"][0]["color"], serde_json::json!(0x31b8d4));
        }
        server.abort();
    }

    #[test]
    fn patch_link_ignores_external_hosts() {
        let html = "<a href=\"https://evil.test/en-us/news/game-updates/fake-patch/\">x</a><a href=\"/en-us/news/game-updates/league-patch-26-21-notes\">real</a>";
        assert_eq!(
            patch_link(html, "www.leagueoflegends.com").unwrap(),
            "https://www.leagueoflegends.com/en-us/news/game-updates/league-patch-26-21-notes"
        );
    }
    #[test]
    fn java_news_only_uses_official_java_articles() {
        let input = serde_json::json!({"entries":[
            {"title":"New Java features","newsType":["Java"],"readMoreLink":"https://www.minecraft.net/article/java-features"},
            {"title":"Bedrock","newsType":["Bedrock"],"readMoreLink":"https://www.minecraft.net/article/bedrock"},
            {"title":"Bad link","newsType":["Java"],"readMoreLink":"https://evil.test/article"}
        ]});
        assert_eq!(
            java_news(&input),
            vec![(
                "New Java features".into(),
                "https://www.minecraft.net/article/java-features".into()
            )]
        );
    }
}
