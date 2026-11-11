use super::*;

#[tokio::test]
async fn unban_retries_a_ban_replaced_while_waiting_for_its_lock() {
    use pinchora_core::moderation::moderation::CaseKind;
    let Some(mut sim) = Sim::new("unban_retries_a_ban_replaced_while_waiting_for_its_lock").await else {
        return;
    };
    let (moderator, target, mod_role) = (unique(), unique(), unique());
    sim.guild_role(mod_role, "Moderators", 5);
    sim.roles = vec![mod_role];
    sim.permissions = STAFF;
    sim.command_resolved(
        moderator,
        "ban",
        json!([user_option("user", target)]),
        member(target, "Mia"),
    )
    .await;
    let app = sim.app.clone();
    let guild = sim.guild;
    let old = sim.cases().await[0].number;
    let held = app.db.lock_ban(guild, target).await.unwrap();
    let command = sim.command_resolved(
        moderator,
        "unban",
        json!([user_option("user", target)]),
        someone(target, "Mia", None),
    );
    let replace = async {
        let key = format!("moderation-ban:{guild}:{target}");
        tokio::time::timeout(Duration::from_secs(3), async {
            loop {
                let waiting: i64 = sqlx::query_scalar(
                    "SELECT count(*) FROM pg_locks WHERE locktype = 'advisory' AND NOT granted
                    AND classid::bigint = ((hashtextextended($1, 0) >> 32) & 4294967295)
                    AND objid::bigint = (hashtextextended($1, 0) & 4294967295)",
                )
                .bind(&key)
                .fetch_one(app.db.pool())
                .await
                .unwrap();
                if waiting > 0 {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("unban reached the ban lock after selecting the old case");
        let latest = app
            .db
            .open_case(&pinchora_db::NewCase {
                guild,
                kind: CaseKind::Ban,
                target,
                target_name: "Mia",
                moderator,
                moderator_name: "Sim",
                reason: None,
                duration: None,
                expires_at: None,
                details: None,
            })
            .await
            .unwrap()
            .case
            .number;
        assert!(!app.db.case(guild, old).await.unwrap().unwrap().active);
        held.commit().await.unwrap();
        latest
    };
    let (reply, latest) = tokio::join!(command, replace);
    assert!(reply.description().contains("was unbanned"), "{}", reply.description());
    assert!(!app.db.case(guild, latest).await.unwrap().unwrap().active);
    assert!(sim.cases().await.iter().any(|c| c.kind == "unban"));
}
