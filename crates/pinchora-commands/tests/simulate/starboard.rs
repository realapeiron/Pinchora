//! Starboard and vote channel setup through the real slash-command handler.

use super::*;

#[tokio::test]
async fn starboard_setup_list_top_and_vote_channels() {
    let Some(mut sim) = Sim::new("starboard_setup_list_top_and_vote_channels").await else {
        return;
    };
    let admin = unique();
    let board_channel = unique();
    let vote_channel = unique();
    sim.permissions = MANAGE_GUILD;
    let saved = sim
        .command_resolved(
            admin,
            "starboard",
            json!([sub(
                "setup",
                vec![
                    text_option("name", "Best of Reef"),
                    channel_option("channel", board_channel),
                    int_option("threshold", 2),
                ]
            )]),
            resolved_channels(&[board_channel]),
        )
        .await;
    assert!(
        saved.ephemeral && saved.content().contains("now posts"),
        "{}",
        saved.content()
    );
    let listed = sim.command(admin, "starboard", json!([sub("list", vec![])])).await;
    assert!(listed.content().contains("Best of Reef"), "{}", listed.content());
    let top = sim.command(admin, "starboard", json!([sub("top", vec![])])).await;
    assert!(
        top.description().contains("No starred posts yet"),
        "{}",
        top.description()
    );
    let board = sim.app.db.starboards(sim.guild).await.unwrap().remove(0);
    sim.permissions = 0;
    let denied = sim.component(unique(), &format!("sr:{}:1", board.id), &[], &top).await;
    assert!(denied.ephemeral && denied.content().contains("Manage Server"));
    sim.permissions = MANAGE_GUILD;
    let mut foreign = board.clone();
    foreign.id = 0;
    foreign.guild_id = unique() as i64;
    foreign.channel_id = unique() as i64;
    foreign.name = "Foreign secret".into();
    let foreign = sim.app.db.save_starboard(&foreign).await.unwrap();
    let forged = sim.component(admin, &format!("sr:{}:1", foreign.id), &[], &top).await;
    assert!(!forged.description().contains("Foreign secret"));
    let vote = sim
        .command_resolved(
            admin,
            "starboard",
            json!([group(
                "votes",
                sub(
                    "add",
                    vec![
                        channel_option("channel", vote_channel),
                        text_option("up", "▲"),
                        text_option("down", "▼"),
                    ]
                )
            )]),
            resolved_channels(&[vote_channel]),
        )
        .await;
    assert!(vote.content().contains("Voting is on"), "{}", vote.content());
    let votes = sim
        .command(admin, "starboard", json!([group("votes", sub("list", vec![]))]))
        .await;
    assert!(votes.content().contains("▲ / ▼"), "{}", votes.content());
}

