use serde_json::json;

use crate::community::tags::*;

fn command() -> CommandDef {
    CommandDef {
        name: "Rules".into(),
        description: "  The   rules ".into(),
        responses: vec!["Be kind.".into()],
        embed: None,
        reply_mode: ReplyMode::Public,
        actions: Vec::new(),
        allow_roles: Vec::new(),
        deny_roles: Vec::new(),
        allow_channels: Vec::new(),
        deny_channels: Vec::new(),
        cooldown_secs: 0,
        enabled: true,
        allow_mentions: false,
    }
}

fn responder(kind: TriggerKind) -> ResponderDef {
    ResponderDef {
        name: "Greeter".into(),
        trigger_kind: kind,
        trigger_value: None,
        match_kind: None,
        responses: vec!["Hi {user}".into()],
        embed: None,
        destination: Destination::Channel,
        channel_id: Some(5),
        actions: Vec::new(),
        chance: 100,
        cooldown_secs: 0,
        allow_channels: Vec::new(),
        deny_channels: Vec::new(),
        enabled: true,
        allow_mentions: false,
    }
}

fn reserved() -> Vec<String> {
    vec!["help".into(), "tag".into()]
}

fn fields(errors: Vec<FieldError>) -> Vec<&'static str> {
    errors.into_iter().map(|e| e.field).collect()
}

fn pick_first(_: usize) -> usize {
    0
}

#[test]
fn a_command_is_made_tidy() {
    let clean = command().clean(&reserved()).expect("valid");
    assert_eq!(
        (clean.name.as_str(), clean.description.as_str()),
        ("rules", "The rules")
    );
}

#[test]
fn names_follow_the_rule_and_skip_the_bots_own_commands() {
    for name in ["a", "rules", "0day", "my-cmd_2", &"x".repeat(32), "/Slash"] {
        assert!(check_name(name, &reserved()).is_ok(), "{name}");
    }
    for name in ["", "-a", "_a", "has space", "dot.dot", "ünï", &"x".repeat(33)] {
        assert!(check_name(name, &reserved()).is_err(), "{name:?}");
    }
    assert_eq!(check_name("HELP", &reserved()).unwrap_err().message, RESERVED_NOTE);
}

#[test]
fn replies_are_counted_and_an_embed_can_stand_in_for_text() {
    let mut c = command();
    c.responses = vec![];
    assert_eq!(fields(c.clean(&reserved()).unwrap_err()), ["responses"]);
    let mut c = command();
    c.responses = vec!["x".into(); 6];
    assert_eq!(fields(c.clean(&reserved()).unwrap_err()), ["responses"]);
    let mut c = command();
    c.responses = vec!["é".repeat(RESPONSE_MAX + 1)];
    assert!(c.clean(&reserved()).unwrap_err()[0].message.contains("2001"));
    let mut c = command();
    c.responses = vec!["é".repeat(RESPONSE_MAX)];
    assert!(c.clean(&reserved()).is_ok(), "characters, not bytes");
    let mut c = command();
    c.responses = vec!["  ".into()];
    assert_eq!(fields(c.clean(&reserved()).unwrap_err()), ["responses"]);
    let mut c = command();
    c.responses = vec![String::new()];
    c.embed = Some(EmbedSpec {
        title: Some("Rules".into()),
        ..Default::default()
    });
    assert!(c.clean(&reserved()).is_ok(), "the embed carries the answer");
}

