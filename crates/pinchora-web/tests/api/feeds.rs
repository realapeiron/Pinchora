//! The Feeds page over the API: sources looked up with the bot's own code (against a local feed
//! server, never the internet), feeds checked, saved, previewed, tested and audited.

use std::sync::OnceLock;

use pinchora_commands::community::feeds::Fetcher;
use pinchora_commands::community::feeds::install_fetcher;

use crate::*;

type Pages = Arc<Mutex<HashMap<String, String>>>;

/// The feed server the feed tests share, on its own thread, with the process's fetcher pointed at it.
fn pages() -> &'static Pages {
    static SERVER: OnceLock<Pages> = OnceLock::new();
    SERVER.get_or_init(|| {
        let pages = Pages::default();
        let served = pages.clone();
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("feed server runtime");
            runtime.block_on(async move {
                let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind the feed server");
                tx.send(listener.local_addr().expect("address")).unwrap();
                let router = Router::new().fallback(move |uri: Uri| {
                    let served = served.clone();
                    async move {
                        let key = uri.path_and_query().map_or(uri.path(), |p| p.as_str()).to_owned();
                        match served.lock().unwrap().get(&key) {
                            Some(body) => (StatusCode::OK, body.clone()),
                            None => (StatusCode::NOT_FOUND, String::new()),
                        }
                    }
                });
                axum::serve(listener, router).await.expect("feed server");
            });
        });
        install_fetcher(Fetcher::local(rx.recv().expect("address")));
        pages
    })
}

/// Serves an RSS feed with one item at `https://<host>/feed.xml`; returns the address.
fn serve_feed(host: &str, title: &str, item: &str) -> String {
    let at = chrono::Utc::now() - chrono::Duration::hours(1);
    let body = format!(
        r#"<?xml version="1.0"?><rss version="2.0"><channel><title>{title}</title><item><title>{item}</title>
<link>https://{host}/posts/1</link><guid>{host}-1</guid><pubDate>{}</pubDate>
<description>&lt;p&gt;The first lines.&lt;/p&gt;</description></item></channel></rss>"#,
        at.to_rfc2822()
    );
    pages().lock().unwrap().insert(format!("/{host}/feed.xml"), body);
    format!("https://{host}/feed.xml")
}

fn g(guild: u64) -> String {
    format!("/api/guilds/{guild}/feeds")
}

