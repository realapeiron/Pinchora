//! Invite cards, paging and database attribution through the simulator's real database.

use chrono::{Duration, Utc};
use pinchora_db::{GuildInvite, InviteObservation};

use super::*;

fn observed(code: &str, inviter: u64, uses: i32) -> GuildInvite {
    GuildInvite {
        code: code.to_owned(),
        inviter_id: Some(inviter as i64),
        uses,
        max_uses: 0,
        max_age: 86400,
        created_at: Utc::now(),
    }
}

#[tokio::test]
async fn invite_diff_two_joins_leave_fake_and_rejoin() {
    let Some(mut sim) = Sim::new("invite_diff_two_joins_leave_fake_and_rejoin").await else {
        return;
    };
    let inviter = unique();
    let first = unique();
    let second = unique();
    sim.app.db.guild_settings(sim.guild).await.unwrap();
    sim.app
        .db
        .baseline_invites(sim.guild, &[observed("reef", inviter, 0)])
        .await
        .unwrap();
    let now = Utc::now();
    let one = sim
        .app
        .db
        .record_invite_join(
            sim.guild,
            first,
            now,
            false,
            InviteObservation {
                invites: &[observed("reef", inviter, 2)],
                vanity_changed: false,
                list_available: true,
                preferred_code: Some("reef"),
            },
        )
        .await
        .unwrap();
    let two = sim
        .app
        .db
        .record_invite_join(
            sim.guild,
            second,
            now,
            false,
            InviteObservation {
                invites: &[observed("reef", inviter, 2)],
                vanity_changed: false,
                list_available: true,
                preferred_code: Some("reef"),
            },
        )
        .await
        .unwrap();
    assert_eq!((one.inviter_id, two.inviter_id), (Some(inviter), Some(inviter)));
    assert_eq!(sim.app.db.invite_score(sim.guild, inviter).await.unwrap().total, 2);
    sim.app.db.leave_invite(sim.guild, first).await.unwrap();
    let score = sim.app.db.invite_score(sim.guild, inviter).await.unwrap();
    assert_eq!((score.regular, score.left, score.total), (2, 1, 1));
    let again = sim
        .app
        .db
        .record_invite_join(
            sim.guild,
            first,
            now + Duration::minutes(1),
            false,
            InviteObservation {
                invites: &[observed("reef", inviter, 2)],
                vanity_changed: false,
                list_available: true,
                preferred_code: Some("reef"),
            },
        )
        .await
        .unwrap();
    assert!(!again.fresh);
    assert_eq!(sim.app.db.invite_score(sim.guild, inviter).await.unwrap().total, 2);
    let fake = unique();
    sim.app
        .db
        .record_invite_join(
            sim.guild,
            fake,
            now,
            true,
            InviteObservation {
                invites: &[observed("reef", inviter, 3)],
                vanity_changed: false,
                list_available: true,
                preferred_code: Some("reef"),
            },
        )
        .await
        .unwrap();
    let score = sim.app.db.invite_score(sim.guild, inviter).await.unwrap();
    assert_eq!((score.fake, score.total), (1, 2));
    sim.app.db.change_invite_bonus(sim.guild, inviter, 3).await.unwrap();
    assert_eq!(sim.app.db.invite_score(sim.guild, inviter).await.unwrap().total, 5);
    let reward_role = unique();
    sim.guild_role(reward_role, "Invite reward", 3);
    pinchora_commands::grant_invite_reward(&sim.app, sim.guild, inviter, reward_role)
        .await
        .unwrap();
    assert!(
        sim.role_changes()
            .iter()
            .any(|call| call.contains(&reward_role.to_string()))
    );
    let unsafe_role = unique();
    sim.guild_role_with(unsafe_role, "Unsafe", 3, ADMINISTRATOR);
    assert!(
        pinchora_commands::grant_invite_reward(&sim.app, sim.guild, inviter, unsafe_role)
            .await
            .is_err()
    );
    assert!(sim.role_changes().is_empty());
    let card = sim.command(inviter, "invites", json!([sub("user", vec![])])).await;
    assert_eq!(card.files.len(), 1, "the user view draws a pixel card");
    let top = sim.command(inviter, "invites", json!([sub("top", vec![])])).await;
    assert!(
        top.embeds[0]
            .description
            .as_deref()
            .unwrap_or_default()
            .contains(&inviter.to_string())
    );
    assert!(
        top.controls()
            .any(|c| matches!(c,Component::Button(b) if b.custom_id.as_deref().unwrap_or_default().starts_with("in:")))
    );
}

