//! through real gateway/interaction handlers, real Postgres/Redis and mock Discord REST.
use crate::*;
use pinchora_commands::{on_automod_execution, sync_automod};
use pinchora_core::moderation::automod as rules;
use pinchora_core::moderation::automod::Action;
use std::time::Duration;

#[derive(Clone)]
struct NativeMock {
    base: Mock,
    rules: Arc<Mutex<Vec<Value>>>,
    mutations: Arc<AtomicUsize>,
}
async fn native_discord(
    State(mock): State<NativeMock>,
    method: Method,
    uri: Uri,
    headers: HeaderMap,
    body: Bytes,
) -> (StatusCode, String) {
    let parts: Vec<_> = uri.path().trim_matches('/').split('/').collect();
    if let Some(pos) = parts.iter().position(|p| *p == "auto-moderation") {
        let mut rules = mock.rules.lock().unwrap();
        let id = parts.get(pos + 2).copied();
        if method == Method::GET {
            return (StatusCode::OK, json!(*rules).to_string());
        }
        mock.mutations.fetch_add(1, Ordering::SeqCst);
        if method == Method::DELETE {
            rules.retain(|r| r["id"].as_str() != id);
            return (StatusCode::NO_CONTENT, String::new());
        }
        let mut value: Value = serde_json::from_slice(&body).unwrap();
        if let Some(id) = id {
            let old = rules.iter_mut().find(|r| r["id"] == id).unwrap();
            for (k, v) in value.as_object().unwrap() {
                old[k] = v.clone();
            }
            return (StatusCode::OK, old.to_string());
        }
        value["id"] = json!(unique().to_string());
        value["guild_id"] = json!(parts[pos - 1]);
        value["creator_id"] = json!(APPLICATION_ID.to_string());
        rules.push(value.clone());
        return (StatusCode::OK, value.to_string());
    }
    let (status, body) = discord(State(mock.base), method.clone(), uri.clone(), headers, body).await;
    // The incumbent mock deliberately gives its bot no permissions. This extension grants only
    // the permissions under test, without changing any other package's fixtures.
    if method == Method::GET && uri.path().ends_with("/roles") && status == StatusCode::OK {
        let mut value: Value = serde_json::from_str(&body).unwrap();
        value[0]["permissions"] =
            json!(((1u64 << 5) | (1 << 13) | (1 << 40) | (1 << 1) | (1 << 2) | (1 << 28)).to_string());
        return (status, value.to_string());
    }
    (status, body)
}
async fn native_sim(name: &'static str) -> Option<(Sim, NativeMock, tokio::task::JoinHandle<()>)> {
    let mut sim = Sim::new(name).await?;
    sim.channel_is(CHANNEL_ID, 0, None);
    let mock = NativeMock {
        base: sim.mock.clone(),
        rules: Arc::default(),
        mutations: Arc::default(),
    };
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let router = Router::new().fallback(native_discord).with_state(mock.clone());
    let task = tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });
    let http = Arc::new(
        Client::builder()
            .token("Bot sim".into())
            .proxy(addr.to_string(), true)
            .ratelimiter(None)
            .build(),
    );
    Arc::get_mut(&mut sim.app).unwrap().http = http;
    Some((sim, mock, task))
}
async fn config(sim: &Sim, kind: &str, dry: bool) -> rules::Config {
    let mut c = rules::profile("strict", sim.app.now()).unwrap();
    c.rules.retain(|r| r.kind == kind);
    c.rules[0].enabled = true;
    if kind == "honeypot" {
        c.rules[0].only_channels = vec![CHANNEL_ID.to_string()];
    }
    c.rules[0].native = false;
    c.rules[0].log_only = dry;
    c.alert_channel_id = Some(unique().to_string());
    sim.app.db.save_automod(sim.guild, unique(), &c).await.unwrap();
    sim.app.db.automod_config(sim.guild).await.unwrap()
}
async fn message(sim: &Sim, user: u64, text: &str, roles: &[u64]) -> u64 {
    let mut message = sim.chat(user, roles);
    message.content = text.into();
    let id = message.id.get();
    on_message(sim.app.clone(), message).await.unwrap();
    id
}
async fn settled(sim: &Sim) {
    sim.app.drain(Duration::from_secs(10)).await;
}

