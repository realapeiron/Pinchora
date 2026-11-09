//! Atomic star claims, unstars, cascades and vote settings on the dedicated test database.

use pinchora_db::{Starboard, VoteChannel};

// Cleanup claims consume a global queue, so these fixtures must not claim each other's jobs.

use crate::support::db;

/// Old cleanup jobs otherwise fill the worker's bounded batch on repeated runs.
/// The caller owns the exclusive suite lock before opening this fixture pool.
async fn cleanup_db() -> Option<pinchora_db::Db> {
    let db = db(5).await?;
    sqlx::raw_sql("DELETE FROM starboard_posts; DELETE FROM starboard_copy_cleanup")
        .execute(db.pool())
        .await
        .unwrap();
    Some(db)
}

fn ids() -> [u64; 7] {
    let base = crate::support::id();
    std::array::from_fn(|i| base + i as u64)
}
fn board(guild: u64, channel: u64) -> Starboard {
    Starboard {
        id: 0,
        guild_id: guild as i64,
        name: "Test".into(),
        channel_id: channel as i64,
        emoji: "⭐".into(),
        threshold: 3,
        self_star: false,
        allow_bots: false,
        only_channels: vec![],
        except_channels: vec![],
        require_image: false,
        color: 0xffc857,
        role_weights: serde_json::json!({}),
        remove_below: true,
        enabled: true,
    }
}

#[tokio::test]
async fn concurrent_stars_claim_one_post_and_weighted_unstar_recounts() {
    let _isolation = crate::support::shared().await;
    let Some(db) = db(5).await else { return };
    let [guild, target, source, message, author, user_a, user_b] = ids();
    let board = db.save_starboard(&board(guild, target)).await.unwrap();
    let (a, b) = tokio::join!(
        db.star_change(board.id, message, source, author, user_a, Some(2)),
        db.star_change(board.id, message, source, author, user_b, Some(1)),
    );
    assert_eq!(a.unwrap().star_count.max(b.unwrap().star_count), 3);
    let (first, second) = tokio::join!(
        db.claim_star_post(board.id, message, 3),
        db.claim_star_post(board.id, message, 3),
    );
    let (first, second) = (first.unwrap(), second.unwrap());
    assert_ne!(first.is_some(), second.is_some());
    let token = first.or(second).unwrap();
    assert!(
        db.finish_star_post(board.id, message, token, Some(message + 1))
            .await
            .unwrap()
    );
    let same = db
        .star_change(board.id, message, source, author, user_a, Some(2))
        .await
        .unwrap();
    assert_eq!(same.star_count, 3);
    let fewer = db
        .star_change(board.id, message, source, author, user_b, None)
        .await
        .unwrap();
    assert_eq!(fewer.star_count, 2);
    let gone = db.delete_star_source(guild, message).await.unwrap();
    assert_eq!(gone.len(), 1);
    assert!(db.star_posts_for_source(guild, message).await.unwrap().is_empty());
    assert!(db.claim_star_post(board.id, message, 3).await.unwrap().is_none());
}

#[tokio::test]
async fn multiple_boards_and_vote_channels_are_independent() {
    let _isolation = crate::support::shared().await;
    let Some(db) = db(5).await else { return };
    let [guild, target, second, message, author, user, source] = ids();
    let a = db.save_starboard(&board(guild, target)).await.unwrap();
    let b = db.save_starboard(&board(guild, second)).await.unwrap();
    db.star_change(a.id, message, source, author, user, Some(1))
        .await
        .unwrap();
    db.star_change(b.id, message, source, author, user, Some(2))
        .await
        .unwrap();
    assert_eq!(db.top_star_posts(a.id, 0).await.unwrap()[0].star_count, 1);
    assert_eq!(db.top_star_posts(b.id, 0).await.unwrap()[0].star_count, 2);
    db.save_vote(&VoteChannel {
        channel_id: source as i64,
        guild_id: guild as i64,
        up_emoji: "▲".into(),
        down_emoji: "▼".into(),
        only_media: true,
        create_thread: false,
        enabled: true,
    })
    .await
    .unwrap();
    assert_eq!(db.vote(source).await.unwrap().unwrap().up_emoji, "▲");
    assert!(db.delete_vote(guild, source).await.unwrap());
    assert!(db.vote(source).await.unwrap().is_none());
}

