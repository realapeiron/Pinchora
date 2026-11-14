use pinchora_core::community::tags::EmbedFieldSpec;
use serde_json::json;

use crate::admin::custom::*;

fn first(_: usize) -> usize {
    0
}

fn spec<'a>(responses: &'a [String], embed: Option<&'a EmbedSpec>, allow: bool) -> Spec<'a> {
    Spec {
        responses,
        embed,
        allow_mentions: allow,
    }
}

#[test]
fn mentions_in_a_template_are_dropped_unless_the_server_allows_them() {
    let responses = vec!["hi <@123> and <@&456>, @everyone @here".to_owned()];
    let mut vars = Vars::new();
    vars.set("user", "<@9>");
    let off = render_message(&spec(&responses, None, false), &vars, &mut first);
    assert_eq!(off.content, "hi and , everyone here");
    assert!(off.users.is_empty() && off.roles.is_empty());
    let on = render_message(&spec(&responses, None, true), &vars, &mut first);
    assert_eq!(
        on.content, "hi <@123> and <@&456>, everyone here",
        "never everyone or here"
    );
    assert_eq!((on.users, on.roles), (vec![123], vec![456]));
}

#[test]
fn what_a_member_typed_cannot_ping_whatever_the_server_allows() {
    let responses = vec!["{user.name} says: {args}".to_owned()];
    let mut vars = Vars::new();
    vars.set("user.name", scrub_value("Sneaky <@7> @everyone"));
    vars.args(&scrub_value("<@&5> look <@!6> @here"));
    for allow in [false, true] {
        let r = render_message(&spec(&responses, None, allow), &vars, &mut first);
        assert_eq!(r.content, "Sneaky everyone says: look here");
        assert!(r.users.is_empty() && r.roles.is_empty(), "allow {allow}");
    }
}

#[test]
fn the_sender_of_the_message_is_the_only_one_pinged_by_default() {
    let responses = vec!["{user}".to_owned()];
    let mut vars = Vars::new();
    vars.set("user", mention(42));
    let r = render_message(&spec(&responses, None, false), &vars, &mut first);
    let (content, _, mentions) = to_discord(&r, 1, 42);
    assert_eq!(content, "<@42>");
    assert!(mentions.parse.is_empty(), "nothing is parsed from the text");
    assert_eq!(mentions.users, [Id::new(42)]);
    assert!(mentions.roles.is_empty());
    // The server's own @everyone role is never pingable.
    let r = Rendered {
        content: "<@&1>".into(),
        embed: None,
        users: vec![42, 7],
        roles: vec![1, 5],
    };
    let (_, _, mentions) = to_discord(&r, 1, 42);
    assert_eq!(mentions.users, [Id::new(42), Id::new(7)]);
    assert_eq!(mentions.roles, [Id::new(5)]);
}

#[test]
fn a_reply_is_picked_filled_and_cut_to_discords_limit() {
    let responses = vec!["one".to_owned(), "two {1:x}".to_owned()];
    let mut vars = Vars::new();
    vars.args("");
    let mut last = |n: usize| n - 1;
    assert_eq!(
        render_message(&spec(&responses, None, false), &vars, &mut last).content,
        "two x"
    );
    let long = vec!["é{args}".to_owned()];
    let mut vars = Vars::new();
    vars.args(&"é".repeat(RESPONSE_MAX));
    let r = render_message(&spec(&long, None, false), &vars, &mut first);
    assert_eq!(r.content.chars().count(), RESPONSE_MAX);
    assert!(r.content.ends_with('…'));
}

#[test]
fn an_embed_is_filled_scrubbed_and_kept_under_the_total() {
    let embed = EmbedSpec {
        title: Some("Hello {user.name} <@5>".into()),
        description: Some("d".repeat(4_096)),
        footer: Some("{args}".into()),
        fields: vec![EmbedFieldSpec {
            name: "n".into(),
            value: "v".repeat(1_024),
            inline: true,
        }],
        ..Default::default()
    };
    let responses = vec![String::new()];
    let mut vars = Vars::new();
    vars.set("user.name", "Sam");
    vars.args(&"f".repeat(2_000));
    let r = render_message(&spec(&responses, Some(&embed), false), &vars, &mut first);
    let e = r.embed.expect("an embed");
    assert_eq!(e.title.as_deref(), Some("Hello Sam"), "the mention is gone");
    assert!(e.footer.as_deref().unwrap().chars().count() <= EMBED_FOOTER_MAX);
    assert!(e.total_chars() <= EMBED_TOTAL_MAX, "{}", e.total_chars());
    assert!(e.fields[0].inline);
    let built = embed_of(&e);
    assert_eq!(built.fields.len(), 1);
    assert_eq!(built.title.as_deref(), Some("Hello Sam"));
}

#[test]
fn an_embeds_author_and_footer_show_names_without_escapes() {
    let embed = EmbedSpec {
        description: Some("{user.name}".into()),
        author: Some("{user.name}".into()),
        footer: Some("by {user.name} \\\\".into()),
        ..Default::default()
    };
    let mut vars = Vars::new();
    vars.set("user.name", scrub_value(r"captain_crab-(\*)"));
    let r = render_message(&spec(&[String::new()], Some(&embed), false), &vars, &mut first);
    let e = r.embed.expect("an embed");
    assert_eq!(e.description.as_deref(), Some(r"captain\_crab\-\(\\\*\)"));
    assert_eq!(e.author.as_deref(), Some(r"captain_crab-(\*)"));
    assert_eq!(e.footer.as_deref(), Some(r"by captain_crab-(\*) \"));
}

#[test]
fn mention_ids_find_users_and_roles() {
    assert_eq!(mention_ids("<@1> <@!2> <@&3> <@1>"), (vec![1, 2], vec![3]));
    assert_eq!(mention_ids("<@abc> <@> <@12 <#5> @1"), (vec![], vec![]));
    assert_eq!(mention_ids("<@<@4>"), (vec![4], vec![]));
}

#[test]
fn defang_removes_only_the_at_sign() {
    assert_eq!(defang("hi @Everyone and @HERE"), "hi Everyone and HERE");
    assert_eq!(
        defang("@@everyone @@@here"),
        "everyone here",
        "removing one cannot make another"
    );
    assert_eq!(
        defang("mail me@here.com"),
        "mail mehere.com",
        "the same tradeoff as the guestbook"
    );
    assert!(!defang("@eve@everyoneryone").contains("@everyone"));
}

#[test]
fn templates_lists_every_text() {
    let embed = EmbedSpec {
        title: Some("t".into()),
        fields: vec![EmbedFieldSpec {
            name: "n".into(),
            value: "v".into(),
            inline: false,
        }],
        ..Default::default()
    };
    let responses = vec!["a".to_owned(), "b".to_owned()];
    let spec = spec(&responses, Some(&embed), false);
    assert_eq!(templates(&spec), ["a", "b", "t", "n", "v"]);
}

#[test]
fn a_diff_keeps_only_what_changed() {
    let before = json!({ "name": "a", "cooldown_secs": 0, "responses": ["x"] });
    let after = json!({ "name": "a", "cooldown_secs": 5, "responses": ["x", "y"], "enabled": true });
    let (old, new) = def_diff(&before, &after);
    assert_eq!(old, json!({ "cooldown_secs": 0, "responses": ["x"], "enabled": null }));
    assert_eq!(
        new,
        json!({ "cooldown_secs": 5, "responses": ["x", "y"], "enabled": true })
    );
    let (old, new) = def_diff(&before, &before);
    assert_eq!((old, new), (json!({}), json!({})));
}
