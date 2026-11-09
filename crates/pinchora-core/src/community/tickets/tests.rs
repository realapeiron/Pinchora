use chrono::TimeZone;

use crate::community::tickets::*;

#[test]
fn transcript_html_keeps_newest_messages_under_cap() {
    let head = TranscriptHead {
        server: "Reef",
        number: 1,
        topic: "Help",
        opener: None,
        opened_at: at(0),
        closed_at: None,
        closed_by: None,
        reason: None,
        with_content: true,
    };
    let lines: Vec<Line> = (0..20)
        .map(|n| Line {
            a: 0,
            t: at(n),
            c: Some(format!("MARKER-{n} {}", "x".repeat(500_000))),
            f: vec![],
            e: 0,
        })
        .collect();
    let html = render_html(&head, &lines, &[], Lang::En);
    assert!(html.len() <= TRANSCRIPT_HTML_MAX);
    assert!(html.contains("MARKER-19"));
    assert!(!html.contains("MARKER-0"));
    assert!(html.contains("older messages were omitted"));
}

#[test]
fn mention_names_are_resolved_only_when_rendered() {
    let raw = "Hello <@42> and <@!99>.";
    let names = HashMap::from([(42, "Sam".to_owned())]);
    assert_eq!(render_mentions(raw, &names, Lang::En), "Hello @Sam and @Deleted user.");
    assert_eq!(
        render_mentions(raw, &HashMap::new(), Lang::Tr),
        "Hello @Silinmiş kullanıcı and @Silinmiş kullanıcı."
    );
    assert_eq!(raw, "Hello <@42> and <@!99>.");
}

fn draft<'a>(topics: &'a [Topic], questions: &'a [Question]) -> PanelDraft<'a> {
    PanelDraft {
        name: "Support",
        title: "Need a hand?",
        description: "Press a button and we answer in private.",
        style: PanelStyle::Buttons,
        topics,
        questions,
        welcome: "Hi {user}, tell us more.",
        support_roles: 1,
    }
}

#[test]
fn a_good_panel_passes_and_every_problem_is_named() {
    let topics = [Topic::new("Support"), Topic::new("Billing")];
    assert!(check_panel(&draft(&topics, &[]), Lang::En).is_empty());

    let six: Vec<Topic> = (0..6).map(|n| Topic::new(&format!("Topic {n}"))).collect();
    let errors = check_panel(&draft(&six, &[]), Lang::En);
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert_eq!(errors[0].field, "buttons");
    let menu = PanelDraft {
        style: PanelStyle::Select,
        ..draft(&six, &[])
    };
    assert!(check_panel(&menu, Lang::En).is_empty(), "a menu takes 25");

    let twice = [Topic::new("Help"), Topic::new("help")];
    let errors = check_panel(&draft(&twice, &[]), Lang::En);
    assert!(errors[0].message.to_lowercase().contains("help"), "{errors:?}");

    let bad = PanelDraft {
        name: "",
        title: " ",
        ..draft(&[], &[])
    };
    let fields: Vec<&str> = check_panel(&bad, Lang::Tr).iter().map(|e| e.field).collect();
    assert_eq!(fields, ["name", "title", "buttons"]);
    let number = PanelDraft {
        name: "12",
        ..draft(&topics, &[])
    };
    assert_eq!(
        check_panel(&number, Lang::En)[0].field,
        "name",
        "a number would read as an id"
    );
}

#[test]
fn emoji_are_emoji_and_nothing_else() {
    for good in ["🎫", "❤️", "🇹🇷", "👩‍💻"] {
        assert!(is_emoji(good), "{good}");
    }
    for bad in ["", "ticket", "<:pin:123>", "@here", "a🎫", "🎫 🎫"] {
        assert!(!is_emoji(bad), "{bad}");
    }
    let mut topic = Topic::new("Help");
    topic.emoji = Some("help".into());
    let errors = check_panel(&draft(&[topic], &[]), Lang::En);
    assert_eq!(errors.len(), 1);
}