#[tokio::test]
async fn fourth_board_is_refused_even_from_parallel_writers() {
    let _isolation = crate::support::shared().await;
    let Some(db) = db(5).await else { return };
    let [guild, first, second, third, fourth, _, _] = ids();
    let boards = [
        board(guild, first),
        board(guild, second),
        board(guild, third),
        board(guild, fourth),
    ];
    let (a, b, c, d) = tokio::join!(
        db.save_starboard(&boards[0]),
        db.save_starboard(&boards[1]),
        db.save_starboard(&boards[2]),
        db.save_starboard(&boards[3]),
    );
    assert_eq!(
        [a.is_ok(), b.is_ok(), c.is_ok(), d.is_ok()]
            .into_iter()
            .filter(|v| *v)
            .count(),
        3
    );
    assert_eq!(db.starboards(guild).await.unwrap().len(), 3);
}

#[tokio::test]
async fn deleting_source_during_send_fences_completion_and_queues_losing_copy() {
    let _isolation = crate::support::exclusive().await;

    let Some(db) = cleanup_db().await else { return };
    let [guild, target, source, message, author, voter, copy] = ids();
    let board = db.save_starboard(&board(guild, target)).await.unwrap();
    db.star_change(board.id, message, source, author, voter, Some(3))
        .await
        .unwrap();
    let token = db.claim_star_post(board.id, message, 3).await.unwrap().unwrap();
    db.delete_star_source(guild, message).await.unwrap();
    assert!(!db.finish_star_post(board.id, message, token, Some(copy)).await.unwrap());
    db.queue_star_cleanup(target, copy).await.unwrap();
    assert!(
        db.due_star_cleanups()
            .await
            .unwrap()
            .contains(&(target as i64, copy as i64))
    );
    assert!(db.touch_star_post(board.id, message, source, author).await.is_ok());
    assert!(db.star_post(board.id, message).await.unwrap().is_none());
}

#[tokio::test]
async fn expired_claim_fences_old_sender_and_preserves_losing_copy_cleanup() {
    let _isolation = crate::support::exclusive().await;

    let Some(db) = cleanup_db().await else { return };
    let [guild, target, source, message, author, voter, copy] = ids();
    let board = db.save_starboard(&board(guild, target)).await.unwrap();
    db.star_change(board.id, message, source, author, voter, Some(3))
        .await
        .unwrap();
    let old = db.claim_star_post(board.id, message, 3).await.unwrap().unwrap();
    sqlx::query("UPDATE starboard_posts SET updated_at=now()-interval '3 minutes' WHERE starboard_id=$1")
        .bind(board.id)
        .execute(db.pool())
        .await
        .unwrap();
    let current = db.claim_star_post(board.id, message, 3).await.unwrap().unwrap();
    assert_ne!(old, current);
    assert!(!db.finish_star_post(board.id, message, old, Some(copy)).await.unwrap());
    db.queue_star_cleanup(target, copy).await.unwrap();
    assert!(
        db.finish_star_post(board.id, message, current, Some(copy + 1))
            .await
            .unwrap()
    );
    assert!(
        db.due_star_cleanups()
            .await
            .unwrap()
            .contains(&(target as i64, copy as i64))
    );
    assert_eq!(
        db.star_post(board.id, message).await.unwrap().unwrap().board_message_id,
        Some((copy + 1) as i64)
    );
}

