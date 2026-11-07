//! F11 uses real Postgres/Redis and a Discord REST mock, including recovery requests.
use super::*;
use pinchora_commands::{on_antinuke_audit, restore_antinuke, restore_antinuke_actor_roles};
use pinchora_core::anti_nuke::{Config, Limit};
use twilight_model::guild::audit_log::AuditLogEntry;
#[derive(Clone)]
struct SafetyMock {
    base: Mock,
    audit: Arc<Mutex<Vec<Value>>>,
    created: Arc<Mutex<Vec<Value>>>,
    positions: Arc<Mutex<Vec<Value>>>,
    permissions: Arc<AtomicU64>,
    bots: Arc<Mutex<Vec<u64>>>,
    audit_reads: Arc<AtomicU64>,
    systems: Arc<Mutex<Vec<u64>>>,
}
async fn safety_discord(
    State(mock): State<SafetyMock>,
    method: Method,
    uri: Uri,
    headers: HeaderMap,
    body: Bytes,
) -> (StatusCode, String) {
    let path = uri.path();
    let parts = path.trim_matches('/').split('/').collect::<Vec<_>>();
    if path.ends_with("/integrations") {
        let entries = mock.audit.lock().unwrap();
        let integrations = entries
            .iter()
            .filter(|e| e["action_type"] == 28)
            .map(|e| {
                let bot = e["target_id"].as_str().unwrap().parse::<u64>().unwrap();
                let installer = e["user_id"].as_str().unwrap().parse::<u64>().unwrap();
                json!({"id":bot.to_string(),"name":"Bot","type":"discord","enabled":true,
                "account":{"id":bot.to_string(),"name":"Bot"},"user":user_json(installer),
                "application":{"id":bot.to_string(),"name":"Bot","description":"","icon":null,"bot":user_json(bot)}})
            })
            .collect::<Vec<_>>();
        return (StatusCode::OK, json!(integrations).to_string());
    }
    if path.ends_with("/audit-logs") {
        mock.audit_reads.fetch_add(1, Ordering::SeqCst);
        return (StatusCode::OK,json!({"audit_log_entries":*mock.audit.lock().unwrap(),"users":[],"webhooks":[],"integrations":[],"threads":[],"application_commands":[],"auto_moderation_rules":[],"guild_scheduled_events":[]}).to_string());
    }
    if method == Method::GET && parts.len() >= 2 && parts[parts.len() - 2] == "members" {
        let user = parts.last().unwrap().parse::<u64>().unwrap();
        if mock.base.bans.lock().unwrap().contains(&user) {
            return (
                StatusCode::NOT_FOUND,
                json!({"message":"Unknown Member","code":10007}).to_string(),
            );
        }
    }
    if method == Method::PATCH && parts.len() >= 2 && parts[parts.len() - 2] == "members" {
        let data: Value = serde_json::from_slice(&body).unwrap();
        if let Some(roles) = data["roles"].as_array() {
            mock.base.member_roles.lock().unwrap().insert(
                parts.last().unwrap().parse().unwrap(),
                roles.iter().map(|r| r.as_str().unwrap().parse().unwrap()).collect(),
            );
        }
    }
    if method == Method::POST && (path.ends_with("/channels") || path.ends_with("/roles")) && path.contains("/guilds/")
    {
        let mut data: Value = serde_json::from_slice(&body).unwrap();
        mock.created.lock().unwrap().push(data.clone());
        let id = unique();
        if path.ends_with("/roles") {
            let mut role = role_json(id, data["name"].as_str().unwrap(), 1, false);
            for (k, v) in data.as_object().unwrap() {
                role[k] = v.clone();
            }
            mock.base.roles.lock().unwrap().push(role.clone());
            return (StatusCode::OK, role.to_string());
        }
        data["id"] = json!(id.to_string());
        data["guild_id"] = json!(parts[parts.len() - 2]);
        mock.base.channels.lock().unwrap().insert(id, data.clone());
        return (StatusCode::OK, data.to_string());
    }
    if method == Method::PATCH && path.ends_with("/roles") {
        mock.positions
            .lock()
            .unwrap()
            .push(serde_json::from_slice(&body).unwrap());
        return (StatusCode::OK, json!(*mock.base.roles.lock().unwrap()).to_string());
    }
    let (status, body) = discord(State(mock.base), method.clone(), uri.clone(), headers, body).await;
    if method == Method::GET && path.ends_with("/roles") && status == StatusCode::OK {
        let mut data: Value = serde_json::from_str(&body).unwrap();
        data[0]["permissions"] = json!(mock.permissions.load(Ordering::SeqCst).to_string());
        return (status, data.to_string());
    }
    if method == Method::GET && parts.len() >= 2 && parts[parts.len() - 2] == "members" && status == StatusCode::OK {
        let mut data: Value = serde_json::from_str(&body).unwrap();
        data["user"]["system"] = json!(
            mock.systems
                .lock()
                .unwrap()
                .contains(&parts.last().unwrap().parse().unwrap())
        );
        data["user"]["bot"] = json!(
            mock.bots
                .lock()
                .unwrap()
                .contains(&parts.last().unwrap().parse().unwrap())
        );
        return (status, data.to_string());
    }
    (status, body)
}
async fn setup(name: &'static str) -> Option<(Sim, SafetyMock, tokio::task::JoinHandle<()>)> {
    let mut sim = Sim::new(name).await?;
    sim.mock.owner.store(1, Ordering::SeqCst);
    sim.permissions = 1 << 5;
    let mock = SafetyMock {
        base: sim.mock.clone(),
        audit: Arc::default(),
        created: Arc::default(),
        positions: Arc::default(),
        bots: Arc::default(),
        systems: Arc::default(),
        audit_reads: Arc::default(),
        permissions: Arc::new(AtomicU64::new((1 << 7) | (1 << 28) | (1 << 4) | (1 << 1) | (1 << 2))),
    };
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let router = Router::new().fallback(safety_discord).with_state(mock.clone());
    let task = tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });
    Arc::get_mut(&mut sim.app).unwrap().http = Arc::new(
        Client::builder()
            .token("Bot sim".into())
            .proxy(address.to_string(), true)
            .ratelimiter(None)
            .build(),
    );
    let mut c = Config {
        enabled: true,
        log_channel_id: Some(CHANNEL_ID.to_string()),
        ..Config::default()
    };
    for limit in c.limits.values_mut() {
        *limit = Limit {
            count: 1,
            window_secs: 5,
        };
    }
    sim.app.db.save_antinuke(sim.guild, 1, &c).await.unwrap();
    sim.app
        .cache
        .antinuke_activation(sim.guild, sim.app.now().timestamp_millis() - 30_000, true)
        .await
        .unwrap();
    Some((sim, mock, task))
}
fn audit(sim: &Sim, actor: u64, kind: u64, target: u64, changes: Value) -> AuditLogEntry {
    let id = (((sim.app.now().timestamp_millis() - 1_420_070_400_000) as u64) << 22) | (unique() & 0x3fffff);
    serde_json::from_value(json!({"id":id.to_string(),"guild_id":sim.guild.to_string(),"user_id":actor.to_string(),"target_id":target.to_string(),"action_type":kind,"changes":changes})).unwrap()
}
async fn send(sim: &Sim, actor: u64, kind: u64, target: u64) -> i64 {
    let e = audit(sim, actor, kind, target, json!([]));
    let id = e.id.get() as i64;
    on_antinuke_audit(&sim.app, e).await.unwrap();
    id
}
#[tokio::test]
async fn antinuke_windows_replay_actor_kind_and_exemptions() {
    let Some((sim, _, task)) = setup("antinuke_windows").await else {
        return;
    };
    let actor = unique();
    let e = audit(&sim, actor, 12, unique(), json!([]));
    on_antinuke_audit(&sim.app, e.clone()).await.unwrap();
    on_antinuke_audit(&sim.app, e).await.unwrap();
    assert_eq!(sim.app.db.antinuke_events(sim.guild).await.unwrap().len(), 1);
    send(&sim, unique(), 12, unique()).await;
    send(&sim, actor, 32, unique()).await;
    assert!(sim.mock.posts.lock().unwrap().is_empty());
    send(&sim, actor, 12, unique()).await;
    assert_eq!(sim.mock.posts.lock().unwrap().len(), 1);
    sim.app.shift_clock(chrono::Duration::seconds(6));
    let id = send(&sim, actor, 12, unique()).await;
    assert_eq!(
        sim.app.db.antinuke_event(sim.guild, id).await.unwrap().unwrap().count,
        1
    );
    let mut c = sim.app.db.antinuke_config(sim.guild).await.unwrap();
    c.trusted_users = vec![actor.to_string()];
    sim.app.db.save_antinuke(sim.guild, 1, &c).await.unwrap();
    let before = sim.app.db.antinuke_events(sim.guild).await.unwrap().len();
    for actor in [1, APPLICATION_ID, actor] {
        send(&sim, actor, 12, unique()).await;
    }
    assert_eq!(sim.app.db.antinuke_events(sim.guild).await.unwrap().len(), before);
    task.abort();
}
#[tokio::test]
async fn antinuke_hierarchy_and_missing_permission_alert_only() {
    let Some((sim, mock, task)) = setup("antinuke_hierarchy").await else {
        return;
    };
    let actor = unique();
    let role = unique();
    sim.guild_role(role, "High staff", BOT_ROLE_POSITION + 1);
    sim.mock.member_roles.lock().unwrap().insert(actor, vec![role]);
    let mut c = sim.app.db.antinuke_config(sim.guild).await.unwrap();
    c.response = "ban".into();
    sim.app.db.save_antinuke(sim.guild, 1, &c).await.unwrap();
    send(&sim, actor, 12, unique()).await;
    let id = send(&sim, actor, 12, unique()).await;
    assert!(sim.mock.bans.lock().unwrap().is_empty());
    assert!(
        sim.app
            .db
            .antinuke_event(sim.guild, id)
            .await
            .unwrap()
            .unwrap()
            .action_taken
            .contains("highest role")
    );
    mock.permissions.store(1 << 7, Ordering::SeqCst);
    let actor = unique();
    send(&sim, actor, 12, unique()).await;
    let id = send(&sim, actor, 12, unique()).await;
    assert!(
        sim.app
            .db
            .antinuke_event(sim.guild, id)
            .await
            .unwrap()
            .unwrap()
            .action_taken
            .contains("Ban Members")
    );
    task.abort();
}
#[tokio::test]
async fn antinuke_quarantine_backup_and_owner_recovery() {
    let Some((sim, _, task)) = setup("antinuke_quarantine").await else {
        return;
    };
    let actor = unique();
    let role = unique();
    let quarantine = unique();
    sim.guild_role(role, "Member", 1);
    sim.guild_role(quarantine, "Quarantine", 2);
    sim.mock.member_roles.lock().unwrap().insert(actor, vec![role]);
    let mut c = sim.app.db.antinuke_config(sim.guild).await.unwrap();
    c.response = "quarantine".into();
    c.quarantine_role_id = Some(quarantine.to_string());
    sim.app.db.save_antinuke(sim.guild, 1, &c).await.unwrap();
    send(&sim, actor, 12, unique()).await;
    let id = send(&sim, actor, 12, unique()).await;
    assert_eq!(
        sim.mock.member_roles.lock().unwrap().get(&actor).unwrap(),
        &vec![quarantine]
    );
    assert_eq!(
        sim.app
            .db
            .antinuke_backup(sim.guild, actor)
            .await
            .unwrap()
            .unwrap()
            .role_ids,
        vec![role as i64]
    );
    assert!(
        restore_antinuke_actor_roles(&sim.app, sim.guild, id, actor)
            .await
            .is_err()
    );
    restore_antinuke_actor_roles(&sim.app, sim.guild, id, 1).await.unwrap();
    assert_eq!(sim.mock.member_roles.lock().unwrap().get(&actor).unwrap(), &vec![role]);
    assert!(restore_antinuke_actor_roles(&sim.app, sim.guild, id, 1).await.is_err());
    task.abort();
}
#[tokio::test]
async fn antinuke_mass_unban_once_and_vip_refusal() {
    let Some((mut sim, mock, task)) = setup("antinuke_bans").await else {
        return;
    };
    let actor = unique();
    let first = unique();
    let second = unique();
    let vip = unique();
    sim.guild_role(vip, "VIP", 1);
    sim.mock.member_roles.lock().unwrap().insert(first, vec![vip]);
    let mut c = sim.app.db.antinuke_config(sim.guild).await.unwrap();
    c.vip_roles = vec![vip.to_string()];
    sim.app.db.save_antinuke(sim.guild, 1, &c).await.unwrap();
    sim.permissions = 1 << 2;
    let reply = sim
        .command_resolved(
            1,
            "ban",
            json!([user_option("user", first)]),
            resolved_member(first, &[vip]),
        )
        .await;
    assert!(reply.content().contains("protected VIP"));
    assert!(sim.mock.bans.lock().unwrap().is_empty());
    c.vip_roles.clear();
    sim.app.db.save_antinuke(sim.guild, 1, &c).await.unwrap();
    sim.mock.bans.lock().unwrap().extend([first, second]);
    let one = audit(&sim, actor, 22, first, json!([]));
    let two = audit(&sim, actor, 22, second, json!([]));
    mock.audit.lock().unwrap().extend([json!(one), json!(two)]);
    on_antinuke_audit(&sim.app, one).await.unwrap();
    let id = two.id.get() as i64;
    on_antinuke_audit(&sim.app, two).await.unwrap();
    let result = restore_antinuke(&sim.app, sim.guild, id, 1, false).await.unwrap();
    assert!(result.contains("Restored: 2"), "{result}");
    assert!(sim.mock.bans.lock().unwrap().is_empty());
    assert!(restore_antinuke(&sim.app, sim.guild, id, 1, false).await.is_err());
    task.abort();
}
#[tokio::test]
async fn antinuke_restore_channels_roles_and_overwrites() {
    let Some((sim, mock, task)) = setup("antinuke_restore").await else {
        return;
    };
    let actor = unique();
    let role = unique();
    let channel = unique();
    let member = unique();
    sim.app
        .db
        .save_antinuke_snapshot(sim.guild, "role", role, &role_json(role, "Restored", 2, false), &[])
        .await
        .unwrap();
    let deletion = audit(
        &sim,
        actor,
        32,
        role,
        json!([{"key":"permissions","old_value":"0"},{"key":"position","old_value":2}]),
    );
    let role_event = deletion.id.get() as i64;
    on_antinuke_audit(&sim.app, deletion).await.unwrap();
    restore_antinuke(&sim.app, sim.guild, role_event, 1, false)
        .await
        .unwrap();
    let new_role = sim.app.db.antinuke_remap(sim.guild, "role", role).await.unwrap();
    assert_ne!(new_role, role);
    sim.app.db.save_antinuke_snapshot(sim.guild,"channel",channel,&json!({"id":channel.to_string(),"guild_id":sim.guild.to_string(),"name":"important","type":0,"position":3,"deletion_snapshot":true,"topic":"saved topic","permission_overwrites":[{"id":role.to_string(),"type":0,"allow":"1024","deny":"0"}]}),&[(member as i64,"1024".into(),"2048".into())]).await.unwrap();
    let event = send(&sim, actor, 12, channel).await;
    restore_antinuke(&sim.app, sim.guild, event, 1, false).await.unwrap();
    let created = mock.created.lock().unwrap();
    let restored = created.last().unwrap();
    assert_eq!(restored["position"], 3);
    assert_eq!(restored["topic"], "saved topic");
    assert_eq!(restored["permission_overwrites"][0]["id"], new_role.to_string());
    assert_eq!(restored["permission_overwrites"].as_array().unwrap().len(), 2);
    task.abort();
}
#[tokio::test]
async fn antinuke_commands_enforce_manage_owner_trust_and_clicks() {
    let Some((mut sim, _, task)) = setup("antinuke_commands").await else {
        return;
    };
    let user = unique();
    sim.permissions = 0;
    let reply = sim.command(user, "antinuke", json!([sub("disable", vec![])])).await;
    assert!(reply.content().contains("Manage Server"));
    sim.permissions = 1 << 5;
    let denied_module = modules(&mut sim, user, toggle("anti_nuke", false), &[]).await;
    assert!(denied_module.content().contains("server owner"));
    assert!(
        sim.app
            .guild_modules(sim.guild)
            .await
            .unwrap()
            .rule(pinchora_core::modules::Module::AntiNuke)
            .enabled
    );
    let reply = sim.command(user, "antinuke", json!([sub("disable", vec![])])).await;
    assert!(reply.content().contains("server owner"));
    let reply = sim.command(1, "antinuke", json!([sub("status", vec![])])).await;
    assert!(reply.content().contains("Anti-nuke"));
    let actor = unique();
    let id = send(&sim, actor, 12, unique()).await;
    let denied = sim.component(user, &format!("an:undo:{id}"), &[], &reply).await;
    assert!(denied.content().contains("server owner"));
    let forged = sim.component(1, "vp:roles:0", &[], &reply).await;
    assert!(forged.content().contains("valid action"));
    task.abort();
}
#[tokio::test]
async fn antinuke_audit_kinds_and_dangerous_delta() {
    let Some((sim, _, task)) = setup("antinuke_kinds").await else {
        return;
    };
    let actor = unique();
    for kind in [10, 12, 20, 22, 28, 30, 32, 50, 62, 1] {
        send(&sim, actor, kind, unique()).await;
    }
    let role = unique();
    sim.guild_role_with(role, "Danger", 1, 1 << 3);
    for (kind, changes) in [
        (31, json!([{"key":"permissions","old_value":"0","new_value":"8"}])),
        (
            25,
            json!([{"key":"$add","new_value":[{"id":role.to_string(),"name":"Danger"}]}]),
        ),
    ] {
        on_antinuke_audit(&sim.app, audit(&sim, actor, kind, unique(), changes))
            .await
            .unwrap();
    }
    let events = sim.app.db.antinuke_events(sim.guild).await.unwrap();
    assert_eq!(events.len(), 12);
    assert_eq!(events.iter().filter(|e| e.kind == "permission_grant").count(), 2);
    on_antinuke_audit(
        &sim.app,
        audit(
            &sim,
            actor,
            31,
            unique(),
            json!([{"key":"permissions","old_value":"8","new_value":"8"}]),
        ),
    )
    .await
    .unwrap();
    assert_eq!(sim.app.db.antinuke_events(sim.guild).await.unwrap().len(), 12);
    let vip = unique();
    let user = unique();
    sim.guild_role(vip, "Protected", 1);
    sim.mock.member_roles.lock().unwrap().insert(user, vec![vip]);
    let mut config = sim.app.db.antinuke_config(sim.guild).await.unwrap();
    config.vip_roles = vec![vip.to_string()];
    sim.app.db.save_antinuke(sim.guild, 1, &config).await.unwrap();
    on_antinuke_audit(
        &sim.app,
        audit(
            &sim,
            actor,
            25,
            user,
            json!([{"key":"$add","new_value":[{"id":vip.to_string(),"name":"Protected"}]}]),
        ),
    )
    .await
    .unwrap();
    assert_eq!(
        sim.app.db.antinuke_events(sim.guild).await.unwrap().len(),
        12,
        "safe role additions are not VIP violations"
    );
    on_antinuke_audit(
        &sim.app,
        audit(
            &sim,
            actor,
            25,
            user,
            json!([{"key":"$remove","new_value":[{"id":vip.to_string(),"name":"Protected"}]}]),
        ),
    )
    .await
    .unwrap();
    assert!(
        sim.app
            .db
            .antinuke_events(sim.guild)
            .await
            .unwrap()
            .iter()
            .any(|e| e.kind == "vip_action")
    );
    task.abort();
}

