//! Social feeds: `/feeds` and the poller, against a local feed server. Every address a feed names is
//! answered by that server (`Fetcher::local`), so nothing here reaches the internet; YouTube, Reddit
//! and Bluesky are served at the paths their real addresses have.

use std::sync::OnceLock;

use chrono::{Duration as Span, Utc};
use pinchora_commands::feeds::{FeedsTick, Fetcher, install_fetcher, tick_for};
use pinchora_db::{FeedEdit, NewFeed};

use super::*;

/// What the local feed server answers for a path (with its query): status and body.
type Pages = Arc<Mutex<HashMap<String, (u16, String)>>>;

/// The feed server every feed test shares, started once on its own thread (each test has its own
/// runtime), with the process's fetcher pointed at it.
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
                tx.send(listener.local_addr().expect("feed server address")).unwrap();
                let router = Router::new().fallback(move |uri: Uri| {
                    let served = served.clone();
                    async move {
                        let key = uri.path_and_query().map_or(uri.path(), |p| p.as_str()).to_owned();
                        match served.lock().unwrap().get(&key) {
                            Some((status, body)) => (StatusCode::from_u16(*status).unwrap(), body.clone()),
                            None => (StatusCode::NOT_FOUND, String::new()),
                        }
                    }
                });
                axum::serve(listener, router).await.expect("feed server");
            });
        });
        install_fetcher(Fetcher::local(rx.recv().expect("feed server address")));
        pages
    })
}

/// Serves `body` at `url` (as the feed server sees it: `/<host><path>?<query>`).
fn serve(url: &str, status: u16, body: impl Into<String>) {
    let parsed = reqwest::Url::parse(url).expect("a url");
    let mut key = format!("/{}{}", parsed.host_str().unwrap(), parsed.path());
    if let Some(query) = parsed.query() {
        key.push('?');
        key.push_str(query);
    }
    pages().lock().unwrap().insert(key, (status, body.into()));
}

/// One step of the poller for this test's server (tests share the database, and each polls its own).
async fn tick(sim: &Sim) -> anyhow::Result<FeedsTick> {
    tick_for(&sim.app, Some(&[sim.guild])).await
}

/// A channel id YouTube could have, unique per call.
fn youtube_id() -> String {
    let mut id = format!("UC{:022}", unique());
    id.truncate(24);
    id
}

fn youtube_feed(channel: &str, name: &str, videos: &[(&str, &str, chrono::DateTime<Utc>)]) -> String {
    let entries: String = videos
        .iter()
        .map(|(id, title, at)| {
            format!(
                r#"<entry><id>yt:video:{id}</id><yt:videoId>{id}</yt:videoId><title>{title}</title>
<link rel="alternate" href="https://www.youtube.com/watch?v={id}"/><author><name>{name}</name></author>
<published>{}</published><media:group><media:content url="https://www.youtube.com/v/{id}" type="application/x-shockwave-flash"/>
<media:thumbnail url="https://i2.ytimg.com/vi/{id}/hqdefault.jpg" width="480" height="360"/></media:group></entry>"#,
                at.to_rfc3339()
            )
        })
        .collect();
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?><feed xmlns:yt="http://www.youtube.com/xml/schemas/2015"
 xmlns:media="http://search.yahoo.com/mrss/" xmlns="http://www.w3.org/2005/Atom">
<id>yt:channel:{channel}</id><title>{name}</title>{entries}</feed>"#
    )
}

