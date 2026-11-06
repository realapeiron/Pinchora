//! `/info` cards and XP administration, including an offline MEE6 server.

use super::*;

#[tokio::test]
async fn info_cards_render_and_hide_private_profile_data() {
    let Some(mut sim) = Sim::new("info_cards_render_and_hide_private_profile_data").await else {
        return;
    };
    let user = unique();
    let roles: Vec<u64> = (0..8)
        .map(|i| {
            let id = unique();
            sim.guild_role(id, &format!("Rôle {i} \u{202e}<>&"), i + 1);
            id
        })
        .collect();
    sim.mock.member_roles.lock().unwrap().insert(user, roles);
    let mut people = resolved(&[user], &[]);
    people["users"][user.to_string()] =
        person_json(user, &format!("{}\u{202e}<bad>&", "Z\u{0301}".repeat(200)), None, false);
    let visible = sim
        .command_resolved(
            user,
            "info",
            json!([sub("user", vec![user_option("user", user)])]),
            people.clone(),
        )
        .await;
    assert_eq!(visible.files.len(), 1, "{visible}");
    let image = image::load_from_memory(&visible.files[0].1).unwrap();
    assert_eq!(image.width(), 900);
    let next = sim.click(user, &visible, "Next").await;
    assert_eq!(next.files.len(), 1, "{next}");
    assert_ne!(visible.files[0].1, next.files[0].1);
    sim.app
        .db
        .set_privacy(user, pinchora_db::Hide::Profile, true)
        .await
        .unwrap();
    let viewer = unique();
    let hidden = sim
        .command_resolved(
            viewer,
            "info",
            json!([sub("user", vec![user_option("user", user)])]),
            people,
        )
        .await;
    assert_eq!(hidden.files.len(), 1, "{hidden}");
    assert_ne!(visible.files[0].1, hidden.files[0].1);
    let server = sim.command(viewer, "info", json!([sub("server", vec![])])).await;
    assert_eq!(server.files.len(), 1, "{server}");
    let members = sim.command(viewer, "info", json!([sub("members", vec![])])).await;
    assert_eq!(members.files.len(), 1, "{members}");
}

#[tokio::test]
async fn xp_admin_requires_manage_server_and_confirms_reset() {
    let Some(mut sim) = Sim::new("xp_admin_requires_manage_server_and_confirms_reset").await else {
        return;
    };
    let (actor, target) = (unique(), unique());
    let options = json!([sub("add", vec![user_option("user", target), int_option("amount", 200)])]);
    let denied = sim
        .command_resolved(actor, "xp", options.clone(), resolved(&[target], &[]))
        .await;
    assert!(
        denied.ephemeral && denied.content().contains("Manage Server"),
        "{denied}"
    );
    for command in [
        sub("remove", vec![user_option("user", target), int_option("amount", 1)]),
        sub("set", vec![user_option("user", target), int_option("amount", 1)]),
        sub("reset", vec![user_option("user", target)]),
        sub("reset-all", vec![]),
        group("import", sub("mee6", vec![text_option("mode", "merge")])),
    ] {
        let denied = sim
            .command_resolved(actor, "xp", json!([command]), resolved(&[target], &[]))
            .await;
        assert!(
            denied.ephemeral && denied.content().contains("Manage Server"),
            "{denied}"
        );
    }
    sim.permissions = MANAGE_GUILD;
    let added = sim
        .command_resolved(actor, "xp", options, resolved(&[target], &[]))
        .await;
    assert!(added.content().contains("200"), "{added}");
    assert_eq!(
        sim.app.db.member_rank(sim.guild, target).await.unwrap().unwrap().xp,
        200
    );
    let ask = sim
        .command_resolved(
            actor,
            "xp",
            json!([sub("reset", vec![user_option("user", target)])]),
            resolved(&[target], &[]),
        )
        .await;
    assert!(ask.ephemeral, "{ask}");
    let done = sim.click(actor, &ask, "Confirm").await;
    assert!(done.content().contains("0"), "{done}");
    assert_eq!(sim.app.db.member_rank(sim.guild, target).await.unwrap().unwrap().xp, 0);
    let all = sim.command(actor, "xp", json!([sub("reset-all", vec![])])).await;
    let second = sim.click(actor, &all, "Confirm").await;
    assert!(second.content().contains("final confirmation"), "{second}");
    let complete = sim.click(actor, &second, "Confirm").await;
    assert!(complete.content().contains("Reset server XP"), "{complete}");
}

