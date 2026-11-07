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
    #[serde(default)]
    raised_done: bool,
    #[serde(default)]
    paused_done: bool,
    channels: Vec<Channel>,
    window_secs: u32,
    #[serde(default)]
    uncertain: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Channel {
    id: u64,
    slow: u16,
    applied_slow: u16,
    allow: u64,
    deny: u64,
    locked: bool,
    #[serde(default)]
    existed: bool,
    #[serde(default)]
    slow_done: bool,
    #[serde(default)]
    lock_done: bool,
}

pub(super) async fn joined(
    app: &Arc<App>,
    guild: u64,
    join: &RaidJoin,
    c: &Config,
) -> anyhow::Result<Option<RaidEvent>> {
    let fresh = app.db.note_raid_join(join).await?;
    let active = app.db.active_raid(guild).await?;
    if active.as_ref().is_some_and(|e| e.action != "alert")
        || !fresh
        || !c.raid.enabled
        || !app.module_allowed(guild, None, Module::AntiRaid).await
    {
        return Ok(active);
    }
    let r = &c.raid;
    let count = app
        .cache
        .raid_window(guild, join.user_id as u64, app.now().timestamp_millis(), r.window_secs)
        .await?;
    // A longer correlated window catches coordinated slow drips. Youth alone is not identity evidence.
    let horizon = r.window_secs.saturating_mul(10).min(3600);
    let recent = app
        .db
        .raid_joins_since(guild, app.now() - Span::seconds(i64::from(horizon)))
        .await?;
    let similar = recent.iter().filter(|j| correlated(r, join, j)).count();
    let fast_similar = recent
        .iter()
        .filter(|j| j.joined_at >= app.now() - Span::seconds(i64::from(r.window_secs)) && correlated(r, join, j))
        .count();
    let coordinated =
        fast_similar >= r.similar_threshold as usize || similar >= r.similar_threshold.saturating_mul(2) as usize;
    if coordinated || count >= r.join_threshold {
        // Volume alone on a busy day only alerts; destructive actions require independent correlation.
        let action = if coordinated { r.action.as_str() } else { "alert" };
        if active.as_ref().is_none_or(|e| e.action != action)
            && app
                .cache
                .once(&format!("raid:{guild}:{action}"), Duration::from_secs(5))
                .await?
        {
            if let Some(e) = active {
                unlock(app, guild, app.application_id.get(), Some(e.id)).await?;
            }
            if let Err(error) = start(app, guild, None, r.lockdown_minutes, count as i32, action).await {
                // Partial Discord failures leave a durable raid. Still create this member's
                // pending gate below, otherwise the triggering join has no way to verify.
                if let Some(event) = app.db.active_raid(guild).await? {
                    tracing::warn!(guild,%error,"raid partially applied; holding triggering member");
                    return Ok(Some(event));
                }
                return Err(error);
            }
        }
    }
    app.db.active_raid(guild).await
}
fn correlated(r: &pinchora_core::anti_raid::RaidSettings, a: &RaidJoin, b: &RaidJoin) -> bool {
    (r.similar_names
        && a.name_family.chars().count() >= 4
        && a.name_family == b.name_family
        && (r.new_account_hours == 0 || (a.young && b.young)))
        || (r.similar_avatars && a.avatar.is_some() && a.avatar == b.avatar)
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
        let g = safety::request(app, app.http.guild(Id::new(guild)))
            .await?
            .model()
            .await?;
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
            let ch = safety::request(app, app.http.channel(Id::new(id.parse()?)))
                .await?
                .model()
                .await?;
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
                existed: overwrite.is_some(),
                slow_done: false,
                lock_done: false,
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
    if let Err(error) = apply(app, guild, event.id, &mut snapshot).await {
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
async fn save_snapshot(app: &App, id: i64, s: &Snapshot) -> anyhow::Result<()> {
    app.db.raid_snapshot(id, &serde_json::to_value(s)?).await
}
// A crash or transport timeout cannot prove whether Discord accepted a write. Keep that
// intent durable rather than claiming restoration succeeded or touching an unowned change.
async fn transition<T, E>(
    app: &App,
    id: i64,
    s: &mut Snapshot,
    key: String,
    request: impl std::future::IntoFuture<Output = Result<T, E>>,
) -> anyhow::Result<T>
where
    E: Into<anyhow::Error>,
{
    s.uncertain.push(key.clone());
    save_snapshot(app, id, s).await?;
    match safety::request(app, request).await {
        Ok(value) => {
            s.uncertain.retain(|k| k != &key);
            Ok(value)
        }
        Err(error) => {
            if error.downcast_ref::<twilight_http::Error>().is_some_and(|e| matches!(e.kind(), twilight_http::error::ErrorType::Response {status,..} if (400..500).contains(&status.get()))) {
                s.uncertain.retain(|k| k != &key);
                save_snapshot(app, id, s).await?;
            }
            Err(error)
        }
    }
}
async fn apply(app: &App, guild: u64, id: i64, s: &mut Snapshot) -> anyhow::Result<()> {
    let mut errors = Vec::new();
    if s.raised {
        match transition(
            app,
            id,
            s,
            "verification_level".into(),
            app.http
                .update_guild(Id::new(guild))
                .verification_level(Some(VerificationLevel::High)),
        )
        .await
        {
            Ok(_) => {
                s.raised_done = true;
                save_snapshot(app, id, s).await?;
            }
            Err(error) => errors.push(error.to_string()),
        }
    }
    if s.paused {
        let pause: anyhow::Result<()> = async {
            let g = safety::request(app, app.http.guild(Id::new(guild)))
                .await?
                .model()
                .await?;
            let names: Vec<String> = g
                .features
                .into_iter()
                .map(|f| std::borrow::Cow::<'static, str>::from(f).into_owned())
                .collect();
            let mut features: Vec<&str> = names.iter().map(String::as_str).collect();
            if features.contains(&"INVITES_DISABLED") {
                return Ok(());
            }
            features.push("INVITES_DISABLED");
            transition(
                app,
                id,
                s,
                "invites".into(),
                app.http.update_guild(Id::new(guild)).features(&features),
            )
            .await?;
            s.paused_done = true;
            Ok(())
        }
        .await;
        if let Err(error) = pause {
            errors.push(error.to_string());
        }
        save_snapshot(app, id, s).await?;
    }
    for i in 0..s.channels.len() {
        let ch = &s.channels[i];
        if ch.slow != ch.applied_slow {
            let (channel, slow) = (ch.id, ch.applied_slow);
            match transition(
                app,
                id,
                s,
                format!("slowmode:{channel}"),
                app.http.update_channel(Id::new(channel)).rate_limit_per_user(slow),
            )
            .await
            {
                Ok(_) => {
                    s.channels[i].slow_done = true;
                    save_snapshot(app, id, s).await?;
                }
                Err(error) => errors.push(error.to_string()),
            }
        }
        let ch = &s.channels[i];
        if ch.locked {
            let (a, d) = lock_change(
                Permissions::from_bits_retain(ch.allow),
                Permissions::from_bits_retain(ch.deny),
                true,
                None,
            )
            .expect("captured unlocked channel");
            let channel = ch.id;
            match transition(
                app,
                id,
                s,
                format!("overwrite:{channel}"),
                app.http.update_channel_permission(
                    Id::new(channel),
                    &PermissionOverwrite {
                        allow: Some(a),
                        deny: Some(d),
                        id: Id::new(guild),
                        kind: PermissionOverwriteType::Role,
                    },
                ),
            )
            .await
            {
                Ok(_) => {
                    s.channels[i].lock_done = true;
                    save_snapshot(app, id, s).await?;
                }
                Err(error) => errors.push(error.to_string()),
            }
        }
    }
    anyhow::ensure!(errors.is_empty(), "{}", errors.join("; "));
    Ok(())
}
async fn overwrite(
    app: &App,
    guild: u64,
    channel: u64,
    allow: Permissions,
    deny: Permissions,
    existed: bool,
) -> anyhow::Result<()> {
    if !existed && allow.is_empty() && deny.is_empty() {
        safety::request(
            app,
            app.http
                .delete_channel_permission(Id::new(channel))
                .role(Id::new(guild)),
        )
        .await?;
    } else {
        safety::request(
            app,
            app.http.update_channel_permission(
                Id::new(channel),
                &PermissionOverwrite {
                    allow: Some(allow),
                    deny: Some(deny),
                    id: Id::new(guild),
                    kind: PermissionOverwriteType::Role,
                },
            ),
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
    let mut s: Snapshot = serde_json::from_value(event.snapshot.clone())?;
    if let Err(error) = restore(app, guild, event.id, &mut s).await {
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
async fn restore(app: &App, guild: u64, id: i64, s: &mut Snapshot) -> anyhow::Result<()> {
    anyhow::ensure!(
        s.uncertain.is_empty(),
        "{}",
        t!(
            app.guild_lang(guild).await,
            "anti_raid.uncertain",
            operations = s.uncertain.join(", ")
        )
    );
    let mut errors = Vec::new();
    let guild_restore: anyhow::Result<()> = async {
        if s.raised_done || s.paused_done {
            let g = safety::request(app, app.http.guild(Id::new(guild)))
                .await?
                .model()
                .await?;
            if s.raised_done && g.verification_level == VerificationLevel::High {
                let level = VerificationLevel::from(s.level);
                transition(
                    app,
                    id,
                    s,
                    "restore:verification_level".into(),
                    app.http.update_guild(Id::new(guild)).verification_level(Some(level)),
                )
                .await?;
            }
            s.raised_done = false;
            save_snapshot(app, id, s).await?;
            if s.paused_done
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
                transition(
                    app,
                    id,
                    s,
                    "restore:invites".into(),
                    app.http.update_guild(Id::new(guild)).features(&features),
                )
                .await?;
            }
            s.paused_done = false;
            save_snapshot(app, id, s).await?;
        }
        Ok(())
    }
    .await;
    if let Err(error) = guild_restore {
        errors.push(error.to_string());
    }
    for i in 0..s.channels.len() {
        let ch = s.channels[i].clone();
        if !ch.slow_done && !ch.lock_done {
            continue;
        }
        let restored: anyhow::Result<()> = async {
            let current = match safety::request(app, app.http.channel(Id::new(ch.id))).await {
                Ok(r) => r.model().await?,
                Err(e)
                    if e.downcast_ref::<twilight_http::Error>()
                        .and_then(crate::roles::error_code)
                        == Some(10003) =>
                {
                    s.channels[i].slow_done = false;
                    s.channels[i].lock_done = false;
                    save_snapshot(app, id, s).await?;
                    return Ok(());
                }
                Err(e) => return Err(e),
            };
            if ch.slow_done && current.rate_limit_per_user.unwrap_or(0) == ch.applied_slow {
                transition(
                    app,
                    id,
                    s,
                    format!("restore:slowmode:{}", ch.id),
                    app.http.update_channel(Id::new(ch.id)).rate_limit_per_user(ch.slow),
                )
                .await?;
            }
            s.channels[i].slow_done = false;
            save_snapshot(app, id, s).await?;
            if ch.lock_done {
                let current = current
                    .permission_overwrites
                    .iter()
                    .flatten()
                    .find(|o| o.id.get() == guild);
                if let Some(current) = current {
                    // Preserve unrelated edits made by staff during the raid.
                    if current.deny.contains(LOCKED) && !current.allow.intersects(LOCKED) {
                        let allow = (current.allow - LOCKED) | (Permissions::from_bits_retain(ch.allow) & LOCKED);
                        let deny = (current.deny - LOCKED) | (Permissions::from_bits_retain(ch.deny) & LOCKED);
                        transition(
                            app,
                            id,
                            s,
                            format!("restore:overwrite:{}", ch.id),
                            overwrite(app, guild, ch.id, allow, deny, ch.existed),
                        )
                        .await?;
                    }
                }
            }
            s.channels[i].lock_done = false;
            save_snapshot(app, id, s).await?;
            Ok(())
        }
        .await;
        if let Err(error) = restored {
            errors.push(error.to_string());
        }
    }
    if !s.uncertain.is_empty() {
        errors.push(t!(
            app.guild_lang(guild).await,
            "anti_raid.uncertain",
            operations = s.uncertain.join(", ")
        ));
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
            joins = pinchora_core::server_stats::grouped_in(lang, i64::from(event.joins)),
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
    let Some(_batch) = super::lock(app, &format!("raid-actions:{guild}")).await? else {
        anyhow::bail!(t!(lang, "verification.busy"));
    };
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
        if app.db.active_raid(guild).await?.is_none_or(|e| e.id != id) {
            break;
        }
        if app.db.raid_join(guild, j.user_id as u64).await?.is_none_or(|current| {
            current.joined_at != j.joined_at || matches!(current.status.as_str(), "kicked" | "banned" | "verified")
        }) {
            continue;
        }
        if let Err(error) = safety::punish(app, guild, j.user_id as u64, &c, ban, actor).await {
            app.db.raid_error(id, Some(&error.to_string())).await?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn youth_and_short_common_names_are_not_raid_identity() {
        let r = pinchora_core::anti_raid::RaidSettings::default();
        let mut a = RaidJoin {
            guild_id: 1,
            user_id: 1,
            joined_at: chrono::Utc::now(),
            name_family: "ada".into(),
            avatar: None,
            young: true,
            status: "joined".into(),
        };
        let mut b = a.clone();
        assert!(!correlated(&r, &a, &b));
        b.name_family = "efe".into();
        assert!(!correlated(&r, &a, &b), "being young is not enough");
        a.name_family = "raider".into();
        b.name_family = "raider".into();
        assert!(correlated(&r, &a, &b));
        b.young = false;
        assert!(!correlated(&r, &a, &b));
        a.avatar = Some("same-custom-avatar".into());
        b.avatar = a.avatar.clone();
        assert!(correlated(&r, &a, &b));
    }
}
