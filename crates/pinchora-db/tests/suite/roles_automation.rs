//! Runs against a real PostgreSQL when `PINCHORA_TEST_DATABASE_URL` is set; skipped otherwise.
//!
//! The roles pack's tables: panels with a style and rules, reactions, auto roles, sticky roles, roles on
//! a timer and colour roles.

use chrono::{Duration, Utc};
use pinchora_db::{ColorAdded, ColorSettings, NewAutoRole, PanelDoc, PanelEntry, PanelPatch, PanelRoleDoc, PanelSaved};

// Pruning visits every guild; own the sweep before creating old rows.

use crate::support::db;

/// Ids unique per run, so the suite can run repeatedly against one database.
use crate::support::ids;

/// Global oldest-first queues need an empty baseline even after interrupted runs.
/// Call only while holding the exclusive suite lock, before creating any fixtures.
async fn reset_timer_queues(db: &pinchora_db::Db) {
    sqlx::raw_sql("DELETE FROM timed_roles; DELETE FROM delayed_roles")
        .execute(db.pool())
        .await
        .unwrap();
}

#[tokio::test]
async fn panels_keep_their_style_and_rules() {
    let _isolation = crate::support::shared().await;
    let Some(db) = db(5).await else { return };
    let [guild, other] = ids();

    // A panel made the old way is plain buttons with no rules.
    let (plain, _) = db.save_panel(guild, "Plain", None).await.unwrap();
    assert_eq!(plain.style, "buttons");
    assert!(!plain.exclusive && plain.required_role.is_none() && plain.min_picks == 0 && plain.max_picks == 0);

    // The same title changes only what was given.
    let patch = PanelPatch {
        style: Some("select".to_owned()),
        exclusive: Some(true),
        required_role: Some(Some(77)),
        min_picks: Some(1),
        max_picks: None,
    };
    let (menu, created) = db.save_panel_with(guild, "plain", Some("Hi"), &patch).await.unwrap();
    assert!(!created && menu.id == plain.id);
    assert_eq!(menu.description.as_deref(), Some("Hi"));
    assert_eq!(
        (menu.style.as_str(), menu.exclusive, menu.required_role, menu.min_picks),
        ("select", true, Some(77), 1)
    );
    let kept = PanelPatch {
        style: Some("reactions".to_owned()),
        ..Default::default()
    };
    let (still, _) = db.save_panel_with(guild, "Plain", None, &kept).await.unwrap();
    assert_eq!(
        (still.style.as_str(), still.exclusive, still.required_role),
        ("reactions", true, Some(77)),
        "nothing given, nothing lost"
    );
    let cleared = PanelPatch {
        required_role: Some(None),
        ..Default::default()
    };
    let (open, _) = db.save_panel_with(guild, "Plain", None, &cleared).await.unwrap();
    assert_eq!(open.required_role, None, "Some(None) takes the requirement away");

    // Found by number only inside its own server.
    assert_eq!(db.panel_by_id(guild, plain.id).await.unwrap().unwrap().title, "Plain");
    assert!(db.panel_by_id(other, plain.id).await.unwrap().is_none());

    // Entries carry a description, a custom emoji id and an expiry.
    let entry = PanelEntry {
        label: "Gamer".to_owned(),
        emoji: Some("<:blob:555>".to_owned()),
        emoji_id: Some(555),
        description: Some("Plays games".to_owned()),
    };
    assert!(db.add_panel_entry(plain.id, 301, &entry).await.unwrap());
    let renamed = PanelEntry {
        label: "Gamers".to_owned(),
        emoji: Some("<:blob:555>".to_owned()),
        emoji_id: Some(555),
        description: None,
    };
    assert!(db.add_panel_entry(plain.id, 301, &renamed).await.unwrap());
    let row = db.panel_role(guild, plain.id, 301).await.unwrap().unwrap();
    assert_eq!(
        (row.label.as_str(), row.description.as_deref(), row.emoji_id),
        ("Gamers", Some("Plays games"), Some(555)),
        "no new description, the old one stays"
    );
    assert!(db.add_panel_role(plain.id, 302, "Other", None).await.unwrap());
    assert_eq!(
        db.set_panel_role_expiry(guild, 301, Some(plain.id), Some(3_600))
            .await
            .unwrap(),
        1
    );
    assert_eq!(
        db.set_panel_role_expiry(other, 301, None, Some(60)).await.unwrap(),
        0,
        "another server's panels are not touched"
    );
    assert_eq!(
        db.panel_role(guild, plain.id, 301).await.unwrap().unwrap().temp_secs,
        Some(3_600)
    );
    assert_eq!(
        db.set_panel_role_expiry(guild, 301, None, None).await.unwrap(),
        1,
        "no panel means every panel that has the role"
    );
    assert_eq!(
        db.panel_role(guild, plain.id, 301).await.unwrap().unwrap().temp_secs,
        None
    );
}

