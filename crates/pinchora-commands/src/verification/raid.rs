//! Reversible raid transitions: save the before-image before touching Discord.
use super::safety;
use crate::{
    App,
    commands::moderation::channels::{LOCKED, lock_change},
    server_events::{button, row},
};
use chrono::Duration as Span;
use pinchora_core::{modules::Module, t, verification::Config};
use pinchora_db::{RaidEvent, RaidJoin};
use serde::{Deserialize, Serialize};
use std::{sync::Arc, time::Duration};
use twilight_model::{
    channel::message::component::ButtonStyle,
    guild::{Permissions, VerificationLevel},
    http::permission_overwrite::{PermissionOverwrite, PermissionOverwriteType},
    id::Id,
};
#[derive(Debug, Default, Serialize, Deserialize)]
struct Snapshot {
    level: u8,
    raised: bool,
    paused: bool,
    channels: Vec<Channel>,
    window_secs: u32,
}
#[derive(Debug, Serialize, Deserialize)]
struct Channel {
    id: u64,
    slow: u16,
    applied_slow: u16,
    allow: u64,
    deny: u64,
    locked: bool,
}

pub(super) async fn joined(
    app: &Arc<App>,
    guild: u64,
    join: &RaidJoin,
    c: &Config,
) -> anyhow::Result<Option<RaidEvent>> {
    let fresh = app.db.note_raid_join(join).await?;
    let active = app.db.active_raid(guild).await?;
    if active.is_some() || !fresh || !c.raid.enabled || !app.module_allowed(guild, None, Module::AntiRaid).await {
        return Ok(active);
    }
    let r = &c.raid;
    let count = app
        .cache
        .raid_window(guild, join.user_id as u64, app.now().timestamp_millis(), r.window_secs)
        .await?;
    let recent = app
        .db
        .raid_joins_since(guild, app.now() - Span::seconds(i64::from(r.window_secs)))
        .await?;
    let similar = recent
        .iter()
        .filter(|j| {
            (r.similar_names && !join.name_family.is_empty() && j.name_family == join.name_family)
                || (r.similar_avatars && join.avatar.is_some() && j.avatar == join.avatar)
                || (r.new_account_hours > 0 && join.young && j.young)
        })
        .count();
    if count >= r.join_threshold || similar >= r.similar_threshold as usize {
        // The database unique active index remains the authority after this short cross-pod lease.
        if app.cache.once(&format!("raid:{guild}"), Duration::from_secs(5)).await? {
            start(app, guild, None, r.lockdown_minutes, count as i32, &r.action).await?;
        }
    }
    app.db.active_raid(guild).await
}
pub async fn start(
    app: &Arc<App>,
    guild: u64,
    actor: Option<u64>,
    minutes: u32,
    joins: i32,
    action: &str,
) -> anyhow::Result<()> {
    let lang = app.guild_lang(guild).await;
    anyhow::ensure!((1..=1440).contains(&minutes), t!(lang, "verification.invalid"));
    let Some(_guard) = super::lock(app, &format!("raid:{guild}")).await? else {
        anyhow::bail!(t!(lang, "verification.busy"))
    };
    if app.db.active_raid(guild).await?.is_some() {
        return Ok(());
    }
    let c = app.db.safety_config(guild).await?;
    let r = &c.raid;
    let mut snapshot = Snapshot {
        window_secs: r.window_secs,
        ..Default::default()
    };
    if action != "alert" {
        safety::permission(app, guild, Permissions::MANAGE_GUILD, "manage_guild").await?;
        let g = app.http.guild(Id::new(guild)).await?.model().await?;
        snapshot.level = g.verification_level.into();
        snapshot.raised = snapshot.level < 3;
        snapshot.paused = r.pause_invites
            && !g
                .features
                .contains(&twilight_model::guild::GuildFeature::InvitesDisabled);
        if !r.channels.is_empty() {
            safety::permission(app, guild, Permissions::MANAGE_CHANNELS, "manage_channels").await?;
        }
        for id in &r.channels {
            let ch = app.http.channel(Id::new(id.parse()?)).await?.model().await?;
            anyhow::ensure!(
                ch.guild_id.map(|g| g.get()) == Some(guild),
                t!(lang, "verification.invalid")
            );
            let overwrite = ch.permission_overwrites.iter().flatten().find(|o| o.id.get() == guild);
            let (allow, deny) = overwrite.map_or((Permissions::empty(), Permissions::empty()), |o| (o.allow, o.deny));
            snapshot.channels.push(Channel {
                id: ch.id.get(),
                slow: ch.rate_limit_per_user.unwrap_or(0),
                applied_slow: ch.rate_limit_per_user.unwrap_or(0).max(r.slowmode_secs as u16),
                allow: allow.bits(),
                deny: deny.bits(),
                locked: r.lock_channels && lock_change(allow, deny, true, None).is_some(),
            });
        }
    }
    let Some(event) = app
        .db
        .open_raid(
            guild,
            joins,
            action,
            app.now(),
            app.now() + Span::minutes(i64::from(minutes)),
            &serde_json::to_value(&snapshot)?,
        )
        .await?
    else {
        return Ok(());
    };
    if let Err(error) = apply(app, guild, &snapshot).await {
        app.db.raid_error(event.id, Some(&error.to_string())).await?;
        let _ = announce(app, &event, actor).await;
        return Err(error);
    }
    announce(app, &event, actor).await?;
    if matches!(action, "kick" | "ban") {
        act_window(app, guild, event.id, action == "ban", actor).await?;
    }
    Ok(())
}
async fn apply(app: &App, guild: u64, s: &Snapshot) -> anyhow::Result<()> {
    let mut errors = Vec::new();
    if s.raised
        && let Err(error) = app
            .http
            .update_guild(Id::new(guild))
            .verification_level(Some(VerificationLevel::High))
            .await
    {
        errors.push(error.to_string());
    }
    if s.paused {
        // Discord does not expose invite pausing on every guild. Its refusal must not skip channel protection.
        let pause: anyhow::Result<()> = async {
            let g = app.http.guild(Id::new(guild)).await?.model().await?;
            let names: Vec<String> = g
                .features
                .into_iter()
                .map(|f| std::borrow::Cow::<'static, str>::from(f).into_owned())
                .collect();
            let mut features: Vec<&str> = names.iter().map(String::as_str).collect();
            if !features.contains(&"INVITES_DISABLED") {
                features.push("INVITES_DISABLED");
            }
            app.http.update_guild(Id::new(guild)).features(&features).await?;
            Ok(())
        }
        .await;
        if let Err(error) = pause {
            errors.push(error.to_string());
        }
    }
    for ch in &s.channels {
        if let Err(error) = app
            .http
            .update_channel(Id::new(ch.id))
            .rate_limit_per_user(ch.applied_slow)
            .await
        {
            errors.push(error.to_string());
        }
        if ch.locked {
            let (a, d) = lock_change(
                Permissions::from_bits_truncate(ch.allow),
                Permissions::from_bits_truncate(ch.deny),
                true,
                None,
            )
            .expect("captured unlocked channel");
            if let Err(error) = overwrite(app, guild, ch.id, a, d).await {
                errors.push(error.to_string());
            }
        }
    }
    anyhow::ensure!(errors.is_empty(), "{}", errors.join("; "));
    Ok(())
}
async fn overwrite(app: &App, guild: u64, channel: u64, allow: Permissions, deny: Permissions) -> anyhow::Result<()> {
    if allow.is_empty() && deny.is_empty() {
        app.http
            .delete_channel_permission(Id::new(channel))
            .role(Id::new(guild))
            .await?;
    } else {
        app.http
            .update_channel_permission(
                Id::new(channel),
                &PermissionOverwrite {
                    allow: Some(allow),
                    deny: Some(deny),
                    id: Id::new(guild),
                    kind: PermissionOverwriteType::Role,
                },
            )
            .await?;
    }
    Ok(())
}
pub async fn unlock(app: &Arc<App>, guild: u64, actor: u64, expected: Option<i64>) -> anyhow::Result<()> {
    let lang = app.guild_lang(guild).await;
    let Some(_guard) = super::lock(app, &format!("raid:{guild}")).await? else {
        anyhow::bail!(t!(lang, "verification.busy"))
    };
    let Some(event) = app.db.active_raid(guild).await? else {
        return Ok(());
    };
    anyhow::ensure!(
        expected.is_none_or(|id| id == event.id),
        t!(lang, "verification.expired")
    );
    let s: Snapshot = serde_json::from_value(event.snapshot.clone())?;
    if let Err(error) = restore(app, guild, &s).await {
        app.db.raid_error(event.id, Some(&error.to_string())).await?;
        return Err(error);
    }
    app.db.end_raid(event.id, actor, app.now()).await?;
    safety::log(
        app,
        guild,
        actor,
        None,
        "raid.unlock",
        &t!(lang, "anti_raid.ended", id = event.id),
        vec![],
    )
    .await
}
async fn restore(app: &App, guild: u64, s: &Snapshot) -> anyhow::Result<()> {
    let mut errors = Vec::new();
    let guild_restore: anyhow::Result<()> = async {
        if s.raised || s.paused {
            let g = app.http.guild(Id::new(guild)).await?.model().await?;
            if s.raised && g.verification_level == VerificationLevel::High {
                app.http
                    .update_guild(Id::new(guild))
                    .verification_level(Some(VerificationLevel::from(s.level)))
                    .await?;
            }
            if s.paused
                && g.features
                    .contains(&twilight_model::guild::GuildFeature::InvitesDisabled)
            {
                let names: Vec<String> = g
                    .features
                    .into_iter()
                    .map(|f| std::borrow::Cow::<'static, str>::from(f).into_owned())
                    .collect();
                let features: Vec<&str> = names
                    .iter()
                    .map(String::as_str)
                    .filter(|f| *f != "INVITES_DISABLED")
                    .collect();
                app.http.update_guild(Id::new(guild)).features(&features).await?;
            }
        }
        Ok(())
    }
    .await;
    if let Err(error) = guild_restore {
        errors.push(error.to_string());
    }
    for ch in &s.channels {
        let restored: anyhow::Result<()> = async {
            let current = match app.http.channel(Id::new(ch.id)).await {
                Ok(r) => r.model().await?,
                Err(e) if crate::roles::error_code(&e) == Some(10003) => return Ok(()),
                Err(e) => return Err(e.into()),
            };
            if current.rate_limit_per_user.unwrap_or(0) == ch.applied_slow {
                app.http
                    .update_channel(Id::new(ch.id))
                    .rate_limit_per_user(ch.slow)
                    .await?;
            }
            if ch.locked {
                let current = current
                    .permission_overwrites
                    .iter()
                    .flatten()
                    .find(|o| o.id.get() == guild);
                if let Some(current) = current {
                    // Preserve unrelated edits made by staff during the raid.
                    if current.deny.contains(LOCKED) && !current.allow.intersects(LOCKED) {
                        let allow = (current.allow - LOCKED) | (Permissions::from_bits_truncate(ch.allow) & LOCKED);
                        let deny = (current.deny - LOCKED) | (Permissions::from_bits_truncate(ch.deny) & LOCKED);
                        overwrite(app, guild, ch.id, allow, deny).await?;
                    }
                }
            }
            Ok(())
        }
        .await;
        if let Err(error) = restored {
            errors.push(error.to_string());
        }
    }
    anyhow::ensure!(errors.is_empty(), "{}", errors.join("; "));
    Ok(())
}
async fn announce(app: &App, event: &RaidEvent, actor: Option<u64>) -> anyhow::Result<()> {
    let guild = event.guild_id as u64;
    let lang = app.guild_lang(guild).await;
    let controls = row(["unlock", "kick", "ban"]
        .into_iter()
        .map(|a| {
            button(
                format!("rd:{a}:{}", event.id),
                &t!(lang, &format!("anti_raid.{a}")),
                None,
                if a == "unlock" {
                    ButtonStyle::Secondary
                } else {
                    ButtonStyle::Danger
                },
                false,
            )
        })
        .collect());
    safety::log(
        app,
        guild,
        actor.unwrap_or(app.application_id.get()),
        None,
        "raid.start",
        &t!(
            lang,
            "anti_raid.alert",
            id = event.id,
            joins = event.joins,
            action = t!(lang, &format!("anti_raid.action.{}", event.action))
        ),
        vec![controls],
    )
    .await
}
pub(super) async fn act_window(
    app: &Arc<App>,
    guild: u64,
    id: i64,
    ban: bool,
    actor: Option<u64>,
) -> anyhow::Result<()> {
    let lang = app.guild_lang(guild).await;
    let event = app
        .db
        .raid_event(guild, id)
        .await?
        .filter(|e| e.ended_at.is_none())
        .ok_or_else(|| anyhow::anyhow!(t!(lang, "verification.expired")))?;
    let c = app.db.safety_config(guild).await?;
    let s: Snapshot = serde_json::from_value(event.snapshot)?;
    for j in app
        .db
        .raid_joins_since(guild, event.started_at - Span::seconds(i64::from(s.window_secs)))
        .await?
    {
        if j.joined_at > event.started_at || matches!(j.status.as_str(), "kicked" | "banned" | "verified") {
            continue;
        }
        let Some(_guard) = super::lock(app, &format!("member:{guild}:{}", j.user_id)).await? else {
            continue;
        };
        if let Err(error) = safety::punish(app, guild, j.user_id as u64, &c, ban, actor).await {
            app.db.raid_error(id, Some(&error.to_string())).await?;
        }
    }
    Ok(())
}
