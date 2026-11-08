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
    pub active_nickname: Option<String>,
    pub active_avatar_upload_id: Option<String>,
    pub active_banner_upload_id: Option<String>,
    pub active_bio: Option<String>,
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

    /// Claim before HTTP, without holding a transaction across Discord. The colour is from the
    /// same immutable snapshot as the profile payload, even when another editor saves a draft.
    pub async fn claim_branding_apply(&self, guild: u64, color: Option<i32>) -> anyhow::Result<bool> {
        self.guild_settings(guild).await?;
        sqlx::query("INSERT INTO guild_branding(guild_id) VALUES($1) ON CONFLICT DO NOTHING")
            .bind(sql_id(guild))
            .execute(self.pool())
            .await?;
        let claimed: Option<i64> = sqlx::query_scalar("UPDATE guild_branding SET applied_at = now(), active_embed_color = $2,
            last_error = 'nickname,avatar,banner,bio'
            WHERE guild_id = $1 AND (applied_at IS NULL OR applied_at <= now() - interval '10 minutes') RETURNING guild_id")
            .bind(sql_id(guild)).bind(color).fetch_optional(self.pool()).await?;
        Ok(claimed.is_some())
    }

    /// Keep desired and accepted state separate. A refused field retains its last accepted value.
    pub async fn branding_result(
        &self,
        guild: u64,
        desired: &GuildBranding,
        accepted: [bool; 4],
        features: [Option<bool>; 3],
        reset: Option<&GuildBranding>,
    ) -> anyhow::Result<()> {
        let fields = ["nickname", "avatar", "banner", "bio"];
        let failed: Vec<_> = fields
            .into_iter()
            .zip(accepted)
            .filter_map(|(f, ok)| (!ok).then_some(f))
            .collect();
        let error = (!failed.is_empty()).then(|| failed.join(","));
        sqlx::query(
            "UPDATE guild_branding SET last_error=$2,
            active_nickname=CASE WHEN $3 THEN $7 ELSE active_nickname END,
            active_avatar_upload_id=CASE WHEN $4 THEN $8 ELSE active_avatar_upload_id END,
            active_banner_upload_id=CASE WHEN $5 THEN $9 ELSE active_banner_upload_id END,
            active_bio=CASE WHEN $6 THEN $10 ELSE active_bio END,
            avatar_supported=COALESCE($11,avatar_supported), banner_supported=COALESCE($12,banner_supported),
            bio_supported=COALESCE($13,bio_supported),
            nickname=CASE WHEN $14 AND $3 AND nickname IS NOT DISTINCT FROM $15 THEN NULL ELSE nickname END,
            avatar_upload_id=CASE WHEN $14 AND $4 AND avatar_upload_id IS NOT DISTINCT FROM $16 THEN NULL ELSE avatar_upload_id END,
            banner_upload_id=CASE WHEN $14 AND $5 AND banner_upload_id IS NOT DISTINCT FROM $17 THEN NULL ELSE banner_upload_id END,
            bio=CASE WHEN $14 AND $6 AND bio IS NOT DISTINCT FROM $18 THEN NULL ELSE bio END,
            embed_color=CASE WHEN $14 AND embed_color IS NOT DISTINCT FROM $19 THEN NULL ELSE embed_color END WHERE guild_id=$1",
        )
        .bind(sql_id(guild))
        .bind(error)
        .bind(accepted[0])
        .bind(accepted[1])
        .bind(accepted[2])
        .bind(accepted[3])
        .bind(&desired.nickname)
        .bind(&desired.avatar_upload_id)
        .bind(&desired.banner_upload_id)
        .bind(&desired.bio)
        .bind(features[0])
        .bind(features[1])
        .bind(features[2])
        .bind(reset.is_some())
        .bind(reset.and_then(|r| r.nickname.as_deref()))
        .bind(reset.and_then(|r| r.avatar_upload_id.as_deref()))
        .bind(reset.and_then(|r| r.banner_upload_id.as_deref()))
        .bind(reset.and_then(|r| r.bio.as_deref()))
        .bind(reset.and_then(|r| r.embed_color))
        .execute(self.pool())
        .await?;
        Ok(())
    }
}