#[tokio::test]
async fn a_reactions_panel_is_found_by_its_message() {
    let _isolation = crate::support::shared().await;
    let Some(db) = db(5).await else { return };
    let [guild, other, message] = ids();
    let (panel, _) = db.save_panel(guild, "Pings", None).await.unwrap();
    assert!(db.panel_by_message(guild, message).await.unwrap().is_none());
    db.set_panel_message(panel.id, 9, message).await.unwrap();
    assert_eq!(db.panel_by_message(guild, message).await.unwrap().unwrap().id, panel.id);
    assert!(db.panel_by_message(other, message).await.unwrap().is_none());

    db.replace_panel_reactions(panel.id, &[("u:🔔".to_owned(), 5), ("c:42".to_owned(), 6)])
        .await
        .unwrap();
    assert_eq!(
        db.panel_reactions(panel.id).await.unwrap(),
        vec![("c:42".to_owned(), 6), ("u:🔔".to_owned(), 5)]
    );
    db.replace_panel_reactions(panel.id, &[("u:🔔".to_owned(), 5)])
        .await
        .unwrap();
    assert_eq!(db.panel_reactions(panel.id).await.unwrap().len(), 1);

    // A deleted role leaves nothing behind in the pack's tables.
    db.forget_pack_role(guild, 5).await.unwrap();
    assert!(db.panel_reactions(panel.id).await.unwrap().is_empty());
    // The panel's reactions go with the panel.
    db.replace_panel_reactions(panel.id, &[("u:🔔".to_owned(), 5)])
        .await
        .unwrap();
    let gone = db.delete_panel(guild, panel.id).await.unwrap();
    assert!(gone.is_some() && db.delete_panel(guild, panel.id).await.unwrap().is_none());
    assert!(db.panel_reactions(panel.id).await.unwrap().is_empty());
}

#[tokio::test]
async fn a_whole_panel_is_saved_in_one_go() {
    let _isolation = crate::support::shared().await;
    let Some(db) = db(5).await else { return };
    let [guild] = ids();
    let entry = |role: u64, label: &str, secs: Option<i32>| PanelRoleDoc {
        role_id: role,
        entry: PanelEntry {
            label: label.to_owned(),
            ..Default::default()
        },
        temp_secs: secs,
    };
    let doc = PanelDoc {
        title: "Teams".to_owned(),
        description: Some("Pick".to_owned()),
        style: "select".to_owned(),
        exclusive: true,
        required_role: Some(9),
        min_picks: 1,
        max_picks: 0,
        roles: vec![entry(1, "Red", None), entry(2, "Blue", Some(600))],
    };
    let PanelSaved::Saved(made) = db.create_panel_doc(guild, &doc).await.unwrap() else {
        panic!("a new title is saved");
    };
    assert!(matches!(
        db.create_panel_doc(guild, &doc).await.unwrap(),
        PanelSaved::TitleTaken
    ));
    let roles = db.panel_roles(made.id).await.unwrap();
    assert_eq!(
        roles
            .iter()
            .map(|r| (r.role_id, r.label.as_str(), r.temp_secs))
            .collect::<Vec<_>>(),
        [(1, "Red", None), (2, "Blue", Some(600))]
    );

    // Replacing swaps the entries and settings, and keeps the posted message.
    db.set_panel_message(made.id, 3, 4).await.unwrap();
    let again = PanelDoc {
        title: "Teams v2".to_owned(),
        style: "buttons".to_owned(),
        roles: vec![entry(3, "Green", None)],
        ..doc.clone()
    };
    let PanelSaved::Saved(saved) = db.replace_panel_doc(guild, made.id, &again).await.unwrap() else {
        panic!("the panel is there");
    };
    assert_eq!(
        (saved.title.as_str(), saved.style.as_str(), saved.message_id),
        ("Teams v2", "buttons", Some(4))
    );
    assert_eq!(db.panel_roles(made.id).await.unwrap().len(), 1);
    assert!(matches!(
        db.replace_panel_doc(guild, made.id + 1_000_000, &again).await.unwrap(),
        PanelSaved::Missing
    ));
    // A title another panel has is refused, and the panel stays as it was.
    let (_, _) = db.save_panel(guild, "Taken", None).await.unwrap();
    let clash = PanelDoc {
        title: "taken".to_owned(),
        ..again
    };
    assert!(matches!(
        db.replace_panel_doc(guild, made.id, &clash).await.unwrap(),
        PanelSaved::TitleTaken
    ));
    assert_eq!(db.panel_by_id(guild, made.id).await.unwrap().unwrap().title, "Teams v2");
    assert_eq!(
        db.panel_roles(made.id).await.unwrap().len(),
        1,
        "the failed save kept the entries"
    );
}

