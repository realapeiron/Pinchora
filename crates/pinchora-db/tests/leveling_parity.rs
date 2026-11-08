use std::time::{Duration, SystemTime, UNIX_EPOCH};

use pinchora_core::{
    leveling::week_start,
    leveling_parity::{Curve, LevelingParity},
};
use pinchora_db::Db;

#[tokio::test]
async fn policy_voice_ledger_and_leave_reset() {
    let Ok(url) = std::env::var("PINCHORA_TEST_DATABASE_URL") else {
        return;
    };
    let db = Db::connect_waiting(&url, 4, Duration::from_secs(60)).await.unwrap();
    db.migrate().await.unwrap();
    let base = (SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos() as u64) >> 4;
    let (guild, user) = (base, base + 1);
    db.guild_settings(guild).await.unwrap();
    let mut config = LevelingParity {
        curve: Curve::Linear {
            base: 100,
            per_level: 25,
        },
        ..Default::default()
    };
    config.vanity_slug = Some(format!("level-{guild}"));
    db.save_leveling_parity(guild, &config).await.unwrap();
    assert_eq!(db.leveling_parity(guild).await.unwrap(), config);
    assert_eq!(
        db.vanity_guild(config.vanity_slug.as_deref().unwrap()).await.unwrap(),
        Some(guild)
    );
    let week = week_start(chrono::Utc::now());
    let separate = db.add_separate_voice_xp(guild, &[(user, 120)], week).await.unwrap();
    assert_eq!(separate[0].1.old_xp, 0);
    assert_eq!(separate[0].1.new_xp, 120);
    assert!(db.member_rank(guild, user).await.unwrap().is_none());
    assert_eq!(db.voice_level_xp(guild, user).await.unwrap(), 120);
    let shared = db.add_voice_xp(guild, &[(user, 15)], week).await.unwrap();
    assert_eq!(shared[0].1.new_xp, 15);
    assert_eq!(db.voice_level_xp(guild, user).await.unwrap(), 135);
    assert_eq!(db.voice_level_board(guild, 10, 0).await.unwrap()[0].value, 135);
    db.reset_departed_level(guild, user).await.unwrap();
    assert_eq!(db.member_rank(guild, user).await.unwrap().unwrap().xp, 0);
    assert_eq!(db.voice_level_xp(guild, user).await.unwrap(), 0);
    assert!(db.weekly_leaderboard(guild, week, 10, 0).await.unwrap().is_empty());
}