#[tokio::test]
async fn automod_never_punishes_staff_below_bot() {
    let Some((sim, _, task)) = native_sim("automod_staff_guard").await else {
        return;
    };
    let mut c = config(&sim, "honeypot", false).await;
    c.rules[0].actions = vec![Action::new("ban")];
    sim.app.db.save_automod(sim.guild, unique(), &c).await.unwrap();
    for power in [1 << 3, 1 << 5, 1 << 13, 1 << 40] {
        let user = unique();
        let role = unique();
        sim.guild_role_with(role, "Staff", 1, power);
        sim.app.forget_permission_facts(sim.guild).await;
        sim.mock.member_roles.lock().unwrap().insert(user, vec![role]);
        message(&sim, user, "ordinary message", &[role]).await;
        settled(&sim).await;
        assert!(!sim.mock.bans.lock().unwrap().contains(&user));
    }
    assert!(sim.app.db.automod_hits(sim.guild).await.unwrap().is_empty());
    task.abort();
}

#[tokio::test]
async fn automod_honeypot_respects_exempt_category() {
    let Some((sim, _, task)) = native_sim("automod_exempt_category").await else {
        return;
    };
    let mut c = config(&sim, "honeypot", false).await;
    let category = unique();
    sim.channel_is(category, 4, None);
    sim.channel_is(CHANNEL_ID, 0, Some(category));
    c.rules[0].exempt_channels = vec![category.to_string()];
    c.rules[0].actions = vec![Action::new("ban")];
    sim.app.db.save_automod(sim.guild, unique(), &c).await.unwrap();
    let user = unique();
    message(&sim, user, "message", &[]).await;
    settled(&sim).await;
    assert!(!sim.mock.bans.lock().unwrap().contains(&user));
    assert!(sim.app.db.automod_hits(sim.guild).await.unwrap().is_empty());
    task.abort();
}

#[tokio::test]
async fn automod_legacy_native_url_false_positive_cannot_ban() {
    let Some((sim, _, task)) = native_sim("automod_legacy_native_url").await else {
        return;
    };
    let mut c = config(&sim, "links", false).await;
    c.rules[0].native = true;
    c.rules[0].params.block_domains = vec!["x.com".into()];
    c.rules[0].actions = vec![Action::new("ban")];
    sim.app.db.save_automod(sim.guild, unique(), &c).await.unwrap();
    let rule_id: i64 = c.rules[0].id.as_deref().unwrap().parse().unwrap();
    let old_native = unique();
    sim.app
        .db
        .set_automod_native(sim.guild, rule_id, Some(old_native))
        .await
        .unwrap();
    let user = unique();
    let event = serde_json::from_value(json!({
        "action":{"type":2,"metadata":{"channel_id":c.alert_channel_id}},
        "guild_id":sim.guild.to_string(),"channel_id":CHANNEL_ID.to_string(),
        "user_id":user.to_string(),"rule_id":old_native.to_string(),"rule_trigger_type":1,
        "content":"https://safe.example/docs/x.com","message_id":unique().to_string()
    }))
    .unwrap();
    on_automod_execution(sim.app.clone(), event).await.unwrap();
    assert!(!sim.mock.bans.lock().unwrap().contains(&user));
    assert!(sim.app.db.automod_hits(sim.guild).await.unwrap().is_empty());
    task.abort();
}

#[tokio::test]
async fn automod_sync_preserves_later_drift() {
    let Some(sim) = Sim::new("automod_sync_generation").await else {
        return;
    };
    config(&sim, "mentions", false).await;
    let status = sim.app.db.automod_sync_status(sim.guild).await.unwrap().unwrap();
    sim.app.db.queue_automod_sync(sim.guild).await.unwrap();
    sim.app
        .db
        .finish_automod_sync(sim.guild, status.revision, status.generation, None)
        .await
        .unwrap();
    let queued: Option<chrono::DateTime<chrono::Utc>> =
        sqlx::query_scalar("SELECT sync_after FROM automod_settings WHERE guild_id=$1")
            .bind(sim.guild as i64)
            .fetch_one(sim.app.db.pool())
            .await
            .unwrap();
    assert!(queued.is_some());
}

