//! Gateway enforcement and bounded autopurge. Workers claim a distributed lease before polling.
use crate::{
    App,
    commands::moderation::{self, Act},
};
use pinchora_core::{mod_parity::Config, moderation::CaseKind, modules::Module};
use std::{sync::Arc, time::Duration};
use twilight_http::request::AuditLogReason;
use twilight_model::{id::Id, voice::VoiceState};

async fn enabled(app: &App, g: u64) -> bool {
    app.module_allowed(g, None, Module::Moderation).await
}
pub async fn voice_event(app: &Arc<App>, v: &VoiceState) {
    if let Err(e) = voice_inner(app, v).await {
        tracing::warn!(error=%e,"moderation voice enforcement failed")
    }
}
async fn voice_inner(app: &Arc<App>, v: &VoiceState) -> anyhow::Result<()> {
    let Some(g) = v.guild_id.map(|id| id.get()) else {
        return Ok(());
    };
    let user = v.user_id.get();
    let place = format!("modparity:at:{g}:{user}");
    if let Some(old) = app.cache.get(&place).await? {
        app.cache
            .set_remove(&format!("modparity:voice:{g}:{old}"), &user.to_string())
            .await?;
    }
    if let Some(channel) = v.channel_id {
        app.cache
            .put(&place, &channel.to_string(), Duration::from_secs(172800))
            .await?;
        app.cache
            .set_add(&format!("modparity:voice:{g}:{channel}"), &user.to_string())
            .await?;
    } else {
        app.cache.forget(&place).await?;
    }
    if !enabled(app, g).await {
        return Ok(());
    }
    let banned: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM mod_voice_bans WHERE guild_id=$1 AND user_id=$2)")
            .bind(g as i64)
            .bind(user as i64)
            .fetch_one(app.db.pool())
            .await?;
    if banned && v.channel_id.is_some() {
        app.http
            .update_guild_member(Id::new(g), v.user_id)
            .channel_id(None)
            .await?;
    }
    reconcile(app, g, user, v.channel_id.map(|id| id.to_string())).await
}
pub(crate) async fn member_event(app: &Arc<App>, g: u64, user: u64) {
    let result = async {
        if !enabled(app, g).await {
            return Ok(());
        }
        let voice = app.cache.get(&format!("modparity:at:{g}:{user}")).await?;
        reconcile(app, g, user, voice).await
    }
    .await;
    if let Err(e) = result {
        tracing::warn!(error=%e,g,user,"moderation role reconciliation failed")
    }
}
async fn reconcile(app: &App, g: u64, user: u64, voice: Option<String>) -> anyhow::Result<()> {
    let c = app.db.mod_parity_config(g).await?;
    if c.conditions.is_empty() && c.booster_role.is_none() && !c.booster_personal {
        return Ok(());
    }
    let member = app.http.guild_member(Id::new(g), Id::new(user)).await?.model().await?;
    if member.premium_since.is_none() {
        let role: Option<i64> =
            sqlx::query_scalar("SELECT role_id FROM mod_booster_roles WHERE guild_id=$1 AND user_id=$2")
                .bind(g as i64)
                .bind(user as i64)
                .fetch_optional(app.db.pool())
                .await?;
        if let Some(role) = role {
            app.http
                .remove_guild_member_role(Id::new(g), Id::new(user), Id::new(role as u64))
                .await?;
        }
    }
    let restricted: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM mod_restrictions WHERE guild_id=$1 AND user_id=$2)")
            .bind(g as i64)
            .bind(user as i64)
            .fetch_one(app.db.pool())
            .await?;
    if restricted {
        return Ok(());
    }
    let table = app.http.roles(Id::new(g)).await?.models().await?;
    let mine = app.my_roles(g).await?;
    let top = table
        .iter()
        .filter(|r| mine.contains(&r.id.get()))
        .map(|r| r.position)
        .max()
        .unwrap_or(0);
    let mut decisions = Vec::new();
    if let Some(role) = c.booster_role {
        decisions.push((role, member.premium_since.is_some()))
    }
    for rule in c.conditions {
        let held = |s: &String| member.roles.iter().any(|id| id.to_string() == *s);
        let matches = if rule.all {
            rule.requires.iter().all(held)
        } else {
            rule.requires.is_empty() || rule.requires.iter().any(held)
        };
        decisions.push((
            rule.role,
            matches && rule.voice_channel.as_ref().is_none_or(|id| Some(id) == voice.as_ref()),
        ));
    }
    for (role, want) in decisions {
        let Some(r) = table.iter().find(|r| r.id.to_string() == role) else {
            continue;
        };
        if r.managed
            || r.id.get() == g
            || r.position >= top
            || !pinchora_core::settings::dangerous_permissions(r.permissions.bits()).is_empty()
        {
            continue;
        }
        if member.roles.contains(&r.id) == want {
            continue;
        }
        if want {
            app.http.add_guild_member_role(Id::new(g), Id::new(user), r.id).await?;
        } else {
            app.http
                .remove_guild_member_role(Id::new(g), Id::new(user), r.id)
                .await?;
        }
        let act = Act {
            kind: if want { CaseKind::RoleAdd } else { CaseKind::RoleRemove },
            target: user,
            target_name: member.user.name.clone(),
            reason: None,
            duration: None,
            details: Some(role),
        };
        moderation::record(app, g, (app.application_id.get(), "Pinchora"), &act, None).await?;
        moderation::parity::extra_log(app, g, app.application_id.get(), act.kind, user).await;
    }
    Ok(())
}
pub fn spawn_ticker(app: Arc<App>) {
    let task = app.clone();
    app.spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(60));
        loop {
            interval.tick().await;
            if let Err(e) = sweep(&task).await {
                tracing::warn!(error=%e,"autopurge failed")
            }
        }
    });
}
async fn sweep(app: &App) -> anyhow::Result<()> {
    let bucket = app.now().timestamp() / 60;
    if app
        .cache
        .try_cooldown(&format!("modparity:autopurge:{bucket}"), Duration::from_secs(120))
        .await?
        .is_some()
    {
        return Ok(());
    }
    let rows: Vec<(i64, serde_json::Value)> =
        sqlx::query_as("SELECT guild_id,config FROM mod_parity_settings WHERE config->'autopurge' != '[]'::jsonb")
            .fetch_all(app.db.pool())
            .await?;
    for (g, json) in rows {
        if !enabled(app, g as u64).await {
            continue;
        }
        let c: Config = serde_json::from_value(json)?;
        for rule in c.autopurge {
            let Some(ch) = pinchora_core::mod_parity::id(&rule.channel) else {
                continue;
            };
            let result = async {
                let messages = app
                    .http
                    .channel_messages(Id::new(ch))
                    .before(Id::new(
                        (((app.now() - chrono::Duration::seconds(rule.age_secs)).timestamp_millis() - 1_420_070_400_000)
                            as u64)
                            << 22,
                    ))
                    .limit(100)
                    .await?
                    .models()
                    .await?;
                let now = app.now();
                let mut removed = 0;
                for msg in messages {
                    if msg.pinned
                        || (rule.bots_only && !msg.author.bot)
                        || pinchora_core::snowflake::timestamp(msg.id.get())
                            > now - chrono::Duration::seconds(rule.age_secs)
                    {
                        continue;
                    }
                    if app
                        .http
                        .delete_message(Id::new(ch), msg.id)
                        .reason("Autopurge")
                        .await
                        .is_ok()
                    {
                        removed += 1;
                    }
                }
                if removed > 0 {
                    let act = Act {
                        kind: CaseKind::Purge,
                        target: ch,
                        target_name: rule.channel,
                        reason: None,
                        duration: None,
                        details: Some(removed.to_string()),
                    };
                    moderation::record(app, g as u64, (app.application_id.get(), "Pinchora"), &act, None).await?;
                    moderation::parity::extra_log(app, g as u64, app.application_id.get(), act.kind, ch).await;
                }
                anyhow::Ok(())
            }
            .await;
            if let Err(e) = result {
                tracing::warn!(error=%e,g,ch,"channel autopurge failed")
            }
        }
    }
    Ok(())
}
/// Seed every occupied voice channel after identify, before bulk voice operations are offered.
pub fn seed_voice(app: &Arc<App>, event: &twilight_model::gateway::payload::incoming::GuildCreate) {
    let twilight_model::gateway::payload::incoming::GuildCreate::Available(guild) = event else {
        return;
    };
    let states = guild.voice_states.clone();
    let g = guild.id.get();
    let task = app.clone();
    app.spawn(async move {
        for mut v in states {
            v.guild_id = Some(Id::new(g));
            voice_event(&task, &v).await;
        }
    });
}
