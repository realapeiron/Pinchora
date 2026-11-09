use chrono::{Duration, Utc};
use pinchora_db::{Db, FeedAdd, NewFeed};

#[tokio::test]
async fn source_and_post_claims_lease_only_the_requested_batch() {
    let _isolation = crate::support::shared().await;
    let Some((_, guild)) = setup().await else {
        return;
    };
    let db = Db::connect_waiting(
        &std::env::var("PINCHORA_TEST_DATABASE_URL").unwrap(),
        1,
        std::time::Duration::from_secs(5),
    )
    .await
    .unwrap();
    // Exercise the valid nested-loop plan that used to re-evaluate the LIMIT subquery.
    for setting in [
        "SET enable_hashjoin=off",
        "SET enable_mergejoin=off",
        "SET enable_material=off",
        "SET plan_cache_mode=force_generic_plan",
    ] {
        sqlx::query(sqlx::AssertSqlSafe(setting))
            .execute(db.pool())
            .await
            .unwrap();
    }
    for n in 0..3 {
        let source = format!("https://batch-{guild}-{n}.example/feed");
        let FeedAdd::Added(feed) = db
            .add_feed(&NewFeed {
                guild,
                kind: "rss",
                source: &source,
                title: None,
                channel: guild + 1,
                message: None,
                embed: true,
                ping_role: None,
                live_role: None,
                include: &[],
                exclude: &[],
                created_by: guild,
                seen: &[],
                interval_secs: 120,
            })
            .await
            .unwrap()
        else {
            panic!("feed not added");
        };
        for i in 0..3 {
            let item = pinchora_core::feeds::Item {
                id: format!("item-{i}"),
                ..Default::default()
            };
            sqlx::query("INSERT INTO feed_outbox (feed_id,item_id,item) VALUES ($1,$2,$3)")
                .bind(feed.id)
                .bind(&item.id)
                .bind(serde_json::to_value(&item).unwrap())
                .execute(db.pool())
                .await
                .unwrap();
        }
    }
    sqlx::query("UPDATE feed_sources SET next_poll_at=now()-interval '1 minute' WHERE source IN (SELECT source FROM feeds WHERE guild_id=$1)")
        .bind(guild as i64).execute(db.pool()).await.unwrap();
    for _ in 0..3 {
        assert_eq!(
            db.claim_due_sources(Utc::now(), 1, Some(&[guild])).await.unwrap().len(),
            1
        );
    }
    assert!(
        db.claim_due_sources(Utc::now(), 1, Some(&[guild]))
            .await
            .unwrap()
            .is_empty()
    );
    for expected in 1..=9_i64 {
        let posts = db.claim_feed_posts(1, Some(&[guild])).await.unwrap();
        assert_eq!(posts.len(), 1, "LIMIT 1 must not lease multiple posts");
        let leased: i64 = sqlx::query_scalar("SELECT count(*) FROM feed_outbox WHERE feed_id IN (SELECT id FROM feeds WHERE guild_id=$1) AND lease_until IS NOT NULL")
            .bind(guild as i64).fetch_one(db.pool()).await.unwrap();
        assert_eq!(leased, expected, "unreturned posts must remain available to workers");
    }
    assert!(db.claim_feed_posts(1, Some(&[guild])).await.unwrap().is_empty());
}

async fn setup() -> Option<(Db, u64)> {
    let url = std::env::var("PINCHORA_TEST_DATABASE_URL").ok()?;
    let db = Db::connect_waiting(&url, 8, std::time::Duration::from_secs(5))
        .await
        .unwrap();
    db.migrate().await.unwrap();
    let id = crate::support::id();
    Some((db, id))
}

#[tokio::test]
async fn codes_are_member_channel_bound_single_use_and_expired_claims_are_released() {
    let _isolation = crate::support::shared().await;
    let Some((db, user)) = setup().await else { return };
    let source = format!("live{user}");
    assert!(db.start_channel_link(user, "twitch", &source, "one").await.unwrap());
    assert!(
        !db.verify_channel_link(user + 1, "twitch", &source, "one")
            .await
            .unwrap()
    );
    assert!(
        !db.verify_channel_link(user, "twitch", "another-channel", "one")
            .await
            .unwrap()
    );
    assert!(!db.verify_channel_link(user, "kick", &source, "one").await.unwrap());
    assert!(!db.start_channel_link(user + 1, "twitch", &source, "two").await.unwrap());
    sqlx::query("UPDATE feed_channel_links SET code_expires_at = now() - interval '1 second' WHERE user_id = $1")
        .bind(user as i64)
        .execute(db.pool())
        .await
        .unwrap();
    assert!(!db.verify_channel_link(user, "twitch", &source, "one").await.unwrap());
    assert!(db.start_channel_link(user + 1, "twitch", &source, "two").await.unwrap());
    assert!(db.channel_links(user).await.unwrap().is_empty());
    let (a, b) = tokio::join!(
        db.verify_channel_link(user + 1, "twitch", &source, "two"),
        db.verify_channel_link(user + 1, "twitch", &source, "two")
    );
    assert_ne!(a.unwrap(), b.unwrap());
    assert_eq!(db.linked_streamer("twitch", &source).await.unwrap(), Some(user + 1));
    assert!(!db.start_channel_link(user, "twitch", &source, "steal").await.unwrap());
    let row = db.channel_links(user + 1).await.unwrap().pop().unwrap();
    assert!(row.verification_code.is_none() && row.code_expires_at.is_none());
    db.delete_user_data(user + 1, Utc::now()).await.unwrap();
    assert!(db.channel_links(user + 1).await.unwrap().is_empty());
}