#[tokio::test]
async fn auto_roles_are_limited_per_server_and_updated_in_place() {
    let _isolation = crate::support::shared().await;
    let Some(db) = db(5).await else { return };
    let [guild, role] = ids();
    let join = |role_id: u64, delay_secs: i32| NewAutoRole {
        role_id,
        kind: "join",
        delay_secs,
        remove_after_secs: 0,
        enabled: true,
    };
    assert!(db.save_auto_role(guild, &join(role, 0), 3).await.unwrap());
    assert!(
        db.save_auto_role(guild, &join(role, 600), 3).await.unwrap(),
        "same role and kind updates"
    );
    let bot = NewAutoRole {
        kind: "bot",
        ..join(role, 0)
    };
    assert!(
        db.save_auto_role(guild, &bot, 3).await.unwrap(),
        "another kind is another row"
    );
    assert!(db.save_auto_role(guild, &join(role + 1, 0), 3).await.unwrap());
    assert!(
        !db.save_auto_role(guild, &join(role + 2, 0), 3).await.unwrap(),
        "a fourth is past the limit of 3"
    );
    assert!(
        db.save_auto_role(guild, &join(role + 1, 5), 3).await.unwrap(),
        "changing a row at the limit is fine"
    );
    let rows = db.auto_roles(guild).await.unwrap();
    assert_eq!(rows.len(), 3);
    let waiting = rows
        .iter()
        .find(|r| r.role_id as u64 == role && r.kind == "join")
        .unwrap();
    assert_eq!(waiting.delay_secs, 600);
    assert_eq!(db.auto_roles_of_kind(guild, "bot").await.unwrap().len(), 1);
    assert_eq!(db.remove_auto_role(guild, role, Some("bot")).await.unwrap(), 1);
    assert_eq!(
        db.remove_auto_role(guild, role, None).await.unwrap(),
        1,
        "every kind left"
    );
    assert_eq!(db.auto_roles(guild).await.unwrap().len(), 1);
}

#[tokio::test]
async fn sticky_roles_are_kept_for_those_who_left() {
    let _isolation = crate::support::exclusive().await;

    let Some(db) = db(5).await else { return };
    let [guild, user, other] = ids();
    assert!(!db.sticky_enabled(guild).await.unwrap());
    db.set_sticky(guild, true).await.unwrap();
    assert!(db.sticky_enabled(guild).await.unwrap());

    db.remember_roles(guild, user, &[10, 20]).await.unwrap();
    db.remember_roles(guild, other, &[30]).await.unwrap();
    // Still here: nothing to give back.
    assert!(!db.sticky_waiting(guild, user).await.unwrap());
    assert!(db.take_sticky(guild, user).await.unwrap().is_none());
    let left = Utc::now() - Duration::days(1);
    db.note_left(guild, user, left).await.unwrap();
    assert!(db.sticky_waiting(guild, user).await.unwrap());
    let row = db.take_sticky(guild, user).await.unwrap().unwrap();
    assert_eq!(row.role_ids, [10, 20]);
    assert!(row.left_at.is_some());
    assert!(db.take_sticky(guild, user).await.unwrap().is_none(), "taken once");

    // Old rows go; a member with no roles keeps no row.
    db.note_left(guild, other, Utc::now() - Duration::days(90))
        .await
        .unwrap();
    assert!(db.prune_sticky(Utc::now() - Duration::days(60)).await.unwrap() >= 1);
    assert!(db.take_sticky(guild, other).await.unwrap().is_none());
    db.remember_roles(guild, user, &[1]).await.unwrap();
    db.remember_roles(guild, user, &[]).await.unwrap();
    db.note_left(guild, user, left).await.unwrap();
    assert!(
        db.take_sticky(guild, user)
            .await
            .unwrap()
            .is_none_or(|row| row.role_ids.is_empty())
    );

    // Switching it off forgets what was kept.
    db.remember_roles(guild, user, &[5]).await.unwrap();
    db.set_sticky(guild, false).await.unwrap();
    db.note_left(guild, user, left).await.unwrap();
    assert!(db.take_sticky(guild, user).await.unwrap().is_none());
}

