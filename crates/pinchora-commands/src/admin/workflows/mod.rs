//! Workflow execution, entry points and schedule worker.
mod actions;
pub mod engine;
mod messages;
mod schedule;
mod tickets;
pub mod triggers;
use crate::App;
pub use crate::admin::workflows::schedule::spawn_ticker;
use pinchora_core::admin::modules::Module;
use pinchora_core::admin::workflows::Event;
use pinchora_core::admin::workflows::Trigger;
use std::sync::Arc;
pub async fn available(app: &App, guild: u64, trigger: &Trigger) -> bool {
    match trigger.owner() {
        Some(Module::Leveling) => app
            .guild_settings(guild)
            .await
            .is_ok_and(|settings| settings.leveling_enabled),
        Some(module) => app
            .guild_modules(guild)
            .await
            .is_ok_and(|rules| rules.rule(module).enabled),
        None => true,
    }
}
pub async fn trigger_catalog(app: &App, guild: u64) -> Vec<Trigger> {
    let all = vec![
        Trigger::Command {},
        Trigger::Button {},
        Trigger::MemberJoin {},
        Trigger::MemberLeave {},
        Trigger::LevelUp {},
        Trigger::RoleAdded { role_id: String::new() },
        Trigger::RoleRemoved { role_id: String::new() },
        Trigger::ReactionAdded {
            emoji: "⭐".into(),
            channel_id: String::new(),
        },
        Trigger::MessageKeyword { keyword: String::new() },
        Trigger::Schedule {
            spec: pinchora_core::admin::workflows::Schedule::Daily { hour: 9, minute: 0 },
            tz: "UTC".into(),
        },
        Trigger::VoiceJoin {},
        Trigger::Birthday {},
        Trigger::Achievement {},
        Trigger::TicketOpened {},
        Trigger::TicketClosed {},
        Trigger::StarboardPost {},
        Trigger::FeedItem {},
    ];
    let mut result = Vec::new();
    for trigger in all {
        if available(app, guild, &trigger).await {
            result.push(trigger);
        }
    }
    result
}
/// Spawn outside the owning feature: failures cannot fail XP, joins, tickets or feeds.
pub fn emit(app: &Arc<App>, guild: u64, event: Event) {
    if event.depth != 0 {
        return;
    }
    let app = app.clone();
    let task = app.clone();
    app.spawn(async move {
        if let Err(error) = dispatch(&task, guild, event).await {
            tracing::warn!(guild,%error,"workflow event failed");
        }
    });
}
async fn dispatch(app: &App, guild: u64, event: Event) -> anyhow::Result<()> {
    if !app.guild_modules(guild).await?.rule(Module::Workflows).enabled {
        return Ok(());
    }
    for row in app.db.workflows(guild).await? {
        let def = row.definition()?;
        if def.enabled
            && def.trigger.matches(&event)
            && let Err(error) = crate::admin::workflows::engine::run(app, &row, event.clone()).await
        {
            tracing::warn!(guild, workflow=row.id, %error, "workflow run refused");
        }
    }
    Ok(())
}

pub async fn observe(app: &App, guild: u64, user: u64, kind: &str, channel: Option<u64>) {
    if let Err(error) = dispatch(
        app,
        guild,
        Event {
            kind: kind.into(),
            user_id: user.to_string(),
            channel_id: channel.map(|c| c.to_string()).unwrap_or_default(),
            ..Default::default()
        },
    )
    .await
    {
        tracing::warn!(guild,%error,"workflow trigger failed");
    }
}
