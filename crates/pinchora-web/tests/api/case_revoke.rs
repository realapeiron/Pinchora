use super::*;

// The write-failure fixture changes mod_cases schema and must not queue behind a stalled reversal.
static SCHEMA_CHANGE_GUARD: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

#[tokio::test]
async fn case_reason_edits_require_the_author_or_a_higher_moderator() {
    use pinchora_core::moderation::moderation::CaseKind;
    let Some(h) = Harness::new("reason_hierarchy").await else {
        return;
    };
    let (guild, junior, senior, target) = (unique(), unique(), unique(), unique());
    h.mock.bot_guilds.lock().unwrap().insert(guild);
    h.mock
        .member_roles
        .lock()
        .unwrap()
        .extend([(junior, vec![FAN]), (senior, vec![REGULAR]), (target, vec![REGULAR])]);
    let mut browser = h.browser();
    browser
        .login(
            &h,
            junior,
            vec![guild_entry(guild, "Reef", false, MANAGE_GUILD | ADMINISTRATOR)],
        )
        .await;
    let db = &h.state.app.db;
    for moderator in [senior, junior, APPLICATION_ID, 0] {
        db.open_case(&pinchora_db::NewCase {
            guild,
            kind: CaseKind::Warn,
            target,
            target_name: "Mia",
            moderator,
            moderator_name: "Mod",
            reason: Some("original"),
            duration: None,
            expires_at: None,
            details: None,
        })
        .await
        .unwrap();
    }
    let path = |number| format!("/api/guilds/{guild}/cases/{number}");
    for number in [1, 3, 4] {
        let denied = browser
            .write(Method::PATCH, &path(number), json!({"reason": "bypass"}))
            .await;
        assert_eq!(denied.status, StatusCode::FORBIDDEN, "{}", denied.json());
        assert_eq!(denied.json()["error"]["code"], "hierarchy");
        assert_eq!(
            db.case(guild, number).await.unwrap().unwrap().reason.as_deref(),
            Some("original")
        );
    }
    let own = browser.write(Method::PATCH, &path(2), json!({"reason": "own"})).await;
    assert_eq!(own.status, StatusCode::OK);
    let controls = browser
        .write(Method::PATCH, &path(2), json!({"reason": "bad\u{0000}reason"}))
        .await;
    assert_eq!(controls.status, StatusCode::BAD_REQUEST, "{}", controls.text());
    assert!(controls.text().contains("control characters"));
    h.mock.member_roles.lock().unwrap().insert(junior, vec![REGULAR]);
    assert_eq!(
        browser
            .write(Method::PATCH, &path(1), json!({"reason": "peer"}))
            .await
            .status,
        StatusCode::FORBIDDEN
    );
    h.mock.member_roles.lock().unwrap().insert(junior, vec![ADMIN]);
    for number in [1, 3, 4] {
        let higher = browser
            .write(Method::PATCH, &path(number), json!({"reason": "higher"}))
            .await;
        assert_eq!(higher.status, StatusCode::OK, "{}", higher.json());
    }
}

