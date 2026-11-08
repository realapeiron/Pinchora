use crate::{App, Cx, ReactionEvent};
use pinchora_core::{
    t,
    workflows::{Event, Trigger},
};
use std::{str::Split, sync::Arc, time::Duration};
use twilight_model::{
    channel::Message,
    gateway::payload::incoming::{MemberAdd, MemberRemove, MemberUpdate},
    voice::VoiceState,
};
pub fn simple(app: &Arc<App>, guild: u64, user: u64, kind: &str, channel: Option<u64>) {
    super::emit(
        app,
        guild,
        Event {
            kind: kind.into(),
            user_id: user.to_string(),
            channel_id: channel.map(|c| c.to_string()).unwrap_or_default(),
            ..Default::default()
        },
    );
}
pub fn message(app: &Arc<App>, msg: &Message) {
    if app.message_content
        && !msg.author.bot
        && msg.webhook_id.is_none()
        && let Some(guild) = msg.guild_id
    {
        super::emit(
            app,
            guild.get(),
            Event {
                kind: "message_keyword".into(),
                user_id: msg.author.id.to_string(),
                channel_id: msg.channel_id.to_string(),
                message_id: msg.id.to_string(),
                value: msg.content.clone(),
                ..Default::default()
            },
        );
    }
}
pub async fn joined(app: &Arc<App>, e: &MemberAdd) {
    if e.user.bot {
        return;
    }
    seed(
        app,
        e.guild_id.get(),
        e.user.id.get(),
        e.roles.iter().map(ToString::to_string).collect(),
    )
    .await;
    simple(app, e.guild_id.get(), e.user.id.get(), "member_join", None);
}
pub fn left(app: &Arc<App>, e: &MemberRemove) {
    if !e.user.bot {
        simple(app, e.guild_id.get(), e.user.id.get(), "member_leave", None);
    }
}
async fn seed(app: &App, guild: u64, user: u64, roles: Vec<String>) {
    if let Err(error) = app
        .cache
        .put(
            &format!("wf:roles:{guild}:{user}"),
            &serde_json::to_string(&roles).unwrap_or_default(),
            Duration::from_secs(86400),
        )
        .await
    {
        tracing::warn!(%error,"workflow roles unavailable");
    }
}
pub async fn roles(app: &Arc<App>, e: &MemberUpdate) {
    if e.user.bot {
        return;
    }
    let guild = e.guild_id.get();
    let user = e.user.id.get();
    let key = format!("wf:roles:{guild}:{user}");
    let now: Vec<String> = e.roles.iter().map(ToString::to_string).collect();
    let Ok(previous) = app
        .cache
        .replace_workflow_roles(&key, &serde_json::to_string(&now).unwrap_or_default())
        .await
    else {
        return;
    };
    let Some(before) = previous.and_then(|s| serde_json::from_str::<Vec<String>>(&s).ok()) else {
        return;
    };
    for role in before.iter().chain(&now) {
        if before.contains(role) == now.contains(role) {
            continue;
        }
        match app
            .db
            .consume_workflow_role_change(guild, user, role.parse().unwrap_or(0), now.contains(role))
            .await
        {
            Ok(true) => {
                // The legacy responder sees this same callback after the workflow hook.
                let _ = app
                    .cache
                    .put(
                        &crate::custom::own_change_key(guild, user, role.parse().unwrap_or(0)),
                        "1",
                        Duration::from_secs(120),
                    )
                    .await;
                continue;
            }
            Err(_) => continue,
            Ok(false) => {}
        }
        if !matches!(
            app.cache
                .get(&crate::custom::own_change_key(guild, user, role.parse().unwrap_or(0)))
                .await,
            Ok(None)
        ) {
            continue;
        }
        super::emit(
            app,
            guild,
            Event {
                kind: if now.contains(role) {
                    "role_added"
                } else {
                    "role_removed"
                }
                .into(),
                user_id: user.to_string(),
                role_id: role.clone(),
                roles: now.clone(),
                ..Default::default()
            },
        );
    }
}
pub fn reaction(app: &Arc<App>, e: &ReactionEvent) {
    if let ReactionEvent::Add(e) = e {
        if e.user_id.get() == app.application_id.get() || e.member.as_ref().is_some_and(|m| m.user.bot) {
            return;
        }
        if let Some(guild) = e.guild_id {
            super::emit(
                app,
                guild.get(),
                Event {
                    kind: "reaction_added".into(),
                    user_id: e.user_id.to_string(),
                    channel_id: e.channel_id.to_string(),
                    message_id: e.message_id.to_string(),
                    value: match &e.emoji {
                        twilight_model::channel::message::EmojiReactionType::Unicode { name } => name.clone(),
                        twilight_model::channel::message::EmojiReactionType::Custom { id, name, animated } => {
                            format!(
                                "<{}:{}:{}>",
                                if *animated { "a" } else { "" },
                                name.as_deref().unwrap_or("emoji"),
                                id
                            )
                        }
                    },
                    ..Default::default()
                },
            );
        }
    }
}
pub async fn voice(app: &Arc<App>, state: &VoiceState) {
    let Some(guild) = state.guild_id else { return };
    let key = format!("wf:voice:{guild}:{}", state.user_id);
    let current = state.channel_id.map(|c| c.to_string()).unwrap_or_default();
    let previous = app.cache.get(&key).await;
    if let Err(error) = app.cache.put(&key, &current, Duration::from_secs(86400)).await {
        tracing::warn!(%error,"workflow voice state unavailable");
        return;
    }
    if let Ok(previous) = previous
        && !current.is_empty()
        && previous.as_deref() != Some(&current)
    {
        simple(
            app,
            guild.get(),
            state.user_id.get(),
            "voice_join",
            state.channel_id.map(|c| c.get()),
        );
    }
}
pub async fn command(cx: &Cx, name: &str) -> anyhow::Result<bool> {
    let Some(guild) = cx.guild_id() else { return Ok(false) };
    // Prefix prevents collisions with custom commands, while preserving the registered root budget.
    let Some(name) = name.strip_prefix("wf:") else {
        return Ok(false);
    };
    if let Some(row) = cx
        .app
        .db
        .workflows(guild)
        .await?
        .into_iter()
        .find(|r| r.name == name && r.enabled && r.definition().is_ok_and(|d| d.trigger == Trigger::Command {}))
    {
        invoke(cx, row, "command").await?;
    } else {
        cx.reply_ephemeral(t!(cx.lang(), "workflows.unavailable")).await?;
    }
    Ok(true)
}
async fn invoke(cx: &Cx, row: pinchora_db::WorkflowRow, kind: &str) -> anyhow::Result<()> {
    cx.responder.defer(true).await?;
    let event = Event {
        kind: kind.into(),
        user_id: cx.author()?.id.to_string(),
        channel_id: cx
            .interaction
            .channel
            .as_ref()
            .map(|c| c.id.to_string())
            .unwrap_or_default(),
        message_id: cx
            .interaction
            .message
            .as_ref()
            .map(|m| m.id.to_string())
            .unwrap_or_default(),
        ..Default::default()
    };
    let result = super::engine::run(&cx.app, &row, event).await;
    let key = match result {
        Ok(p) if p.outcome == "completed" => "workflows.completed",
        Ok(p) if p.outcome == "filtered" => "workflows.filtered",
        _ => "workflows.failed",
    };
    cx.reply_ephemeral(t!(cx.lang(), key)).await
}
pub async fn on_component(cx: &Cx, mut parts: Split<'_, char>) -> anyhow::Result<()> {
    let guild = cx.guild_id().ok_or_else(|| anyhow::anyhow!("guild"))?;
    let parsed = parts.next() == Some("b");
    let id = parts.next().and_then(|s| s.parse::<i64>().ok());
    let index = parts.next().and_then(|s| s.parse::<usize>().ok());
    if parsed
        && index == Some(0)
        && parts.next().is_none()
        && let Some(id) = id
        && let Some(row) = cx
            .app
            .db
            .workflow(guild, id)
            .await?
            .filter(|r| r.enabled && r.definition().is_ok_and(|d| d.trigger == Trigger::Button {}))
    {
        return invoke(cx, row, "button").await;
    }
    cx.reply_ephemeral(t!(cx.lang(), "workflows.unavailable")).await
}