#[tokio::test]
async fn automod_off_config_is_served_without_database_after_warmup() {
    let Some(sim) = Sim::new("automod_off_cache").await else {
        return;
    };
    assert_eq!(sim.app.automod_config(sim.guild).await.unwrap().profile, "off");
    let mut tx = sim.app.db.pool().begin().await.unwrap();
    sqlx::query("LOCK TABLE automod_settings IN ACCESS EXCLUSIVE MODE")
        .execute(&mut *tx)
        .await
        .unwrap();
    let cached = tokio::time::timeout(Duration::from_millis(250), sim.app.automod_config(sim.guild)).await;
    assert_eq!(cached.unwrap().unwrap().profile, "off");
    tx.rollback().await.unwrap();
    let active = rules::profile("strict", sim.app.now()).unwrap();
    sim.app.db.save_automod(sim.guild, unique(), &active).await.unwrap();
    assert_eq!(sim.app.automod_config(sim.guild).await.unwrap().profile, "off");
    sim.app.forget_automod_config(sim.guild).await;
    assert_eq!(sim.app.automod_config(sim.guild).await.unwrap().profile, "strict");
}

#[tokio::test]
async fn automod_alert_action_claim_is_durable() {
    let Some(sim) = Sim::new("automod_action_claim").await else {
        return;
    };
    let c = config(&sim, "honeypot", false).await;
    let hit = pinchora_db::AutoModHit {
        id: 0,
        guild_id: sim.guild as i64,
        rule_id: c.rules[0].id.as_deref().and_then(|s| s.parse().ok()),
        user_id: unique() as i64,
        channel_id: CHANNEL_ID as i64,
        message_id: Some(unique() as i64),
        action: "pending".into(),
        snippet: None,
        created_at: sim.app.now(),
        reviewed: false,
    };
    let hit = sim.app.db.open_automod_hit(&hit).await.unwrap().unwrap();
    assert!(sim.app.db.claim_automod_action(sim.guild, hit.id, "ban").await.unwrap());
    sim.app.shift_clock(chrono::Duration::seconds(31));
    assert!(!sim.app.db.claim_automod_action(sim.guild, hit.id, "ban").await.unwrap());
    assert!(
        sim.app
            .db
            .claim_automod_action(sim.guild, hit.id, "delete")
            .await
            .unwrap()
    );
}

#[tokio::test]
async fn automod_profile_sync_is_bounded_and_drift_is_repaired_without_rewriting_unchanged_rules() {
    let Some((mut sim, mock, task)) = native_sim("automod_native_profile").await else {
        return;
    };
    sim.permissions = MANAGE_GUILD;
    let reply = sim
        .command(
            unique(),
            "automod",
            json!([sub("profile", vec![text_option("name", "standard")])]),
        )
        .await;
    assert!(reply.ephemeral && reply.content().contains("48"));
    let mut c = sim.app.db.automod_config(sim.guild).await.unwrap();
    c.alert_channel_id = Some(unique().to_string());
    sim.app.db.save_automod(sim.guild, unique(), &c).await.unwrap();
    sync_automod(&sim.app, sim.guild).await.unwrap();
    let saved = sim.app.db.automod_config(sim.guild).await.unwrap();
    assert!(saved.rules.iter().any(|r| r.discord_rule_id.is_some()));
    assert!(mock.rules.lock().unwrap().iter().all(|r| r["actions"][0]["type"] == 2));
    let count = mock.mutations.load(Ordering::SeqCst);
    sync_automod(&sim.app, sim.guild).await.unwrap();
    assert_eq!(count, mock.mutations.load(Ordering::SeqCst));
    mock.rules.lock().unwrap()[0]["enabled"] = json!(false);
    sync_automod(&sim.app, sim.guild).await.unwrap();
    assert_eq!(count + 1, mock.mutations.load(Ordering::SeqCst));
    task.abort();
}

