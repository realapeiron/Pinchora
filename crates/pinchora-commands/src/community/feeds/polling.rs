use super::*;

/// What one step of the ticker did.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FeedsTick {
    pub polled: usize,
    pub queued: usize,
    pub posted: usize,
}

/// One step: polls the sources that are due and sends the posts that are waiting.
pub async fn tick(app: &Arc<App>) -> anyhow::Result<FeedsTick> {
    tick_for(app, None).await
}

/// [`tick`] for the sources and posts of `guilds` only (`None`: every server), for tests that share a
/// database with others.
pub async fn tick_for(app: &Arc<App>, guilds: Option<&[u64]>) -> anyhow::Result<FeedsTick> {
    let due = app.db.claim_due_sources(app.now(), POLL_BATCH, guilds).await?;
    let polled = due.len();
    let (twitch, other): (Vec<_>, Vec<_>) = due.into_iter().partition(|row| row.kind == "twitch");
    let mut queued: usize = futures_util::stream::iter(other)
        .map(|source| poll(app, source))
        .buffer_unordered(POLL_AT_ONCE)
        .collect::<Vec<usize>>()
        .await
        .into_iter()
        .sum();
    if !twitch.is_empty() {
        let names: Vec<String> = twitch.iter().map(|row| row.source.clone()).collect();
        let reads = if available(Kind::Twitch) {
            crate::community::feeds::live::lookup_twitch_batch(&names).await
        } else {
            Err(anyhow::anyhow!("Twitch credentials are missing"))
        };
        match reads {
            Ok(mut reads) => {
                for row in twitch {
                    if let Some(read) = reads.remove(&row.source) {
                        queued += poll_live_read(app, &row, Kind::Twitch, read).await;
                    } else {
                        live_failure(app, &row).await;
                    }
                }
            }
            Err(error) => {
                tracing::warn!(%error, "batched Twitch poll failed");
                for row in twitch {
                    if available(Kind::Twitch) {
                        live_failure(app, &row).await;
                    } else {
                        live_unavailable(app, &row).await;
                    }
                }
            }
        }
    }
    let posted = deliver(app, guilds).await?;
    Ok(FeedsTick { polled, queued, posted })
}

/// Fetches one leased source and records the outcome; returns the posts it queued.
async fn poll(app: &App, row: FeedSourceRow) -> usize {
    let now = app.now();
    let Some(kind) = Kind::parse(&row.kind) else {
        let later = now + Span::seconds(BACKOFF_MAX_SECS);
        let _ = app
            .db
            .record_feed_failure(&row.kind, &row.source, "feeds.error.parse", later)
            .await;
        return 0;
    };
    if matches!(kind, Kind::Twitch | Kind::Kick) {
        return poll_live(app, &row, kind).await;
    }
    // A little spread, so sources added together do not stay in step.
    let spread = |secs: i64| Span::seconds(secs + rand::random_range(0..=secs / 10));
    let result = fetch_source(kind, &row.source, row.etag.as_deref(), row.last_modified.as_deref()).await;
    let recorded = match result {
        Ok(None) => {
            let next = now + spread(i64::from(row.interval_secs));
            app.db
                .record_feed_unchanged(&row.kind, &row.source, next)
                .await
                .map(|()| 0)
        }
        Ok(Some(read)) => {
            let interval = poll_interval(kind, read.parsed.ttl);
            app.db
                .record_feed_poll(&Polled {
                    kind: &row.kind,
                    source: &row.source,
                    title: read.parsed.title.as_deref(),
                    etag: read.etag.as_deref(),
                    last_modified: read.last_modified.as_deref(),
                    interval_secs: interval,
                    next_poll_at: now + spread(i64::from(interval)),
                    items: &read.parsed.items,
                    now,
                })
                .await
        }
        Err(problem) => {
            let wait = match problem {
                Problem::Fetch(FetchError::RateLimited(Some(secs))) => {
                    (secs as i64).clamp(i64::from(row.interval_secs), BACKOFF_MAX_SECS)
                }
                _ => backoff_secs(row.interval_secs, row.fail_count + 1),
            };
            tracing::debug!(kind = %row.kind, source = %row.source, problem = %problem.code(), "feed fetch failed");
            app.db
                .record_feed_failure(&row.kind, &row.source, &problem.code(), now + Span::seconds(wait))
                .await
                .map(|()| 0)
        }
    };
    recorded.unwrap_or_else(|error| {
        tracing::warn!(%error, kind = %row.kind, source = %row.source, "feed poll not recorded");
        0
    })
}