#[tokio::test]
async fn feeds_are_checked_saved_previewed_tested_and_removed() {
    let Some(h) = Harness::new("feeds_api").await else {
        return;
    };
    let (guild, owner, mut browser) = h.managed_server().await;
    let host = format!("reef{}.example", unique());
    let url = serve_feed(&host, "The Reef Times", "Coral **news**");

    let empty = browser.get(&g(guild)).await;
    assert_eq!(empty.status, StatusCode::OK, "{}", empty.text());
    let empty = empty.json();
    assert_eq!(empty["items"], json!([]));
    assert_eq!(empty["max"], 10);
    let kinds: Vec<&str> = empty["kinds"]
        .as_array()
        .unwrap()
        .iter()
        .map(|k| k["key"].as_str().unwrap())
        .collect();
    assert_eq!(
        kinds,
        ["youtube", "rss", "reddit", "bluesky", "bridge", "twitch", "kick"]
    );
    assert_eq!(empty["kinds"][4]["available"], false, "no bridge on this instance");

    // The check names the source and its latest post.
    let checked = browser
        .write(
            Method::POST,
            &format!("{}/check", g(guild)),
            json!({ "kind": "rss", "source": host.clone() + "/feed.xml" }),
        )
        .await;
    assert_eq!(checked.status, StatusCode::OK, "{}", checked.text());
    let checked = checked.json();
    assert_eq!(checked["source"], url);
    assert_eq!(checked["account"], "The Reef Times");
    assert_eq!(checked["latest"]["title"], "Coral **news**");

    // Private addresses and pages that are no feed are refused on the source field.
    for (source, wanted) in [
        ("http://10.0.0.1/feed", "private network"),
        ("http://[::1]/feed", "private network"),
        ("https://nothing.example/feed.xml", "doesn't exist"),
    ] {
        let refused = browser
            .write(
                Method::POST,
                &format!("{}/check", g(guild)),
                json!({ "kind": "rss", "source": source }),
            )
            .await;
        assert_eq!(refused.status, StatusCode::UNPROCESSABLE_ENTITY, "{source}");
        assert_eq!(fields_of(&refused), ["source"], "{source}");
        assert!(refused.text().contains(wanted), "{source}: {}", refused.text());
    }

    // Saving checks the channel, the role and the filters with the bot's rules.
    let bad = browser
        .write(
            Method::POST,
            &g(guild),
            json!({
                "kind": "rss", "source": url, "channel_id": LOUNGE.to_string(),
                "ping_role_id": guild.to_string(), "filter_include": ["x".repeat(41)],
            }),
        )
        .await;
    assert_eq!(bad.status, StatusCode::UNPROCESSABLE_ENTITY, "{}", bad.text());
    assert_eq!(fields_of(&bad), ["channel_id", "ping_role_id", "filter_include"]);
    let made = browser
        .write(
            Method::POST,
            &g(guild),
            json!({
                "kind": "rss", "source": url, "channel_id": GENERAL.to_string(), "message": "Fresh: {title} {url}",
                "ping_role_id": FAN.to_string(), "filter_exclude": ["Sponsored, ads"],
            }),
        )
        .await;
    assert_eq!(made.status, StatusCode::OK, "{}", made.text());
    let made = made.json();
    let feed = &made["items"][0];
    let id = made["created"].as_str().unwrap().to_owned();
    assert_eq!(feed["id"], id);
    assert_eq!(feed["account"], "The Reef Times");
    assert_eq!(feed["embed"], true);
    assert_eq!(feed["filter_exclude"], json!(["sponsored", "ads"]));
    assert_eq!(feed["health"]["state"], "waiting");
    let again = browser
        .write(
            Method::POST,
            &g(guild),
            json!({ "kind": "rss", "source": url, "channel_id": GENERAL.to_string() }),
        )
        .await;
    assert_eq!(fields_of(&again), ["channel_id"], "{}", again.text());

    // The preview is the bot's own message, mentions written out.
    let preview = browser
        .write(
            Method::POST,
            &format!("{}/preview", g(guild)),
            json!({
                "kind": "rss", "account": "The Reef Times", "message": "Fresh: {title} {url}", "embed": true,
                "ping_role_id": FAN.to_string(),
                "item": { "title": "Coral @everyone", "url": "https://reef.example/1" },
            }),
        )
        .await;
    assert_eq!(preview.status, StatusCode::OK, "{}", preview.text());
    let preview = preview.json();
    assert_eq!(preview["text"], "@Fan Fresh: Coral everyone https://reef.example/1");
    assert_eq!(preview["embed"]["url"], "https://reef.example/1");
    assert!(preview["notes"].to_string().contains("@Fan"), "{}", preview["notes"]);
    let sample = browser
        .write(
            Method::POST,
            &format!("{}/preview", g(guild)),
            json!({ "kind": "youtube", "embed": false }),
        )
        .await
        .json();
    assert!(
        sample["text"].as_str().unwrap().contains("posted a new video"),
        "{sample}"
    );

    // Edits: a channel that does not take posts is refused, a good one saved and audited.
    let path = format!("{}/{id}", g(guild));
    let refused = browser
        .write(Method::PUT, &path, json!({ "channel_id": LOUNGE.to_string() }))
        .await;
    assert_eq!(fields_of(&refused), ["channel_id"]);
    let moved = browser
        .write(
            Method::PUT,
            &path,
            json!({ "channel_id": NEWS.to_string(), "message": null, "enabled": false }),
        )
        .await;
    assert_eq!(moved.status, StatusCode::OK, "{}", moved.text());
    let moved = moved.json();
    assert_eq!(moved["items"][0]["channel_id"], NEWS.to_string());
    assert_eq!(moved["items"][0]["message"], Value::Null);
    assert_eq!(moved["items"][0]["health"]["state"], "off");
    let source_change = browser.write(Method::PUT, &path, json!({ "source": "x" })).await;
    assert_eq!(source_change.status, StatusCode::BAD_REQUEST);

    // A test post goes to the channel now.
    let tested = browser.write(Method::POST, &format!("{path}/test"), json!({})).await;
    assert_eq!(tested.status, StatusCode::OK, "{}", tested.text());
    assert_eq!(tested.json()["channel_id"], NEWS.to_string());

    let removed = browser.write(Method::DELETE, &path, json!({})).await;
    assert_eq!(removed.json()["items"], json!([]));
    let missing = browser.write(Method::PUT, &path, json!({ "enabled": true })).await;
    assert_eq!(missing.status, StatusCode::NOT_FOUND);

    let audit = h.state.app.db.audit_page(guild, None, 10).await.unwrap();
    let actions: Vec<(&str, i64)> = audit.iter().map(|a| (a.action.as_str(), a.actor_id)).collect();
    assert_eq!(
        actions,
        [
            ("feeds.remove", owner as i64),
            ("feeds.test", owner as i64),
            ("feeds.edit", owner as i64),
            ("feeds.add", owner as i64)
        ]
    );
}