#[tokio::test]
async fn automod_hit_deletes_opens_warn_case_alerts_and_blocks_xp() {
    let Some((sim, _, task)) = native_sim("automod_hit_actions").await else {
        return;
    };
    let c = config(&sim, "caps", false).await;
    let user = unique();
    let id = message(&sim, user, "THIS IS A LOUD MESSAGE", &[]).await;
    settled(&sim).await;
    let hits = sim.app.db.automod_hits(sim.guild).await.unwrap();
    assert_eq!(hits.len(), 1);
    assert!(hits[0].action.contains("warn"), "{}", hits[0].action);
    assert!(sim.mock.deleted_messages.lock().unwrap().contains(&(CHANNEL_ID, id)));
    let cases = sim
        .app
        .db
        .cases(sim.guild, &pinchora_db::CaseFilter::default())
        .await
        .unwrap();
    assert_eq!(cases.len(), 1);
    assert_eq!(cases[0].kind, "warn");
    assert!(sim.mock.posts.lock().unwrap().iter().any(|p| {
        p.channel == c.alert_channel_id.as_ref().unwrap().parse::<u64>().unwrap()
            && p.body["components"][0]["components"][0]["custom_id"]
                .as_str()
                .unwrap()
                .starts_with("am:del:")
    }));
    let xp: Option<i64> = sqlx::query_scalar("SELECT xp FROM members WHERE guild_id=$1 AND user_id=$2")
        .bind(sim.guild as i64)
        .bind(user as i64)
        .fetch_optional(sim.app.db.pool())
        .await
        .unwrap();
    assert!(xp.is_none_or(|xp| xp == 0));
    task.abort();
}

#[tokio::test]
async fn automod_log_only_and_standard_observation_do_not_add_strikes_or_cases() {
    let Some((sim, _, task)) = native_sim("automod_dry_run").await else {
        return;
    };
    config(&sim, "caps", true).await;
    message(&sim, unique(), "THIS IS A LOUD MESSAGE", &[]).await;
    settled(&sim).await;
    let hits = sim.app.db.automod_hits(sim.guild).await.unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].action, "log_only");
    assert!(sim.mock.deleted_messages.lock().unwrap().is_empty());
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM automod_strikes WHERE guild_id=$1")
        .bind(sim.guild as i64)
        .fetch_one(sim.app.db.pool())
        .await
        .unwrap();
    assert_eq!(count, 0);
    assert!(
        sim.app
            .db
            .cases(sim.guild, &pinchora_db::CaseFilter::default())
            .await
            .unwrap()
            .is_empty()
    );
    task.abort();
}

#[tokio::test]
async fn automod_content_flag_owner_bots_webhooks_and_exemptions_are_respected() {
    let Some((mut sim, _, task)) = native_sim("automod_protection").await else {
        return;
    };
    Arc::get_mut(&mut sim.app).unwrap().message_content = false;
    let owner = unique();
    sim.own_server(owner);
    config(&sim, "caps", false).await;
    message(&sim, unique(), "THIS IS A LOUD MESSAGE", &[]).await;
    assert!(sim.app.db.automod_hits(sim.guild).await.unwrap().is_empty());
    let mut c = config(&sim, "honeypot", false).await; // configured below before any message
    c.rules[0].only_channels = vec![CHANNEL_ID.to_string()];
    let exempt = unique();
    c.rules[0].exempt_roles = vec![exempt.to_string()];
    sim.app.db.save_automod(sim.guild, unique(), &c).await.unwrap();
    sim.app.forget_automod_config(sim.guild).await;
    message(&sim, owner, "hello", &[]).await;
    message(&sim, unique(), "hello", &[exempt]).await;
    let mut bot = sim.chat(unique(), &[]);
    bot.author.bot = true;
    on_message(sim.app.clone(), bot).await.unwrap();
    let mut webhook = sim.chat(unique(), &[]);
    webhook.webhook_id = Some(Id::new(unique()));
    on_message(sim.app.clone(), webhook).await.unwrap();
    assert!(sim.app.db.automod_hits(sim.guild).await.unwrap().is_empty());
    task.abort();
}