async fn poll_live(app: &App, row: &FeedSourceRow, kind: Kind) -> usize {
    if !available(kind) {
        live_unavailable(app, row).await;
        return 0;
    }
    let read = match crate::community::feeds::live::lookup(kind, &row.source).await {
        Ok(read) => read,
        Err(error) => {
            tracing::warn!(%error, source = %row.source, "live source lookup failed");
            live_failure(app, row).await;
            return 0;
        }
    };
    poll_live_read(app, row, kind, read).await
}

async fn live_unavailable(app: &App, row: &FeedSourceRow) {
    let later = app.now() + Span::seconds(i64::from(row.interval_secs));
    let code = format!("feeds.unavailable.{}", row.kind);
    if let Err(error) = app.db.record_feed_failure(&row.kind, &row.source, &code, later).await {
        tracing::warn!(%error, source = %row.source, "unavailable live source was not recorded");
    }
}

async fn live_failure(app: &App, row: &FeedSourceRow) {
    let later = app.now() + Span::seconds(backoff_secs(row.interval_secs, row.fail_count + 1));
    if let Err(error) = app
        .db
        .record_feed_failure(&row.kind, &row.source, "feeds.error.network", later)
        .await
    {
        tracing::warn!(%error, source = %row.source, "live source failure was not recorded");
    }
}

async fn poll_live_read(
    app: &App,
    row: &FeedSourceRow,
    kind: Kind,
    read: crate::community::feeds::live::LiveRead,
) -> usize {
    let now = app.now();
    let stream_id = read.item.as_ref().map(|item| item.id.as_str());
    match app
        .db
        .set_live_stream(&row.kind, &row.source, &read.broadcaster_id, stream_id)
        .await
    {
        Ok(ended) => {
            for feed in ended {
                end_live(app, &feed).await;
            }
        }
        Err(error) => tracing::warn!(%error, "live stream state was not saved"),
    }
    if kind == Kind::Twitch
        && row.eventsub_registered_at.is_none()
        && crate::community::feeds::live::webhook_callback().is_some()
    {
        match crate::community::feeds::live::subscribe_twitch(&read.broadcaster_id).await {
            Ok(()) => {
                if let Err(error) = app.db.mark_eventsub_registered(&row.source).await {
                    tracing::warn!(%error, "EventSub registration was not recorded");
                }
            }
            Err(error) => {
                tracing::warn!(%error, source = %row.source, "EventSub registration failed; polling continues")
            }
        }
    }
    if read.item.is_some()
        && let Ok(feeds) = app.db.live_feeds_for_channel(&row.kind, &row.source).await
    {
        for feed in feeds {
            change_live_role(app, &feed, true).await;
        }
    }
    let items: Vec<Item> = read.item.into_iter().collect();
    match app
        .db
        .record_feed_poll(&Polled {
            kind: &row.kind,
            source: &row.source,
            title: Some(&read.title),
            etag: None,
            last_modified: None,
            interval_secs: kind.interval_secs(),
            next_poll_at: now + Span::seconds(i64::from(kind.interval_secs())),
            items: &items,
            now,
        })
        .await
    {
        Ok(queued) => queued,
        Err(error) => {
            tracing::warn!(%error, "live poll was not recorded");
            0
        }
    }
}

