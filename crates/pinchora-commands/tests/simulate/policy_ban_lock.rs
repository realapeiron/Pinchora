use crate::*;
use std::time::Duration;

fn text(name: &str, value: &str) -> Value {
    json!({"name":name,"type":3,"value":value})
}
fn resolved(user: u64) -> Value {
    json!({"users":{user.to_string():person_json(user,"member",None,false)}})
}

async fn block_case_counter(
    app: &pinchora_commands::App,
    guild: u64,
    actor: u64,
    target: u64,
) -> (i32, sqlx::Transaction<'_, sqlx::Postgres>) {
    use pinchora_core::moderation::moderation::CaseKind;
    let old = app
        .db
        .open_case(&pinchora_db::NewCase {
            guild,
            kind: CaseKind::Ban,
            target,
            target_name: "Mia",
            moderator: actor,
            moderator_name: "Staff",
            reason: None,
            duration: None,
            expires_at: None,
            details: None,
        })
        .await
        .unwrap()
        .case
        .number;
    let mut held = app.db.pool().begin().await.unwrap();
    sqlx::query("UPDATE mod_case_counters SET last_case = last_case WHERE guild_id = $1")
        .bind(guild as i64)
        .execute(&mut *held)
        .await
        .unwrap();
    (old, held)
}

#[tokio::test]
async fn policy_ban_holds_member_lock_until_its_case_is_written() {
    use pinchora_commands::{RevokeActor, revoke_case};
    use pinchora_core::{infra::i18n::Lang, moderation::moderation::CaseKind};
    let Some(mut sim) = Sim::new("policy_ban_case_lock").await else {
        return;
    };
    let (actor, target, role) = (unique(), unique(), unique());
    sim.guild_role_with(role, "Staff", 5, 1 << 2);
    sim.roles = vec![role];
    sim.permissions = (1 << 2) | MANAGE_GUILD;
    sim.mock.member_roles.lock().unwrap().insert(actor, vec![role]);
    let app = sim.app.clone();
    let guild = sim.guild;
    let (old, held) = block_case_counter(&app, guild, actor, target).await;
    sim.mock.bans.lock().unwrap().push(target);
    let mock = sim.mock.clone();
    let sanction = sim.command_resolved(
        actor,
        "modtools",
        json!([sub(
            "restrict",
            vec![
                text("action", "ban"),
                user_option("user", target),
                text("reason", "staff reviewed")
            ]
        )]),
        resolved(target),
    );
    let concurrent = async {
        tokio::time::timeout(Duration::from_secs(3), async {
            loop {
                if mock
                    .mod_calls
                    .lock()
                    .unwrap()
                    .iter()
                    .any(|call| call.starts_with("ban "))
                {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        let undo = revoke_case(
            &app,
            guild,
            old,
            RevokeActor {
                id: actor,
                name: "Staff",
                roles: vec![role],
                source: "command",
                target_name: None,
                language: Lang::En,
            },
            None,
        );
        tokio::pin!(undo);
        assert!(
            tokio::time::timeout(Duration::from_millis(200), &mut undo)
                .await
                .is_err(),
            "revoke passed the policy ban before its case was stored"
        );
        assert!(
            !mock
                .mod_calls
                .lock()
                .unwrap()
                .iter()
                .any(|call| call.starts_with("unban ")),
            "Discord was unbanned before the policy ban case was stored"
        );
        held.commit().await.unwrap();
        undo.await
    };
    let (reply, revoked) = tokio::join!(sanction, concurrent);
    assert!(reply.content().contains("Saved"), "{}", reply.content());
    assert!(matches!(revoked, Err(pinchora_commands::RevokeError::Inactive)));
    assert!(!app.db.case(guild, old).await.unwrap().unwrap().active);
    assert!(
        app.db
            .active_case_for(guild, target, CaseKind::Ban)
            .await
            .unwrap()
            .is_some()
    );
}