#[tokio::test]
async fn roles_on_a_timer_are_claimed_once() {
    let _isolation = crate::support::exclusive().await;
    let Some(db) = db(5).await else { return };
    reset_timer_queues(&db).await;
    let [guild, user, role] = ids();
    let now = Utc::now();
    db.schedule_removal(guild, user, role, now - Duration::minutes(1))
        .await
        .unwrap();
    db.schedule_removal(guild, user, role + 1, now + Duration::hours(1))
        .await
        .unwrap();
    assert_eq!(db.lapsed_roles(guild, user, now).await.unwrap(), [role as i64]);

    let claimed = db.claim_due_removals(now, 50).await.unwrap();
    let mine: Vec<_> = claimed.iter().filter(|r| r.guild_id as u64 == guild).collect();
    assert_eq!(mine.len(), 1);
    assert_eq!((mine[0].user_id as u64, mine[0].role_id as u64), (user, role));
    assert!(
        db.claim_due_removals(now, 50)
            .await
            .unwrap()
            .iter()
            .all(|r| r.guild_id as u64 != guild),
        "a claimed one is leased to the pod that took it"
    );
    db.finish_removal(mine[0]).await.unwrap();
    assert!(db.lapsed_roles(guild, user, now).await.unwrap().is_empty());
    // Moving the time of a role that is still held, and cancelling it.
    db.schedule_removal(guild, user, role + 1, now - Duration::seconds(5))
        .await
        .unwrap();
    assert_eq!(db.lapsed_roles(guild, user, now).await.unwrap(), [role as i64 + 1]);
    db.cancel_removal(guild, user, role + 1).await.unwrap();
    assert!(db.lapsed_roles(guild, user, now).await.unwrap().is_empty());

    // Auto roles that wait for a delay.
    db.schedule_grant(guild, user, role, now - Duration::seconds(1), 3_600)
        .await
        .unwrap();
    db.schedule_grant(guild, user, role + 2, now + Duration::hours(1), 0)
        .await
        .unwrap();
    let due = db.claim_due_grants(now, 50).await.unwrap();
    let mine: Vec<_> = due.iter().filter(|r| r.guild_id as u64 == guild).collect();
    assert_eq!(mine.len(), 1);
    assert_eq!((mine[0].role_id as u64, mine[0].remove_after_secs), (role, 3_600));
    db.finish_grant(mine[0].id).await.unwrap();
    // A member who left takes the roles waiting for them along.
    db.cancel_grants(guild, user).await.unwrap();
    assert!(
        db.claim_due_grants(now + Duration::days(2), 50)
            .await
            .unwrap()
            .iter()
            .all(|r| r.guild_id as u64 != guild)
    );
}

#[tokio::test]
async fn renewed_timer_survives_old_worker_completion() {
    let _isolation = crate::support::exclusive().await;
    let Some(db) = db(5).await else { return };
    reset_timer_queues(&db).await;
    let [guild, user, role] = ids();
    let now = Utc::now();
    db.schedule_removal(guild, user, role, now - Duration::minutes(1))
        .await
        .unwrap();
    let old = db
        .claim_due_removals(now, 50)
        .await
        .unwrap()
        .into_iter()
        .find(|row| row.guild_id as u64 == guild)
        .unwrap();
    db.schedule_removal(guild, user, role, now + Duration::hours(1))
        .await
        .unwrap();
    db.finish_removal(&old).await.unwrap();
    let remaining: (i64,) =
        sqlx::query_as("SELECT count(*) FROM timed_roles WHERE guild_id = $1 AND user_id = $2 AND role_id = $3")
            .bind(guild as i64)
            .bind(user as i64)
            .bind(role as i64)
            .fetch_one(db.pool())
            .await
            .unwrap();
    assert_eq!(remaining.0, 1, "old completion erased the renewed timer");
    db.cancel_removal(guild, user, role).await.unwrap();
}

