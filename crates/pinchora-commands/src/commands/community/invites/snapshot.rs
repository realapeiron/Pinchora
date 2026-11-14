use super::*;

pub async fn refresh_snapshot(app: &Arc<App>, guild: u64) -> anyhow::Result<()> {
    let invites = fetch_invites(app, guild).await?;
    remember_vanity(app, guild).await;
    app.db.reset_invite_baseline(guild, &invites).await?;
    write_snapshot(app, guild, &invites).await
}

pub async fn refresh_created_snapshot(app: &Arc<App>, guild: u64) -> anyhow::Result<()> {
    let invites = fetch_invites(app, guild).await?;
    app.db.baseline_invites(guild, &invites).await?;
    write_snapshot(app, guild, &invites).await
}

pub async fn refresh_deleted_snapshot(app: &Arc<App>, guild: u64) -> anyhow::Result<()> {
    let invites = fetch_invites(app, guild).await?;
    write_snapshot(app, guild, &invites).await
}

async fn fetch_invites(app: &App, guild: u64) -> anyhow::Result<Vec<GuildInvite>> {
    let found = app.http.guild_invites(Id::new(guild)).await?.models().await?;
    Ok(found
        .into_iter()
        .map(|i| GuildInvite {
            code: i.code,
            inviter_id: i.inviter.map(|u| u.id.get() as i64),
            uses: i.uses.unwrap_or(0).min(i32::MAX as u64) as i32,
            max_uses: i.max_uses.unwrap_or(0).min(i32::MAX as u64) as i32,
            max_age: i.max_age.unwrap_or(0).min(i32::MAX as u64) as i32,
            created_at: i
                .created_at
                .and_then(|t| Utc.timestamp_opt(t.as_secs(), 0).single())
                .unwrap_or_else(Utc::now),
        })
        .collect())
}

async fn remember_vanity(app: &App, guild: u64) {
    if let Ok(response) = app.http.guild_vanity_url(Id::new(guild)).await
        && let Ok(vanity) = response.model().await
    {
        let _ = app
            .cache
            .put(
                &format!("invites:vanity:{guild}"),
                &vanity.uses.to_string(),
                Duration::from_secs(86400),
            )
            .await;
    }
}

async fn vanity_advanced(app: &App, guild: u64) -> bool {
    let key = format!("invites:vanity:{guild}");
    let Ok(response) = app.http.guild_vanity_url(Id::new(guild)).await else {
        return false;
    };
    let Ok(vanity) = response.model().await else {
        return false;
    };
    app.cache
        .claim_observed_value(&key, vanity.uses, Duration::from_secs(86400))
        .await
        .unwrap_or(false)
}

async fn write_snapshot(app: &App, guild: u64, invites: &[GuildInvite]) -> anyhow::Result<()> {
    let rows = invites
        .iter()
        .map(|i| (i.code.clone(), i.uses.to_string()))
        .collect::<Vec<_>>();
    app.cache
        .map_replace(&format!("invites:snapshot:{guild}"), &rows, Duration::from_secs(86400))
        .await
}

pub(crate) async fn on_member_join(app: &Arc<App>, event: &MemberAdd) {
    let app = app.clone();
    let event = event.clone();
    app.clone().spawn_event("invite_join", async move {
        if let Err(error) = track_join(&app, &event).await {
            tracing::warn!(guild=event.guild_id.get(), %error, "invite join tracking failed");
        }
    });
}

pub(crate) async fn on_member_leave(app: &Arc<App>, event: &MemberRemove) {
    let app = app.clone();
    let guild = event.guild_id.get();
    let user = event.user.id.get();
    app.clone().spawn_event("invite_leave", async move {
        let inviter = app.db.invite_inviter_for_member(guild, user).await.ok().flatten();
        if let Err(error) = app.db.leave_invite(guild, user).await {
            tracing::warn!(guild, %error, "invite leave tracking failed");
            return;
        }
        if let Some(inviter) = inviter
            && let Err(error) = reconcile_invite_rewards(&app, guild, inviter).await
        {
            tracing::warn!(guild, inviter, %error, "invite reward reconciliation failed");
        }
        if let Ok(settings) = app.db.invite_settings(guild).await
            && let Some(channel) = settings.log_channel_id
        {
            let content = t!(app.guild_lang(guild).await, "invites.leave-log", user = user);
            let _ = app.http.create_message(Id::new(channel as u64)).content(&content).await;
        }
    });
}

async fn track_join(app: &Arc<App>, event: &MemberAdd) -> anyhow::Result<()> {
    let guild = event.guild_id.get();
    let saved = app.db.invite_settings(guild).await?;
    if event.user.bot {
        return Ok(());
    }
    if !saved.enabled || !app.module_allowed(guild, None, Module::Invites).await {
        app.db.require_invite_baseline(guild).await?;
        return Ok(());
    }
    let now = event
        .joined_at
        .and_then(|joined| Utc.timestamp_opt(joined.as_secs(), 0).single())
        .unwrap_or_else(Utc::now);
    let fake = is_fake_account(event.user.id.get(), saved.fake_days, now);
    let mut rebaselined = false;
    if saved.baseline_required && !app.invite_baseline_pending(guild) {
        let invites = fetch_invites(app, guild).await?;
        rebaselined = app.db.reset_required_invite_baseline(guild, &invites).await?;
        if rebaselined {
            remember_vanity(app, guild).await;
            write_snapshot(app, guild, &invites).await?;
        }
    }
    let observed = if rebaselined || app.invite_baseline_pending(guild) {
        None
    } else {
        match fetch_invites(app, guild).await {
            Ok(v) => Some(v),
            Err(error) => {
                tracing::warn!(guild, %error, "Manage Server permission is required to read invites");
                None
            }
        }
    };
    let vanity_changed = if observed.is_some() {
        vanity_advanced(app, guild).await
    } else {
        false
    };
    let credit = app
        .db
        .record_invite_join(
            guild,
            event.user.id.get(),
            now,
            fake,
            InviteObservation {
                invites: observed.as_deref().unwrap_or(&[]),
                vanity_changed,
                list_available: observed.is_some(),
                preferred_code: None,
            },
        )
        .await?;
    if observed.is_some() {
        let credited = app.db.invite_snapshot(guild).await?;
        write_snapshot(app, guild, &credited).await?;
    }
    if let Some(channel) = saved.log_channel_id {
        let lang = app.guild_lang(guild).await;
        let source = match credit.source {
            "invite" => t!(lang, "invites.source.invite"),
            "vanity" => t!(lang, "invites.source.vanity"),
            "temporary" => t!(lang, "invites.source.temporary"),
            "widget" => t!(lang, "invites.source.widget"),
            _ => t!(lang, "invites.source.unknown"),
        };
        let content = t!(lang, "invites.join-log", user = event.user.id.get(), source = source);
        let _ = app.http.create_message(Id::new(channel as u64)).content(&content).await;
    }
    if let Some(inviter) = credit.inviter_id
        && let Err(error) = reconcile_invite_rewards(app, guild, inviter).await
    {
        tracing::warn!(guild, inviter, %error, "invite reward reconciliation failed");
    }
    Ok(())
}