#[tokio::test]
async fn antinuke_vip_unban_rejoin_and_stale_audit() {
    let Some((sim, mock, task)) = setup("antinuke_vip_unban").await else {
        return;
    };
    let vip = unique();
    let user = unique();
    let actor = unique();
    sim.guild_role(vip, "Protected", 1);
    sim.mock.bans.lock().unwrap().push(user);
    let mut c = sim.app.db.antinuke_config(sim.guild).await.unwrap();
    c.vip_roles = vec![vip.to_string()];
    c.revert_vip_bans = true;
    sim.app.db.save_antinuke(sim.guild, 1, &c).await.unwrap();
    sim.app
        .db
        .backup_antinuke_roles(sim.guild, user, &[vip as i64], false)
        .await
        .unwrap();
    let entry = audit(&sim, actor, 22, user, json!([]));
    let id = entry.id.get() as i64;
    mock.audit.lock().unwrap().push(json!(entry));
    on_antinuke_audit(&sim.app, entry).await.unwrap();
    let event = sim.app.db.antinuke_event(sim.guild, id).await.unwrap().unwrap();
    assert_eq!(event.kind, "vip_ban");
    assert!(event.reverted);
    assert!(sim.mock.bans.lock().unwrap().is_empty());
    assert!(
        sim.app
            .db
            .antinuke_backup(sim.guild, user)
            .await
            .unwrap()
            .unwrap()
            .restore_on_join
    );
    let member: twilight_model::gateway::payload::incoming::MemberAdd = serde_json::from_value({
        let mut v = member_json(user, &[]);
        v["guild_id"] = json!(sim.guild.to_string());
        v
    })
    .unwrap();
    pinchora_commands::on_antinuke_gateway(
        &sim.app,
        &twilight_model::gateway::event::Event::MemberAdd(Box::new(member)),
    );
    sim.app.drain(std::time::Duration::from_secs(10)).await;
    assert!(sim.mock.member_roles.lock().unwrap()[&user].contains(&vip));
    let mut stale = audit(&sim, actor, 12, unique(), json!([]));
    stale.id = Id::new(stale.id.get() - (60_000 << 22));
    on_antinuke_audit(&sim.app, stale).await.unwrap();
    assert_eq!(sim.app.db.antinuke_events(sim.guild).await.unwrap().len(), 1);
    task.abort();
}
#[tokio::test]
async fn antinuke_ban_kick_and_member_prune_weight() {
    let Some((sim, _, task)) = setup("antinuke_punishments").await else {
        return;
    };
    let mut c = sim.app.db.antinuke_config(sim.guild).await.unwrap();
    for response in ["ban", "kick"] {
        let actor = unique();
        c.response = response.into();
        sim.app.db.save_antinuke(sim.guild, 1, &c).await.unwrap();
        let mut e = serde_json::to_value(audit(&sim, actor, 21, sim.guild, json!([]))).unwrap();
        e["options"] = json!({"members_removed":"20","delete_member_days":"7"});
        on_antinuke_audit(&sim.app, serde_json::from_value(e).unwrap())
            .await
            .unwrap();
        assert!(
            sim.mock
                .mod_calls
                .lock()
                .unwrap()
                .iter()
                .any(|call| call.starts_with(&format!("{response} {actor}")))
        );
    }
    task.abort();
}
#[tokio::test]
async fn antinuke_gateway_snapshot_and_audit_fallback() {
    let Some((sim, mock, task)) = setup("antinuke_gateway").await else {
        return;
    };
    let channel = unique();
    let e = audit(&sim, unique(), 12, channel, json!([]));
    let id = e.id.get() as i64;
    mock.audit.lock().unwrap().push(serde_json::to_value(e).unwrap());
    let deleted=serde_json::from_value(json!({"id":channel.to_string(),"guild_id":sim.guild.to_string(),"type":0,"name":"deleted","position":4,"permission_overwrites":[]})).unwrap();
    pinchora_commands::on_antinuke_gateway(
        &sim.app,
        &twilight_model::gateway::event::Event::ChannelDelete(Box::new(deleted)),
    );
    sim.app.drain(std::time::Duration::from_secs(10)).await;
    assert_eq!(
        sim.app.db.antinuke_event(sim.guild, id).await.unwrap().unwrap().count,
        1
    );
    assert_eq!(
        sim.app
            .db
            .antinuke_snapshot(sim.guild, "channel", channel)
            .await
            .unwrap()
            .unwrap()["position"],
        4
    );
    sim.app
        .cache
        .release_cooldown(&format!("antinuke:audit-batch:{}", sim.guild))
        .await
        .unwrap();
    sim.app
        .cache
        .release_cooldown(&format!("antinuke:audit-fetch:{}", sim.guild))
        .await
        .unwrap();
    let changed = unique();
    let grant = audit(
        &sim,
        unique(),
        13,
        changed,
        json!([{"key":"allow","old_value":"0","new_value":"8"}]),
    );
    let grant_id = grant.id.get() as i64;
    mock.audit.lock().unwrap().push(serde_json::to_value(grant).unwrap());
    let updated = serde_json::from_value(json!({
        "id":changed.to_string(),"guild_id":sim.guild.to_string(),"type":0,
        "name":"changed","permission_overwrites":[]
    }))
    .unwrap();
    pinchora_commands::on_antinuke_gateway(
        &sim.app,
        &twilight_model::gateway::event::Event::ChannelUpdate(Box::new(updated)),
    );
    sim.app.drain(std::time::Duration::from_secs(10)).await;
    assert_eq!(
        sim.app
            .db
            .antinuke_event(sim.guild, grant_id)
            .await
            .unwrap()
            .unwrap()
            .kind,
        "permission_grant"
    );
    task.abort();
}
#[tokio::test]
async fn antinuke_removes_attack_bots_without_touching_trusted_bots() {
    let Some((sim, mock, task)) = setup("antinuke_bots").await else {
        return;
    };
    let bot = unique();
    let actor = unique();
    mock.bots.lock().unwrap().push(bot);
    mock.audit
        .lock()
        .unwrap()
        .push(json!(audit(&sim, actor, 28, bot, json!([]))));
    let mut c = sim.app.db.antinuke_config(sim.guild).await.unwrap();
    c.remove_bots = true;
    sim.app.db.save_antinuke(sim.guild, 1, &c).await.unwrap();
    send(&sim, bot, 12, unique()).await;
    send(&sim, bot, 12, unique()).await;
    assert!(sim.mock.mod_calls.lock().unwrap().contains(&format!("kick {bot}")));
    sim.mock.mod_calls.lock().unwrap().clear();
    c.trusted_users = vec![bot.to_string()];
    sim.app.db.save_antinuke(sim.guild, 1, &c).await.unwrap();
    send(&sim, actor, 28, bot).await;
    send(&sim, actor, 28, bot).await;
    assert!(sim.mock.mod_calls.lock().unwrap().is_empty());
    task.abort();
}