#[tokio::test]
async fn board_removal_and_retention_keep_copy_cleanup_jobs() {
    let _isolation = crate::support::exclusive().await;

    let Some(db) = cleanup_db().await else { return };
    let [guild, target, source, message, author, voter, copy] = ids();
    let board = db.save_starboard(&board(guild, target)).await.unwrap();
    db.star_change(board.id, message, source, author, voter, Some(3))
        .await
        .unwrap();
    let token = db.claim_star_post(board.id, message, 3).await.unwrap().unwrap();
    assert!(db.finish_star_post(board.id, message, token, Some(copy)).await.unwrap());
    sqlx::query("UPDATE starboard_posts SET created_at=now()-interval '181 days' WHERE starboard_id=$1")
        .bind(board.id)
        .execute(db.pool())
        .await
        .unwrap();
    assert_eq!(db.sweep_star_posts().await.unwrap(), 1);
    assert!(
        db.due_star_cleanups()
            .await
            .unwrap()
            .contains(&(target as i64, copy as i64))
    );
    db.finish_star_cleanup(target as i64, copy as i64).await.unwrap();
    db.star_change(board.id, message + 1, source, author, voter, Some(3))
        .await
        .unwrap();
    let token = db.claim_star_post(board.id, message + 1, 3).await.unwrap().unwrap();
    assert!(
        db.finish_star_post(board.id, message + 1, token, Some(copy + 1))
            .await
            .unwrap()
    );
    db.delete_starboard(guild, board.id).await.unwrap();
    assert!(
        db.due_star_cleanups()
            .await
            .unwrap()
            .contains(&(target as i64, (copy + 1) as i64))
    );
}

#[tokio::test]
async fn privacy_delete_recounts_other_posts_and_queues_authored_copies() {
    let _isolation = crate::support::exclusive().await;

    let Some(db) = cleanup_db().await else { return };
    let [guild, target, source, message, author, voter, copy] = ids();
    let board = db.save_starboard(&board(guild, target)).await.unwrap();
    db.star_change(board.id, message, source, author, voter, Some(3))
        .await
        .unwrap();
    db.delete_user_data(voter, chrono::Utc::now()).await.unwrap();
    assert_eq!(db.star_post(board.id, message).await.unwrap().unwrap().star_count, 0);
    assert!(db.top_stargazers(board.id).await.unwrap().is_empty());
    db.star_change(board.id, message, source, author, voter, Some(3))
        .await
        .unwrap();
    assert_eq!(db.star_post(board.id, message).await.unwrap().unwrap().star_count, 0);
    db.star_change(board.id, message + 1, source, author, voter + 2, Some(3))
        .await
        .unwrap();
    let token = db.claim_star_post(board.id, message + 1, 3).await.unwrap().unwrap();
    assert!(
        db.finish_star_post(board.id, message + 1, token, Some(copy))
            .await
            .unwrap()
    );
    db.delete_user_data(author, chrono::Utc::now()).await.unwrap();
    assert!(
        db.due_star_cleanups()
            .await
            .unwrap()
            .contains(&(target as i64, copy as i64))
    );
}

#[tokio::test]
async fn stale_below_threshold_snapshot_cannot_remove_a_recovered_copy() {
    let _isolation = crate::support::shared().await;
    let Some(db) = db(5).await else { return };
    let [guild, target, source, message, author, voter, copy] = ids();
    let board = db.save_starboard(&board(guild, target)).await.unwrap();
    db.star_change(board.id, message, source, author, voter, Some(3))
        .await
        .unwrap();
    let token = db.claim_star_post(board.id, message, 3).await.unwrap().unwrap();
    assert!(db.finish_star_post(board.id, message, token, Some(copy)).await.unwrap());
    let stale = db
        .star_change(board.id, message, source, author, voter, None)
        .await
        .unwrap();
    assert_eq!(stale.star_count, 0);
    db.star_change(board.id, message, source, author, voter, Some(3))
        .await
        .unwrap();
    assert!(!db.clear_star_copy_if_below(board.id, message, copy, 3).await.unwrap());
    assert_eq!(
        db.star_post(board.id, message).await.unwrap().unwrap().board_message_id,
        Some(copy as i64)
    );
}

