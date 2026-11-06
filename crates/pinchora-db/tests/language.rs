//! Runs against a real PostgreSQL when `PINCHORA_TEST_DATABASE_URL` is set; skipped otherwise.
//!
//! A player's own language and a server's default: set, read back, cleared.

use pinchora_core::i18n::Lang;
use pinchora_db::Db;

async fn db() -> Option<Db> {
    let url = std::env::var("PINCHORA_TEST_DATABASE_URL").ok()?;
    let db = Db::connect_waiting(&url, 5, std::time::Duration::from_secs(60))
        .await
        .expect("connect");
    db.migrate().await.expect("migrate");
    Some(db)
}

/// Ids unique per run, so the suite can run repeatedly against one database.
fn ids<const N: usize>() -> [u64; N] {
    let base = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos() as u64
        >> 4;
    std::array::from_fn(|i| base + i as u64)
}

#[tokio::test]
async fn languages_are_set_read_and_cleared() {
    let Some(db) = db().await else { return };
    let [user, guild, stranger] = ids();
    assert_eq!(db.user_language(user).await.unwrap(), None, "nobody picked one yet");
    db.set_user_language(user, Some(Lang::Tr)).await.unwrap();
    assert_eq!(db.user_language(user).await.unwrap(), Some(Lang::Tr));
    // The row it made is an ordinary player row: the profile reads it.
    assert_eq!(db.user(user).await.unwrap().credits, 0);
    db.set_user_language(user, None).await.unwrap();
    assert_eq!(db.user_language(user).await.unwrap(), None);

    assert_eq!(db.guild_language(guild).await.unwrap(), None);
    db.set_guild_language(guild, Some(Lang::Tr)).await.unwrap();
    assert_eq!(db.guild_language(guild).await.unwrap(), Some(Lang::Tr));
    assert_eq!(db.guild_settings(guild).await.unwrap().id, guild as i64);
    db.set_guild_language(guild, None).await.unwrap();
    assert_eq!(db.guild_language(guild).await.unwrap(), None);

    // A code the bot no longer speaks reads as no choice.
    sqlx::query("INSERT INTO users (id, language) VALUES ($1, 'xx')")
        .bind(stranger as i64)
        .execute(db.pool())
        .await
        .unwrap();
    assert_eq!(db.user_language(stranger).await.unwrap(), None);
}