#[tokio::test]
async fn antinuke_disabled_cache_invalidates_and_avoids_database_reads() {
    let Some((sim, _, task)) = setup("antinuke_disabled_cache").await else {
        return;
    };
    let mut c = sim.app.db.antinuke_config(sim.guild).await.unwrap();
    c.enabled = false;
    sim.app.db.save_antinuke(sim.guild, 1, &c).await.unwrap();
    send(&sim, unique(), 12, unique()).await;
    c.enabled = true;
    sim.app.db.save_antinuke(sim.guild, 1, &c).await.unwrap();
    send(&sim, unique(), 12, unique()).await;
    assert!(sim.app.db.antinuke_events(sim.guild).await.unwrap().is_empty());
    sim.app.announce_guild_change(sim.guild).await;
    send(&sim, unique(), 12, unique()).await;
    assert_eq!(sim.app.db.antinuke_events(sim.guild).await.unwrap().len(), 1);
    c.enabled = false;
    sim.app.db.save_antinuke(sim.guild, 1, &c).await.unwrap();
    // A lost pub/sub notification still learns the off switch on the next fresh read.
    send(&sim, unique(), 12, unique()).await;
    sim.app.db.pool().close().await;
    for _ in 0..20 {
        on_antinuke_audit(&sim.app, audit(&sim, unique(), 12, unique(), json!([])))
            .await
            .expect("disabled event must not touch the closed database pool");
    }
    task.abort();
}

