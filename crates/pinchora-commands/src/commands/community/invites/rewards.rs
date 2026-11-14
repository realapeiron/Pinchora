use super::*;

/// Re-check the role before Discord grants an invitation milestone.
pub async fn grant_invite_reward(app: &App, guild: u64, user: u64, role: u64) -> anyhow::Result<()> {
    let roles = app.http.roles(Id::new(guild)).await?.models().await?;
    let found = roles
        .iter()
        .find(|r| r.id.get() == role)
        .ok_or_else(|| anyhow::anyhow!("role deleted"))?;
    let mine = app
        .http
        .guild_member(Id::new(guild), Id::new(app.application_id.get()))
        .await?
        .model()
        .await?;
    let top = roles
        .iter()
        .filter(|r| mine.roles.contains(&r.id))
        .map(|r| r.position)
        .max()
        .unwrap_or(0);
    let facts = RoleFacts {
        everyone: role == guild,
        managed: found.managed,
        position: found.position,
        permissions: found.permissions.bits(),
    };
    if let Some(why) = settings::role_refusal(facts, Some(top), &format!("<@&{role}>"))
        .or_else(|| settings::run_time_refusal(found.permissions.bits(), &format!("<@&{role}>")))
    {
        anyhow::bail!("{why}");
    }
    app.http
        .add_guild_member_role(Id::new(guild), Id::new(user), Id::new(role))
        .await?;
    Ok(())
}

async fn grantor_can_grant(app: &App, guild: u64, role: &Role, authority: Option<i64>) -> anyhow::Result<bool> {
    let Some(authority) = authority else { return Ok(false) };
    let guild_data = app.http.guild(Id::new(guild)).await?.model().await?;
    let roles = app.http.roles(Id::new(guild)).await?.models().await?;
    let Some(role) = roles.iter().find(|current| current.id == role.id) else {
        return Ok(false);
    };
    let facts = RoleFacts {
        everyone: role.id.get() == guild,
        managed: role.managed,
        position: role.position,
        permissions: role.permissions.bits(),
    };
    let owner = guild_data.owner_id.get() == authority as u64;
    let member = match app.http.guild_member(Id::new(guild), Id::new(authority as u64)).await {
        Ok(response) => response.model().await.ok(),
        Err(_) => None,
    };
    let Some(member) = member else { return Ok(false) };
    let permissions = roles
        .iter()
        .filter(|r| r.id.get() == guild || member.roles.contains(&r.id))
        .fold(Permissions::empty(), |bits, r| bits | r.permissions);
    if !owner && !permissions.intersects(Permissions::MANAGE_GUILD | Permissions::ADMINISTRATOR) {
        return Ok(false);
    }
    let grantor = settings::Grantor {
        owner,
        administrator: permissions.contains(Permissions::ADMINISTRATOR),
        top_role: Some(
            roles
                .iter()
                .filter(|r| member.roles.contains(&r.id))
                .map(|r| r.position)
                .max()
                .unwrap_or(0),
        ),
    };
    Ok(settings::grant_refusal(facts, grantor, &format!("<@&{}>", role.id)).is_none())
}

pub(super) async fn reconcile_invite_rewards(app: &App, guild: u64, user: u64) -> anyhow::Result<()> {
    let guard = app.db.invite_reward_guard(guild, user).await?;
    let score = app.db.invite_score(guild, user).await?.total;
    let rewards = app.db.invite_rewards_with_grantor(guild).await?;
    let granted = app.db.invite_granted_roles(guild, user).await?;
    let member = app
        .http
        .guild_member(Id::new(guild), Id::new(user))
        .await?
        .model()
        .await?;
    let roles = app.http.roles(Id::new(guild)).await?.models().await?;
    let mut desired = std::collections::HashSet::new();
    for (threshold, role, authority) in &rewards {
        if score < i64::from(*threshold) {
            continue;
        }
        let Some(found) = roles.iter().find(|r| r.id.get() == *role as u64) else {
            continue;
        };
        if settings::run_time_refusal(found.permissions.bits(), &format!("<@&{role}>")).is_none()
            && grantor_can_grant(app, guild, found, *authority).await?
        {
            desired.insert(*role as u64);
        }
    }
    for role in granted {
        if !desired.contains(&role) {
            app.http
                .remove_guild_member_role(Id::new(guild), Id::new(user), Id::new(role))
                .await?;
            app.db.mark_invite_role(guild, user, role, false).await?;
        }
    }
    for role in desired {
        if member.roles.iter().any(|held| held.get() == role) {
            continue;
        }
        // Re-read the score before the remote write; a concurrent leave can lower it.
        let current_score = app.db.invite_score(guild, user).await?.total;
        let Some(found) = roles.iter().find(|r| r.id.get() == role) else {
            continue;
        };
        let mut authorized = false;
        for (_, _, authority) in rewards
            .iter()
            .filter(|(threshold, id, _)| *id as u64 == role && current_score >= i64::from(*threshold))
        {
            if grantor_can_grant(app, guild, found, *authority).await? {
                authorized = true;
                break;
            }
        }
        if !authorized {
            continue;
        }
        // Record ownership first so a crash after Discord accepts the role cannot
        // leave an untracked reward that future score drops fail to revoke.
        app.db.mark_invite_role(guild, user, role, true).await?;
        if let Err(error) = grant_invite_reward(app, guild, user, role).await {
            tracing::warn!(guild, user, role, %error, "invite reward refused");
            continue;
        }
    }
    guard.commit().await?;
    Ok(())
}

pub async fn reconcile_all_invite_rewards(app: &App, guild: u64) -> anyhow::Result<()> {
    for user in app.db.invite_reward_candidates(guild).await? {
        if let Err(error) = reconcile_invite_rewards(app, guild, user).await {
            tracing::warn!(guild, user, %error, "invite reward reconciliation failed");
        }
    }
    Ok(())
}

/// Repairs failed Discord grants and revokes roles after score, reward, or role changes.
pub fn spawn_reward_ticker(app: Arc<App>) {
    app.clone().spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(300));
        loop {
            interval.tick().await;
            match app.db.invite_reward_guilds().await {
                Ok(guilds) => {
                    for guild in guilds {
                        let key = format!("invites:reward-reconcile:{guild}");
                        if app.cache.once(&key, Duration::from_secs(240)).await.unwrap_or(false)
                            && let Err(error) = reconcile_all_invite_rewards(&app, guild).await
                        {
                            tracing::warn!(guild, %error, "invite reward sweep failed");
                        }
                    }
                }
                Err(error) => tracing::warn!(%error, "invite reward guild lookup failed"),
            }
        }
    });
}

