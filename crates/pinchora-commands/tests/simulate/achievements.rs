//! The manager flow and public paged card through the real handler.

use super::*;

#[tokio::test]
async fn achievements_are_managed_granted_once_and_paged() {
    let Some(mut sim) = Sim::new("achievements_are_managed_granted_once_and_paged").await else {
        return;
    };
    let (manager, member) = (unique(), unique());
    sim.permissions = MANAGE_GUILD;
    for n in 0..6 {
        let name = format!("Pearl {n}");
        let saved = sim
            .command(
                manager,
                "achievementadmin",
                json!([sub(
                    "add",
                    vec![
                        text_option("name", &name),
                        text_option("kind", "custom"),
                        int_option("target", 1),
                        int_option("points", 25),
                    ]
                )]),
            )
            .await;
        assert!(
            saved.ephemeral && saved.content().contains("saved"),
            "{}",
            saved.content()
        );
    }
    let rows = sim.app.db.achievements(sim.guild).await.unwrap();
    let first = rows[0].id;
    let granted = sim
        .command_resolved(
            manager,
            "achievementadmin",
            json!([sub(
                "grant",
                vec![int_option("id", first), user_option("user", member),]
            )]),
            resolved(&[member], &[]),
        )
        .await;
    assert!(granted.ephemeral && granted.content().contains("Granted"));
    assert_eq!(sim.app.db.member_points(sim.guild, member).await.unwrap().0, 25);
    let again = sim
        .command_resolved(
            manager,
            "achievementadmin",
            json!([sub(
                "grant",
                vec![int_option("id", first), user_option("user", member),]
            )]),
            resolved(&[member], &[]),
        )
        .await;
    assert!(again.content().contains("already earned"));
    assert_eq!(sim.app.db.member_points(sim.guild, member).await.unwrap().0, 25);

    sim.permissions = 0;
    let card = sim
        .command_resolved(
            member,
            "achievements",
            json!([user_option("user", member)]),
            resolved(&[member], &[]),
        )
        .await;
    assert!(
        !card.ephemeral
            && card
                .files
                .iter()
                .any(|(name, bytes)| name == "achievements.png" && bytes.len() > 1000)
    );
    assert_eq!(card.rows(), 1);
    let next = sim.click(member, &card, "Next").await;
    assert!(next.files.iter().any(|(name, _)| name == "achievements.png"));
    assert!(next.button("Next").disabled);
    for options in [
        json!([sub(
            "add",
            vec![
                text_option("name", "No"),
                text_option("kind", "custom"),
                int_option("target", 1)
            ]
        )]),
        json!([sub("edit", vec![int_option("id", first), text_option("name", "No")])]),
        json!([sub("remove", vec![int_option("id", first)])]),
        json!([sub("list", vec![])]),
        json!([sub("grant", vec![int_option("id", first), user_option("user", member)])]),
    ] {
        let denied = sim
            .command_resolved(member, "achievementadmin", options, resolved(&[member], &[]))
            .await;
        assert!(denied.ephemeral && denied.content().contains("Manage Server"));
    }
}

async fn achievement(
    sim: &Sim,
    name: &str,
    kind: pinchora_core::achievements::Kind,
    role: Option<u64>,
    secret: bool,
) -> i64 {
    let result = sim
        .app
        .db
        .save_achievement(
            sim.guild,
            None,
            &pinchora_db::AchievementDraft {
                name,
                description: "Milestone",
                icon: "star",
                kind,
                target: 1,
                reward_role_id: role,
                reward_points: 5,
                announce_channel_id: Some(sim.channel),
                secret,
                enabled: true,
            },
        )
        .await
        .unwrap();
    pinchora_commands::invalidate_achievements(&sim.app, sim.guild)
        .await
        .unwrap();
    let pinchora_db::AchievementSave::Saved(id) = result else {
        panic!()
    };
    id
}

