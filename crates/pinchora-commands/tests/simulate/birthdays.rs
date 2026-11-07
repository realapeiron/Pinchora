//! Birthday opt-in, privacy, setup and pixel card through the real interaction handlers.
use super::*;

#[tokio::test]
async fn birthday_confirmation_privacy_and_preview() {
    let Some(mut sim) = Sim::new("birthday_confirmation_privacy_and_preview").await else {
        return;
    };
    let member = unique();
    let viewer = unique();
    let ask = sim
        .command(
            member,
            "birthday",
            json!([sub(
                "set",
                vec![
                    int_option("day", 29),
                    int_option("month", 2),
                    text_option("timezone", "Europe/Istanbul")
                ]
            )]),
        )
        .await;
    assert!(ask.ephemeral && ask.content().contains("29/02"), "{ask}");
    let stolen = sim.click(viewer, &ask, "Save birthday").await;
    assert!(
        stolen.ephemeral && stolen.content().contains("Only the person"),
        "{stolen}"
    );
    let saved = sim.click(member, &ask, "Save birthday").await;
    assert!(saved.ephemeral && saved.content().contains("saved"), "{saved}");
    let private = sim
        .command_resolved(
            viewer,
            "birthday",
            json!([sub("view", vec![user_option("user", member)])]),
            resolved(&[member], &[]),
        )
        .await;
    assert!(private.content().contains("not shared"), "{private}");
    let me = sim.command(member, "birthday", json!([sub("me", vec![])])).await;
    assert!(
        me.content().contains("29/02") && me.content().contains("private"),
        "{me}"
    );
    let public = sim
        .command(
            member,
            "birthday",
            json!([sub(
                "set",
                vec![
                    int_option("day", 29),
                    int_option("month", 2),
                    text_option("timezone", "Europe/Istanbul"),
                    bool_option("public", true)
                ]
            )]),
        )
        .await;
    sim.click(member, &public, "Save birthday").await;
    let visible = sim
        .command_resolved(
            viewer,
            "birthday",
            json!([sub("view", vec![user_option("user", member)])]),
            resolved(&[member], &[]),
        )
        .await;
    assert!(visible.content().contains("29/02"), "{visible}");
    let denied = sim.command(viewer, "birthdays", json!([sub("settings", vec![])])).await;
    assert!(
        denied.ephemeral && denied.content().contains("Manage Server"),
        "{denied}"
    );
    sim.permissions = MANAGE_GUILD;
    let preview = sim.command(member, "birthdays", json!([sub("preview", vec![])])).await;
    assert_eq!(preview.files.len(), 1, "{preview}");
    let png = image::load_from_memory(&preview.files[0].1).unwrap();
    assert_eq!((png.width(), png.height()), (900, 360));
    let removed = sim.command(member, "birthday", json!([sub("remove", vec![])])).await;
    assert!(removed.content().contains("removed"), "{removed}");
}

#[tokio::test]
async fn local_midnight_posts_one_pixel_card_and_one_gift() {
    let Some(sim) = Sim::new("local_midnight_posts_one_pixel_card_and_one_gift").await else {
        return;
    };
    let user = unique();
    sim.say(user, &[]).await;
    sim.posts();
    let tomorrow = sim.app.now().date_naive().succ_opt().unwrap();
    let midnight = tomorrow.and_hms_opt(0, 5, 0).unwrap().and_utc();
    sim.app.shift_clock(midnight - sim.app.now());
    sim.app
        .db
        .set_birthday(
            user,
            &pinchora_db::BirthdayInput {
                month: chrono::Datelike::month(&tomorrow) as i16,
                day: chrono::Datelike::day(&tomorrow) as i16,
                year: None,
                tz: "UTC",
                public: true,
                show_age: false,
            },
        )
        .await
        .unwrap();
    let settings = pinchora_core::birthdays::BirthdaySettings {
        enabled: true,
        channel_id: Some(sim.channel),
        shells_gift: 100,
        badge: true,
        ..Default::default()
    };
    sim.app.db.save_birthday_settings(sim.guild, &settings).await.unwrap();
    // The ticker scans all test guilds, including rows from previous simulator runs.
    // Assert this guild's delivery below instead of its global post count.
    pinchora_commands::birthdays_tick(&sim.app).await.unwrap();
    pinchora_commands::birthdays_tick(&sim.app).await.unwrap();
    assert_eq!(sim.app.db.user(user).await.unwrap().credits, 100);
    assert!(
        sim.app
            .db
            .owned_badges(user)
            .await
            .unwrap()
            .contains(&"cake_day".to_owned())
    );
    let posts = sim.posts();
    let mine: Vec<_> = posts
        .iter()
        .filter(|p| p.channel == sim.channel && p.content().contains("Happy birthday"))
        .collect();
    assert_eq!(mine.len(), 1, "{} posts", posts.len());
    assert_eq!(mine[0].files.len(), 1);
    let png = image::load_from_memory(&mine[0].files[0].1).unwrap();
    assert_eq!((png.width(), png.height()), (900, 360));
}
