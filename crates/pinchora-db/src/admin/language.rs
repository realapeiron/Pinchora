//! The language a player picked (`users.language`) and a server's default (`guilds.language`).

use pinchora_core::infra::i18n::Lang;

use crate::{Db, sql_id};

impl Db {
    /// The language `user` picked with `/language`; `None` follows their Discord client.
    pub async fn user_language(&self, user: u64) -> anyhow::Result<Option<Lang>> {
        let row: Option<(Option<String>,)> = sqlx::query_as("SELECT language FROM users WHERE id = $1")
            .bind(sql_id(user))
            .fetch_optional(self.pool())
            .await?;
        Ok(row.and_then(|(code,)| code).as_deref().and_then(Lang::from_code))
    }

    /// Sets (or with `None` clears) the language `user` is answered in.
    pub async fn set_user_language(&self, user: u64, lang: Option<Lang>) -> anyhow::Result<()> {
        sqlx::query(
            "INSERT INTO users (id, language) VALUES ($1, $2)
             ON CONFLICT (id) DO UPDATE SET language = EXCLUDED.language",
        )
        .bind(sql_id(user))
        .bind(lang.map(Lang::code))
        .execute(self.pool())
        .await?;
        Ok(())
    }

    /// The server's default language; `None` means English.
    pub async fn guild_language(&self, guild: u64) -> anyhow::Result<Option<Lang>> {
        let row: Option<(Option<String>,)> = sqlx::query_as("SELECT language FROM guilds WHERE id = $1")
            .bind(sql_id(guild))
            .fetch_optional(self.pool())
            .await?;
        Ok(row.and_then(|(code,)| code).as_deref().and_then(Lang::from_code))
    }

    /// Sets (or with `None` clears) the server's default language.
    pub async fn set_guild_language(&self, guild: u64, lang: Option<Lang>) -> anyhow::Result<()> {
        sqlx::query(
            "INSERT INTO guilds (id, language) VALUES ($1, $2)
             ON CONFLICT (id) DO UPDATE SET language = EXCLUDED.language",
        )
        .bind(sql_id(guild))
        .bind(lang.map(Lang::code))
        .execute(self.pool())
        .await?;
        Ok(())
    }
}
