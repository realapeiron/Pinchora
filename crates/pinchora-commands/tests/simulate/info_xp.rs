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
    let stolen = sim.click(unique(), &visible, "Next").await;
    assert!(stolen.ephemeral && stolen.files.is_empty(), "{stolen}");
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
    let stolen = sim.click(unique(), &ask, "Confirm").await;
    assert!(stolen.ephemeral && stolen.content().contains("expired"), "{stolen}");
    let done = sim.click(actor, &ask, "Confirm").await;
    let replay = sim.click(actor, &ask, "Confirm").await;
    assert!(replay.content().contains("expired"), "{replay}");
    assert!(done.content().contains("0"), "{done}");
    assert_eq!(sim.app.db.member_rank(sim.guild, target).await.unwrap().unwrap().xp, 0);
    let all = sim.command(actor, "xp", json!([sub("reset-all", vec![])])).await;
    let forged = sim
        .component(actor, &format!("xi:all:2:{}:{actor}", sim.guild), &[], &all)
        .await;
    assert!(forged.ephemeral && forged.content().contains("expired"), "{forged}");
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
    let report = pinchora_commands::import_mee6_with_fetch(
        &sim.app,
        pinchora_commands::ImportRequest {
            guild: sim.guild,
            actor: unique(),
            actor_name: "Manager",
            source: pinchora_db::AuditSource::Discord,
            mode: pinchora_commands::ImportMode::Merge,
        },
        |_| {},
        |page| {
            let url = format!("{base}?page={page}");
            async move { Ok(reqwest::get(url).await?.bytes().await?.to_vec()) }
        },
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
    let again = pinchora_commands::import_mee6_with_fetch(
        &sim.app,
        pinchora_commands::ImportRequest {
            guild: sim.guild,
            actor: unique(),
            actor_name: "Manager",
            source: pinchora_db::AuditSource::Discord,
            mode: pinchora_commands::ImportMode::Merge,
        },
        |_| {},
        |page| {
            let url = format!("{base}?page={page}");
            async move { Ok(reqwest::get(url).await?.bytes().await?.to_vec()) }
        },
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

#[tokio::test]
async fn info_visibility_and_bilingual_cards() {
    let Some(mut sim) = Sim::new("info_visibility_and_bilingual_cards").await else {
        return;
    };
    let viewer = unique();
    let channel = unique();
    sim.mock.everyone_role.store(true, Ordering::SeqCst);
    sim.channel_is(channel, 0, None);
    let role_id = unique();
    let role = sim.guild_role(role_id, "Coral Crew · İpek <&>", 3);
    for locale in ["en-US", "tr"] {
        sim.locale = locale;
        let user = sim
            .command_resolved(viewer, "info", json!([sub("user", vec![])]), resolved(&[viewer], &[]))
            .await;
        assert_eq!(user.files.len(), 1, "{user}");
        for subcommand in ["server", "members", "emoji"] {
            let card = sim.command(viewer, "info", json!([sub(subcommand, vec![])])).await;
            assert_eq!(card.files.len(), 1, "{card}");
        }
        let card = sim
            .command_resolved(
                viewer,
                "info",
                json!([sub("role", vec![role_option("role", &role)])]),
                resolved(&[], &[&role]),
            )
            .await;
        assert_eq!(card.files.len(), 1, "{card}");
        let card = sim
            .command_resolved(
                viewer,
                "info",
                json!([sub("channel", vec![channel_option("channel", channel)])]),
                resolved_channels(&[channel]),
            )
            .await;
        assert_eq!(card.files.len(), 1, "{card}");
        assert!(card.ephemeral, "channel details must stay with their viewer");
    }
    sim.mock.channels.lock().unwrap().get_mut(&channel).unwrap()["permission_overwrites"] = json!([
        {"id": sim.guild.to_string(), "type": 0, "allow": "0", "deny": "1024"}
    ]);
    let hidden = sim
        .command_resolved(
            viewer,
            "info",
            json!([sub("channel", vec![channel_option("channel", channel)])]),
            resolved_channels(&[channel]),
        )
        .await;
    assert!(hidden.ephemeral && hidden.files.is_empty(), "{hidden}");
    sim.app
        .db
        .set_privacy(viewer, pinchora_db::Hide::Profile, true)
        .await
        .unwrap();
    let mine = sim
        .command_resolved(
            viewer,
            "info",
            json!([sub("user", vec![user_option("user", viewer)])]),
            resolved(&[viewer], &[]),
        )
        .await;
    let theirs = sim
        .command_resolved(
            unique(),
            "info",
            json!([sub("user", vec![user_option("user", viewer)])]),
            resolved(&[viewer], &[]),
        )
        .await;
    assert_eq!(
        mine.files[0].1, theirs.files[0].1,
        "private stats stay hidden even on the owner's public card"
    );
}

#[tokio::test]
async fn xp_bounds_roles_and_expired_confirmations() {
    let Some(mut sim) = Sim::new("xp_bounds_roles_and_expired_confirmations").await else {
        return;
    };
    sim.permissions = MANAGE_GUILD;
    let (actor, user, role) = (unique(), unique(), unique());
    sim.guild_role(role, "Level one", 2);
    sim.app.db.add_level_role(sim.guild, 1, role).await.unwrap();
    let added = sim
        .command_resolved(
            actor,
            "xp",
            json!([sub("set", vec![user_option("user", user), int_option("amount", 200)])]),
            resolved(&[user], &[]),
        )
        .await;
    assert!(added.content().contains("200"), "{added}");
    assert!(sim.mock.member_roles.lock().unwrap()[&user].contains(&role));
    let removed = sim
        .command_resolved(
            actor,
            "xp",
            json!([sub(
                "remove",
                vec![user_option("user", user), int_option("amount", 1_000_000_000)]
            )]),
            resolved(&[user], &[]),
        )
        .await;
    assert!(removed.content().contains("0"), "{removed}");
    assert!(!sim.mock.member_roles.lock().unwrap()[&user].contains(&role));
    for amount in [-1, 1_000_000_001, i64::MAX] {
        assert!(
            sim.app
                .db
                .edit_member_xp(sim.guild, user, pinchora_db::XpEdit::Add, amount)
                .await
                .is_err()
        );
    }
    sim.app
        .db
        .import_member_xp_page(sim.guild, &[(user, i64::MAX)], true)
        .await
        .unwrap();
    let (_, new) = sim
        .app
        .db
        .edit_member_xp(sim.guild, user, pinchora_db::XpEdit::Add, 1)
        .await
        .unwrap();
    assert_eq!(new, i64::MAX);
    let ask = sim.command(actor, "xp", json!([sub("reset-all", vec![])])).await;
    let id = ask.button("Confirm").custom_id.as_ref().unwrap();
    let nonce = id.rsplit(':').next().unwrap();
    sim.app
        .cache
        .forget(&format!("xp:confirm:{}:{actor}:{nonce}", sim.guild))
        .await
        .unwrap();
    let expired = sim.click(actor, &ask, "Confirm").await;
    assert!(expired.content().contains("expired"), "{expired}");
    assert_eq!(
        sim.app.db.member_rank(sim.guild, user).await.unwrap().unwrap().xp,
        i64::MAX
    );
}

#[tokio::test]
async fn mee6_rejects_oversized_pages_and_never_uses_live_http_in_tests() {
    let Some(sim) = Sim::new("mee6_rejects_oversized_pages_and_never_uses_live_http_in_tests").await else {
        return;
    };
    let request = || pinchora_commands::ImportRequest {
        guild: sim.guild,
        actor: unique(),
        actor_name: "Manager",
        source: pinchora_db::AuditSource::Discord,
        mode: pinchora_commands::ImportMode::Merge,
    };
    assert!(
        pinchora_commands::import_mee6(&sim.app, request(), |_| {})
            .await
            .is_err()
    );
    let result = pinchora_commands::import_mee6_with_fetch(
        &sim.app,
        request(),
        |_| {},
        |_| async { Ok(vec![b' '; 2 * 1024 * 1024 + 1]) },
    )
    .await;
    assert!(result.unwrap_err().to_string().contains("too large"));
    assert!(
        pinchora_commands::mee6_resting(&sim.app, sim.guild)
            .await
            .unwrap()
            .is_some()
    );
}

#[tokio::test]
async fn info_private_threads_apply_parent_permission_denials() {
    let Some(mut sim) = Sim::new("info_private_threads_apply_parent_permission_denials").await else {
        return;
    };
    let (viewer, role, parent, thread) = (unique(), unique(), unique(), unique());
    let manage_threads = twilight_model::guild::Permissions::MANAGE_THREADS.bits();
    sim.mock.everyone_role.store(true, Ordering::SeqCst);
    sim.guild_role_with(role, "Thread manager", 3, manage_threads);
    sim.mock.member_roles.lock().unwrap().insert(viewer, vec![role]);
    sim.channel_is(parent, 0, None);
    sim.channel_is(thread, 12, Some(parent));
    sim.mock.channels.lock().unwrap().get_mut(&parent).unwrap()["permission_overwrites"] = json!([
        {"id": role.to_string(), "type":0, "allow":"0", "deny":manage_threads.to_string()}
    ]);
    let denied = sim
        .command_resolved(
            viewer,
            "info",
            json!([sub("channel", vec![channel_option("channel", thread)])]),
            resolved_channels(&[thread]),
        )
        .await;
    assert!(
        denied.ephemeral && denied.files.is_empty() && denied.content().contains("unavailable"),
        "{denied}"
    );
}
