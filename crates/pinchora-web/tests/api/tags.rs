//! Custom commands and auto-responders over the API: written with the bot's rules, previewed with its
//! pipeline, exported and imported, audited and announced.

use pinchora_web::WebState;

use super::*;

fn g(guild: u64) -> String {
    format!("/api/guilds/{guild}")
}

fn command(name: &str, reply: &str) -> Value {
    json!({ "name": name, "responses": [reply] })
}

/// The first error of a refused request: its field and message.
fn refusal(response: &Response) -> (String, String) {
    let error = &response.json()["error"];
    (
        error["field"].as_str().unwrap_or_default().to_owned(),
        error["message"].as_str().unwrap_or_default().to_owned(),
    )
}

/// The same browser, on a dashboard whose bot runs with the Message Content intent (or without it).
fn listening(h: &Harness, browser: &mut Browser, on: bool) {
    let mut settings = (*h.state.settings).clone();
    settings.message_content = on;
    browser.router = pinchora_web::router(WebState::new(h.state.app.clone(), settings));
}

#[tokio::test]
async fn custom_commands_save_with_the_bot_rules_and_reach_the_bot() {
    let Some(h) = Harness::new("tags_commands").await else {
        return;
    };
    let (guild, owner, mut browser) = h.managed_server().await;
    let mut heard = Box::pin(h.state.app.cache.invalidations().await.expect("subscribe"));
    let path = format!("{}/commands", g(guild));

    let empty = browser.get(&path).await;
    assert_eq!(empty.json(), json!({ "items": [], "max": 50 }));

    // A command is made tidy, saved, announced to the bot and written in the audit log.
    let made = browser
        .write(
            Method::POST,
            &path,
            json!({
                "name": " Rules ",
                "description": "The   rules",
                "responses": ["Be kind, {user}.", "Be kind."],
                "cooldown_secs": 30,
                "allow_roles": [REGULAR.to_string()],
                "deny_channels": [LOUNGE.to_string()],
                "actions": [{ "kind": "add_role", "role_id": FAN.to_string() }, { "kind": "give_points", "amount": 5 }],
            }),
        )
        .await;
    assert_eq!(made.status, StatusCode::OK, "{}", made.text());
    let made = made.json();
    assert_eq!(
        (made["name"].as_str(), made["description"].as_str()),
        (Some("rules"), Some("The rules"))
    );
    assert_eq!((made["uses"].clone(), made["enabled"].clone()), (json!(0), json!(true)));
    assert_eq!(made["allow_roles"], json!([REGULAR.to_string()]), "ids are strings");
    let id = made["id"].as_str().expect("an id as a string").to_owned();
    assert!(
        announced(&mut heard, Invalidation::Guild(guild)).await,
        "the bot hears of it"
    );
    let stored = h.state.app.db.custom_commands(guild).await.unwrap();
    assert_eq!(stored.len(), 1);
    assert_eq!(stored[0].created_by, owner as i64);
    let one = browser.get(&format!("{path}/{id}")).await.json();
    assert_eq!(
        one["actions"][0],
        json!({ "kind": "add_role", "role_id": FAN.to_string() })
    );

    // The bot's sentences, with the field to point at.
    let refused = |body: Value| {
        let mut b = browser.clone();
        let path = path.clone();
        async move { b.write(Method::POST, &path, body).await }
    };
    for (body, field, text) in [
        (command("Has Space", "x"), "name", "lowercase letters"),
        (command("help", "x"), "name", "own commands"),
        (command("rules", "x"), "name", "already exists"),
        (
            json!({ "name": "a", "responses": [] }),
            "responses",
            "at least one reply",
        ),
        (
            json!({ "name": "a", "responses": vec!["x"; 6] }),
            "responses",
            "at most 5",
        ),
        (
            json!({ "name": "a", "responses": ["é".repeat(2001)] }),
            "responses",
            "2001",
        ),
        (
            json!({ "name": "a", "responses": ["x"], "cooldown_secs": 3601 }),
            "cooldown_secs",
            "0 to 3600",
        ),
        (
            json!({ "name": "a", "responses": ["x"], "embed": {} }),
            "embed",
            "title",
        ),
        (
            json!({ "name": "a", "responses": ["x"], "actions": [{ "kind": "give_points", "amount": 5000 }] }),
            "actions",
            "points go from",
        ),
        (
            json!({ "name": "a", "responses": ["x"], "actions": [{ "kind": "react", "emoji": "🎉" }] }),
            "actions",
            "only a keyword",
        ),
        (
            json!({ "name": "a", "responses": ["x"], "allow_roles": ["424242"] }),
            "allow_roles",
            "no longer exists",
        ),
        (
            json!({ "name": "a", "responses": ["x"], "deny_channels": ["424242"] }),
            "deny_channels",
            "can't see it",
        ),
        (
            json!({ "name": "a", "responses": ["x"], "actions": [{ "kind": "add_role", "role_id": ADMIN.to_string() }] }),
            "actions",
            "above my highest role",
        ),
        (
            json!({ "name": "a", "responses": ["x"], "actions": [{ "kind": "toggle_role", "role_id": BOOSTER.to_string() }] }),
            "actions",
            "managed by an integration",
        ),
    ] {
        let response = refused(body.clone()).await;
        assert_eq!(
            response.status,
            StatusCode::UNPROCESSABLE_ENTITY,
            "{body}: {}",
            response.text()
        );
        let (got_field, message) = refusal(&response);
        assert_eq!(got_field, field, "{body}");
        assert!(message.contains(text), "{body}: {message}");
    }
    let unknown = refused(json!({ "name": "a", "responses": ["x"], "color": "red" })).await;
    assert_eq!(unknown.status, StatusCode::BAD_REQUEST, "an unknown field is refused");
    assert_eq!(
        h.state.app.db.custom_commands(guild).await.unwrap().len(),
        1,
        "nothing refused was saved"
    );

    // Changing it records only what changed; saving the same thing again records nothing.
    let mut changed = without_ids(one.clone());
    changed["responses"] = json!(["Be kind to everyone."]);
    changed["cooldown_secs"] = json!(10);
    let saved = browser
        .write(Method::PUT, &format!("{path}/{id}"), changed.clone())
        .await;
    assert_eq!(saved.status, StatusCode::OK, "{}", saved.text());
    let same = browser
        .write(Method::PUT, &format!("{path}/{id}"), changed.clone())
        .await;
    assert_eq!(same.status, StatusCode::OK);
    let rows = h.state.app.db.audit_page(guild, None, 10).await.unwrap();
    assert_eq!(
        rows.iter().map(|r| r.action.as_str()).collect::<Vec<_>>(),
        ["command.update", "command.create"]
    );
    let diff: Value = serde_json::from_str(rows[0].after.as_deref().unwrap()).unwrap();
    assert_eq!(
        diff,
        json!({ "responses": ["Be kind to everyone."], "cooldown_secs": 10 })
    );
    assert_eq!(rows[0].target.as_deref(), Some("rules"));
    assert_eq!((rows[0].source.as_str(), rows[0].actor_id as u64), ("web", owner));

    // A role that was deleted since does not stop other edits (only what is new is checked).
    sqlx_exec(
        &h,
        guild,
        "UPDATE custom_commands SET allow_roles = '{424243}' WHERE guild_id = $1",
    )
    .await;
    let mut again = without_ids(browser.get(&format!("{path}/{id}")).await.json());
    assert_eq!(again["allow_roles"], json!(["424243"]));
    again["description"] = json!("Rules, short");
    let edited = browser.write(Method::PUT, &format!("{path}/{id}"), again).await;
    assert_eq!(edited.status, StatusCode::OK, "{}", edited.text());

    // On and off with a PATCH; a rename onto another command's name is refused.
    let off = browser
        .write(Method::PATCH, &format!("{path}/{id}"), json!({ "enabled": false }))
        .await;
    assert_eq!(off.json()["enabled"], false);
    let other = browser.write(Method::POST, &path, command("hello", "Hi!")).await.json();
    let mut clash = other.clone();
    clash["name"] = json!("rules");
    let clash = browser
        .write(
            Method::PUT,
            &format!("{path}/{}", other["id"].as_str().unwrap()),
            without_ids(clash),
        )
        .await;
    assert_eq!(refusal(&clash).0, "name");
    assert_eq!(
        browser
            .write(Method::PATCH, &format!("{path}/999999999"), json!({ "enabled": true }))
            .await
            .status,
        StatusCode::NOT_FOUND
    );

    // Delete.
    let gone = browser.write(Method::DELETE, &format!("{path}/{id}"), json!({})).await;
    assert_eq!(gone.json(), json!({ "deleted": id }));
    assert_eq!(
        browser
            .write(Method::DELETE, &format!("{path}/{id}"), json!({}))
            .await
            .status,
        StatusCode::NOT_FOUND
    );
    let actions: Vec<String> = h
        .state
        .app
        .db
        .audit_page(guild, None, 20)
        .await
        .unwrap()
        .into_iter()
        .map(|r| r.action)
        .collect();
    assert_eq!(
        actions,
        [
            "command.delete",
            "command.create",
            "command.update",
            "command.update",
            "command.update",
            "command.create"
        ]
    );

    // Someone who is not a manager of the server gets nothing.
    let stranger = unique();
    let mut outsider = h.browser();
    outsider
        .login(&h, stranger, vec![guild_entry(unique(), "Elsewhere", true, 0)])
        .await;
    assert_eq!(outsider.get(&path).await.status, StatusCode::FORBIDDEN);
    assert_eq!(
        outsider.write(Method::POST, &path, command("x", "y")).await.status,
        StatusCode::FORBIDDEN
    );
}

