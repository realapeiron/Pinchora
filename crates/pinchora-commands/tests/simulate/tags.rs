//! Custom commands and auto-responders through the real handlers: written with `/customcommand`'s forms,
//! run with `/tag` (autocomplete, cooldowns, restrictions, actions), and the responders that answer joins,
//! leaves, level-ups, role changes and keywords.

use std::time::Duration;

use pinchora_core::tags::{CommandDef, ResponderDef};
use twilight_model::gateway::payload::incoming::MemberUpdate;

use super::*;

/// A command as the database stores it, with only the name and reply filled in.
fn command(name: &str, reply: &str) -> CommandDef {
    serde_json::from_value(json!({ "name": name, "responses": [reply] })).expect("a command")
}

/// A responder of `kind` that posts `text` in `channel`.
fn responder(kind: &str, text: &str, channel: u64) -> ResponderDef {
    serde_json::from_value(json!({
        "name": format!("{kind} responder"),
        "trigger_kind": kind,
        "responses": [text],
        "destination": "channel",
        "channel_id": channel.to_string(),
    }))
    .expect("a responder")
}

/// Puts commands straight into the database and drops the bot's copy, as a dashboard save would.
async fn stock(sim: &Sim, commands: &[CommandDef]) {
    for def in commands {
        sim.app
            .db
            .add_custom_command(sim.guild, def, 1)
            .await
            .expect("add a command");
    }
    sim.app.forget_guild_settings(sim.guild).await;
}

async fn stock_responders(sim: &Sim, responders: &[ResponderDef]) {
    for def in responders {
        sim.app
            .db
            .add_custom_responder(sim.guild, def, 1)
            .await
            .expect("add a responder");
    }
    sim.app.forget_guild_settings(sim.guild).await;
}

/// Ends the cooldowns `user` has on the server's commands, as if the time had passed (a command with
/// actions always has one).
async fn cool_off(sim: &Sim, user: u64) {
    for row in sim.app.db.custom_commands(sim.guild).await.expect("the commands") {
        let key = format!("tag:{}:{}:{user}", sim.guild, row.id);
        sim.app.cache.release_cooldown(&key).await.expect("end a cooldown");
    }
}

fn tag(name: &str, args: Option<&str>) -> Value {
    let mut options = vec![text_option("name", name)];
    options.extend(args.map(|a| text_option("args", a)));
    json!(options)
}

fn manage(sub_command: Value) -> Value {
    json!([sub_command])
}

/// What Discord would suggest while `typed` is in the `name` box of `/tag` or `/customcommand <sub_command>`.
async fn suggest(sim: &Sim, user: u64, command: &str, sub_command: Option<&str>, typed: &str) -> Vec<String> {
    let focused = json!({ "name": "name", "type": 3, "value": typed, "focused": true });
    let options = match sub_command {
        Some(name) => json!([{ "name": name, "type": 1, "options": [focused] }]),
        None => json!([focused]),
    };
    let data = json!({ "id": unique().to_string(), "name": command, "type": 1, "options": options });
    let interaction = sim.interaction(user, 4, data, None);
    let (tx, mut rx) = oneshot::channel();
    let responder = Arc::new(Responder::http(sim.app.clone(), &interaction, tx));
    assert_eq!(responder.deferral(), Deferral::Never, "suggestions are never deferred");
    handle_interaction(sim.app.clone(), interaction, responder).await;
    let first = rx.try_recv().expect("the autocomplete was never answered");
    assert_eq!(
        first.kind,
        InteractionResponseType::ApplicationCommandAutocompleteResult
    );
    first
        .data
        .and_then(|data| data.choices)
        .expect("suggestions")
        .into_iter()
        .map(|choice| match choice.value {
            twilight_model::application::command::CommandOptionChoiceValue::String(name) => name,
            other => panic!("a suggestion that is not text: {other:?}"),
        })
        .collect()
}

/// The users and roles a reply may ping.
fn pings(reply: &Reply) -> (Vec<String>, Vec<String>) {
    let mentions = reply.mentions.clone().expect("every answer sets allowed_mentions");
    let list = |key: &str| {
        mentions
            .get(key)
            .and_then(Value::as_array)
            .map(|a| a.iter().filter_map(|v| v.as_str().map(str::to_owned)).collect())
            .unwrap_or_default()
    };
    assert_eq!(mentions["parse"], json!([]), "nothing is parsed out of the text");
    (list("users"), list("roles"))
}

