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
}