/// A command's JSON without what the database adds.
fn without_ids(mut value: Value) -> Value {
    value
        .as_object_mut()
        .unwrap()
        .retain(|k, _| !["id", "uses", "created_by", "created_at", "updated_at"].contains(&k.as_str()));
    value
}

async fn sqlx_exec(h: &Harness, guild: u64, sql: &'static str) {
    let pool = h.state.app.db.pool();
    sqlx::query(sql).bind(guild as i64).execute(pool).await.expect("update");
}

#[tokio::test]
async fn previews_use_the_bots_pipeline_and_cannot_ping() {
    let Some(h) = Harness::new("tags_preview").await else {
        return;
    };
    let (guild, owner, mut browser) = h.managed_server().await;
    let path = format!("{}/commands", g(guild));

    // A draft: the member and the server are filled in, the arguments cannot carry a mention.
    let draft = json!({
        "command": {
            "name": "hi",
            "responses": ["Hello {user} from {server}! {1:friend} said {args} <@7> <@&9001> @everyone"],
            "embed": { "title": "Hi {user.name}", "description": "In {channel}", "color": "#ff6b4a",
                       "fields": [{ "name": "n", "value": "{random:a|a}", "inline": true }] },
        },
        "args": "<@5> there @here"
    });
    let preview = browser.write(Method::POST, &format!("{path}/preview"), draft).await;
    assert_eq!(preview.status, StatusCode::OK, "{}", preview.text());
    let preview = preview.json();
    let text = preview["text"].as_str().unwrap();
    assert!(text.starts_with("Hello @user"), "{text}");
    assert!(text.contains("from Pinchora Reef! there said there here"), "{text}");
    assert!(
        !text.contains("<@") && !text.contains("@everyone") && !text.contains("<@&"),
        "{text}"
    );
    assert!(preview["embed"]["title"].as_str().unwrap().starts_with("Hi user"));
    assert_eq!(preview["embed"]["description"], "In #general");
    assert_eq!(
        preview["embed"]["fields"][0],
        json!({ "name": "n", "value": "a", "inline": true })
    );
    assert_eq!(preview["ephemeral"], false);
    assert!(
        preview["notes"]
            .as_array()
            .unwrap()
            .iter()
            .any(|n| n.as_str().unwrap().contains("Mentions in the text are left out")),
        "{preview}"
    );
    let _ = owner;

    // With mentions allowed, the ones the author wrote stay (role names written out), never everyone.
    let allowed = browser
        .write(
            Method::POST,
            &format!("{path}/preview"),
            json!({ "command": { "name": "hi", "responses": ["<@&9001> <@7> @everyone"], "allow_mentions": true } }),
        )
        .await
        .json();
    assert_eq!(allowed["text"], "@Fan @someone everyone");

    // A draft the bot would refuse is refused with its sentence; a saved command is tested by id.
    let bad = browser
        .write(
            Method::POST,
            &format!("{path}/preview"),
            json!({ "command": { "name": "x", "responses": [] } }),
        )
        .await;
    assert_eq!(bad.status, StatusCode::UNPROCESSABLE_ENTITY);
    let saved = browser
        .write(
            Method::POST,
            &path,
            json!({ "name": "greet", "responses": ["Hi {1:there}"], "reply_mode": "ephemeral", "actions": [{ "kind": "give_points", "amount": 5 }] }),
        )
        .await
        .json();
    let id = saved["id"].as_str().unwrap();
    let tested = browser
        .write(Method::POST, &format!("{path}/{id}/test"), json!({ "args": "Sam" }))
        .await
        .json();
    assert_eq!(tested["text"], "Hi Sam");
    assert_eq!(tested["ephemeral"], true);
    assert!(
        !tested["notes"]
            .as_array()
            .unwrap()
            .iter()
            .any(|n| n.as_str().unwrap().contains("points are off")),
        "points are on by default: {tested}"
    );
    assert_eq!(
        h.state.app.db.custom_commands(guild).await.unwrap()[0].uses,
        0,
        "a test counts no use"
    );
    assert_eq!(
        h.state.app.db.audit_page(guild, None, 10).await.unwrap().len(),
        1,
        "a test writes no audit row"
    );
    browser
        .write(
            Method::PATCH,
            &format!("{}/settings", g(guild)),
            json!({ "points_enabled": false }),
        )
        .await;
    let tested = browser
        .write(Method::POST, &format!("{path}/{id}/test"), json!({ "args": "Sam" }))
        .await
        .json();
    assert!(
        tested["notes"]
            .as_array()
            .unwrap()
            .iter()
            .any(|n| n.as_str().unwrap().contains("points are off")),
        "{tested}"
    );
}