#[tokio::test]
async fn mee6_import_uses_mock_http_and_is_daily() {
    let Some(mut sim) = Sim::new("mee6_import_uses_mock_http_and_is_daily").await else {
        return;
    };
    Arc::get_mut(&mut sim.app).unwrap().web = reqwest::Client::new();
    let user = unique();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let router = Router::new().fallback(move || async move {
        axum::Json(json!({ "players": [{ "id": user.to_string(), "xp": 999, "level": 1, "detailed_xp": [20, 155] }] }))
    });
    tokio::spawn(axum::serve(listener, router).into_future());
    let base = format!("http://{address}");
    let report = pinchora_commands::import_mee6(
        &sim.app,
        &base,
        pinchora_commands::ImportRequest {
            guild: sim.guild,
            actor: unique(),
            actor_name: "Manager",
            source: pinchora_db::AuditSource::Discord,
            mode: pinchora_commands::ImportMode::Merge,
        },
        |_| {},
    )
    .await
    .unwrap();
    assert_eq!(report.updated, 1);
    assert_eq!(sim.app.db.member_rank(sim.guild, user).await.unwrap().unwrap().xp, 120);
    assert_eq!(
        sim.app
            .db
            .import_member_xp_page(sim.guild, &[(user, 120)], false)
            .await
            .unwrap(),
        0,
        "replaying a page must not count unchanged XP as an update"
    );
    assert_eq!(
        sim.app
            .db
            .import_member_xp_page(sim.guild, &[(user, 50)], false)
            .await
            .unwrap(),
        0
    );
    assert_eq!(
        sim.app
            .db
            .import_member_xp_page(sim.guild, &[(user, 50)], true)
            .await
            .unwrap(),
        1
    );
    assert_eq!(sim.app.db.member_rank(sim.guild, user).await.unwrap().unwrap().xp, 50);
    assert_eq!(
        sim.app
            .db
            .import_member_xp_page(sim.guild, &[(user, 50)], true)
            .await
            .unwrap(),
        0
    );
    assert_eq!(
        sim.app.db.audit_page(sim.guild, None, 10).await.unwrap()[0].action,
        "xp.import.mee6"
    );
    let again = pinchora_commands::import_mee6(
        &sim.app,
        &base,
        pinchora_commands::ImportRequest {
            guild: sim.guild,
            actor: unique(),
            actor_name: "Manager",
            source: pinchora_db::AuditSource::Discord,
            mode: pinchora_commands::ImportMode::Merge,
        },
        |_| {},
    )
    .await;
    assert!(again.is_err(), "one import per guild per day");
    assert_eq!(sim.app.db.member_rank(sim.guild, user).await.unwrap().unwrap().xp, 50);
}

#[tokio::test]
async fn xp_import_waits_for_the_players_lock() {
    let Some(sim) = Sim::new("xp_import_waits_for_the_players_lock").await else {
        return;
    };
    let user = unique();
    let mut tx = sim.app.db.pool().begin().await.unwrap();
    sqlx::query("SELECT lock_user($1)")
        .bind(user as i64)
        .execute(&mut *tx)
        .await
        .unwrap();
    let db = sim.app.db.clone();
    let guild = sim.guild;
    let task = tokio::spawn(async move { db.import_member_xp_page(guild, &[(user, 321)], false).await });
    let mut task = Box::pin(task);
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(100), &mut task)
            .await
            .is_err()
    );
    tx.commit().await.unwrap();
    assert_eq!(task.await.unwrap().unwrap(), 1);
    assert_eq!(sim.app.db.member_rank(sim.guild, user).await.unwrap().unwrap().xp, 321);
}
