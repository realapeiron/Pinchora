use chrono::Utc;
use pinchora_db::{Db, GuildInvite, InviteObservation};

async fn setup() -> (Db, u64, u64, u64) {
    let url = std::env::var("PINCHORA_TEST_DATABASE_URL").unwrap();
    let db = Db::connect_waiting(&url, 4, std::time::Duration::from_secs(5))
        .await
        .unwrap();
    db.migrate().await.unwrap();
    let id = (std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos()
        >> 4) as u64;
    (db, id, id + 1, id + 10)
}
fn invite(owner: u64, uses: i32) -> GuildInvite {
    GuildInvite {
        code: "review".into(),
        inviter_id: Some(owner as i64),
        uses,
        max_uses: 0,
        max_age: 86400,
        created_at: Utc::now(),
    }
}
async fn join(db: &Db, guild: u64, user: u64, owner: u64, uses: i32) -> pinchora_db::JoinCredit {
    db.record_invite_join(
        guild,
        user,
        Utc::now(),
        false,
        InviteObservation {
            invites: &[invite(owner, uses)],
            vanity_changed: false,
            list_available: true,
            preferred_code: None,
        },
    )
    .await
    .unwrap()
}
#[tokio::test]
async fn stale_rest_result_must_not_recredit_consumed_use() {
    let (db, g, o, u) = setup().await;
    db.baseline_invites(g, &[invite(o, 0)]).await.unwrap();
    join(&db, g, u, o, 1).await;
    join(&db, g, u + 1, o, 0).await; // slower pre-join GET returns after newer observation
    let unrelated = join(&db, g, u + 2, o, 1).await;
    assert_eq!(
        unrelated.inviter_id, None,
        "one real use already belongs to the first member"
    );
}
#[tokio::test]
async fn duplicate_member_event_must_not_consume_next_members_credit() {
    let (db, g, o, u) = setup().await;
    db.baseline_invites(g, &[invite(o, 0)]).await.unwrap();
    join(&db, g, u, o, 2).await;
    join(&db, g, u, o, 2).await; // replay of first member event
    let second = join(&db, g, u + 1, o, 2).await;
    assert_eq!(second.inviter_id, Some(o), "second real member must retain second use");
}
#[tokio::test]
async fn unrelated_invite_create_refresh_must_not_swallow_pending_join() {
    let (db, g, o, u) = setup().await;
    db.baseline_invites(g, &[invite(o, 0)]).await.unwrap();
    db.baseline_invites(g, &[invite(o, 1)]).await.unwrap(); // INVITE_CREATE refresh of whole guild
    assert_eq!(join(&db, g, u, o, 1).await.inviter_id, Some(o));
}
#[tokio::test]
async fn delayed_join_must_not_undo_a_processed_leave() {
    let (db, g, o, u) = setup().await;
    db.baseline_invites(g, &[invite(o, 0)]).await.unwrap();
    db.leave_invite(g, u).await.unwrap(); // leave wins scheduling while join waits on REST
    join(&db, g, u, o, 1).await;
    assert_eq!(db.invite_score(g, o).await.unwrap().total, 0, "member has already left");
}

#[tokio::test]
async fn replay_of_an_old_join_cannot_reactivate_after_leave() {
    let (db, g, owner, user) = setup().await;
    db.baseline_invites(g, &[invite(owner, 0)]).await.unwrap();
    let joined = Utc::now() - chrono::Duration::minutes(2);
    db.record_invite_join(
        g,
        user,
        joined,
        false,
        InviteObservation {
            invites: &[invite(owner, 1)],
            vanity_changed: false,
            list_available: true,
            preferred_code: None,
        },
    )
    .await
    .unwrap();
    db.leave_invite(g, user).await.unwrap();
    db.record_invite_join(
        g,
        user,
        joined,
        false,
        InviteObservation {
            invites: &[invite(owner, 1)],
            vanity_changed: false,
            list_available: true,
            preferred_code: None,
        },
    )
    .await
    .unwrap();
    assert_eq!(db.invite_score(g, owner).await.unwrap().total, 0);
}