#[test]
fn embeds_are_limited_like_discord_limits_them() {
    let ok = |e: EmbedSpec| clean_embed(Some(e));
    let titled = |t: &str| EmbedSpec {
        title: Some(t.into()),
        ..Default::default()
    };
    assert!(ok(titled("Hello")).unwrap().is_some());
    assert_eq!(clean_embed(None).unwrap(), None);
    assert!(ok(titled(&"x".repeat(257))).is_err());
    assert!(ok(EmbedSpec::default()).is_err(), "an empty embed is refused");
    assert!(
        ok(EmbedSpec {
            title: Some("  ".into()),
            ..Default::default()
        })
        .is_err()
    );
    let mut e = titled("T");
    e.color = Some("ff6b4a".into());
    assert_eq!(ok(e).unwrap().unwrap().color.as_deref(), Some("#ff6b4a"));
    let mut e = titled("T");
    e.color = Some("red".into());
    assert!(ok(e).is_err());
    let mut e = titled("T");
    e.image = Some("http://example.com/a.png".into());
    assert!(ok(e).is_err(), "https only");
    let mut e = titled("T");
    e.image = Some("https://example.com/a.png".into());
    assert!(ok(e).is_ok());
    let mut e = titled("T");
    e.url = Some("https://example.com".into());
    assert!(ok(e).is_ok());
    let no_title = EmbedSpec {
        description: Some("d".into()),
        url: Some("https://example.com".into()),
        ..Default::default()
    };
    assert!(ok(no_title).is_err(), "a link needs a title");
    let field = |n: &str, v: &str| EmbedFieldSpec {
        name: n.into(),
        value: v.into(),
        inline: false,
    };
    let mut e = titled("T");
    e.fields = (0..26).map(|_| field("a", "b")).collect();
    assert!(ok(e).is_err(), "26 fields");
    let mut e = titled("T");
    e.fields = vec![field("a", "")];
    assert!(ok(e).is_err(), "an empty field value");
    let mut e = EmbedSpec {
        description: Some("d".repeat(4_096)),
        footer: Some("f".repeat(2_048)),
        ..Default::default()
    };
    let error = &ok(e.clone()).unwrap_err()[0].message;
    assert!(
        error.contains("6000") && error.contains("6144"),
        "over the total: {error}"
    );
    e.footer = Some("f".repeat(1_000));
    assert!(ok(e).is_ok(), "under the total");
}

#[test]
fn actions_are_limited_and_checked() {
    let add = |id| Action::AddRole { role_id: id };
    assert!(clean_actions(vec![add(1), add(2), add(3)], false, true).is_ok());
    assert!(clean_actions(vec![add(1), add(2), add(3), add(4)], false, true).is_err());
    assert!(clean_actions(vec![add(0)], false, true).is_err());
    for amount in [0, 1_001, -1_001] {
        assert!(
            clean_actions(vec![Action::GivePoints { amount }], false, true).is_err(),
            "{amount}"
        );
    }
    for amount in [1, -1, 1_000, -1_000] {
        assert!(
            clean_actions(vec![Action::GivePoints { amount }], false, true).is_ok(),
            "{amount}"
        );
    }
    let dm = Action::Dm { text: "  hi ".into() };
    assert_eq!(
        clean_actions(vec![dm], false, true).unwrap(),
        [Action::Dm { text: "hi".into() }]
    );
    assert!(clean_actions(vec![Action::Dm { text: " ".into() }], false, true).is_err());
    let react = || Action::React { emoji: "🎉".into() };
    assert!(clean_actions(vec![react()], true, true).is_ok());
    assert!(
        clean_actions(vec![react()], false, true).is_err(),
        "only a keyword has a message"
    );
    assert!(
        clean_actions(
            vec![Action::React {
                emoji: "two words".into()
            }],
            true,
            true
        )
        .is_err()
    );
    assert!(clean_actions(vec![add(1)], true, false).is_err(), "the member is gone");
    assert!(clean_actions(vec![], true, false).unwrap().is_empty());
}

#[test]
fn actions_travel_as_json_with_string_ids() {
    let action = Action::ToggleRole {
        role_id: 123_456_789_012_345_678,
    };
    let value = serde_json::to_value(&action).unwrap();
    assert_eq!(value, json!({ "kind": "toggle_role", "role_id": "123456789012345678" }));
    assert_eq!(serde_json::from_value::<Action>(value).unwrap(), action);
    assert_eq!(
        serde_json::from_value::<Action>(json!({ "kind": "add_role", "role_id": 5 })).unwrap(),
        Action::AddRole { role_id: 5 }
    );
    assert!(serde_json::from_value::<Action>(json!({ "kind": "add_role", "role_id": "x" })).is_err());
    assert!(serde_json::from_value::<Action>(json!({ "kind": "give_points", "amount": 5, "extra": 1 })).is_err());
    assert!(serde_json::from_value::<Action>(json!({ "kind": "format_disk" })).is_err());
}

#[test]
fn a_command_reads_from_json_with_defaults() {
    let def: CommandDef = serde_json::from_value(json!({
        "name": "hi", "responses": ["hello"], "allow_roles": ["5", 6], "deny_channels": ["7"]
    }))
    .unwrap();
    assert_eq!(def.allow_roles, [5, 6]);
    assert_eq!(def.deny_channels, [7]);
    assert!(def.enabled && !def.allow_mentions);
    assert_eq!(def.reply_mode, ReplyMode::Public);
    let back = serde_json::to_value(&def).unwrap();
    assert_eq!(back["allow_roles"], json!(["5", "6"]));
    assert!(serde_json::from_value::<CommandDef>(json!({ "name": "x", "responses": ["y"], "surprise": 1 })).is_err());
}

