use super::*;

/// Pays this minute's voice XP, unless another tick (a second pod, or this process before a restart)
/// already did. Returns how many members were paid.
pub async fn tick(app: &Arc<App>) -> anyhow::Result<usize> {
    pay_minute(app, app.now().timestamp().div_euclid(60)).await
}

/// Pays `minute` to every guild with somebody in voice, a few guilds at a time.
pub(super) async fn pay_minute(app: &Arc<App>, minute: i64) -> anyhow::Result<usize> {
    if !app.cache.once(&format!("voice-tick:{minute}"), MINUTE_LOCK).await? {
        return Ok(0);
    }
    let guilds: Vec<u64> = app
        .cache
        .set_members(GUILDS)
        .await?
        .iter()
        .filter_map(|guild| guild.parse().ok())
        .collect();
    let slots = Arc::new(Semaphore::new(PARALLEL));
    let mut jobs = JoinSet::new();
    for guild in guilds {
        let slot = slots.clone().acquire_owned().await?;
        let app = app.clone();
        jobs.spawn(async move {
            let paid = pay_guild(&app, guild).await;
            drop(slot);
            (guild, paid)
        });
    }
    let mut paid = 0;
    while let Some(done) = jobs.join_next().await {
        match done {
            Ok((_, Ok(n))) => paid += n,
            Ok((guild, Err(error))) => tracing::warn!(guild, %error, "voice XP failed for a server"),
            Err(error) => tracing::error!(%error, "voice XP task failed"),
        }
    }
    Ok(paid)
}

/// Stops ticking `guild`, unless a voice event refilled its hash between the empty read and the
/// removal: that member would otherwise sit unpaid until the next event in the server.
pub(super) async fn forget_guild(app: &App, guild: u64, key: &str) -> anyhow::Result<()> {
    app.cache.set_remove(GUILDS, &guild.to_string()).await?;
    // Only a member who is in voice counts: the notes of those who left are no reason to keep ticking.
    let someone = app
        .cache
        .map_all(key)
        .await?
        .iter()
        .any(|(_, value)| Record::decode(value).is_some_and(|r| !r.has_left()));
    if someone {
        app.cache.set_add(GUILDS, &guild.to_string()).await?;
    }
    Ok(())
}

/// A level-up earned in voice. Failures are logged: the XP is already paid.
pub(super) async fn announce(
    app: &Arc<App>,
    settings: &GuildSettings,
    guild: u64,
    user: u64,
    levels: (u32, u32),
    record: &Record,
) {
    let author = app.user(user).await;
    let result = leveling::level_up(
        app,
        settings,
        LevelUp {
            guild,
            user,
            before: levels.0,
            after: levels.1,
            author: author.as_deref(),
            nick: None,
            roles: &record.roles,
            here: Id::new(record.channel.max(1)),
        },
    )
    .await;
    if let Err(error) = result {
        tracing::warn!(guild, user, %error, "voice level-up not announced");
    }
}
