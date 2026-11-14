//! End-to-end member profile-code flow through mock Roblox and Discord HTTP.
use crate::*;
use std::time::Duration;

#[tokio::test]
async fn branding_roblox_profile_code_english_and_turkish() {
    let Some(mut sim) = Sim::new("roblox_profile_code_english_and_turkish").await else {
        return;
    };
    sim.app
        .db
        .save_guild_branding(sim.guild, &pinchora_db::GuildBranding::default())
        .await
        .unwrap();
    assert!(
        sim.app
            .db
            .claim_branding_apply(sim.guild, Some(0x31b8d4))
            .await
            .unwrap()
    );
    let description = Arc::new(Mutex::new(String::new()));
    let profile = description.clone();
    let banned = Arc::new(AtomicBool::new(false));
    let missing = Arc::new(AtomicBool::new(false));
    let name = Arc::new(Mutex::new("CrabPlayer".to_owned()));
    let profile_banned = banned.clone();
    let profile_missing = missing.clone();
    let profile_name = name.clone();
    let rank = Arc::new(AtomicU64::new(20));
    let group_rank = rank.clone();
    let roblox_id = unique();
    let app = Router::new()
        .route(
            "/v1/usernames/users",
            axum::routing::post(
                move || async move { axum::Json(json!({"data":[{"id":roblox_id,"name":"CrabPlayer"}]})) },
            ),
        )
        .route(
            "/v1/users/{id}",
            axum::routing::get(move |axum::extract::Path(id): axum::extract::Path<u64>| {
                let value = if id == roblox_id {
                    profile.lock().unwrap().clone()
                } else {
                    String::new()
                };
                let banned = profile_banned.load(Ordering::SeqCst);
                let missing = profile_missing.load(Ordering::SeqCst);
                let name = profile_name.lock().unwrap().clone();
                async move {
                    (
                        if missing { StatusCode::NOT_FOUND } else { StatusCode::OK },
                        axum::Json(json!({"id":id,"name":name,"description":value,"isBanned":banned})),
                    )
                }
            }),
        )
        .route(
            "/v2/users/{id}/groups/roles",
            axum::routing::get(move || {
                let rank = group_rank.load(Ordering::SeqCst);
                async move { axum::Json(json!({"data":[{"group":{"id":42},"role":{"rank":rank}}]})) }
            }),
        );
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    pinchora_commands::integrations::roblox::install_test_endpoint(Some(&endpoint));
    let user = unique();
    let start = sim
        .command(
            user,
            "roblox",
            json!([sub("verify", vec![text_option("username", "CrabPlayer")])]),
        )
        .await;
    assert!(
        start.ephemeral && start.content().contains("About"),
        "{}",
        start.content()
    );
    let pending = sim.app.db.roblox_pending(user).await.unwrap().unwrap();
    assert_eq!(
        pending.code.rsplit('-').next().unwrap().len(),
        32,
        "128-bit random challenge"
    );
    *description.lock().unwrap() = format!("my beach profile {}", pending.code);
    // Copying a code to/from a different profile cannot prove the selected numeric identity.
    let attacker = unique();
    sim.app
        .db
        .roblox_start(attacker, roblox_id + 1, "OtherPlayer", &pending.code)
        .await
        .unwrap();
    let copied = sim.click(attacker, &start, "Check profile").await;
    assert!(copied.content().contains("couldn't find"), "{}", copied.content());
    assert!(sim.app.db.roblox_link(attacker).await.unwrap().is_none());
    let repeated = sim
        .command(
            user,
            "roblox",
            json!([sub("verify", vec![text_option("username", "CrabPlayer")])]),
        )
        .await;
    assert!(repeated.content().contains("60 seconds"), "{}", repeated.content());
    assert_eq!(
        sim.app.db.roblox_pending(user).await.unwrap().unwrap().code,
        pending.code
    );
    let verified = sim.click(user, &start, "Check profile").await;
    assert!(
        verified.ephemeral && verified.content().contains("linked"),
        "{}",
        verified.content()
    );
    assert_eq!(
        sim.app.db.roblox_link(user).await.unwrap().unwrap().roblox_id,
        roblox_id as i64
    );
    // Completion is single-use, and removing the proof after linking is allowed.
    let reused = sim.click(user, &start, "Check profile").await;
    assert!(reused.content().contains("expired"), "{}", reused.content());
    *description.lock().unwrap() = String::new();
    *name.lock().unwrap() = "RenamedCrab".to_owned();
    let whois = sim.command(user, "roblox", json!([sub("whois", vec![])])).await;
    assert!(whois.ephemeral);
    assert_eq!(whois.embeds[0].color, Some(0x31b8d4));
    assert_eq!(
        sim.app.db.roblox_link(user).await.unwrap().unwrap().roblox_name,
        "RenamedCrab"
    );
    sim.app
        .db
        .set_privacy(user, pinchora_db::Hide::Profile, true)
        .await
        .unwrap();
    let hidden = sim
        .command_resolved(
            attacker,
            "roblox",
            json!([sub(
                "whois",
                vec![json!({"type":6,"name":"user","value":user.to_string()})]
            )]),
            json!({"users":{user.to_string():user_json(user)}}),
        )
        .await;
    assert!(hidden.content().contains("private"), "{}", hidden.content());
    let denied_update = sim
        .command_resolved(
            attacker,
            "roblox",
            json!([sub(
                "update",
                vec![json!({"type":6,"name":"user","value":user.to_string()})]
            )]),
            json!({"users":{user.to_string():user_json(user)}}),
        )
        .await;
    assert!(
        denied_update.content().contains("Manage Server"),
        "{}",
        denied_update.content()
    );

    let verified_role = unique();
    let retained_role = unique();
    let ranked_role = unique();
    let admin_role = unique();
    let high_role = unique();
    let managed_role = unique();
    for role in [verified_role, retained_role, ranked_role] {
        sim.guild_role(role, "Safe", 1);
    }
    sim.guild_role_with(admin_role, "Admin", 1, 1 << 3);
    sim.guild_role(high_role, "Above bot", BOT_ROLE_POSITION + 1);
    let mut managed = role_json(managed_role, "Managed", 1, true);
    managed["managed"] = json!(true);
    sim.mock.roles.lock().unwrap().push(managed);
    sim.mock.everyone_role.store(true, Ordering::SeqCst);
    sim.app
        .db
        .roblox_save_settings(
            sim.guild,
            &pinchora_db::RobloxSettings {
                enabled: true,
                verified_role_id: Some(verified_role as i64),
                nickname_template: Some("{roblox-name}".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    sim.app
        .db
        .roblox_add_bind(sim.guild, "verified", None, None, None, retained_role, false)
        .await
        .unwrap();
    sim.app
        .db
        .roblox_add_bind(sim.guild, "group_rank", Some(42), Some(10), Some(30), ranked_role, true)
        .await
        .unwrap();
    for role in [admin_role, high_role, managed_role, sim.guild] {
        sim.app
            .db
            .roblox_add_bind(sim.guild, "verified", None, None, None, role, true)
            .await
            .unwrap();
    }
    // The owner must never be renamed; unsafe roles are never granted.
    sim.own_server(user);
    sim.mock.member_roles.lock().unwrap().insert(user, vec![admin_role]);
    assert!(
        pinchora_commands::integrations::roblox::sync(&sim.app, sim.guild, user)
            .await
            .unwrap()
    );
    let changes = sim.role_changes();
    assert!(
        changes.contains(&format!("DELETE {user} {admin_role}")),
        "roles that become privileged must be removed"
    );
    for role in [verified_role, retained_role, ranked_role] {
        assert!(changes.contains(&format!("PUT {user} {role}")), "{changes:?}");
    }
    for role in [admin_role, high_role, managed_role, sim.guild] {
        assert!(!changes.contains(&format!("PUT {user} {role}")), "{changes:?}");
    }
    assert!(
        !sim.mock
            .mod_calls
            .lock()
            .unwrap()
            .iter()
            .any(|call| call.starts_with(&format!("member {user} ")))
    );
    rank.store(5, Ordering::SeqCst);
    sqlx::query("DELETE FROM roblox_cache WHERE key=$1")
        .bind(format!("groups:{roblox_id}"))
        .execute(sim.app.db.pool())
        .await
        .unwrap();
    assert!(
        pinchora_commands::integrations::roblox::sync(&sim.app, sim.guild, user)
            .await
            .unwrap()
    );
    assert!(sim.role_changes().contains(&format!("DELETE {user} {ranked_role}")));
    banned.store(true, Ordering::SeqCst);
    assert!(
        !pinchora_commands::integrations::roblox::sync(&sim.app, sim.guild, user)
            .await
            .unwrap()
    );
    let changes = sim.role_changes();
    assert!(changes.contains(&format!("DELETE {user} {verified_role}")));
    assert!(changes.contains(&format!("DELETE {user} {retained_role}")));
    banned.store(false, Ordering::SeqCst);
    assert!(
        pinchora_commands::integrations::roblox::sync(&sim.app, sim.guild, user)
            .await
            .unwrap()
    );
    missing.store(true, Ordering::SeqCst);
    assert!(
        !pinchora_commands::integrations::roblox::sync(&sim.app, sim.guild, user)
            .await
            .unwrap()
    );
    assert!(sim.role_changes().contains(&format!("DELETE {user} {verified_role}")));
    missing.store(false, Ordering::SeqCst);
    assert!(
        pinchora_commands::integrations::roblox::sync(&sim.app, sim.guild, user)
            .await
            .unwrap()
    );
    sim.locale = "tr";
    let gone = sim.command(user, "roblox", json!([sub("unverify", vec![])])).await;
    assert!(
        gone.ephemeral && gone.content().contains("kaldırıldı"),
        "{}",
        gone.content()
    );
    assert!(sim.app.db.roblox_link(user).await.unwrap().is_none());
    let mut disabled = sim.app.db.roblox_settings(sim.guild).await.unwrap();
    disabled.enabled = false;
    sim.app.db.roblox_save_settings(sim.guild, &disabled).await.unwrap();
    assert!(
        !pinchora_commands::integrations::roblox::sync(&sim.app, sim.guild, user)
            .await
            .unwrap()
    );
    let changes = sim.role_changes();
    assert!(
        changes.contains(&format!("DELETE {user} {retained_role}")),
        "unverify overrides retain-on-rank-change"
    );
    // Two pods cannot own a scheduled batch or synchronize the same member concurrently.
    let scope = format!("roblox:tick:{}", sim.app.application_id);
    let lease = sim
        .app
        .cache
        .verification_lease(&scope, "test-pod")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        pinchora_commands::integrations::roblox::tick(&sim.app).await.unwrap(),
        0
    );
    lease.release().await.unwrap();
    let key = format!("roblox:user:{user}");
    let _lease = sim
        .app
        .cache
        .verification_lease(&key, "test-pod")
        .await
        .unwrap()
        .unwrap();
    assert!(
        pinchora_commands::integrations::roblox::sync(&sim.app, sim.guild, user)
            .await
            .is_err()
    );
    pinchora_commands::integrations::roblox::install_test_endpoint(None);
}

#[tokio::test]
async fn privacy_removes_roblox_roles_before_forgetting_the_link() {
    let Some(mut sim) = Sim::new("roblox_privacy_roles").await else {
        return;
    };
    let (user, role) = (unique(), unique());
    sim.guild_role(role, "Linked", 1);
    sim.app
        .db
        .roblox_save_settings(
            sim.guild,
            &pinchora_db::RobloxSettings {
                enabled: false,
                verified_role_id: Some(role as i64),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    sim.app.db.roblox_queue(sim.guild, user).await.unwrap();
    sim.app.db.roblox_start(user, unique(), "Crab", "proof").await.unwrap();
    let pending = sim.app.db.roblox_pending(user).await.unwrap().unwrap();
    assert!(sim.app.db.roblox_complete(user, &pending).await.unwrap());
    sim.mock.member_roles.lock().unwrap().insert(user, vec![role]);
    sim.allow_role_changes(false);
    let refused = delete_through_the_panel(&mut sim, user).await;
    assert!(refused.content().contains("No data was deleted"), "{refused}");
    assert!(sim.app.db.roblox_link(user).await.unwrap().is_some());
    sim.allow_role_changes(true);
    let done = delete_through_the_panel(&mut sim, user).await;
    assert_eq!(done.title(), "Your data is deleted");
    assert!(sim.role_changes().contains(&format!("DELETE {user} {role}")));
    assert!(sim.app.db.roblox_link(user).await.unwrap().is_none());
    assert!(sim.app.db.roblox_user_guilds(user).await.unwrap().is_empty());
}

#[tokio::test]
async fn roblox_admin_rejects_overflowed_optional_ranks_and_join_prompts_are_bounded() {
    let Some(mut sim) = Sim::new("roblox_validation_and_join").await else {
        return;
    };
    let user = unique();
    let role = sim.guild_role(unique(), "Safe", 1);
    sim.permissions = 1 << 5;
    sim.own_server(user);
    for kind in ["verified", "group_member"] {
        let mut options = vec![
            text_option("kind", kind),
            role_option("role", &role),
            int_option("rank_min", 65_536),
        ];
        if kind == "group_member" {
            options.push(int_option("group", 42));
        }
        let reply = sim
            .legacy_command_resolved(
                user,
                "robloxadmin",
                json!([sub("bind-add", options)]),
                resolved(&[], &[&role]),
            )
            .await;
        assert!(reply.content().contains("rank range"), "{}", reply.content());
    }
    assert!(sim.app.db.roblox_binds(sim.guild).await.unwrap().is_empty());
    for (channel_guild, kind) in [(unique(), 0), (sim.guild, 2)] {
        let channel = unique();
        sim.mock.channels.lock().unwrap().insert(
            channel,
            json!({"id":channel.to_string(),"guild_id":channel_guild.to_string(),"type":kind,"name":"verification"}),
        );
        let reply = sim
            .legacy_command(
                user,
                "robloxadmin",
                json!([sub(
                    "settings",
                    vec![json!({"type":7,"name":"verify_channel","value":channel.to_string()})]
                )]),
            )
            .await;
        assert!(reply.content().contains("text channel"), "{}", reply.content());
        assert!(
            sim.app
                .db
                .roblox_settings(sim.guild)
                .await
                .unwrap()
                .verify_channel_id
                .is_none()
        );
    }
    for _ in 0..25 {
        sim.app
            .db
            .roblox_add_bind(
                sim.guild,
                "group_rank",
                Some(i64::MAX as u64),
                Some(1),
                Some(255),
                i64::MAX as u64,
                true,
            )
            .await
            .unwrap();
    }
    for locale in ["en-US", "tr"] {
        sim.locale = locale;
        let reply = sim
            .legacy_command(user, "robloxadmin", json!([sub("bind-list", vec![])]))
            .await;
        assert!(reply.ephemeral);
        assert_eq!(reply.description().lines().count(), 25);
        assert!(reply.description().chars().count() <= 4096);
    }
    sim.app
        .db
        .roblox_save_settings(
            sim.guild,
            &pinchora_db::RobloxSettings {
                enabled: true,
                require_verified: true,
                verify_channel_id: Some(sim.channel as i64),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert!(pinchora_commands::integrations::roblox::on_join(&sim.app, sim.guild, user, false).await);
    assert!(pinchora_commands::integrations::roblox::on_join(&sim.app, sim.guild, user, false).await);
    assert!(pinchora_commands::integrations::roblox::on_join(&sim.app, sim.guild, unique(), false).await);
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(sim.posts().len(), 1, "replayed and burst joins must not spam prompts");
}

#[tokio::test]
async fn roblox_lease_release_is_awaited_and_preserves_a_replacement_owner() {
    let Some(sim) = Sim::new("roblox_lease_release").await else {
        return;
    };
    let key = format!("roblox:user:{}", unique());
    let lease = sim.app.cache.verification_lease(&key, "first").await.unwrap().unwrap();
    assert!(
        sim.app
            .cache
            .verification_lease(&key, "contender")
            .await
            .unwrap()
            .is_none()
    );
    lease.release().await.unwrap();
    let old = sim
        .app
        .cache
        .verification_lease(&key, "old")
        .await
        .unwrap()
        .expect("release completed");
    // Simulate expiry and another pod replacing the owner before the old task finishes.
    let redis_key = format!("{}:vf:lease:{key}", sim.app.cache.namespace());
    let _: () = redis::cmd("SET")
        .arg(&redis_key)
        .arg("replacement")
        .arg("EX")
        .arg(180)
        .query_async(&mut sim.app.cache.conn())
        .await
        .unwrap();
    old.release().await.unwrap();
    assert!(
        sim.app
            .cache
            .verification_lease(&key, "contender")
            .await
            .unwrap()
            .is_none()
    );
    let _: () = redis::cmd("DEL")
        .arg(&redis_key)
        .query_async(&mut sim.app.cache.conn())
        .await
        .unwrap();
}