#[tokio::test]
async fn antinuke_gateway_bursts_batch_late_audits_without_cross_actor_attribution() {
    let Some((sim, mock, task)) = setup("antinuke_batched_audit").await else {
        return;
    };
    let first = audit(&sim, unique(), 12, unique(), json!([]));
    let late = audit(&sim, unique(), 12, unique(), json!([]));
    let departed = unique();
    sim.mock.bans.lock().unwrap().push(departed);
    let unavailable = audit(&sim, departed, 12, unique(), json!([]));
    mock.audit.lock().unwrap().extend([json!(unavailable), json!(first)]);
    for _ in 0..20 {
        let deleted = serde_json::from_value(json!({"id":unique().to_string(),"guild_id":sim.guild.to_string(),
            "type":0,"name":"deleted","position":4,"permission_overwrites":[]}))
        .unwrap();
        pinchora_commands::on_antinuke_gateway(
            &sim.app,
            &twilight_model::gateway::event::Event::ChannelDelete(Box::new(deleted)),
        );
    }
    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    mock.audit.lock().unwrap().insert(0, json!(late));
    sim.app.drain(std::time::Duration::from_secs(20)).await;
    assert!(
        mock.audit_reads.load(Ordering::SeqCst) <= 3,
        "one batch per guild, not per event"
    );
    let events = sim.app.db.antinuke_events(sim.guild).await.unwrap();
    assert_eq!(events.len(), 2);
    for audit in [first, late] {
        let event = events.iter().find(|e| e.id == audit.id.get() as i64).unwrap();
        assert_eq!(event.actor_id as u64, audit.user_id.unwrap().get());
        assert_eq!(event.target_id.unwrap() as u64, audit.target_id.unwrap().get());
        assert_eq!(event.count, 1);
    }
    task.abort();
}