#[tokio::test]
async fn duplicate_leave_does_not_block_a_real_rejoin() {
    let (db, g, owner, user) = setup().await;
    db.baseline_invites(g, &[invite(owner, 0)]).await.unwrap();
    join(&db, g, user, owner, 1).await;
    db.leave_invite(g, user).await.unwrap();
    db.leave_invite(g, user).await.unwrap();
    db.record_invite_join(
        g,
        user,
        Utc::now() + chrono::Duration::seconds(1),
        false,
        InviteObservation {
            invites: &[invite(owner, 1)],
            vanity_changed: false,
            list_available: true,
            preferred_code: None,
        },
    )
    .await
    .unwrap();
    assert_eq!(db.invite_score(g, owner).await.unwrap().total, 1);
}

#[tokio::test]
async fn ambiguous_codes_do_not_guess_or_pass_credit_to_a_later_join() {
    let (db, g, owner, user) = setup().await;
    let mut other = invite(owner + 1, 0);
    other.code = "other".into();
    db.baseline_invites(g, &[invite(owner, 0), other.clone()])
        .await
        .unwrap();
    other.uses = 1;
    let first = db
        .record_invite_join(
            g,
            user,
            Utc::now(),
            false,
            InviteObservation {
                invites: &[invite(owner, 1), other.clone()],
                vanity_changed: false,
                list_available: true,
                preferred_code: Some("review"),
            },
        )
        .await
        .unwrap();
    assert_eq!(first.inviter_id, None);
    let second = db
        .record_invite_join(
            g,
            user + 1,
            Utc::now(),
            false,
            InviteObservation {
                invites: &[invite(owner, 1), other],
                vanity_changed: false,
                list_available: true,
                preferred_code: None,
            },
        )
        .await
        .unwrap();
    assert_eq!(second.inviter_id, None);
}

#[tokio::test]
async fn connection_epoch_reset_discards_offline_uses() {
    let (db, g, owner, user) = setup().await;
    db.baseline_invites(g, &[invite(owner, 0)]).await.unwrap();
    db.reset_invite_baseline(g, &[invite(owner, 3)]).await.unwrap();
    assert_eq!(join(&db, g, user, owner, 3).await.inviter_id, None);
    assert_eq!(join(&db, g, user + 1, owner, 4).await.inviter_id, Some(owner));
}

#[tokio::test]
async fn reenabling_claims_the_baseline_once() {
    let (db, g, owner, user) = setup().await;
    db.baseline_invites(g, &[invite(owner, 0)]).await.unwrap();
    let mut settings = db.invite_settings(g).await.unwrap();
    settings.enabled = false;
    db.save_invite_settings(g, &settings).await.unwrap();
    settings.enabled = true;
    db.save_invite_settings(g, &settings).await.unwrap();
    assert!(db.reset_required_invite_baseline(g, &[invite(owner, 3)]).await.unwrap());
    assert!(!db.reset_required_invite_baseline(g, &[invite(owner, 3)]).await.unwrap());
    assert_eq!(join(&db, g, user, owner, 3).await.inviter_id, None);
    assert_eq!(join(&db, g, user + 1, owner, 4).await.inviter_id, Some(owner));
}

#[tokio::test]
async fn reward_retains_the_manager_who_authorized_it() {
    let (db, guild, manager, _) = setup().await;
    db.put_invite_reward_by(guild, 2, manager + 1, Some(manager))
        .await
        .unwrap();
    assert_eq!(
        db.invite_reward_authority(guild, manager + 1).await.unwrap(),
        vec![Some(manager)]
    );
    db.mark_invite_role(guild, manager, manager + 1, true).await.unwrap();
    db.delete_user_data(manager, Utc::now()).await.unwrap();
    assert_eq!(
        db.invite_reward_authority(guild, manager + 1).await.unwrap(),
        vec![None]
    );
    assert!(db.invite_granted_roles(guild, manager).await.unwrap().is_empty());
}
