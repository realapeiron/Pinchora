use super::*;
use std::{sync::LazyLock, time::Duration};
static PAGED_CHANNELS: LazyLock<Mutex<Vec<u64>>> = LazyLock::new(Mutex::default);
static BOOSTING: LazyLock<Mutex<Vec<u64>>> = LazyLock::new(Mutex::default);
static AUDITS: LazyLock<Mutex<HashMap<u64, Vec<String>>>> = LazyLock::new(Mutex::default);
fn text(name: &str, value: &str) -> Value {
    json!({"name":name,"type":3,"value":value})
}
fn resolved(user: u64) -> Value {
    json!({"users":{user.to_string():person_json(user,"member",None,false)}})
}
#[tokio::test]
async fn mod_parity_notes_permissions_and_bilingual_report_flow() {
    let Some(mut sim) = Sim::new("mod_parity_notes_reports").await else {
        return;
    };
    let actor = unique();
    let target = unique();
    let opts = json!([sub(
        "notes",
        vec![
            text("action", "add"),
            user_option("user", target),
            text("text", "Private staff note")
        ]
    )]);
    sim.permissions = 0;
    let denied = sim
        .command_resolved(actor, "modtools", opts.clone(), resolved(target))
        .await;
    assert!(denied.ephemeral);
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM mod_notes WHERE guild_id=$1")
        .bind(sim.guild as i64)
        .fetch_one(sim.app.db.pool())
        .await
        .unwrap();
    assert_eq!(count, 0);
    sim.permissions = 1 << 40;
    let added = sim.command_resolved(actor, "modtools", opts, resolved(target)).await;
    assert!(added.ephemeral);
    assert!(added.content().contains("Saved"), "{}", added.content());
    let list = sim
        .command_resolved(
            actor,
            "modtools",
            json!([sub("notes", vec![text("action", "list"), user_option("user", target)])]),
            resolved(target),
        )
        .await;
    assert!(list.content().contains("Private staff note"));
    let c = pinchora_core::mod_parity::Config {
        report_channel: Some(CHANNEL_ID.to_string()),
        ..Default::default()
    };
    sim.app.db.save_mod_parity_config(sim.guild, &c).await.unwrap();
    for (lang, word) in [
        (pinchora_core::i18n::Lang::En, "Saved"),
        (pinchora_core::i18n::Lang::Tr, "Kaydedildi"),
    ] {
        let user = unique();
        sim.app.set_user_lang(user, Some(lang)).await.unwrap();
        sim.permissions = 0;
        let reply = sim
            .command_resolved(
                user,
                "modtools",
                json!([sub(
                    "report",
                    vec![user_option("user", target), text("reason", "Please investigate")]
                )]),
                resolved(target),
            )
            .await;
        assert!(reply.ephemeral);
        assert!(reply.content().contains(word), "{}", reply.content());
    }
    assert_eq!(
        sim.app.db.mod_parity_queues(sim.guild).await.unwrap()["reports"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
}
#[tokio::test]
async fn mod_parity_bulk_preview_never_mutates_and_confirmation_is_owner_bound() {
    let Some(mut sim) = Sim::new("mod_parity_mass_preview").await else {
        return;
    };
    let actor = unique();
    let target = unique();
    sim.permissions = 1 << 2;
    let preview = sim
        .command(
            actor,
            "modtools",
            json!([sub(
                "mass",
                vec![text("action", "ban"), text("ids", &target.to_string())]
            )]),
        )
        .await;
    assert!(preview.content().contains("Preview"), "{}", preview.content());
    let token = preview.content().split("confirm:").last().unwrap().trim();
    let stolen = sim
        .command(
            actor + 1,
            "modtools",
            json!([sub("mass", vec![text("action", "ban"), text("confirm", token)])]),
        )
        .await;
    assert!(stolen.content().contains("Check"));
    let ids = (1_000_000_000_000_000_000_u64..1_000_000_000_000_000_100)
        .map(|id| id.to_string())
        .collect::<Vec<_>>()
        .join(" ");
    let large = sim
        .command(
            actor,
            "modtools",
            json!([sub("mass", vec![text("action", "ban"), text("ids", &ids)])]),
        )
        .await;
    assert!(large.ephemeral);
    assert!(large.description().contains("100 members"));
    assert!(large.description().contains("1000000000000000099"));
    assert!(large.description().chars().count() <= 4096);
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM mod_cases WHERE guild_id=$1")
        .bind(sim.guild as i64)
        .fetch_one(sim.app.db.pool())
        .await
        .unwrap();
    assert_eq!(count, 0);
}
/// Model the role replacement and channel inventory used by restrictions.
pub(super) fn mock(
    state: &Mock,
    method: &Method,
    uri: &Uri,
    parts: &[&str],
    headers: &HeaderMap,
    body: &[u8],
) -> Option<(StatusCode, String)> {
    if method != Method::GET
        && let [.., "guilds", guild, _, _] = parts
        && let Some(reason) = headers.get("x-audit-log-reason").and_then(|v| v.to_str().ok())
    {
        AUDITS
            .lock()
            .unwrap()
            .entry(guild.parse().unwrap())
            .or_default()
            .push(reason.into());
    }
    if let (&Method::GET, [.., "guilds", _, "members", user]) = (method, parts)
        && let Ok(user) = user.parse::<u64>()
        && BOOSTING.lock().unwrap().contains(&user)
    {
        let roles = state
            .member_roles
            .lock()
            .unwrap()
            .get(&user)
            .cloned()
            .unwrap_or_default();
        let mut member = member_json(user, &roles);
        member["premium_since"] = json!("2024-01-01T00:00:00.000000+00:00");
        return Some((StatusCode::OK, member.to_string()));
    }
    if let (&Method::GET, [.., "channels", channel, "messages"]) = (method, parts)
        && let Ok(channel) = channel.parse::<u64>()
        && PAGED_CHANNELS.lock().unwrap().contains(&channel)
    {
        let before = uri
            .query()
            .unwrap_or_default()
            .split('&')
            .find_map(|p| p.strip_prefix("before="))
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(u64::MAX);
        let messages: Vec<_> = state
            .history
            .lock()
            .unwrap()
            .get(&channel)
            .into_iter()
            .flatten()
            .filter(|m| m["id"].as_str().unwrap().parse::<u64>().unwrap() < before)
            .take(100)
            .cloned()
            .collect();
        return Some((StatusCode::OK, serde_json::to_string(&messages).unwrap()));
    }
    if let (&Method::GET, [.., "guilds", _, "channels"]) = (method, parts) {
        return Some((
            StatusCode::OK,
            serde_json::to_string(&state.channels.lock().unwrap().values().cloned().collect::<Vec<_>>()).unwrap(),
        ));
    }
    if let (&Method::PATCH, [.., "guilds", _, "members", user]) = (method, parts) {
        if !state.roles_allowed.load(Ordering::SeqCst) {
            return Some((
                StatusCode::FORBIDDEN,
                json!({"code":50013,"message":"Missing Permissions"}).to_string(),
            ));
        }
        let data: Value = serde_json::from_slice(body).unwrap_or_default();
        if let Some(roles) = data["roles"].as_array() {
            state.member_roles.lock().unwrap().insert(
                user.parse().unwrap(),
                roles.iter().filter_map(|r| r.as_str()?.parse().ok()).collect(),
            );
        }
    }
    None
}
#[tokio::test]
async fn mod_parity_restrictions_restore_roles_softban_voice_and_name_actions() {
    let Some(mut sim) = Sim::new("mod_parity_actions").await else {
        return;
    };
    let (actor, target, staff, reward, mute) = (unique(), unique(), unique(), unique(), unique());
    sim.guild_role_with(
        staff,
        "Staff",
        5,
        (1 << 40) | (1 << 28) | (1 << 2) | (1 << 24) | (1 << 27),
    );
    sim.guild_role(reward, "Member", 1);
    sim.guild_role(mute, "Restricted", 2);
    sim.roles = vec![staff];
    sim.mock.member_roles.lock().unwrap().insert(actor, vec![staff]);
    sim.permissions = (1 << 40) | (1 << 28) | (1 << 2) | (1 << 24) | (1 << 27);
    sim.mock.member_roles.lock().unwrap().insert(target, vec![reward]);
    sim.app
        .db
        .save_mod_parity_config(
            sim.guild,
            &pinchora_core::mod_parity::Config {
                mute_role: Some(mute.to_string()),
                jail_role: Some(mute.to_string()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    for action in ["mute", "unmute", "jail", "unjail", "softban"] {
        let result = sim
            .command_resolved(
                actor,
                "modtools",
                json!([sub(
                    "restrict",
                    vec![
                        text("action", action),
                        user_option("user", target),
                        text("reason", "staff reviewed")
                    ]
                )]),
                resolved(target),
            )
            .await;
        assert!(result.content().contains("Saved"), "{action}: {}", result.content());
        if matches!(action, "mute" | "jail") {
            assert_eq!(sim.mock.member_roles.lock().unwrap()[&target], vec![mute]);
        }
        if matches!(action, "unmute" | "unjail") {
            assert_eq!(sim.mock.member_roles.lock().unwrap()[&target], vec![reward]);
        }
    }
    assert!(!sim.mock.bans.lock().unwrap().contains(&target));
    let result = sim
        .command_resolved(
            actor,
            "modtools",
            json!([sub("voice", vec![text("action", "ban"), user_option("user", target)])]),
            resolved(target),
        )
        .await;
    assert!(result.content().contains("Completed 1 of 1"), "{}", result.content());
    let banned: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM mod_voice_bans WHERE guild_id=$1 AND user_id=$2)")
            .bind(sim.guild as i64)
            .bind(target as i64)
            .fetch_one(sim.app.db.pool())
            .await
            .unwrap();
    assert!(banned);
    let renamed = sim
        .command_resolved(
            actor,
            "modtools",
            json!([sub("names", vec![user_option("user", target)])]),
            resolved_member(target, &[reward]),
        )
        .await;
    assert!(renamed.content().contains("Saved"), "{}", renamed.content());
}
#[tokio::test]
async fn mod_parity_message_context_report_is_private_and_keeps_message_reference() {
    let Some(mut sim) = Sim::new("mod_parity_context").await else {
        return;
    };
    let actor = unique();
    let target = unique();
    let msg = unique();
    sim.app
        .db
        .save_mod_parity_config(
            sim.guild,
            &pinchora_core::mod_parity::Config {
                report_channel: Some(CHANNEL_ID.to_string()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    let mut message = message_json(CHANNEL_ID, msg);
    message["author"] = person_json(target, "reported", None, false);
    let reply=sim.run("Report message".into(),actor,2,json!({"id":unique().to_string(),"name":"Report","type":3,"target_id":msg.to_string(),"resolved":{"messages":{msg.to_string():message}}}),None).await;
    assert!(reply.ephemeral);
    assert!(reply.content().contains("Saved"));
    let rows = sim.app.db.mod_parity_queues(sim.guild).await.unwrap();
    assert_eq!(rows["reports"][0]["message_id"], msg.to_string());
}

#[tokio::test]
async fn mod_parity_appeal_review_checks_permission_and_rank_then_reverses_sanction() {
    let Some(sim) = Sim::new("mod_parity_review").await else {
        return;
    };
    let (actor, original, target, senior, junior) = (unique(), unique(), unique(), unique(), unique());
    sim.guild_role_with(senior, "Senior", 5, (1 << 40) | (1 << 2));
    sim.guild_role_with(junior, "Junior", 3, 1 << 40);
    sim.mock.member_roles.lock().unwrap().insert(original, vec![junior]);
    for kind in [
        pinchora_core::moderation::CaseKind::Warn,
        pinchora_core::moderation::CaseKind::Ban,
    ] {
        sqlx::query("UPDATE mod_appeals SET created_at=now()-interval '11 minutes' WHERE guild_id=$1")
            .bind(sim.guild as i64)
            .execute(sim.app.db.pool())
            .await
            .unwrap();
        let opened = sim
            .app
            .db
            .open_case(&pinchora_db::NewCase {
                guild: sim.guild,
                kind,
                target,
                target_name: "Member",
                moderator: original,
                moderator_name: "Staff",
                reason: Some("Initial sanction"),
                duration: None,
                expires_at: None,
                details: None,
            })
            .await
            .unwrap();
        let number = opened.case.number;
        if kind == pinchora_core::moderation::CaseKind::Ban {
            sim.mock.bans.lock().unwrap().push(target);
        }
        assert!(
            sim.app
                .db
                .submit_mod_appeal(sim.guild, target, number, "Please reconsider this sanction")
                .await
                .unwrap()
        );
        let id: i64 = sqlx::query_scalar("SELECT id FROM mod_appeals WHERE guild_id=$1 AND case_number=$2")
            .bind(sim.guild as i64)
            .bind(number)
            .fetch_one(sim.app.db.pool())
            .await
            .unwrap();
        for roles in [vec![], vec![junior]] {
            sim.mock.member_roles.lock().unwrap().insert(actor, roles);
            assert!(
                pinchora_commands::review_mod_parity(
                    &sim.app,
                    sim.guild,
                    (actor, "Reviewer"),
                    "appeals",
                    id,
                    true,
                    "Reviewed evidence"
                )
                .await
                .is_err()
            );
            assert!(sim.app.db.case(sim.guild, number).await.unwrap().unwrap().active);
        }
        sim.mock.member_roles.lock().unwrap().insert(actor, vec![senior]);
        pinchora_commands::review_mod_parity(
            &sim.app,
            sim.guild,
            (actor, "Reviewer"),
            "appeals",
            id,
            true,
            "Reviewed evidence",
        )
        .await
        .unwrap();
        assert!(!sim.app.db.case(sim.guild, number).await.unwrap().unwrap().active);
        assert!(!sim.mock.bans.lock().unwrap().contains(&target));
        let decision: (String, String, i64) =
            sqlx::query_as("SELECT status,decision,reviewer_id FROM mod_appeals WHERE id=$1")
                .bind(id)
                .fetch_one(sim.app.db.pool())
                .await
                .unwrap();
        assert_eq!(decision, ("accepted".into(), "Reviewed evidence".into(), actor as i64));
        assert!(
            pinchora_commands::review_mod_parity(
                &sim.app,
                sim.guild,
                (actor, "Reviewer"),
                "appeals",
                id,
                true,
                "Duplicate"
            )
            .await
            .is_err()
        );
    }
}

async fn restriction(sim: &mut Sim, actor: u64, target: u64, action: &str) -> Reply {
    sim.command_resolved(
        actor,
        "modtools",
        json!([sub(
            "restrict",
            vec![text("action", action), user_option("user", target)]
        )]),
        resolved(target),
    )
    .await
}

#[tokio::test]
async fn mod_parity_rejoin_restart_and_unsafe_restore_keep_original_snapshot() {
    let _serial = SWEEPS.lock().await;
    let Some(mut sim) = Sim::new("mod_parity_recovery").await else {
        return;
    };
    let (actor, target, staff, reward, mute) = (unique(), unique(), unique(), unique(), unique());
    let perms = (1 << 40) | (1 << 28);
    sim.guild_role_with(staff, "Staff", 5, perms);
    sim.guild_role(reward, "Member", 1);
    sim.guild_role(mute, "Muted", 2);
    sim.roles = vec![staff];
    sim.permissions = perms;
    sim.mock.member_roles.lock().unwrap().insert(actor, vec![staff]);
    sim.mock.member_roles.lock().unwrap().insert(target, vec![reward]);
    sim.app
        .db
        .save_mod_parity_config(
            sim.guild,
            &pinchora_core::mod_parity::Config {
                mute_role: Some(mute.to_string()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    // Never remove a privileged original role that this moderator cannot safely restore.
    for r in sim.mock.roles.lock().unwrap().iter_mut() {
        if r["id"].as_str().and_then(|id| id.parse::<u64>().ok()) == Some(reward) {
            r["permissions"] = json!((1_u64 << 3).to_string());
        }
    }
    assert!(
        restriction(&mut sim, actor, target, "mute")
            .await
            .content()
            .contains("refused")
    );
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM mod_restrictions WHERE guild_id=$1")
        .bind(sim.guild as i64)
        .fetch_one(sim.app.db.pool())
        .await
        .unwrap();
    assert_eq!(count, 0);
    assert_eq!(sim.mock.member_roles.lock().unwrap()[&target], vec![reward]);
    for r in sim.mock.roles.lock().unwrap().iter_mut() {
        if r["id"].as_str().and_then(|id| id.parse::<u64>().ok()) == Some(reward) {
            r["permissions"] = json!("0");
        }
    }
    // A failed Discord write still has durable recovery data.
    sim.mock.roles_allowed.store(false, Ordering::SeqCst);
    assert!(
        restriction(&mut sim, actor, target, "mute")
            .await
            .content()
            .contains("refused")
    );
    sim.mock.roles_allowed.store(true, Ordering::SeqCst);
    let restarted = App::new(
        sim.app.http.clone(),
        sim.app.http.clone(),
        Id::new(APPLICATION_ID),
        sim.app.db.clone(),
        sim.app.cache.clone(),
        "sim".into(),
    );
    reset_sweep_cooldown(&restarted).await;
    pinchora_commands::sweep_mod_parity(&restarted).await.unwrap();
    assert_sweep_lock_released();
    assert_eq!(sim.mock.member_roles.lock().unwrap()[&target], vec![mute]);
    sim.leave(target, "member", false).await;
    sim.mock.member_roles.lock().unwrap().insert(target, vec![]);
    sim.join(target, "member", false, None).await;
    assert_eq!(sim.mock.member_roles.lock().unwrap()[&target], vec![mute]);
    // A role acquires privileges while its member is jailed. Do not grant it or forget it.
    for r in sim.mock.roles.lock().unwrap().iter_mut() {
        if r["id"].as_str().and_then(|id| id.parse::<u64>().ok()) == Some(reward) {
            r["permissions"] = json!((1_u64 << 3).to_string());
        }
    }
    assert!(
        restriction(&mut sim, actor, target, "unmute")
            .await
            .content()
            .contains("refused")
    );
    let saved: Vec<i64> = sqlx::query_scalar("SELECT role_ids FROM mod_restrictions WHERE guild_id=$1 AND user_id=$2")
        .bind(sim.guild as i64)
        .bind(target as i64)
        .fetch_one(sim.app.db.pool())
        .await
        .unwrap();
    assert_eq!(saved, vec![reward as i64]);
    assert_eq!(sim.mock.member_roles.lock().unwrap()[&target], vec![mute]);
    // A legacy/manual restriction role in the snapshot must not be re-granted on release.
    sqlx::query("UPDATE mod_restrictions SET role_ids=array_append(role_ids,$3) WHERE guild_id=$1 AND user_id=$2")
        .bind(sim.guild as i64)
        .bind(target as i64)
        .bind(mute as i64)
        .execute(sim.app.db.pool())
        .await
        .unwrap();
    for r in sim.mock.roles.lock().unwrap().iter_mut() {
        if r["id"].as_str().and_then(|id| id.parse::<u64>().ok()) == Some(reward) {
            r["permissions"] = json!("0");
        }
    }
    assert!(
        restriction(&mut sim, actor, target, "unmute")
            .await
            .content()
            .contains("Saved")
    );
    assert_eq!(sim.mock.member_roles.lock().unwrap()[&target], vec![reward]);
}

#[tokio::test]
async fn mod_parity_fresh_hierarchy_self_owner_and_bot_boundaries() {
    let Some(mut sim) = Sim::new("mod_parity_hierarchy").await else {
        return;
    };
    let (actor, target, staff, held, owner) = (unique(), unique(), unique(), unique(), unique());
    let perms = (1 << 40) | (1 << 28) | (1 << 2) | (1 << 27);
    sim.guild_role_with(staff, "Staff", 5, perms);
    sim.guild_role(held, "Target", 1);
    sim.roles = vec![staff];
    sim.permissions = perms;
    sim.mock.member_roles.lock().unwrap().insert(actor, vec![staff]);
    sim.mock.member_roles.lock().unwrap().insert(target, vec![held]);
    sim.mock.owner.store(owner, Ordering::SeqCst);
    for uid in [actor, owner, APPLICATION_ID] {
        assert!(
            restriction(&mut sim, actor, uid, "softban")
                .await
                .content()
                .contains("refused")
        );
    }
    for position in [5, 6, 10, 11] {
        for role in sim.mock.roles.lock().unwrap().iter_mut() {
            if role["id"].as_str().and_then(|id| id.parse::<u64>().ok()) == Some(held) {
                role["position"] = json!(position);
            }
        }
        assert!(
            restriction(&mut sim, actor, target, "softban")
                .await
                .content()
                .contains("refused")
        );
    }
    let renamed = sim
        .command_resolved(
            actor,
            "modtools",
            json!([sub("names", vec![user_option("user", target)])]),
            resolved(target),
        )
        .await;
    assert!(
        renamed.content().contains("refused"),
        "stale interaction must not bypass nickname hierarchy"
    );
    for role in sim.mock.roles.lock().unwrap().iter_mut() {
        if role["id"].as_str().and_then(|id| id.parse::<u64>().ok()) == Some(held) {
            role["position"] = json!(1);
        }
    }
    assert!(
        restriction(&mut sim, actor, target, "softban")
            .await
            .content()
            .contains("Saved")
    );
    assert!(
        sim.mock
            .mod_calls
            .lock()
            .unwrap()
            .contains(&format!("ban {target} 86400"))
    );
    // Revocation after a successful call takes effect immediately, despite interaction permissions.
    sim.mock.member_roles.lock().unwrap().insert(actor, vec![]);
    assert!(
        restriction(&mut sim, actor, target, "softban")
            .await
            .content()
            .contains("refused")
    );
}

#[tokio::test]
async fn mod_parity_bulk_partial_failures_replay_and_rate_limit() {
    let Some(mut sim) = Sim::new("mod_parity_bulk_limits").await else {
        return;
    };
    let (actor, target, staff) = (unique(), unique(), unique());
    sim.guild_role_with(staff, "Staff", 5, 1 << 2);
    sim.roles = vec![staff];
    sim.permissions = 1 << 2;
    sim.mock.member_roles.lock().unwrap().insert(actor, vec![staff]);
    let options = json!([sub(
        "mass",
        vec![
            text("action", "ban"),
            text("ids", &format!("{actor} {target}")),
            text("reason", "reviewed evidence")
        ]
    )]);
    let preview = sim.command(actor, "modtools", options.clone()).await;
    assert!(sim.mock.bans.lock().unwrap().is_empty());
    let token = preview.content().split("confirm:").last().unwrap().trim().to_owned();
    let confirm = json!([sub("mass", vec![text("action", "ban"), text("confirm", &token)])]);
    let result = sim.command(actor, "modtools", confirm.clone()).await;
    assert!(result.content().contains("Completed 1 of 2"), "{}", result.content());
    assert!(result.content().contains(&actor.to_string()));
    assert_eq!(sim.mock.bans.lock().unwrap().as_slice(), &[target]);
    assert!(
        AUDITS.lock().unwrap()[&sim.guild]
            .iter()
            .any(|reason| reason.contains("reviewed") && reason.contains("evidence"))
    );
    assert!(
        sim.command(actor, "modtools", confirm)
            .await
            .content()
            .contains("Check")
    );
    let preview = sim.command(actor, "modtools", options).await;
    let token = preview.content().split("confirm:").last().unwrap().trim().to_owned();
    let result = sim
        .command(
            actor,
            "modtools",
            json!([sub("mass", vec![text("action", "ban"), text("confirm", &token)])]),
        )
        .await;
    assert!(result.content().contains("Wait a minute"));
}

#[tokio::test]
async fn mod_parity_reports_never_publish_reporter_in_cases() {
    let Some(mut sim) = Sim::new("mod_parity_anonymity").await else {
        return;
    };
    let (reporter, target) = (unique(), unique());
    sim.app
        .db
        .save_mod_parity_config(
            sim.guild,
            &pinchora_core::mod_parity::Config {
                report_channel: Some(CHANNEL_ID.to_string()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    let result = sim
        .command_resolved(
            reporter,
            "modtools",
            json!([sub(
                "report",
                vec![user_option("user", target), text("reason", "evidence")]
            )]),
            resolved(target),
        )
        .await;
    assert!(result.content().contains("Saved"));
    let moderators: Vec<i64> =
        sqlx::query_scalar("SELECT moderator_id FROM mod_cases WHERE guild_id=$1 AND kind='report'")
            .bind(sim.guild as i64)
            .fetch_all(sim.app.db.pool())
            .await
            .unwrap();
    assert_eq!(moderators, vec![APPLICATION_ID as i64]);
    assert!(
        sim.mock
            .posts
            .lock()
            .unwrap()
            .iter()
            .all(|p| !p.body.to_string().contains(&reporter.to_string()))
    );
}

#[tokio::test]
async fn mod_parity_role_delete_uses_live_role_not_resolved_snapshot() {
    let Some(mut sim) = Sim::new("mod_parity_role_snapshot").await else {
        return;
    };
    let (actor, staff, role) = (unique(), unique(), unique());
    sim.guild_role_with(staff, "Staff", 5, 1 << 28);
    let stale = sim.guild_role(role, "Reward", 1);
    sim.permissions = 1 << 28;
    sim.roles = vec![staff];
    sim.mock.member_roles.lock().unwrap().insert(actor, vec![staff]);
    for (position, managed) in [(5, false), (11, false), (1, true), (1, false)] {
        // The last case revokes Administrator but keeps Manage Roles in the fresh member.
        sim.permissions = (1 << 28) | (1 << 3);
        for r in sim.mock.roles.lock().unwrap().iter_mut() {
            if r["id"].as_str().and_then(|id| id.parse::<u64>().ok()) == Some(role) {
                r["position"] = json!(position);
                r["managed"] = json!(managed);
                r["permissions"] = json!((1_u64 << 3).to_string());
            }
        }
        let reply = sim
            .command_resolved(
                actor,
                "modtools",
                json!([sub(
                    "roles",
                    vec![
                        text("action", "delete"),
                        json!({"name":"role","type":8,"value":role.to_string()}),
                        json!({"name":"confirm","type":5,"value":true})
                    ]
                )]),
                json!({"roles":{role.to_string():stale}}),
            )
            .await;
        assert!(reply.content().contains("Check"));
        assert!(
            sim.mock
                .roles
                .lock()
                .unwrap()
                .iter()
                .any(|r| r["id"].as_str().and_then(|id| id.parse::<u64>().ok()) == Some(role))
        );
    }
}

fn voice_state(guild: u64, user: u64, channel: u64) -> twilight_model::voice::VoiceState {
    serde_json::from_value(json!({"guild_id":guild.to_string(),"user_id":user.to_string(),"channel_id":channel.to_string(),"session_id":"test","deaf":false,"mute":false,"self_deaf":false,"self_mute":false,"self_video":false,"suppress":false})).unwrap()
}

#[tokio::test]
async fn mod_parity_voice_failure_is_reported_and_persistent_ban_grants_no_voice_reward() {
    let Some(mut sim) = Sim::new("mod_parity_voice_failure").await else {
        return;
    };
    let (actor, target, staff, reward, channel) = (unique(), unique(), unique(), unique(), unique());
    let perms = (1 << 24) | (1 << 10);
    sim.guild_role_with(staff, "Staff", 5, perms);
    sim.guild_role(reward, "Voice", 1);
    sim.permissions = perms;
    sim.roles = vec![staff];
    sim.mock.member_roles.lock().unwrap().insert(actor, vec![staff]);
    sim.channel_is(channel, 2, None);
    sim.app
        .db
        .save_mod_parity_config(
            sim.guild,
            &pinchora_core::mod_parity::Config {
                conditions: vec![pinchora_core::mod_parity::RoleCondition {
                    role: reward.to_string(),
                    requires: vec![],
                    all: true,
                    voice_channel: Some(channel.to_string()),
                }],
                ..Default::default()
            },
        )
        .await
        .unwrap();
    sim.mock.roles_allowed.store(false, Ordering::SeqCst);
    let reply = sim
        .command_resolved(
            actor,
            "modtools",
            json!([sub("voice", vec![text("action", "ban"), user_option("user", target)])]),
            resolved(target),
        )
        .await;
    assert!(reply.content().contains("Completed 0 of 1"), "{}", reply.content());
    assert!(reply.content().contains(&target.to_string()));
    sim.mock.roles_allowed.store(true, Ordering::SeqCst);
    pinchora_commands::on_mod_parity_voice(&sim.app, &voice_state(sim.guild, target, channel)).await;
    assert!(
        sim.mock
            .mod_calls
            .lock()
            .unwrap()
            .iter()
            .any(|call| call.contains(&target.to_string()) && call.contains("channel_id"))
    );
    assert!(
        !sim.mock
            .member_roles
            .lock()
            .unwrap()
            .get(&target)
            .is_some_and(|r| r.contains(&reward))
    );
}

#[tokio::test]
async fn mod_parity_voice_snapshot_removes_disconnected_members_and_respects_channel_denies() {
    let Some(mut sim) = Sim::new("mod_parity_voice_snapshot").await else {
        return;
    };
    let (actor, target, staff, channel) = (unique(), unique(), unique(), unique());
    let perms = (1 << 24) | (1 << 10);
    sim.guild_role_with(staff, "Staff", 5, perms);
    sim.permissions = perms;
    sim.roles = vec![staff];
    sim.mock.member_roles.lock().unwrap().insert(actor, vec![staff]);
    sim.channel_is(channel, 2, None);
    sim.mock.channels.lock().unwrap().get_mut(&channel).unwrap()["permission_overwrites"] =
        json!([{"id":staff.to_string(),"type":0,"deny":(1_u64 << 24).to_string(),"allow":"0"}]);
    pinchora_commands::on_mod_parity_voice(&sim.app, &voice_state(sim.guild, target, channel)).await;
    let reply = sim
        .command_resolved(
            actor,
            "modtools",
            json!([sub(
                "voice",
                vec![text("action", "disconnect"), user_option("user", target)]
            )]),
            resolved(target),
        )
        .await;
    assert!(reply.content().contains("Completed 0 of 1"));
    let key = format!("modparity:voice-v2:{}", sim.guild);
    sim.app
        .cache
        .map_set(
            &key,
            &target.to_string(),
            &format!("{channel}|0|1"),
            Duration::from_secs(100),
        )
        .await
        .unwrap();
    let mut guild = guild_json(sim.guild);
    guild["voice_states"] = json!([]);
    let event = serde_json::from_value(guild).unwrap();
    pinchora_commands::seed_mod_parity_voice(&sim.app, &event);
    sim.app.drain(Duration::from_secs(5)).await;
    assert!(sim.app.cache.map_all(&key).await.unwrap().is_empty());
}

#[tokio::test]
async fn mod_parity_restriction_refuses_channel_overwrite_bypass_without_snapshot() {
    let Some(mut sim) = Sim::new("mod_parity_restriction_overwrite").await else {
        return;
    };
    let (actor, target, staff, mute, channel) = (unique(), unique(), unique(), unique(), unique());
    let perms = (1 << 40) | (1 << 28);
    sim.guild_role_with(staff, "Staff", 5, perms);
    sim.guild_role(mute, "Mute", 1);
    sim.permissions = perms;
    sim.roles = vec![staff];
    sim.mock.member_roles.lock().unwrap().insert(actor, vec![staff]);
    sim.channel_is(channel, 0, None);
    sim.mock.channels.lock().unwrap().get_mut(&channel).unwrap()["permission_overwrites"] =
        json!([{"id":target.to_string(),"type":1,"allow":(1_u64 << 11).to_string(),"deny":"0"}]);
    sim.app
        .db
        .save_mod_parity_config(
            sim.guild,
            &pinchora_core::mod_parity::Config {
                mute_role: Some(mute.to_string()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert!(
        restriction(&mut sim, actor, target, "mute")
            .await
            .content()
            .contains("refused")
    );
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM mod_restrictions WHERE guild_id=$1")
        .bind(sim.guild as i64)
        .fetch_one(sim.app.db.pool())
        .await
        .unwrap();
    assert_eq!(count, 0);
    assert!(sim.mock.mod_calls.lock().unwrap().is_empty());
}

#[tokio::test]
async fn mod_parity_automated_roles_respect_live_target_hierarchy() {
    let Some(sim) = Sim::new("mod_parity_auto_hierarchy").await else {
        return;
    };
    let (target, senior, reward, channel) = (unique(), unique(), unique(), unique());
    sim.guild_role(senior, "Senior", 11);
    sim.guild_role(reward, "Voice", 1);
    sim.mock.member_roles.lock().unwrap().insert(target, vec![senior]);
    sim.app
        .db
        .save_mod_parity_config(
            sim.guild,
            &pinchora_core::mod_parity::Config {
                conditions: vec![pinchora_core::mod_parity::RoleCondition {
                    role: reward.to_string(),
                    requires: vec![],
                    all: true,
                    voice_channel: Some(channel.to_string()),
                }],
                ..Default::default()
            },
        )
        .await
        .unwrap();
    pinchora_commands::on_mod_parity_voice(&sim.app, &voice_state(sim.guild, target, channel)).await;
    assert_eq!(sim.mock.member_roles.lock().unwrap()[&target], vec![senior]);
    sim.mock.member_roles.lock().unwrap().insert(target, vec![]);
    pinchora_commands::on_mod_parity_voice(&sim.app, &voice_state(sim.guild, target, channel)).await;
    assert_eq!(sim.mock.member_roles.lock().unwrap()[&target], vec![reward]);
}

#[tokio::test]
async fn mod_parity_restricted_booster_cannot_grant_a_personal_role() {
    let Some(mut sim) = Sim::new("mod_parity_restricted_booster").await else {
        return;
    };
    let user = unique();
    let mute = unique();
    BOOSTING.lock().unwrap().push(user);
    sim.guild_role(mute, "Mute", 1);
    sim.mock.member_roles.lock().unwrap().insert(user, vec![mute]);
    sim.app
        .db
        .save_mod_parity_config(
            sim.guild,
            &pinchora_core::mod_parity::Config {
                booster_personal: true,
                ..Default::default()
            },
        )
        .await
        .unwrap();
    sqlx::query("INSERT INTO mod_restrictions(guild_id,user_id,role_id,role_ids,kind) VALUES($1,$2,$3,'{}','mute')")
        .bind(sim.guild as i64)
        .bind(user as i64)
        .bind(mute as i64)
        .execute(sim.app.db.pool())
        .await
        .unwrap();
    let reply = sim
        .command(
            user,
            "modtools",
            json!([sub("roles", vec![text("action", "booster"), text("name", "My role")])]),
        )
        .await;
    assert!(reply.content().contains("Check"));
    assert!(sim.mock.role_calls.lock().unwrap().is_empty());
    assert_eq!(sim.mock.roles.lock().unwrap().len(), 1);
    sqlx::query("DELETE FROM mod_restrictions WHERE guild_id=$1")
        .bind(sim.guild as i64)
        .execute(sim.app.db.pool())
        .await
        .unwrap();
}

#[tokio::test]
async fn mod_parity_report_blocks_respect_self_owner_and_staff_hierarchy() {
    let Some(mut sim) = Sim::new("mod_parity_block_hierarchy").await else {
        return;
    };
    let (actor, target, owner, staff) = (unique(), unique(), unique(), unique());
    let perms = (1 << 5) | (1 << 40);
    sim.guild_role_with(staff, "Staff", 5, perms);
    sim.permissions = perms;
    sim.roles = vec![staff];
    sim.mock.member_roles.lock().unwrap().insert(actor, vec![staff]);
    sim.mock.member_roles.lock().unwrap().insert(target, vec![staff]);
    sim.own_server(owner);
    for user in [actor, target, owner] {
        let reply = sim
            .command_resolved(
                actor,
                "modtools",
                json!([sub(
                    "policy",
                    vec![
                        user_option("report_user", user),
                        json!({"name":"blocked","type":5,"value":true})
                    ]
                )]),
                resolved(user),
            )
            .await;
        assert!(reply.content().contains("refused"), "{}", reply.content());
    }
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM mod_report_blocks WHERE guild_id=$1")
        .bind(sim.guild as i64)
        .fetch_one(sim.app.db.pool())
        .await
        .unwrap();
    assert_eq!(count, 0);
}

#[tokio::test]
async fn mod_parity_autopurge_scans_past_filtered_pages_and_deletes_old_messages_singly() {
    let _serial = SWEEPS.lock().await;
    let Some(sim) = Sim::new("mod_parity_autopurge_cursor").await else {
        return;
    };
    // Leave three connections for the sweep lease, restriction ban lock and queries.
    // Two would starve restriction recovery when another fixture has an active mute.
    let _reserved = [
        sim.app.db.pool().acquire().await.unwrap(),
        sim.app.db.pool().acquire().await.unwrap(),
    ];
    let channel = unique();
    PAGED_CHANNELS.lock().unwrap().push(channel);
    let oldest = (((sim.app.now() - chrono::Duration::days(30)).timestamp_millis() - 1_420_070_400_000) as u64) << 22;
    let mut history = Vec::new();
    for n in (0..102).rev() {
        let mut msg = message_json(channel, oldest + n);
        msg["author"] = person_json(unique(), "author", None, n < 2);
        msg["pinned"] = json!(n == 1);
        history.push(msg);
    }
    sim.mock.history.lock().unwrap().insert(channel, history);
    sim.app
        .db
        .save_mod_parity_config(
            sim.guild,
            &pinchora_core::mod_parity::Config {
                autopurge: vec![pinchora_core::mod_parity::AutoPurge {
                    channel: channel.to_string(),
                    age_secs: 3600,
                    bots_only: true,
                }],
                ..Default::default()
            },
        )
        .await
        .unwrap();
    // Use a distinct scheduler minute; other tests also exercise the fleet lease.
    sim.app.shift_clock(chrono::Duration::hours(4));
    reset_sweep_cooldown(&sim.app).await;
    pinchora_commands::sweep_mod_parity(&sim.app).await.unwrap();
    assert_sweep_lock_released();
    assert!(sim.mock.deleted_messages.lock().unwrap().is_empty());
    sim.app.shift_clock(chrono::Duration::minutes(1));
    reset_sweep_cooldown(&sim.app).await;
    pinchora_commands::sweep_mod_parity(&sim.app).await.unwrap();
    assert_sweep_lock_released();
    assert_eq!(
        sim.mock.deleted_messages.lock().unwrap().as_slice(),
        &[(channel, oldest)]
    );
    assert!(
        !sim.mock
            .mod_calls
            .lock()
            .unwrap()
            .iter()
            .any(|c| c.starts_with("bulk-delete"))
    );
    sim.app
        .db
        .save_mod_parity_config(sim.guild, &Default::default())
        .await
        .unwrap();
}

fn assert_sweep_lock_released() {
    // A separate connection cannot flush the lease connection's pending rollback.
    // Block this current-thread test runtime while probing so its pool-return task
    // cannot hide a transaction that the sweep dropped without awaiting completion.
    let acquired = std::thread::spawn(|| {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(async {
                let mut connection = <sqlx::PgConnection as sqlx::Connection>::connect(
                    &std::env::var("PINCHORA_TEST_DATABASE_URL").unwrap(),
                )
                .await
                .unwrap();
                sqlx::query_scalar::<_, bool>("SELECT pg_try_advisory_xact_lock(74, 0)")
                    .fetch_one(&mut connection)
                    .await
                    .unwrap()
            })
    })
    .join()
    .unwrap();
    assert!(acquired, "a completed sweep must have released its advisory lock");
}

#[tokio::test]
async fn sweep_releases_lease_on_cooldown_and_error() {
    let _serial = SWEEPS.lock().await;
    let Some(sim) = Sim::new("sweep_lease_exits").await else {
        return;
    };
    sim.app.shift_clock(chrono::Duration::hours(8));
    reset_sweep_cooldown(&sim.app).await;
    pinchora_commands::sweep_mod_parity(&sim.app).await.unwrap();
    assert_sweep_lock_released();
    // The same minute returns early from the cooldown path.
    pinchora_commands::sweep_mod_parity(&sim.app).await.unwrap();
    assert_sweep_lock_released();
    sim.app
        .db
        .save_mod_parity_config(sim.guild, &Default::default())
        .await
        .unwrap();
    sqlx::query(r#"UPDATE mod_parity_settings SET config='{"autopurge":false}'::jsonb WHERE guild_id=$1"#)
        .bind(sim.guild as i64)
        .execute(sim.app.db.pool())
        .await
        .unwrap();
    sim.app.shift_clock(chrono::Duration::minutes(1));
    reset_sweep_cooldown(&sim.app).await;
    let result = pinchora_commands::sweep_mod_parity(&sim.app).await;
    assert_sweep_lock_released();
    // Restore the malformed fixture even when the expected error is missing.
    sim.app
        .db
        .save_mod_parity_config(sim.guild, &Default::default())
        .await
        .unwrap();
    assert!(result.is_err(), "malformed stored settings must propagate an error");
}

async fn reset_sweep_cooldown(app: &App) {
    // SWEEPS serializes these fixtures; clear only their minute so repeated runs
    // against the same test Redis database exercise work instead of an old cooldown.
    app.cache
        .release_cooldown(&format!("modparity:autopurge:{}", app.now().timestamp() / 60))
        .await
        .unwrap();
}