#[tokio::test]
async fn antinuke_system_and_owner_installed_bots_are_never_punished() {
    let Some((sim, mock, task)) = setup("antinuke_system_integrations").await else {
        return;
    };
    let mut c = sim.app.db.antinuke_config(sim.guild).await.unwrap();
    c.response = "ban".into();
    c.remove_bots = true;
    sim.app.db.save_antinuke(sim.guild, 1, &c).await.unwrap();
    let system = unique();
    mock.systems.lock().unwrap().push(system);
    let bot = unique();
    let unknown = unique();
    mock.bots.lock().unwrap().extend([bot, unknown]);
    mock.audit
        .lock()
        .unwrap()
        .push(json!(audit(&sim, 1, 28, bot, json!([]))));
    for actor in [system, bot, unknown, 1, APPLICATION_ID] {
        send(&sim, actor, 12, unique()).await;
        send(&sim, actor, 12, unique()).await;
    }
    let mut system_entry = audit(&sim, system, 12, unique(), json!([]));
    system_entry.user_id = None;
    on_antinuke_audit(&sim.app, system_entry).await.unwrap();
    assert!(sim.mock.mod_calls.lock().unwrap().is_empty());
    c.response = "alert".into();
    sim.app.db.save_antinuke(sim.guild, 1, &c).await.unwrap();
    let inviter = unique();
    send(&sim, inviter, 28, unknown).await;
    let id = send(&sim, inviter, 28, unknown).await;
    let event = sim.app.db.antinuke_event(sim.guild, id).await.unwrap().unwrap();
    assert!(
        !event.action_taken.contains("Added bot removed"),
        "{}",
        event.action_taken
    );
    assert!(sim.mock.mod_calls.lock().unwrap().is_empty());
    task.abort();
}