#[tokio::test]
async fn achievement_races_grant_roles_once_and_batch_retroactive_announcements() {
    let Some(sim) = Sim::new("achievement_races_grant_roles_once_and_batch_retroactive_announcements").await else {
        return;
    };
    let user = unique();
    let role_id = unique();
    sim.guild_role(role_id, "Prize", 2);
    sim.app.db.add_message_xp(sim.guild, user, 300, 0).await.unwrap();
    achievement(
        &sim,
        "Level one",
        pinchora_core::achievements::Kind::Level,
        Some(role_id),
        false,
    )
    .await;
    achievement(
        &sim,
        "Another level",
        pinchora_core::achievements::Kind::Level,
        None,
        false,
    )
    .await;
    let (a, b) = tokio::join!(
        pinchora_commands::evaluate_achievements(&sim.app, sim.guild, user),
        pinchora_commands::evaluate_achievements(&sim.app, sim.guild, user)
    );
    a.unwrap();
    b.unwrap();
    assert_eq!(sim.app.db.earned_achievements(sim.guild, user).await.unwrap().len(), 2);
    assert_eq!(sim.app.db.member_points(sim.guild, user).await.unwrap().0, 10);
    assert_eq!(sim.role_changes().iter().filter(|c| c.starts_with("PUT")).count(), 1);
    assert_eq!(
        sim.mock.posts.lock().unwrap().len(),
        1,
        "one summary for a retroactive batch"
    );
    // A second member's retroactive awards in the same minute must not flood the channel.
    let other = unique();
    sim.app.db.add_message_xp(sim.guild, other, 300, 0).await.unwrap();
    pinchora_commands::evaluate_achievements(&sim.app, sim.guild, other)
        .await
        .unwrap();
    assert_eq!(sim.mock.posts.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn achievement_role_safety_is_rechecked_after_configuration() {
    let Some(sim) = Sim::new("achievement_role_safety_is_rechecked_after_configuration").await else {
        return;
    };
    let (user, role_id) = (unique(), unique());
    sim.guild_role(role_id, "Originally safe", 2);
    achievement(
        &sim,
        "Unsafe now",
        pinchora_core::achievements::Kind::Level,
        Some(role_id),
        false,
    )
    .await;
    let role_key = role_id.to_string();
    sim.mock
        .roles
        .lock()
        .unwrap()
        .iter_mut()
        .find(|r| r["id"].as_str() == Some(role_key.as_str()))
        .unwrap()["permissions"] = json!((1u64 << 3).to_string());
    sim.app.db.add_message_xp(sim.guild, user, 300, 0).await.unwrap();
    pinchora_commands::evaluate_achievements(&sim.app, sim.guild, user)
        .await
        .unwrap();
    assert!(
        sim.role_changes().is_empty(),
        "an edited admin role is never handed out"
    );
    assert_eq!(sim.app.db.member_points(sim.guild, user).await.unwrap().0, 5);
}

#[tokio::test]
async fn achievement_secret_privacy_and_server_language_survive_paging() {
    let Some(mut sim) = Sim::new("achievement_secret_privacy_and_server_language_survive_paging").await else {
        return;
    };
    let (user, stranger) = (unique(), unique());
    for n in 0..5 {
        achievement(
            &sim,
            &format!("Visible {n}"),
            pinchora_core::achievements::Kind::Custom,
            None,
            false,
        )
        .await;
    }
    let secret = achievement(&sim, "Secret", pinchora_core::achievements::Kind::Custom, None, true).await;
    let english = sim.command(user, "achievements", json!([])).await;
    assert_eq!(english.rows(), 0, "unearned secret does not create a second page");
    sim.app
        .db
        .grant_achievement(sim.guild, user, secret, pinchora_core::achievements::Kind::Custom)
        .await
        .unwrap();
    turkish_server(&sim).await;
    let turkish = sim.command(user, "achievements", json!([])).await;
    assert_eq!(turkish.rows(), 1);
    assert!(
        turkish.button("Sonraki").custom_id.is_some(),
        "public card follows Turkish server, not English invoker"
    );
    assert_ne!(english.files[0].1, turkish.files[0].1);
    sim.app
        .db
        .set_privacy(user, pinchora_db::Hide::Profile, true)
        .await
        .unwrap();
    let denied = sim
        .command_resolved(
            stranger,
            "achievements",
            json!([user_option("user", user)]),
            resolved(&[user], &[]),
        )
        .await;
    assert!(denied.ephemeral && denied.files.is_empty());
    let denied = sim.component(stranger, &format!("ah:{user}:1"), &[], &turkish).await;
    assert!(denied.ephemeral && denied.files.is_empty());
}

#[tokio::test]
async fn achievement_empty_guild_and_disabled_rules_do_not_spend_a_tick() {
    let Some(sim) = Sim::new("achievement_empty_guild_and_disabled_rules_do_not_spend_a_tick").await else {
        return;
    };
    let user = unique();
    pinchora_commands::achievement_tick(&sim.app, sim.guild, user).await;
    let key = format!("ah:tick:{}:{user}", sim.guild);
    assert!(
        sim.app
            .cache
            .try_cooldown(&key, std::time::Duration::from_secs(60))
            .await
            .unwrap()
            .is_none()
    );
    sim.app.cache.release_cooldown(&key).await.unwrap();
    achievement(
        &sim,
        "Manual only",
        pinchora_core::achievements::Kind::Custom,
        None,
        false,
    )
    .await;
    pinchora_commands::achievement_tick(&sim.app, sim.guild, user).await;
    assert!(
        sim.app
            .cache
            .try_cooldown(&key, std::time::Duration::from_secs(60))
            .await
            .unwrap()
            .is_none()
    );
    assert!(sim.role_changes().is_empty());
    assert!(sim.mock.posts.lock().unwrap().is_empty());
}

#[tokio::test]
async fn achievement_tick_budget_and_bot_recipient_fail_closed() {
    let Some(sim) = Sim::new("achievement_tick_budget_and_bot_recipient_fail_closed").await else {
        return;
    };
    let user = unique();
    achievement(&sim, "Bounded", pinchora_core::achievements::Kind::Level, None, false).await;
    sim.app.db.add_message_xp(sim.guild, user, 300, 0).await.unwrap();
    let minute = sim.app.now().timestamp().div_euclid(60);
    sim.app
        .cache
        .incr_by(
            &format!("ah:budget:{}:{minute}", sim.guild),
            100,
            std::time::Duration::from_secs(120),
        )
        .await
        .unwrap();
    pinchora_commands::achievement_tick(&sim.app, sim.guild, user).await;
    assert!(
        sim.app
            .db
            .earned_achievements(sim.guild, user)
            .await
            .unwrap()
            .is_empty()
    );
    sim.app
        .db
        .add_message_xp(sim.guild, APPLICATION_ID, 300, 0)
        .await
        .unwrap();
    pinchora_commands::evaluate_achievements(&sim.app, sim.guild, APPLICATION_ID)
        .await
        .unwrap();
    assert!(
        sim.app
            .db
            .earned_achievements(sim.guild, APPLICATION_ID)
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn achievement_voice_tick_evaluates_without_a_level_up() {
    let Some(mut sim) = Sim::new("achievement_voice_tick_evaluates_without_a_level_up").await else {
        return;
    };
    sim.app.shift_clock(chrono::Duration::days((unique() % 36_500) as i64));
    let (manager, user, companion, channel) = (unique(), unique(), unique(), unique());
    sim.permissions = MANAGE_GUILD;
    sim.xp(
        manager,
        sub("voice", vec![bool_option("enabled", true), int_option("amount", 20)]),
    )
    .await;
    sim.app.db.add_message_xp(sim.guild, user, 100, 0).await.unwrap();
    achievement(
        &sim,
        "Voice visitor",
        pinchora_core::achievements::Kind::Level,
        None,
        false,
    )
    .await;
    sim.voice_in(user, Some(channel), &[], false, false, false).await;
    sim.voice_in(companion, Some(channel), &[], false, false, false).await;
    sim.app.shift_clock(chrono::Duration::seconds(60));
    pinchora_commands::voice_tick(&sim.app).await.unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        while sim
            .app
            .db
            .earned_achievements(sim.guild, user)
            .await
            .unwrap()
            .is_empty()
        {
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("voice tick evaluates without waiting for another level or message");
    assert_eq!(sim.server_xp(user).await, 120);
}

#[tokio::test]
async fn achievement_source_module_and_feature_gate_block_rewards_and_cards() {
    let Some(mut sim) = Sim::new("achievement_source_module_and_feature_gate_block_rewards_and_cards").await else {
        return;
    };
    let user = unique();
    achievement(
        &sim,
        "Hidden while off",
        pinchora_core::achievements::Kind::Level,
        None,
        false,
    )
    .await;
    sim.app.db.add_message_xp(sim.guild, user, 300, 0).await.unwrap();
    let scoped = unique();
    sim.app.db.add_message_xp(sim.guild, scoped, 300, 0).await.unwrap();
    sim.app
        .db
        .set_module(
            sim.guild,
            pinchora_core::modules::Module::Leveling,
            Some(true),
            Some((pinchora_core::modules::ChannelMode::Only, &[sim.channel])),
        )
        .await
        .unwrap();
    sim.app.forget_guild_settings(sim.guild).await;
    pinchora_commands::evaluate_achievements(&sim.app, sim.guild, scoped)
        .await
        .unwrap();
    assert_eq!(
        sim.app.db.earned_achievements(sim.guild, scoped).await.unwrap().len(),
        1,
        "channel restrictions must not hide an enabled source's server total"
    );
    sim.app
        .db
        .set_module(sim.guild, pinchora_core::modules::Module::Leveling, Some(false), None)
        .await
        .unwrap();
    sim.app.forget_guild_settings(sim.guild).await;
    pinchora_commands::evaluate_achievements(&sim.app, sim.guild, user)
        .await
        .unwrap();
    assert!(
        sim.app
            .db
            .earned_achievements(sim.guild, user)
            .await
            .unwrap()
            .is_empty()
    );
    sim.app
        .db
        .set_module(
            sim.guild,
            pinchora_core::modules::Module::Achievements,
            Some(false),
            None,
        )
        .await
        .unwrap();
    sim.app.forget_guild_settings(sim.guild).await;
    let denied = sim.command(user, "achievements", json!([])).await;
    assert!(denied.ephemeral && denied.files.is_empty());
    let denied = sim.component(user, &format!("ah:{user}:0"), &[], &denied).await;
    assert!(denied.ephemeral && denied.files.is_empty());
}

#[tokio::test]
async fn achievement_secret_edit_is_private_even_without_cache_notification() {
    let Some(mut sim) = Sim::new("achievement_secret_edit_is_private_even_without_cache_notification").await else {
        return;
    };
    let user = unique();
    let mut last = 0;
    for n in 0..6 {
        last = achievement(
            &sim,
            &format!("Rule {n}"),
            pinchora_core::achievements::Kind::Custom,
            None,
            false,
        )
        .await;
    }
    // Prime the evaluation cache on this pod, then miss the other pod's edit notification.
    pinchora_commands::achievement_tick(&sim.app, sim.guild, user).await;
    let before = sim.command(user, "achievements", json!([])).await;
    assert_eq!(before.rows(), 1);
    sqlx::query("UPDATE server_achievements SET secret=true WHERE id=$1")
        .bind(last)
        .execute(sim.app.db.pool())
        .await
        .unwrap();
    let after = sim.command(user, "achievements", json!([])).await;
    assert_eq!(after.rows(), 0, "newly secret unearned rule must disappear immediately");
}
