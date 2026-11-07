//! Birthday opt-in, privacy, setup and pixel card through the real interaction handlers.
use super::*;
static TICKS: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

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
                    int_option("year", 2000),
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
        me.content().contains("29/02") && me.content().contains("private") && !me.content().contains("2000"),
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
                    int_option("year", 2000),
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
    sim.app
        .db
        .set_privacy(member, pinchora_db::Hide::Profile, true)
        .await
        .unwrap();
    let hidden = sim
        .command_resolved(
            viewer,
            "birthday",
            json!([sub("view", vec![user_option("user", member)])]),
            resolved(&[member], &[]),
        )
        .await;
    assert!(hidden.content().contains("not shared"), "{hidden}");
    let changed = sim
        .command(
            member,
            "birthday",
            json!([sub("set", vec![int_option("day", 1), int_option("month", 1)])]),
        )
        .await;
    let locked = sim.click(member, &changed, "Save birthday").await;
    assert!(locked.content().contains("locked for 30 days"), "{locked}");
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
    let _ticks = TICKS.lock().await;
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

#[tokio::test]
async fn thousand_birthdays_are_batched_after_restart_with_safe_mentions() {
    let _ticks = TICKS.lock().await;
    let Some(sim) = Sim::new("thousand_birthdays_are_batched_after_restart").await else {
        return;
    };
    let date = sim.app.now().date_naive();
    // Start well after the former midnight window.
    sim.app
        .shift_clock(date.and_hms_opt(12, 0, 0).unwrap().and_utc() - sim.app.now());
    let users: Vec<i64> = (0..1000).map(|_| unique() as i64).collect();
    let db = &sim.app.db;
    db.guild_settings(sim.guild).await.unwrap();
    sqlx::query("INSERT INTO members (guild_id,user_id) SELECT $1,unnest($2::bigint[])")
        .bind(sim.guild as i64)
        .bind(&users)
        .execute(db.pool())
        .await
        .unwrap();
    sqlx::query("INSERT INTO birthdays (user_id,month,day,public) SELECT unnest($1::bigint[]),$2,$3,true")
        .bind(&users)
        .bind(chrono::Datelike::month(&date) as i16)
        .bind(chrono::Datelike::day(&date) as i16)
        .execute(db.pool())
        .await
        .unwrap();
    db.save_birthday_settings(
        sim.guild,
        &pinchora_core::birthdays::BirthdaySettings {
            enabled: true,
            channel_id: Some(sim.channel),
            message: "Happy birthday {user}! <@123> <@&456>".into(),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    pinchora_commands::birthdays_tick(&sim.app).await.unwrap();
    pinchora_commands::birthdays_tick(&sim.app).await.unwrap();
    let posts = sim.posts();
    let mine: Vec<_> = posts.iter().filter(|p| p.channel == sim.channel).collect();
    assert_eq!(mine.len(), 40);
    let mut mentioned = Vec::new();
    for post in mine {
        assert!(post.content().encode_utf16().count() <= 2000);
        assert_eq!(post.body["enforce_nonce"], true);
        let mentions = &post.body["allowed_mentions"];
        assert!(mentions["parse"].as_array().is_none_or(|v| v.is_empty()));
        assert!(mentions["roles"].as_array().is_none_or(|v| v.is_empty()));
        mentioned.extend(
            mentions["users"]
                .as_array()
                .unwrap()
                .iter()
                .map(|u| u.as_str().unwrap().parse::<i64>().unwrap()),
        );
        assert_eq!(post.files.len(), 1);
    }
    mentioned.sort_unstable();
    let mut expected = users.clone();
    expected.sort_unstable();
    assert_eq!(mentioned, expected);
    // Clean just this test's bulk fixture so subsequent ticker tests cannot pick it up.
    sqlx::query("DELETE FROM birthdays WHERE user_id=ANY($1)")
        .bind(&users)
        .execute(db.pool())
        .await
        .unwrap();
}

#[tokio::test]
async fn birthday_role_rechecks_safety_and_expired_grants_never_reappear() {
    let _ticks = TICKS.lock().await;
    let Some(sim) = Sim::new("birthday_role_rechecks_safety").await else {
        return;
    };
    let (user, role) = (unique(), unique());
    sim.guild_role_with(role, "Now dangerous", 2, 1 << 3);
    let db = &sim.app.db;
    db.set_birthday(
        user,
        &pinchora_db::BirthdayInput {
            month: 1,
            day: 1,
            year: None,
            tz: "UTC",
            public: true,
            show_age: false,
        },
    )
    .await
    .unwrap();
    db.save_birthday_settings(
        sim.guild,
        &pinchora_core::birthdays::BirthdaySettings {
            enabled: true,
            channel_id: Some(sim.channel),
            role_id: Some(role),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    db.claim_birthday(
        sim.guild,
        user,
        2026,
        0,
        false,
        Some((role, sim.app.now() + chrono::Duration::hours(24))),
    )
    .await
    .unwrap();
    pinchora_commands::birthdays_tick(&sim.app).await.unwrap();
    assert!(
        !sim.mock
            .member_roles
            .lock()
            .unwrap()
            .get(&user)
            .is_some_and(|roles| roles.contains(&role))
    );
    // A crash before grant, followed by a restart after its durable deadline, must not grant a stale role.
    let role_id = role.to_string();
    for current in sim.mock.roles.lock().unwrap().iter_mut() {
        if current["id"].as_str() == Some(role_id.as_str()) {
            current["permissions"] = json!("0");
        }
    }
    let late = unique();
    db.set_birthday(
        late,
        &pinchora_db::BirthdayInput {
            month: 1,
            day: 1,
            year: None,
            tz: "UTC",
            public: true,
            show_age: false,
        },
    )
    .await
    .unwrap();
    db.claim_birthday(
        sim.guild,
        late,
        2026,
        0,
        false,
        Some((role, sim.app.now() - chrono::Duration::hours(1))),
    )
    .await
    .unwrap();
    pinchora_commands::birthdays_tick(&sim.app).await.unwrap();
    assert!(
        !sim.mock
            .member_roles
            .lock()
            .unwrap()
            .get(&late)
            .is_some_and(|roles| roles.contains(&role))
    );
}

#[tokio::test]
async fn birthday_keeps_existing_roles_and_obeys_module_switch() {
    let _ticks = TICKS.lock().await;
    let Some(sim) = Sim::new("birthday_existing_role").await else {
        return;
    };
    let (user, role) = (unique(), unique());
    sim.guild_role(role, "Existing", 2);
    sim.say(user, &[role]).await;
    sim.posts();
    sim.mock.member_roles.lock().unwrap().insert(user, vec![role]);
    let db = &sim.app.db;
    let date = sim.app.now().date_naive();
    db.set_birthday(
        user,
        &pinchora_db::BirthdayInput {
            month: chrono::Datelike::month(&date) as i16,
            day: chrono::Datelike::day(&date) as i16,
            year: None,
            tz: "UTC",
            public: true,
            show_age: false,
        },
    )
    .await
    .unwrap();
    db.save_birthday_settings(
        sim.guild,
        &pinchora_core::birthdays::BirthdaySettings {
            enabled: true,
            channel_id: Some(sim.channel),
            role_id: Some(role),
            shells_gift: 100,
            ..Default::default()
        },
    )
    .await
    .unwrap();
    db.set_module(sim.guild, pinchora_core::modules::Module::Birthdays, Some(false), None)
        .await
        .unwrap();
    sim.app.forget_guild_settings(sim.guild).await;
    pinchora_commands::birthdays_tick(&sim.app).await.unwrap();
    assert!(!sim.posts().iter().any(|p| p.channel == sim.channel));
    assert_eq!(db.user(user).await.unwrap().credits, 0);
    db.set_module(sim.guild, pinchora_core::modules::Module::Birthdays, Some(true), None)
        .await
        .unwrap();
    sim.app.forget_guild_settings(sim.guild).await;
    pinchora_commands::birthdays_tick(&sim.app).await.unwrap();
    assert_eq!(db.user(user).await.unwrap().credits, 100);
    assert!(
        db.lapsed_roles(sim.guild, user, sim.app.now() + chrono::Duration::days(2))
            .await
            .unwrap()
            .is_empty()
    );
    assert!(
        !sim.role_changes()
            .iter()
            .any(|change| change.contains(&role.to_string()))
    );
}

#[tokio::test]
async fn deleting_data_removes_the_birthday_role_before_erasing_its_timer() {
    let _ticks = TICKS.lock().await;
    let Some(mut sim) = Sim::new("birthday_delete_role").await else {
        return;
    };
    let (user, role) = (unique(), unique());
    sim.guild_role(role, "Birthday", 2);
    sim.app.db.guild_settings(sim.guild).await.unwrap();
    sim.app
        .db
        .set_birthday(
            user,
            &pinchora_db::BirthdayInput {
                month: 1,
                day: 1,
                year: None,
                tz: "UTC",
                public: true,
                show_age: false,
            },
        )
        .await
        .unwrap();
    sim.app
        .db
        .claim_birthday(
            sim.guild,
            user,
            2026,
            100,
            true,
            Some((role, sim.app.now() + chrono::Duration::hours(24))),
        )
        .await
        .unwrap();
    sim.mock.member_roles.lock().unwrap().insert(user, vec![role]);
    sim.allow_role_changes(false);
    let refused = delete_through_the_panel(&mut sim, user).await;
    assert!(refused.content().contains("No data was deleted"), "{refused}");
    assert!(sim.app.db.birthday(user).await.unwrap().is_some());
    sim.allow_role_changes(true);
    let done = delete_through_the_panel(&mut sim, user).await;
    assert_eq!(done.title(), "Your data is deleted");
    assert_eq!(sim.role_changes(), vec![format!("DELETE {user} {role}")]);
    for table in ["birthdays", "birthday_sent", "birthday_gift_years", "timed_roles"] {
        let count: i64 = sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
            "SELECT count(*) FROM {table} WHERE user_id=$1"
        )))
        .bind(user as i64)
        .fetch_one(sim.app.db.pool())
        .await
        .unwrap();
        assert_eq!(count, 0, "{table}");
    }
}