#[tokio::test]
async fn commands_export_and_import_between_servers() {
    let Some(h) = Harness::new("tags_import").await else {
        return;
    };
    let (guild, _, mut browser) = h.managed_server().await;
    let path = format!("{}/commands", g(guild));
    browser
        .write(
            Method::POST,
            &path,
            json!({ "name": "rules", "responses": ["Be kind."], "allow_roles": [REGULAR.to_string()], "cooldown_secs": 5 }),
        )
        .await;
    browser.write(Method::POST, &path, command("hello", "Hi!")).await;
    let export = browser.get(&format!("{path}/export")).await.json();
    assert_eq!(export["version"], 1);
    let items = export["items"].as_array().unwrap().clone();
    assert_eq!(items.len(), 2);
    assert!(
        items[0].get("id").is_none() && items[0].get("uses").is_none(),
        "an export holds what was written"
    );
    let rules_at = items
        .iter()
        .position(|c| c["name"] == "rules")
        .expect("rules is exported");

    // Another server: a role it lacks is left out and said so; names already there are skipped.
    let (other, _, mut elsewhere) = h.managed_server().await;
    let there = format!("{}/commands", g(other));
    let mut items = items;
    items[rules_at]["allow_roles"] = json!([REGULAR.to_string(), "424242"]);
    items.push(json!({ "name": "Not A Name", "responses": ["x"] }));
    items.push(json!("not even an object"));
    let imported = elsewhere
        .write(
            Method::POST,
            &format!("{there}/import"),
            json!({ "version": 1, "items": items }),
        )
        .await;
    assert_eq!(imported.status, StatusCode::OK, "{}", imported.text());
    let report = imported.json();
    assert_eq!(report["created"].as_array().unwrap().len(), 2, "{report}");
    assert!(
        report["created"]
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c.as_str().unwrap().contains("not roles on this server")),
        "{report}"
    );
    assert_eq!(report["failed"].as_array().unwrap().len(), 2, "{report}");
    assert_eq!(report["failed"][0]["name"], "Not A Name");
    let list = elsewhere.get(&there).await.json();
    let rules = list["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == "rules")
        .unwrap();
    assert_eq!(rules["allow_roles"], json!([REGULAR.to_string()]));
    let again = elsewhere
        .write(
            Method::POST,
            &format!("{there}/import"),
            json!({ "items": [command("rules", "Changed")] }),
        )
        .await
        .json();
    assert_eq!(
        (again["created"].as_array().unwrap().len(), again["skipped"].clone()),
        (0, json!(["rules"]))
    );
    let replaced = elsewhere
        .write(
            Method::POST,
            &format!("{there}/import"),
            json!({ "items": [command("rules", "Changed")], "on_conflict": "replace" }),
        )
        .await
        .json();
    assert_eq!(replaced["replaced"], json!(["rules"]), "{replaced}");
    let list = elsewhere.get(&there).await.json();
    let rules = list["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == "rules")
        .unwrap();
    assert_eq!(rules["responses"], json!(["Changed"]));
    let rows = h.state.app.db.audit_page(other, None, 10).await.unwrap();
    assert_eq!(
        rows.iter().map(|r| r.action.as_str()).collect::<Vec<_>>(),
        ["commands.import", "commands.import"]
    );

    // The limits: a file too long, a version that is not ours, a nonsense conflict rule.
    let many: Vec<Value> = (0..201).map(|n| command(&format!("c{n}"), "x")).collect();
    let big = elsewhere
        .write(Method::POST, &format!("{there}/import"), json!({ "items": many }))
        .await;
    assert_eq!(
        (big.status, big.code().as_str()),
        (StatusCode::UNPROCESSABLE_ENTITY, "limit")
    );
    let version = elsewhere
        .write(
            Method::POST,
            &format!("{there}/import"),
            json!({ "version": 9, "items": [] }),
        )
        .await;
    assert_eq!(version.status, StatusCode::BAD_REQUEST);
    let rule = elsewhere
        .write(
            Method::POST,
            &format!("{there}/import"),
            json!({ "items": [], "on_conflict": "yolo" }),
        )
        .await;
    assert_eq!(rule.status, StatusCode::BAD_REQUEST);

    // 50 is the most a server holds, in both ways in.
    for n in 0..48 {
        h.state
            .app
            .db
            .add_custom_command(
                other,
                &serde_json::from_value(command(&format!("fill{n}"), "x")).unwrap(),
                1,
            )
            .await
            .unwrap();
    }
    let full = elsewhere.write(Method::POST, &there, command("one-more", "x")).await;
    assert_eq!(
        (full.status, full.code().as_str()),
        (StatusCode::UNPROCESSABLE_ENTITY, "limit")
    );
    let report = elsewhere
        .write(
            Method::POST,
            &format!("{there}/import"),
            json!({ "items": [command("last", "x")] }),
        )
        .await
        .json();
    assert_eq!(
        report["failed"][0]["message"]
            .as_str()
            .map(|m| m.contains("at most 50")),
        Some(true),
        "{report}"
    );
}

#[tokio::test]
async fn keyword_responders_are_locked_without_the_message_content_intent() {
    let Some(h) = Harness::new("tags_responders").await else {
        return;
    };
    let (guild, _, mut browser) = h.managed_server().await;
    let path = format!("{}/responders", g(guild));
    let me = browser.get("/api/me").await.json();
    assert_eq!(me["capabilities"], json!({ "message_content": false }));
    let list = browser.get(&path).await.json();
    assert_eq!(
        (
            list["items"].clone(),
            list["max"].clone(),
            list["message_content"].clone()
        ),
        (json!([]), json!(25), json!(false))
    );

    let keyword = |enabled: bool| {
        json!({
            "name": "morning", "trigger_kind": "keyword", "trigger_value": "good morning", "match_kind": "contains",
            "responses": ["Morning {user}!"], "destination": "current", "enabled": enabled,
        })
    };
    let blocked = browser.write(Method::POST, &path, keyword(true)).await;
    assert_eq!(
        (blocked.status, blocked.code().as_str()),
        (StatusCode::UNPROCESSABLE_ENTITY, "blocked")
    );
    assert!(
        refusal(&blocked).1.contains("Message Content intent"),
        "{}",
        blocked.text()
    );
    let parked = browser.write(Method::POST, &path, keyword(false)).await;
    assert_eq!(parked.status, StatusCode::OK, "{}", parked.text());
    let id = parked.json()["id"].as_str().unwrap().to_owned();
    let on = browser
        .write(Method::PATCH, &format!("{path}/{id}"), json!({ "enabled": true }))
        .await;
    assert_eq!(on.code(), "blocked", "switching it on is refused too");
    let preview = browser
        .write(Method::POST, &format!("{path}/{id}/test"), json!({}))
        .await
        .json();
    assert!(preview["notes"][0].as_str().unwrap().contains("locked"), "{preview}");

    // The bot with the intent: the same request works.
    listening(&h, &mut browser, true);
    let me = browser.get("/api/me").await.json();
    assert_eq!(me["capabilities"], json!({ "message_content": true }));
    let on = browser
        .write(Method::PATCH, &format!("{path}/{id}"), json!({ "enabled": true }))
        .await;
    assert_eq!(on.json()["enabled"], true, "{}", on.text());
    let live = browser.write(Method::POST, &path, keyword(true)).await;
    assert_eq!(live.status, StatusCode::OK, "{}", live.text());
    // An empty keyword, and a match rule that does not exist.
    let mut empty = keyword(true);
    empty["trigger_value"] = json!("  ");
    assert_eq!(
        refusal(&browser.write(Method::POST, &path, empty).await).0,
        "trigger_value"
    );
    let mut odd = keyword(true);
    odd["match_kind"] = json!("regex");
    assert_eq!(
        browser.write(Method::POST, &path, odd).await.status,
        StatusCode::BAD_REQUEST
    );
    listening(&h, &mut browser, false);
    // Editing one that is already on stays possible while the intent is off (it just stays quiet).
    let one = browser.get(&format!("{path}/{id}")).await.json();
    let mut edited = without_ids(one);
    edited["responses"] = json!(["Morning!"]);
    let kept = browser.write(Method::PUT, &format!("{path}/{id}"), edited).await;
    assert_eq!(kept.status, StatusCode::OK, "{}", kept.text());
}

#[tokio::test]
async fn event_responders_check_their_triggers_channels_and_roles() {
    let Some(h) = Harness::new("tags_events").await else {
        return;
    };
    let (guild, _, mut browser) = h.managed_server().await;
    let path = format!("{}/responders", g(guild));
    let mut heard = Box::pin(h.state.app.cache.invalidations().await.expect("subscribe"));

    let join = json!({
        "name": "welcome", "trigger_kind": "member_join", "responses": ["Hi {user}, member {count}!"],
        "destination": "channel", "channel_id": GENERAL.to_string(), "cooldown_secs": 60,
        "actions": [{ "kind": "add_role", "role_id": FAN.to_string() }],
    });
    let made = browser.write(Method::POST, &path, join.clone()).await;
    assert_eq!(made.status, StatusCode::OK, "{}", made.text());
    assert!(announced(&mut heard, Invalidation::Guild(guild)).await);
    let id = made.json()["id"].as_str().unwrap().to_owned();
    assert_eq!(made.json()["chance"], 100, "always unless told otherwise");

    for (body, field, text) in [
        (
            json!({ "name": "w", "trigger_kind": "member_join", "responses": ["x"], "destination": "current" }),
            "destination",
            "pick a channel",
        ),
        (
            json!({ "name": "w", "trigger_kind": "member_leave", "responses": ["x"], "destination": "dm" }),
            "destination",
            "can't be messaged",
        ),
        (
            json!({ "name": "w", "trigger_kind": "member_join", "responses": ["x"], "destination": "channel", "channel_id": LOUNGE.to_string() }),
            "channel_id",
            "isn't a text or announcement",
        ),
        (
            json!({ "name": "w", "trigger_kind": "member_join", "responses": ["x"], "destination": "channel", "channel_id": "424242" }),
            "channel_id",
            "no longer exists",
        ),
        (
            json!({ "name": "w", "trigger_kind": "role_added", "responses": ["x"], "destination": "dm" }),
            "trigger_value",
            "Pick the role",
        ),
        (
            json!({ "name": "w", "trigger_kind": "role_added", "trigger_value": "424242", "responses": ["x"], "destination": "dm" }),
            "trigger_value",
            "no longer exists",
        ),
        (
            json!({ "name": "w", "trigger_kind": "level_up", "trigger_value": "0", "responses": ["x"], "destination": "current" }),
            "trigger_value",
            "1 to 1000",
        ),
        (
            json!({ "name": "w", "trigger_kind": "member_join", "responses": ["x"], "destination": "dm", "chance": 0 }),
            "chance",
            "1 to 100",
        ),
        (
            json!({ "name": "w", "trigger_kind": "member_leave", "responses": ["x"], "destination": "channel", "channel_id": GENERAL.to_string(), "actions": [{ "kind": "dm", "text": "x" }] }),
            "actions",
            "member is gone",
        ),
    ] {
        let response = browser.write(Method::POST, &path, body.clone()).await;
        assert_eq!(
            response.status,
            StatusCode::UNPROCESSABLE_ENTITY,
            "{body}: {}",
            response.text()
        );
        let (got, message) = refusal(&response);
        assert_eq!(got, field, "{body}");
        assert!(message.contains(text), "{body}: {message}");
    }
    let watch = json!({ "name": "vip", "trigger_kind": "role_added", "trigger_value": FAN.to_string(), "responses": ["{user} got {role}"], "destination": "dm" });
    assert_eq!(browser.write(Method::POST, &path, watch).await.status, StatusCode::OK);

    // The test shows the event's own placeholders with made-up numbers.
    let tested = browser
        .write(Method::POST, &format!("{path}/{id}/test"), json!({}))
        .await
        .json();
    assert!(tested["text"].as_str().unwrap().ends_with("member 1234!"), "{tested}");
    let leveled = browser
        .write(
            Method::POST,
            &format!("{path}/preview"),
            json!({ "responder": { "name": "l", "trigger_kind": "level_up", "trigger_value": "10", "responses": ["Level {level}! {role}"], "destination": "current" } }),
        )
        .await
        .json();
    assert_eq!(
        leveled["text"], "Level 10! @Regular",
        "the first role the bot can hand out stands in"
    );

    // 25 at most; an export and an import keep what a server has.
    let export = browser.get(&format!("{path}/export")).await.json();
    assert_eq!(export["items"].as_array().unwrap().len(), 2);
    for n in 0..23 {
        let def: pinchora_core::tags::ResponderDef = serde_json::from_value(json!({
            "name": format!("filler {n}"), "trigger_kind": "member_leave", "responses": ["x"],
            "destination": "channel", "channel_id": GENERAL.to_string(),
        }))
        .unwrap();
        h.state.app.db.add_custom_responder(guild, &def, 1).await.unwrap();
    }
    let full = browser.write(Method::POST, &path, join).await;
    assert_eq!(
        (full.status, full.code().as_str()),
        (StatusCode::UNPROCESSABLE_ENTITY, "limit")
    );
    let list = browser.get(&path).await.json();
    assert_eq!(list["items"].as_array().unwrap().len(), 25);

    // Imports keep going past one that cannot be saved here.
    let (other, _, mut elsewhere) = h.managed_server().await;
    let there = format!("{}/responders", g(other));
    let mut items = export["items"].as_array().unwrap().clone();
    items[0]["channel_id"] = json!("424242");
    let report = elsewhere
        .write(
            Method::POST,
            &format!("{there}/import"),
            json!({ "version": 1, "items": items }),
        )
        .await
        .json();
    assert_eq!(report["created"].as_array().unwrap().len(), 1, "{report}");
    assert!(
        report["failed"][0]["message"]
            .as_str()
            .unwrap()
            .contains("channel it posts in"),
        "{report}"
    );

    // Delete, with the audit trail.
    assert_eq!(
        browser
            .write(Method::DELETE, &format!("{path}/{id}"), json!({}))
            .await
            .json(),
        json!({ "deleted": id })
    );
    let actions: Vec<String> = h
        .state
        .app
        .db
        .audit_page(guild, None, 10)
        .await
        .unwrap()
        .into_iter()
        .map(|r| r.action)
        .collect();
    assert_eq!(actions, ["responder.delete", "responder.create", "responder.create"]);
}
