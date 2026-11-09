//! Runs against a real PostgreSQL when `PINCHORA_TEST_DATABASE_URL` is set; skipped otherwise.
//!
//! A player's own language and a server's default: set, read back, cleared.

use pinchora_core::i18n::Lang;

use crate::support::db;

/// Ids unique per run, so the suite can run repeatedly against one database.
use crate::support::ids;

#[tokio::test]
async fn languages_are_set_read_and_cleared() {
    let _isolation = crate::support::shared().await;
    let Some(db) = db(5).await else { return };
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