#[tokio::test]
async fn invite_admin_requires_manage_server_and_temp_is_private() {
    let Some(mut sim) = Sim::new("invite_admin_requires_manage_server_and_temp_is_private").await else {
        return;
    };
    sim.app.db.guild_settings(sim.guild).await.unwrap();
    let user = unique();
    let denied = sim.command(user, "inviteadmin", json!([sub("settings", vec![])])).await;
    assert!(denied.ephemeral && denied.content().contains("Manage Server"));
    sim.permissions = MANAGE_GUILD;
    let saved = sim.command(user, "inviteadmin", json!([sub("settings", vec![])])).await;
    assert!(saved.ephemeral && saved.content().contains("saved"));
    let temp = sim.command(user, "invites", json!([sub("temp", vec![])])).await;
    assert!(temp.ephemeral && temp.content().contains("turned off"));
}

#[tokio::test]
async fn invite_card_respects_private_profile() {
    let Some(mut sim) = Sim::new("invite_card_respects_private_profile").await else {
        return;
    };
    let viewer = unique();
    let private = unique();
    sim.app
        .db
        .set_privacy(private, pinchora_db::Hide::Profile, true)
        .await
        .unwrap();
    let reply = sim
        .command_resolved(
            viewer,
            "invites",
            json!([sub("user", vec![user_option("user", private)])]),
            resolved(&[private], &[]),
        )
        .await;
    assert!(reply.ephemeral && reply.content().contains("private"));
    assert!(reply.files.is_empty());
}

#[tokio::test]
async fn invite_reward_is_revoked_when_score_falls_or_manager_loses_authority() {
    let Some(sim) = Sim::new("invite_reward_reconciliation").await else {
        return;
    };
    let (manager, inviter, invited, role) = (unique(), unique(), unique(), unique());
    sim.mock.owner.store(manager, Ordering::SeqCst);
    sim.guild_role(role, "Invite reward", 3);
    sim.app
        .db
        .baseline_invites(sim.guild, &[observed("reward", inviter, 0)])
        .await
        .unwrap();
    let reward = observed("reward", inviter, 1);
    sim.app
        .db
        .record_invite_join(
            sim.guild,
            invited,
            Utc::now(),
            false,
            InviteObservation {
                invites: &[reward],
                vanity_changed: false,
                list_available: true,
                preferred_code: None,
            },
        )
        .await
        .unwrap();
    sim.app
        .db
        .put_invite_reward_by(sim.guild, 1, role, Some(manager))
        .await
        .unwrap();
    pinchora_commands::reconcile_all_invite_rewards(&sim.app, sim.guild)
        .await
        .unwrap();
    assert!(
        sim.role_changes()
            .iter()
            .any(|call| call.contains(&format!("PUT {inviter} {role}")))
    );
    sim.app.db.leave_invite(sim.guild, invited).await.unwrap();
    pinchora_commands::reconcile_all_invite_rewards(&sim.app, sim.guild)
        .await
        .unwrap();
    assert!(
        sim.role_changes()
            .iter()
            .any(|call| call.contains(&format!("DELETE {inviter} {role}")))
    );
    sim.app.db.change_invite_bonus(sim.guild, inviter, 1).await.unwrap();
    pinchora_commands::reconcile_all_invite_rewards(&sim.app, sim.guild)
        .await
        .unwrap();
    assert!(
        sim.role_changes()
            .iter()
            .any(|call| call.contains(&format!("PUT {inviter} {role}")))
    );
    let role_text = role.to_string();
    {
        let mut roles = sim.mock.roles.lock().unwrap();
        roles
            .iter_mut()
            .find(|r| r["id"].as_str() == Some(role_text.as_str()))
            .unwrap()["permissions"] = json!(ADMINISTRATOR.to_string());
    }
    pinchora_commands::reconcile_all_invite_rewards(&sim.app, sim.guild)
        .await
        .unwrap();
    assert!(
        sim.role_changes()
            .iter()
            .any(|call| call.contains(&format!("DELETE {inviter} {role}")))
    );
    {
        let mut roles = sim.mock.roles.lock().unwrap();
        roles
            .iter_mut()
            .find(|r| r["id"].as_str() == Some(role_text.as_str()))
            .unwrap()["permissions"] = json!("0");
    }
    pinchora_commands::reconcile_all_invite_rewards(&sim.app, sim.guild)
        .await
        .unwrap();
    assert!(
        sim.role_changes()
            .iter()
            .any(|call| call.contains(&format!("PUT {inviter} {role}")))
    );
    sim.mock.owner.store(unique(), Ordering::SeqCst);
    pinchora_commands::reconcile_all_invite_rewards(&sim.app, sim.guild)
        .await
        .unwrap();
    assert!(
        sim.role_changes()
            .iter()
            .any(|call| call.contains(&format!("DELETE {inviter} {role}")))
    );
}