/// Sends the posts this pod can lease; returns how many went out.
async fn deliver(app: &App, guilds: Option<&[u64]>) -> anyhow::Result<usize> {
    let mut sent = 0;
    // Claim just before sending: a rate-limit wait for one post cannot age the others' leases.
    for _ in 0..POST_BATCH {
        let Some(post) = app.db.claim_feed_posts(1, guilds).await?.pop() else {
            break;
        };
        let send = deliver_one(app, &post);
        tokio::pin!(send);
        let mut heartbeat = tokio::time::interval(Duration::from_secs(30));
        heartbeat.tick().await;
        let outcome = loop {
            tokio::select! {
                outcome = &mut send => break outcome,
                _ = heartbeat.tick() => {
                    if !app.db.renew_feed_post(&post).await? {
                        // Another pod reclaimed it. The in-flight Discord request may still
                        // finish, but its stable nonce deduplicates the competing send.
                        break PostOutcome::Dropped;
                    }
                }
            }
        };
        if outcome == PostOutcome::Sent {
            crate::admin::workflows::observe(
                app,
                post.feed.guild_id as u64,
                0,
                "feed_item",
                Some(post.feed.channel_id as u64),
            )
            .await;
            sent += 1;
        }
        app.db.finish_feed_post(&post, &outcome).await?;
    }
    Ok(sent)
}

async fn deliver_one(app: &App, post: &FeedPost) -> PostOutcome {
    if !post.feed.enabled
        || Kind::parse(&post.feed.kind).is_none_or(|kind| !available(kind))
        || !app.module_enabled(post.feed.guild_id as u64, Module::Feeds).await
    {
        return PostOutcome::Dropped;
    }
    send(app, &post.feed, &post.item, true).await
}

/// Posts `item` in the feed's channel; `ping` lets the feed's role be pinged (a test post pings nobody).
pub async fn send(app: &App, feed: &FeedRow, item: &Item, ping: bool) -> PostOutcome {
    let Some(draft) = Draft::of(feed) else {
        return PostOutcome::Dropped;
    };
    let lang = app.guild_lang(feed.guild_id as u64).await;
    let message = render(lang, &draft, item);
    let mentions = AllowedMentions {
        roles: draft.ping_role.filter(|_| ping).map(Id::new).into_iter().collect(),
        ..AllowedMentions::default()
    };
    let mut embeds: Vec<_> = message.embed.iter().map(|e| e.to_embed(draft.kind)).collect();
    app.brand_embeds(feed.guild_id as u64, &mut embeds).await;
    // Twilight 0.18 has `nonce` but omits Discord's `enforce_nonce` field. Its raw JSON
    // escape hatch sends the complete payload, including that field.
    let mut payload = delivery_payload(
        &message,
        &embeds,
        &mentions,
        ping.then(|| delivery_nonce(feed.id, &item.id)),
    );
    if matches!(draft.kind, Kind::Twitch | Kind::Kick)
        && let Some(url) = pinchora_core::community::feeds::page_url(draft.kind, &feed.source)
    {
        let mut value: serde_json::Value = serde_json::from_slice(&payload).expect("feed payload");
        value["components"] = serde_json::json!([{ "type": 1, "components": [{
            "type": 2, "style": 5, "label": t!(lang, "feeds.live.watch"), "url": url
        }]}]);
        payload = serde_json::to_vec(&value).expect("live payload");
    }
    let request = app
        .http
        .create_message(Id::new((feed.channel_id as u64).max(1)))
        .payload_json(&payload);
    match request.await {
        Ok(response) => {
            if ping && matches!(draft.kind, Kind::Twitch | Kind::Kick) {
                match response.model().await {
                    Ok(message) => match app.db.set_live_message(feed.id, message.id.get(), &item.id).await {
                        Ok(true) => {
                            change_live_role(app, feed, true).await;
                        }
                        Ok(false) => {
                            let mut ended = feed.clone();
                            ended.live_message_id = Some(message.id.get() as i64);
                            end_live(app, &ended).await;
                        }
                        Err(error) => tracing::warn!(%error, feed = feed.id, "live message id was not saved"),
                    },
                    Err(error) => tracing::warn!(%error, feed = feed.id, "live response was not decoded"),
                }
            }
            PostOutcome::Sent
        }
        Err(error) => {
            tracing::debug!(%error, feed = feed.id, "feed post refused");
            let code = match error_code(&error) {
                Some(50001 | 50013) => "feeds.error.post-permission",
                Some(10003) => "feeds.error.post-channel",
                _ if is_permanent(&error) => "feeds.error.post-refused",
                _ => return PostOutcome::Retry("feeds.error.post-down".to_owned()),
            };
            PostOutcome::Refused(code.to_owned())
        }
    }
}
