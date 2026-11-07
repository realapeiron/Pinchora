//! Shared five-minute Minecraft and patch polling. One Redis lease covers every pod.
use std::{sync::Arc, time::Duration};

use pinchora_core::{modules::Module, t, trackers};
use twilight_model::id::Id;

use crate::App;

const MANIFEST: &str = "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json";
const LAUNCHER_NEWS: &str = "https://launchercontent.mojang.com/v2/news.json";

pub async fn manifest(app: &App) -> anyhow::Result<serde_json::Value> {
    if let Some(cached) = app.db.tracker_cached("minecraft:manifest", 300).await? {
        return Ok(cached);
    }
    let bytes = app.web.get(MANIFEST).send().await?.error_for_status()?.bytes().await?;
    anyhow::ensure!(bytes.len() <= 1024 * 1024, "manifest too large");
    let value = serde_json::from_slice(&bytes)?;
    app.db.tracker_cache_put("minecraft:manifest", &value).await?;
    Ok(value)
}

pub async fn launcher_news(app: &App) -> anyhow::Result<serde_json::Value> {
    if let Some(cached) = app.db.tracker_cached("minecraft:news", 300).await? {
        return Ok(cached);
    }
    let bytes = app
        .web
        .get(LAUNCHER_NEWS)
        .send()
        .await?
        .error_for_status()?
        .bytes()
        .await?;
    anyhow::ensure!(bytes.len() <= 1024 * 1024, "launcher news too large");
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
            let title = entry["title"].as_str()?.chars().take(90).collect::<String>();
            let link = entry["readMoreLink"].as_str()?;
            let url = reqwest::Url::parse(link).ok()?;
            (url.scheme() == "https" && matches!(url.host_str(), Some("www.minecraft.net" | "minecraft.net")))
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
        if path.starts_with("/en-us/news/game-updates/")
            && path.contains("patch")
            && path
                .trim_end_matches('/')
                .rsplit('/')
                .next()
                .is_some_and(|slug| !slug.is_empty() && slug.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-'))
            && !path.contains(['?', '#', '<', '>'])
        {
            return Some(format!("https://{domain}{path}"));
        }
    }
    None
}

async fn post(
    app: &App,
    channel: u64,
    title: &str,
    description: &str,
    url: Option<&str>,
    previous: Option<u64>,
) -> anyhow::Result<Option<u64>> {
    let mut embed = serde_json::json!({"title":title,"description":description,"color":0x53c7c0});
    if let Some(url) = url {
        embed["url"] = serde_json::json!(url);
    }
    let body = serde_json::to_vec(&serde_json::json!({"embeds":[embed],"allowed_mentions":{"parse":[]}}))?;
    if let Some(message) = previous
        && app
            .http
            .update_message(Id::new(channel), Id::new(message))
            .payload_json(&body)
            .await
            .is_ok()
    {
        return Ok(None);
    }
    let response = app.http.create_message(Id::new(channel)).payload_json(&body).await?;
    Ok(Some(response.model().await?.id.get()))
}

pub async fn tick(app: &Arc<App>) -> anyhow::Result<usize> {
    if app
        .cache
        .try_cooldown("trackers:poll", Duration::from_secs(290))
        .await?
        .is_some()
    {
        return Ok(0);
    }
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
            let status = match trackers::ping(host).await {
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
            match post(app, channel, &title, &detail, None, previous).await {
                Ok(Some(message)) => {
                    app.db.tracker_watch_message(*guild as u64, host, message).await?;
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
                Some((id, title, url))
            }),
        ),
        ("lol", official_patch(app, "www.leagueoflegends.com").await),
        ("valorant", official_patch(app, "playvalorant.com").await),
    ] {
        let Some((id, title, url)) = item else {
            continue;
        };
        if app.db.patch_is_seen(game, &id).await? {
            continue;
        }
        let mut any = false;
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
            let lang = app.guild_lang(*guild as u64).await;
            match post(app, channel, &title, &t!(lang, "trackers.new-patch"), Some(&url), None).await {
                Ok(_) => {
                    any = true;
                    done += 1;
                }
                Err(error) => tracing::warn!(%error,guild,game,"patch post failed"),
            }
        }
        if any {
            app.db.patch_mark_seen(game, &id).await?;
        }
    }
    Ok(done)
}

async fn official_patch(app: &App, domain: &str) -> Option<(String, String, String)> {
    let url = format!("https://{domain}/en-us/news/tags/patch-notes/");
    let bytes = app
        .web
        .get(url)
        .send()
        .await
        .ok()?
        .error_for_status()
        .ok()?
        .bytes()
        .await
        .ok()?;
    if bytes.len() > 2 * 1024 * 1024 {
        return None;
    }
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
