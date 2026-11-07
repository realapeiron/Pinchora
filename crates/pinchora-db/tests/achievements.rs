//! Achievement progress and one-time rewards against the real test database.

use chrono::Utc;
use pinchora_core::achievements::Kind;
use pinchora_db::{AchievementDraft, AchievementSave, Db};

async fn db() -> Option<Db> {
    let url = std::env::var("PINCHORA_TEST_DATABASE_URL").ok()?;
    let db = Db::connect_waiting(&url, 5, std::time::Duration::from_secs(60))
        .await
        .expect("connect");
    db.migrate().await.expect("migrate");
    Some(db)
}

fn ids<const N: usize>() -> [u64; N] {
    let base = (std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos() as u64)
        >> 4;
    std::array::from_fn(|i| base + i as u64)
}

fn draft<'a>(name: &'a str, kind: Kind, points: i64) -> AchievementDraft<'a> {
    AchievementDraft {
        name,
        description: "A milestone",
        icon: "star",
        kind,
        target: 1,
        reward_role_id: None,
        reward_points: points,
        announce_channel_id: None,
        secret: false,
        enabled: true,
    }
}

#[tokio::test]
async fn progress_reads_only_the_requested_kind() {
    let Some(db) = db().await else { return };
    let [guild, user, board, channel] = ids();
    db.guild_settings(guild).await.unwrap();
    sqlx::query("INSERT INTO users(id,reputation) VALUES($1,8)")
        .bind(user as i64)
        .execute(db.pool())
        .await
        .unwrap();
    sqlx::query("INSERT INTO members(guild_id,user_id,xp) VALUES($1,$2,300)")
        .bind(guild as i64)
        .bind(user as i64)
        .execute(db.pool())
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO member_activity_daily(guild_id,user_id,day,messages,voice_minutes) VALUES($1,$2,$3,12,34)",
    )
    .bind(guild as i64)
    .bind(user as i64)
    .bind(Utc::now().date_naive())
    .execute(db.pool())
    .await
    .unwrap();
    sqlx::query("INSERT INTO invite_joins(guild_id,user_id,inviter_id,source) VALUES($1,$2,$2,'invite')")
        .bind(guild as i64)
        .bind(user as i64)
        .execute(db.pool())
        .await
        .unwrap();
    sqlx::query("INSERT INTO starboards(id,guild_id,name,channel_id) VALUES($1,$2,'Test',$3)")
        .bind(board as i64)
        .bind(guild as i64)
        .bind(channel as i64)
        .execute(db.pool())
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO starboard_posts(starboard_id,message_id,channel_id,author_id,star_count) VALUES($1,$2,$3,$4,4)",
    )
    .bind(board as i64)
    .bind((channel + 1) as i64)
    .bind(channel as i64)
    .bind(user as i64)
    .execute(db.pool())
    .await
    .unwrap();
    sqlx::query("INSERT INTO tickets(guild_id,number,opener_id,closed_by,status) VALUES($1,1,$2,$2,'closed')")
        .bind(guild as i64)
        .bind(user as i64)
        .execute(db.pool())
        .await
        .unwrap();
    sqlx::query("INSERT INTO game_stats(user_id,game,played,won) VALUES($1,'chess',3,2)")
        .bind(user as i64)
        .execute(db.pool())
        .await
        .unwrap();
    sqlx::query("INSERT INTO fish_book(user_id,fish_id,caught) VALUES($1,'koi',7)")
        .bind(user as i64)
        .execute(db.pool())
        .await
        .unwrap();
    let expected = [12, 34, 2, 1, 1, 4, 1, 8, 2, 7, 0];
    for (kind, expected) in Kind::ALL.into_iter().zip(expected) {
        assert_eq!(
            db.achievement_progress(guild, user, kind).await.unwrap(),
            expected,
            "{}",
            kind.key()
        );
    }
}

#[tokio::test]
async fn unique_grant_pays_server_points_once_and_privacy_deletes_it() {
    let Some(db) = db().await else { return };
    let [guild, user] = ids();
    let saved = db
        .save_achievement(guild, None, &draft("First win", Kind::Custom, 150))
        .await
        .unwrap();
    let AchievementSave::Saved(id) = saved else {
        panic!("not saved: {saved:?}")
    };
    assert!(db.grant_achievement(guild, user, id, true).await.unwrap().is_some());
    assert!(db.grant_achievement(guild, user, id, true).await.unwrap().is_none());
    assert_eq!(db.member_points(guild, user).await.unwrap().0, 150);
    assert_eq!(db.earned_achievements(guild, user).await.unwrap().len(), 1);
    db.delete_user_data(user, Utc::now()).await.unwrap();
    assert!(db.earned_achievements(guild, user).await.unwrap().is_empty());
    assert!(
        db.grant_achievement(guild, user, id, true).await.unwrap().is_none(),
        "tombstone stops another grant"
    );
}

#[tokio::test]
async fn limit_and_duplicate_names_are_enforced() {
    let Some(db) = db().await else { return };
    let [guild] = ids();
    for n in 0..25 {
        assert!(matches!(
            db.save_achievement(guild, None, &draft(&format!("Prize {n}"), Kind::Level, 0))
                .await
                .unwrap(),
            AchievementSave::Saved(_)
        ));
    }
    assert_eq!(
        db.save_achievement(guild, None, &draft("Extra", Kind::Level, 0))
            .await
            .unwrap(),
        AchievementSave::Full
    );
    let rows = db.achievements(guild).await.unwrap();
    assert_eq!(rows.len(), 25);
    assert!(db.remove_achievement(guild, rows[0].id).await.unwrap());
    assert_eq!(
        db.save_achievement(guild, None, &draft("Prize 1", Kind::Level, 0))
            .await
            .unwrap(),
        AchievementSave::NameTaken
    );
}