#[test]
fn lists_and_cooldowns_are_bounded() {
    let mut c = command();
    c.allow_roles = (1..=26).collect();
    assert_eq!(fields(c.clean(&reserved()).unwrap_err()), ["allow_roles"]);
    let mut c = command();
    c.deny_roles = vec![3, 3, 4];
    assert_eq!(c.clean(&reserved()).unwrap().deny_roles, [3, 4], "repeats go");
    let mut c = command();
    c.cooldown_secs = 3_601;
    assert_eq!(fields(c.clean(&reserved()).unwrap_err()), ["cooldown_secs"]);
    let mut c = command();
    c.cooldown_secs = -1;
    assert_eq!(fields(c.clean(&reserved()).unwrap_err()), ["cooldown_secs"]);
    let mut c = command();
    c.cooldown_secs = 3_600;
    assert!(c.clean(&reserved()).is_ok());
}

#[test]
fn actions_need_a_cooldown_of_their_own() {
    let mut c = command();
    c.actions = vec![Action::GivePoints { amount: 1_000 }];
    for seconds in [0, ACTION_COOLDOWN_MIN - 1] {
        c.cooldown_secs = seconds;
        let errors = c.clone().clean(&reserved()).unwrap_err();
        assert_eq!(fields(errors.clone()), ["cooldown_secs"], "{seconds}");
        assert!(
            errors[0].message.contains("at least 30 seconds"),
            "{}",
            errors[0].message
        );
    }
    c.cooldown_secs = ACTION_COOLDOWN_MIN;
    assert!(c.clean(&reserved()).is_ok());
    let mut r = responder(TriggerKind::LevelUp);
    r.actions = vec![Action::Dm { text: "hi".into() }];
    assert_eq!(fields(r.clone().clean().unwrap_err()), ["cooldown_secs"]);
    r.cooldown_secs = 60;
    assert!(r.clean().is_ok());
    // Without actions any cooldown is fine; a stored row without one still runs with the floor.
    assert!(command().clean(&reserved()).is_ok());
    assert_eq!(run_cooldown(0, &[]), 0);
    assert_eq!(
        run_cooldown(0, &[Action::GivePoints { amount: 5 }]),
        ACTION_COOLDOWN_MIN
    );
    assert_eq!(run_cooldown(90, &[Action::GivePoints { amount: 5 }]), 90);
}

#[test]
fn who_may_run_a_command_and_where() {
    let mut c = command();
    assert_eq!(c.allows(&[], &[1]), Ok(()));
    c.allow_roles = vec![10];
    assert!(c.allows(&[], &[1]).is_err());
    assert!(c.allows(&[10, 11], &[1]).is_ok());
    c.deny_roles = vec![11];
    assert!(c.allows(&[10, 11], &[1]).is_err(), "a deny wins");
    c.allow_channels = vec![2];
    assert!(c.allows(&[10], &[1]).is_err(), "not in the allowed channel");
    assert!(c.allows(&[10], &[1, 2]).is_ok(), "its category counts");
    c.deny_channels = vec![1];
    assert!(c.allows(&[10], &[1, 2]).is_err());
}

#[test]
fn a_responder_needs_what_its_trigger_needs() {
    let mut r = responder(TriggerKind::Keyword);
    assert_eq!(fields(r.clone().clean().unwrap_err()), ["trigger_value"]);
    r.trigger_value = Some("  Hello  ".into());
    let clean = r.clone().clean().unwrap();
    assert_eq!(clean.trigger_value.as_deref(), Some("Hello"));
    assert_eq!(clean.match_kind, Some(MatchKind::Contains), "contains unless said");

    let mut r = responder(TriggerKind::RoleAdded);
    assert_eq!(fields(r.clone().clean().unwrap_err()), ["trigger_value"]);
    r.trigger_value = Some("0".into());
    assert!(r.clone().clean().is_err());
    r.trigger_value = Some("42".into());
    assert_eq!(r.clean().unwrap().watched_role(), Some(42));

    let mut r = responder(TriggerKind::LevelUp);
    assert_eq!(r.clone().clean().unwrap().min_level(), 1);
    r.trigger_value = Some("10".into());
    assert_eq!(r.clone().clean().unwrap().min_level(), 10);
    r.trigger_value = Some("0".into());
    assert_eq!(fields(r.clone().clean().unwrap_err()), ["trigger_value"]);
    r.trigger_value = Some("many".into());
    assert!(r.clean().is_err());

    let mut r = responder(TriggerKind::MemberJoin);
    r.trigger_value = Some("ignored".into());
    r.match_kind = Some(MatchKind::Exact);
    let clean = r.clean().unwrap();
    assert_eq!((clean.trigger_value, clean.match_kind), (None, None));
}

