//! Atomic star claims, unstars, cascades and vote settings on the dedicated test database.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use pinchora_db::{Db, Starboard, VoteChannel};

async fn db() -> Option<Db> {
    let url = std::env::var("PINCHORA_TEST_DATABASE_URL").ok()?;
    let db = Db::connect_waiting(&url, 5, Duration::from_secs(60))
        .await
        .expect("connect");
    db.migrate().await.expect("migrate");
    Some(db)
}
fn ids() -> [u64; 7] {
    let base = (SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos() as u64) >> 4;
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
    let Some(db) = db().await else { return };
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
    assert_ne!(first.unwrap(), second.unwrap());
    db.finish_star_post(board.id, message, Some(message + 1)).await.unwrap();
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
    assert!(!db.claim_star_post(board.id, message, 3).await.unwrap());
}

#[tokio::test]
async fn multiple_boards_and_vote_channels_are_independent() {
    let Some(db) = db().await else { return };
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
    let Some(db) = db().await else { return };
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