#[tokio::test]
async fn antinuke_newer_bans_even_by_same_actor_and_expanded_windows_are_preserved() {
    let Some((sim, mock, task)) = setup("antinuke_ban_scope").await else {
        return;
    };
    let actor = unique();
    let old_user = unique();
    let user = unique();
    let mut old = audit(&sim, actor, 22, old_user, json!([]));
    old.id = Id::new(old.id.get() - (10_000 << 22));
    on_antinuke_audit(&sim.app, old.clone()).await.unwrap();
    let event = audit(&sim, actor, 22, user, json!([]));
    on_antinuke_audit(&sim.app, event.clone()).await.unwrap();
    let mut newer = event.clone();
    newer.id = Id::new(event.id.get() + 1);
    mock.audit
        .lock()
        .unwrap()
        .extend([json!(old), json!(event), json!(newer)]);
    sim.mock.bans.lock().unwrap().extend([old_user, user]);
    let mut c = sim.app.db.antinuke_config(sim.guild).await.unwrap();
    c.limits.get_mut("ban").unwrap().window_secs = 300;
    sim.app.db.save_antinuke(sim.guild, 1, &c).await.unwrap();
    let result = restore_antinuke(&sim.app, sim.guild, event.id.get() as i64, 1, false)
        .await
        .unwrap();
    assert!(result.contains("Restored: 0"), "{result}");
    assert_eq!(*sim.mock.bans.lock().unwrap(), vec![old_user, user]);
    mock.audit.lock().unwrap().pop();
    let result = restore_antinuke(&sim.app, sim.guild, event.id.get() as i64, 1, false)
        .await
        .unwrap();
    assert!(result.contains("Restored: 1"), "{result}");
    assert_eq!(
        *sim.mock.bans.lock().unwrap(),
        vec![old_user],
        "original window is immutable"
    );
    task.abort();
}