#[tokio::test]
async fn only_managers_reach_the_feeds() {
    let Some(h) = Harness::new("feeds_api_managers").await else {
        return;
    };
    let guild = unique();
    h.add_bot(guild);
    let mut browser = h.browser();
    browser
        .login(
            &h,
            unique(),
            vec![guild_entry(guild, "Pinchora Reef", false, SEND_MESSAGES)],
        )
        .await;
    let list = browser.get(&g(guild)).await;
    assert_eq!(list.status, StatusCode::FORBIDDEN, "{}", list.text());
    let check = browser
        .write(
            Method::POST,
            &format!("{}/check", g(guild)),
            json!({ "kind": "rss", "source": "https://x.example/f" }),
        )
        .await;
    assert_eq!(check.status, StatusCode::FORBIDDEN, "{}", check.text());
}

#[tokio::test]
async fn feeds_dashboard_and_errors_follow_the_guild_language() {
    let Some(h) = Harness::new("feeds_turkish").await else {
        return;
    };
    let (guild, _, mut browser) = h.managed_server().await;
    h.state.app.set_guild_lang(guild, Some(Lang::Tr)).await.unwrap();
    let list = browser.get(&g(guild)).await;
    assert_eq!(list.status, StatusCode::OK);
    assert_eq!(list.json()["strings"]["social-feeds"], "SOSYAL AKIŞLAR");
    assert_eq!(list.json()["placeholders"][0]["hint"], "Paylaşımın başlığı");
    let invalid = browser
        .write(
            Method::POST,
            &format!("{}/check", g(guild)),
            json!({"kind": "unknown", "source": "example"}),
        )
        .await;
    assert_eq!(invalid.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(invalid.text().contains("Paylaşımların kaynağını seç."));
    let preview = browser
        .write(
            Method::POST,
            &format!("{}/preview", g(guild)),
            json!({"kind": "rss", "embed": true}),
        )
        .await;
    assert_eq!(preview.status, StatusCode::OK);
    assert!(
        preview.json()["notes"][0]
            .as_str()
            .unwrap()
            .starts_with("Bu örnek bir paylaşım")
    );
}

#[tokio::test]
async fn personal_channels_require_csrf_are_private_and_use_the_member_language() {
    let Some(h) = Harness::new("feeds_personal_auth").await else {
        return;
    };
    let (_, user, mut browser) = h.managed_server().await;
    h.state.app.db.set_user_language(user, Some(Lang::Tr)).await.unwrap();
    let source = format!("stream{user}");
    h.state
        .app
        .db
        .start_channel_link(user, "twitch", &source, "private-proof")
        .await
        .unwrap();
    let list = browser.get("/api/feeds/my-channels").await;
    assert_eq!(list.status, StatusCode::OK);
    assert_eq!(list.json()["items"][0]["code"], "private-proof");
    assert_eq!(
        list.json()["strings"]["my-channels"],
        pinchora_core::t!(Lang::Tr, "feeds.ui.my-channels")
    );
    for (method, path) in [
        (Method::POST, "/api/feeds/my-channels"),
        (Method::POST, "/api/feeds/my-channels/twitch/verify"),
        (Method::DELETE, "/api/feeds/my-channels/twitch"),
    ] {
        let rejected = browser
            .send(
                Request::builder()
                    .method(method)
                    .uri(path)
                    .header("content-type", "application/json")
                    .body(Body::from("{}"))
                    .unwrap(),
            )
            .await;
        assert_eq!(rejected.status, StatusCode::FORBIDDEN, "{}", rejected.text());
    }
    let mut stranger = h.browser();
    assert_eq!(
        stranger.get("/api/feeds/my-channels").await.status,
        StatusCode::UNAUTHORIZED
    );
    stranger.login(&h, unique(), vec![]).await;
    assert_eq!(stranger.get("/api/feeds/my-channels").await.json()["items"], json!([]));
    // Missing credentials disable verification cleanly; repeated lookup attempts are still bounded.
    assert_eq!(list.json()["kinds"][0]["available"], false);
    let disabled = browser
        .write(Method::POST, "/api/feeds/my-channels/twitch/verify", json!({}))
        .await;
    assert_eq!(disabled.status, StatusCode::UNPROCESSABLE_ENTITY);
    for _ in 0..10 {
        let disabled = browser
            .write(
                Method::POST,
                "/api/feeds/my-channels",
                json!({"kind":"twitch", "source":"abc"}),
            )
            .await;
        assert_eq!(disabled.status, StatusCode::UNPROCESSABLE_ENTITY);
    }
    let limited = browser
        .write(
            Method::POST,
            "/api/feeds/my-channels",
            json!({"kind":"twitch", "source":"abc"}),
        )
        .await;
    assert_eq!(limited.status, StatusCode::TOO_MANY_REQUESTS);
    let removed = browser
        .write(Method::DELETE, "/api/feeds/my-channels/twitch", json!({}))
        .await;
    assert_eq!(removed.status, StatusCode::OK, "{}", removed.text());
    assert!(h.state.app.db.channel_links(user).await.unwrap().is_empty());
}