#[tokio::test]
async fn vote_channel_puts_up_before_down() {
    use pinchora_commands::on_starboard_message;
    let Some(sim) = Sim::new("vote_channel_puts_up_before_down").await else {
        return;
    };
    let channel = unique();
    sim.app
        .db
        .save_vote(&pinchora_db::VoteChannel {
            channel_id: channel as i64,
            guild_id: sim.guild as i64,
            up_emoji: "▲".into(),
            down_emoji: "▼".into(),
            only_media: false,
            create_thread: false,
            enabled: true,
        })
        .await
        .unwrap();
    let seen = Arc::new(Mutex::new(Vec::<String>::new()));
    let routes = Router::new().fallback({
        let seen = seen.clone();
        move |uri: Uri, method: Method| {
            let seen = seen.clone();
            async move {
                if method == Method::PUT && uri.path().contains("/reactions/") {
                    seen.lock().unwrap().push(uri.path().to_owned());
                    (StatusCode::NO_CONTENT, String::new())
                } else {
                    (StatusCode::NOT_FOUND, String::new())
                }
            }
        }
    });
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    tokio::spawn(axum::serve(listener, routes).into_future());
    let http = Arc::new(
        Client::builder()
            .token("Bot sim".into())
            .proxy(address.to_string(), true)
            .ratelimiter(None)
            .build(),
    );
    let mut app = App::new(
        http.clone(),
        http,
        Id::new(APPLICATION_ID),
        sim.app.db.clone(),
        sim.app.cache.clone(),
        "vote-sim".into(),
    );
    app.message_content = true;
    let app = Arc::new(app);
    let mut message = sim.chat(unique(), &[]).message;
    message.channel_id = Id::new(channel);
    on_starboard_message(&app, &message).await;
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            if seen.lock().unwrap().len() >= 2 {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    let paths = seen.lock().unwrap().clone();
    assert!(paths[0].contains("%E2%96%B2") || paths[0].contains('▲'), "{paths:?}");
    assert!(paths[1].contains("%E2%96%BC") || paths[1].contains('▼'), "{paths:?}");
}

#[tokio::test]
async fn vote_thread_flood_obeys_channel_budget() {
    use pinchora_commands::on_starboard_message;
    let Some(sim) = Sim::new("vote_thread_flood_obeys_channel_budget").await else {
        return;
    };
    let channel = unique();
    sim.app
        .db
        .save_vote(&pinchora_db::VoteChannel {
            channel_id: channel as i64,
            guild_id: sim.guild as i64,
            up_emoji: "▲".into(),
            down_emoji: "▼".into(),
            only_media: false,
            create_thread: true,
            enabled: true,
        })
        .await
        .unwrap();
    let threads = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let routes = Router::new().fallback({
        let threads = threads.clone();
        move |uri: Uri, method: Method| {
            let threads = threads.clone();
            async move {
                if method == Method::PUT && uri.path().contains("/reactions/") {
                    (StatusCode::NO_CONTENT, String::new())
                } else if method == Method::POST && uri.path().ends_with("/threads") {
                    threads.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    (StatusCode::CREATED, "{}".into())
                } else {
                    (StatusCode::NOT_FOUND, String::new())
                }
            }
        }
    });
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    tokio::spawn(axum::serve(listener, routes).into_future());
    let http = Arc::new(
        Client::builder()
            .token("Bot sim".into())
            .proxy(address.to_string(), true)
            .ratelimiter(None)
            .build(),
    );
    let app = Arc::new(App::new(
        http.clone(),
        http,
        Id::new(APPLICATION_ID),
        sim.app.db.clone(),
        sim.app.cache.clone(),
        "vote-flood".into(),
    ));
    for _ in 0..10 {
        let mut message = sim.chat(unique(), &[]).message;
        message.channel_id = Id::new(channel);
        on_starboard_message(&app, &message).await;
    }
    tokio::time::sleep(std::time::Duration::from_millis(700)).await;
    assert!(threads.load(std::sync::atomic::Ordering::SeqCst) <= 1);
}

#[tokio::test]
async fn starboard_rankings_use_the_players_number_format() {
    let Some(mut sim) = Sim::new("starboard_rankings_use_the_players_number_format").await else {
        return;
    };
    let admin = unique();
    let channel = unique();
    sim.permissions = MANAGE_GUILD;
    sim.command_resolved(
        admin,
        "starboard",
        json!([sub(
            "setup",
            vec![text_option("name", "Yıldızlar"), channel_option("channel", channel)]
        )]),
        resolved_channels(&[channel]),
    )
    .await;
    let board = sim.app.db.starboards(sim.guild).await.unwrap().remove(0);
    let message = unique();
    sim.app
        .db
        .star_change(board.id, message, CHANNEL_ID, admin, unique(), Some(1))
        .await
        .unwrap();
    sqlx::query("UPDATE starboard_posts SET star_count=1234 WHERE starboard_id=$1 AND message_id=$2")
        .bind(board.id)
        .bind(message as i64)
        .execute(sim.app.db.pool())
        .await
        .unwrap();
    for (locale, count, title) in [
        ("tr", "1.234", "En çok yıldız alanlar"),
        ("en-US", "1,234", "Top posts"),
    ] {
        sim.locale = locale;
        let top = sim.command(admin, "starboard", json!([sub("top", vec![])])).await;
        assert!(top.title().starts_with(title), "{}", top.title());
        assert!(
            top.description().contains(&format!("⭐ {count} ·")),
            "{}",
            top.description()
        );
        assert!(
            top.description().contains(&format!("/{message}")),
            "message IDs stay literal"
        );
    }
}
