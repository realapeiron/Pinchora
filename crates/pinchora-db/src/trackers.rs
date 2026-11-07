use serde_json::Value;
use sqlx::FromRow;

use crate::{Db, sql_id};

#[derive(Debug, Clone, FromRow)]
pub struct TrackerSettings {
    pub guild_id: i64,
    pub patch_channels: Value,
    pub mc_watch: Value,
    pub mc_news_channel_id: Option<i64>,
}

#[derive(Debug, Clone, FromRow)]
pub struct RiotLink {
    pub puuid: String,
    pub game_name: String,
    pub tag_line: String,
    pub region: String,
}

impl Db {
    pub async fn tracker_prune(&self) -> anyhow::Result<()> {
        sqlx::query("DELETE FROM tracker_cache WHERE fetched_at < now() - interval '1 day'")
            .execute(self.pool())
            .await?;
        Ok(())
    }
    pub async fn tracker_settings_all(&self) -> anyhow::Result<Vec<TrackerSettings>> {
        Ok(
            sqlx::query_as("SELECT guild_id,patch_channels,mc_watch,mc_news_channel_id FROM tracker_settings")
                .fetch_all(self.pool())
                .await?,
        )
    }
    pub async fn tracker_watch_message(&self, guild: u64, host: &str, message_id: u64) -> anyhow::Result<()> {
        sqlx::query("UPDATE tracker_settings SET mc_watch=(SELECT coalesce(jsonb_agg(CASE WHEN item->>'host'=$2 THEN jsonb_set(item,'{message_id}',to_jsonb($3::text)) ELSE item END ORDER BY ordinal), '[]'::jsonb) FROM jsonb_array_elements(mc_watch) WITH ORDINALITY AS x(item,ordinal)) WHERE guild_id=$1")
            .bind(sql_id(guild)).bind(host).bind(message_id.to_string()).execute(self.pool()).await?;
        Ok(())
    }
    pub async fn tracker_settings(&self, guild: u64) -> anyhow::Result<TrackerSettings> {
        Ok(sqlx::query_as(
            "SELECT guild_id,patch_channels,mc_watch,mc_news_channel_id FROM tracker_settings WHERE guild_id=$1",
        )
        .bind(sql_id(guild))
        .fetch_optional(self.pool())
        .await?
        .unwrap_or(TrackerSettings {
            guild_id: sql_id(guild),
            patch_channels: serde_json::json!({}),
            mc_watch: serde_json::json!([]),
            mc_news_channel_id: None,
        }))
    }
    pub async fn save_tracker_settings(&self, guild: u64, settings: &TrackerSettings) -> anyhow::Result<()> {
        sqlx::query("INSERT INTO tracker_settings(guild_id,patch_channels,mc_watch,mc_news_channel_id) VALUES($1,$2,$3,$4) ON CONFLICT(guild_id) DO UPDATE SET patch_channels=$2,mc_watch=$3,mc_news_channel_id=$4")
            .bind(sql_id(guild)).bind(&settings.patch_channels).bind(&settings.mc_watch).bind(settings.mc_news_channel_id)
            .execute(self.pool()).await?;
        Ok(())
    }
    pub async fn riot_link(&self, user: u64, game: &str) -> anyhow::Result<Option<RiotLink>> {
        Ok(
            sqlx::query_as("SELECT puuid,game_name,tag_line,region FROM riot_links WHERE user_id=$1 AND game=$2")
                .bind(sql_id(user))
                .bind(game)
                .fetch_optional(self.pool())
                .await?,
        )
    }
    pub async fn save_riot_link(&self, user: u64, game: &str, link: &RiotLink) -> anyhow::Result<()> {
        sqlx::query("INSERT INTO riot_links(user_id,game,puuid,game_name,tag_line,region) VALUES($1,$2,$3,$4,$5,$6) ON CONFLICT(user_id,game) DO UPDATE SET puuid=$3,game_name=$4,tag_line=$5,region=$6,linked_at=now()")
            .bind(sql_id(user)).bind(game).bind(&link.puuid).bind(&link.game_name).bind(&link.tag_line).bind(&link.region)
            .execute(self.pool()).await?;
        Ok(())
    }
    pub async fn delete_riot_link(&self, user: u64, game: &str) -> anyhow::Result<bool> {
        Ok(sqlx::query("DELETE FROM riot_links WHERE user_id=$1 AND game=$2")
            .bind(sql_id(user))
            .bind(game)
            .execute(self.pool())
            .await?
            .rows_affected()
            > 0)
    }
    pub async fn tracker_cached(&self, key: &str, max_age_secs: i64) -> anyhow::Result<Option<Value>> {
        Ok(sqlx::query_scalar(
            "SELECT payload FROM tracker_cache WHERE key=$1 AND fetched_at > now() - make_interval(secs => $2)",
        )
        .bind(key)
        .bind(max_age_secs as f64)
        .fetch_optional(self.pool())
        .await?)
    }
    pub async fn tracker_cache_put(&self, key: &str, value: &Value) -> anyhow::Result<()> {
        sqlx::query("INSERT INTO tracker_cache(key,payload) VALUES($1,$2) ON CONFLICT(key) DO UPDATE SET payload=$2,fetched_at=now()")
            .bind(key).bind(value).execute(self.pool()).await?;
        Ok(())
    }
    pub async fn patch_mark_seen(&self, game: &str, item_id: &str) -> anyhow::Result<bool> {
        Ok(
            sqlx::query("INSERT INTO patch_seen(game,item_id) VALUES($1,$2) ON CONFLICT DO NOTHING")
                .bind(game)
                .bind(item_id)
                .execute(self.pool())
                .await?
                .rows_affected()
                > 0,
        )
    }
    pub async fn patch_is_seen(&self, game: &str, item_id: &str) -> anyhow::Result<bool> {
        Ok(
            sqlx::query_scalar::<_, bool>("SELECT EXISTS(SELECT 1 FROM patch_seen WHERE game=$1 AND item_id=$2)")
                .bind(game)
                .bind(item_id)
                .fetch_one(self.pool())
                .await?,
        )
    }
}