#[tokio::test]
async fn antinuke_forged_suffix_and_revoked_trust_cannot_restore() {
    let Some((mut sim, _, task)) = setup("antinuke_revoke_restore").await else {
        return;
    };
    let trusted = unique();
    let actor = unique();
    let id = send(&sim, actor, 12, unique()).await;
    let reply = sim.command(1, "antinuke", json!([sub("status", vec![])])).await;
    for suffix in ["bad".to_string(), format!("{}:extra", sim.guild), "0".into()] {
        let forged = sim
            .component(1, &format!("an:confirm:{id}:{suffix}"), &[], &reply)
            .await;
        assert!(forged.content().contains("valid action"), "{}", forged.content());
    }
    let mut c = sim.app.db.antinuke_config(sim.guild).await.unwrap();
    c.trusted_users.push(trusted.to_string());
    sim.app.db.save_antinuke(sim.guild, 1, &c).await.unwrap();
    let guard = sim.app.db.lock_antinuke(sim.guild).await.unwrap();
    let app = sim.app.clone();
    let guild = sim.guild;
    let waiting = tokio::spawn(async move { restore_antinuke(&app, guild, id, trusted, false).await });
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    c.trusted_users.clear();
    sim.app.db.save_antinuke(sim.guild, 1, &c).await.unwrap();
    guard.rollback().await.unwrap();
    let err = waiting.await.unwrap().unwrap_err().to_string();
    assert!(err.contains("server owner"), "{err}");
    task.abort();
}

#[tokio::test]
async fn antinuke_restore_never_regrants_stale_permissions_or_missing_acls() {
    let Some((sim, mock, task)) = setup("antinuke_restore_permission_ceiling").await else {
        return;
    };
    let role = unique();
    let mut snapshot = role_json(role, "Old admin", 4, false);
    snapshot["permissions"] = json!("8");
    sim.app
        .db
        .save_antinuke_snapshot(sim.guild, "role", role, &snapshot, &[])
        .await
        .unwrap();
    let entry = audit(
        &sim,
        unique(),
        32,
        role,
        json!([{"key":"permissions","old_value":"1024"},{"key":"position","old_value":2}]),
    );
    on_antinuke_audit(&sim.app, entry.clone()).await.unwrap();
    restore_antinuke(&sim.app, sim.guild, entry.id.get() as i64, 1, false)
        .await
        .unwrap();
    assert_eq!(mock.created.lock().unwrap()[0]["permissions"], "1024");
    assert_eq!(
        mock.positions.lock().unwrap()[0][0]["position"],
        2,
        "audit position replaces stale hierarchy"
    );
    let channel = unique();
    sim.app
        .db
        .save_antinuke_snapshot(
            sim.guild,
            "channel",
            channel,
            &json!({"id":channel.to_string(),
        "guild_id":sim.guild.to_string(),"type":0,"name":"unverified","position":1,"permission_overwrites":[]}),
            &[],
        )
        .await
        .unwrap();
    let id = send(&sim, unique(), 12, channel).await;
    let result = restore_antinuke(&sim.app, sim.guild, id, 1, false).await.unwrap();
    assert!(result.contains("Restored: 0"), "{result}");
    assert_eq!(
        mock.created.lock().unwrap().len(),
        1,
        "an incomplete ACL cannot become public"
    );
    let missing_role = unique();
    sim.app
        .db
        .save_antinuke_snapshot(
            sim.guild,
            "channel",
            channel,
            &json!({"id":channel.to_string(),
        "guild_id":sim.guild.to_string(),"type":0,"name":"missing-deny","position":1,"deletion_snapshot":true,
        "permission_overwrites":[{"id":missing_role.to_string(),"type":0,"allow":"0","deny":"1024"}]}),
            &[],
        )
        .await
        .unwrap();
    let result = restore_antinuke(&sim.app, sim.guild, id, 1, false).await.unwrap();
    assert!(result.contains("Restored: 0"), "{result}");
    assert_eq!(
        mock.created.lock().unwrap().len(),
        1,
        "a missing deny role must never be silently omitted"
    );
    task.abort();
}

#[tokio::test]
async fn antinuke_every_admin_path_requires_manage_guild() {
    let Some((mut sim, _, task)) = setup("antinuke_admin_rows").await else {
        return;
    };
    let user = unique();
    let target = unique();
    let role = sim.guild_role(unique(), "VIP", 1);
    let mut rows = ["status", "enable", "disable", "events"]
        .map(|name| json!([sub(name, vec![])]))
        .to_vec();
    rows.push(json!([sub(
        "limits",
        vec![
            text_option("kind", "ban"),
            int_option("count", 1),
            int_option("seconds", 5)
        ]
    )]));
    for action in ["add", "remove"] {
        rows.push(json!([group("trust", sub(action, vec![user_option("user", target)]))]));
        rows.push(json!([group("vip", sub(action, vec![role_option("role", &role)]))]));
    }
    for permissions in [0, !(1u64 << 5) & !(1 << 3)] {
        sim.permissions = permissions;
        for row in &rows {
            let reply = sim
                .command_resolved(user, "antinuke", row.clone(), resolved(&[target], &[&role]))
                .await;
            assert!(reply.content().contains("Manage Server"), "{row}: {}", reply.content());
        }
    }
    task.abort();
}