/// Posts made since the last call, waiting a moment for ones a spawned task makes.
async fn posts_after(sim: &Sim, wanted: impl Fn(&[Post]) -> bool) -> Vec<Post> {
    let mut seen = Vec::new();
    for _ in 0..60 {
        seen.extend(sim.posts());
        if wanted(&seen) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    seen
}

/// A chat message saying `text`, as the gateway delivers it.
fn chat_saying(sim: &Sim, user: u64, channel: u64, roles: &[u64], text: &str) -> MessageCreate {
    let mut message = serde_json::to_value(sim.chat_in(user, channel, roles)).expect("message JSON");
    message["content"] = json!(text);
    serde_json::from_value(message).expect("message JSON")
}

async fn say_saying(sim: &Sim, user: u64, channel: u64, text: &str) {
    on_message(sim.app.clone(), chat_saying(sim, user, channel, &[], text))
        .await
        .expect("the message handler failed");
}

/// A member update that leaves `user` holding `roles`.
async fn hold_roles(sim: &Sim, user: u64, roles: &[u64]) {
    let event: MemberUpdate = serde_json::from_value(json!({
        "guild_id": sim.guild.to_string(),
        "user": person_json(user, "Robin", Some("Robin"), false),
        "nick": null,
        "roles": roles.iter().map(u64::to_string).collect::<Vec<_>>(),
        "avatar": null,
        "communication_disabled_until": null,
        "joined_at": null,
        "deaf": null,
        "mute": null,
        "pending": false,
        "premium_since": null,
    }))
    .expect("member update JSON");
    on_member_update(sim.app.clone(), event)
        .await
        .expect("the update handler failed");
}

#[tokio::test]
async fn custom_commands_are_written_in_forms_and_run_with_tag() {
    let Some(mut sim) = Sim::new("custom_commands_are_written_in_forms_and_run_with_tag").await else {
        return;
    };
    let (manager, member) = (unique(), unique());

    // Only managers write commands.
    let denied = sim
        .command(
            member,
            "customcommand",
            manage(sub("create", vec![text_option("name", "rules")])),
        )
        .await;
    assert!(
        denied.ephemeral && denied.content().contains("Manage Server"),
        "{}",
        denied.content()
    );
    sim.permissions = MANAGE_GUILD;

    // A name the bot cannot take is refused before any form opens.
    for (name, expected) in [
        ("help", "own commands"),
        ("has space", "lowercase letters"),
        ("-x", "lowercase letters"),
    ] {
        let bad = sim
            .command(
                manager,
                "customcommand",
                manage(sub("create", vec![text_option("name", name)])),
            )
            .await;
        assert!(
            bad.ephemeral && bad.modal.is_none() && bad.content().contains(expected),
            "{name}: {}",
            bad.content()
        );
    }

    // The form: the name is cleaned up, the description option is filled in for the manager.
    let form = sim
        .command(
            manager,
            "customcommand",
            manage(sub(
                "create",
                vec![
                    text_option("name", "Rules"),
                    text_option("description", "The server rules"),
                ],
            )),
        )
        .await;
    assert_eq!(form.modal.as_ref().map(|m| m.0.as_str()), Some("cm:c:rules:0"));
    assert!(
        form.modal_inputs
            .iter()
            .any(|(id, value)| id == "description" && value.as_deref() == Some("The server rules")),
        "{:?}",
        form.modal_inputs
    );
    let done = sim
        .submit_fields(
            manager,
            &form,
            &[
                ("response", "Be kind, {user}. {1:Friend} said: {args}"),
                ("description", "The server rules"),
                ("cooldown", ""),
            ],
        )
        .await;
    assert!(
        done.ephemeral && done.content().contains("Created `/tag rules`"),
        "{}",
        done.content()
    );
    let again = sim
        .command(
            manager,
            "customcommand",
            manage(sub("create", vec![text_option("name", "rules")])),
        )
        .await;
    assert!(again.content().contains("already exists"), "{}", again.content());

    // Anyone runs it. Typed text cannot ping: mentions and broadcasts in the arguments are dropped.
    sim.permissions = 0;
    let ran = sim
        .command(member, "tag", tag("rules", Some("<@5> hello @everyone friends")))
        .await;
    assert!(!ran.ephemeral, "public unless the command says otherwise");
    assert_eq!(
        ran.content(),
        format!("Be kind, <@{member}>. hello said: hello everyone friends")
    );
    let (users, roles) = pings(&ran);
    assert_eq!(
        (users, roles),
        (vec![member.to_string()], Vec::<String>::new()),
        "only the one who ran it"
    );
    let bare = sim.command(member, "tag", tag("rules", None)).await;
    assert_eq!(
        bare.content(),
        format!("Be kind, <@{member}>. Friend said:"),
        "the end is trimmed"
    );
    let unknown = sim.command(member, "tag", tag("nope", None)).await;
    assert!(
        unknown.ephemeral && unknown.content().contains("no custom command"),
        "{}",
        unknown.content()
    );
    let stored = sim.app.db.custom_commands(sim.guild).await.unwrap();
    assert_eq!(
        (stored.len(), stored[0].uses),
        (1, 2),
        "every run is counted, a missing one is not"
    );

    // Suggestions as it is typed: the name, then the description.
    let mut pets = command("pets", "Pets are great.");
    pets.description = "All about pets.".into();
    stock(&sim, &[command("rulebook", "Read it."), pets]).await;
    assert_eq!(suggest(&sim, member, "tag", None, "rul").await, ["rulebook", "rules"]);
    assert_eq!(
        suggest(&sim, member, "tag", None, "about").await,
        ["pets"],
        "the description counts"
    );
    assert_eq!(suggest(&sim, member, "tag", None, "").await.len(), 3);
    assert!(
        suggest(&sim, member, "customcommand", Some("edit"), "pet")
            .await
            .is_empty()
    );
    sim.permissions = MANAGE_GUILD;
    assert_eq!(
        suggest(&sim, manager, "customcommand", Some("edit"), "pet").await,
        ["pets"]
    );

    // The manager's views: list, info.
    sim.permissions = MANAGE_GUILD;
    let list = sim.command(manager, "customcommand", manage(sub("list", vec![]))).await;
    assert!(
        list.ephemeral && list.title().contains("Custom commands (3/50)"),
        "{}",
        list.title()
    );
    assert!(
        list.description().contains("`/tag rules` · The server rules · used 2×"),
        "{}",
        list.description()
    );
    let info = sim
        .command(
            manager,
            "customcommand",
            manage(sub("info", vec![text_option("name", "rules")])),
        )
        .await;
    assert!(
        info.ephemeral && info.title() == "/tag rules" && info.field("Used") == "2×",
        "{:?}",
        info.embeds
    );

    // Edit: the form is filled in with what is there; privacy and a cooldown change at once.
    let edit = sim
        .command(
            manager,
            "customcommand",
            manage(sub(
                "edit",
                vec![text_option("name", "rules"), bool_option("private", true)],
            )),
        )
        .await;
    assert!(
        edit.modal
            .as_ref()
            .is_some_and(|m| m.0.starts_with("cm:e:") && m.0.ends_with(":1")),
        "{:?}",
        edit.modal
    );
    assert_eq!(
        edit.modal_inputs[0].1.as_deref(),
        Some("Be kind, {user}. {1:Friend} said: {args}")
    );
    let saved = sim
        .submit_fields(
            manager,
            &edit,
            &[
                ("response", "Rule one: be kind."),
                ("description", "Rules"),
                ("cooldown", "30"),
            ],
        )
        .await;
    assert!(
        saved.ephemeral && saved.content().contains("Saved `/tag rules`"),
        "{}",
        saved.content()
    );
    let bad_cooldown = sim
        .submit_fields(
            manager,
            &edit,
            &[("response", "x"), ("description", ""), ("cooldown", "soon")],
        )
        .await;
    assert!(
        bad_cooldown.content().contains("whole number of seconds"),
        "{}",
        bad_cooldown.content()
    );
    sim.permissions = 0;
    let private = sim.command(member, "tag", tag("rules", None)).await;
    assert!(
        private.ephemeral && private.content() == "Rule one: be kind.",
        "{}",
        private.content()
    );
    let cooling = sim.command(member, "tag", tag("rules", None)).await;
    assert!(
        cooling.ephemeral && cooling.content().contains("cooling down"),
        "the cooldown is per member: {}",
        cooling.content()
    );
    let other = sim.command(unique(), "tag", tag("rules", None)).await;
    assert_eq!(other.content(), "Rule one: be kind.");

    // Delete, and the audit trail from Discord.
    sim.permissions = MANAGE_GUILD;
    let gone = sim
        .command(
            manager,
            "customcommand",
            manage(sub("delete", vec![text_option("name", "rules")])),
        )
        .await;
    assert!(gone.content().contains("Removed `/tag rules`"), "{}", gone.content());
    sim.permissions = 0;
    let missing = sim.command(member, "tag", tag("rules", None)).await;
    assert!(missing.content().contains("no custom command"), "{}", missing.content());
    let rows = sim.app.db.audit_page(sim.guild, None, 10).await.unwrap();
    let actions: Vec<(&str, &str)> = rows
        .iter()
        .rev()
        .map(|r| (r.action.as_str(), r.source.as_str()))
        .collect();
    assert_eq!(
        actions,
        [
            ("command.create", "discord"),
            ("command.update", "discord"),
            ("command.delete", "discord")
        ]
    );
    assert_eq!(rows[2].target.as_deref(), Some("rules"));
    assert!(
        rows[1].after.as_deref().unwrap().contains("cooldown_secs"),
        "{:?}",
        rows[1].after
    );
}

#[tokio::test]
async fn custom_commands_keep_to_their_roles_channels_cooldowns_and_module() {
    let Some(mut sim) = Sim::new("custom_commands_keep_to_their_roles_channels_cooldowns_and_module").await else {
        return;
    };
    let (manager, member, other) = (unique(), unique(), unique());
    let (vip, banned, category, lounge, closed, free) = (unique(), unique(), unique(), unique(), unique(), unique());
    sim.channel_is(category, 4, None);
    sim.channel_is(lounge, 0, Some(category));
    sim.channel_is(closed, 0, None);
    sim.channel_is(free, 0, None);

    let mut secret = command("secret", "Shh.");
    secret.allow_roles = vec![vip];
    secret.deny_roles = vec![banned];
    secret.deny_channels = vec![category];
    secret.cooldown_secs = 60;
    let mut off = command("old", "Gone.");
    off.enabled = false;
    stock(&sim, &[secret, off, command("open", "Hello.")]).await;

    sim.channel = free;
    let no_role = sim.command(member, "tag", tag("secret", None)).await;
    assert!(
        no_role.ephemeral && no_role.content().contains("particular role"),
        "{}",
        no_role.content()
    );
    sim.roles = vec![vip, banned];
    let denied = sim.command(member, "tag", tag("secret", None)).await;
    assert!(
        denied.content().contains("don't allow"),
        "a deny beats an allow: {}",
        denied.content()
    );
    sim.roles = vec![vip];
    sim.channel = lounge;
    let wrong_place = sim.command(member, "tag", tag("secret", None)).await;
    assert!(
        wrong_place.content().contains("can't be used in this channel"),
        "its category is denied: {}",
        wrong_place.content()
    );
    sim.channel = free;
    let ok = sim.command(member, "tag", tag("secret", None)).await;
    assert_eq!(ok.content(), "Shh.");
    let cooling = sim.command(member, "tag", tag("secret", None)).await;
    assert!(
        cooling.ephemeral && cooling.content().contains("cooling down"),
        "{}",
        cooling.content()
    );
    let disabled = sim.command(member, "tag", tag("old", None)).await;
    assert!(
        disabled.ephemeral && disabled.content().contains("switched off"),
        "{}",
        disabled.content()
    );
    assert!(
        suggest(&sim, member, "tag", None, "").await.iter().all(|n| n != "old"),
        "off ones are not offered"
    );

    // The module: off, the command, its suggestions and the manager's own commands all stop; the way back
    // is told to a manager.
    modules(&mut sim, manager, toggle("custom_commands", false), &[]).await;
    sim.roles = Vec::new();
    let refused = sim.command(member, "tag", tag("open", None)).await;
    assert!(
        refused.ephemeral && refused.content() == "This feature is turned off on this server.",
        "{}",
        refused.content()
    );
    assert!(suggest(&sim, member, "tag", None, "").await.is_empty());
    sim.permissions = MANAGE_GUILD;
    let manager_sees = sim.command(manager, "customcommand", manage(sub("list", vec![]))).await;
    assert!(
        manager_sees.content().contains("**Custom commands**"),
        "{}",
        manager_sees.content()
    );
    let page = sim.component(manager, "tg:l:0", &[], &manager_sees).await;
    assert!(page.content().contains("turned off"), "{}", page.content());
    let form = sim.component(manager, "cm:c:x:0", &[], &manager_sees).await;
    assert!(
        form.content().contains("turned off"),
        "a form passes the same switch: {}",
        form.content()
    );
    modules(&mut sim, manager, toggle("custom_commands", true), &[]).await;
    sim.permissions = 0;
    assert_eq!(sim.command(member, "tag", tag("open", None)).await.content(), "Hello.");

    // Kept to some channels, like any module.
    modules(
        &mut sim,
        manager,
        limit("custom_commands", "only", free, false),
        &[free],
    )
    .await;
    sim.channel = closed;
    let shut = sim.command(other, "tag", tag("open", None)).await;
    assert_eq!(shut.content(), "This feature is turned off in this channel.");
    sim.channel = free;
    assert_eq!(sim.command(other, "tag", tag("open", None)).await.content(), "Hello.");
}

#[tokio::test]
async fn custom_command_actions_give_roles_and_points_and_say_when_they_cannot() {
    let Some(mut sim) = Sim::new("custom_command_actions_give_roles_and_points_and_say_when_they_cannot").await else {
        return;
    };
    let (admin, member) = (unique(), unique());
    let fan = unique();
    sim.guild_role(fan, "Reef Fan", 2);
    sim.permissions = MANAGE_GUILD;
    let points_on = sim
        .command(
            admin,
            "config",
            json!([sub("points", vec![bool_option("enabled", true)])]),
        )
        .await;
    assert!(points_on.ephemeral, "{}", points_on.content());
    sim.permissions = 0;

    let mut join = command("join", "You now have {points} {points.name} and the role.");
    join.actions = serde_json::from_value(json!([
        { "kind": "toggle_role", "role_id": fan.to_string() },
        { "kind": "give_points", "amount": 25 },
        { "kind": "dm", "text": "Welcome to the fan club, {user.name}!" },
    ]))
    .unwrap();
    let mut tax = command("tax", "Paid.");
    tax.actions = serde_json::from_value(json!([{ "kind": "give_points", "amount": -10 }])).unwrap();
    stock(&sim, &[join, tax]).await;

    let ran = sim.command(member, "tag", tag("join", None)).await;
    let points_name = sim.app.db.guild_settings(sim.guild).await.unwrap().points_name;
    assert_eq!(ran.content(), format!("You now have 25 {points_name} and the role."));
    assert_eq!(sim.points(member).await, 25);
    assert_eq!(
        sim.role_changes(),
        [format!("PUT {member} {fan}")],
        "not holding it, so it is added"
    );
    let dm = sim.posts();
    assert!(
        dm.iter()
            .any(|p| p.channel == member && p.content() == "Welcome to the fan club, Sim!"),
        "the direct message action"
    );

    // Holding it, the same toggle takes it away; taking points never goes below zero.
    sim.roles = vec![fan];
    let again = sim.command(member, "tag", tag("join", None)).await;
    assert!(
        again.ephemeral && again.content().contains("cooling down"),
        "a command with actions waits at least 30 s even with no cooldown set: {}",
        again.content()
    );
    cool_off(&sim, member).await;
    sim.command(member, "tag", tag("join", None)).await;
    assert_eq!(sim.role_changes(), [format!("DELETE {member} {fan}")]);
    assert_eq!(sim.points(member).await, 50);
    sim.command(member, "tag", tag("tax", None)).await;
    assert_eq!(sim.points(member).await, 40);
    let poor = unique();
    sim.command(poor, "tag", tag("tax", None)).await;
    assert_eq!(sim.points(poor).await, 0);

    // The answer goes out first; what failed is told privately afterwards.
    sim.roles = Vec::new();
    sim.allow_role_changes(false);
    cool_off(&sim, member).await;
    let failed = sim.command(member, "tag", tag("join", None)).await;
    assert!(
        !failed.ephemeral && failed.content().contains("You now have"),
        "{}",
        failed.content()
    );
    let note = failed.followups.first().expect("a private note about the role");
    assert!(
        note.ephemeral && note.content().contains("I couldn't give you") && note.content().contains(&fan.to_string()),
        "{}",
        note.content()
    );
    sim.allow_role_changes(true);

    // Points switched off: the action does nothing, and the answer says so by its placeholders staying empty.
    sim.permissions = MANAGE_GUILD;
    sim.command(
        admin,
        "config",
        json!([sub("points", vec![bool_option("enabled", false)])]),
    )
    .await;
    sim.permissions = 0;
    let before = sim.points(member).await;
    cool_off(&sim, member).await;
    sim.command(member, "tag", tag("tax", None)).await;
    assert_eq!(sim.points(member).await, before);
}

#[tokio::test]
async fn auto_responders_answer_joins_leaves_levels_and_role_changes() {
    let Some(mut sim) = Sim::new("auto_responders_answer_joins_leaves_levels_and_role_changes").await else {
        return;
    };
    let (manager, newcomer, repeat) = (unique(), unique(), unique());
    let (lobby, chatty, vip) = (unique(), unique(), unique());
    sim.guild_role(vip, "VIP", 2);

    let mut join = responder("member_join", "Hello {user.name}, member {count} of {server}!", lobby);
    join.cooldown_secs = 3600;
    let mut dm = responder("member_join", "Psst, {user.name}: read the rules.", lobby);
    dm.destination = serde_json::from_value(json!("dm")).unwrap();
    dm.channel_id = None;
    let leave = responder("member_leave", "{user.name} left us.", lobby);
    let mut level = responder("level_up", "Level {level}! [{role}]", lobby);
    level.destination = serde_json::from_value(json!("current")).unwrap();
    level.channel_id = None;
    let mut high = responder("level_up", "Level five!", lobby);
    high.trigger_value = Some("5".into());
    let mut got = responder("role_added", "{user.name} is VIP now: {role}", lobby);
    got.trigger_value = Some(vip.to_string());
    let mut lost = responder("role_removed", "{user.name} lost {role}", lobby);
    lost.trigger_value = Some(vip.to_string());
    stock_responders(&sim, &[join, dm, leave, level, high, got, lost]).await;

    // Joins: a channel post and a direct message; a bot never sets one off; the cooldown is per member.
    sim.join(newcomer, "Robin", false, None).await;
    let posts = posts_after(&sim, |p| {
        p.iter().any(|x| x.channel == lobby) && p.iter().any(|x| x.channel == newcomer)
    })
    .await;
    let welcome = in_channel(&posts, lobby);
    assert_eq!(
        welcome.len(),
        1,
        "{:?}",
        welcome.iter().map(|p| p.content()).collect::<Vec<_>>()
    );
    assert_eq!(welcome[0].content(), "Hello Robin, member 42 of Pinchora Reef!");
    assert_eq!(
        welcome[0].body["allowed_mentions"]["users"],
        json!([newcomer.to_string()])
    );
    assert!(
        posts
            .iter()
            .any(|p| p.channel == newcomer && p.content() == "Psst, Robin: read the rules.")
    );
    sim.join(newcomer, "Robin", false, None).await;
    let posts = sim.posts();
    assert!(
        in_channel(&posts, lobby).is_empty(),
        "the join message has a cooldown; the direct message has none"
    );
    sim.join(unique(), "Botty", true, None).await;
    assert!(sim.posts().is_empty(), "bots set nothing off");
    sim.leave(repeat, "Sam", false).await;
    let posts = sim.posts();
    assert_eq!(
        in_channel(&posts, lobby)
            .iter()
            .map(|p| p.content())
            .collect::<Vec<_>>(),
        ["Sam left us."]
    );

    // A level-up: the responders that fit answer where the member chatted, next to the level-up message.
    sim.app.db.add_message_xp(sim.guild, newcomer, 99, 0).await.unwrap();
    sim.say_in(newcomer, chatty, &[]).await;
    let posts = posts_after(&sim, |p| {
        in_channel(p, chatty)
            .iter()
            .any(|x| x.content().starts_with("Level 1!"))
    })
    .await;
    let texts: Vec<&str> = in_channel(&posts, chatty).iter().map(|p| p.content()).collect();
    assert!(texts.iter().any(|t| t.starts_with("Level 1! []")), "{texts:?}");
    assert!(
        texts.iter().any(|t| t.contains("reached **level 1**")),
        "the level-up message still goes out: {texts:?}"
    );
    assert!(
        !texts.iter().any(|t| t.contains("Level five")),
        "level 5 and up only: {texts:?}"
    );

    // Roles: the first update teaches the bot what the member holds; after that changes are noticed.
    hold_roles(&sim, repeat, &[]).await;
    assert!(sim.posts().is_empty(), "the first sighting only seeds");
    hold_roles(&sim, repeat, &[vip]).await;
    let posts = posts_after(&sim, |p| !in_channel(p, lobby).is_empty()).await;
    assert_eq!(
        in_channel(&posts, lobby)
            .iter()
            .map(|p| p.content())
            .collect::<Vec<_>>(),
        [format!("Robin is VIP now: <@&{vip}>")]
    );
    hold_roles(&sim, repeat, &[vip]).await;
    assert!(sim.posts().is_empty(), "no change, no answer");
    hold_roles(&sim, repeat, &[]).await;
    let posts = posts_after(&sim, |p| !in_channel(p, lobby).is_empty()).await;
    assert_eq!(
        in_channel(&posts, lobby)
            .iter()
            .map(|p| p.content())
            .collect::<Vec<_>>(),
        [format!("Robin lost <@&{vip}>")]
    );
    let none = &posts[0].body["allowed_mentions"];
    assert_eq!(
        none["roles"],
        Value::Null,
        "a role in the text is shown, never pinged: {none}"
    );

    // Switched off, they stay quiet; channels can be kept out too.
    modules(&mut sim, manager, toggle("custom_commands", false), &[]).await;
    sim.join(unique(), "Quiet", false, None).await;
    sim.leave(unique(), "Gone", false).await;
    assert!(sim.posts().is_empty(), "the module is off");
    modules(&mut sim, manager, toggle("custom_commands", true), &[]).await;
    modules(
        &mut sim,
        manager,
        limit("custom_commands", "except", lobby, false),
        &[lobby],
    )
    .await;
    sim.join(unique(), "Blocked", false, None).await;
    let posts = sim.posts();
    assert!(
        in_channel(&posts, lobby).is_empty(),
        "the module is off in the channel it would post in"
    );
}

#[tokio::test]
async fn keyword_responders_hear_chat_once_and_only_where_allowed() {
    let Some(mut sim) = Sim::new("keyword_responders_hear_chat_once_and_only_where_allowed").await else {
        return;
    };
    let (manager, user, user2) = (unique(), unique(), unique());
    let (chat, no_bots, elsewhere) = (unique(), unique(), unique());

    let keyword = |name: &str, word: &str, kind: &str, text: &str| -> ResponderDef {
        serde_json::from_value(json!({
            "name": name,
            "trigger_kind": "keyword",
            "trigger_value": word,
            "match_kind": kind,
            "responses": [text],
            "destination": "current",
            "cooldown_secs": 3600,
            "deny_channels": [no_bots.to_string()],
        }))
        .unwrap()
    };
    stock_responders(
        &sim,
        &[
            keyword("morning", "Good Morning", "contains", "Morning, {user.name}! ☀️"),
            keyword("also morning", "morning", "contains", "Second responder"),
            keyword("exact", "gm", "exact", "gm to you"),
        ],
    )
    .await;

    // Words are matched without case, anywhere; the answer is a reply to the message; one answer per message.
    say_saying(&sim, user, chat, "oh GOOD MORNING everyone").await;
    let posts = posts_after(&sim, |p| !p.is_empty()).await;
    let answers = in_channel(&posts, chat);
    assert_eq!(
        answers.len(),
        1,
        "the second responder also matched, but one answer per message"
    );
    assert_eq!(answers[0].content(), "Morning, Sim! ☀️");
    assert!(
        answers[0].body["message_reference"]["message_id"].is_string(),
        "{}",
        answers[0].body
    );
    assert_eq!(answers[0].body["allowed_mentions"]["users"], json!([user.to_string()]));

    // The cooldown holds for that member and responder; the next responder gets the message instead.
    say_saying(&sim, user, chat, "good morning again").await;
    let posts = sim.posts();
    assert_eq!(
        in_channel(&posts, chat).iter().map(|p| p.content()).collect::<Vec<_>>(),
        ["Second responder"],
        "the first is cooling down for this member, so the next one that matches answers"
    );
    say_saying(&sim, user2, chat, "good morning").await;
    assert_eq!(
        in_channel(&sim.posts(), chat).len(),
        1,
        "another member has no cooldown"
    );

    // Exact means the whole message.
    say_saying(&sim, user, chat, "gm all").await;
    assert!(sim.posts().is_empty());
    say_saying(&sim, user, chat, "  GM ").await;
    assert_eq!(in_channel(&sim.posts(), chat)[0].content(), "gm to you");

    // Without the Message Content intent a message has no text and nothing is heard.
    say_saying(&sim, user2, chat, "").await;
    assert!(sim.posts().is_empty(), "no text, no keywords");

    // Denied channels stay silent, and so does a channel the module is off in.
    say_saying(&sim, unique(), no_bots, "good morning").await;
    assert!(sim.posts().is_empty(), "the responder denies that channel");
    modules(
        &mut sim,
        manager,
        limit("custom_commands", "except", elsewhere, false),
        &[elsewhere],
    )
    .await;
    say_saying(&sim, unique(), elsewhere, "good morning").await;
    assert!(sim.posts().is_empty(), "the module is off there");
    say_saying(&sim, unique(), chat, "good morning").await;
    assert_eq!(in_channel(&sim.posts(), chat).len(), 1);
}

/// Direct messages to `who` posted since the last call, as their text (the mock opens a member's direct
/// messages as a channel with their own id).
fn dms(sim: &Sim, who: u64) -> Vec<String> {
    sim.posts()
        .iter()
        .filter(|p| p.channel == who)
        .map(|p| p.content().to_owned())
        .collect()
}

#[tokio::test]
async fn custom_actions_are_capped_checked_again_and_kept_out_of_closed_dms() {
    let Some(mut sim) = Sim::new("custom_actions_are_capped_checked_again_and_kept_out_of_closed_dms").await else {
        return;
    };
    let (admin, member, quiet) = (unique(), unique(), unique());
    let fan = unique();
    sim.guild_role(fan, "Reef Fan", 2);
    sim.permissions = MANAGE_GUILD;
    sim.command(
        admin,
        "config",
        json!([sub("points", vec![bool_option("enabled", true)])]),
    )
    .await;
    sim.permissions = 0;
    let with = |name: &str, reply: &str, actions: Value| {
        let mut def = command(name, reply);
        def.actions = serde_json::from_value(actions).unwrap();
        def.cooldown_secs = 30;
        def
    };
    stock(
        &sim,
        &[
            with(
                "loot",
                "Looted.",
                json!([
                    { "kind": "give_points", "amount": 400 },
                    { "kind": "give_points", "amount": 400 },
                    { "kind": "give_points", "amount": 400 },
                ]),
            ),
            with(
                "fan",
                "Welcome, fan.",
                json!([{ "kind": "add_role", "role_id": fan.to_string() }]),
            ),
            with(
                "hello",
                "Check your DMs.",
                json!([{ "kind": "dm", "text": "Hi {user.name}!" }]),
            ),
            command("echo", "You said: {args}"),
        ],
    )
    .await;

    // Points from custom actions stop at 1,000 a member a day, and the member is told why.
    let looted = sim.command(member, "tag", tag("loot", None)).await;
    assert_eq!(sim.points(member).await, 1_000);
    let note = looted.followups.first().expect("a note about the cap");
    assert!(
        note.ephemeral && note.content().contains("today's limit"),
        "{}",
        note.content()
    );
    cool_off(&sim, member).await;
    sim.command(member, "tag", tag("loot", None)).await;
    assert_eq!(sim.points(member).await, 1_000, "nothing more today");

    // A role is checked again when it is handed out: one that gained Administrator since is refused.
    sim.command(member, "tag", tag("fan", None)).await;
    assert_eq!(sim.role_changes(), [format!("PUT {member} {fan}")]);
    for role in sim.mock.roles.lock().unwrap().iter_mut() {
        if role["id"].as_str().and_then(|id| id.parse::<u64>().ok()) == Some(fan) {
            role["permissions"] = json!((1u64 << 3).to_string());
        }
    }
    sim.roles = Vec::new();
    let other = unique();
    let refused = sim.command(other, "tag", tag("fan", None)).await;
    assert!(sim.role_changes().is_empty(), "no role was given");
    let note = refused.followups.first().expect("a note about the role");
    assert!(
        note.content()
            .contains("custom commands and auto-responders can't hand it out"),
        "{}",
        note.content()
    );

    // At most five direct messages an hour from one server's actions, and none to who closed them.
    sim.posts();
    for _ in 0..7 {
        cool_off(&sim, member).await;
        sim.command(member, "tag", tag("hello", None)).await;
    }
    assert_eq!(dms(&sim, member), vec!["Hi Sim!"; 5], "two were held back");
    let closed = sim
        .command(quiet, "privacy", json!([bool_option("server_dms", false)]))
        .await;
    assert!(
        closed.ephemeral && closed.description().contains("Saved.") && closed.description().contains("🔒 Off."),
        "{}",
        closed.description()
    );
    let shown = sim.command(quiet, "privacy", json!([])).await;
    assert!(
        shown.description().contains("🔒 Off.") && shown.has_button("Server DMs: off"),
        "{}",
        shown.description()
    );
    sim.command(quiet, "tag", tag("hello", None)).await;
    assert!(dms(&sim, quiet).is_empty(), "the member said no");
    // The panel's own button turns them back on, like the option does.
    let open = sim.click(quiet, &shown, "Server DMs: off").await;
    assert!(
        open.description().contains("🌊 On.") && open.has_button("Server DMs: on"),
        "{}",
        open.description()
    );
    assert!(!sim.app.db.dm_opt_out(quiet).await.unwrap());
    cool_off(&sim, quiet).await;
    sim.command(quiet, "tag", tag("hello", None)).await;
    assert_eq!(dms(&sim, quiet), ["Hi Sim!"]);
    let again = sim
        .command(quiet, "privacy", json!([bool_option("server_dms", true)]))
        .await;
    assert!(again.description().contains("🌊 On."), "{}", again.description());

    // What a member types shows as typed: no masked link, no bold of their own.
    let echoed = sim
        .command(
            member,
            "tag",
            tag("echo", Some("[Free Nitro](https://evil.example) **now** <@1>")),
        )
        .await;
    assert_eq!(
        echoed.content(),
        r"You said: \[Free Nitro\]\(https://evil.example\) \*\*now\*\*"
    );
}

#[tokio::test]
async fn a_responders_own_role_change_sets_off_no_other_responder() {
    let Some(sim) = Sim::new("a_responders_own_role_change_sets_off_no_other_responder").await else {
        return;
    };
    let (member, lobby) = (unique(), unique());
    let (crab, shell) = (unique(), unique());
    sim.guild_role(crab, "Crab", 2);
    sim.guild_role(shell, "Shell", 3);
    // Crab gives Shell, and Shell would give Crab back: a loop without the chain cap.
    let mut gives_shell = responder("role_added", "Crab {role}", lobby);
    gives_shell.trigger_value = Some(crab.to_string());
    gives_shell.cooldown_secs = 30;
    gives_shell.actions =
        serde_json::from_value(json!([{ "kind": "add_role", "role_id": shell.to_string() }])).unwrap();
    let mut gives_crab = responder("role_added", "Shell {role}", lobby);
    gives_crab.trigger_value = Some(shell.to_string());
    gives_crab.cooldown_secs = 30;
    gives_crab.actions =
        serde_json::from_value(json!([{ "kind": "toggle_role", "role_id": crab.to_string() }])).unwrap();
    stock_responders(&sim, &[gives_shell, gives_crab]).await;

    hold_roles(&sim, member, &[]).await;
    hold_roles(&sim, member, &[crab]).await;
    let posted: Vec<String> = posts_after(&sim, |p| !p.is_empty())
        .await
        .iter()
        .map(|p| p.content().to_owned())
        .collect();
    assert_eq!(posted.len(), 1, "{posted:?}");
    assert!(posted[0].starts_with("Crab"), "{posted:?}");
    assert_eq!(sim.role_changes(), [format!("PUT {member} {shell}")]);

    // Discord tells of the role the responder gave: nothing else answers.
    hold_roles(&sim, member, &[crab, shell]).await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert!(sim.posts().is_empty(), "the second responder stayed quiet");
    assert!(sim.role_changes().is_empty(), "and changed no role");

    // The same role given by someone else still counts.
    hold_roles(&sim, member, &[crab]).await;
    hold_roles(&sim, member, &[crab, shell]).await;
    let posted = posts_after(&sim, |p| !p.is_empty()).await;
    assert!(
        posted.iter().any(|p| p.content().starts_with("Shell")),
        "{:?}",
        posted.iter().map(|p| p.content()).collect::<Vec<_>>()
    );
}
