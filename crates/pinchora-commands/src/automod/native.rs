//! Debounced, restart-safe reconciliation. Only rules created by this bot are touched.
use super::{
    actions::{alert_channel, protected},
    process_hit,
};
use crate::App;
use pinchora_cache::Invalidation;
use pinchora_core::{
    automod::native_rule,
    modules::{Access, Module},
    t,
};
use pinchora_db::AutoModHit;
use std::{sync::Arc, time::Duration};
use twilight_http::{request::Request, routing::Route};
use twilight_model::{
    gateway::payload::incoming::AutoModerationActionExecution,
    guild::auto_moderation::{AutoModerationRule, AutoModerationTriggerMetadata},
    id::Id,
};

pub fn on_native_rule_change(app: Arc<App>, guild: u64) {
    let owned = app.clone();
    app.spawn(async move {
        if let Err(error) = owned.db.queue_automod_sync(guild).await {
            tracing::warn!(%error,"automod drift queue failed");
        }
    });
}

pub async fn sync_guild(app: &App, guild: u64) -> anyhow::Result<()> {
    let key = format!("am:sync:{guild}");
    if app.cache.try_cooldown(&key, Duration::from_secs(120)).await?.is_some() {
        return Ok(());
    }
    let result = tokio::time::timeout(Duration::from_secs(90), reconcile(app, guild)).await;
    app.cache.release_cooldown(&key).await?;
    result?
}
async fn reconcile(app: &App, guild: u64) -> anyhow::Result<()> {
    let Some(status) = app.db.automod_sync_status(guild).await? else {
        return Ok(());
    };
    let result = reconcile_inner(app, guild).await;
    let error = result.as_ref().err().map(|e| e.to_string());
    app.db
        .finish_automod_sync(guild, status.revision, status.generation, error.as_deref())
        .await?;
    result
}
async fn reconcile_inner(app: &App, guild: u64) -> anyhow::Result<()> {
    let config = app.db.automod_config(guild).await?;
    let lang = app.guild_lang(guild).await;
    let enabled = config.profile != "off" && app.module_access(guild, None, Module::AutoMod).await? == Access::Allowed;
    let existing = app.http.auto_moderation_rules(Id::new(guild)).await?.models().await?;
    let channel = alert_channel(app, guild, &config).await;
    let mut wanted = Vec::new();
    for rule in &config.rules {
        if let Some(payload) = channel.and_then(|c| native_rule(rule, &c.to_string(), enabled)) {
            wanted.push((rule, payload));
        }
    }
    // Explicitly count other applications' rules against the same guild-wide limits.
    let mut counts = [0usize; 7];
    for r in &existing {
        let kind = u8::from(r.trigger_type) as usize;
        if kind < counts.len() {
            counts[kind] += 1;
        }
    }
    let ours = |r: &AutoModerationRule| {
        r.creator_id.get() == app.application_id.get()
            && (r.name.starts_with("Pinchora · ")
                || config
                    .rules
                    .iter()
                    .any(|rule| rule.discord_rule_id.as_deref() == Some(r.id.to_string().as_str())))
    };
    for old in existing.iter().filter(|r| ours(r)) {
        let desired = wanted
            .iter()
            .find(|(r, p)| r.discord_rule_id.as_deref() == Some(old.id.to_string().as_str()) || p.name == old.name);
        if desired.is_none_or(|(_, p)| p.trigger_type != u8::from(old.trigger_type)) {
            app.http.delete_auto_moderation_rule(Id::new(guild), old.id).await?;
            counts[u8::from(old.trigger_type) as usize] = counts[u8::from(old.trigger_type) as usize].saturating_sub(1);
            for rule in &config.rules {
                if rule.discord_rule_id.as_deref() == Some(old.id.to_string().as_str()) {
                    set_native(app, guild, rule.id.as_deref().unwrap_or("0").parse()?, None).await?;
                }
            }
        }
    }
    let mut limited = false;
    for (rule, payload) in wanted {
        let id: i64 = rule.id.as_deref().unwrap_or("0").parse()?;
        let old = existing.iter().find(|r| {
            ours(r)
                && u8::from(r.trigger_type) == payload.trigger_type
                && (rule.discord_rule_id.as_deref() == Some(r.id.to_string().as_str()) || r.name == payload.name)
        });
        let native_id = if let Some(old) = old {
            let metadata: AutoModerationTriggerMetadata = serde_json::from_value(payload.trigger_metadata.clone())?;
            let actions: Vec<twilight_model::guild::auto_moderation::AutoModerationAction> =
                serde_json::from_value(serde_json::to_value(&payload.actions)?)?;
            if old.enabled != payload.enabled
                || old.name != payload.name
                || old.trigger_metadata != metadata
                || old.actions != actions
                || old.exempt_roles.iter().map(ToString::to_string).collect::<Vec<_>>() != payload.exempt_roles
                || old.exempt_channels.iter().map(ToString::to_string).collect::<Vec<_>>() != payload.exempt_channels
            {
                let mut body = serde_json::to_value(&payload)?;
                body.as_object_mut().unwrap().remove("trigger_type");
                let request = Request::builder(&Route::UpdateAutoModerationRule {
                    guild_id: guild,
                    auto_moderation_rule_id: old.id.get(),
                })
                .json(&body)
                .build()?;
                app.http.request::<AutoModerationRule>(request).await?;
            }
            old.id.get()
        } else {
            let cap = if payload.trigger_type == 1 { 6 } else { 1 };
            if counts[payload.trigger_type as usize] >= cap {
                limited = true;
                set_native(app, guild, id, None).await?;
                continue;
            }
            let request = Request::builder(&Route::CreateAutoModerationRule { guild_id: guild })
                .json(&payload)
                .build()?;
            let created = app.http.request::<AutoModerationRule>(request).await?.model().await?;
            counts[payload.trigger_type as usize] += 1;
            created.id.get()
        };
        set_native(app, guild, id, Some(native_id)).await?;
    }
    if limited {
        anyhow::bail!(t!(lang, "automod.native_limit"));
    }
    if channel.is_none() && config.rules.iter().any(|r| r.native && r.enabled) {
        anyhow::bail!(t!(lang, "automod.native_channel"));
    }
    Ok(())
}

