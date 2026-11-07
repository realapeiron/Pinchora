//! Gateway enforcement and bounded autopurge. Workers claim a distributed lease before polling.
use crate::{
    App,
    commands::moderation::{self, Act},
};
use pinchora_core::{mod_parity::Config, moderation::CaseKind, modules::Module};
use std::{sync::Arc, time::Duration};
use twilight_http::request::AuditLogReason;
use twilight_model::{id::Id, voice::VoiceState};

pub(crate) fn voice_channel(value: &str) -> Option<String> {
    value.split('|').next().filter(|id| *id != "0").map(str::to_owned)
}
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
    // Timestamped tombstones preserve disconnects that race the identify snapshot.
    app.cache
        .map_set(
            &format!("modparity:voice-v2:{g}"),
            &user.to_string(),
            &format!(
                "{}|0|{}",
                v.channel_id.map_or(0, |id| id.get()),
                chrono::Utc::now().timestamp_millis()
            ),
            Duration::from_secs(172800),
        )
        .await?;
    enforce_voice(app, v).await
}
async fn enforce_voice(app: &Arc<App>, v: &VoiceState) -> anyhow::Result<()> {
    let Some(g) = v.guild_id.map(|id| id.get()) else {
        return Ok(());
    };
    let user = v.user_id.get();
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
            .reason("Enforce voice ban")
            .await?;
    }
    reconcile(
        app,
        g,
        user,
        if banned {
            None
        } else {
            v.channel_id.map(|id| id.to_string())
        },
    )
    .await
}
pub(crate) async fn member_event(app: &Arc<App>, g: u64, user: u64) {
    let result = async {
        if !enabled(app, g).await {
            return Ok(());
        }
        let voice = app
            .cache
            .map_all(&format!("modparity:voice-v2:{g}"))
            .await?
            .into_iter()
            .find(|(id, _)| id == &user.to_string())
            .and_then(|(_, value)| voice_channel(&value));
        reconcile(app, g, user, voice).await
    }
    .await;
    if let Err(e) = result {
        tracing::warn!(error=%e,g,user,"moderation role reconciliation failed")
    }
}
async fn reconcile(app: &App, g: u64, user: u64, voice: Option<String>) -> anyhow::Result<()> {
    if enforce_restriction(app, g, user).await? {
        return Ok(());
    }
    let c = app.db.mod_parity_config(g).await?;
    if c.conditions.is_empty() && c.booster_role.is_none() && !c.booster_personal {
        return Ok(());
    }
    let member = app.http.guild_member(Id::new(g), Id::new(user)).await?.model().await?;
    let table = app.http.roles(Id::new(g)).await?.models().await?;
    let bot = app
        .http
        .guild_member(Id::new(g), Id::new(app.application_id.get()))
        .await?
        .model()
        .await?;
    let top = table
        .iter()
        .filter(|r| bot.roles.contains(&r.id))
        .map(|r| r.position)
        .max()
        .unwrap_or(0);
    let owner = app.http.guild(Id::new(g)).await?.model().await?.owner_id.get();
    if user == owner
        || user == app.application_id.get()
        || table
            .iter()
            .filter(|r| member.roles.contains(&r.id))
            .any(|r| r.position >= top)
    {
        return Ok(());
    }
    let mut decisions = Vec::new();
    if member.premium_since.is_none() {
        let role: Option<i64> =
            sqlx::query_scalar("SELECT role_id FROM mod_booster_roles WHERE guild_id=$1 AND user_id=$2")
                .bind(g as i64)
                .bind(user as i64)
                .fetch_optional(app.db.pool())
                .await?;
        if let Some(role) = role {
            decisions.push((role.to_string(), false));
        }
    }
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
/// Hold joins before sticky/automatic roles or welcomes can bypass a persistent restriction.
pub(crate) async fn member_join(app: &Arc<App>, g: u64, user: u64) -> crate::hooks::JoinVerdict {
    match enforce_restriction(app, g, user).await {
        Ok(false) => crate::hooks::JoinVerdict::Continue,
        Ok(true) => crate::hooks::JoinVerdict::Held,
        Err(error) => {
            tracing::warn!(%error, g, user, "restriction recovery failed");
            crate::hooks::JoinVerdict::Held
        }
    }
}

pub(crate) async fn restriction_overwrites(
    app: &App,
    g: u64,
    user: u64,
    rid: u64,
    managed: &[u64],
    audit: &str,
) -> anyhow::Result<()> {
    use twilight_model::channel::permission_overwrite::PermissionOverwriteType as Kind;
    let denied = twilight_model::guild::Permissions::SEND_MESSAGES
        | twilight_model::guild::Permissions::SEND_MESSAGES_IN_THREADS
        | twilight_model::guild::Permissions::CREATE_PUBLIC_THREADS
        | twilight_model::guild::Permissions::CREATE_PRIVATE_THREADS
        | twilight_model::guild::Permissions::ADD_REACTIONS
        | twilight_model::guild::Permissions::CONNECT
        | twilight_model::guild::Permissions::SPEAK;
    let channels = app.http.guild_channels(Id::new(g)).await?.models().await?;
    for channel in &channels {
        anyhow::ensure!(
            !channel
                .permission_overwrites
                .as_deref()
                .unwrap_or_default()
                .iter()
                .any(|o| o.allow.intersects(denied)
                    && ((o.kind == Kind::Member && o.id.get() == user)
                        || (o.kind == Kind::Role && managed.contains(&o.id.get())))),
            "restriction overwrite bypass"
        );
    }
    for channel in channels {
        let old = channel
            .permission_overwrites
            .as_deref()
            .unwrap_or_default()
            .iter()
            .find(|o| o.kind == Kind::Role && o.id.get() == rid);
        let allow = old.map_or(twilight_model::guild::Permissions::empty(), |o| o.allow) & !denied;
        let deny = old.map_or(twilight_model::guild::Permissions::empty(), |o| o.deny) | denied;
        if old.is_some_and(|o| o.allow == allow && o.deny == deny) {
            continue;
        }
        app.http
            .update_channel_permission(
                channel.id,
                &twilight_model::http::permission_overwrite::PermissionOverwrite {
                    id: Id::new(rid),
                    kind: twilight_model::http::permission_overwrite::PermissionOverwriteType::Role,
                    allow: Some(allow),
                    deny: Some(deny),
                },
            )
            .reason(audit)
            .await?;
    }
    Ok(())
}

async fn enforce_restriction(app: &App, g: u64, user: u64) -> anyhow::Result<bool> {
    // Same lock as apply/release: recovery must never reapply a restriction after release.
    let _guard = app.db.lock_ban(g, user).await?;
    let rid: Option<i64> = sqlx::query_scalar("SELECT role_id FROM mod_restrictions WHERE guild_id=$1 AND user_id=$2")
        .bind(g as i64)
        .bind(user as i64)
        .fetch_optional(app.db.pool())
        .await?;
    let Some(rid) = rid else {
        return Ok(false);
    };
    let member = match app.http.guild_member(Id::new(g), Id::new(user)).await {
        Ok(response) => response.model().await?,
        Err(error) if crate::roles::error_code(&error) == Some(10007) => return Ok(true),
        Err(error) => return Err(error.into()),
    };
    let roles = app.http.roles(Id::new(g)).await?.models().await?;
    let bot = app
        .http
        .guild_member(Id::new(g), Id::new(app.application_id.get()))
        .await?
        .model()
        .await?;
    let top = roles
        .iter()
        .filter(|r| bot.roles.contains(&r.id))
        .map(|r| r.position)
        .max()
        .unwrap_or(0);
    let owner = app.http.guild(Id::new(g)).await?.model().await?.owner_id.get();
    anyhow::ensure!(
        user != owner
            && user != app.application_id.get()
            && roles
                .iter()
                .filter(|r| member.roles.contains(&r.id))
                .all(|r| r.position < top),
        "hierarchy changed"
    );
    anyhow::ensure!(
        roles.iter().any(|r| r.id.get() == rid as u64
            && r.id.get() != g
            && !r.managed
            && r.position < top
            && pinchora_core::settings::dangerous_permissions(r.permissions.bits()).is_empty()),
        "unsafe restriction role"
    );
    let mut desired: Vec<_> = roles
        .iter()
        .filter(|r| r.managed && member.roles.contains(&r.id))
        .map(|r| r.id)
        .collect();
    anyhow::ensure!(
        !roles.iter().any(|r| member.roles.contains(&r.id)
            && r.managed
            && r.permissions
                .contains(twilight_model::guild::Permissions::ADMINISTRATOR)),
        "managed administrator"
    );
    restriction_overwrites(
        app,
        g,
        user,
        rid as u64,
        &desired.iter().map(|r| r.get()).collect::<Vec<_>>(),
        "Restore active moderation restriction",
    )
    .await?;
    desired.push(Id::new(rid as u64));
    if member.roles.len() != desired.len() || member.roles.iter().any(|r| !desired.contains(r)) {
        app.http
            .update_guild_member(Id::new(g), Id::new(user))
            .roles(&desired)
            .reason("Restore active moderation restriction")
            .await?;
    }
    Ok(true)
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
pub async fn sweep(app: &App) -> anyhow::Result<()> {
    // A transaction lock spans the entire run, including slow Discord rate-limit waits.
    let Some(_guard) = app.db.claim_mod_parity_sweep().await? else {
        return Ok(());
    };
    let bucket = app.now().timestamp() / 60;
    if app
        .cache
        .try_cooldown(&format!("modparity:autopurge:{bucket}"), Duration::from_secs(120))
        .await?
        .is_some()
    {
        return Ok(());
    }
    let restrictions: Vec<(i64, i64)> =
        sqlx::query_as("SELECT guild_id,user_id FROM mod_restrictions ORDER BY guild_id,user_id")
            .fetch_all(app.db.pool())
            .await?;
    for (g, user) in restrictions {
        if let Err(error) = enforce_restriction(app, g as u64, user as u64).await {
            tracing::warn!(%error, g, user, "restriction recovery failed");
        }
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
                let cursor_key = format!("modparity:purge-cursor:{g}:{ch}:{}:{}", rule.age_secs, rule.bots_only);
                let cutoff = (((app.now() - chrono::Duration::seconds(rule.age_secs)).timestamp_millis()
                    - 1_420_070_400_000) as u64)
                    << 22;
                let before = app
                    .cache
                    .get(&cursor_key)
                    .await?
                    .and_then(|s| s.parse::<u64>().ok())
                    .filter(|id| *id > 0)
                    .map_or(cutoff, |id| id.min(cutoff));
                let messages = app
                    .http
                    .channel_messages(Id::new(ch))
                    .before(Id::new(before))
                    .limit(100)
                    .await?
                    .models()
                    .await?;
                // Advance even if this page only contains pinned or nonmatching authors.
                let next = (messages.len() == 100).then(|| messages.last().unwrap().id.get());
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
                if let Some(next) = next {
                    app.cache
                        .put(&cursor_key, &next.to_string(), Duration::from_secs(172800))
                        .await?;
                } else {
                    app.cache.forget(&cursor_key).await?;
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
    let captured = chrono::Utc::now().timestamp_millis();
    let task = app.clone();
    app.spawn(async move {
        let entries: Vec<_> = states
            .iter()
            .map(|v| {
                (
                    v.user_id.to_string(),
                    format!("{}|0|{captured}", v.channel_id.map_or(0, |id| id.get())),
                )
            })
            .collect();
        if let Err(error) = task
            .cache
            .map_merge(
                &format!("modparity:voice-v2:{g}"),
                &entries,
                captured,
                Duration::from_secs(172800),
            )
            .await
        {
            tracing::warn!(%error, g, "voice snapshot failed");
            return;
        }
        for mut v in states {
            v.guild_id = Some(Id::new(g));
            let current = task
                .cache
                .map_all(&format!("modparity:voice-v2:{g}"))
                .await
                .unwrap_or_default();
            v.channel_id = current
                .iter()
                .find(|(id, _)| id == &v.user_id.to_string())
                .and_then(|(_, value)| voice_channel(value))
                .and_then(|id| id.parse().ok())
                .map(Id::new);
            if let Err(error) = enforce_voice(&task, &v).await {
                tracing::warn!(%error, g, "voice recovery failed");
            }
        }
    });
}
