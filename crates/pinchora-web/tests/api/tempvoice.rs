//! Temp voice over the API: creators set up with the bot's own rules, the live list of rooms, the audit log.

use pinchora_db::NewTempRoom;

use super::*;

fn g(guild: u64) -> String {
    format!("/api/guilds/{guild}/temp-voice")
}

/// The first error of a refused request: its field and message.
fn refusal(response: &Response) -> (String, String) {
    let error = &response.json()["error"];
    (
        error["field"].as_str().unwrap_or_default().to_owned(),
        error["message"].as_str().unwrap_or_default().to_owned(),
    )
}

#[tokio::test]
async fn creators_are_saved_with_the_bots_rules_listed_and_audited() {
    let Some(h) = Harness::new("tempvoice_creators").await else {
        return;
    };
    let (guild, _owner, mut browser) = h.managed_server().await;
    let path = g(guild);

    let empty = browser.get(&path).await;
    assert_eq!(empty.status, StatusCode::OK, "{}", empty.text());
    let page = empty.json();
    assert_eq!(
        (page["creators"].clone(), page["rooms"].clone()),
        (json!([]), json!([]))
    );
    assert_eq!(page["enabled"], true);
    assert_eq!(page["limits"]["max_creators"], 10);
    assert_eq!(page["limits"]["default_template"], "{user}'s room");
    assert!(page["missing"].is_array() || page["missing"].is_null());

    // A voice channel becomes a creator with the defaults; its rooms go where it sits.
    let made = browser
        .write(Method::PUT, &format!("{path}/creators/{LOUNGE}"), json!({}))
        .await;
    assert_eq!(made.status, StatusCode::OK, "{}", made.text());
    let creator = made.json()["creators"][0].clone();
    assert_eq!(creator["channel_id"], LOUNGE.to_string());
    assert_eq!(creator["category_id"], CATEGORY.to_string());
    assert_eq!(creator["name_template"], "{user}'s room");
    assert_eq!(
        (
            creator["default_limit"].clone(),
            creator["max_per_user"].clone(),
            creator["panel"].clone()
        ),
        (json!(0), json!(1), json!(true))
    );
    assert_eq!(creator["rooms"], 0);

    // Any subset changes it, the rest stays; a category of null puts rooms at the top.
    let changed = browser
        .write(
            Method::PUT,
            &format!("{path}/creators/{LOUNGE}"),
            json!({ "name_template": "  Room {n} · {user} ", "default_limit": 5, "default_locked": true,
                    "panel_channel_id": GENERAL.to_string(), "category_id": null, "max_per_user": 2 }),
        )
        .await;
    assert_eq!(changed.status, StatusCode::OK, "{}", changed.text());
    let creator = changed.json()["creators"][0].clone();
    assert_eq!(creator["name_template"], "Room {n} · {user}");
    assert_eq!(creator["category_id"], Value::Null);
    assert_eq!(creator["panel_channel_id"], GENERAL.to_string());
    assert_eq!(
        (
            creator["default_limit"].clone(),
            creator["default_locked"].clone(),
            creator["max_per_user"].clone()
        ),
        (json!(5), json!(true), json!(2))
    );
    let again = browser
        .write(
            Method::PUT,
            &format!("{path}/creators/{LOUNGE}"),
            json!({ "enabled": true }),
        )
        .await;
    assert_eq!(again.status, StatusCode::OK);

    // Refused with the bot's sentences, field by field.
    let text = browser
        .write(Method::PUT, &format!("{path}/creators/{GENERAL}"), json!({}))
        .await;
    assert_eq!(text.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(refusal(&text).0, "channel_id");
    assert!(
        refusal(&text).1.contains("isn't a voice channel"),
        "{}",
        refusal(&text).1
    );
    for (body, field, words) in [
        (
            json!({ "name_template": "{nick}" }),
            "name_template",
            "isn't one of the words",
        ),
        (json!({ "name_template": "" }), "name_template", "Give the rooms a name"),
        (json!({ "default_limit": 100 }), "default_limit", "0 (no limit) to 99"),
        (json!({ "max_per_user": 0 }), "max_per_user", "1 to 5"),
        (
            json!({ "category_id": GENERAL.to_string() }),
            "category_id",
            "isn't a category",
        ),
        (
            json!({ "panel_channel_id": LOUNGE.to_string() }),
            "panel_channel_id",
            "isn't a text or announcement",
        ),
        (json!({ "category_id": "9999999" }), "category_id", "no longer exists"),
    ] {
        let refused = browser
            .write(Method::PUT, &format!("{path}/creators/{LOUNGE}"), body.clone())
            .await;
        assert_eq!(
            refused.status,
            StatusCode::UNPROCESSABLE_ENTITY,
            "{body}: {}",
            refused.text()
        );
        let (got, message) = refusal(&refused);
        assert_eq!(got, field, "{body}");
        assert!(message.contains(words), "{body}: {message}");
    }
    let colour = browser
        .write(
            Method::PUT,
            &format!("{path}/creators/{LOUNGE}"),
            json!({ "colour": 1 }),
        )
        .await;
    assert_eq!(colour.status, StatusCode::BAD_REQUEST);
    let unknown = browser
        .write(Method::PUT, &format!("{path}/creators/9999999"), json!({}))
        .await;
    assert_eq!(
        unknown.status,
        StatusCode::UNPROCESSABLE_ENTITY,
        "a channel the server doesn't have"
    );

    // The rooms alive are listed read-only, with who sits in them.
    let (room, owner, member) = (unique(), unique(), unique());
    h.state
        .app
        .db
        .create_temp_room(NewTempRoom {
            channel_id: room,
            guild_id: guild,
            source_id: LOUNGE,
            owner_id: owner,
            locked: true,
            user_limit: 4,
        })
        .await
        .unwrap();
    h.state
        .app
        .cache
        .set_add(&format!("tvoice:in:{room}"), &member.to_string())
        .await
        .unwrap();
    h.state
        .app
        .cache
        .set_add(&format!("tvoice:in:{room}"), &owner.to_string())
        .await
        .unwrap();
    let live = browser.get(&path).await.json();
    assert_eq!(live["creators"][0]["rooms"], 1);
    let shown = live["rooms"][0].clone();
    assert_eq!(shown["channel_id"], room.to_string());
    assert_eq!(shown["owner_id"], owner.to_string());
    assert_eq!(shown["source_id"], LOUNGE.to_string());
    assert_eq!(
        (
            shown["locked"].clone(),
            shown["hidden"].clone(),
            shown["user_limit"].clone()
        ),
        (json!(true), json!(false), json!(4))
    );
    assert_eq!(shown["members"], 2);
    let write_room = browser
        .write(Method::PUT, &format!("{path}/creators/{room}"), json!({}))
        .await;
    assert_eq!(
        write_room.status,
        StatusCode::UNPROCESSABLE_ENTITY,
        "a room is no creator"
    );

    // The module switch shows on the page.
    let off = browser
        .write(
            Method::PUT,
            &format!("/api/guilds/{guild}/modules/temp_voice"),
            json!({ "enabled": false }),
        )
        .await;
    assert_eq!(off.status, StatusCode::OK, "{}", off.text());
    assert_eq!(browser.get(&path).await.json()["enabled"], false);

    // Removing the creator leaves its rooms.
    let removed = browser
        .write(Method::DELETE, &format!("{path}/creators/{LOUNGE}"), json!({}))
        .await;
    assert_eq!(removed.status, StatusCode::OK);
    assert_eq!(removed.json()["creators"], json!([]));
    assert_eq!(removed.json()["rooms"].as_array().map(Vec::len), Some(1));
    let twice = browser
        .write(Method::DELETE, &format!("{path}/creators/{LOUNGE}"), json!({}))
        .await;
    assert_eq!(twice.status, StatusCode::OK, "removing what is gone is not an error");

    // Every real change is in the audit log, and only the real ones: a name for each channel.
    let rows = h.state.app.db.audit_page(guild, None, 20).await.unwrap();
    let actions: Vec<(&str, Option<&str>)> = rows
        .iter()
        .filter(|r| r.action.starts_with("voice_creator."))
        .map(|r| (r.action.as_str(), r.target.as_deref()))
        .collect();
    let target = format!("channel:{LOUNGE}");
    assert_eq!(
        actions,
        [
            ("voice_creator.delete", Some(target.as_str())),
            ("voice_creator.put", Some(target.as_str())),
            ("voice_creator.put", Some(target.as_str())),
        ]
    );
    let edit = rows
        .iter()
        .find(|r| r.action == "voice_creator.put" && r.before.as_deref() != Some("null"))
        .unwrap();
    let (before, after) = (
        serde_json::from_str::<Value>(edit.before.as_deref().unwrap()).unwrap(),
        serde_json::from_str::<Value>(edit.after.as_deref().unwrap()).unwrap(),
    );
    assert_eq!(before["default_limit"], 0);
    assert_eq!(after["default_limit"], 5);
    assert!(after.get("enabled").is_none(), "only the fields that changed");
}

#[tokio::test]
async fn only_managers_see_or_change_temp_voice() {
    let Some(h) = Harness::new("tempvoice_managers_only").await else {
        return;
    };
    let guild = unique();
    h.add_bot(guild);
    let mut browser = h.browser();
    browser
        .login(&h, unique(), vec![guild_entry(guild, "Pinchora Reef", false, 0)])
        .await;
    let read = browser.get(&g(guild)).await;
    assert_eq!(
        (read.status, read.code().as_str()),
        (StatusCode::FORBIDDEN, "not_manager")
    );
    let write = browser
        .write(Method::PUT, &format!("{}/creators/{LOUNGE}", g(guild)), json!({}))
        .await;
    assert_eq!(write.code(), "not_manager");
    assert!(h.state.app.db.voice_creators(guild).await.unwrap().is_empty());
}

#[tokio::test]
async fn a_server_has_at_most_ten_creators() {
    let Some(h) = Harness::new("tempvoice_creator_cap").await else {
        return;
    };
    let (guild, _, mut browser) = h.managed_server().await;
    // Ten creators the server set up some other way (the mock lists one voice channel only).
    for n in 0..10 {
        let saved = h
            .state
            .app
            .db
            .save_voice_creator(&pinchora_db::VoiceCreator {
                guild_id: guild as i64,
                channel_id: (unique() + n) as i64,
                category_id: None,
                name_template: "{user}'s room".to_owned(),
                default_limit: 0,
                default_locked: false,
                max_per_user: 1,
                panel: true,
                panel_channel_id: None,
                enabled: true,
            })
            .await
            .unwrap();
        assert!(matches!(saved, pinchora_db::CreatorSave::Saved(true)));
    }
    let eleventh = browser
        .write(Method::PUT, &format!("{}/creators/{LOUNGE}", g(guild)), json!({}))
        .await;
    assert_eq!(
        (eleventh.status, eleventh.code().as_str()),
        (StatusCode::UNPROCESSABLE_ENTITY, "limit")
    );
    assert!(
        eleventh.json()["error"]["message"]
            .as_str()
            .unwrap_or_default()
            .contains("10 creators"),
        "{}",
        eleventh.text()
    );
}