#[tokio::test]
async fn empty_update_after_leave_revokes_sticky_role() {
    let _isolation = crate::support::shared().await;
    let Some(db) = db(5).await else { return };
    let [guild, user, role] = ids();
    db.remember_roles(guild, user, &[role]).await.unwrap();
    db.note_left(guild, user, Utc::now()).await.unwrap();
    db.remember_roles(guild, user, &[]).await.unwrap();
    let kept = db.take_sticky(guild, user).await.unwrap();
    assert!(
        kept.is_none_or(|row| row.role_ids.is_empty()),
        "revoked role survived a reordered leave/update"
    );
}

#[tokio::test]
async fn stale_snapshot_cannot_replace_gateway_tombstone() {
    let _isolation = crate::support::shared().await;
    let Some(db) = db(5).await else { return };
    let [guild, user, role] = ids();
    db.remember_roles(guild, user, &[role]).await.unwrap();
    let versions = db.sticky_snapshot_versions(guild).await.unwrap();
    db.remember_roles(guild, user, &[]).await.unwrap();
    db.remember_snapshot(guild, user, &[role], versions.get(&user).map(String::as_str))
        .await
        .unwrap();
    db.note_left(guild, user, Utc::now()).await.unwrap();
    assert!(!db.sticky_waiting(guild, user).await.unwrap());
}

#[tokio::test]
async fn snapshot_refreshes_roles_revoked_while_bot_was_offline() {
    let _isolation = crate::support::shared().await;
    let Some(db) = db(5).await else { return };
    let [guild, user, role] = ids();
    db.remember_roles(guild, user, &[role]).await.unwrap();
    let versions = db.sticky_snapshot_versions(guild).await.unwrap();
    db.remember_snapshot(guild, user, &[], versions.get(&user).map(String::as_str))
        .await
        .unwrap();
    db.note_left(guild, user, Utc::now()).await.unwrap();
    assert!(!db.sticky_waiting(guild, user).await.unwrap());
}

#[tokio::test]
async fn expiry_removes_role_from_sticky_history() {
    let _isolation = crate::support::exclusive().await;
    let Some(db) = db(5).await else { return };
    reset_timer_queues(&db).await;
    let [guild, user, role] = ids();
    db.remember_roles(guild, user, &[role]).await.unwrap();
    db.note_left(guild, user, Utc::now()).await.unwrap();
    let now = Utc::now();
    db.schedule_removal(guild, user, role, now - Duration::minutes(1))
        .await
        .unwrap();
    let claimed = db
        .claim_due_removals(now, 50)
        .await
        .unwrap()
        .into_iter()
        .find(|row| row.guild_id as u64 == guild)
        .unwrap();
    assert!(db.finish_removal(&claimed).await.unwrap());
    assert!(!db.sticky_waiting(guild, user).await.unwrap());
}

#[tokio::test]
async fn cleanup_waits_for_privacy_user_lock() {
    let _isolation = crate::support::shared().await;
    let Some(db) = db(5).await else { return };
    let [guild, user, role] = ids();
    db.schedule_removal(guild, user, role, Utc::now()).await.unwrap();
    db.remember_roles(guild, user, &[role]).await.unwrap();
    let mut privacy = db.pool().begin().await.unwrap();
    sqlx::query("SELECT lock_user($1)")
        .bind(user as i64)
        .execute(&mut *privacy)
        .await
        .unwrap();
    let worker_db = db.clone();
    let mut worker = tokio::spawn(async move { worker_db.forget_pack_role(guild, role).await });
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(100), &mut worker)
            .await
            .is_err()
    );
    privacy.rollback().await.unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(5), worker)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
}

#[tokio::test]
async fn role_member_lock_serializes_concurrent_changes() {
    let _isolation = crate::support::shared().await;
    let Some(db) = db(5).await else { return };
    let [guild, user] = ids();
    let first = db.lock_role_member(guild, user).await.unwrap();
    let second_db = db.clone();
    let mut second = tokio::spawn(async move { second_db.lock_role_member(guild, user).await });
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(100), &mut second)
            .await
            .is_err()
    );
    drop(first);
    drop(
        tokio::time::timeout(std::time::Duration::from_secs(5), second)
            .await
            .unwrap()
            .unwrap()
            .unwrap(),
    );
}