#[tokio::test]
async fn temporary_invite_uses_discord_and_redis_daily_quota() {
    let hits = Arc::new(AtomicUsize::new(0));
    let guild_id = Arc::new(AtomicU64::new(0));
    let guild_for_router = guild_id.clone();
    let blocked_user = Arc::new(AtomicU64::new(0));
    let blocked_for_router = blocked_user.clone();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let router = Router::new().fallback({
        let hits = hits.clone();
        let guild_id = guild_for_router.clone();
        let blocked_user = blocked_for_router.clone();
        move |method: Method, uri: Uri| {
            let hits = hits.clone();
            let guild_id = guild_id.clone();
            let blocked_user = blocked_user.clone();
            async move {
                let path = uri.path();
                let guild = guild_id.load(Ordering::SeqCst);
                if method == Method::GET {
                    if path.ends_with("/roles") {
                        let mut everyone = role_json(guild, "@everyone", 0, false);
                        everyone["permissions"] = json!("1025");
                        return axum::Json(json!([everyone]));
                    }
                    if path.contains("/members/") {
                        let user = path.rsplit('/').next().unwrap().parse().unwrap();
                        return axum::Json(member_json(user, &[]));
                    }
                    if path.contains("/channels/") {
                        let denied = blocked_user.load(Ordering::SeqCst);
                        let overwrites = if denied == 0 {
                            vec![]
                        } else {
                            vec![json!({"id": denied.to_string(), "type": 1, "allow": "0", "deny": "1025"})]
                        };
                        return axum::Json(json!({"id": CHANNEL_ID.to_string(), "type": 0,
                            "guild_id": guild.to_string(), "permission_overwrites": overwrites}));
                    }
                    if path.contains("/guilds/") {
                        return axum::Json(guild_json(guild));
                    }
                }
                assert_eq!(method, Method::POST);
                assert!(uri.path().ends_with("/invites"));
                hits.fetch_add(1, Ordering::SeqCst);
                axum::Json(json!({"code":"friendabc","channel":null,"type":0}))
            }
        }
    });
    tokio::spawn(axum::serve(listener, router).into_future());
    let rest = Arc::new(
        Client::builder()
            .token("Bot sim".into())
            .proxy(address.to_string(), true)
            .ratelimiter(None)
            .build(),
    );
    let Some(mut sim) = Sim::with("temporary_invite_uses_discord_and_redis_daily_quota", move |app| {
        app.http = rest
    })
    .await
    else {
        return;
    };
    guild_id.store(sim.guild, Ordering::SeqCst);
    sim.app.db.guild_settings(sim.guild).await.unwrap();
    let mut settings = sim.app.db.invite_settings(sim.guild).await.unwrap();
    settings.temp_enabled = true;
    settings.temp_channel_id = Some(CHANNEL_ID as i64);
    settings.temp_per_user_daily = 2;
    sim.app.db.save_invite_settings(sim.guild, &settings).await.unwrap();
    let user = unique();
    for _ in 0..2 {
        let link = sim.command(user, "invites", json!([sub("temp", vec![])])).await;
        assert!(
            link.ephemeral && link.content().contains("https://discord.gg/friendabc"),
            "{:?}",
            link.content()
        );
    }
    let denied = sim.command(user, "invites", json!([sub("temp", vec![])])).await;
    assert!(denied.ephemeral && denied.content().contains("today's 2"));
    let hidden = unique();
    blocked_user.store(hidden, Ordering::SeqCst);
    let denied = sim.command(hidden, "invites", json!([sub("temp", vec![])])).await;
    assert!(denied.ephemeral && denied.content().contains("both need"));
    let forged = sim.component(hidden, "tl:wrong", &[], &denied).await;
    assert!(forged.ephemeral && forged.content().contains("expired"));
    assert_eq!(hits.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn turkish_invite_bonus_and_leaderboard_use_grouped_counts() {
    let Some(mut sim) = Sim::new("turkish_invite_counts").await else {
        return;
    };
    sim.locale = "tr";
    sim.permissions = MANAGE_GUILD;
    let user = unique();
    sim.app.db.change_invite_bonus(sim.guild, user, 1500).await.unwrap();
    let reply = sim
        .command_resolved(
            user,
            "inviteadmin",
            json!([sub(
                "bonus-add",
                vec![user_option("user", user), int_option("amount", 1000)]
            )]),
            resolved(&[user], &[]),
        )
        .await;
    assert_eq!(reply.content(), "Bonus 2.500 olarak güncellendi.");
    let board = sim.command(user, "invites", json!([sub("top", vec![])])).await;
    assert!(board.description().contains("2.500"), "{}", board.description());
    let card = sim.command(user, "invites", json!([sub("user", vec![])])).await;
    assert_eq!(card.files.len(), 1);
}
