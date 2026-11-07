//! AFK, sticky and scheduled messages through real commands, hooks and the mock REST server.

use chrono::{Duration as ChronoDuration, Utc};
use pinchora_commands::utility_tick;
use pinchora_core::utility::Schedule;
use pinchora_db::{ScheduledRow, UtilityStickyRow};

use super::*;

static TICK_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

#[tokio::test]
async fn afk_sets_clears_once_and_rate_limits_mention_replies() {
    let Some(mut sim) = Sim::new("afk_sets_clears_once_and_rate_limits_mention_replies").await else {
        return;
    };
    let away = unique();
    let sender = unique();
    let channel = unique();
    sim.channel_is(channel, 0, None);
    // Prime a negative cache entry before setting AFK; the generation must invalidate it.
    let mut warm = serde_json::to_value(sim.chat(away, &[])).unwrap();
    warm["channel_id"] = json!(channel.to_string());
    on_message(sim.app.clone(), serde_json::from_value(warm).unwrap())
        .await
        .unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    let reply = sim
        .command(
            away,
            "afk",
            json!([sub("set", vec![text_option("reason", "At lunch")])]),
        )
        .await;
    assert!(reply.ephemeral && reply.content().contains("At lunch"));
    let saved = sim.app.db.afk_for(sim.guild, &[away as i64]).await.unwrap();
    assert_eq!(saved.len(), 1);
    assert!(
        sim.app
            .db
            .clear_afk(sim.guild, away, saved[0].since - ChronoDuration::milliseconds(1))
            .await
            .unwrap()
            .is_empty(),
        "an older message must not clear a newly set AFK"
    );
    let mut message = serde_json::to_value(sim.chat(sender, &[])).unwrap();
    message["channel_id"] = json!(channel.to_string());
    message["timestamp"] = json!(Utc::now().to_rfc3339());
    message["mentions"] = json!([person_json(away, "away", None, false)]);
    message["mentions"][0]["public_flags"] = json!(0);
    on_message(sim.app.clone(), serde_json::from_value(message.clone()).unwrap())
        .await
        .unwrap();
    let count = tokio::time::timeout(std::time::Duration::from_secs(30), async {
        loop {
            let count = sim
                .posts()
                .iter()
                .filter(|p| p.channel == channel && p.body.to_string().contains("At lunch"))
                .count();
            if count > 0 {
                break count;
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
    })
    .await
    .expect("AFK reply arrived");
    assert_eq!(count, 1);
    on_message(sim.app.clone(), serde_json::from_value(message).unwrap())
        .await
        .unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    assert_eq!(
        sim.posts()
            .iter()
            .filter(|p| p.body.to_string().contains("At lunch"))
            .count(),
        0
    );
    let mut own = serde_json::to_value(sim.chat(away, &[])).unwrap();
    own["channel_id"] = json!(channel.to_string());
    own["timestamp"] = json!((Utc::now() + ChronoDuration::seconds(1)).to_rfc3339());
    on_message(sim.app.clone(), serde_json::from_value(own).unwrap())
        .await
        .unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(30), async {
        loop {
            if sim.app.db.afk_for(sim.guild, &[away as i64]).await.unwrap().is_empty() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
    })
    .await
    .expect("AFK cleared");
}

#[tokio::test]
async fn sticky_counts_humans_and_ticker_posts_once() {
    let _tick_guard = TICK_LOCK.lock().await;
    let Some(sim) = Sim::new("sticky_counts_humans_and_ticker_posts_once").await else {
        return;
    };
    let channel = unique();
    sim.mock.everyone_role.store(true, Ordering::SeqCst);
    sim.guild_role_with(BOT_ROLE_ID, "Pinchora", BOT_ROLE_POSITION, 1 << 13);
    sim.channel_is(channel, 0, None);
    sim.app
        .db
        .upsert_sticky(&UtilityStickyRow {
            channel_id: channel as i64,
            guild_id: sim.guild as i64,
            content: "Read the rules".into(),
            embed: None,
            last_message_id: None,
            min_gap_secs: 5,
            min_messages: 2,
            message_count: 0,
            enabled: true,
            lease_until: None,
            pause_reason: None,
            updated_by: None,
        })
        .await
        .unwrap();
    sqlx::query("UPDATE sticky_messages SET updated_at=now()-INTERVAL '6 seconds' WHERE channel_id=$1")
        .bind(channel as i64)
        .execute(sim.app.db.pool())
        .await
        .unwrap();
    for bot in [APPLICATION_ID, unique()] {
        let mut raw = serde_json::to_value(sim.chat(bot, &[])).unwrap();
        raw["channel_id"] = json!(channel.to_string());
        raw["author"]["bot"] = json!(true);
        on_message(sim.app.clone(), serde_json::from_value(raw).unwrap())
            .await
            .unwrap();
    }
    utility_tick(&sim.app).await.unwrap();
    assert!(sim.posts().is_empty(), "bot messages must not trigger a sticky");
    for _ in 0..2 {
        let mut raw = serde_json::to_value(sim.chat(unique(), &[])).unwrap();
        raw["channel_id"] = json!(channel.to_string());
        on_message(sim.app.clone(), serde_json::from_value(raw).unwrap())
            .await
            .unwrap();
    }
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    utility_tick(&sim.app).await.unwrap();
    assert_eq!(
        sim.posts()
            .iter()
            .filter(|p| p.body.to_string().contains("Read the rules"))
            .count(),
        1
    );
    utility_tick(&sim.app).await.unwrap();
    assert!(sim.posts().is_empty(), "the bot's copy must not cause a repost");
}

#[tokio::test]
async fn sticky_requires_both_count_and_gap() {
    let _tick_guard = TICK_LOCK.lock().await;
    let Some(sim) = Sim::new("sticky_requires_both_count_and_gap").await else {
        return;
    };
    let channel = unique();
    sim.app
        .db
        .upsert_sticky(&UtilityStickyRow {
            channel_id: channel as i64,
            guild_id: sim.guild as i64,
            content: "Timed note".into(),
            embed: None,
            last_message_id: None,
            min_gap_secs: 5,
            min_messages: 1000,
            message_count: 0,
            enabled: true,
            lease_until: None,
            pause_reason: None,
            updated_by: None,
        })
        .await
        .unwrap();
    sqlx::query("UPDATE sticky_messages SET updated_at=now()-INTERVAL '6 seconds' WHERE channel_id=$1")
        .bind(channel as i64)
        .execute(sim.app.db.pool())
        .await
        .unwrap();
    assert!(
        sim.app
            .db
            .claim_stickies(1000)
            .await
            .unwrap()
            .iter()
            .all(|r| r.channel_id != channel as i64)
    );
    sim.app.db.count_sticky_messages(channel, 1000).await.unwrap();
    sqlx::query("UPDATE sticky_messages SET updated_at=now() WHERE channel_id=$1")
        .bind(channel as i64)
        .execute(sim.app.db.pool())
        .await
        .unwrap();
    assert!(
        sim.app
            .db
            .claim_stickies(1000)
            .await
            .unwrap()
            .iter()
            .all(|r| r.channel_id != channel as i64)
    );
    sqlx::query("UPDATE sticky_messages SET updated_at=now()-INTERVAL '6 seconds' WHERE channel_id=$1")
        .bind(channel as i64)
        .execute(sim.app.db.pool())
        .await
        .unwrap();
    let claimed = sim.app.db.claim_stickies(1000).await.unwrap();
    assert!(claimed.iter().any(|r| r.channel_id == channel as i64));
    let second = sim.app.db.claim_stickies(1000).await.unwrap();
    assert!(
        second.iter().all(|row| row.channel_id != channel as i64),
        "second pod must not claim a live lease"
    );
    for row in claimed.into_iter().chain(second) {
        sim.app.db.release_sticky(&row).await.unwrap();
    }
    sim.app.db.remove_sticky(sim.guild, channel).await.unwrap();
}

#[tokio::test]
async fn scheduled_claim_is_single_across_two_workers() {
    let _tick_guard = TICK_LOCK.lock().await;
    let Some(sim) = Sim::new("scheduled_claim_is_single_across_two_workers").await else {
        return;
    };
    sim.mock.everyone_role.store(true, Ordering::SeqCst);
    sim.channel_is(CHANNEL_ID, 0, None);
    let message = format!("One announcement {}", unique());
    let id = sim
        .app
        .db
        .add_schedule(&ScheduledRow {
            id: 0,
            guild_id: sim.guild as i64,
            channel_id: CHANNEL_ID as i64,
            content: format!("@everyone {message}"),
            embed: None,
            schedule: json!(Schedule::Once),
            tz: "UTC".into(),
            next_run_at: Utc::now() - ChronoDuration::seconds(1),
            last_run_at: None,
            runs: 0,
            enabled: true,
            lease_until: None,
            pause_reason: None,
            created_by: None,
            ping_allowed: true,
        })
        .await
        .unwrap()
        .unwrap();
    let (a, b) = tokio::join!(sim.app.db.claim_schedules(1000), sim.app.db.claim_schedules(1000));
    let (a, b) = (a.unwrap(), b.unwrap());
    assert_eq!(a.iter().chain(b.iter()).filter(|row| row.id == id).count(), 1);
    for row in a.into_iter().chain(b) {
        sim.app.db.release_schedule(&row).await.unwrap();
    }
    utility_tick(&sim.app).await.unwrap();
    let posts = sim.posts();
    assert_eq!(
        posts.iter().filter(|p| p.body.to_string().contains(&message)).count(),
        1
    );
    let posted = posts.iter().find(|p| p.body.to_string().contains(&message)).unwrap();
    assert!(posted.content().contains("@\u{200B}everyone"));
    assert_eq!(posted.body["allowed_mentions"]["parse"], json!([]));
    assert!(!sim.app.db.schedule(sim.guild, id).await.unwrap().unwrap().enabled);
}

#[tokio::test]
async fn manager_commands_create_preview_pause_and_remove_messages() {
    let Some(mut sim) = Sim::new("manager_commands_create_preview_pause_and_remove_messages").await else {
        return;
    };
    let channel = unique();
    sim.permissions = MANAGE_GUILD;
    sim.mock.everyone_role.store(true, Ordering::SeqCst);
    sim.guild_role_with(BOT_ROLE_ID, "Pinchora", BOT_ROLE_POSITION, 1 << 13);
    sim.channel_is(channel, 0, None);
    let manager = unique();
    let sticky = sim
        .command_resolved(
            manager,
            "sticky",
            json!([sub(
                "set",
                vec![
                    channel_option("channel", channel),
                    text_option("text", "Rules stay visible"),
                    int_option("messages", 2),
                    int_option("seconds", 30)
                ]
            )]),
            resolved_channels(&[channel]),
        )
        .await;
    assert!(sticky.ephemeral && sticky.content().contains("Saved"), "{sticky}");
    let shown = sim.command(manager, "sticky", json!([sub("show", vec![])])).await;
    assert!(shown.content().contains("Rules stay visible"), "{shown}");
    let form = sim.click(manager, &shown, "Write a long sticky").await;
    assert!(form.modal.as_ref().is_some_and(|(id, _)| id.starts_with("sk:e:")));
    let updated = sim
        .submit_fields(
            manager,
            &form,
            &[
                ("channel", &channel.to_string()),
                ("content", "Updated sticky"),
                ("messages", "2"),
                ("gap", "30"),
            ],
        )
        .await;
    assert!(updated.content().contains("Saved"), "{updated}");
    let when = (Utc::now() + ChronoDuration::minutes(30)).to_rfc3339();
    let scheduled = sim
        .command_resolved(
            manager,
            "schedule",
            json!([sub(
                "add",
                vec![
                    channel_option("channel", channel),
                    text_option("text", "Reef meeting"),
                    text_option("when", &when),
                    text_option("every", "daily")
                ]
            )]),
            resolved_channels(&[channel]),
        )
        .await;
    assert!(
        scheduled.ephemeral && scheduled.content().contains("Scheduled message"),
        "{scheduled}"
    );
    let id = sim.app.db.schedules(sim.guild).await.unwrap()[0].id;
    let listed = sim.command(manager, "schedule", json!([sub("list", vec![])])).await;
    assert!(listed.content().contains("Reef meeting"), "{listed}");
    let preview = sim
        .command(manager, "schedule", json!([sub("preview", vec![int_option("id", id)])]))
        .await;
    assert!(preview.content().contains("Next runs"), "{preview}");
    let paused = sim
        .command(manager, "schedule", json!([sub("pause", vec![int_option("id", id)])]))
        .await;
    assert!(paused.content().contains("Paused"));
    assert!(!sim.app.db.schedule(sim.guild, id).await.unwrap().unwrap().enabled);
    let resumed = sim
        .command(manager, "schedule", json!([sub("resume", vec![int_option("id", id)])]))
        .await;
    assert!(resumed.content().contains("Resumed"));
    let now = sim
        .command(manager, "schedule", json!([sub("run-now", vec![int_option("id", id)])]))
        .await;
    assert!(now.content().contains("Saved"), "{now}");
    assert_eq!(sim.posts().iter().filter(|p| p.content() == "Reef meeting").count(), 1);
    sim.command(manager, "schedule", json!([sub("remove", vec![int_option("id", id)])]))
        .await;
    assert!(sim.app.db.schedules(sim.guild).await.unwrap().is_empty());
    let empty = sim.command(manager, "schedule", json!([sub("list", vec![])])).await;
    let form = sim.click(manager, &empty, "Create with form").await;
    assert!(form.modal.as_ref().is_some_and(|(id, _)| id.starts_with("sm:c:")));
    let made = sim
        .submit_fields(
            manager,
            &form,
            &[
                ("channel", &channel.to_string()),
                ("content", "From form"),
                ("when", &when),
                ("every", "once"),
                ("tz", "UTC"),
            ],
        )
        .await;
    assert!(made.content().contains("Scheduled message"), "{made}");
    assert_eq!(sim.app.db.schedules(sim.guild).await.unwrap().len(), 1);
    sim.command_resolved(
        manager,
        "sticky",
        json!([sub("remove", vec![channel_option("channel", channel)])]),
        resolved_channels(&[channel]),
    )
    .await;
    assert!(sim.app.db.stickies(sim.guild).await.unwrap().is_empty());
}

#[tokio::test]
async fn revoked_creator_permissions_suppress_everyone_here_and_role_pings() {
    let _guard = TICK_LOCK.lock().await;
    let Some(sim) = Sim::new("utility_revoked_pings").await else {
        return;
    };
    let creator = unique();
    let channel = unique();
    let role = unique();
    sim.mock.everyone_role.store(true, Ordering::SeqCst);
    sim.guild_role_with(BOT_ROLE_ID, "Pinchora", BOT_ROLE_POSITION, 1 << 17);
    sim.guild_role_with(role, "Broad pings", 2, 1 << 17);
    sim.mock.member_roles.lock().unwrap().insert(creator, vec![role]);
    sim.channel_is(channel, 0, None);
    assert!(pinchora_commands::utility_creator_may_ping(&sim.app, sim.guild, channel, creator).await);
    sim.mock.member_roles.lock().unwrap().insert(creator, vec![]);
    let id = sim
        .app
        .db
        .add_schedule(&ScheduledRow {
            id: 0,
            guild_id: sim.guild as i64,
            channel_id: channel as i64,
            content: format!("@everyone @here <@&{role}>"),
            embed: None,
            schedule: json!(Schedule::Once),
            tz: "UTC".into(),
            next_run_at: Utc::now() - ChronoDuration::seconds(1),
            last_run_at: None,
            runs: 0,
            enabled: true,
            created_by: Some(creator as i64),
            ping_allowed: true,
            lease_until: None,
            pause_reason: None,
        })
        .await
        .unwrap()
        .unwrap();
    utility_tick(&sim.app).await.unwrap();
    let posts = sim.posts();
    let post = posts.iter().find(|p| p.channel == channel).expect("scheduled post");
    assert_eq!(post.body["allowed_mentions"]["parse"], json!([]));
    assert!(!post.content().contains("@everyone"));
    assert!(!post.content().contains("@here"));
    sim.app.db.remove_schedule(sim.guild, id).await.unwrap();
}

#[tokio::test]
async fn lost_permissions_pause_instead_of_retrying_each_tick() {
    let _guard = TICK_LOCK.lock().await;
    let Some(sim) = Sim::new("utility_permissions_pause").await else {
        return;
    };
    let channel = unique();
    sim.channel_is(channel, 0, None);
    let id = sim
        .app
        .db
        .add_schedule(&ScheduledRow {
            id: 0,
            guild_id: sim.guild as i64,
            channel_id: channel as i64,
            content: "never sent".into(),
            embed: None,
            schedule: json!(Schedule::Once),
            tz: "UTC".into(),
            next_run_at: Utc::now() - ChronoDuration::seconds(1),
            last_run_at: None,
            runs: 0,
            enabled: true,
            created_by: None,
            ping_allowed: false,
            lease_until: None,
            pause_reason: None,
        })
        .await
        .unwrap()
        .unwrap();
    utility_tick(&sim.app).await.unwrap();
    let row = sim.app.db.schedule(sim.guild, id).await.unwrap().unwrap();
    assert!(!row.enabled);
    assert_eq!(row.pause_reason.as_deref(), Some("permissions_lost"));
    utility_tick(&sim.app).await.unwrap();
    assert!(sim.posts().iter().all(|p| p.channel != channel));
    sim.app.db.remove_schedule(sim.guild, id).await.unwrap();
    sim.mock.channels.lock().unwrap().remove(&channel);
    let mut deleted = row;
    deleted.id = 0;
    deleted.enabled = true;
    let id = sim.app.db.add_schedule(&deleted).await.unwrap().unwrap();
    utility_tick(&sim.app).await.unwrap();
    let paused = sim.app.db.schedule(sim.guild, id).await.unwrap().unwrap();
    assert!(!paused.enabled);
    assert_eq!(paused.pause_reason.as_deref(), Some("channel_deleted"));
    sim.app.db.remove_schedule(sim.guild, id).await.unwrap();
}

#[tokio::test]
async fn utility_admin_rows_refuse_unprivileged_members() {
    let Some(mut sim) = Sim::new("utility_admin_rows").await else {
        return;
    };
    let user = unique();
    for permissions in [0, (1 << 17) | (1 << 13)] {
        sim.permissions = permissions;
        for (cmd, options) in [
            ("sticky", json!([sub("show", vec![])])),
            (
                "sticky",
                json!([sub("remove", vec![channel_option("channel", CHANNEL_ID)])]),
            ),
            ("schedule", json!([sub("list", vec![])])),
            ("schedule", json!([sub("pause", vec![int_option("id", 1)])])),
            ("schedule", json!([sub("resume", vec![int_option("id", 1)])])),
            ("schedule", json!([sub("run-now", vec![int_option("id", 1)])])),
            ("schedule", json!([sub("remove", vec![int_option("id", 1)])])),
        ] {
            let reply = sim
                .command_resolved(user, cmd, options, resolved_channels(&[CHANNEL_ID]))
                .await;
            assert!(
                reply.ephemeral && reply.content().contains("Manage Server"),
                "{cmd}: {reply}"
            );
        }
    }
}

#[tokio::test]
async fn utility_forms_cannot_target_another_servers_channel() {
    let Some(mut sim) = Sim::new("utility_cross_guild_channel").await else {
        return;
    };
    let channel = unique();
    sim.permissions = MANAGE_GUILD;
    sim.mock.everyone_role.store(true, Ordering::SeqCst);
    sim.guild_role_with(BOT_ROLE_ID, "Pinchora", BOT_ROLE_POSITION, 1 << 13);
    sim.channel_is(channel, 0, None);
    sim.mock.channels.lock().unwrap().get_mut(&channel).unwrap()["guild_id"] = json!((sim.guild + 1).to_string());
    assert!(
        pinchora_commands::utility_bot_permissions(&sim.app, sim.guild, channel)
            .await
            .is_err()
    );
    let user = unique();
    let shown = sim.command(user, "sticky", json!([sub("show", vec![])])).await;
    let form = sim.click(user, &shown, "Write a long sticky").await;
    sim.submit_fields(
        user,
        &form,
        &[
            ("channel", &channel.to_string()),
            ("content", "no cross-guild posts"),
            ("messages", "1"),
            ("gap", "5"),
        ],
    )
    .await;
    assert!(sim.app.db.stickies(sim.guild).await.unwrap().is_empty());
}

#[tokio::test]
async fn disabled_utility_leaves_afk_and_sticky_untouched() {
    let _guard = TICK_LOCK.lock().await;
    let Some(sim) = Sim::new("utility_module_off").await else {
        return;
    };
    let channel = unique();
    let user = unique();
    sim.channel_is(channel, 0, None);
    sim.app.db.set_afk(sim.guild, user, "still away", false).await.unwrap();
    sim.app
        .db
        .upsert_sticky(&UtilityStickyRow {
            channel_id: channel as i64,
            guild_id: sim.guild as i64,
            content: "off".into(),
            embed: None,
            last_message_id: None,
            min_gap_secs: 5,
            min_messages: 1,
            message_count: 0,
            enabled: true,
            updated_by: None,
            lease_until: None,
            pause_reason: None,
        })
        .await
        .unwrap();
    sim.app
        .db
        .set_module(sim.guild, pinchora_core::modules::Module::Utility, Some(false), None)
        .await
        .unwrap();
    sim.app.forget_guild_settings(sim.guild).await;
    for _ in 0..3 {
        let mut raw = serde_json::to_value(sim.chat(user, &[])).unwrap();
        raw["channel_id"] = json!(channel.to_string());
        raw["timestamp"] = json!((Utc::now() + ChronoDuration::seconds(1)).to_rfc3339());
        on_message(sim.app.clone(), serde_json::from_value(raw).unwrap())
            .await
            .unwrap();
    }
    utility_tick(&sim.app).await.unwrap();
    assert_eq!(sim.app.db.afk_for(sim.guild, &[user as i64]).await.unwrap().len(), 1);
    assert_eq!(sim.app.db.stickies(sim.guild).await.unwrap()[0].message_count, 0);
    assert!(sim.posts().iter().all(|p| p.channel != channel));
    sim.app.db.remove_sticky(sim.guild, channel).await.unwrap();
}

#[tokio::test]
async fn utility_turkish_numbers_and_iana_default_round_trip() {
    let Some(mut sim) = Sim::new("utility_turkish_numbers_and_iana_default_round_trip").await else {
        return;
    };
    sim.permissions = MANAGE_GUILD;
    sim.locale = "tr";
    let channel = unique();
    sim.mock.everyone_role.store(true, Ordering::SeqCst);
    sim.guild_role_with(BOT_ROLE_ID, "Pinchora", BOT_ROLE_POSITION, 1 << 13);
    sim.channel_is(channel, 0, None);
    let manager = unique();
    sim.command_resolved(
        manager,
        "sticky",
        json!([sub(
            "set",
            vec![
                channel_option("channel", channel),
                text_option("text", "Kurallar"),
                int_option("messages", 1000),
                int_option("seconds", 86400)
            ]
        )]),
        resolved_channels(&[channel]),
    )
    .await;
    let shown = sim.command(manager, "sticky", json!([sub("show", vec![])])).await;
    assert!(shown.content().contains("1.000 mesajda bir / 86.400 sn"), "{shown}");
    sim.app
        .db
        .set_utility_timezone(sim.guild, "Europe/Istanbul")
        .await
        .unwrap();
    let when = (Utc::now() + ChronoDuration::days(2)).to_rfc3339();
    let added = sim
        .command_resolved(
            manager,
            "schedule",
            json!([sub(
                "add",
                vec![
                    channel_option("channel", channel),
                    text_option("text", "Duyuru"),
                    text_option("when", &when),
                    text_option("every", "daily")
                ]
            )]),
            resolved_channels(&[channel]),
        )
        .await;
    assert!(added.content().contains("kaydedildi"), "{added}");
    let rows = sim.app.db.schedules(sim.guild).await.unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].tz, "Europe/Istanbul");
    sim.app.db.remove_schedule(sim.guild, rows[0].id).await.unwrap();
    sim.app.db.remove_sticky(sim.guild, channel).await.unwrap();
}
