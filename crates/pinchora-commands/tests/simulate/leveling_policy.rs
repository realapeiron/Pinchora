use crate::*;
use pinchora_core::progression::leveling::week_start;
use std::time::Duration;

#[tokio::test]
async fn curve_tuning_and_voice_rank_work_in_english_and_turkish() {
    let Some(mut sim) = Sim::new("leveling_parity_curve_voice_rank").await else {
        return;
    };
    sim.permissions = MANAGE_GUILD;
    let (admin, member, channel) = (unique(), unique(), unique());
    let response = sim
        .xp(
            admin,
            sub(
                "curve",
                vec![
                    text_option("kind", "linear"),
                    int_option("base", 100),
                    int_option("linear", 20),
                ],
            ),
        )
        .await;
    assert!(response.ephemeral && response.content().contains("saved"), "{response}");
    let response = sim
        .xp(
            admin,
            sub(
                "tuning",
                vec![int_option("min", 40), int_option("max", 40), int_option("cooldown", 5)],
            ),
        )
        .await;
    assert!(response.ephemeral && response.content().contains("saved"), "{response}");
    sim.say_in(member, channel, &[]).await;
    assert_eq!(sim.server_xp(member).await, 40);
    assert_eq!(sim.app.db.leveling_policy(sim.guild).await.unwrap().xp_to_next(2), 140);
    sim.locale = "tr";
    let response = sim
        .xp(
            admin,
            sub(
                "sources",
                vec![
                    bool_option("reactions", true),
                    bool_option("separate_voice_level", true),
                ],
            ),
        )
        .await;
    assert!(
        response.ephemeral && response.content().contains("kaydedildi"),
        "{response}"
    );
    let gain = sim
        .app
        .db
        .add_separate_voice_xp(sim.guild, &[(member, 150)], week_start(sim.app.now()))
        .await
        .unwrap();
    assert_eq!(gain[0].1.new_xp, 150);
    assert_eq!(
        sim.server_xp(member).await,
        40,
        "separate voice XP leaves chat XP alone"
    );
    let rank = sim.command(member, "rank", json!([bool_option("voice", true)])).await;
    assert_eq!(rank.files.len(), 1, "{rank}");
    let board = sim
        .command(member, "leaderboard", json!([text_option("board", "voice")]))
        .await;
    assert_eq!(board.files.len(), 1, "{board}");
}

#[tokio::test]
async fn reaction_xp_rejects_self_bots_and_repeated_reactions() {
    use pinchora_commands::{ReactionEvent, on_reaction};
    let Some(sim) = Sim::new("reaction_xp_abuse").await else {
        return;
    };
    let (user, other, channel) = (unique(), unique(), unique());
    let policy = pinchora_core::progression::leveling_policy::LevelingPolicy {
        reactions: true,
        ..Default::default()
    };
    sim.app.db.save_leveling_policy(sim.guild, &policy).await.unwrap();
    for (author, bot, reactor_bot, expected) in [
        (user, false, false, 0),
        (other, true, false, 0),
        (other, false, true, 0),
        (other, false, false, 5),
    ] {
        let mut message = serde_json::to_value(sim.chat_in(author, channel, &[])).unwrap();
        message["author"]["bot"] = json!(bot);
        let id = message["id"].clone();
        sim.mock.history.lock().unwrap().insert(channel, vec![message]);
        let mut member = member_json(user, &[]);
        member["user"]["bot"] = json!(reactor_bot);
        let added = json!({"guild_id":sim.guild.to_string(),"channel_id":channel.to_string(),"message_id":id,"user_id":user.to_string(),"member":member,"emoji":{"id":null,"name":"⭐"},"burst":false,"burst_colors":[],"type":0});
        on_reaction(
            sim.app.clone(),
            ReactionEvent::Add(Box::new(serde_json::from_value(added.clone()).unwrap())),
        )
        .await
        .unwrap();
        sim.app.drain(Duration::from_secs(10)).await;
        assert_eq!(sim.server_xp(user).await, expected);
        if expected > 0 {
            // Redis may expire or be evicted; a remove/re-add still cannot earn on this message.
            sim.app
                .cache
                .release_cooldown(&format!("xp:reaction:minute:{}:{user}", sim.guild))
                .await
                .unwrap();
            on_reaction(
                sim.app.clone(),
                ReactionEvent::Add(Box::new(serde_json::from_value(added).unwrap())),
            )
            .await
            .unwrap();
            sim.app.drain(Duration::from_secs(10)).await;
            assert_eq!(sim.server_xp(user).await, 5);
        }
    }
}

