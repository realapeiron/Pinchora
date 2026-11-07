use chrono::{DateTime, Utc};
use sqlx::FromRow;

use crate::{Db, sql_id};

#[derive(Clone, Debug, Default, FromRow)]
pub struct GuildBranding {
    pub guild_id: i64,
    pub nickname: Option<String>,
    pub avatar_upload_id: Option<String>,
    pub banner_upload_id: Option<String>,
    pub bio: Option<String>,
    pub embed_color: Option<i32>,
    pub active_embed_color: Option<i32>,
    pub applied_at: Option<DateTime<Utc>>,
    pub last_error: Option<String>,
    pub avatar_supported: Option<bool>,
    pub banner_supported: Option<bool>,
    pub bio_supported: Option<bool>,
}

impl Db {
    pub async fn guild_branding(&self, guild: u64) -> anyhow::Result<GuildBranding> {
        Ok(
            sqlx::query_as::<_, GuildBranding>("SELECT * FROM guild_branding WHERE guild_id = $1")
                .bind(sql_id(guild))
                .fetch_optional(self.pool())
                .await?
                .unwrap_or(GuildBranding {
                    guild_id: sql_id(guild),
                    ..Default::default()
                }),
        )
    }

    pub async fn save_guild_branding(&self, guild: u64, branding: &GuildBranding) -> anyhow::Result<()> {
        self.guild_settings(guild).await?;
        sqlx::query(
            "INSERT INTO guild_branding (guild_id, nickname, avatar_upload_id, banner_upload_id, bio, embed_color)
            VALUES ($1,$2,$3,$4,$5,$6) ON CONFLICT (guild_id) DO UPDATE SET
            nickname=EXCLUDED.nickname, avatar_upload_id=EXCLUDED.avatar_upload_id,
            banner_upload_id=EXCLUDED.banner_upload_id, bio=EXCLUDED.bio, embed_color=EXCLUDED.embed_color",
        )
        .bind(sql_id(guild))
        .bind(&branding.nickname)
        .bind(&branding.avatar_upload_id)
        .bind(&branding.banner_upload_id)
        .bind(&branding.bio)
        .bind(branding.embed_color)
        .execute(self.pool())
        .await?;
        Ok(())
    }

    /// Atomic per-guild gate, shared by slash commands and the dashboard across pods.
    pub async fn claim_branding_apply(&self, guild: u64) -> anyhow::Result<bool> {
        let claimed: Option<i64> = sqlx::query_scalar("UPDATE guild_branding SET applied_at = now(), active_embed_color = embed_color, last_error = NULL
            WHERE guild_id = $1 AND (applied_at IS NULL OR applied_at <= now() - interval '10 minutes') RETURNING guild_id")
            .bind(sql_id(guild)).fetch_optional(self.pool()).await?;
        Ok(claimed.is_some())
    }

    pub async fn branding_result(
        &self,
        guild: u64,
        error: Option<&str>,
        features: [Option<bool>; 3],
    ) -> anyhow::Result<()> {
        sqlx::query(
            "UPDATE guild_branding SET last_error=$2, avatar_supported=COALESCE($3,avatar_supported),
            banner_supported=COALESCE($4,banner_supported), bio_supported=COALESCE($5,bio_supported) WHERE guild_id=$1",
        )
        .bind(sql_id(guild))
        .bind(error)
        .bind(features[0])
        .bind(features[1])
        .bind(features[2])
        .execute(self.pool())
        .await?;
        Ok(())
    }

    pub async fn reset_guild_branding(&self, guild: u64) -> anyhow::Result<()> {
        sqlx::query(
            "UPDATE guild_branding SET nickname=NULL, avatar_upload_id=NULL, banner_upload_id=NULL,
            bio=NULL, embed_color=NULL, active_embed_color=NULL WHERE guild_id=$1",
        )
        .bind(sql_id(guild))
        .execute(self.pool())
        .await?;
        Ok(())
    }
}
