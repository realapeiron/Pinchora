use super::*;

pub fn spawn_ticker(app: Arc<App>) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(2));
        let mut sweeps = 0_u64;
        loop {
            interval.tick().await;
            if let Err(error) = tick_with_periodic(&app, sweeps.is_multiple_of(30)).await {
                tracing::warn!(%error, "starboard reconciliation failed");
            }
            sweeps += 1;
            if sweeps.is_multiple_of(43_200)
                && let Err(error) = app.db.sweep_star_posts().await
            {
                tracing::warn!(%error, "starboard sweep failed");
            }
        }
    });
}

pub async fn tick(app: &App) -> anyhow::Result<()> {
    tick_with_periodic(app, true).await
}

async fn tick_with_periodic(app: &App, periodic: bool) -> anyhow::Result<()> {
    for (channel, copy) in app.db.due_star_cleanups().await? {
        cleanup_one(app, channel, copy).await?;
    }
    for (board_id, message, token, force) in app.db.due_star_posts(100, periodic).await? {
        let version = app.db.star_sync_version(board_id, message as u64).await?;
        let result = async {
            let Some(board) = app.db.starboard(board_id).await? else {
                return Ok(());
            };
            let Some(post) = app.db.star_post(board_id, message as u64).await? else {
                return Ok(());
            };
            reconcile_post(app, &board, &post, force).await
        }
        .await;
        if let Some(version) = version {
            app.db
                .finish_star_sync(board_id, message as u64, token, version, result.is_ok())
                .await?;
        }
        if let Err(error) = result {
            tracing::warn!(board_id, message, %error, "starboard post sync will retry");
        }
    }
    Ok(())
}

pub async fn cleanup_one(app: &App, channel: i64, copy: i64) -> anyhow::Result<()> {
    if copy < 0 {
        let source = (-copy) as u64;
        let destination = Id::new(channel as u64);
        let mut before = None;
        for _ in 0..20 {
            let response = match before {
                Some(id) => app.http.channel_messages(destination).before(id).limit(100).await,
                None => app.http.channel_messages(destination).limit(100).await,
            };
            let messages = match response {
                Ok(response) => response.models().await?,
                Err(error) if unknown_message(&error) => {
                    app.db.finish_star_cleanup(channel, copy).await?;
                    return Ok(());
                }
                Err(error) => return Err(error.into()),
            };
            if let Some(found) = messages.iter().find(|message| {
                message.author.id.get() == app.application_id.get()
                    && message.embeds.iter().any(|embed| {
                        embed
                            .description
                            .as_deref()
                            .is_some_and(|description| description.ends_with(&format!("/{source}")))
                    })
            }) {
                if !app
                    .db
                    .recover_ambiguous_star_post(channel as u64, source, found.id.get())
                    .await?
                {
                    app.db.queue_star_cleanup(channel as u64, found.id.get()).await?;
                }
                app.db.finish_star_cleanup(channel, copy).await?;
                return Ok(());
            }
            if messages.len() < 100 {
                break;
            }
            before = messages.last().map(|message| message.id);
        }
        return Ok(());
    }
    match app
        .http
        .delete_message(Id::new(channel as u64), Id::new(copy as u64))
        .await
    {
        Ok(_) => app.db.finish_star_cleanup(channel, copy).await?,
        Err(error) if unknown_message(&error) => app.db.finish_star_cleanup(channel, copy).await?,
        Err(error) => tracing::warn!(channel, copy, %error, "starboard copy delete will retry"),
    }
    Ok(())
}