#[tokio::test]
async fn automod_admin_paths_and_alert_clicks_refuse_without_manage_server() {
    let Some(mut sim) = Sim::new("automod_admin_paths").await else {
        return;
    };
    let user = unique();
    let exempt_role = sim.guild_role(99, "Exempt", 1);
    let known = json!({"roles": resolved(&[], &[&exempt_role])["roles"], "channels": resolved_channels(&[CHANNEL_ID])["channels"]});
    for option in [
        sub("show", vec![]),
        sub("profile", vec![text_option("name", "strict")]),
        sub(
            "toggle",
            vec![text_option("rule", "caps"), bool_option("enabled", true)],
        ),
        sub("test", vec![text_option("text", "hi")]),
        group(
            "exempt",
            sub(
                "role",
                vec![
                    text_option("action", "add"),
                    json!({"name":"role","type":8,"value":"99"}),
                ],
            ),
        ),
        group(
            "exempt",
            sub(
                "channel",
                vec![
                    text_option("action", "add"),
                    json!({"name":"channel","type":7,"value":CHANNEL_ID.to_string()}),
                ],
            ),
        ),
    ] {
        let reply = sim
            .command_resolved(user, "automod", json!([option]), known.clone())
            .await;
        assert!(reply.ephemeral && reply.content().contains("Manage Server"));
    }
    let empty = sim.command(user, "automod", json!([sub("show", vec![])])).await;
    for id in ["am:del:1", "am:to:1", "am:ban:1", "am:ok:1", "ax:c:1:1", "ax:r:1:1"] {
        let reply = sim.component(user, id, &[], &empty).await;
        assert!(reply.ephemeral && reply.content().contains("Manage Server"));
    }
}

#[tokio::test]
async fn automod_windows_roll_forward_atomically_and_duplicate_hashes_do_not_mix() {
    let Some(sim) = Sim::new("automod_windows").await else {
        return;
    };
    let user = unique();
    let now = sim.app.now().timestamp_millis();
    assert_eq!(
        sim.app
            .cache
            .automod_window(sim.guild, user, "1", 1, now, 10, None)
            .await
            .unwrap(),
        1
    );
    assert_eq!(
        sim.app
            .cache
            .automod_window(sim.guild, user, "1", 2, now + 9999, 10, None)
            .await
            .unwrap(),
        2
    );
    assert_eq!(
        sim.app
            .cache
            .automod_window(sim.guild, user, "1", 3, now + 10000, 10, None)
            .await
            .unwrap(),
        2
    );
    for (id, hash, expected) in [(1, 7, 1), (2, 8, 1), (3, 7, 2), (3, 7, 2)] {
        assert_eq!(
            sim.app
                .cache
                .automod_window(sim.guild, user, "2", id, now, 10, Some(hash))
                .await
                .unwrap(),
            expected
        );
    }
    assert_eq!(
        sim.app
            .db
            .automod_strike(sim.guild, user, sim.app.now(), 10)
            .await
            .unwrap(),
        1
    );
    assert_eq!(
        sim.app
            .db
            .automod_strike(sim.guild, user, sim.app.now() + chrono::Duration::seconds(10), 10)
            .await
            .unwrap(),
        1
    );
}

#[tokio::test]
async fn automod_native_event_without_content_creates_hit_and_case_once() {
    let Some((mut sim, _, task)) = native_sim("automod_native_execution").await else {
        return;
    };
    Arc::get_mut(&mut sim.app).unwrap().message_content = false;
    let mut c = config(&sim, "mentions", false).await;
    c.rules[0].native = true;
    c.rules[0].actions = vec![Action::new("warn")];
    sim.app.db.save_automod(sim.guild, unique(), &c).await.unwrap();
    sync_automod(&sim.app, sim.guild).await.unwrap();
    let c = sim.app.db.automod_config(sim.guild).await.unwrap();
    let event:twilight_model::gateway::payload::incoming::AutoModerationActionExecution=serde_json::from_value(json!({"action":{"type":2,"metadata":{"channel_id":c.alert_channel_id}},"guild_id":sim.guild.to_string(),"channel_id":CHANNEL_ID.to_string(),"user_id":unique().to_string(),"rule_id":c.rules[0].discord_rule_id,"rule_trigger_type":5,"content":"","message_id":unique().to_string()})).unwrap();
    on_automod_execution(sim.app.clone(), event.clone()).await.unwrap();
    on_automod_execution(sim.app.clone(), event).await.unwrap();
    let hits = sim.app.db.automod_hits(sim.guild).await.unwrap();
    assert_eq!(hits.len(), 1);
    assert!(hits[0].snippet.is_none());
    assert_eq!(
        sim.app
            .db
            .cases(sim.guild, &pinchora_db::CaseFilter::default())
            .await
            .unwrap()
            .len(),
        1
    );
    task.abort();
}

