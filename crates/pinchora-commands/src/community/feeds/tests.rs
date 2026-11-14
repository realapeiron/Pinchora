use crate::community::feeds::*;

#[test]
fn reddit_off_rejects_stored_sources_before_fetch() {
    let off = Instance {
        bridge: None,
        reddit: false,
        twitch: false,
        kick: false,
    };
    assert_eq!(
        source_url(Kind::Reddit, "existing-subscription", &off),
        Err(Problem::Unavailable(Kind::Reddit))
    );
    assert!(source_url(Kind::Rss, "https://example.com/rss", &off).is_ok());
}

#[test]
fn delivery_nonce_is_stable_and_enforced_only_for_announcements() {
    let message = FeedMessage {
        content: "Fresh".into(),
        embed: None,
    };
    let mentions = AllowedMentions::default();
    let nonce = delivery_nonce(42, "entry-1");
    assert_eq!(nonce, delivery_nonce(42, "entry-1"));
    assert_ne!(nonce, delivery_nonce(42, "entry-2"));
    assert_ne!(nonce, delivery_nonce(43, "entry-1"));
    let payload: serde_json::Value =
        serde_json::from_slice(&delivery_payload(&message, &[], &mentions, Some(nonce))).unwrap();
    assert_eq!(payload["nonce"], nonce.to_string());
    assert_eq!(payload["enforce_nonce"], true);
    let test_post: serde_json::Value =
        serde_json::from_slice(&delivery_payload(&message, &[], &mentions, None)).unwrap();
    assert!(test_post.get("nonce").is_none());
}

#[test]
fn problems_are_worded_in_both_languages() {
    for problem in [
        Problem::Source(SourceError::YouTube),
        Problem::Unavailable(Kind::Bridge),
        Problem::Unavailable(Kind::Reddit),
        Problem::Fetch(FetchError::Status(503)),
        Problem::Fetch(FetchError::Blocked),
        Problem::Parse(ParseError::NoAccount),
        Problem::YouTubePage,
    ] {
        for lang in Lang::ALL {
            let text = problem.text(lang);
            assert!(!text.starts_with("feeds."), "{problem:?} in {lang:?} has no text");
        }
    }
    assert!(error_text(Lang::En, "feeds.error.status|503").contains("503"));
    assert!(error_text(Lang::Tr, "feeds.error.status|503").contains("503"));
}
