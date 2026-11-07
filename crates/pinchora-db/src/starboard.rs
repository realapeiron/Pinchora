//! Starboard configuration and atomic reaction accounting.

use crate::{Db, sql_id};

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct Starboard {
    pub id: i64,
    pub guild_id: i64,
    pub name: String,
    pub channel_id: i64,
    pub emoji: String,
    pub threshold: i32,
    pub self_star: bool,
    pub allow_bots: bool,
    pub only_channels: Vec<i64>,
    pub except_channels: Vec<i64>,
    pub require_image: bool,
    pub color: i32,
    pub role_weights: serde_json::Value,
    pub remove_below: bool,
    pub enabled: bool,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct StarPost {
    pub starboard_id: i64,
    pub message_id: i64,
    pub channel_id: i64,
    pub author_id: i64,
    pub star_count: i32,
    pub board_message_id: Option<i64>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct VoteChannel {
    pub channel_id: i64,
    pub guild_id: i64,
    pub up_emoji: String,
    pub down_emoji: String,
    pub only_media: bool,
    pub create_thread: bool,
    pub enabled: bool,
}

const BOARD: &str = "id,guild_id,name,channel_id,emoji,threshold,self_star,allow_bots,only_channels,except_channels,require_image,color,role_weights,remove_below,enabled";
const POST: &str = "starboard_id,message_id,channel_id,author_id,star_count,board_message_id";

impl Db {
    pub async fn starboards(&self, guild: u64) -> anyhow::Result<Vec<Starboard>> {
        let q = format!("SELECT {BOARD} FROM starboards WHERE guild_id=$1 ORDER BY id");
        Ok(sqlx::query_as(sqlx::AssertSqlSafe(q.as_str()))
            .bind(sql_id(guild))
            .fetch_all(self.pool())
            .await?)
    }

    pub async fn starboard(&self, id: i64) -> anyhow::Result<Option<Starboard>> {
        let q = format!("SELECT {BOARD} FROM starboards WHERE id=$1");
        Ok(sqlx::query_as(sqlx::AssertSqlSafe(q.as_str()))
            .bind(id)
            .fetch_optional(self.pool())
            .await?)
    }

    pub async fn save_starboard(&self, board: &Starboard) -> anyhow::Result<Starboard> {
        let mut tx = self.pool().begin().await?;
        sqlx::query("SELECT pg_advisory_xact_lock($1)")
            .bind(board.guild_id ^ 0x5342_0000_0000_0000_i64)
            .execute(&mut *tx)
            .await?;
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM starboards WHERE guild_id=$1 AND channel_id<>$2")
            .bind(board.guild_id)
            .bind(board.channel_id)
            .fetch_one(&mut *tx)
            .await?;
        if count >= pinchora_core::starboard::MAX_BOARDS as i64 {
            anyhow::bail!("starboard limit reached");
        }
        let q = format!(
            "INSERT INTO starboards (guild_id,name,channel_id,emoji,threshold,self_star,allow_bots,only_channels,except_channels,require_image,color,role_weights,remove_below,enabled) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14) ON CONFLICT (guild_id,channel_id) DO UPDATE SET name=EXCLUDED.name, emoji=EXCLUDED.emoji, threshold=EXCLUDED.threshold, self_star=EXCLUDED.self_star, allow_bots=EXCLUDED.allow_bots, only_channels=EXCLUDED.only_channels, except_channels=EXCLUDED.except_channels, require_image=EXCLUDED.require_image, color=EXCLUDED.color, role_weights=EXCLUDED.role_weights, remove_below=EXCLUDED.remove_below, enabled=EXCLUDED.enabled RETURNING {BOARD}"
        );
        let saved = sqlx::query_as(sqlx::AssertSqlSafe(q.as_str()))
            .bind(board.guild_id)
            .bind(&board.name)
            .bind(board.channel_id)
            .bind(&board.emoji)
            .bind(board.threshold)
            .bind(board.self_star)
            .bind(board.allow_bots)
            .bind(&board.only_channels)
            .bind(&board.except_channels)
            .bind(board.require_image)
            .bind(board.color)
            .bind(&board.role_weights)
            .bind(board.remove_below)
            .bind(board.enabled)
            .fetch_one(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(saved)
    }

    pub async fn delete_starboard(&self, guild: u64, id: i64) -> anyhow::Result<bool> {
        Ok(sqlx::query("DELETE FROM starboards WHERE guild_id=$1 AND id=$2")
            .bind(sql_id(guild))
            .bind(id)
            .execute(self.pool())
            .await?
            .rows_affected()
            != 0)
    }

    pub async fn votes(&self, guild: u64) -> anyhow::Result<Vec<VoteChannel>> {
        Ok(sqlx::query_as("SELECT channel_id,guild_id,up_emoji,down_emoji,only_media,create_thread,enabled FROM vote_channels WHERE guild_id=$1 ORDER BY channel_id")
            .bind(sql_id(guild)).fetch_all(self.pool()).await?)
    }

    pub async fn vote(&self, channel: u64) -> anyhow::Result<Option<VoteChannel>> {
        Ok(sqlx::query_as("SELECT channel_id,guild_id,up_emoji,down_emoji,only_media,create_thread,enabled FROM vote_channels WHERE channel_id=$1 AND enabled")
            .bind(sql_id(channel)).fetch_optional(self.pool()).await?)
    }

    pub async fn save_vote(&self, vote: &VoteChannel) -> anyhow::Result<()> {
        sqlx::query("INSERT INTO vote_channels (channel_id,guild_id,up_emoji,down_emoji,only_media,create_thread,enabled) VALUES ($1,$2,$3,$4,$5,$6,$7) ON CONFLICT (channel_id) DO UPDATE SET up_emoji=EXCLUDED.up_emoji,down_emoji=EXCLUDED.down_emoji,only_media=EXCLUDED.only_media,create_thread=EXCLUDED.create_thread,enabled=EXCLUDED.enabled")
            .bind(vote.channel_id).bind(vote.guild_id).bind(&vote.up_emoji).bind(&vote.down_emoji)
            .bind(vote.only_media).bind(vote.create_thread).bind(vote.enabled).execute(self.pool()).await?;
        Ok(())
    }

    pub async fn delete_vote(&self, guild: u64, channel: u64) -> anyhow::Result<bool> {
        Ok(
            sqlx::query("DELETE FROM vote_channels WHERE guild_id=$1 AND channel_id=$2")
                .bind(sql_id(guild))
                .bind(sql_id(channel))
                .execute(self.pool())
                .await?
                .rows_affected()
                != 0,
        )
    }

    /// Reconcile a single user's star under a row lock; duplicate gateway events have no effect.
    pub async fn star_change(
        &self,
        board: i64,
        message: u64,
        channel: u64,
        author: u64,
        user: u64,
        weight: Option<i32>,
    ) -> anyhow::Result<StarPost> {
        let mut tx = self.pool.begin().await?;
        crate::locks::lock_users(&mut tx, &[sql_id(author), sql_id(user)]).await?;
        sqlx::query("INSERT INTO starboard_posts (starboard_id,message_id,channel_id,author_id) VALUES ($1,$2,$3,$4) ON CONFLICT DO NOTHING")
            .bind(board).bind(sql_id(message)).bind(sql_id(channel)).bind(sql_id(author)).execute(&mut *tx).await?;
        sqlx::query("SELECT message_id FROM starboard_posts WHERE starboard_id=$1 AND message_id=$2 FOR UPDATE")
            .bind(board)
            .bind(sql_id(message))
            .fetch_one(&mut *tx)
            .await?;
        match weight {
            Some(weight) => {
                sqlx::query("INSERT INTO starboard_stars (starboard_id,message_id,user_id,weight) VALUES ($1,$2,$3,$4) ON CONFLICT (starboard_id,message_id,user_id) DO UPDATE SET weight=EXCLUDED.weight")
                .bind(board).bind(sql_id(message)).bind(sql_id(user)).bind(weight).execute(&mut *tx).await?;
            }
            None => {
                sqlx::query("DELETE FROM starboard_stars WHERE starboard_id=$1 AND message_id=$2 AND user_id=$3")
                    .bind(board)
                    .bind(sql_id(message))
                    .bind(sql_id(user))
                    .execute(&mut *tx)
                    .await?;
            }
        }
        let q = format!(
            "UPDATE starboard_posts SET star_count=(SELECT COALESCE(SUM(weight),0)::INT FROM starboard_stars WHERE starboard_id=$1 AND message_id=$2),updated_at=CASE WHEN board_message_id=-1 THEN updated_at ELSE now() END WHERE starboard_id=$1 AND message_id=$2 RETURNING {POST}"
        );
        let post = sqlx::query_as(sqlx::AssertSqlSafe(q.as_str()))
            .bind(board)
            .bind(sql_id(message))
            .fetch_one(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(post)
    }

    /// Clear an emoji's stars after Discord removes all its reactions.
    pub async fn clear_stars(&self, board: i64, message: u64) -> anyhow::Result<Option<StarPost>> {
        crate::locks::retry("clear starboard stars", || async {
            let ids: Vec<i64> = sqlx::query_scalar(
                "SELECT user_id FROM starboard_stars WHERE starboard_id=$1 AND message_id=$2
                 UNION SELECT author_id FROM starboard_posts WHERE starboard_id=$1 AND message_id=$2",
            )
            .bind(board)
            .bind(sql_id(message))
            .fetch_all(self.pool())
            .await?;
            let mut tx = self.pool().begin().await?;
            crate::locks::lock_users(&mut tx, &ids).await?;
            sqlx::query("SELECT message_id FROM starboard_posts WHERE starboard_id=$1 AND message_id=$2 FOR UPDATE")
                .bind(board)
                .bind(sql_id(message))
                .fetch_optional(&mut *tx)
                .await?;
            let current: Vec<i64> = sqlx::query_scalar(
                "SELECT user_id FROM starboard_stars WHERE starboard_id=$1 AND message_id=$2",
            )
            .bind(board)
            .bind(sql_id(message))
            .fetch_all(&mut *tx)
            .await?;
            crate::locks::try_lock_users(&mut tx, &current).await?;
            sqlx::query("DELETE FROM starboard_stars WHERE starboard_id=$1 AND message_id=$2")
                .bind(board)
                .bind(sql_id(message))
                .execute(&mut *tx)
                .await?;
            let post = sqlx::query_as("UPDATE starboard_posts SET star_count=0,updated_at=now() WHERE starboard_id=$1 AND message_id=$2 RETURNING starboard_id,message_id,channel_id,author_id,star_count,board_message_id")
                .bind(board).bind(sql_id(message)).fetch_optional(&mut *tx).await?;
            tx.commit().await?;
            Ok(post)
        })
        .await
    }

    pub async fn star_post(&self, board: i64, message: u64) -> anyhow::Result<Option<StarPost>> {
        Ok(sqlx::query_as("SELECT starboard_id,message_id,channel_id,author_id,star_count,board_message_id FROM starboard_posts WHERE starboard_id=$1 AND message_id=$2")
            .bind(board).bind(sql_id(message)).fetch_optional(self.pool()).await?)
    }

    /// The one winner marks an unposted row with -1 before calling Discord.
    pub async fn claim_star_post(&self, board: i64, message: u64, threshold: i32) -> anyhow::Result<bool> {
        Ok(sqlx::query("UPDATE starboard_posts SET board_message_id=-1,updated_at=now() WHERE starboard_id=$1 AND message_id=$2 AND (board_message_id IS NULL OR (board_message_id=-1 AND updated_at<now()-interval '2 minutes')) AND star_count >= $3")
            .bind(board).bind(sql_id(message)).bind(threshold).execute(self.pool()).await?.rows_affected() == 1)
    }

    pub async fn finish_star_post(&self, board: i64, message: u64, posted: Option<u64>) -> anyhow::Result<()> {
        sqlx::query("UPDATE starboard_posts SET board_message_id=$3,updated_at=now() WHERE starboard_id=$1 AND message_id=$2 AND board_message_id=-1")
            .bind(board).bind(sql_id(message)).bind(posted.map(sql_id)).execute(self.pool()).await?;
        Ok(())
    }

    pub async fn clear_star_copy(&self, board: i64, message: u64, copy: u64) -> anyhow::Result<()> {
        sqlx::query("UPDATE starboard_posts SET board_message_id=NULL WHERE starboard_id=$1 AND message_id=$2 AND board_message_id=$3")
            .bind(board).bind(sql_id(message)).bind(sql_id(copy)).execute(self.pool()).await?;
        Ok(())
    }

    pub async fn star_posts_for_source(&self, guild: u64, message: u64) -> anyhow::Result<Vec<StarPost>> {
        let q = "SELECT p.starboard_id,p.message_id,p.channel_id,p.author_id,p.star_count,p.board_message_id FROM starboard_posts p JOIN starboards b ON b.id=p.starboard_id WHERE b.guild_id=$1 AND p.message_id=$2";
        Ok(sqlx::query_as(q)
            .bind(sql_id(guild))
            .bind(sql_id(message))
            .fetch_all(self.pool())
            .await?)
    }

    pub async fn delete_star_source(&self, guild: u64, message: u64) -> anyhow::Result<Vec<StarPost>> {
        crate::locks::retry("delete starboard source", || async {
            let users = "SELECT p.author_id AS user_id FROM starboard_posts p JOIN starboards b ON b.id=p.starboard_id WHERE b.guild_id=$1 AND p.message_id=$2
                         UNION SELECT s.user_id FROM starboard_stars s JOIN starboards b ON b.id=s.starboard_id WHERE b.guild_id=$1 AND s.message_id=$2";
            let ids: Vec<i64> = sqlx::query_scalar(users)
                .bind(sql_id(guild))
                .bind(sql_id(message))
                .fetch_all(self.pool())
                .await?;
            let mut tx = self.pool().begin().await?;
            crate::locks::lock_users(&mut tx, &ids).await?;
            sqlx::query("SELECT p.message_id FROM starboard_posts p JOIN starboards b ON b.id=p.starboard_id WHERE b.guild_id=$1 AND p.message_id=$2 FOR UPDATE OF p")
                .bind(sql_id(guild))
                .bind(sql_id(message))
                .fetch_all(&mut *tx)
                .await?;
            let current: Vec<i64> = sqlx::query_scalar(users)
                .bind(sql_id(guild))
                .bind(sql_id(message))
                .fetch_all(&mut *tx)
                .await?;
            crate::locks::try_lock_users(&mut tx, &current).await?;
            let q = "DELETE FROM starboard_posts p USING starboards b WHERE p.starboard_id=b.id AND b.guild_id=$1 AND p.message_id=$2 RETURNING p.starboard_id,p.message_id,p.channel_id,p.author_id,p.star_count,p.board_message_id";
            let posts = sqlx::query_as(q)
                .bind(sql_id(guild))
                .bind(sql_id(message))
                .fetch_all(&mut *tx)
                .await?;
            tx.commit().await?;
            Ok(posts)
        })
        .await
    }

    pub async fn top_star_posts(&self, board: i64, offset: i64) -> anyhow::Result<Vec<StarPost>> {
        let q = format!(
            "SELECT {POST} FROM starboard_posts WHERE starboard_id=$1 AND star_count>0 ORDER BY star_count DESC,created_at DESC LIMIT 10 OFFSET $2"
        );
        Ok(sqlx::query_as(sqlx::AssertSqlSafe(q.as_str()))
            .bind(board)
            .bind(offset)
            .fetch_all(self.pool())
            .await?)
    }

    pub async fn top_stargazers(&self, board: i64) -> anyhow::Result<Vec<(i64, i64)>> {
        Ok(sqlx::query_as("SELECT user_id,COUNT(*)::BIGINT FROM starboard_stars WHERE starboard_id=$1 GROUP BY user_id ORDER BY COUNT(*) DESC LIMIT 5")
            .bind(board).fetch_all(self.pool()).await?)
    }

    pub async fn sweep_star_posts(&self) -> anyhow::Result<u64> {
        crate::locks::retry("sweep starboard posts", || async {
            Ok(sqlx::query(
                "DELETE FROM starboard_posts WHERE (starboard_id,message_id) IN
                (SELECT starboard_id,message_id FROM starboard_posts
                 WHERE created_at < now()-interval '180 days' LIMIT 500 FOR UPDATE SKIP LOCKED)",
            )
            .execute(self.pool())
            .await?
            .rows_affected())
        })
        .await
    }
}
