//! Welcome and goodbye image designs (`card_designs`) and the pictures uploaded for them (`card_uploads`).
//! The JSON is checked by `pinchora_core::card_design` before it is written; here it is only stored.

use chrono::{DateTime, Utc};
use pinchora_core::card_design::{CardKind, Delivery, MAX_UPLOADS};

use crate::{Db, sql_id};

/// A server's saved design for one kind of image.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct CardDesignRow {
    /// The design as JSON; `None` means the classic card from the settings.
    pub design: Option<String>,
    pub delivery: String,
    pub updated_at: DateTime<Utc>,
}

impl CardDesignRow {
    /// How the image is sent, the kind's default when the stored key is unknown.
    pub fn delivery(&self, kind: CardKind) -> Delivery {
        Delivery::parse(&self.delivery).unwrap_or(kind.default_delivery())
    }
}

/// An uploaded picture, without its bytes.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct CardUploadRow {
    pub id: String,
    pub width: i32,
    pub height: i32,
    pub created_at: DateTime<Utc>,
}

impl Db {
    pub async fn card_design(&self, guild: u64, kind: CardKind) -> anyhow::Result<Option<CardDesignRow>> {
        Ok(sqlx::query_as::<_, CardDesignRow>(
            "SELECT design::TEXT AS design, delivery, updated_at FROM card_designs WHERE guild_id = $1 AND kind = $2",
        )
        .bind(sql_id(guild))
        .bind(kind.key())
        .fetch_optional(self.pool())
        .await?)
    }

    /// Saves the design (`None`: the classic card) and how it is sent, and returns the row.
    pub async fn put_card_design(
        &self,
        guild: u64,
        kind: CardKind,
        design: Option<&str>,
        delivery: Delivery,
    ) -> anyhow::Result<CardDesignRow> {
        Ok(sqlx::query_as::<_, CardDesignRow>(
            "INSERT INTO card_designs (guild_id, kind, design, delivery) VALUES ($1, $2, $3::JSONB, $4)
             ON CONFLICT (guild_id, kind) DO UPDATE
                 SET design = EXCLUDED.design, delivery = EXCLUDED.delivery, updated_at = now()
             RETURNING design::TEXT AS design, delivery, updated_at",
        )
        .bind(sql_id(guild))
        .bind(kind.key())
        .bind(design)
        .bind(delivery.key())
        .fetch_one(self.pool())
        .await?)
    }

    /// Goes back to the classic card, keeping how it is sent. Returns whether a design of the server's own
    /// was there.
    pub async fn clear_card_design(&self, guild: u64, kind: CardKind) -> anyhow::Result<bool> {
        let done = sqlx::query(
            "UPDATE card_designs SET design = NULL, updated_at = now()
             WHERE guild_id = $1 AND kind = $2 AND design IS NOT NULL",
        )
        .bind(sql_id(guild))
        .bind(kind.key())
        .execute(self.pool())
        .await?;
        Ok(done.rows_affected() > 0)
    }

    /// Keeps an uploaded picture, then lets go of the oldest ones past [`MAX_UPLOADS`] that no saved design
    /// uses.
    pub async fn add_card_upload(
        &self,
        guild: u64,
        id: &str,
        png: &[u8],
        width: u32,
        height: u32,
    ) -> anyhow::Result<()> {
        let mut tx = self.pool().begin().await?;
        sqlx::query("INSERT INTO card_uploads (id, guild_id, png, width, height) VALUES ($1, $2, $3, $4, $5)")
            .bind(id)
            .bind(sql_id(guild))
            .bind(png)
            .bind(width as i32)
            .bind(height as i32)
            .execute(&mut *tx)
            .await?;
        sqlx::query(
            "DELETE FROM card_uploads
             WHERE guild_id = $1
               AND id NOT IN (SELECT id FROM card_uploads WHERE guild_id = $1 ORDER BY created_at DESC, id LIMIT $2)
               AND id NOT IN (SELECT design #>> '{background,id}' FROM card_designs
                              WHERE guild_id = $1 AND design #>> '{background,type}' = 'upload')
               AND id NOT IN (SELECT avatar_upload_id FROM guild_branding WHERE guild_id = $1)
               AND id NOT IN (SELECT banner_upload_id FROM guild_branding WHERE guild_id = $1)",
        )
        .bind(sql_id(guild))
        .bind(MAX_UPLOADS as i64)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(())
    }

    /// An uploaded picture's PNG, if the server has it.
    pub async fn card_upload(&self, guild: u64, id: &str) -> anyhow::Result<Option<Vec<u8>>> {
        let row: Option<(Vec<u8>,)> = sqlx::query_as("SELECT png FROM card_uploads WHERE guild_id = $1 AND id = $2")
            .bind(sql_id(guild))
            .bind(id)
            .fetch_optional(self.pool())
            .await?;
        Ok(row.map(|(png,)| png))
    }

    /// The server's uploads, newest first.
    pub async fn card_uploads(&self, guild: u64) -> anyhow::Result<Vec<CardUploadRow>> {
        Ok(sqlx::query_as::<_, CardUploadRow>(
            "SELECT id, width, height, created_at FROM card_uploads WHERE guild_id = $1 ORDER BY created_at DESC, id",
        )
        .bind(sql_id(guild))
        .fetch_all(self.pool())
        .await?)
    }
}