#[tokio::test]
async fn case_revoke_checks_access_hierarchy_discord_and_concurrency() {
    use pinchora_core::moderation::moderation::CaseKind;
    let Some(h) = Harness::new("case_revoke").await else {
        return;
    };
    let (guild, actor, giver, target) = (unique(), unique(), unique(), unique());
    h.add_bot(guild);
    h.mock
        .member_roles
        .lock()
        .unwrap()
        .extend([(actor, vec![FAN]), (giver, vec![REGULAR]), (target, vec![REGULAR])]);
    let mut browser = h.browser();
    browser
        .login(&h, actor, vec![guild_entry(guild, "Reef", true, 0)])
        .await;
    let db = &h.state.app.db;
    db.set_mod_settings(guild, Some(Some(GENERAL)), None).await.unwrap();
    let open = |kind, target_id| pinchora_db::NewCase {
        guild,
        kind,
        target: target_id,
        target_name: "Mia",
        moderator: giver,
        moderator_name: "Giver",
        reason: Some("original"),
        duration: None,
        expires_at: None,
        details: None,
    };
    let warn = db.open_case(&open(CaseKind::Warn, target)).await.unwrap().case.number;
    let timeout = db
        .open_case(&open(CaseKind::Timeout, target))
        .await
        .unwrap()
        .case
        .number;
    let ban = db.open_case(&open(CaseKind::Ban, target)).await.unwrap().case.number;
    let kick = db.open_case(&open(CaseKind::Kick, target)).await.unwrap().case.number;
    let lock = db.open_case(&open(CaseKind::Lock, GENERAL)).await.unwrap().case.number;
    let path = |n| format!("/api/guilds/{guild}/cases/{n}/revoke");
    let denied = browser.write(Method::POST, &path(warn), json!({})).await;
    assert_eq!(denied.status, StatusCode::FORBIDDEN);
    assert_eq!(denied.code(), "hierarchy");
    let lower = unique();
    h.mock
        .member_roles
        .lock()
        .unwrap()
        .extend([(actor, vec![HELPER]), (lower, vec![BOOSTER])]);
    let permission_case = db
        .open_case(&pinchora_db::NewCase {
            guild,
            kind: CaseKind::Warn,
            target: lower,
            target_name: "Low",
            moderator: 0,
            moderator_name: "Pinchora",
            reason: None,
            duration: None,
            expires_at: None,
            details: None,
        })
        .await
        .unwrap()
        .case
        .number;
    let permission = browser.write(Method::POST, &path(permission_case), json!({})).await;
    assert_eq!(permission.status, StatusCode::FORBIDDEN);
    assert_eq!(permission.code(), "permission");
    let revoked_power_case = assert_revoked_permission(&h, &mut browser, guild, actor, lower, permission_case).await;
    let invalid = browser
        .write(Method::POST, &path(warn), json!({"reason":"x".repeat(513)}))
        .await;
    assert_eq!(invalid.status, StatusCode::UNPROCESSABLE_ENTITY);
    let controls = browser
        .write(Method::POST, &path(warn), json!({"reason":"bad\u{0000}reason"}))
        .await;
    assert_eq!(controls.status, StatusCode::BAD_REQUEST, "{}", controls.text());
    assert!(controls.text().contains("control characters"));
    for number in [kick, lock] {
        let wrong = browser.write(Method::POST, &path(number), json!({})).await;
        assert_eq!(wrong.status, StatusCode::BAD_REQUEST);
        assert_eq!(wrong.code(), "kind");
    }
    assert_eq!(
        browser.write(Method::POST, &path(999), json!({})).await.status,
        StatusCode::NOT_FOUND
    );
    assert_reversal_results(&h, &mut browser, guild, giver, target, (warn, timeout, ban)).await;
    assert_delegated_access(&h, guild, actor, revoked_power_case).await;
}

async fn assert_revoked_permission(
    h: &Harness,
    browser: &mut Browser,
    guild: u64,
    actor: u64,
    lower: u64,
    permission_case: i32,
) -> i32 {
    use pinchora_core::moderation::moderation::CaseKind;
    let db = &h.state.app.db;
    let path = |n| format!("/api/guilds/{guild}/cases/{n}/revoke");
    h.mock.member_roles.lock().unwrap().insert(actor, vec![ADMIN]);
    let warm = browser.write(Method::POST, &path(permission_case), json!({})).await;
    assert_eq!(warm.status, StatusCode::OK, "{}", warm.text());
    let revoked_power_case = db
        .open_case(&pinchora_db::NewCase {
            guild,
            kind: CaseKind::Warn,
            target: lower,
            target_name: "Low",
            moderator: 0,
            moderator_name: "Pinchora",
            reason: None,
            duration: None,
            expires_at: None,
            details: None,
        })
        .await
        .unwrap()
        .case
        .number;
    h.mock.role_permissions.lock().unwrap().insert(ADMIN, 0);
    let lost = browser.write(Method::POST, &path(revoked_power_case), json!({})).await;
    assert_eq!(lost.status, StatusCode::FORBIDDEN, "{}", lost.text());
    assert_eq!(lost.code(), "permission");
    h.mock.role_permissions.lock().unwrap().remove(&ADMIN);
    h.mock.member_roles.lock().unwrap().insert(actor, vec![HELPER]);
    h.mock
        .role_permissions
        .lock()
        .unwrap()
        .insert(HELPER, twilight_model::guild::Permissions::BAN_MEMBERS.bits());
    let ban_case = db
        .open_case(&pinchora_db::NewCase {
            guild,
            kind: CaseKind::Ban,
            target: lower,
            target_name: "Low",
            moderator: 0,
            moderator_name: "Pinchora",
            reason: None,
            duration: None,
            expires_at: None,
            details: None,
        })
        .await
        .unwrap()
        .case
        .number;
    let allowed_ban = browser.write(Method::POST, &path(ban_case), json!({})).await;
    assert_eq!(allowed_ban.status, StatusCode::OK, "{}", allowed_ban.text());
    h.mock.role_permissions.lock().unwrap().insert(HELPER, 0);
    let next_ban = db
        .open_case(&pinchora_db::NewCase {
            guild,
            kind: CaseKind::Ban,
            target: lower,
            target_name: "Low",
            moderator: 0,
            moderator_name: "Pinchora",
            reason: None,
            duration: None,
            expires_at: None,
            details: None,
        })
        .await
        .unwrap()
        .case
        .number;
    let lost_ban = browser.write(Method::POST, &path(next_ban), json!({})).await;
    assert_eq!(lost_ban.status, StatusCode::FORBIDDEN, "{}", lost_ban.text());
    assert_eq!(lost_ban.code(), "permission");
    h.mock.role_permissions.lock().unwrap().remove(&HELPER);
    h.mock.member_roles.lock().unwrap().insert(actor, vec![ADMIN]);
    revoked_power_case
}