#[test]
fn forms_ask_up_to_twenty_questions_across_steps() {
    let q = |label: &str| Question {
        label: label.into(),
        placeholder: String::new(),
        long: false,
        required: true,
        choices: Vec::new(),
        upload: false,
    };
    assert!(check_questions(&[q("Your order number")], Lang::En).is_empty());
    let six: Vec<Question> = (0..6).map(|n| q(&format!("Q{n}"))).collect();
    assert!(check_questions(&six, Lang::En).is_empty());
    let twenty_one: Vec<Question> = (0..21).map(|n| q(&format!("Q{n}"))).collect();
    assert_eq!(check_questions(&twenty_one, Lang::En).len(), 1);
    assert_eq!(check_questions(&[q(""), q(&"x".repeat(46))], Lang::En).len(), 2);
    let json: Vec<Question> = serde_json::from_str("[{\"label\":\"Why?\"}]").expect("valid JSON");
    assert!(json[0].required && !json[0].long, "required unless said otherwise");
}

#[test]
fn topics_come_from_one_line() {
    let topics = topics_from_list("Support, Billing |Report a member,,");
    let labels: Vec<&str> = topics.iter().map(|t| t.label.as_str()).collect();
    assert_eq!(labels, ["Support", "Billing", "Report a member"]);
}

#[test]
fn channel_names_are_valid_discord_names() {
    assert_eq!(channel_name(DEFAULT_NAMING, 42, "Sam", "Help"), "ticket-0042");
    assert_eq!(
        channel_name("{topic}-{user}", 7, "Mira Çelik", "Billing & Refunds"),
        "billing-refunds-mira-çelik"
    );
    assert_eq!(channel_name("  🎫 {user}!! ", 1, "a  b", ""), "a-b");
    assert_eq!(channel_name("{user}", 1, "@#$%", ""), "");
    assert_eq!(channel_name(&"x".repeat(150), 1, "", "").chars().count(), 100);
    assert!(check_naming("{user}", Lang::En).is_ok());
    assert!(check_naming("!!!", Lang::En).is_err());
    assert!(check_naming(&"a".repeat(61), Lang::En).is_err());
}

#[test]
fn auto_mode_switches_to_threads_past_the_threshold() {
    assert!(!Mode::Auto.threads(THREAD_THRESHOLD));
    assert!(Mode::Auto.threads(THREAD_THRESHOLD + 1));
    assert!(Mode::Thread.threads(0));
    assert!(!Mode::Channel.threads(499));
    assert_eq!(Mode::parse("thread"), Some(Mode::Thread));
    assert!(check_auto_close(721, Lang::En).is_err() && check_auto_close(0, Lang::En).is_ok());
    assert!(check_open_limit(0, Lang::En).is_err() && check_open_limit(10, Lang::En).is_ok());
}

#[test]
fn a_ticket_goes_open_claimed_closed_and_reopens_unclaimed() {
    use {Action as A, Refusal as R, State as S};
    let table = [
        (S::Open, A::Claim, Ok(S::Claimed)),
        (S::Claimed, A::Claim, Err(R::AlreadyClaimed)),
        (S::Claimed, A::Unclaim, Ok(S::Open)),
        (S::Open, A::Unclaim, Err(R::NotClaimed)),
        (S::Claimed, A::Close, Ok(S::Closed)),
        (S::Open, A::Close, Ok(S::Closed)),
        (S::Closed, A::Close, Err(R::AlreadyClosed)),
        (S::Closed, A::Claim, Err(R::AlreadyClosed)),
        (S::Closed, A::Lock, Err(R::AlreadyClosed)),
        (S::Closed, A::Reopen, Ok(S::Open)),
        (S::Open, A::Reopen, Err(R::NotClosed)),
        (S::Claimed, A::Lock, Ok(S::Claimed)),
        (S::Open, A::AddMember, Ok(S::Open)),
        (S::Closed, A::Transcript, Ok(S::Closed)),
        (S::Open, A::Transcript, Ok(S::Open)),
        (S::Open, A::Delete, Err(R::NotClosed)),
        (S::Closed, A::Delete, Ok(S::Closed)),
    ];
    for (state, action, wanted) in table {
        assert_eq!(transition(state, action), wanted, "{state:?} + {action:?}");
    }
    assert_eq!(State::of(false, true), S::Claimed);
    assert_eq!(State::of(true, true), S::Closed);
}

#[test]
fn only_staff_run_a_ticket_but_its_opener_may_close_it() {
    assert_eq!(may(Action::Close, false, true), Ok(()));
    assert_eq!(may(Action::Transcript, false, true), Ok(()));
    assert_eq!(may(Action::Claim, false, true), Err(Refusal::StaffOnly));
    assert_eq!(may(Action::Close, false, false), Err(Refusal::StaffOnly));
    assert_eq!(may(Action::Delete, true, false), Ok(()));
    for refusal in [Refusal::AlreadyClaimed, Refusal::StaffOnly] {
        assert_ne!(refusal.text(Lang::Tr), refusal.text(Lang::En));
    }
}