#[tokio::test]
async fn automod_hierarchy_role_safety_escalation_and_warn_ladder() {
    let Some((sim, _, task)) = native_sim("automod_hierarchy_escalation").await else {
        return;
    };
    let mut c = config(&sim, "caps", false).await;
    c.rules[0].actions = vec![Action::new("timeout"), Action::new("kick"), Action::new("ban")];
    sim.app.db.save_automod(sim.guild, unique(), &c).await.unwrap();
    let high = unique();
    let top_role = unique();
    sim.guild_role(top_role, "High", BOT_ROLE_POSITION + 1);
    sim.mock.member_roles.lock().unwrap().insert(high, vec![top_role]);
    message(&sim, high, "LOUD MESSAGE ABOVE THE BOT", &[top_role]).await;
    settled(&sim).await;
    assert!(sim.mock.bans.lock().unwrap().is_empty());
    assert!(
        sim.app
            .db
            .cases(sim.guild, &pinchora_db::CaseFilter::default())
            .await
            .unwrap()
            .is_empty()
    );
    let danger = unique();
    sim.guild_role_with(danger, "Admin", 1, 1 << 3);
    let mut role = Action::new("add_role");
    role.role = Some(danger.to_string());
    c.rules[0].actions = vec![role];
    assert!(
        pinchora_commands::check_automod_roles(&sim.app, sim.guild, unique(), &c)
            .await
            .is_err()
    );
    sim.app.db.save_automod(sim.guild, unique(), &c).await.unwrap(); // proves runtime safety after a role's powers change
    sim.app.forget_automod_config(sim.guild).await;
    message(&sim, unique(), "DANGEROUS ROLE MUST NOT BE GIVEN", &[]).await;
    settled(&sim).await;
    assert!(sim.mock.role_calls.lock().unwrap().is_empty());
    c.rules[0].actions = vec![Action::new("warn")];
    c.escalation = vec![rules::Escalation {
        strikes: 2,
        window_secs: 3600,
        action: Action::new("timeout"),
    }];
    sim.app.db.save_automod(sim.guild, unique(), &c).await.unwrap();
    sim.app.forget_automod_config(sim.guild).await;
    sim.app
        .db
        .replace_warn_thresholds(
            sim.guild,
            &[
                pinchora_core::moderation::moderation::check_threshold(
                    1,
                    "timeout",
                    Some(chrono::Duration::minutes(2)),
                )
                .unwrap(),
            ],
        )
        .await
        .unwrap();
    let user = unique();
    message(&sim, user, "FIRST WARNING HAS A LADDER", &[]).await;
    settled(&sim).await;
    message(&sim, user, "SECOND WARNING HAS ESCALATION", &[]).await;
    settled(&sim).await;
    let cases = sim
        .app
        .db
        .cases(
            sim.guild,
            &pinchora_db::CaseFilter {
                target: Some(user),
                limit: 100,
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(cases.iter().filter(|c| c.kind == "warn").count(), 2);
    assert_eq!(cases.iter().filter(|c| c.kind == "timeout").count(), 2);
    sim.app.db.reset_automod_strikes(sim.guild, user).await.unwrap();
    assert_eq!(
        sim.app
            .db
            .automod_strike(sim.guild, user, sim.app.now(), 3600)
            .await
            .unwrap(),
        1
    );
    task.abort();
}
