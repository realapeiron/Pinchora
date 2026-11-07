//! The dashboard keeps Discord IDs exact and never lists a private birth year.

use super::*;
use chrono::Datelike;

#[tokio::test]
async fn birthday_settings_and_upcoming_respect_id_and_date_privacy() {
    let Some(h) = Harness::new("birthdays_api").await else {
        return;
    };
    let (guild, _owner, mut browser) = h.managed_server().await;
    let path = format!("/api/guilds/{guild}/birthdays");
    let initial = browser.get(&path).await;
    assert_eq!(initial.status, StatusCode::OK, "{}", initial.text());
    assert_eq!(initial.json()["settings"]["channel_id"], Value::Null);

    let saved = browser
        .write(
            Method::PUT,
            &path,
            json!({
                "enabled": true,
                "channel_id": GENERAL.to_string(),
                "role_id": FAN.to_string(),
                "message": "Happy birthday, {user}! 🎂",
                "announce_hour": 0,
                "server_tz": "Europe/Istanbul",
                "shells_gift": 100,
                "allow_age": false,
                "badge": true
            }),
        )
        .await;
    assert_eq!(saved.status, StatusCode::OK, "{}", saved.text());
    assert_eq!(saved.json()["settings"]["channel_id"], GENERAL.to_string());
    assert_eq!(saved.json()["settings"]["role_id"], FAN.to_string());
    assert_eq!(saved.json()["settings"]["shells_gift"], 100);

    let member = unique();
    h.seed_xp(guild, member, 1).await;
    let date = chrono::Utc::now().date_naive() + chrono::Duration::days(1);
    let input = |public| pinchora_db::BirthdayInput {
        month: date.month() as i16,
        day: date.day() as i16,
        year: Some(2000),
        tz: "UTC",
        public,
        show_age: true,
    };
    h.state.app.db.set_birthday(member, &input(false)).await.unwrap();
    assert_eq!(browser.get(&path).await.json()["upcoming"], json!([]));
    h.state.app.db.set_birthday(member, &input(true)).await.unwrap();
    let upcoming = browser.get(&path).await.json()["upcoming"].clone();
    assert_eq!(upcoming[0]["user_id"], member.to_string());
    assert!(upcoming[0].get("year").is_none());
    assert!(upcoming[0].get("age").is_none());
    h.state
        .app
        .db
        .set_privacy(member, pinchora_db::Hide::Profile, true)
        .await
        .unwrap();
    assert_eq!(browser.get(&path).await.json()["upcoming"], json!([]));
}

#[tokio::test]
async fn birthday_writes_require_manager_csrf_and_audit() {
    let Some(h) = Harness::new("birthday_access").await else {
        return;
    };
    let (guild, _, mut browser) = h.managed_server().await;
    let path = format!("/api/guilds/{guild}/birthdays");
    let body = json!({"enabled":false,"channel_id":null,"role_id":null,"message":"Hello {user}","announce_hour":0,"server_tz":"UTC","shells_gift":0,"allow_age":false,"badge":false});
    let csrf = browser.csrf.take();
    assert_eq!(
        browser.write(Method::PUT, &path, body.clone()).await.status,
        StatusCode::FORBIDDEN
    );
    browser.csrf = csrf;
    let stranger = guild + 1;
    h.add_bot(stranger);
    assert_eq!(
        browser.get(&format!("/api/guilds/{stranger}/birthdays")).await.status,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        browser
            .write(Method::PUT, &format!("/api/guilds/{stranger}/birthdays"), body.clone())
            .await
            .status,
        StatusCode::FORBIDDEN
    );
    for id in ["0", "18446744073709551615"] {
        let mut invalid = body.clone();
        invalid["role_id"] = json!(id);
        assert_ne!(browser.write(Method::PUT, &path, invalid).await.status, StatusCode::OK);
    }
    assert_eq!(browser.write(Method::PUT, &path, body).await.status, StatusCode::OK);
    let audit = browser.get(&format!("/api/guilds/{guild}/audit")).await.json();
    assert!(
        audit["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r["action"] == "birthdays.set")
    );
}
