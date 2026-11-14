use super::*;

static VOTE_SLOTS: LazyLock<Arc<Semaphore>> = LazyLock::new(|| Arc::new(Semaphore::new(32)));

pub async fn on_message(app: &Arc<App>, message: &Message) {
    let Some(guild) = message.guild_id.map(|g| g.get()) else {
        return;
    };
    if !app
        .module_allowed(guild, Some(message.channel_id.get()), Module::Starboard)
        .await
    {
        return;
    }
    let Ok(permit) = VOTE_SLOTS.clone().try_acquire_owned() else {
        tracing::warn!(guild, "vote channel queue full");
        return;
    };
    let app = app.clone();
    let msg = message.clone();
    app.clone().spawn(async move {
        let _permit = permit;
        if let Err(error) = react_vote(&app, &msg).await {
            tracing::warn!(guild, %error, "vote channel reaction failed");
        }
    });
}

async fn react_vote(app: &App, message: &Message) -> anyhow::Result<()> {
    let Some(vote) = app.db.vote(message.channel_id.get()).await? else {
        return Ok(());
    };
    if vote.only_media && (!app.message_content || !has_image(message) && message.attachments.is_empty()) {
        return Ok(());
    }
    let channel = message.channel_id;
    let id = message.id;
    app.http
        .create_reaction(channel, id, &reaction_type(&vote.up_emoji))
        .await?;
    app.http
        .create_reaction(channel, id, &reaction_type(&vote.down_emoji))
        .await?;
    if vote.create_thread {
        if app
            .cache
            .try_cooldown(
                &format!("vote-thread-user:{}:{}", vote.channel_id, message.author.id),
                Duration::from_secs(30),
            )
            .await?
            .is_some()
            || app
                .cache
                .try_cooldown(
                    &format!("vote-thread-channel:{}", vote.channel_id),
                    Duration::from_secs(6),
                )
                .await?
                .is_some()
        {
            return Ok(());
        }
        let lang = app.guild_lang(vote.guild_id as u64).await;
        app.http
            .create_thread_from_message(channel, id, &t!(lang, "starboard.vote.thread"))
            .await?;
    }
    Ok(())
}

pub async fn on_update(app: &Arc<App>, event: &MessageUpdate) {
    let Some(guild) = event.0.guild_id.map(|g| g.get()) else {
        return;
    };
    let app = app.clone();
    let message = event.0.clone();
    app.clone().spawn(async move {
        if let Err(error) = refresh(&app, guild, message.id.get()).await {
            tracing::warn!(%error, "starboard edit failed");
        }
    });
}

async fn refresh(app: &App, guild: u64, message: u64) -> anyhow::Result<()> {
    let posts = app.db.star_posts_for_source(guild, message).await?;
    for post in posts {
        app.db
            .touch_star_post(
                post.starboard_id,
                message,
                post.channel_id as u64,
                post.author_id as u64,
            )
            .await?;
    }
    Ok(())
}

pub(crate) async fn on_delete(app: &Arc<App>, event: &MessageDelete) {
    if let Some(guild) = event.guild_id {
        delete_source(app, guild.get(), event.id.get()).await;
    }
}

pub(crate) async fn on_delete_bulk(app: &Arc<App>, event: &MessageDeleteBulk) {
    if let Some(guild) = event.guild_id {
        for message in &event.ids {
            delete_source(app, guild.get(), message.get()).await;
        }
    }
}

async fn delete_source(app: &Arc<App>, guild: u64, message: u64) {
    let app = app.clone();
    app.clone().spawn(async move {
        match app.db.delete_star_source(guild, message).await {
            Ok(_) => {}
            Err(error) => tracing::warn!(%error, "starboard source delete failed"),
        }
    });
}
