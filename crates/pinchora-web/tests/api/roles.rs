//! The roles page over the API: auto roles, sticky roles, role panels (saved, posted by the bot, deleted)
//! and colour roles, each with the rules and the sentences the commands use.

use super::*;

fn g(guild: u64) -> String {
    format!("/api/guilds/{guild}/roles")
}

/// The first error of a refused request: its field and message.
fn refusal(response: &Response) -> (String, String) {
    let error = &response.json()["error"];
    (
        error["field"].as_str().unwrap_or_default().to_owned(),
        error["message"].as_str().unwrap_or_default().to_owned(),
    )
}

fn panel(title: &str, style: &str, roles: Value) -> Value {
    json!({ "title": title, "style": style, "roles": roles })
}

fn entry(role: u64, label: &str) -> Value {
    json!({ "role_id": role.to_string(), "label": label })
}

#[tokio::test]
async fn auto_roles_and_sticky_roles_follow_the_command_rules() {
    let Some(h) = Harness::new("roles_auto").await else {
        return;
    };
    let (guild, _, mut browser) = h.managed_server().await;
    let base = g(guild);
    let mut heard = Box::pin(h.state.app.cache.invalidations().await.expect("subscribe"));

    // Nothing is set up yet, and the page knows its limits and the palette.
    let page = browser.get(&base).await;
    assert_eq!(page.status, StatusCode::OK, "{}", page.text());
    let view = page.json();
    assert_eq!(view["auto"]["items"], json!([]));
    assert_eq!(
        (view["auto"]["max"].as_i64(), view["auto"]["sticky"].as_bool()),
        (Some(10), Some(false))
    );
    assert_eq!(view["panels"]["items"], json!([]));
    assert_eq!(
        (
            view["colors"]["max"].as_i64(),
            view["colors"]["enabled"].as_bool(),
            view["colors"]["mode"].as_str()
        ),
        (Some(32), Some(false), Some("palette"))
    );
    assert_eq!(
        view["colors"]["palette"][0],
        json!({ "name": "Coral", "hex": "#ff6b4a" })
    );

    // A role with a delay and an expiry, like `/autoroles add`.
    let added = browser
        .write(
            Method::PUT,
            &format!("{base}/auto/{FAN}/join"),
            json!({ "delay_secs": 600, "remove_after_secs": 86_400 }),
        )
        .await;
    assert_eq!(added.status, StatusCode::OK, "{}", added.text());
    assert_eq!(
        added.json()["auto"]["items"],
        json!([{ "role_id": FAN.to_string(), "kind": "join", "delay_secs": 600, "remove_after_secs": 86_400, "enabled": true }])
    );
    assert!(
        announced(&mut heard, Invalidation::Guild(guild)).await,
        "the bot hears of it"
    );
    // The same role and kind again changes it; another kind is another row; pausing keeps the row.
    let paused = browser
        .write(
            Method::PUT,
            &format!("{base}/auto/{FAN}/join"),
            json!({ "delay_secs": 0, "enabled": false }),
        )
        .await;
    assert_eq!(paused.json()["auto"]["items"][0]["enabled"], false);
    assert_eq!(paused.json()["auto"]["items"][0]["delay_secs"], 0);
    browser
        .write(Method::PUT, &format!("{base}/auto/{FAN}/bot"), json!({}))
        .await;
    let both = browser.get(&base).await.json();
    assert_eq!(both["auto"]["items"].as_array().unwrap().len(), 2);

    // What the commands refuse, with their sentences.
    for (role, sentence) in [
        (
            ADMIN,
            "@Admin is above my highest role. Drag my role above it in Server Settings > Roles, then try again.",
        ),
        (
            BOOSTER,
            "@Server Booster is managed by an integration or a bot, so it can't be handed out.",
        ),
        (guild, "@everyone can't be handed out: every member already has it."),
    ] {
        let refused = browser
            .write(Method::PUT, &format!("{base}/auto/{role}/join"), json!({}))
            .await;
        assert_eq!(refused.status, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(refusal(&refused), ("role_id".to_owned(), sentence.to_owned()), "{role}");
    }
    for (body, field) in [
        (json!({ "delay_secs": 5 }), "delay_secs"),
        (json!({ "delay_secs": 40 * 86_400 }), "delay_secs"),
        (json!({ "remove_after_secs": 10 }), "remove_after_secs"),
        (json!({ "remove_after_secs": 400 * 86_400 }), "remove_after_secs"),
    ] {
        let refused = browser
            .write(Method::PUT, &format!("{base}/auto/{REGULAR}/join"), body)
            .await;
        assert_eq!(refused.status, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(refusal(&refused).0, field);
    }
    assert_eq!(
        browser
            .write(Method::PUT, &format!("{base}/auto/{REGULAR}/nobody"), json!({}))
            .await
            .status,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        browser
            .write(Method::PUT, &format!("{base}/auto/{REGULAR}/all"), json!({}))
            .await
            .status,
        StatusCode::NOT_FOUND,
        "`all` is for taking roles away only"
    );

    // Ten at most; a row at the limit can still be changed.
    for n in 0..8 {
        h.state
            .app
            .db
            .save_auto_role(
                guild,
                &pinchora_db::NewAutoRole {
                    role_id: 5_000 + n,
                    kind: "join",
                    delay_secs: 0,
                    remove_after_secs: 0,
                    enabled: true,
                },
                10,
            )
            .await
            .unwrap();
    }
    let full = browser
        .write(Method::PUT, &format!("{base}/auto/{REGULAR}/join"), json!({}))
        .await;
    assert_eq!(full.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(full.json()["error"]["code"], "limit");
    assert_eq!(refusal(&full).1, "A server can have 10 auto roles. Remove one first.");
    let again = browser
        .write(
            Method::PUT,
            &format!("{base}/auto/{FAN}/bot"),
            json!({ "delay_secs": 60 }),
        )
        .await;
    assert_eq!(again.status, StatusCode::OK, "{}", again.text());

    // Taking one away: one kind, or every kind.
    let one = browser
        .write(Method::DELETE, &format!("{base}/auto/{FAN}/bot"), json!({}))
        .await;
    assert_eq!(one.json()["auto"]["items"].as_array().unwrap().len(), 9);
    let all = browser
        .write(Method::DELETE, &format!("{base}/auto/{FAN}/all"), json!({}))
        .await;
    assert_eq!(all.json()["auto"]["items"].as_array().unwrap().len(), 8);

    // Sticky roles are a switch, and the page says so.
    let on = browser
        .write(Method::PUT, &format!("{base}/sticky"), json!({ "enabled": true }))
        .await;
    assert_eq!(on.json()["auto"]["sticky"], true);
    assert!(h.state.app.db.sticky_enabled(guild).await.unwrap());
    let off = browser
        .write(Method::PUT, &format!("{base}/sticky"), json!({ "enabled": false }))
        .await;
    assert_eq!(off.json()["auto"]["sticky"], false);
    assert_eq!(
        browser
            .write(Method::PUT, &format!("{base}/sticky"), json!({ "enabled": "yes" }))
            .await
            .status,
        StatusCode::BAD_REQUEST,
        "a switch is true or false"
    );

    // Every change is in the audit log.
    let audit = browser.get(&format!("/api/guilds/{guild}/audit")).await.json();
    let actions: Vec<&str> = audit["items"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|i| i["action"].as_str())
        .collect();
    for wanted in ["roles.auto.put", "roles.auto.delete", "roles.sticky"] {
        assert!(actions.contains(&wanted), "{wanted} is not in {actions:?}");
    }
}

#[tokio::test]
async fn panels_are_checked_saved_posted_and_deleted() {
    let Some(h) = Harness::new("roles_panels").await else {
        return;
    };
    let (guild, _, mut browser) = h.managed_server().await;
    let base = g(guild);

    // What a panel may not be: untitled, too long, repeated, with a role the bot cannot give, an emoji it
    // cannot read, a rule that makes no sense.
    let roles = json!([entry(FAN, "Fan"), entry(REGULAR, "Regular")]);
    for (body, field, text) in [
        (panel("  ", "buttons", roles.clone()), "title", "1 to 80 characters"),
        (
            panel(&"x".repeat(81), "buttons", roles.clone()),
            "title",
            "1 to 80 characters",
        ),
        (
            panel("Ok", "dropdown", roles.clone()),
            "style",
            "buttons, select or reactions",
        ),
        (
            panel("Ok", "buttons", json!([entry(FAN, "Fan"), entry(FAN, "Again")])),
            "roles",
            "twice",
        ),
        (
            panel("Ok", "buttons", json!([entry(ADMIN, "Boss")])),
            "roles",
            "@Admin is above my highest role",
        ),
        (
            panel(
                "Ok",
                "buttons",
                json!([{ "role_id": FAN.to_string(), "label": "Fan", "emoji": "fan" }]),
            ),
            "roles",
            "couldn't read the emoji",
        ),
        (
            panel("Ok", "buttons", json!([{ "role_id": FAN.to_string(), "label": " " }])),
            "roles",
            "1 to 80 characters",
        ),
        (
            panel(
                "Ok",
                "buttons",
                json!([{ "role_id": FAN.to_string(), "label": "Fan", "temp_secs": 5 }]),
            ),
            "roles",
            "a minute to a year",
        ),
    ] {
        let refused = browser.write(Method::POST, &format!("{base}/panels"), body).await;
        assert_eq!(refused.status, StatusCode::UNPROCESSABLE_ENTITY, "{field}");
        let (got, message) = refusal(&refused);
        assert_eq!(got, field, "{message}");
        assert!(message.contains(text), "{field}: {message}");
    }
    let mut rules = panel("Ok", "select", roles.clone());
    rules["min_picks"] = json!(3);
    rules["max_picks"] = json!(2);
    let refused = browser.write(Method::POST, &format!("{base}/panels"), rules).await;
    assert_eq!(
        refusal(&refused),
        (
            "min_picks".to_owned(),
            "The fewest roles can't be more than the most.".to_owned()
        )
    );
    assert!(
        browser.get(&base).await.json()["panels"]["items"]
            .as_array()
            .unwrap()
            .is_empty()
    );

    // A panel with rules, saved in one go.
    let mut doc = panel(
        "Teams",
        "select",
        json!([
            { "role_id": FAN.to_string(), "label": "Fan", "emoji": "🎉", "description": "Cheers", "temp_secs": 3600 },
            entry(REGULAR, "Regular"),
        ]),
    );
    doc["description"] = json!("Pick one");
    doc["exclusive"] = json!(true);
    doc["required_role"] = json!(HELPER.to_string());
    doc["min_picks"] = json!(1);
    let made = browser.write(Method::POST, &format!("{base}/panels"), doc).await;
    assert_eq!(made.status, StatusCode::OK, "{}", made.text());
    let saved = made.json()["panels"]["items"][0].clone();
    assert_eq!(
        (
            saved["title"].as_str(),
            saved["style"].as_str(),
            saved["exclusive"].as_bool(),
            saved["required_role"].as_str()
        ),
        (
            Some("Teams"),
            Some("select"),
            Some(true),
            Some(HELPER.to_string().as_str())
        )
    );
    assert_eq!(saved["roles"][0]["description"], "Cheers");
    assert_eq!(saved["roles"][0]["temp_secs"], 3600);
    assert_eq!(saved["roles"][1]["label"], "Regular");
    let id = saved["id"].as_str().unwrap().to_owned();
    // The commands see the same panel.
    let by_title = h.state.app.db.find_panel(guild, "teams").await.unwrap().unwrap();
    assert_eq!((by_title.style.as_str(), by_title.min_picks), ("select", 1));
    // A title another panel has is refused, in any case.
    let clash = browser
        .write(
            Method::POST,
            &format!("{base}/panels"),
            panel("TEAMS", "buttons", json!([])),
        )
        .await;
    assert_eq!(refusal(&clash).0, "title");

    // Saving replaces the roles; a role the panel already has stays even when it could not be added now.
    let mut next = panel(
        "Teams v2",
        "buttons",
        json!([entry(REGULAR, "Regular"), entry(HELPER, "Helper")]),
    );
    next["description"] = json!(null);
    let replaced = browser.write(Method::PUT, &format!("{base}/panels/{id}"), next).await;
    assert_eq!(replaced.status, StatusCode::OK, "{}", replaced.text());
    let items = replaced.json()["panels"]["items"].clone();
    assert_eq!(
        (items[0]["title"].as_str(), items[0]["roles"].as_array().unwrap().len()),
        (Some("Teams v2"), 2)
    );
    assert_eq!(items[0]["exclusive"], false, "what the editor sent is what is saved");
    assert_eq!(
        browser
            .write(
                Method::PUT,
                &format!("{base}/panels/{}", 987_654_321),
                panel("X", "buttons", json!([]))
            )
            .await
            .status,
        StatusCode::NOT_FOUND
    );

    // The bot posts it: a channel it can post in, from the server's list.
    for (channel, expect) in [
        (LOUNGE, "isn't a text or announcement channel"),
        (1, "no longer exists"),
    ] {
        let refused = browser
            .write(
                Method::POST,
                &format!("{base}/panels/{id}/post"),
                json!({ "channel_id": channel.to_string() }),
            )
            .await;
        assert_eq!(refusal(&refused).0, "channel_id");
        assert!(refusal(&refused).1.contains(expect), "{}", refusal(&refused).1);
    }
    let posted = browser
        .write(
            Method::POST,
            &format!("{base}/panels/{id}/post"),
            json!({ "channel_id": GENERAL.to_string() }),
        )
        .await;
    assert_eq!(posted.status, StatusCode::OK, "{}", posted.text());
    let body = posted.json();
    assert_eq!(body["posted"]["channel_id"], GENERAL.to_string());
    assert_eq!(body["posted"]["updated"], false);
    assert!(
        body["posted"]["url"]
            .as_str()
            .unwrap()
            .starts_with(&format!("https://discord.com/channels/{guild}/{GENERAL}/"))
    );
    assert_eq!(body["panels"]["items"][0]["channel_id"], GENERAL.to_string());
    assert!(body["panels"]["items"][0]["message_id"].is_string());

    // A reactions panel posted where Discord will not take the emojis says which ones.
    let reactions = browser
        .write(
            Method::POST,
            &format!("{base}/panels"),
            panel(
                "Pings",
                "reactions",
                json!([
                    { "role_id": FAN.to_string(), "label": "Fan", "emoji": "🔔" },
                    { "role_id": REGULAR.to_string(), "label": "Regular", "emoji": "<:blob:555>", },
                ]),
            ),
        )
        .await;
    let pings = reactions.json()["panels"]["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["title"] == "Pings")
        .unwrap()
        .clone();
    let sent = browser
        .write(
            Method::POST,
            &format!("{base}/panels/{}/post", pings["id"].as_str().unwrap()),
            json!({ "channel_id": GENERAL.to_string() }),
        )
        .await;
    assert_eq!(sent.status, StatusCode::OK, "{}", sent.text());
    assert_eq!(
        sent.json()["posted"]["unusable"].as_array().unwrap().len(),
        2,
        "the mock takes no reactions"
    );
    // No emoji, or the same emoji twice, cannot be posted.
    let bare = browser
        .write(
            Method::POST,
            &format!("{base}/panels"),
            panel("Bare", "reactions", json!([entry(FAN, "Fan")])),
        )
        .await;
    let bare_id = bare.json()["panels"]["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["title"] == "Bare")
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let refused = browser
        .write(
            Method::POST,
            &format!("{base}/panels/{bare_id}/post"),
            json!({ "channel_id": GENERAL.to_string() }),
        )
        .await;
    assert_eq!(refusal(&refused).0, "roles");
    assert!(
        refusal(&refused).1.contains("needs an emoji on every role"),
        "{}",
        refusal(&refused).1
    );

    // Deleting a panel takes it off the page; a second time is quiet.
    let gone = browser
        .write(Method::DELETE, &format!("{base}/panels/{id}"), json!({}))
        .await;
    assert_eq!(gone.status, StatusCode::OK);
    assert!(
        gone.json()["panels"]["items"]
            .as_array()
            .unwrap()
            .iter()
            .all(|p| p["id"] != id.as_str())
    );
    assert_eq!(
        browser
            .write(Method::DELETE, &format!("{base}/panels/{id}"), json!({}))
            .await
            .status,
        StatusCode::OK
    );
    assert!(h.state.app.db.find_panel(guild, "Teams v2").await.unwrap().is_none());
}

#[tokio::test]
async fn colour_roles_save_with_the_command_rules() {
    let Some(h) = Harness::new("roles_colours").await else {
        return;
    };
    let (guild, _, mut browser) = h.managed_server().await;
    let base = g(guild);

    let saved = browser
        .write(
            Method::PUT,
            &format!("{base}/colors"),
            json!({ "enabled": true, "mode": "hex", "allowed_roles": [FAN.to_string(), FAN.to_string()], "anchor_role_id": REGULAR.to_string() }),
        )
        .await;
    assert_eq!(saved.status, StatusCode::OK, "{}", saved.text());
    let colors = &saved.json()["colors"];
    assert_eq!(
        (
            colors["enabled"].as_bool(),
            colors["mode"].as_str(),
            colors["anchor_role_id"].as_str()
        ),
        (Some(true), Some("hex"), Some(REGULAR.to_string().as_str()))
    );
    assert_eq!(
        colors["allowed_roles"],
        json!([FAN.to_string()]),
        "a role listed twice is listed once"
    );
    let stored = h.state.app.db.color_settings(guild).await.unwrap();
    assert_eq!((stored.enabled, stored.mode.as_str()), (true, "hex"));

    for (body, field) in [
        (json!({ "enabled": true, "mode": "rainbow" }), "mode"),
        (
            json!({ "enabled": true, "mode": "palette", "allowed_roles": ["1"] }),
            "allowed_roles",
        ),
        (
            json!({ "enabled": true, "mode": "palette", "allowed_roles": ["x"] }),
            "allowed_roles",
        ),
        (
            json!({ "enabled": true, "mode": "palette", "anchor_role_id": ADMIN.to_string() }),
            "anchor_role_id",
        ),
        (
            json!({ "enabled": true, "mode": "palette", "anchor_role_id": BOOSTER.to_string() }),
            "anchor_role_id",
        ),
    ] {
        let refused = browser.write(Method::PUT, &format!("{base}/colors"), body).await;
        assert_eq!(refused.status, StatusCode::UNPROCESSABLE_ENTITY, "{field}");
        assert_eq!(refusal(&refused).0, field);
    }
    let many: Vec<String> = (1..=11).map(|n| (9_000 + n).to_string()).collect();
    let refused = browser
        .write(
            Method::PUT,
            &format!("{base}/colors"),
            json!({ "enabled": true, "mode": "palette", "allowed_roles": many }),
        )
        .await;
    assert!(
        refusal(&refused).1.contains("At most 10 roles"),
        "{}",
        refusal(&refused).1
    );
    assert_eq!(
        h.state.app.db.color_settings(guild).await.unwrap().anchor_role_id,
        Some(REGULAR as i64),
        "a refused save changes nothing"
    );

    // The bot makes the missing roles; where Discord will not, the page is told why and nothing is kept.
    let made = browser
        .write(Method::POST, &format!("{base}/colors/create-missing"), json!({}))
        .await;
    assert_eq!(made.status, StatusCode::OK, "{}", made.text());
    assert_eq!(made.json()["report"]["created"], json!([]));
    assert_eq!(
        made.json()["report"]["stopped"],
        "Discord refused to make that colour role."
    );
    assert!(h.state.app.db.color_roles(guild).await.unwrap().is_empty());

    // Taking a colour role off the list (and, when asked, off Discord).
    h.state
        .app
        .db
        .add_color_role(guild, FAN, "#06d6a0", "Mint", 32)
        .await
        .unwrap();
    h.state
        .app
        .db
        .add_color_role(guild, REGULAR, "#8fd3ff", "Sky", 32)
        .await
        .unwrap();
    let listed = browser.get(&base).await.json();
    assert_eq!(listed["colors"]["roles"].as_array().unwrap().len(), 2);
    let one = browser
        .write(Method::DELETE, &format!("{base}/colors/{FAN}"), json!({}))
        .await;
    assert_eq!(one.json()["colors"]["roles"].as_array().unwrap().len(), 1);
    let two = browser
        .write(
            Method::DELETE,
            &format!("{base}/colors/{REGULAR}?delete_role=true"),
            json!({}),
        )
        .await;
    assert_eq!(two.status, StatusCode::OK, "{}", two.text());
    assert_eq!(two.json()["colors"]["roles"], json!([]));
    assert_eq!(
        browser
            .write(Method::DELETE, &format!("{base}/colors/{FAN}"), json!({}))
            .await
            .status,
        StatusCode::OK
    );
}

#[tokio::test]
async fn the_roles_page_is_for_managers_of_a_server_with_the_bot() {
    let Some(h) = Harness::new("roles_access").await else {
        return;
    };
    let (guild, _, mut browser) = h.managed_server().await;
    let stranger = guild + 1;
    h.add_bot(stranger);
    // Somebody else's server: not on this user's list.
    let denied = browser.get(&g(stranger)).await;
    assert_eq!(denied.status, StatusCode::FORBIDDEN);
    assert_eq!(
        browser
            .write(
                Method::PUT,
                &format!("{}/sticky", g(stranger)),
                json!({ "enabled": true })
            )
            .await
            .status,
        StatusCode::FORBIDDEN
    );
    // No login at all.
    let mut nobody = h.browser();
    assert_eq!(nobody.get(&g(guild)).await.status, StatusCode::UNAUTHORIZED);
}