#[tokio::test]
async fn message_cooldown_is_shared_by_pods_and_curve_changes_are_immediate() {
    let Some(sim) = Sim::new("leveling_cooldown_pod_a").await else {
        return;
    };
    let Some(mut other) = Sim::new("leveling_cooldown_pod_b").await else {
        return;
    };
    other.guild = sim.guild;
    let (user, channel) = (unique(), unique());
    assert_eq!(other.app.leveling_policy(sim.guild).await.unwrap().message_min, 15);
    let policy = pinchora_core::progression::leveling_policy::LevelingPolicy {
        message_min: 40,
        message_max: 40,
        curve: pinchora_core::progression::leveling_policy::Curve::Linear { base: 10, per_level: 0 },
        ..Default::default()
    };
    sim.app.db.save_leveling_policy(sim.guild, &policy).await.unwrap();
    assert_eq!(*other.app.leveling_policy(sim.guild).await.unwrap(), policy);
    let (a, b) = tokio::join!(
        on_message(sim.app.clone(), sim.chat_in(user, channel, &[])),
        on_message(other.app.clone(), other.chat_in(user, channel, &[]))
    );
    a.unwrap();
    b.unwrap();
    assert_eq!(sim.server_xp(user).await, 40);
    assert_eq!(other.app.leveling_policy(sim.guild).await.unwrap().level_of(40), 4);
}

#[tokio::test]
async fn separate_voice_rewards_do_not_regrant_from_stale_voice_state() {
    let Some(mut sim) = Sim::new("voice_reward_stale_roles").await else {
        return;
    };
    sim.permissions = MANAGE_GUILD;
    let (admin, user, other, channel, role) = (unique(), unique(), unique(), unique(), unique());
    sim.guild_role(role, "Voice", 2);
    sim.xp(
        admin,
        sub("voice", vec![bool_option("enabled", true), int_option("amount", 100)]),
    )
    .await;
    let policy = pinchora_core::progression::leveling_policy::LevelingPolicy {
        separate_voice_level: true,
        curve: pinchora_core::progression::leveling_policy::Curve::Linear {
            base: 100,
            per_level: 0,
        },
        ..Default::default()
    };
    sim.app.db.save_leveling_policy(sim.guild, &policy).await.unwrap();
    sim.app.db.add_voice_level_role(sim.guild, role, 1).await.unwrap();
    let unsafe_role = unique();
    sim.guild_role(unsafe_role, "Changed after setup", 3);
    sim.app
        .db
        .add_voice_level_role(sim.guild, unsafe_role, 1)
        .await
        .unwrap();
    sim.mock
        .roles
        .lock()
        .unwrap()
        .iter_mut()
        .find(|r| r["id"].as_str().and_then(|id| id.parse::<u64>().ok()) == Some(unsafe_role))
        .unwrap()["permissions"] = json!(ADMINISTRATOR.to_string());

    sim.app.shift_clock(chrono::Duration::days((unique() % 36_500) as i64));
    sim.voice_in(user, Some(channel), &[], false, false, false).await;
    sim.voice_in(other, Some(channel), &[], false, false, false).await;
    for _ in 0..2 {
        sim.app.shift_clock(chrono::Duration::seconds(60));
        pinchora_commands::voice_tick(&sim.app).await.unwrap();
    }
    assert_eq!(sim.app.db.voice_level_xp(sim.guild, user).await.unwrap(), 200);
    assert_eq!(sim.server_xp(user).await, 0);
    assert_eq!(
        sim.mock
            .role_calls
            .lock()
            .unwrap()
            .iter()
            .filter(|s| **s == format!("PUT {user} {role}"))
            .count(),
        1
    );
    assert!(
        sim.mock
            .role_calls
            .lock()
            .unwrap()
            .iter()
            .all(|call| !call.ends_with(&unsafe_role.to_string()))
    );
    let calls = sim.mock.role_calls.lock().unwrap().len();
    sim.app
        .db
        .save_leveling_policy(
            sim.guild,
            &pinchora_core::progression::leveling_policy::LevelingPolicy::default(),
        )
        .await
        .unwrap();
    assert_eq!(
        sim.mock.role_calls.lock().unwrap().len(),
        calls,
        "curve edits do not announce or grant roles"
    );
}