fn rss_feed(title: &str, items: &[(&str, &str, chrono::DateTime<Utc>)]) -> String {
    let items: String = items
        .iter()
        .map(|(guid, title, at)| {
            format!(
                "<item><title>{title}</title><link>https://reef.example/{guid}</link><guid>{guid}</guid>\
                 <pubDate>{}</pubDate><description>&lt;p&gt;About {guid}&lt;/p&gt;</description></item>",
                at.to_rfc2822()
            )
        })
        .collect();
    format!(r#"<?xml version="1.0"?><rss version="2.0"><channel><title>{title}</title>{items}</channel></rss>"#)
}

fn add_options(kind: &str, source: &str, channel: u64, extra: Vec<Value>) -> Value {
    let mut options = vec![
        text_option("kind", kind),
        text_option("source", source),
        channel_option("channel", channel),
    ];
    options.extend(extra);
    json!([sub("add", options)])
}

/// The number `/feeds add` gave the feed, read from its answer ("Following **#12** · …").
fn feed_id(reply: &Reply) -> i64 {
    let text = reply.content();
    let start = text.find("**#").unwrap_or_else(|| panic!("no feed number in {text:?}")) + 3;
    text[start..]
        .split("**")
        .next()
        .and_then(|n| n.parse().ok())
        .unwrap_or_else(|| panic!("no feed number in {text:?}"))
}

#[tokio::test]
async fn a_manager_adds_lists_tests_edits_and_removes_a_youtube_feed() {
    let Some(mut sim) = Sim::new("a_manager_adds_lists_tests_edits_and_removes_a_youtube_feed").await else {
        return;
    };
    let admin = unique();
    sim.permissions = MANAGE_GUILD;
    let (channel, other) = (unique(), unique());
    let id = youtube_id();
    let handle = format!("reef{}", unique() % 1_000_000);
    // The handle's page names its channel; the channel's feed has one video already.
    serve(
        &format!("https://www.youtube.com/@{handle}"),
        200,
        format!(r#"<html><head><link rel="canonical" href="https://www.youtube.com/channel/{id}"></head></html>"#),
    );
    let now = Utc::now();
    let feed_url = format!("https://www.youtube.com/feeds/videos.xml?channel_id={id}");
    serve(
        &feed_url,
        200,
        youtube_feed(
            &id,
            "Reef &amp; Friends",
            &[("vid00000001", "First dive", now - Span::hours(2))],
        ),
    );

    let added = sim
        .command_resolved(
            admin,
            "feeds",
            add_options("youtube", &format!("@{handle}"), channel, vec![]),
            resolved_channels(&[channel]),
        )
        .await;
    assert!(added.ephemeral);
    assert!(
        added.content().contains("Reef & Friends") && added.content().contains("First dive"),
        "{}",
        added.content()
    );
    let feed = feed_id(&added);
    let stored = sim
        .app
        .db
        .feed(sim.guild, feed)
        .await
        .unwrap()
        .expect("the feed is stored");
    assert_eq!(stored.source, id, "the handle was turned into the channel id");
    assert!(!stored.embed, "YouTube keeps Discord's own player");
    assert_eq!(stored.created_by as u64, admin);

    // The same source in the same channel again is refused; in another channel it is fine.
    let again = sim
        .command_resolved(
            admin,
            "feeds",
            add_options("youtube", &id, channel, vec![]),
            resolved_channels(&[channel]),
        )
        .await;
    assert!(again.content().contains("already posted"), "{}", again.content());

    let list = sim.command(admin, "feeds", json!([sub("list", vec![])])).await;
    assert!(list.ephemeral);
    assert!(list.title().starts_with("Feeds (1/10)"), "{}", list.title());
    assert!(
        list.description().contains(&format!("#{feed}")),
        "{}",
        list.description()
    );

    // A test post: the latest video, as new posts will look, pinging nobody.
    let tested = sim
        .command(admin, "feeds", json!([sub("test", vec![int_option("id", feed)])]))
        .await;
    assert!(
        tested.content().contains("Posted the latest item"),
        "{}",
        tested.content()
    );
    let posts = sim.posts();
    let post = in_channel(&posts, channel);
    assert_eq!(post.len(), 1);
    assert!(
        post[0]
            .content()
            .contains("https://www.youtube.com/watch?v=vid00000001"),
        "{}",
        post[0].content()
    );
    assert_eq!(post[0].body["allowed_mentions"], json!({ "parse": [] }));

    // Edit: a role to ping, the card on, a filter, another channel.
    let role_id = unique();
    let role = sim.guild_role(role_id, "Divers", 2);
    let edited = sim
        .command_resolved(
            admin,
            "feeds",
            json!([sub(
                "edit",
                vec![
                    int_option("id", feed),
                    channel_option("channel", other),
                    role_option("role", &role),
                    bool_option("embed", true),
                    text_option("include", "dive, Reef"),
                    text_option("message", "New: {title} {url}"),
                ]
            )]),
            json!({ "roles": resolved(&[], &[&role])["roles"], "channels": resolved_channels(&[other])["channels"] }),
        )
        .await;
    assert!(edited.content().contains("Saved"), "{}", edited.content());
    let stored = sim.app.db.feed(sim.guild, feed).await.unwrap().unwrap();
    assert_eq!(stored.channel_id as u64, other);
    assert_eq!(stored.ping_role_id.map(|r| r as u64), Some(role_id));
    assert_eq!(stored.filter_include, ["dive", "reef"]);
    assert_eq!(stored.message.as_deref(), Some("New: {title} {url}"));

    // A filter with too many words is refused (Discord itself keeps a message to 500 characters).
    let words = (0..11).map(|i| format!("w{i}")).collect::<Vec<_>>().join(",");
    let many = sim
        .command(
            admin,
            "feeds",
            json!([sub(
                "edit",
                vec![int_option("id", feed), text_option("exclude", &words)]
            )]),
        )
        .await;
    assert!(many.content().contains("at most 10 words"), "{}", many.content());

    let removed = sim
        .command(admin, "feeds", json!([sub("remove", vec![int_option("id", feed)])]))
        .await;
    assert!(removed.content().contains("Stopped following"), "{}", removed.content());
    assert!(sim.app.db.feed(sim.guild, feed).await.unwrap().is_none());
    let gone = sim
        .command(admin, "feeds", json!([sub("remove", vec![int_option("id", feed)])]))
        .await;
    assert!(gone.content().contains("no feed"), "{}", gone.content());
    let audit = sim.app.db.audit_page(sim.guild, None, 10).await.unwrap();
    let actions: Vec<&str> = audit.iter().map(|a| a.action.as_str()).collect();
    assert_eq!(actions, ["feeds.remove", "feeds.edit", "feeds.add"]);
}

#[tokio::test]
async fn sources_that_cannot_be_followed_are_explained() {
    let Some(mut sim) = Sim::new("sources_that_cannot_be_followed_are_explained").await else {
        return;
    };
    let admin = unique();
    sim.permissions = MANAGE_GUILD;
    let channel = unique();
    let host = format!("feeds{}.example", unique());
    // A page that is no feed; addresses inside private networks; a Bluesky account that does not exist.
    serve(&format!("https://{host}/page"), 200, "<html><body>hello</body></html>");
    let cases = [
        ("rss", "http://127.0.0.1/feed".to_owned(), "private network"),
        (
            "rss",
            "http://169.254.169.254/latest/meta-data".to_owned(),
            "private network",
        ),
        ("rss", format!("https://{host}/page"), "isn't an RSS or Atom feed"),
        ("rss", format!("https://{host}/missing"), "doesn't exist"),
        (
            "youtube",
            "https://vimeo.com/reef".to_owned(),
            "isn't a YouTube channel",
        ),
        ("reddit", "r/x".to_owned(), "isn't a subreddit"),
        ("bluesky", format!("ghost{}.bsky.social", unique()), "no account"),
        ("bridge", "twitter/user/reef".to_owned(), "RSS bridge"),
    ];
    for (kind, source, wanted) in cases {
        let reply = sim
            .command_resolved(
                admin,
                "feeds",
                add_options(kind, &source, channel, vec![]),
                resolved_channels(&[channel]),
            )
            .await;
        assert!(reply.ephemeral);
        assert!(reply.content().contains(wanted), "{kind} {source}: {}", reply.content());
    }
    assert!(sim.app.db.feeds_of(sim.guild).await.unwrap().is_empty());
}

#[tokio::test]
async fn new_items_are_posted_once_per_feed_even_with_two_pods() {
    let Some(mut sim) = Sim::new("new_items_are_posted_once_per_feed_even_with_two_pods").await else {
        return;
    };
    let admin = unique();
    sim.permissions = MANAGE_GUILD;
    let (channel, elsewhere) = (unique(), unique());
    let url = format!("https://reef{}.example/feed.xml", unique());
    let now = Utc::now();
    let first = ("ep-1", "Old news", now - Span::hours(5));
    serve(&url, 200, rss_feed("The Reef Times", &[first]));
    let role_id = unique();
    let role = sim.guild_role(role_id, "Readers", 2);
    let added = sim
        .command_resolved(
            admin,
            "feeds",
            add_options("rss", &url, channel, vec![role_option("role", &role)]),
            json!({ "roles": resolved(&[], &[&role])["roles"], "channels": resolved_channels(&[channel])["channels"] }),
        )
        .await;
    let feed = feed_id(&added);
    // Another server follows the same source; the source is fetched once for both.
    let other_guild = unique();
    let stored = sim.app.db.feed(sim.guild, feed).await.unwrap().unwrap();
    sim.app
        .db
        .add_feed(&NewFeed {
            guild: other_guild,
            kind: "rss",
            source: &stored.source,
            title: None,
            channel: elsewhere,
            message: Some("{account} · {title} · {author} · {url}"),
            embed: false,
            ping_role: None,
            include: &[],
            exclude: &["boring".to_owned()],
            created_by: admin,
            seen: &[],
            interval_secs: 600,
        })
        .await
        .unwrap();
    let source = sim.app.db.feed_source("rss", &stored.source).await.unwrap().unwrap();
    assert_eq!(source.subscribers, 2);
    assert!(source.primed, "adding recorded what was already there");

    // Nothing new yet: polling posts nothing.
    let quiet_tick = tick(&sim).await.unwrap();
    assert_eq!(quiet_tick.queued, 0, "{quiet_tick:?}");
    assert!(sim.posts().is_empty());

    // Two new items, one hostile and one the second server filters out; two pods poll at once.
    let later = Utc::now();
    serve(
        &url,
        200,
        rss_feed(
            "The Reef Times",
            &[
                ("ep-3", "A boring update", later),
                (
                    "ep-2",
                    "Coral **found** @everyone &lt;@&amp;123&gt;",
                    later - Span::minutes(1),
                ),
                first,
            ],
        ),
    );
    sim.app.shift_clock(chrono::Duration::minutes(30));
    let both = [sim.guild, other_guild];
    let pod = || tick_for(&sim.app, Some(&both));
    let (a, b) = tokio::join!(pod(), pod());
    let (a, b) = (a.unwrap(), b.unwrap());
    assert_eq!(a.polled + b.polled, 1, "one pod polled the source: {a:?} {b:?}");
    assert_eq!(
        a.queued + b.queued,
        3,
        "two posts for the first server, one for the second"
    );
    // Whatever one pod left in the outbox, the next step sends; nothing goes out twice.
    let rest = pod().await.unwrap();
    assert_eq!(a.posted + b.posted + rest.posted, 3, "{a:?} {b:?} {rest:?}");
    let posts = sim.posts();
    let here = in_channel(&posts, channel);
    let there = in_channel(&posts, elsewhere);
    assert_eq!((here.len(), there.len()), (2, 1));
    // Oldest first, with the card, the role pinged and only the role.
    assert!(
        here[0].content().starts_with(&format!("<@&{role_id}> ")),
        "{}",
        here[0].content()
    );
    assert!(here[0].content().contains("Coral"), "{}", here[0].content());
    assert!(here[1].content().contains("boring"), "{}", here[1].content());
    assert_eq!(
        here[0].body["allowed_mentions"],
        json!({ "parse": [], "roles": [role_id.to_string()] })
    );
    assert_eq!(here[0].embed()["url"], "https://reef.example/ep-2");
    assert_eq!(here[0].embed()["description"], "About ep\\-2");
    for post in &posts {
        let text = post.content();
        assert!(!text.contains("@everyone") && !text.contains("<@&123>"), "{text}");
        assert!(text.contains(r"\*\*found\*\*") || text.contains("boring"), "{text}");
    }
    assert!(there[0].content().contains("Coral"), "{}", there[0].content());
    assert_eq!(there[0].body["allowed_mentions"], json!({ "parse": [] }));

    // Polling again finds nothing new.
    sim.app.shift_clock(chrono::Duration::minutes(30));
    let again = pod().await.unwrap();
    assert_eq!((again.polled, again.queued, again.posted), (1, 0, 0));
    let stored = sim.app.db.feed(sim.guild, feed).await.unwrap().unwrap();
    assert_eq!(stored.last_item_id.as_deref(), Some("ep-3"));
    assert!(stored.last_sent_at.is_some());
}

#[tokio::test]
async fn a_failing_source_backs_off_and_a_refused_post_is_shown() {
    let Some(mut sim) = Sim::new("a_failing_source_backs_off_and_a_refused_post_is_shown").await else {
        return;
    };
    let admin = unique();
    sim.permissions = MANAGE_GUILD;
    let channel = unique();
    let sub_name = format!("t{}", unique() % 10_000_000_000);
    let url = format!("https://www.reddit.com/r/{sub_name}/new/.rss");
    let now = Utc::now();
    let reddit = |items: &[(&str, &str, chrono::DateTime<Utc>)]| {
        let entries: String = items
            .iter()
            .map(|(id, title, at)| {
                format!(
                    r#"<entry><author><name>/u/ferris</name></author><id>{id}</id>
<link href="https://www.reddit.com/r/{sub_name}/comments/{id}/"/><updated>{0}</updated><published>{0}</published>
<title>{title}</title></entry>"#,
                    at.to_rfc3339()
                )
            })
            .collect::<String>();
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?><feed xmlns="http://www.w3.org/2005/Atom"><title>newest submissions : {sub_name}</title>{entries}</feed>"#
        )
    };
    serve(&url, 200, reddit(&[("t3_a", "Hello", now - Span::hours(1))]));
    let added = sim
        .command_resolved(
            admin,
            "feeds",
            add_options("reddit", &format!("r/{sub_name}"), channel, vec![]),
            resolved_channels(&[channel]),
        )
        .await;
    let feed = feed_id(&added);

    // The site breaks: the source waits twice as long each time and says why.
    serve(&url, 503, "down");
    sim.app.shift_clock(chrono::Duration::minutes(10));
    let checked_at = sim.app.now();
    tick(&sim).await.unwrap();
    let source = sim.app.db.feed_source("reddit", &sub_name).await.unwrap().unwrap();
    assert_eq!(source.fail_count, 1);
    assert_eq!(source.last_error.as_deref(), Some("feeds.error.status|503"));
    let wait = (source.next_poll_at - checked_at).num_seconds();
    assert!(
        (600..=660).contains(&wait),
        "first failure waits 2 × 5 minutes, got {wait} s"
    );
    let list = sim.command(admin, "feeds", json!([sub("list", vec![])])).await;
    assert!(list.description().contains("HTTP 503"), "{}", list.description());

    // It comes back with a new post, but the bot may not post in the channel: the feed says so.
    serve(
        &url,
        200,
        reddit(&[("t3_b", "Fresh", Utc::now()), ("t3_a", "Hello", now - Span::hours(1))]),
    );
    sim.block_channel(channel);
    sim.app.shift_clock(chrono::Duration::minutes(15));
    let step = tick(&sim).await.unwrap();
    assert_eq!((step.queued, step.posted), (1, 0));
    let source = sim.app.db.feed_source("reddit", &sub_name).await.unwrap().unwrap();
    assert_eq!((source.fail_count, source.last_error), (0, None));
    let stored = sim.app.db.feed(sim.guild, feed).await.unwrap().unwrap();
    assert_eq!(stored.last_error.as_deref(), Some("feeds.error.post-permission"));
    let list = sim.command(admin, "feeds", json!([sub("list", vec![])])).await;
    assert!(
        list.description()
            .contains("View Channel, Send Messages and Embed Links"),
        "{}",
        list.description()
    );
    // A test post says the same at once.
    let tested = sim
        .command(admin, "feeds", json!([sub("test", vec![int_option("id", feed)])]))
        .await;
    assert!(tested.content().contains("Send Messages"), "{}", tested.content());

    // Switching it off and on again clears the error.
    let edit = FeedEdit {
        enabled: Some(false),
        ..FeedEdit::default()
    };
    sim.app.db.edit_feed(sim.guild, feed, &edit).await.unwrap();
    let edit = FeedEdit {
        enabled: Some(true),
        ..FeedEdit::default()
    };
    sim.app.db.edit_feed(sim.guild, feed, &edit).await.unwrap();
    assert_eq!(
        sim.app.db.feed(sim.guild, feed).await.unwrap().unwrap().last_error,
        None
    );
}

#[tokio::test]
async fn bluesky_posts_and_a_switched_off_module() {
    let Some(mut sim) = Sim::new("bluesky_posts_and_a_switched_off_module").await else {
        return;
    };
    let admin = unique();
    sim.permissions = MANAGE_GUILD;
    let channel = unique();
    let handle = format!("reef{}.bsky.social", unique());
    let url = format!(
        "https://public.api.bsky.app/xrpc/app.bsky.feed.getAuthorFeed?actor={handle}&limit=25&filter=posts_no_replies"
    );
    let post = |rkey: &str, text: &str, at: chrono::DateTime<Utc>| {
        json!({ "post": {
            "uri": format!("at://did:plc:reef/app.bsky.feed.post/{rkey}"), "cid": "c",
            "author": { "did": "did:plc:reef", "handle": handle, "displayName": "Reef Watch" },
            "record": { "text": text, "createdAt": at.to_rfc3339() }, "indexedAt": at.to_rfc3339(),
        }})
    };
    let now = Utc::now();
    serve(
        &url,
        200,
        json!({ "feed": [post("a1", "Morning", now - Span::hours(3))] }).to_string(),
    );
    let added = sim
        .command_resolved(
            admin,
            "feeds",
            add_options("bluesky", &format!("@{handle}"), channel, vec![]),
            resolved_channels(&[channel]),
        )
        .await;
    assert!(added.content().contains("Reef Watch"), "{}", added.content());

    serve(
        &url,
        200,
        json!({ "feed": [post("b2", "Turtles <@1> spotted", Utc::now()), post("a1", "Morning", now - Span::hours(3))] })
            .to_string(),
    );
    sim.app.shift_clock(chrono::Duration::minutes(5));
    let step = tick(&sim).await.unwrap();
    assert_eq!(step.posted, 1, "{step:?}");
    let posts = sim.posts();
    assert!(
        posts[0]
            .content()
            .contains(&format!("https://bsky.app/profile/{handle}/post/b2")),
        "{}",
        posts[0].content()
    );
    assert_eq!(posts[0].embed()["title"], "Turtles spotted");

    // With the module switched off, new posts are dropped, not saved up for later.
    sim.app
        .db
        .set_module(sim.guild, pinchora_core::modules::Module::Feeds, Some(false), None)
        .await
        .unwrap();
    sim.app.announce_guild_change(sim.guild).await;
    serve(
        &url,
        200,
        json!({ "feed": [post("c3", "Sharks", Utc::now()), post("b2", "Turtles", Utc::now())] }).to_string(),
    );
    sim.app.shift_clock(chrono::Duration::minutes(5));
    let step = tick(&sim).await.unwrap();
    assert_eq!((step.queued, step.posted), (1, 0), "{step:?}");
    assert!(sim.posts().is_empty());
    let refused = sim.command(admin, "feeds", json!([sub("list", vec![])])).await;
    assert!(refused.content().contains("turned off"), "{}", refused.content());
}

#[tokio::test]
async fn feeds_need_manage_server() {
    let Some(mut sim) = Sim::new("feeds_need_manage_server").await else {
        return;
    };
    let (member, channel) = (unique(), unique());
    let role = sim.guild_role(unique(), "Fans", 2);
    let rows = [
        add_options("rss", "https://example.com/feed.xml", channel, vec![]),
        json!([sub("list", vec![])]),
        json!([sub("edit", vec![int_option("id", 1), bool_option("enabled", false)])]),
        json!([sub("test", vec![int_option("id", 1)])]),
        json!([sub("remove", vec![int_option("id", 1)])]),
    ];
    let known =
        json!({ "roles": resolved(&[], &[&role])["roles"], "channels": resolved_channels(&[channel])["channels"] });
    let all_but_manage = ((1u64 << 50) - 1) & !(MANAGE_GUILD | (1 << 3));
    for permissions in [0, all_but_manage] {
        sim.permissions = permissions;
        for options in &rows {
            let reply = sim
                .command_resolved(member, "feeds", options.clone(), known.clone())
                .await;
            assert!(
                reply.ephemeral && reply.content().contains("Manage Server"),
                "{options} with {permissions}: {}",
                reply.content()
            );
        }
        // The list's page buttons ask too.
        let page = sim
            .component(
                member,
                "sn:l:1",
                &[],
                &Reply::new(InteractionResponseType::ChannelMessageWithSource),
            )
            .await;
        assert!(page.content().contains("Manage Server"), "{}", page.content());
    }
    assert!(sim.app.db.feeds_of(sim.guild).await.unwrap().is_empty());
}
