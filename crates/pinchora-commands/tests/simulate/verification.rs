//! F10 flows use real Postgres/Redis and a Discord mock with mutable raid settings.
use super::*;
use pinchora_core::verification::Config;
use sha2::{Digest, Sha256};
#[derive(Clone)]
struct SafetyMock {
    base: Mock,
    guild: Arc<Mutex<Value>>,
    patches: Arc<AtomicUsize>,
    refuse_invites: Arc<std::sync::atomic::AtomicBool>,
    refuse_channels: Arc<std::sync::atomic::AtomicBool>,
    channel_writes: Arc<AtomicUsize>,
}
async fn safety_discord(
    State(mock): State<SafetyMock>,
    method: Method,
    uri: Uri,
    headers: HeaderMap,
    body: Bytes,
) -> (StatusCode, String) {
    let parts: Vec<_> = uri.path().trim_matches('/').split('/').collect();
    if parts.contains(&"channels") && matches!(method, Method::PATCH | Method::PUT | Method::DELETE) {
        mock.channel_writes.fetch_add(1, Ordering::SeqCst);
        if mock.refuse_channels.load(Ordering::SeqCst) {
            return (
                StatusCode::FORBIDDEN,
                json!({"code":50013,"message":"Missing Permissions"}).to_string(),
            );
        }
    }
    if matches!(parts.as_slice(), [.., "guilds", _]) {
        if method == Method::PATCH {
            let patch: Value = serde_json::from_slice(&body).unwrap();
            if patch.get("features").is_some() && mock.refuse_invites.load(Ordering::SeqCst) {
                return (
                    StatusCode::FORBIDDEN,
                    json!({"code":50013,"message":"Missing Permissions"}).to_string(),
                );
            }
            let mut g = mock.guild.lock().unwrap();
            for (k, v) in patch.as_object().unwrap() {
                g[k] = v.clone();
            }
            mock.patches.fetch_add(1, Ordering::SeqCst);
            return (StatusCode::OK, g.to_string());
        }
        if method == Method::GET {
            return (StatusCode::OK, mock.guild.lock().unwrap().to_string());
        }
    }
    if let (&Method::PATCH, [.., "channels", channel]) = (&method, parts.as_slice()) {
        let patch: Value = serde_json::from_slice(&body).unwrap();
        let mut channels = mock.base.channels.lock().unwrap();
        if let Some(ch) = channels.get_mut(&channel.parse::<u64>().unwrap()) {
            for (k, v) in patch.as_object().unwrap() {
                ch[k] = v.clone();
            }
            return (StatusCode::OK, ch.to_string());
        }
    }
    let (status, body) = discord(State(mock.base), method.clone(), uri.clone(), headers, body).await;
    if method == Method::GET && uri.path().ends_with("/roles") && status == StatusCode::OK {
        let mut roles: Value = serde_json::from_str(&body).unwrap();
        roles[0]["permissions"] = json!(((1u64 << 5) | (1 << 28) | (1 << 4) | (1 << 1) | (1 << 2)).to_string());
        return (status, roles.to_string());
    }
    (status, body)
}
async fn setup(name: &'static str) -> Option<(Sim, SafetyMock, tokio::task::JoinHandle<()>)> {
    let mut sim = Sim::new(name).await?;
    sim.channel_is(CHANNEL_ID, 0, None);
    let mut g = guild_json(sim.guild);
    g["owner_id"] = json!(APPLICATION_ID.to_string());
    let mock = SafetyMock {
        base: sim.mock.clone(),
        guild: Arc::new(Mutex::new(g)),
        patches: Arc::default(),
        refuse_invites: Arc::default(),
        refuse_channels: Arc::default(),
        channel_writes: Arc::default(),
    };
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let router = Router::new().fallback(safety_discord).with_state(mock.clone());
    let task = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    Arc::get_mut(&mut sim.app).unwrap().http = Arc::new(
        Client::builder()
            .token("Bot sim".into())
            .proxy(addr.to_string(), true)
            .ratelimiter(None)
            .build(),
    );
    Some((sim, mock, task))
}
async fn config(sim: &Sim, mode: &str) -> (Config, u64, u64) {
    let verified = unique();
    let held = unique();
    sim.guild_role(verified, "Verified", 2);
    sim.guild_role(held, "Waiting", 1);
    let mut c = Config::default();
    c.verification.enabled = true;
    c.verification.mode = mode.into();
    c.verification.verified_role_id = Some(verified.to_string());
    c.verification.unverified_role_id = Some(held.to_string());
    sim.app.db.save_safety(sim.guild, &c).await.unwrap();
    (c, verified, held)
}
#[tokio::test]
async fn verification_holds_join_then_button_releases_once() {
    let Some((mut sim, _, task)) = setup("verification_button").await else {
        return;
    };
    let (_, verified, held) = config(&sim, "button").await;
    let user = unique();
    sim.join(user, "Mia", false, None).await;
    assert!(
        sim.app
            .db
            .pending_verification(sim.guild, user)
            .await
            .unwrap()
            .unwrap()
            .verified_at
            .is_none()
    );
    assert!(sim.role_changes().contains(&format!("PUT {user} {held}")));
    let joins = tokio::time::timeout(std::time::Duration::from_secs(10), async {
        loop {
            let joins: i64 =
                sqlx::query_scalar("SELECT count(*) FROM invite_joins WHERE guild_id = $1 AND user_id = $2")
                    .bind(sim.guild as i64)
                    .bind(user as i64)
                    .fetch_one(sim.app.db.pool())
                    .await
                    .unwrap();
            if joins > 0 {
                return joins;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("invite attribution finishes in the background");
    assert_eq!(joins, 1, "held joins still reach invite tracking");
    let result = sim.command(user, "verify", json!([])).await;
    assert!(result.content.unwrap().contains("Verified"));
    let changes = sim.role_changes();
    assert!(changes.contains(&format!("PUT {user} {verified}")));
    assert!(changes.contains(&format!("DELETE {user} {held}")));
    sim.command(user, "verify", json!([])).await;
    assert!(sim.role_changes().is_empty());
    sim.app.drain(std::time::Duration::from_secs(10)).await;
    let joins: i64 = sqlx::query_scalar("SELECT count(*) FROM invite_joins WHERE guild_id = $1 AND user_id = $2")
        .bind(sim.guild as i64)
        .bind(user as i64)
        .fetch_one(sim.app.db.pool())
        .await
        .unwrap();
    assert_eq!(joins, 1, "verification must not attribute the join again");
    for attempt in 0..100 {
        pinchora_commands::stats_flush(&sim.app).await.unwrap();
        let hours = sim
            .app
            .db
            .guild_hours(sim.guild, chrono::Utc::now() - chrono::Duration::days(1))
            .await
            .unwrap();
        let counted: i64 = hours.iter().map(|h| h.joins).sum();
        if counted > 0 {
            assert_eq!(counted, 1, "held joins count once in stats");
            break;
        }
        assert!(attempt < 99, "held join never reached stats");
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    task.abort();
}
#[tokio::test]
async fn verification_captcha_card_modal_cooldown_and_owner_binding() {
    let Some((mut sim, _, task)) = setup("verification_captcha").await else {
        return;
    };
    config(&sim, "captcha").await;
    let user = unique();
    sim.join(user, "Mia", false, None).await;
    let card = sim.command(user, "verify", json!([])).await;
    assert_eq!(card.files.len(), 1);
    assert!(card.ephemeral);
    let mut modal = sim.click(user, &card, "Enter answer").await;
    let stolen = sim.submit_fields(unique(), &modal, &[("answer", "00000")]).await;
    assert!(stolen.content.unwrap().contains("expired"));
    for attempt in 0..3 {
        if attempt > 0 {
            let fresh = sim.command(user, "verify", json!([])).await;
            assert_eq!(fresh.files.len(), 1);
            assert_ne!(fresh.files[0].1, card.files[0].1);
            modal = sim.click(user, &fresh, "Enter answer").await;
        }
        let wrong = sim.submit_fields(user, &modal, &[("answer", "00000")]).await;
        assert!(
            wrong
                .content
                .unwrap()
                .contains(if attempt == 2 { "wait" } else { "did not match" })
        );
    }
    assert!(
        sim.app
            .db
            .pending_verification(sim.guild, user)
            .await
            .unwrap()
            .unwrap()
            .verified_at
            .is_none()
    );
    task.abort();
}
#[tokio::test]
async fn verification_captcha_consumes_once_and_attempts_survive_reissue() {
    let Some((sim, _, task)) = setup("verification_atomic").await else {
        return;
    };
    config(&sim, "captcha").await;
    let user = unique();
    sim.join(user, "Mia", false, None).await;
    let nonce = "0123456789abcdef0123456789abcdef";
    let hash = format!("{:x}", Sha256::digest(format!("{nonce}:A7K3M")));
    assert!(
        sim.app
            .cache
            .captcha_issue(sim.guild, user, nonce, &hash)
            .await
            .unwrap()
    );
    let (a, b) = tokio::join!(
        sim.app.cache.captcha_answer(sim.guild, user, nonce, &hash),
        sim.app.cache.captcha_answer(sim.guild, user, nonce, &hash)
    );
    let mut outcomes = vec![a.unwrap(), b.unwrap()];
    outcomes.sort();
    assert_eq!(outcomes, [-1, 1]);
    let user = unique();
    assert!(
        sim.app
            .cache
            .captcha_issue(sim.guild, user, nonce, &hash)
            .await
            .unwrap()
    );
    assert_eq!(
        sim.app
            .cache
            .captcha_answer(sim.guild, user, nonce, "wrong")
            .await
            .unwrap(),
        0
    );
    assert!(
        sim.app
            .cache
            .captcha_issue(sim.guild, user, nonce, &hash)
            .await
            .unwrap()
    );
    assert_eq!(
        sim.app
            .cache
            .captcha_answer(sim.guild, user, nonce, "wrong")
            .await
            .unwrap(),
        0
    );
    assert!(
        sim.app
            .cache
            .captcha_issue(sim.guild, user, nonce, &hash)
            .await
            .unwrap()
    );
    assert_eq!(
        sim.app
            .cache
            .captcha_answer(sim.guild, user, nonce, "wrong")
            .await
            .unwrap(),
        -2
    );
    assert!(
        !sim.app
            .cache
            .captcha_issue(sim.guild, user, "different", &hash)
            .await
            .unwrap()
    );
    task.abort();
}
#[tokio::test]
async fn verification_protects_staff_whitelist_and_owner() {
    let Some((sim, _, task)) = setup("verification_protected").await else {
        return;
    };
    let (mut c, _, _) = config(&sim, "captcha").await;
    let role = unique();
    sim.guild_role_with(role, "Moderator", 1, 1 << 40);
    let staff = unique();
    sim.mock.member_roles.lock().unwrap().insert(staff, vec![role]);
    let exempt = unique();
    c.verification.exempt_users.push(exempt.to_string());
    sim.app.db.save_safety(sim.guild, &c).await.unwrap();
    for user in [staff, exempt, APPLICATION_ID] {
        sim.join(user, "Member", false, None).await;
        assert!(
            sim.app
                .db
                .pending_verification(sim.guild, user)
                .await
                .unwrap()
                .is_none()
        );
    }
    let bot = unique();
    sim.join(bot, "FriendlyBot", true, None).await;
    assert!(sim.app.db.pending_verification(sim.guild, bot).await.unwrap().is_none());
    assert!(sim.role_changes().is_empty());
    task.abort();
}
#[tokio::test]
async fn verification_raid_start_is_idempotent_and_restores() {
    let Some((sim, mock, task)) = setup("verification_raid").await else {
        return;
    };
    let mut c = Config::default();
    c.raid.pause_invites = true;
    sim.app.db.save_safety(sim.guild, &c).await.unwrap();
    pinchora_commands::start_raid(&sim.app, sim.guild, None, 10, 12, "lockdown")
        .await
        .unwrap();
    let first = sim.app.db.active_raid(sim.guild).await.unwrap().unwrap();
    pinchora_commands::start_raid(&sim.app, sim.guild, None, 10, 12, "lockdown")
        .await
        .unwrap();
    assert_eq!(sim.app.db.raid_events(sim.guild).await.unwrap().len(), 1);
    assert_eq!(mock.patches.load(Ordering::SeqCst), 2);
    assert_eq!(mock.guild.lock().unwrap()["verification_level"], 3);
    pinchora_commands::unlock_raid(&sim.app, sim.guild, APPLICATION_ID, Some(first.id))
        .await
        .unwrap();
    assert_eq!(mock.guild.lock().unwrap()["verification_level"], 0);
    assert_eq!(mock.guild.lock().unwrap()["features"], json!([]));
    assert!(sim.app.db.active_raid(sim.guild).await.unwrap().is_none());
    task.abort();
}
#[tokio::test]
async fn verification_admin_paths_refuse_without_manage_server() {
    let Some((mut sim, _, task)) = setup("verification_permissions").await else {
        return;
    };
    sim.permissions = 0;
    let user = unique();
    for (name, options) in [
        (
            "verification",
            json!([sub("setup", vec![bool_option("enabled", false)])]),
        ),
        (
            "verification",
            json!([sub("mode", vec![text_option("mode", "button")])]),
        ),
        (
            "verification",
            json!([sub("message", vec![text_option("text", "Hello")])]),
        ),
        ("verification", json!([sub("post", vec![])])),
        ("raid", json!([sub("lockdown", vec![])])),
        ("raid", json!([sub("unlock", vec![])])),
        ("raid", json!([sub("status", vec![])])),
    ] {
        let reply = sim.command(user, name, options).await;
        assert!(reply.content.unwrap().contains("Manage Server"));
    }
    task.abort();
}
#[tokio::test]
async fn verification_success_resumes_after_verify_roles_and_timeout_kicks() {
    let Some((mut sim, _, task)) = setup("verification_release_timeout").await else {
        return;
    };
    let (mut c, verified, _) = config(&sim, "captcha").await;
    let after = unique();
    sim.guild_role(after, "After verification", 2);
    sim.app
        .db
        .save_auto_role(
            sim.guild,
            &pinchora_db::NewAutoRole {
                role_id: after,
                kind: "after_verify",
                delay_secs: 0,
                remove_after_secs: 0,
                enabled: true,
            },
            10,
        )
        .await
        .unwrap();
    let user = unique();
    sim.join(user, "Mia", false, None).await;
    sim.role_changes();
    let nonce = "1123456789abcdef0123456789abcdef";
    let hash = format!("{:x}", Sha256::digest(format!("{nonce}:A7K3M")));
    sim.app
        .cache
        .captcha_issue(sim.guild, user, nonce, &hash)
        .await
        .unwrap();
    let message = pinchora_commands::verification_answer(
        &sim.app,
        sim.guild,
        user,
        nonce,
        "a7k3m",
        pinchora_core::i18n::Lang::En,
    )
    .await
    .unwrap();
    assert!(message.contains("Verified"));
    let changes = sim.role_changes();
    assert!(changes.contains(&format!("PUT {user} {verified}")));
    assert!(changes.contains(&format!("PUT {user} {after}")));
    assert!(
        pinchora_commands::verification_answer(
            &sim.app,
            sim.guild,
            user,
            nonce,
            "a7k3m",
            pinchora_core::i18n::Lang::En
        )
        .await
        .unwrap()
        .contains("expired")
    );
    c.verification.kick_after_mins = 1;
    sim.app.db.save_safety(sim.guild, &c).await.unwrap();
    let late = unique();
    sim.join(late, "Late", false, None).await;
    pinchora_commands::verification_tick(&sim.app).await.unwrap();
    assert!(
        sim.app
            .db
            .pending_verification(sim.guild, late)
            .await
            .unwrap()
            .is_none()
    );
    let cases = sqlx::query_scalar::<_, i64>(
        "SELECT count(*) FROM mod_cases WHERE guild_id=$1 AND target_id=$2 AND kind='kick'",
    )
    .bind(sim.guild as i64)
    .bind(late as i64)
    .fetch_one(sim.app.db.pool())
    .await
    .unwrap();
    assert_eq!(cases, 1);
    // Staff controls still require permission at click time, even for a forged active raid id.
    sim.permissions = 0;
    let reply = Reply::new(InteractionResponseType::ChannelMessageWithSource);
    for id in ["rd:ban:1", "rd:kick:1", "rd:unlock:1"] {
        let refused = sim.component(user, id, &[], &reply).await;
        assert!(refused.content.unwrap().contains("Manage Server"));
    }
    task.abort();
}
#[tokio::test]
async fn verification_web_tokens_are_member_bound_and_expire() {
    let Some((sim, _, task)) = setup("verification_web_binding").await else {
        return;
    };
    config(&sim, "web").await;
    let user = unique();
    sim.join(user, "Mia", false, None).await;
    let joined = sim
        .app
        .db
        .pending_verification(sim.guild, user)
        .await
        .unwrap()
        .unwrap()
        .joined_at
        .timestamp_micros();
    let token = "2123456789abcdef0123456789abcdef";
    sim.app
        .cache
        .put(
            &format!("vf:web:{token}"),
            &format!("{}:{user}:{joined}", sim.guild),
            std::time::Duration::from_secs(600),
        )
        .await
        .unwrap();
    assert_eq!(
        pinchora_commands::verification_web_identity(&sim.app, token, user)
            .await
            .unwrap(),
        sim.guild
    );
    assert!(
        pinchora_commands::verification_web_identity(&sim.app, token, unique())
            .await
            .is_err()
    );
    sim.app.cache.forget(&format!("vf:web:{token}")).await.unwrap();
    assert!(
        pinchora_commands::verification_web_identity(&sim.app, token, user)
            .await
            .is_err()
    );
    task.abort();
}
#[tokio::test]
async fn verification_raid_window_expires_and_deduplicates() {
    let Some((sim, _, task)) = setup("verification_window").await else {
        return;
    };
    let cache = &sim.app.cache;
    let g = sim.guild;
    let now = sim.app.now().timestamp_millis();
    assert_eq!(cache.raid_window(g, 100, now, 30).await.unwrap(), 1);
    assert_eq!(cache.raid_window(g, 100, now, 30).await.unwrap(), 1);
    assert_eq!(cache.raid_window(g, 101, now + 1000, 30).await.unwrap(), 2);
    assert_eq!(cache.raid_window(g, 102, now + 31000, 30).await.unwrap(), 1);
    task.abort();
}
#[tokio::test]
async fn verification_lockdown_restores_channel_bits_and_preserves_staff_edits() {
    let Some((sim, _, task)) = setup("verification_restore_channels").await else {
        return;
    };
    let mut c = Config::default();
    c.raid.channels = vec![CHANNEL_ID.to_string()];
    c.raid.slowmode_secs = 30;
    c.raid.lock_channels = true;
    {
        let mut channels = sim.mock.channels.lock().unwrap();
        let channel = channels.get_mut(&CHANNEL_ID).unwrap();
        channel["guild_id"] = json!(sim.guild.to_string());
        channel["rate_limit_per_user"] = json!(5);
        channel["permission_overwrites"] = json!([{"id":sim.guild.to_string(),"type":0,"allow":"2048","deny":"0"}]);
    }
    sim.app.db.save_safety(sim.guild, &c).await.unwrap();
    pinchora_commands::start_raid(&sim.app, sim.guild, None, 10, 12, "lockdown")
        .await
        .unwrap();
    {
        let mut channels = sim.mock.channels.lock().unwrap();
        let ch = channels.get_mut(&CHANNEL_ID).unwrap();
        assert_eq!(ch["rate_limit_per_user"], 30);
        let deny = ch["permission_overwrites"][0]["deny"]
            .as_str()
            .unwrap()
            .parse::<u64>()
            .unwrap();
        assert_ne!(deny & 2048, 0);
        ch["permission_overwrites"][0]["deny"] = json!((deny | 1024).to_string());
    }
    pinchora_commands::unlock_raid(&sim.app, sim.guild, APPLICATION_ID, None)
        .await
        .unwrap();
    {
        let channels = sim.mock.channels.lock().unwrap();
        let ch = channels.get(&CHANNEL_ID).unwrap();
        assert_eq!(ch["rate_limit_per_user"], 5);
        assert_eq!(ch["permission_overwrites"][0]["allow"], "2048");
        assert_eq!(ch["permission_overwrites"][0]["deny"], "1024");
    }
    task.abort();
}
#[tokio::test]
async fn verification_busy_day_volume_only_alerts_without_punishment() {
    let Some((mut sim, _, task)) = setup("verification_detection").await else {
        return;
    };
    let (mut c, _, _) = config(&sim, "button").await;
    c.raid.enabled = true;
    c.raid.join_threshold = 2;
    c.raid.similar_threshold = 500;
    c.raid.action = "lockdown".into();
    sim.app.db.save_safety(sim.guild, &c).await.unwrap();
    let first = unique();
    let second = unique();
    sim.join(first, "Ada", false, None).await;
    sim.join(second, "Efe", false, None).await;
    assert!(sim.app.db.active_raid(sim.guild).await.unwrap().is_some());
    let card = sim.command(second, "verify", json!([])).await;
    assert!(card.files.is_empty());
    assert_eq!(
        sim.app.db.active_raid(sim.guild).await.unwrap().unwrap().action,
        "alert"
    );
    // An alert's debounce must not suppress a stronger correlated response in the same burst.
    c.raid.similar_threshold = 2;
    c.raid.new_account_hours = 0;
    sim.app.db.save_safety(sim.guild, &c).await.unwrap();
    for name in ["Raider1", "Raider2"] {
        let mut event = member_json(unique(), &[]);
        event["guild_id"] = json!(sim.guild.to_string());
        event["joined_at"] = json!(sim.app.now().to_rfc3339());
        event["user"]["username"] = json!(name);
        event["user"]["avatar"] = Value::Null;
        Box::pin(on_member_add(sim.app.clone(), serde_json::from_value(event).unwrap()))
            .await
            .unwrap();
    }
    assert_eq!(
        sim.app.db.active_raid(sim.guild).await.unwrap().unwrap().action,
        "lockdown"
    );
    pinchora_commands::unlock_raid(&sim.app, sim.guild, APPLICATION_ID, None)
        .await
        .unwrap();
    task.abort();
}

#[tokio::test]
async fn verification_unsupported_invite_pause_still_applies_channel_protection() {
    let Some((sim, mock, task)) = setup("verification_partial_lockdown").await else {
        return;
    };
    let (mut c, _, _) = config(&sim, "captcha").await;
    c.raid.channels = vec![CHANNEL_ID.to_string()];
    c.raid.slowmode_secs = 15;
    c.raid.pause_invites = true;
    sim.app.db.save_safety(sim.guild, &c).await.unwrap();
    mock.refuse_invites.store(true, Ordering::SeqCst);
    assert!(
        pinchora_commands::start_raid(&sim.app, sim.guild, Some(APPLICATION_ID), 10, 0, "lockdown")
            .await
            .is_err()
    );
    assert_eq!(
        sim.mock.channels.lock().unwrap()[&CHANNEL_ID]["rate_limit_per_user"],
        15
    );
    assert!(
        sim.app
            .db
            .active_raid(sim.guild)
            .await
            .unwrap()
            .unwrap()
            .error
            .is_some()
    );
    pinchora_commands::unlock_raid(&sim.app, sim.guild, APPLICATION_ID, None)
        .await
        .unwrap();
    assert_eq!(sim.mock.channels.lock().unwrap()[&CHANNEL_ID]["rate_limit_per_user"], 0);
    task.abort();
}

#[tokio::test]
async fn verification_removes_pregranted_and_racing_roles_and_rejoin_invalidates_proof() {
    let Some((mut sim, _, task)) = setup("verification_role_race").await else {
        return;
    };
    let (_, verified, held) = config(&sim, "captcha").await;
    let user = unique();
    sim.mock.member_roles.lock().unwrap().insert(user, vec![verified]);
    sim.join(user, "Mia", false, None).await;
    assert_eq!(sim.mock.member_roles.lock().unwrap()[&user], vec![held]);
    sim.mock.member_roles.lock().unwrap().insert(user, vec![verified]);
    sim.update_member(user, "Mia", None).await;
    assert_eq!(sim.mock.member_roles.lock().unwrap()[&user], vec![held]);
    let old = "a123456789abcdef0123456789abcdef";
    let hash = format!("{:x}", Sha256::digest(format!("{old}:A7K3M")));
    sim.app.cache.captcha_issue(sim.guild, user, old, &hash).await.unwrap();
    sim.leave(user, "Mia", false).await;
    sim.join(user, "Mia", false, None).await;
    assert!(
        pinchora_commands::verification_answer(&sim.app, sim.guild, user, old, "A7K3M", pinchora_core::i18n::Lang::En)
            .await
            .unwrap()
            .contains("expired")
    );
    let blank = Reply::new(InteractionResponseType::ChannelMessageWithSource);
    for id in ["vf:go:999", "cq:a:a123456789abcdef0123456789abcdef:999"] {
        assert!(
            sim.component(user, id, &[], &blank)
                .await
                .content
                .unwrap()
                .contains("expired")
        );
    }
    assert!(
        sim.app
            .db
            .pending_verification(sim.guild, user)
            .await
            .unwrap()
            .unwrap()
            .verified_at
            .is_none()
    );
    task.abort();
}

#[tokio::test]
async fn verification_restore_skips_failed_and_unchanged_channels_and_preserves_empty_overwrite() {
    let Some((sim, mock, task)) = setup("verification_snapshot_ownership").await else {
        return;
    };
    let mut c = Config::default();
    c.raid.channels = vec![CHANNEL_ID.to_string()];
    c.raid.slowmode_secs = 0;
    {
        let mut channels = sim.mock.channels.lock().unwrap();
        let ch = channels.get_mut(&CHANNEL_ID).unwrap();
        ch["guild_id"] = json!(sim.guild.to_string());
        ch["permission_overwrites"] = json!([{"id":sim.guild.to_string(),"type":0,"allow":"0","deny":"0"}]);
    }
    sim.app.db.save_safety(sim.guild, &c).await.unwrap();
    pinchora_commands::start_raid(&sim.app, sim.guild, None, 10, 0, "lockdown")
        .await
        .unwrap();
    pinchora_commands::unlock_raid(&sim.app, sim.guild, APPLICATION_ID, None)
        .await
        .unwrap();
    assert_eq!(
        mock.channel_writes.load(Ordering::SeqCst),
        0,
        "unchanged channels must not be written"
    );
    c.raid.lock_channels = true;
    c.raid.slowmode_secs = 15;
    sim.app.db.save_safety(sim.guild, &c).await.unwrap();
    mock.refuse_channels.store(true, Ordering::SeqCst);
    assert!(
        pinchora_commands::start_raid(&sim.app, sim.guild, None, 10, 0, "lockdown")
            .await
            .is_err()
    );
    let e = sim.app.db.active_raid(sim.guild).await.unwrap().unwrap();
    assert_eq!(e.snapshot["channels"][0]["slow_done"], false);
    assert_eq!(e.snapshot["channels"][0]["lock_done"], false);
    // Another moderator independently sets the same slowmode after the bot's write failed.
    sim.mock.channels.lock().unwrap().get_mut(&CHANNEL_ID).unwrap()["rate_limit_per_user"] = json!(15);
    mock.refuse_channels.store(false, Ordering::SeqCst);
    let writes = mock.channel_writes.load(Ordering::SeqCst);
    pinchora_commands::unlock_raid(&sim.app, sim.guild, APPLICATION_ID, Some(e.id))
        .await
        .unwrap();
    assert_eq!(mock.channel_writes.load(Ordering::SeqCst), writes);
    assert_eq!(
        sim.mock.channels.lock().unwrap()[&CHANNEL_ID]["rate_limit_per_user"],
        15
    );
    // A later successful lock restores the originally present, empty overwrite exactly.
    pinchora_commands::start_raid(&sim.app, sim.guild, None, 10, 0, "lockdown")
        .await
        .unwrap();
    let saved = sim.app.db.active_raid(sim.guild).await.unwrap().unwrap();
    assert_eq!(saved.snapshot["channels"][0]["lock_done"], true);
    // Unlock consumes only the persisted snapshot: no start-worker memory is needed.
    pinchora_commands::unlock_raid(&sim.app, sim.guild, APPLICATION_ID, Some(saved.id))
        .await
        .unwrap();
    let ch = sim.mock.channels.lock().unwrap()[&CHANNEL_ID].clone();
    assert_eq!(
        ch["permission_overwrites"],
        json!([{"id":sim.guild.to_string(),"type":0,"allow":"0","deny":"0"}])
    );
    task.abort();
}

#[tokio::test]
async fn verification_slow_drip_correlated_joins_force_captcha_but_youth_alone_does_not() {
    let Some((sim, mock, task)) = setup("verification_slow_drip").await else {
        return;
    };
    let (mut c, _, _) = config(&sim, "button").await;
    c.raid.enabled = true;
    c.raid.join_threshold = 500;
    c.raid.similar_threshold = 2;
    c.raid.action = "lockdown".into();
    c.raid.new_account_hours = 0;
    c.raid.pause_invites = true;
    mock.refuse_invites.store(true, Ordering::SeqCst);
    sim.app.db.save_safety(sim.guild, &c).await.unwrap();
    for (i, name) in ["Raider1", "Raider2", "Raider3"].into_iter().enumerate() {
        sim.app
            .db
            .note_raid_join(&pinchora_db::RaidJoin {
                guild_id: sim.guild as i64,
                user_id: unique() as i64,
                joined_at: sim.app.now() - chrono::Duration::seconds(50 + i as i64 * 40),
                name_family: pinchora_core::anti_raid::name_family(name),
                avatar: None,
                young: true,
                status: "joined".into(),
            })
            .await
            .unwrap();
    }
    let user = unique();
    let mut event = member_json(user, &[]);
    event["guild_id"] = json!(sim.guild.to_string());
    event["joined_at"] = json!(sim.app.now().to_rfc3339());
    event["user"]["username"] = json!("Raider4");
    Box::pin(on_member_add(sim.app.clone(), serde_json::from_value(event).unwrap()))
        .await
        .unwrap();
    assert_eq!(
        sim.app.db.active_raid(sim.guild).await.unwrap().unwrap().action,
        "lockdown"
    );
    assert!(
        sim.app
            .db
            .pending_verification(sim.guild, user)
            .await
            .unwrap()
            .is_some(),
        "a partial lockdown must still let its triggering member verify"
    );
    pinchora_commands::unlock_raid(&sim.app, sim.guild, APPLICATION_ID, None)
        .await
        .unwrap();
    task.abort();
}

#[tokio::test]
async fn verification_interrupted_discord_write_remains_durable_for_reconciliation() {
    let Some((sim, _, task)) = setup("verification_interrupted_write").await else {
        return;
    };
    let now = sim.app.now();
    let event=sim.app.db.open_raid(sim.guild,0,"lockdown",now,now,&json!({"level":0,"raised":true,"paused":false,"channels":[],"window_secs":30,"uncertain":["verification_level"]})).await.unwrap().unwrap();
    assert!(
        pinchora_commands::unlock_raid(&sim.app, sim.guild, APPLICATION_ID, Some(event.id))
            .await
            .is_err()
    );
    let retained = sim.app.db.active_raid(sim.guild).await.unwrap().unwrap();
    assert!(retained.error.unwrap().contains("reconcile"));
    // Simulate a moderator reconciling the unknown write; never silently claim it was restored.
    sim.app
        .db
        .raid_snapshot(
            event.id,
            &json!({"level":0,"raised":false,"paused":false,"channels":[],"window_secs":30}),
        )
        .await
        .unwrap();
    pinchora_commands::unlock_raid(&sim.app, sim.guild, APPLICATION_ID, Some(event.id))
        .await
        .unwrap();
    task.abort();
}

#[tokio::test]
async fn verification_old_button_panel_cannot_bypass_forced_captcha() {
    let Some((mut sim, _, task)) = setup("verification_forced_button").await else {
        return;
    };
    config(&sim, "button").await;
    let user = unique();
    sim.join(user, "Mia", false, None).await;
    pinchora_commands::start_raid(&sim.app, sim.guild, None, 10, 0, "lockdown")
        .await
        .unwrap();
    let panel = Reply::new(InteractionResponseType::ChannelMessageWithSource);
    let card = sim.component(user, "vf:go", &[], &panel).await;
    assert_eq!(card.files.len(), 1);
    assert!(
        sim.app
            .db
            .pending_verification(sim.guild, user)
            .await
            .unwrap()
            .unwrap()
            .verified_at
            .is_none()
    );
    pinchora_commands::unlock_raid(&sim.app, sim.guild, APPLICATION_ID, None)
        .await
        .unwrap();
    task.abort();
}

#[tokio::test]
async fn verification_newly_exempt_timeout_is_retired_and_admin_changes_are_logged() {
    let Some((mut sim, _, task)) = setup("verification_exempt_timeout").await else {
        return;
    };
    let (mut c, _, _) = config(&sim, "captcha").await;
    c.verification.kick_after_mins = 1;
    sim.app.db.save_safety(sim.guild, &c).await.unwrap();
    let user = unique();
    sim.join(user, "Mia", false, None).await;
    c.verification.exempt_users.push(user.to_string());
    sim.app.db.save_safety(sim.guild, &c).await.unwrap();
    pinchora_commands::verification_tick(&sim.app).await.unwrap();
    assert!(
        sim.app
            .db
            .pending_verification(sim.guild, user)
            .await
            .unwrap()
            .unwrap()
            .expires_at
            .is_none()
    );
    let cases: i64 = sqlx::query_scalar("SELECT count(*) FROM mod_cases WHERE guild_id=$1 AND target_id=$2")
        .bind(sim.guild as i64)
        .bind(user as i64)
        .fetch_one(sim.app.db.pool())
        .await
        .unwrap();
    assert_eq!(cases, 0);
    sim.permissions = 1 << 5;
    let reply = sim
        .command(
            APPLICATION_ID,
            "verification",
            json!([sub("message", vec![text_option("text", "Verify here")])]),
        )
        .await;
    assert!(reply.content.unwrap().contains("saved"));
    let rows: i64 =
        sqlx::query_scalar("SELECT count(*) FROM staff_actions WHERE guild_id=$1 AND action='verification.set'")
            .bind(sim.guild as i64)
            .fetch_one(sim.app.db.pool())
            .await
            .unwrap();
    assert_eq!(rows, 1);
    task.abort();
}

#[tokio::test]
async fn verification_partial_restore_retry_does_not_rewrite_completed_operations() {
    let Some((sim, mock, task)) = setup("verification_restore_retry").await else {
        return;
    };
    let mut c = Config::default();
    c.raid.channels = vec![CHANNEL_ID.to_string()];
    c.raid.slowmode_secs = 30;
    sim.mock.channels.lock().unwrap().get_mut(&CHANNEL_ID).unwrap()["guild_id"] = json!(sim.guild.to_string());
    sim.app.db.save_safety(sim.guild, &c).await.unwrap();
    pinchora_commands::start_raid(&sim.app, sim.guild, None, 10, 0, "lockdown")
        .await
        .unwrap();
    mock.refuse_channels.store(true, Ordering::SeqCst);
    assert!(
        pinchora_commands::unlock_raid(&sim.app, sim.guild, APPLICATION_ID, None)
            .await
            .is_err()
    );
    assert_eq!(mock.guild.lock().unwrap()["verification_level"], 0);
    assert_eq!(
        sim.app.db.active_raid(sim.guild).await.unwrap().unwrap().snapshot["raised_done"],
        false
    );
    // Staff re-raises verification while the channel restore is waiting for permission.
    mock.guild.lock().unwrap()["verification_level"] = json!(3);
    mock.refuse_channels.store(false, Ordering::SeqCst);
    pinchora_commands::unlock_raid(&sim.app, sim.guild, APPLICATION_ID, None)
        .await
        .unwrap();
    assert_eq!(mock.guild.lock().unwrap()["verification_level"], 3);
    assert_eq!(sim.mock.channels.lock().unwrap()[&CHANNEL_ID]["rate_limit_per_user"], 0);
    task.abort();
}