#[tokio::test]
async fn concurrent_role_locks_leave_a_connection_for_timers() {
    let _isolation = crate::support::shared().await;
    let Some(db) = db(5).await else { return };
    let [guild, first_user, role] = ids();
    let tasks: Vec<_> = (0..5)
        .map(|offset| {
            let db = db.clone();
            tokio::spawn(async move {
                let user = first_user + offset;
                let _lock = db.lock_role_member(guild, user).await.unwrap();
                db.schedule_removal(guild, user, role, Utc::now()).await.unwrap();
            })
        })
        .collect();
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        for task in tasks {
            task.await.unwrap();
        }
    })
    .await
    .expect("role locks exhausted every database connection");
    for offset in 0..5 {
        db.cancel_removal(guild, first_user + offset, role).await.unwrap();
    }
}

#[tokio::test]
async fn sticky_prune_skips_a_user_being_deleted() {
    let _isolation = crate::support::exclusive().await;

    let Some(db) = db(5).await else { return };
    let [guild, user, role] = ids();
    db.remember_roles(guild, user, &[role]).await.unwrap();
    let before = Utc::now() - Duration::days(61);
    let mut privacy = db.pool().begin().await.unwrap();
    sqlx::query("SELECT lock_user($1)")
        .bind(user as i64)
        .execute(&mut *privacy)
        .await
        .unwrap();
    db.note_left(guild, user, before - Duration::days(1)).await.unwrap();
    assert!(db.sticky_waiting(guild, user).await.unwrap());
    assert_eq!(
        tokio::time::timeout(std::time::Duration::from_secs(2), db.prune_sticky(before))
            .await
            .unwrap()
            .unwrap(),
        0
    );
    privacy.rollback().await.unwrap();
    db.prune_sticky(before).await.unwrap();
    assert!(!db.sticky_waiting(guild, user).await.unwrap());
}

#[tokio::test]
async fn colour_roles_are_one_per_colour_and_limited() {
    let _isolation = crate::support::shared().await;
    let Some(db) = db(5).await else { return };
    let [guild, role] = ids();
    assert_eq!(db.color_settings(guild).await.unwrap(), ColorSettings::default());
    let settings = ColorSettings {
        enabled: true,
        allowed_roles: vec![1, 2],
        mode: "hex".to_owned(),
        anchor_role_id: Some(7),
    };
    db.save_color_settings(guild, &settings).await.unwrap();
    assert_eq!(db.color_settings(guild).await.unwrap(), settings);

    assert_eq!(
        db.add_color_role(guild, role, "#ff6b4a", "Coral", 2).await.unwrap(),
        ColorAdded::Added
    );
    assert_eq!(
        db.add_color_role(guild, role, "#ff6b4a", "Coral!", 2).await.unwrap(),
        ColorAdded::Updated
    );
    assert_eq!(
        db.add_color_role(guild, role + 1, "#ff6b4a", "Twin", 2).await.unwrap(),
        ColorAdded::HexTaken,
        "one role per colour"
    );
    assert_eq!(
        db.add_color_role(guild, role + 1, "#06d6a0", "Mint", 2).await.unwrap(),
        ColorAdded::Added
    );
    assert_eq!(
        db.add_color_role(guild, role + 2, "#8fd3ff", "Sky", 2).await.unwrap(),
        ColorAdded::Full
    );
    let rows = db.color_roles(guild).await.unwrap();
    assert_eq!(
        rows.iter().map(|r| r.name.as_str()).collect::<Vec<_>>(),
        ["Coral!", "Mint"]
    );

    // A deleted role drops out of the colours, the allowed list and the anchor.
    db.forget_pack_role(guild, role).await.unwrap();
    assert_eq!(db.color_roles(guild).await.unwrap().len(), 1);
    db.forget_pack_role(guild, 7).await.unwrap();
    db.forget_pack_role(guild, 1).await.unwrap();
    let after = db.color_settings(guild).await.unwrap();
    assert_eq!((after.allowed_roles, after.anchor_role_id), (vec![2], None));
    assert!(db.remove_color_role(guild, role + 1).await.unwrap());
    assert!(!db.remove_color_role(guild, role + 1).await.unwrap());
}