#[tokio::test]
async fn antinuke_restores_lower_role_positions_first() {
    let Some((sim, mock, task)) = setup("antinuke_restore_order").await else {
        return;
    };
    let actor = unique();
    let mut last = 0;
    for position in [4, 2] {
        let role = unique();
        sim.app
            .db
            .save_antinuke_snapshot(
                sim.guild,
                "role",
                role,
                &role_json(role, &format!("Role {position}"), position, false),
                &[],
            )
            .await
            .unwrap();
        let mut entry = audit(
            &sim,
            actor,
            32,
            role,
            json!([{"key":"permissions","old_value":"0"},{"key":"position","old_value":position}]),
        );
        if last > 0 {
            entry.id = Id::new(last as u64 + 1);
        }
        last = entry.id.get() as i64;
        on_antinuke_audit(&sim.app, entry).await.unwrap();
    }
    let result = restore_antinuke(&sim.app, sim.guild, last, 1, false).await.unwrap();
    assert!(result.contains("Restored: 2"), "{result}");
    let created = mock.created.lock().unwrap();
    assert_eq!(created[0]["name"], "Role 2");
    assert_eq!(created[1]["name"], "Role 4");
    task.abort();
}

#[tokio::test]
async fn antinuke_recreated_roles_cannot_bypass_deleted_channel_denies() {
    let Some((sim, _, task)) = setup("antinuke_restored_role_membership").await else {
        return;
    };
    let actor = unique();
    let old = unique();
    let new = unique();
    sim.guild_role(new, "Recreated", 2);
    sim.app
        .db
        .save_antinuke_snapshot(sim.guild, "role", old, &role_json(old, "Original", 2, false), &[])
        .await
        .unwrap();
    sim.app
        .db
        .antinuke_restored_object(sim.guild, "role", old, new)
        .await
        .unwrap();
    sim.app
        .db
        .backup_antinuke_roles(sim.guild, actor, &[old as i64], true)
        .await
        .unwrap();
    let event = send(&sim, actor, 12, unique()).await;
    let error = restore_antinuke_actor_roles(&sim.app, sim.guild, event, 1)
        .await
        .unwrap_err()
        .to_string();
    assert!(error.contains("channel permission review"), "{error}");
    assert!(
        !sim.mock
            .member_roles
            .lock()
            .unwrap()
            .get(&actor)
            .is_some_and(|roles| roles.contains(&new))
    );
    task.abort();
}

#[tokio::test]
async fn antinuke_disabled_module_avoids_database_reads() {
    let Some((sim, _, task)) = setup("antinuke_disabled_module_cache").await else {
        return;
    };
    sim.app
        .db
        .set_module(sim.guild, pinchora_core::modules::Module::AntiNuke, Some(false), None)
        .await
        .unwrap();
    sim.app.announce_guild_change(sim.guild).await;
    send(&sim, unique(), 12, unique()).await;
    sim.app.db.pool().close().await;
    for _ in 0..20 {
        on_antinuke_audit(&sim.app, audit(&sim, unique(), 12, unique(), json!([])))
            .await
            .expect("a disabled module must not query the closed database pool");
    }
    task.abort();
}

#[tokio::test]
async fn antinuke_reenable_never_counts_actions_from_before_activation() {
    let Some((mut sim, _, task)) = setup("antinuke_activation_boundary").await else {
        return;
    };
    sim.channel_is(CHANNEL_ID, 0, None);
    let actor = unique();
    send(&sim, actor, 12, unique()).await;
    let mut c = sim.app.db.antinuke_config(sim.guild).await.unwrap();
    c.enabled = false;
    pinchora_commands::save_antinuke_config(&sim.app, sim.guild, 1, &c)
        .await
        .unwrap();
    let during_off = audit(&sim, actor, 12, unique(), json!([]));
    sim.app.shift_clock(chrono::Duration::seconds(1));
    c.enabled = true;
    pinchora_commands::save_antinuke_config(&sim.app, sim.guild, 1, &c)
        .await
        .unwrap();
    on_antinuke_audit(&sim.app, during_off.clone()).await.unwrap();
    assert!(
        sim.app
            .db
            .antinuke_event(sim.guild, during_off.id.get() as i64)
            .await
            .unwrap()
            .is_none()
    );
    sim.app.shift_clock(chrono::Duration::seconds(1));
    let fresh = send(&sim, actor, 12, unique()).await;
    assert_eq!(
        sim.app
            .db
            .antinuke_event(sim.guild, fresh)
            .await
            .unwrap()
            .unwrap()
            .count,
        1,
        "re-enabling also excludes the previous activation's Redis window"
    );
    assert!(sim.mock.posts.lock().unwrap().is_empty());
    modules(&mut sim, 1, toggle("anti_nuke", false), &[]).await;
    let module_off = audit(&sim, actor, 12, unique(), json!([]));
    sim.app.shift_clock(chrono::Duration::seconds(1));
    modules(&mut sim, 1, toggle("anti_nuke", true), &[]).await;
    on_antinuke_audit(&sim.app, module_off.clone()).await.unwrap();
    assert!(
        sim.app
            .db
            .antinuke_event(sim.guild, module_off.id.get() as i64)
            .await
            .unwrap()
            .is_none()
    );
    sim.app.shift_clock(chrono::Duration::seconds(1));
    let fresh = send(&sim, actor, 12, unique()).await;
    assert_eq!(
        sim.app
            .db
            .antinuke_event(sim.guild, fresh)
            .await
            .unwrap()
            .unwrap()
            .count,
        1
    );
    sim.app.drain(std::time::Duration::from_secs(10)).await;
    task.abort();
}