#[test]
fn a_responder_answers_somewhere_that_exists() {
    let mut r = responder(TriggerKind::MemberJoin);
    r.destination = Destination::Current;
    assert_eq!(fields(r.clone().clean().unwrap_err()), ["destination"]);
    r.destination = Destination::Dm;
    assert!(r.clone().clean().unwrap().channel_id.is_none());
    r.destination = Destination::Channel;
    r.channel_id = None;
    assert_eq!(fields(r.clone().clean().unwrap_err()), ["channel_id"]);

    let mut leave = responder(TriggerKind::MemberLeave);
    leave.destination = Destination::Dm;
    assert_eq!(fields(leave.clone().clean().unwrap_err()), ["destination"]);
    leave.destination = Destination::Channel;
    leave.actions = vec![Action::GivePoints { amount: 5 }];
    assert_eq!(
        fields(leave.clean().unwrap_err()),
        ["actions"],
        "nobody left to give to"
    );

    let mut talk = responder(TriggerKind::Keyword);
    talk.trigger_value = Some("hi".into());
    talk.destination = Destination::Current;
    talk.actions = vec![Action::React { emoji: "👋".into() }];
    talk.cooldown_secs = ACTION_COOLDOWN_MIN;
    assert!(talk.clean().unwrap().channel_id.is_none());
}

#[test]
fn chance_and_cooldown_have_ranges() {
    for chance in [0, 101, -5] {
        let mut r = responder(TriggerKind::MemberJoin);
        r.chance = chance;
        assert_eq!(fields(r.clean().unwrap_err()), ["chance"], "{chance}");
    }
    let mut r = responder(TriggerKind::MemberJoin);
    r.chance = 1;
    r.cooldown_secs = 86_400;
    assert!(r.clone().clean().is_ok());
    r.cooldown_secs = 86_401;
    assert_eq!(fields(r.clean().unwrap_err()), ["cooldown_secs"]);
}

#[test]
fn keywords_match_by_kind_without_case() {
    let hears = |kind: MatchKind, text: &str| kind.matches("Hello", text);
    assert!(hears(MatchKind::Contains, "oh HELLO there"));
    assert!(!hears(MatchKind::Contains, "help"));
    assert!(hears(MatchKind::Exact, "  hello "));
    assert!(!hears(MatchKind::Exact, "hello there"));
    assert!(hears(MatchKind::StartsWith, "Hello world"));
    assert!(!hears(MatchKind::StartsWith, "oh hello"));
    assert!(hears(MatchKind::EndsWith, "oh hello"));
    assert!(!hears(MatchKind::Contains, ""));
    assert!(
        !MatchKind::Contains.matches("  ", "anything"),
        "an empty keyword hears nothing"
    );
    let mut r = responder(TriggerKind::Keyword);
    r.trigger_value = Some("gm".into());
    r.match_kind = Some(MatchKind::Exact);
    assert!(r.hears("GM") && !r.hears("gm all"));
    assert!(!responder(TriggerKind::MemberJoin).hears("gm"));
}

#[test]
fn responder_channels_follow_allow_and_deny() {
    let mut r = responder(TriggerKind::Keyword);
    assert!(r.allows_channel(&[1]));
    r.allow_channels = vec![2];
    assert!(!r.allows_channel(&[1]));
    assert!(r.allows_channel(&[1, 2]));
    r.deny_channels = vec![1];
    assert!(!r.allows_channel(&[1, 2]));
}

fn filled(template: &str, vars: &Vars) -> String {
    render(template, vars, &mut pick_first)
}

#[test]
fn named_placeholders_are_filled_once() {
    let mut v = Vars::new();
    v.set("user", "<@1>").set("server", "Reef");
    assert_eq!(
        filled("Hi {user}, welcome to {server}! {user}", &v),
        "Hi <@1>, welcome to Reef! <@1>"
    );
    assert_eq!(filled("{level} {", &v), "{level} {", "unknown and unclosed stay");
    assert_eq!(filled("{{user}}", &v), "{<@1>}");
    v.set("user", "{server}");
    assert_eq!(
        filled("{user} in {server}", &v),
        "{server} in Reef",
        "a value is not filled again"
    );
    assert_eq!(filled("", &v), "");
    assert_eq!(filled("👋 {server} — ok", &v), "👋 Reef — ok");
}