fn at(minute: u32) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 11, 6, 18, minute, 0).unwrap()
}

#[test]
fn the_first_reply_is_the_first_human_who_is_not_the_opener() {
    let authors = [
        Author {
            idx: 0,
            name: Some("Pinchora".into()),
            bot: true,
        },
        Author {
            idx: 1,
            name: Some("Sam".into()),
            bot: false,
        },
        Author {
            idx: 2,
            name: Some("Staff".into()),
            bot: false,
        },
    ];
    let line = |a: u16, minute: u32| Line {
        a,
        t: at(minute),
        c: Some("hi".into()),
        f: vec![],
        e: 0,
    };
    let lines = [line(0, 0), line(1, 1), line(2, 5), line(1, 6)];
    assert_eq!(first_reply(&lines, &authors, Some(1)), Some(at(5)));
    assert_eq!(first_reply(&lines[..2], &authors, Some(1)), None);
}

/// The transcript golden: hostile text stays text, deleted members read as such, nothing loads from
/// anywhere else.
#[test]
fn transcripts_escape_everything_a_member_wrote() {
    let authors = [
        Author {
            idx: 0,
            name: Some("<b>Sam</b>".into()),
            bot: false,
        },
        Author {
            idx: 1,
            name: Some("Pinchora".into()),
            bot: true,
        },
    ];
    let lines = [
        Line {
            a: 0,
            t: at(1),
            c: Some("<script>alert('x')</script>\n<img src=x onerror=alert(1)> & \"quotes\"".into()),
            f: vec!["\"><svg onload=alert(1)>.png".into()],
            e: 0,
        },
        Line {
            a: 1,
            t: at(2),
            c: Some(String::new()),
            f: vec![],
            e: 2,
        },
        Line {
            a: 7,
            t: at(3),
            c: Some("gone".into()),
            f: vec![],
            e: 0,
        },
    ];
    let head = TranscriptHead {
        server: "Reef & <Friends>",
        number: 42,
        topic: "</title><script>",
        opener: Some("<b>Sam</b>"),
        opened_at: at(0),
        closed_at: Some(at(4)),
        closed_by: None,
        reason: Some("Solved <3"),
        with_content: true,
    };
    let html = render_html(&head, &lines, &authors, Lang::En);
    let golden = "<div class=\"msg\"><div class=\"who\">&lt;b&gt;Sam&lt;/b&gt;<time class=\"when\" \
                      datetime=\"2026-11-06T18:01:00+00:00\">2026-11-06 18:01 UTC</time></div><div><div class=\"text\">\
                      &lt;script&gt;alert(&#39;x&#39;)&lt;/script&gt;\n&lt;img src=x onerror=alert(1)&gt; &amp; \
                      &quot;quotes&quot;</div><div class=\"files\">1 file: &quot;&gt;&lt;svg onload=alert(1)&gt;.png\
                      </div></div></div>";
    assert!(html.contains(golden), "{html}");
    for hostile in ["<script", "<img", "<svg", "<b>", "</title><script>"] {
        assert!(!html.contains(hostile), "{hostile} got through");
    }
    assert!(html.contains("Reef &amp; &lt;Friends&gt;"));
    assert!(html.contains("2 embeds"));
    assert!(html.contains("who gone\">Deleted user"));
    assert!(html.contains("Closed by Deleted user"));
    assert!(!html.contains("http"), "the page loads nothing from anywhere");
    assert!(!html.contains("<script"));

    let quiet = TranscriptHead {
        with_content: false,
        ..head
    };
    let tr = render_html(&quiet, &[], &authors, Lang::Tr);
    assert!(tr.contains("lang=\"tr\"") && tr.contains("class=\"note\"") && tr.contains("class=\"empty\""));
}

#[test]
fn the_welcome_text_fills_its_placeholders() {
    assert_eq!(
        fill_welcome(
            "Hi {user}, ticket {number} on {server} ({topic})",
            "<@1>",
            "Reef",
            7,
            "Billing"
        ),
        "Hi <@1>, ticket 7 on Reef (Billing)"
    );
}