async fn assert_reversal_results(
    h: &Harness,
    browser: &mut Browser,
    guild: u64,
    giver: u64,
    target: u64,
    numbers: (i32, i32, i32),
) {
    let (warn, timeout, ban) = numbers;
    use pinchora_core::moderation::moderation::CaseKind;
    let db = &h.state.app.db;
    let path = |n| format!("/api/guilds/{guild}/cases/{n}/revoke");
    let saved = browser
        .write(Method::POST, &path(warn), json!({"reason":"taken back"}))
        .await;
    assert_eq!(saved.status, StatusCode::OK, "{}", saved.text());
    assert_eq!(saved.json()["case"]["kind"], "unwarn");
    assert_eq!(saved.json()["case"]["reason"], "taken back");
    assert!(
        h.mock
            .posts
            .lock()
            .unwrap()
            .iter()
            .any(|(channel, body)| *channel == GENERAL && body.contains("Warn removed"))
    );
    assert!(!db.case(guild, warn).await.unwrap().unwrap().active);
    assert_eq!(
        browser.write(Method::POST, &path(warn), json!({})).await.status,
        StatusCode::CONFLICT
    );
    h.mock.timed_out_members.lock().unwrap().insert(target);
    h.mock.refuse_reversal.store(true, Ordering::SeqCst);
    for number in [timeout, ban] {
        let refused = browser.write(Method::POST, &path(number), json!({})).await;
        assert_eq!(refused.status, StatusCode::BAD_GATEWAY, "{}", refused.text());
        assert_eq!(refused.code(), "discord");
        assert!(db.case(guild, number).await.unwrap().unwrap().active);
    }
    h.mock.refuse_reversal.store(false, Ordering::SeqCst);
    for (number, kind) in [(timeout, "untimeout"), (ban, "unban")] {
        let saved = browser.write(Method::POST, &path(number), json!({})).await;
        assert_eq!(saved.status, StatusCode::OK, "{}", saved.text());
        assert_eq!(saved.json()["case"]["kind"], kind);
        assert!(!db.case(guild, number).await.unwrap().unwrap().active);
    }
    let calls = h.mock.reversal_calls.lock().unwrap().clone();
    assert!(calls.contains(&format!("untimeout:{guild}:{target}")));
    assert!(calls.contains(&format!("unban:{guild}:{target}")));
    let race = db
        .open_case(&pinchora_db::NewCase {
            guild,
            kind: CaseKind::Warn,
            target,
            target_name: "Mia",
            moderator: giver,
            moderator_name: "Giver",
            reason: None,
            duration: None,
            expires_at: None,
            details: None,
        })
        .await
        .unwrap()
        .case
        .number;
    let mut first = browser.clone();
    let mut second = browser.clone();
    let race_path = path(race);
    let (one, two) = tokio::join!(
        first.write(Method::POST, &race_path, json!({})),
        second.write(Method::POST, &race_path, json!({})),
    );
    let mut statuses = [one.status, two.status];
    statuses.sort();
    assert_eq!(statuses, [StatusCode::OK, StatusCode::CONFLICT]);
    let staff = db
        .staff_actions(
            guild,
            &pinchora_db::StaffFilter {
                source: Some("web".into()),
                limit: 20,
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(staff.iter().filter(|row| row.action == "unwarn").count(), 3);
}

async fn assert_delegated_access(h: &Harness, guild: u64, actor: u64, permission_case: i32) {
    let db = &h.state.app.db;
    let path = |n| format!("/api/guilds/{guild}/cases/{n}/revoke");
    let mut outsider = h.browser();
    let visitor = unique();
    outsider
        .login(h, visitor, vec![guild_entry(guild, "Reef", false, 0)])
        .await;
    assert_eq!(
        outsider
            .write(Method::POST, &path(permission_case), json!({}))
            .await
            .status,
        StatusCode::FORBIDDEN
    );
    h.mock.member_roles.lock().unwrap().insert(visitor, vec![ADMIN]);
    db.replace_dashboard_managers(
        guild,
        actor,
        "Actor",
        &[pinchora_db::DashboardManager {
            role_id: ADMIN as i64,
            areas: vec!["community".into()],
        }],
    )
    .await
    .unwrap();
    assert_eq!(
        outsider
            .write(Method::POST, &path(permission_case), json!({}))
            .await
            .status,
        StatusCode::FORBIDDEN
    );
    db.replace_dashboard_managers(
        guild,
        actor,
        "Actor",
        &[pinchora_db::DashboardManager {
            role_id: ADMIN as i64,
            areas: vec!["safety".into()],
        }],
    )
    .await
    .unwrap();
    let delegated = outsider.write(Method::POST, &path(permission_case), json!({})).await;
    assert_eq!(delegated.status, StatusCode::OK, "{}", delegated.text());
}

#[tokio::test]
async fn page_revoke_request_opens_reverse_cases_after_direct_case_import() {
    let Some(h) = Harness::new("page_revoke").await else {
        return;
    };
    let (guild, actor, target) = (unique(), unique(), unique());
    h.add_bot(guild);
    h.mock.member_roles.lock().unwrap().insert(actor, vec![ADMIN]);
    let mut browser = h.browser();
    browser
        .login(&h, actor, vec![guild_entry(guild, "Reef", true, 0)])
        .await;
    for (number, kind) in [(2, "warn"), (4, "ban")] {
        sqlx::query("INSERT INTO mod_cases (guild_id, number, kind, target_id, target_name, moderator_id, moderator_name, active)
            VALUES ($1, $2, $3, $4, 'Mia', 0, 'Pinchora', TRUE)")
            .bind(guild as i64).bind(number).bind(kind).bind(target as i64)
            .execute(h.state.app.db.pool()).await.unwrap();
    }
    for (number, reverse) in [(2, "unwarn"), (4, "unban")] {
        let saved = browser
            .write(
                Method::POST,
                &format!("/api/guilds/{guild}/cases/{number}/revoke"),
                json!({"reason":null}),
            )
            .await;
        assert_eq!(saved.status, StatusCode::OK, "{}", saved.text());
        assert_eq!(saved.json()["case"]["kind"], reverse);
        assert!(!h.state.app.db.case(guild, number).await.unwrap().unwrap().active);
    }
    assert!(
        h.mock
            .reversal_calls
            .lock()
            .unwrap()
            .contains(&format!("unban:{guild}:{target}"))
    );
}

#[tokio::test]
async fn ban_revoke_needs_one_connection_with_a_two_connection_pool() {
    use pinchora_core::moderation::moderation::CaseKind;
    let Some(h) = Harness::with_pool("revoke_pool_two", 2).await else {
        return;
    };
    let (guild, actor, target) = (unique(), unique(), unique());
    h.add_bot(guild);
    h.mock.member_roles.lock().unwrap().insert(actor, vec![ADMIN]);
    let mut browser = h.browser();
    browser
        .login(&h, actor, vec![guild_entry(guild, "Reef", true, 0)])
        .await;
    let open = || pinchora_db::NewCase {
        guild,
        kind: CaseKind::Ban,
        target,
        target_name: "Mia",
        moderator: 0,
        moderator_name: "Pinchora",
        reason: None,
        duration: None,
        expires_at: None,
        details: None,
    };
    let first = h.state.app.db.open_case(&open()).await.unwrap().case.number;
    let path = |n| format!("/api/guilds/{guild}/cases/{n}/revoke");
    let result = tokio::time::timeout(
        Duration::from_secs(3),
        browser.write(Method::POST, &path(first), json!({"reason":null})),
    )
    .await
    .expect("one revoke must not wait for another connection");
    assert_eq!(result.status, StatusCode::OK, "{}", result.text());
    let race = h.state.app.db.open_case(&open()).await.unwrap().case.number;
    let mut second = browser.clone();
    let race_path = path(race);
    let (one, two) = tokio::time::timeout(Duration::from_secs(3), async {
        tokio::join!(
            browser.write(Method::POST, &race_path, json!({"reason":null})),
            second.write(Method::POST, &race_path, json!({"reason":null}))
        )
    })
    .await
    .expect("concurrent revokes must not exhaust the pool");
    let mut statuses = [one.status, two.status];
    statuses.sort();
    assert_eq!(statuses, [StatusCode::OK, StatusCode::CONFLICT]);
}

#[tokio::test]
async fn new_timeout_waits_for_revoke_and_remains_active() {
    use pinchora_core::moderation::moderation::CaseKind;
    let Some(h) = Harness::new("timeout_revoke_race").await else {
        return;
    };
    let (guild, actor, target) = (unique(), unique(), unique());
    h.add_bot(guild);
    h.mock
        .member_roles
        .lock()
        .unwrap()
        .extend([(actor, vec![ADMIN]), (target, vec![REGULAR])]);
    h.mock.reversal_delay_ms.store(200, Ordering::SeqCst);
    h.mock.timed_out_members.lock().unwrap().insert(target);
    let mut browser = h.browser();
    browser
        .login(&h, actor, vec![guild_entry(guild, "Reef", true, 0)])
        .await;
    let first = h
        .state
        .app
        .db
        .open_case(&pinchora_db::NewCase {
            guild,
            kind: CaseKind::Timeout,
            target,
            target_name: "Mia",
            moderator: 0,
            moderator_name: "Pinchora",
            reason: None,
            duration: None,
            expires_at: None,
            details: None,
        })
        .await
        .unwrap()
        .case
        .number;
    let started = h.mock.reversal_started.notified();
    let revoke_path = format!("/api/guilds/{guild}/cases/{first}/revoke");
    let revoke = browser.write(Method::POST, &revoke_path, json!({"reason":null}));
    let next = async {
        started.await;
        let mut tx = h.state.app.db.lock_timeout(guild, target).await.unwrap();
        let opened = pinchora_db::Db::open_reversal_in(
            &mut tx,
            &pinchora_db::NewCase {
                guild,
                kind: CaseKind::Timeout,
                target,
                target_name: "Mia",
                moderator: actor,
                moderator_name: "Actor",
                reason: None,
                duration: None,
                expires_at: None,
                details: None,
            },
            "command",
        )
        .await
        .unwrap();
        tx.commit().await.unwrap();
        opened.case.number
    };
    let (revoked, newest) = tokio::time::timeout(Duration::from_secs(3), async { tokio::join!(revoke, next) })
        .await
        .unwrap();
    assert_eq!(revoked.status, StatusCode::OK, "{}", revoked.text());
    assert!(!h.state.app.db.case(guild, first).await.unwrap().unwrap().active);
    assert!(h.state.app.db.case(guild, newest).await.unwrap().unwrap().active);
}

#[tokio::test]
async fn expired_timeout_cannot_be_revoked_on_dashboard() {
    use pinchora_core::moderation::moderation::CaseKind;
    let Some(h) = Harness::new("expired_timeout_revoke").await else {
        return;
    };
    let (guild, actor, target) = (unique(), unique(), unique());
    h.add_bot(guild);
    h.mock
        .member_roles
        .lock()
        .unwrap()
        .extend([(actor, vec![ADMIN]), (target, vec![REGULAR])]);
    let mut browser = h.browser();
    browser
        .login(&h, actor, vec![guild_entry(guild, "Reef", true, 0)])
        .await;
    let number = h
        .state
        .app
        .db
        .open_case(&pinchora_db::NewCase {
            guild,
            kind: CaseKind::Timeout,
            target,
            target_name: "Mia",
            moderator: 0,
            moderator_name: "Pinchora",
            reason: None,
            duration: None,
            expires_at: None,
            details: None,
        })
        .await
        .unwrap()
        .case
        .number;
    let result = browser
        .write(
            Method::POST,
            &format!("/api/guilds/{guild}/cases/{number}/revoke"),
            json!({}),
        )
        .await;
    assert_eq!(result.status, StatusCode::CONFLICT, "{}", result.text());
    assert!(result.text().contains("That member isn't timed out."));
    assert!(h.state.app.db.case(guild, number).await.unwrap().unwrap().active);
    assert!(h.mock.reversal_calls.lock().unwrap().is_empty());
}

#[tokio::test]
async fn stalled_discord_reversal_times_out_and_releases_ban_lock() {
    use pinchora_core::moderation::moderation::CaseKind;
    let _schema_guard = SCHEMA_CHANGE_GUARD.lock().await;
    let Some(h) = Harness::new("stalled_reversal").await else {
        return;
    };
    let (guild, actor, target) = (unique(), unique(), unique());
    h.add_bot(guild);
    h.mock.member_roles.lock().unwrap().insert(actor, vec![ADMIN]);
    h.mock.unban_delay_ms.store(10_000, Ordering::SeqCst);
    let mut browser = h.browser();
    browser
        .login(&h, actor, vec![guild_entry(guild, "Reef", true, 0)])
        .await;
    let number = h
        .state
        .app
        .db
        .open_case(&pinchora_db::NewCase {
            guild,
            kind: CaseKind::Ban,
            target,
            target_name: "Mia",
            moderator: 0,
            moderator_name: "Pinchora",
            reason: None,
            duration: None,
            expires_at: None,
            details: None,
        })
        .await
        .unwrap()
        .case
        .number;
    let result = tokio::time::timeout(
        Duration::from_secs(10),
        browser.write(
            Method::POST,
            &format!("/api/guilds/{guild}/cases/{number}/revoke"),
            json!({}),
        ),
    )
    .await
    .expect("bounded reversal");
    assert_eq!(result.status, StatusCode::BAD_GATEWAY, "{}", result.text());
    assert!(result.text().contains("Discord did not respond in time."));
    let held = tokio::time::timeout(Duration::from_millis(300), h.state.app.db.lock_ban(guild, target))
        .await
        .expect("released lock")
        .unwrap();
    held.commit().await.unwrap();
    assert!(h.state.app.db.case(guild, number).await.unwrap().unwrap().active);
    assert!(
        !h.state
            .app
            .db
            .cases(guild, &Default::default())
            .await
            .unwrap()
            .iter()
            .any(|c| c.kind == "unban")
    );
}

#[tokio::test]
async fn successful_unban_with_failed_case_write_retries_and_can_finish_later() {
    use pinchora_core::moderation::moderation::CaseKind;
    let _schema_guard = SCHEMA_CHANGE_GUARD.lock().await;
    let Some(h) = Harness::new("reversal_write_failure").await else {
        return;
    };
    let (guild, actor, target) = (unique(), unique(), unique());
    h.add_bot(guild);
    h.mock.member_roles.lock().unwrap().insert(actor, vec![ADMIN]);
    h.mock.track_ban_state.store(true, Ordering::SeqCst);
    let mut browser = h.browser();
    browser
        .login(&h, actor, vec![guild_entry(guild, "Reef", true, 0)])
        .await;
    let db = &h.state.app.db;
    let number = db
        .open_case(&pinchora_db::NewCase {
            guild,
            kind: CaseKind::Ban,
            target,
            target_name: "Mia",
            moderator: 0,
            moderator_name: "Pinchora",
            reason: None,
            duration: None,
            expires_at: None,
            details: None,
        })
        .await
        .unwrap()
        .case
        .number;
    let sequence = format!("revoke_write_seq_{guild}");
    let function = format!("revoke_write_fail_{guild}");
    let trigger = format!("revoke_write_trigger_{guild}");
    sqlx::query(sqlx::AssertSqlSafe(format!("CREATE SEQUENCE {sequence}")))
        .execute(db.pool())
        .await
        .unwrap();
    sqlx::query(sqlx::AssertSqlSafe(format!("CREATE FUNCTION {function}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.guild_id = {guild} AND NEW.kind = 'unban' AND nextval('{sequence}') <= 2 THEN RAISE EXCEPTION 'injected reversal write failure'; END IF; RETURN NEW; END $$")))
        .execute(db.pool()).await.unwrap();
    sqlx::query(sqlx::AssertSqlSafe(format!(
        "CREATE TRIGGER {trigger} BEFORE INSERT ON mod_cases FOR EACH ROW EXECUTE FUNCTION {function}()"
    )))
    .execute(db.pool())
    .await
    .unwrap();
    let path = format!("/api/guilds/{guild}/cases/{number}/revoke");
    let first = browser.write(Method::POST, &path, json!({})).await;
    let attempts: i64 = sqlx::query_scalar(sqlx::AssertSqlSafe(format!("SELECT last_value FROM {sequence}")))
        .fetch_one(db.pool())
        .await
        .unwrap();
    sqlx::query(sqlx::AssertSqlSafe(format!("DROP TRIGGER {trigger} ON mod_cases")))
        .execute(db.pool())
        .await
        .unwrap();
    sqlx::query(sqlx::AssertSqlSafe(format!("DROP FUNCTION {function}()")))
        .execute(db.pool())
        .await
        .unwrap();
    sqlx::query(sqlx::AssertSqlSafe(format!("DROP SEQUENCE {sequence}")))
        .execute(db.pool())
        .await
        .unwrap();
    assert_eq!(attempts, 2, "the write must be retried once under the lock");
    assert_eq!(first.status, StatusCode::INTERNAL_SERVER_ERROR, "{}", first.text());
    assert!(db.case(guild, number).await.unwrap().unwrap().active);
    let finished = browser.write(Method::POST, &path, json!({})).await;
    assert_eq!(finished.status, StatusCode::OK, "{}", finished.text());
    assert_eq!(finished.json()["case"]["kind"], "unban");
    assert!(!db.case(guild, number).await.unwrap().unwrap().active);
}

pub(super) async fn unban_response(mock: &Mock, guild: &str, user: &str) -> (StatusCode, String) {
    mock.reversal_started.notify_one();
    tokio::time::sleep(Duration::from_millis(mock.unban_delay_ms.load(Ordering::SeqCst))).await;
    let target = user.parse::<u64>().unwrap_or(0);
    if mock.track_ban_state.load(Ordering::SeqCst) && mock.absent_bans.lock().unwrap().contains(&target) {
        return (
            StatusCode::NOT_FOUND,
            json!({"message":"Unknown Ban","code":10026}).to_string(),
        );
    }
    if mock.track_ban_state.load(Ordering::SeqCst) && !mock.refuse_reversal.load(Ordering::SeqCst) {
        mock.absent_bans.lock().unwrap().insert(target);
    }
    reversal_response(mock, "unban", guild, user)
}

pub(super) fn member_with_timeout(mock: &Mock, user: u64, roles: &[u64]) -> Value {
    let mut member = member_json(user, roles);
    if mock.timed_out_members.lock().unwrap().contains(&user) {
        member["communication_disabled_until"] = json!("2099-01-01T00:00:00.000000+00:00");
    }
    member
}

pub(super) async fn timeout_reversal_response(mock: &Mock, guild: &str, user: &str) -> (StatusCode, String) {
    mock.reversal_started.notify_one();
    tokio::time::sleep(Duration::from_millis(mock.reversal_delay_ms.load(Ordering::SeqCst))).await;
    let response = reversal_response(mock, "untimeout", guild, user);
    if response.0.is_success() {
        mock.timed_out_members
            .lock()
            .unwrap()
            .remove(&user.parse::<u64>().unwrap_or(0));
    }
    response
}