#[test]
fn words_come_from_what_was_typed() {
    let mut v = Vars::new();
    v.args("red  green blue yellow orange purple");
    assert_eq!(filled("{args}", &v), "red  green blue yellow orange purple");
    assert_eq!(filled("{1} {2} {5}", &v), "red green orange");
    assert_eq!(filled("{6}", &v), "{6}", "only five words");
    assert_eq!(filled("{0}", &v), "{0}");
    let mut v = Vars::new();
    v.args("");
    assert_eq!(filled("[{1}] [{1:friend}]", &v), "[] [friend]");
    v.args("Sam");
    assert_eq!(filled("Hi {1:friend}!", &v), "Hi Sam!");
    assert_eq!(filled("{01}", &v), "Sam", "leading zeros count as the number");
    assert_eq!(filled("{+1}", &v), "{+1}");
}

#[test]
fn random_picks_by_the_index_it_is_given() {
    let v = Vars::new();
    let mut last = |n: usize| n - 1;
    assert_eq!(render("{random:a|b|c}", &v, &mut last), "c");
    assert_eq!(render("{random:only}", &v, &mut last), "only");
    assert_eq!(render("{random:}", &v, &mut last), "");
    let mut high = |_: usize| 99;
    assert_eq!(
        render("{random:a|b}", &v, &mut high),
        "b",
        "an index past the end is held back"
    );
    assert_eq!(render("{random}", &v, &mut last), "{random}");
    assert_eq!(render("x {random:a|b} y {random:c|d}", &v, &mut last), "x b y d");
    let many = format!(
        "{{random:{}}}",
        (0..40).map(|n| n.to_string()).collect::<Vec<_>>().join("|")
    );
    assert_eq!(render(&many, &v, &mut last), "24", "the first 25 choices count");
}

#[test]
fn the_catalogue_names_what_the_editor_offers() {
    let all = command_placeholders();
    assert!(all.iter().any(|p| p.token == "{args}") && all.iter().any(|p| p.token == "{1:friend}"));
    for kind in TriggerKind::ALL {
        let tokens: Vec<&str> = responder_placeholders(kind).iter().map(|p| p.token).collect();
        assert!(tokens.contains(&"{user}") && tokens.contains(&"{points}"), "{kind:?}");
        assert!(!tokens.contains(&"{args}"), "{kind:?}");
    }
    let tokens = |kind| responder_placeholders(kind).iter().map(|p| p.token).collect::<Vec<_>>();
    assert!(tokens(TriggerKind::LevelUp).contains(&"{level}"));
    assert!(tokens(TriggerKind::MemberJoin).contains(&"{count}"));
    assert!(tokens(TriggerKind::RoleAdded).contains(&"{role}"));
    assert!(!tokens(TriggerKind::Keyword).contains(&"{level}"));
}

#[test]
fn long_text_is_cut_with_an_ellipsis() {
    assert_eq!(truncate("short", 10), "short");
    assert_eq!(truncate("abcdef", 4), "abc…");
    assert_eq!(truncate("éééé", 4), "éééé");
    assert_eq!(truncate("", 0), "");
}

#[test]
fn keys_round_trip() {
    for kind in TriggerKind::ALL {
        assert_eq!(TriggerKind::parse(kind.key()), Some(kind));
        assert_eq!(serde_json::to_value(kind).unwrap(), json!(kind.key()));
    }
    for kind in MatchKind::ALL {
        assert_eq!(MatchKind::parse(kind.key()), Some(kind));
        assert_eq!(serde_json::to_value(kind).unwrap(), json!(kind.key()));
    }
    for d in [Destination::Current, Destination::Channel, Destination::Dm] {
        assert_eq!(Destination::parse(d.key()), Some(d));
        assert_eq!(serde_json::to_value(d).unwrap(), json!(d.key()));
    }
    assert_eq!(ReplyMode::parse("ephemeral"), Some(ReplyMode::Ephemeral));
    assert_eq!(Action::GivePoints { amount: 1 }.key(), "give_points");
}

#[test]
fn an_export_wraps_its_items_with_a_version() {
    let export = Export::new(vec![command()]);
    let value = serde_json::to_value(&export).unwrap();
    assert_eq!(value["version"], 1);
    let back: Export<CommandDef> = serde_json::from_value(value).unwrap();
    assert_eq!(back, export);
}