async fn set_native(app: &App, guild: u64, id: i64, native: Option<u64>) -> anyhow::Result<()> {
    app.db.set_automod_native(guild, id, native).await?;
    app.forget_automod_config(guild).await;
    if let Err(error) = app.cache.publish_invalidation(Invalidation::Guild(guild)).await {
        tracing::warn!(%error, guild, "automod native cache invalidation failed");
    }
    Ok(())
}

pub async fn on_native_execution(app: Arc<App>, event: AutoModerationActionExecution) -> anyhow::Result<()> {
    let guild = event.guild_id.get();
    let Some(channel) = event.channel_id else {
        return Ok(());
    };
    if app.module_access(guild, Some(channel.get()), Module::AutoMod).await? != Access::Allowed {
        return Ok(());
    }
    let config = app.db.automod_config(guild).await?;
    if config.profile == "off" {
        return Ok(());
    }
    let Some(rule) = config
        .rules
        .iter()
        .find(|r| r.enabled && r.discord_rule_id.as_deref() == Some(event.rule_id.to_string().as_str()))
    else {
        return Ok(());
    };
    let Some(facts) = app.guild_facts(guild).await else {
        return Ok(());
    };
    if facts.owner == event.user_id.get() {
        return Ok(());
    }
    let member = app
        .http
        .guild_member(event.guild_id, event.user_id)
        .await?
        .model()
        .await?;
    if member.user.bot {
        return Ok(());
    }
    if protected(&app, guild, event.user_id.get(), &member.roles).await? {
        return Ok(());
    }
    // Old keyword mirrors can remain at Discord until reconciliation. Their notifications
    // cannot prove a local match, so they must never cause a punitive action.
    if matches!(rule.kind.as_str(), "links" | "invites" | "bad_words") {
        return Ok(());
    }
    let roles = member.roles.iter().map(ToString::to_string).collect::<Vec<_>>();
    let Some(ancestry) = app.automod_lineage(channel.get()).await else {
        return Ok(());
    };
    let channels: Vec<String> = ancestry.into_iter().map(|id| id.to_string()).collect();
    if !rule.applies(&channels, &roles) {
        return Ok(());
    }
    // Discord emits one event per native action. Our mirrors have only one alert action.
    let message = event.message_id.or(event.alert_system_message_id);
    let hit = AutoModHit {
        id: 0,
        guild_id: guild as i64,
        rule_id: rule.id.as_deref().and_then(|s| s.parse().ok()),
        user_id: event.user_id.get() as i64,
        channel_id: channel.get() as i64,
        message_id: message.map(|id| id.get() as i64),
        action: if config.dry(rule, app.now()) {
            "log_only"
        } else {
            "pending"
        }
        .into(),
        snippet: app
            .message_content
            .then(|| crate::util::strip_mentions(&event.content).chars().take(200).collect()),
        created_at: app.now(),
        reviewed: false,
    };
    if let Some(mut hit) = app.db.open_automod_hit(&hit).await? {
        // The alert's id deduplicates a blocked event, but is never a member message to delete.
        if event.message_id.is_none() {
            hit.message_id = None;
        }
        process_hit(&app, &config, rule, hit).await?;
    }
    Ok(())
}

pub fn spawn_ticker(app: Arc<App>) {
    tokio::spawn(async move {
        let mut wake = tokio::time::interval(Duration::from_secs(5));
        wake.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let mut rounds = 0u32;
        loop {
            wake.tick().await;
            match app.db.automod_sync_due().await {
                Ok(guilds) => {
                    for guild in guilds {
                        if let Err(error) = Box::pin(sync_guild(&app, guild as u64)).await {
                            tracing::warn!(%error,guild,"automod native sync failed");
                        }
                    }
                }
                Err(error) => tracing::warn!(%error,"automod sync queue unavailable"),
            }
            rounds = rounds.wrapping_add(1);
            if rounds.is_multiple_of(720)
                && let Err(error) = app.db.sweep_automod().await
            {
                tracing::warn!(%error,"automod retention sweep failed");
            }
        }
    });
}