#[tokio::test]
async fn all_parity_admin_commands_reject_members_in_both_languages() {
    let Some(mut sim) = Sim::new("parity_admin_permissions").await else {
        return;
    };
    let admin = unique();
    let role = sim.guild_role(unique(), "Voice", 2);
    for lang in ["en", "tr"] {
        sim.locale = lang;
        for command in [
            sub("curve", vec![text_option("kind", "mee6")]),
            sub("sources", vec![bool_option("reset_on_leave", true)]),
            sub("tuning", vec![int_option("min", 20)]),
            sub("public", vec![text_option("slug", "secret")]),
            sub(
                "voice-role-add",
                vec![int_option("level", 1), role_option("role", &role)],
            ),
            sub("voice-role-remove", vec![role_option("role", &role)]),
            sub("voice-role-list", vec![]),
        ] {
            let reply = sim.xp(admin, command).await;
            assert!(reply.ephemeral, "{reply}");
            assert!(
                reply.content().contains(if lang == "en" {
                    "Manage Server"
                } else {
                    "Sunucuyu Yönet"
                }),
                "{reply}"
            );
        }
    }
    assert_eq!(sim.app.db.leveling_policy(sim.guild).await.unwrap(), Default::default());
    assert!(sim.app.db.voice_level_roles(sim.guild).await.unwrap().is_empty());
}

#[tokio::test]
async fn weekly_highlights_obey_leveling_switch_privacy_and_once_only_claim() {
    let Some(sim) = Sim::new("weekly_highlights_policy").await else {
        return;
    };
    let (shown, hidden, channel) = (unique(), unique(), unique());
    let policy = pinchora_core::progression::leveling_policy::LevelingPolicy {
        highlights_channel_id: Some(channel.to_string()),
        ..Default::default()
    };
    sim.app.db.save_leveling_policy(sim.guild, &policy).await.unwrap();
    let week = week_start(sim.app.now()) - chrono::Duration::weeks(1);
    sim.app.db.add_event_xp(sim.guild, shown, 10, week).await.unwrap();
    sim.app.db.add_event_xp(sim.guild, hidden, 20, week).await.unwrap();
    sim.app
        .db
        .set_privacy(hidden, pinchora_db::Hide::Profile, true)
        .await
        .unwrap();
    sim.app
        .db
        .update_leveling(sim.guild, Some(false), None, None)
        .await
        .unwrap();
    sim.app.forget_guild_settings(sim.guild).await;
    pinchora_commands::leveling_highlights_tick(&sim.app).await.unwrap();
    assert!(sim.posts().iter().all(|p| p.channel != channel));
    sim.app
        .db
        .update_leveling(sim.guild, Some(true), None, None)
        .await
        .unwrap();
    sim.app.forget_guild_settings(sim.guild).await;
    pinchora_commands::leveling_highlights_tick(&sim.app).await.unwrap();
    pinchora_commands::leveling_highlights_tick(&sim.app).await.unwrap();
    let posts = sim.posts();
    let posts: Vec<_> = posts.iter().filter(|p| p.channel == channel).collect();
    assert_eq!(posts.len(), 1);
    let raw = posts[0].body.to_string();
    assert!(raw.contains(&shown.to_string()));
    assert!(!raw.contains(&hidden.to_string()));
}

#[tokio::test]
async fn server_tuning_cannot_multiply_global_xp_or_bypass_its_cooldown() {
    let Some(sim) = Sim::new("server_tuning_global_xp").await else {
        return;
    };
    let (user, channel) = (unique(), unique());
    let policy = pinchora_core::progression::leveling_policy::LevelingPolicy {
        message_min: 1000,
        message_max: 1000,
        message_cooldown_secs: 5,
        effort: true,
        ..Default::default()
    };
    sim.app.db.save_leveling_policy(sim.guild, &policy).await.unwrap();
    sim.say_in(user, channel, &[]).await;
    let global = sim.global_xp(user).await;
    assert!((15..=25).contains(&global));
    sim.app
        .cache
        .release_cooldown(&format!("xp:{}:{user}", sim.guild))
        .await
        .unwrap();
    sim.say_in(user, channel, &[]).await;
    assert_eq!(sim.server_xp(user).await, 2000);
    assert_eq!(sim.global_xp(user).await, global);
}

#[tokio::test]
async fn imported_level_rewards_recheck_live_role_safety() {
    let Some(sim) = Sim::new("import_reward_live_safety").await else {
        return;
    };
    let (user, role) = (unique(), unique());
    sim.guild_role(role, "Reward", 2);
    sim.app.db.add_level_role(sim.guild, 1, role).await.unwrap();
    sim.app
        .db
        .edit_member_xp(sim.guild, user, pinchora_db::XpEdit::Set, 255)
        .await
        .unwrap();
    {
        let mut roles = sim.mock.roles.lock().unwrap();
        roles
            .iter_mut()
            .find(|r| r["id"].as_str().and_then(|id| id.parse::<u64>().ok()) == Some(role))
            .unwrap()["permissions"] = json!(ADMINISTRATOR.to_string());
    }
    pinchora_commands::sync_member_roles(&sim.app, sim.guild, user)
        .await
        .unwrap();
    assert!(sim.mock.role_calls.lock().unwrap().is_empty());
}
