use super::*;
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
pub(super) fn mock(state: &Mock, method: &Method, parts: &[&str], body: &[u8]) -> Option<(StatusCode, String)> {
    if let (&Method::GET, [.., "guilds", _, "channels"]) = (method, parts) {
        return Some((
            StatusCode::OK,
            serde_json::to_string(&state.channels.lock().unwrap().values().cloned().collect::<Vec<_>>()).unwrap(),
        ));
    }
    if let (&Method::PATCH, [.., "guilds", _, "members", user]) = (method, parts) {
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