#[tokio::test]
async fn webhook_replay_is_atomic_and_cannot_break_a_poll_lease() {
    let _isolation = crate::support::shared().await;
    let Some((db, user)) = setup().await else { return };
    let source = format!("live{user}");
    let feed = db
        .add_feed(&NewFeed {
            guild: user,
            kind: "twitch",
            source: &source,
            title: None,
            channel: user + 1,
            message: None,
            embed: true,
            ping_role: None,
            live_role: None,
            include: &[],
            exclude: &[],
            created_by: user,
            seen: &[],
            interval_secs: 120,
        })
        .await
        .unwrap();
    assert!(matches!(feed, FeedAdd::Added(_)));
    db.set_live_stream("twitch", &source, &user.to_string(), None)
        .await
        .unwrap();
    assert_eq!(
        db.twitch_subscription_source(&user.to_string()).await.unwrap(),
        Some(source.clone())
    );
    assert!(db.twitch_subscription_source("unknown").await.unwrap().is_none());
    sqlx::query("UPDATE feed_sources SET next_poll_at = now() - interval '1 minute' WHERE source = $1")
        .bind(&source)
        .execute(db.pool())
        .await
        .unwrap();
    assert_eq!(
        db.claim_due_sources(Utc::now(), 1, Some(&[user])).await.unwrap().len(),
        1
    );
    let message = format!("message-{user}");
    let (a, b) = tokio::join!(
        db.record_twitch_webhook(&message, &source),
        db.record_twitch_webhook(&message, &source)
    );
    assert_ne!(a.unwrap(), b.unwrap());
    assert!(
        db.claim_due_sources(Utc::now() + Duration::seconds(1), 1, Some(&[user]))
            .await
            .unwrap()
            .is_empty()
    );
    db.mark_eventsub_registered(&source).await.unwrap();
    let revocation = format!("revoke-{user}");
    db.record_twitch_revocation(&revocation, &user.to_string())
        .await
        .unwrap();
    db.mark_eventsub_registered(&source).await.unwrap();
    db.record_twitch_revocation(&revocation, &user.to_string())
        .await
        .unwrap();
    let registered: bool =
        sqlx::query_scalar("SELECT eventsub_registered_at IS NOT NULL FROM feed_sources WHERE source = $1")
            .bind(&source)
            .fetch_one(db.pool())
            .await
            .unwrap();
    assert!(registered, "a replay must not revoke the replacement subscription");
    // Even concurrent live polls can queue only one go-live post for the same stream.
    let items = [pinchora_core::feeds::Item {
        id: format!("stream-{user}"),
        published: Some(Utc::now()),
        ..Default::default()
    }];
    let polled = pinchora_db::Polled {
        kind: "twitch",
        source: &source,
        title: None,
        etag: None,
        last_modified: None,
        interval_secs: 120,
        next_poll_at: Utc::now() + Duration::minutes(2),
        items: &items,
        now: Utc::now(),
    };
    let (a, b) = tokio::join!(db.record_feed_poll(&polled), db.record_feed_poll(&polled));
    assert_eq!(a.unwrap() + b.unwrap(), 1);
    let guilds = [user];
    let (a, b) = tokio::join!(
        db.claim_feed_posts(1, Some(&guilds)),
        db.claim_feed_posts(1, Some(&guilds))
    );
    assert_eq!(a.unwrap().len() + b.unwrap().len(), 1);
    sqlx::query("DELETE FROM feeds WHERE guild_id = $1")
        .bind(user as i64)
        .execute(db.pool())
        .await
        .unwrap();
    sqlx::query("DELETE FROM feed_sources WHERE source = $1")
        .bind(&source)
        .execute(db.pool())
        .await
        .unwrap();
}
