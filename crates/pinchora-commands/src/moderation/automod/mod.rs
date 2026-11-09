//! AutoMod executes before XP and text commands. All network actions are supervised background work.
pub(crate) mod actions;
mod native;
use crate::App;
use crate::infra::hooks::Verdict;
pub use crate::moderation::automod::actions::check_config_roles;
pub use crate::moderation::automod::native::on_native_execution;
pub use crate::moderation::automod::native::on_native_rule_change;
pub use crate::moderation::automod::native::spawn_ticker;
pub use crate::moderation::automod::native::sync_guild;
use pinchora_core::admin::modules::Access;
use pinchora_core::admin::modules::Module;
use pinchora_core::moderation::automod::Config;
use pinchora_core::moderation::automod::Input;
use pinchora_core::moderation::automod::Rule;
use pinchora_core::moderation::automod::matches;
use pinchora_core::t;
use pinchora_db::{AutoModHit, GuildSettings};
use std::{
    hash::{Hash, Hasher},
    sync::Arc,
    time::Duration,
};
use twilight_model::{channel::Message, id::Id};

pub(crate) async fn after_message(app: &Arc<App>, msg: &Message, _settings: &GuildSettings) -> Verdict {
    match evaluate(app, msg).await {
        Ok(blocked) if blocked => Verdict::Blocked,
        Ok(_) => Verdict::Continue,
        Err(error) => {
            tracing::warn!(%error, "automod message evaluation failed");
            Verdict::Continue
        }
    }
}
async fn evaluate(app: &Arc<App>, msg: &Message) -> anyhow::Result<bool> {
    let Some(guild) = msg.guild_id.map(Id::get) else {
        return Ok(false);
    };
    if msg.author.bot
        || msg.webhook_id.is_some()
        || app
            .module_access(guild, Some(msg.channel_id.get()), Module::AutoMod)
            .await?
            != Access::Allowed
    {
        return Ok(false);
    }
    let config = app.automod_config(guild).await?;
    if config.profile == "off" || !config.rules.iter().any(|r| r.enabled) {
        return Ok(false);
    }
    let Some(facts) = app.guild_facts(guild).await else {
        return Ok(false);
    };
    if facts.owner == msg.author.id.get() {
        return Ok(false);
    }
    let roles: Vec<String> = match &msg.member {
        Some(m) => m.roles.iter().map(ToString::to_string).collect(),
        None => return Ok(false),
    };
    if crate::moderation::automod::actions::protected(
        app,
        guild,
        msg.author.id.get(),
        &msg.member.as_ref().unwrap().roles,
    )
    .await?
    {
        return Ok(false);
    }
    let Some(ancestry) = app.automod_lineage(msg.channel_id.get()).await else {
        return Ok(false);
    };
    let channels: Vec<String> = ancestry.into_iter().map(|id| id.to_string()).collect();
    let mut blocked = false;
    for rule in &config.rules {
        if !rule.applies(&channels, &roles) || (!app.message_content && rule.needs_content()) {
            continue;
        }
        let mut input = Input {
            content: &msg.content,
            mentions: msg.mentions.len() + msg.mention_roles.len() + usize::from(msg.mention_everyone),
            attachments: msg.attachments.len(),
            media: msg.attachments.iter().any(|a| {
                a.content_type
                    .as_deref()
                    .is_some_and(|mime| mime.starts_with("image/") || mime.starts_with("video/"))
            }) || msg.embeds.iter().any(|e| e.image.is_some() || e.video.is_some()),
            ..Input::default()
        };
        if matches!(rule.kind.as_str(), "spam" | "duplicate") {
            let hash = if rule.kind == "duplicate" {
                let mut h = std::collections::hash_map::DefaultHasher::new();
                msg.content.trim().to_lowercase().hash(&mut h);
                Some(h.finish())
            } else {
                None
            };
            let count = app
                .cache
                .automod_window(
                    guild,
                    msg.author.id.get(),
                    rule.id.as_deref().unwrap_or(&rule.name),
                    msg.id.get(),
                    app.now().timestamp_millis(),
                    rule.params.window_secs,
                    hash,
                )
                .await?;
            input.spam = count;
            input.duplicates = count;
        }
        if !matches(rule, &input) {
            continue;
        }
        let dry = config.dry(rule, app.now());
        let hit = AutoModHit {
            id: 0,
            guild_id: guild as i64,
            rule_id: rule.id.as_deref().and_then(|s| s.parse().ok()),
            user_id: msg.author.id.get() as i64,
            channel_id: msg.channel_id.get() as i64,
            message_id: Some(msg.id.get() as i64),
            action: if dry { "log_only" } else { "pending" }.into(),
            snippet: app.message_content.then(|| {
                crate::infra::util::strip_mentions(&msg.content)
                    .chars()
                    .take(200)
                    .collect()
            }),
            created_at: app.now(),
            reviewed: false,
        };
        if let Some(hit) = app.db.open_automod_hit(&hit).await? {
            if !dry
                && rule
                    .actions
                    .iter()
                    .any(|a| matches!(a.kind.as_str(), "delete" | "timeout" | "kick" | "ban"))
            {
                blocked = true;
            }
            let (owned, config, rule) = (app.clone(), config.clone(), rule.clone());
            app.spawn(async move {
                if let Err(error) = process_hit(&owned, &config, &rule, hit).await {
                    tracing::warn!(%error, "automod action chain failed");
                }
            });
        } else if !dry && rule.actions.iter().any(|a| a.kind == "delete") {
            blocked = true;
        }
    }
    Ok(blocked)
}

pub(crate) async fn process_hit(app: &App, config: &Config, rule: &Rule, mut hit: AutoModHit) -> anyhow::Result<()> {
    let guild = hit.guild_id as u64;
    if hit.action == "log_only" || config.dry(rule, app.now()) {
        crate::moderation::automod::actions::alert(app, config, rule, &hit).await?;
        return Ok(());
    }
    let mut chain = rule.actions.clone();
    let key = format!("am:strike:{guild}:{}:{}", hit.user_id, hit.message_id.unwrap_or(hit.id));
    if app.cache.once(&key, Duration::from_secs(3600)).await? {
        let window = config.escalation.first().map_or(3600, |e| e.window_secs);
        let count = app
            .db
            .automod_strike(guild, hit.user_id as u64, app.now(), window)
            .await?;
        if let Some(e) = config.escalation.iter().find(|e| e.strikes as i32 == count) {
            chain.push(e.action.clone());
        }
    }
    let mut outcomes = Vec::new();
    let mut logged = false;
    for action in &chain {
        match crate::moderation::automod::actions::execute(app, config, rule, &hit, action, None).await {
            Ok(extra) => {
                logged |= action.kind == "log";
                outcomes.push(action.kind.clone());
                outcomes.extend(extra);
            }
            Err(error) => {
                let lang = app.guild_lang(guild).await;
                outcomes.push(t!(
                    lang,
                    "automod.action_failed",
                    action = t!(lang, &format!("automod.action.{}", action.kind)),
                    reason = error
                ));
            }
        }
        hit.action = outcomes.join(" · ");
    }
    app.db.finish_automod_hit(guild, hit.id, &hit.action).await?;
    if !logged {
        crate::moderation::automod::actions::alert(app, config, rule, &hit).await?;
    }
    Ok(())
}
