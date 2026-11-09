use axum::{
    Json, Router,
    body::Bytes,
    http::{Method, StatusCode, Uri},
    response::IntoResponse,
};
use pinchora_commands::{
    App, ReactionEvent, cleanup_starboard_copy, on_starboard_reaction, on_starboard_update, reconcile_starboard_post,
};
use pinchora_db::{Db, Starboard};
use serde_json::{Value, json};
use std::{
    future::IntoFuture,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
use twilight_http::Client;
use twilight_model::{
    gateway::payload::incoming::{MessageUpdate, ReactionAdd},
    id::Id,
};

async fn exercise(private: bool, reactor_bot: bool, content: bool, live: bool, unsafe_after: bool) -> Option<Value> {
    let _ = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::WARN)
        .try_init();
    let base = rand::random_range(1u64 << 58..1u64 << 59);
    let (guild, source, target, message, author, reactor, copy) =
        (base, base + 1, base + 2, base + 3, base + 4, base + 5, base + 6);
    let db = Db::connect_waiting(
        &std::env::var("PINCHORA_TEST_DATABASE_URL").unwrap(),
        3,
        Duration::from_secs(10),
    )
    .await
    .unwrap();
    let board = db
        .save_starboard(&Starboard {
            id: 0,
            guild_id: guild as i64,
            name: "Review".into(),
            channel_id: target as i64,
            emoji: "⭐".into(),
            threshold: 1,
            self_star: false,
            allow_bots: false,
            only_channels: vec![],
            except_channels: vec![],
            require_image: false,
            color: 0xffc857,
            role_weights: json!({}),
            remove_below: true,
            enabled: true,
        })
        .await
        .unwrap();
    let user = |id: u64, bot: bool| json!({"id":id.to_string(),"username":"review","discriminator":"0","avatar":null,"bot":bot});
    let original = json!({"id":message.to_string(),"channel_id":source.to_string(),"guild_id":guild.to_string(),"type":0,
        "author":user(author,false),"content":"staff secret", "timestamp":"2024-01-01T00:00:00+00:00","edited_timestamp":null,
        "tts":false,"mention_everyone":false,"mention_roles":[],"mentions":[],"pinned":false,"embeds":[],"flags":0,
        "attachments":[{"id":"123","filename":"secret.png","size":12,"url":"https://cdn.discordapp.com/attachments/secret.png",
        "proxy_url":"https://media.discordapp.net/attachments/secret.png","content_type":"image/png"}]});
    let posted: Arc<Mutex<Option<Value>>> = Arc::default();
    let source_nsfw = Arc::new(AtomicBool::new(false));
    let deleted = Arc::new(AtomicBool::new(false));
    let fail_delete = Arc::new(AtomicBool::new(unsafe_after));
    let capture = posted.clone();
    let nsfw_for_http = source_nsfw.clone();
    let deleted_for_http = deleted.clone();
    let fail_for_http = fail_delete.clone();
    let original_for_http = original.clone();
    let router = Router::new().fallback(move |uri: Uri, method: Method, body: Bytes| {
        let capture = capture.clone();
        let nsfw = nsfw_for_http.clone();
        let deleted = deleted_for_http.clone();
        let fail = fail_for_http.clone();
        let original = original_for_http.clone();
        async move {
            let path = uri.path();
            if method == Method::GET && (path.ends_with(&format!("/channels/{source}")) || path.ends_with(&format!("/channels/{target}"))) {
                let channel = if path.ends_with(&source.to_string()) { source } else { target };
                let audience = if channel == source { guild+10 } else { guild+11 };
                let overwrites = if private {json!([
                    {"id":guild.to_string(),"type":0,"allow":"0","deny":"1024"},
                    {"id":audience.to_string(),"type":0,"allow":"1024","deny":"0"}])} else {json!([])};
                return Json(json!({"id":channel.to_string(),"guild_id":guild.to_string(),"type":0,
                    "nsfw":channel==source && nsfw.load(Ordering::SeqCst),"permission_overwrites":overwrites})).into_response();
            }
            if method == Method::GET && path.ends_with("/roles") {
                return Json(json!([{"id":guild.to_string(),"name":"@everyone","permissions":"1024","color":0,"colors":{"primary_color":0,"secondary_color":null,"tertiary_color":null},"hoist":false,"managed":false,"mentionable":false,"position":0,"flags":0}])).into_response();
            }
            if method == Method::GET && path.ends_with(&format!("/messages/{message}")) {
                return Json(original).into_response();
            }
            if method == Method::GET && path.contains(&format!("/messages/{message}/reactions/")) {
                return Json(if live {json!([user(reactor, reactor_bot)])} else {json!([])}).into_response();
            }
            if method == Method::GET && path.ends_with(&format!("/members/{reactor}")) {
                return Json(json!({"user":user(reactor,reactor_bot),"roles":[(guild+10).to_string()],
                    "joined_at":"2024-01-01T00:00:00+00:00","deaf":false,"mute":false,"flags":0})).into_response();
            }
            if method == Method::POST && path.ends_with(&format!("/channels/{target}/messages")) {
                *capture.lock().unwrap() = Some(serde_json::from_slice(&body).unwrap());
                let mut response = original;
                response["id"] = json!(copy.to_string());
                response["channel_id"] = json!(target.to_string());
                return Json(response).into_response();
            }
            if method == Method::DELETE && path.ends_with(&format!("/channels/{target}/messages/{copy}")) {
                if fail.swap(false,Ordering::SeqCst) {
                    return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"message":"temporary","code":0}))).into_response();
                }
                deleted.store(true,Ordering::SeqCst);
                return StatusCode::NO_CONTENT.into_response();
            }
            (StatusCode::NOT_FOUND, format!("unknown review route {method} {path}")).into_response()
        }
    });
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(axum::serve(listener, router).into_future());
    let http = Arc::new(
        Client::builder()
            .token("Bot review".into())
            .proxy(addr.to_string(), true)
            .ratelimiter(None)
            .build(),
    );
    let cache = pinchora_cache::Cache::connect(&std::env::var("PINCHORA_TEST_REDIS_URL").unwrap())
        .await
        .unwrap();
    let mut app = App::new(http.clone(), http, Id::new(1), db.clone(), cache, "review".into());
    app.message_content = content;
    let event: ReactionAdd = serde_json::from_value(json!({"burst":false,"channel_id":source.to_string(),
        "emoji":{"id":null,"name":"⭐"},"guild_id":guild.to_string(),"message_id":message.to_string(),"user_id":reactor.to_string(),
        "member":{"user":user(reactor,reactor_bot),"roles":[(guild+10).to_string()],"joined_at":"2024-01-01T00:00:00+00:00","deaf":false,"mute":false,"flags":0}})).unwrap();
    let app = Arc::new(app);
    on_starboard_reaction(&app, &ReactionEvent::Add(Box::new(event))).await;
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            if db.star_post(board.id, message).await.unwrap().is_some() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("reaction queued");
    tokio::time::sleep(Duration::from_millis(2_100)).await;
    sqlx::query("UPDATE starboard_posts SET sync_due=now()-interval '1 day' WHERE starboard_id=$1 AND message_id=$2")
        .bind(board.id)
        .bind(message as i64)
        .execute(db.pool())
        .await
        .unwrap();
    reconcile_starboard_post(&app, board.id, message).await.unwrap();
    let result = posted.lock().unwrap().clone();
    if unsafe_after {
        assert!(result.is_some(), "public message first publishes");
        source_nsfw.store(true, Ordering::SeqCst);
        let event: MessageUpdate = serde_json::from_value(original.clone()).unwrap();
        on_starboard_update(&app, &event).await;
        tokio::time::sleep(Duration::from_millis(2_100)).await;
        sqlx::query(
            "UPDATE starboard_posts SET sync_due=now()-interval '1 day' WHERE starboard_id=$1 AND message_id=$2",
        )
        .bind(board.id)
        .bind(message as i64)
        .execute(db.pool())
        .await
        .unwrap();
        reconcile_starboard_post(&app, board.id, message).await.unwrap();
        sqlx::query(
            "UPDATE starboard_copy_cleanup SET due_at=now()-interval '1 day' WHERE channel_id=$1 AND copy_id=$2",
        )
        .bind(target as i64)
        .bind(copy as i64)
        .execute(db.pool())
        .await
        .unwrap();
        cleanup_starboard_copy(&app, target as i64, copy as i64).await.unwrap();
        assert!(!deleted.load(Ordering::SeqCst), "failed delete kept for retry");
        let pending: i64 =
            sqlx::query_scalar("SELECT count(*) FROM starboard_copy_cleanup WHERE channel_id=$1 AND copy_id=$2")
                .bind(target as i64)
                .bind(copy as i64)
                .fetch_one(db.pool())
                .await
                .unwrap();
        assert_eq!(pending, 1);
        cleanup_starboard_copy(&app, target as i64, copy as i64).await.unwrap();
        assert!(deleted.load(Ordering::SeqCst), "unsafe source copy deleted");
        assert!(
            db.star_post(board.id, message)
                .await
                .unwrap()
                .unwrap()
                .board_message_id
                .is_none()
        );
    }
    db.delete_starboard(guild, board.id).await.unwrap();
    server.abort();
    result
}

#[tokio::test]
async fn disjoint_private_audiences_do_not_leak() {
    let sent = exercise(true, false, true, true, false).await;
    assert!(
        sent.is_none(),
        "private source must not publish to a different audience"
    );
}

#[tokio::test]
async fn other_bot_reaction_does_not_post_human_message() {
    let sent = exercise(false, true, true, true, false).await;
    assert!(sent.is_none(), "bot reaction must not count when allow_bots is false");
}

#[tokio::test]
async fn attachments_are_not_copied_with_content_flag_off() {
    let sent = exercise(false, false, false, true, false).await;
    let sent = sent.expect("human public reaction posts metadata");
    assert!(!sent.to_string().contains("staff secret"));
    assert!(!sent.to_string().contains("secret.png"));
}

#[tokio::test]
async fn delayed_add_does_not_restore_a_removed_reaction() {
    let sent = exercise(false, false, true, false, false).await;
    assert!(sent.is_none(), "a delayed add must follow Discord's current reactions");
}

#[tokio::test]
async fn source_edit_into_nsfw_removes_public_copy() {
    exercise(false, false, true, true, true).await;
}