#[tokio::test]
async fn repeated_reaction_changes_coalesce_into_one_due_sync() {
    let _isolation = crate::support::exclusive().await;
    let Some(db) = cleanup_db().await else { return };
    let [guild, target, source, message, author, _, _] = ids();
    let board = db.save_starboard(&board(guild, target)).await.unwrap();
    for _ in 0..10 {
        db.touch_star_post(board.id, message, source, author).await.unwrap();
    }
    let pending: i64 =
        sqlx::query_scalar("SELECT count(*) FROM starboard_posts WHERE starboard_id=$1 AND message_id=$2")
            .bind(board.id)
            .bind(message as i64)
            .fetch_one(db.pool())
            .await
            .unwrap();
    assert_eq!(pending, 1);
    let due: chrono::DateTime<chrono::Utc> =
        sqlx::query_scalar("SELECT sync_due FROM starboard_posts WHERE starboard_id=$1 AND message_id=$2")
            .bind(board.id)
            .bind(message as i64)
            .fetch_one(db.pool())
            .await
            .unwrap();
    assert!(due > chrono::Utc::now());
    for periodic in [false, true] {
        sqlx::query(
            "UPDATE starboard_posts SET sync_due=timestamp '1900-01-01' WHERE starboard_id=$1 AND message_id=$2",
        )
        .bind(board.id)
        .bind(message as i64)
        .execute(db.pool())
        .await
        .unwrap();
        let claimed = db.due_star_posts(1, periodic).await.unwrap();
        assert_eq!(claimed[0].0, board.id);
        assert_eq!(claimed[0].1, message as i64);
        assert!(claimed[0].3);
        let version = db.star_sync_version(board.id, message).await.unwrap().unwrap();
        db.finish_star_sync(board.id, message, claimed[0].2, version, false)
            .await
            .unwrap();
        let retry: chrono::DateTime<chrono::Utc> =
            sqlx::query_scalar("SELECT sync_due FROM starboard_posts WHERE starboard_id=$1 AND message_id=$2")
                .bind(board.id)
                .bind(message as i64)
                .fetch_one(db.pool())
                .await
                .unwrap();
        assert!(retry > chrono::Utc::now());
    }
}

#[tokio::test]
async fn ambiguous_send_is_parked_until_copy_can_be_recovered_or_deleted() {
    let _isolation = crate::support::exclusive().await;

    let Some(db) = cleanup_db().await else { return };
    let [guild, target, source, message, author, voter, copy] = ids();
    let board = db.save_starboard(&board(guild, target)).await.unwrap();
    db.star_change(board.id, message, source, author, voter, Some(3))
        .await
        .unwrap();
    let token = db.claim_star_post(board.id, message, 3).await.unwrap().unwrap();
    db.mark_star_post_ambiguous(board.id, message, token, target)
        .await
        .unwrap();
    sqlx::query("UPDATE starboard_posts SET updated_at=now()-interval '3 minutes' WHERE starboard_id=$1")
        .bind(board.id)
        .execute(db.pool())
        .await
        .unwrap();
    assert!(db.claim_star_post(board.id, message, 3).await.unwrap().is_none());
    assert!(db.recover_ambiguous_star_post(target, message, copy).await.unwrap());
    assert_eq!(
        db.star_post(board.id, message).await.unwrap().unwrap().board_message_id,
        Some(copy as i64)
    );
    db.delete_star_source(guild, message).await.unwrap();
    assert!(!db.recover_ambiguous_star_post(target, message, copy + 1).await.unwrap());
    assert!(
        db.due_star_cleanups()
            .await
            .unwrap()
            .contains(&(target as i64, copy as i64))
    );
}
