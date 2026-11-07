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

fn source_lock(guild: u64, message: u64) -> i64 {
    (guild ^ message.rotate_left(13)) as i64
}

impl Db {
    pub async fn touch_star_post(&self, board: i64, message: u64, channel: u64, author: u64) -> anyhow::Result<()> {
        let guild: Option<i64> = sqlx::query_scalar("SELECT guild_id FROM starboards WHERE id=$1")
            .bind(board)
            .fetch_optional(self.pool())
            .await?;
        let Some(guild) = guild else { return Ok(()) };
        let mut tx = self.pool().begin().await?;
        sqlx::query("SELECT pg_advisory_xact_lock($1)")
            .bind(source_lock(guild as u64, message))
            .execute(&mut *tx)
            .await?;
        crate::locks::lock_users(&mut tx, &[sql_id(author)]).await?;
        sqlx::query("INSERT INTO starboard_posts(starboard_id,message_id,channel_id,author_id,sync_due,sync_version) SELECT $1,$2,$3,$4,now()+interval '2 seconds',1 WHERE NOT EXISTS (SELECT 1 FROM starboard_deleted_sources WHERE guild_id=$5 AND message_id=$2) ON CONFLICT (starboard_id,message_id) DO UPDATE SET sync_due=now()+interval '2 seconds',sync_version=starboard_posts.sync_version+1")
            .bind(board).bind(sql_id(message)).bind(sql_id(channel)).bind(sql_id(author)).bind(guild)
            .execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn due_star_posts(&self, limit: i64, periodic: bool) -> anyhow::Result<Vec<(i64, i64, i64, bool)>> {
        let query = if periodic {
            "WITH ready AS (SELECT starboard_id,message_id,COALESCE(sync_due<=now(),false) AS forced FROM starboard_posts WHERE sync_due<=now() OR (board_message_id>0 AND checked_at<now()-interval '1 minute') OR checked_at<now()-interval '5 minutes' ORDER BY COALESCE(sync_due,checked_at) LIMIT $1 FOR UPDATE SKIP LOCKED) UPDATE starboard_posts p SET sync_due=now()+interval '1 minute',sync_token=txid_current(),checked_at=now() FROM ready r WHERE p.starboard_id=r.starboard_id AND p.message_id=r.message_id RETURNING p.starboard_id,p.message_id,p.sync_token,r.forced"
        } else {
            "WITH ready AS (SELECT starboard_id,message_id,true AS forced FROM starboard_posts WHERE sync_due<=now() ORDER BY sync_due LIMIT $1 FOR UPDATE SKIP LOCKED) UPDATE starboard_posts p SET sync_due=now()+interval '1 minute',sync_token=txid_current(),checked_at=now() FROM ready r WHERE p.starboard_id=r.starboard_id AND p.message_id=r.message_id RETURNING p.starboard_id,p.message_id,p.sync_token,r.forced"
        };
        Ok(sqlx::query_as(query).bind(limit).fetch_all(self.pool()).await?)
    }

    pub async fn finish_star_sync(
        &self,
        board: i64,
        message: u64,
        token: i64,
        version: i64,
        success: bool,
    ) -> anyhow::Result<()> {
        sqlx::query("UPDATE starboard_posts SET sync_due=CASE WHEN sync_version=$4 THEN CASE WHEN $5 THEN NULL ELSE now()+interval '10 seconds' END ELSE sync_due END WHERE starboard_id=$1 AND message_id=$2 AND sync_token=$3")
            .bind(board).bind(sql_id(message)).bind(token).bind(version).bind(success).execute(self.pool()).await?;
        Ok(())
    }

    pub async fn star_sync_version(&self, board: i64, message: u64) -> anyhow::Result<Option<i64>> {
        Ok(
            sqlx::query_scalar("SELECT sync_version FROM starboard_posts WHERE starboard_id=$1 AND message_id=$2")
                .bind(board)
                .bind(sql_id(message))
                .fetch_optional(self.pool())
                .await?,
        )
    }

    pub async fn replace_star_votes(
        &self,
        board: i64,
        message: u64,
        votes: &[(u64, i32)],
    ) -> anyhow::Result<Option<(StarPost, bool)>> {
        let mut tx = self.pool().begin().await?;
        let mut ids: Vec<i64> =
            sqlx::query_scalar("SELECT user_id FROM starboard_stars WHERE starboard_id=$1 AND message_id=$2")
                .bind(board)
                .bind(sql_id(message))
                .fetch_all(&mut *tx)
                .await?;
        ids.extend(votes.iter().map(|(user, _)| sql_id(*user)));
        if let Some(author) = sqlx::query_scalar::<_, i64>(
            "SELECT author_id FROM starboard_posts WHERE starboard_id=$1 AND message_id=$2",
        )
        .bind(board)
        .bind(sql_id(message))
        .fetch_optional(&mut *tx)
        .await?
        {
            ids.push(author);
        }
        crate::locks::lock_users(&mut tx, &ids).await?;
        let post: Option<StarPost> = sqlx::query_as("SELECT starboard_id,message_id,channel_id,author_id,star_count,board_message_id FROM starboard_posts WHERE starboard_id=$1 AND message_id=$2 FOR UPDATE")
            .bind(board)
            .bind(sql_id(message))
            .fetch_optional(&mut *tx)
            .await?;
        let Some(post) = post else {
            return Ok(None);
        };
        let current: Vec<(i64, i32)> = sqlx::query_as(
            "SELECT user_id,weight FROM starboard_stars WHERE starboard_id=$1 AND message_id=$2 ORDER BY user_id",
        )
        .bind(board)
        .bind(sql_id(message))
        .fetch_all(&mut *tx)
        .await?;
        let mut wanted: Vec<(i64, i32)> = votes.iter().map(|(user, weight)| (sql_id(*user), *weight)).collect();
        wanted.sort_unstable_by_key(|(user, _)| *user);
        if current == wanted {
            tx.commit().await?;
            return Ok(Some((post, false)));
        }
        sqlx::query("DELETE FROM starboard_stars WHERE starboard_id=$1 AND message_id=$2")
            .bind(board)
            .bind(sql_id(message))
            .execute(&mut *tx)
            .await?;
        for (user, weight) in votes {
            sqlx::query("INSERT INTO starboard_stars(starboard_id,message_id,user_id,weight) SELECT $1,$2,$3,$4 WHERE NOT EXISTS (SELECT 1 FROM deleted_users WHERE user_id=$3)")
                .bind(board).bind(sql_id(message)).bind(sql_id(*user)).bind(weight).execute(&mut *tx).await?;
        }
        let post = sqlx::query_as("UPDATE starboard_posts SET star_count=(SELECT COALESCE(SUM(weight),0)::INT FROM starboard_stars WHERE starboard_id=$1 AND message_id=$2) WHERE starboard_id=$1 AND message_id=$2 RETURNING starboard_id,message_id,channel_id,author_id,star_count,board_message_id")
            .bind(board).bind(sql_id(message)).fetch_one(&mut *tx).await?;
        tx.commit().await?;
        Ok(Some((post, true)))
    }

    pub async fn queue_star_cleanup(&self, channel: u64, copy: u64) -> anyhow::Result<()> {
        sqlx::query("INSERT INTO starboard_copy_cleanup(channel_id,copy_id) VALUES($1,$2) ON CONFLICT DO NOTHING")
            .bind(sql_id(channel))
            .bind(sql_id(copy))
            .execute(self.pool())
            .await?;
        Ok(())
    }

    pub async fn due_star_cleanups(&self) -> anyhow::Result<Vec<(i64, i64)>> {
        Ok(sqlx::query_as("UPDATE starboard_copy_cleanup SET due_at=now()+interval '1 minute' WHERE (channel_id,copy_id) IN (SELECT channel_id,copy_id FROM starboard_copy_cleanup WHERE due_at<=now() LIMIT 100 FOR UPDATE SKIP LOCKED) RETURNING channel_id,copy_id")
            .fetch_all(self.pool()).await?)
    }

    pub async fn finish_star_cleanup(&self, channel: i64, copy: i64) -> anyhow::Result<()> {
        sqlx::query("DELETE FROM starboard_copy_cleanup WHERE channel_id=$1 AND copy_id=$2")
            .bind(channel)
            .bind(copy)
            .execute(self.pool())
            .await?;
        Ok(())
    }

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
        let saved: Starboard = sqlx::query_as(sqlx::AssertSqlSafe(q.as_str()))
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
        sqlx::query("UPDATE starboard_posts SET sync_version=sync_version+1,sync_due=now() WHERE starboard_id=$1")
            .bind(saved.id)
            .execute(&mut *tx)
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
        let guild: i64 = sqlx::query_scalar("SELECT guild_id FROM starboards WHERE id=$1")
            .bind(board)
            .fetch_one(&mut *tx)
            .await?;
        sqlx::query("SELECT pg_advisory_xact_lock($1)")
            .bind(source_lock(guild as u64, message))
            .execute(&mut *tx)
            .await?;
        crate::locks::lock_users(&mut tx, &[sql_id(author), sql_id(user)]).await?;
        sqlx::query("INSERT INTO starboard_posts (starboard_id,message_id,channel_id,author_id) SELECT $1,$2,$3,$4 WHERE NOT EXISTS (SELECT 1 FROM starboard_deleted_sources d JOIN starboards b ON b.guild_id=d.guild_id WHERE b.id=$1 AND d.message_id=$2) ON CONFLICT DO NOTHING")
            .bind(board).bind(sql_id(message)).bind(sql_id(channel)).bind(sql_id(author)).execute(&mut *tx).await?;
        sqlx::query("SELECT message_id FROM starboard_posts WHERE starboard_id=$1 AND message_id=$2 FOR UPDATE")
            .bind(board)
            .bind(sql_id(message))
            .fetch_one(&mut *tx)
            .await?;
        match weight {
            Some(weight) => {
                sqlx::query("INSERT INTO starboard_stars (starboard_id,message_id,user_id,weight) SELECT $1,$2,$3,$4 WHERE NOT EXISTS (SELECT 1 FROM deleted_users WHERE user_id=$3) ON CONFLICT (starboard_id,message_id,user_id) DO UPDATE SET weight=EXCLUDED.weight")
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
            "UPDATE starboard_posts SET star_count=(SELECT COALESCE(SUM(weight),0)::INT FROM starboard_stars WHERE starboard_id=$1 AND message_id=$2),sync_version=sync_version+1,sync_due=now()+interval '2 seconds' WHERE starboard_id=$1 AND message_id=$2 RETURNING {POST}"
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
            let post = sqlx::query_as("UPDATE starboard_posts SET star_count=0,sync_version=sync_version+1,sync_due=now()+interval '2 seconds' WHERE starboard_id=$1 AND message_id=$2 RETURNING starboard_id,message_id,channel_id,author_id,star_count,board_message_id")
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

    /// The token fences a slow HTTP sender after another worker reclaims its lease.
    pub async fn claim_star_post(&self, board: i64, message: u64, threshold: i32) -> anyhow::Result<Option<i64>> {
        Ok(sqlx::query_scalar("UPDATE starboard_posts SET board_message_id=-1,updated_at=now(),claim_token=txid_current() WHERE starboard_id=$1 AND message_id=$2 AND (board_message_id IS NULL OR (board_message_id=-1 AND claim_token<>-1 AND updated_at<now()-interval '2 minutes')) AND star_count >= $3 RETURNING claim_token")
            .bind(board).bind(sql_id(message)).bind(threshold).fetch_optional(self.pool()).await?)
    }

    pub async fn finish_star_post(
        &self,
        board: i64,
        message: u64,
        token: i64,
        posted: Option<u64>,
    ) -> anyhow::Result<bool> {
        Ok(sqlx::query("UPDATE starboard_posts SET board_message_id=$4,updated_at=now(),sync_due=now() WHERE starboard_id=$1 AND message_id=$2 AND board_message_id=-1 AND claim_token=$3")
            .bind(board).bind(sql_id(message)).bind(token).bind(posted.map(sql_id)).execute(self.pool()).await?.rows_affected() == 1)
    }

    pub async fn mark_star_post_ambiguous(
        &self,
        board: i64,
        message: u64,
        token: i64,
        destination: u64,
    ) -> anyhow::Result<()> {
        let mut tx = self.pool().begin().await?;
        sqlx::query("UPDATE starboard_posts SET claim_token=-1 WHERE starboard_id=$1 AND message_id=$2 AND board_message_id=-1 AND claim_token=$3")
            .bind(board).bind(sql_id(message)).bind(token).execute(&mut *tx).await?;
        sqlx::query("INSERT INTO starboard_copy_cleanup(channel_id,copy_id,due_at) VALUES($1,$2,now()+interval '10 seconds') ON CONFLICT DO NOTHING")
            .bind(sql_id(destination)).bind(-sql_id(message)).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn recover_ambiguous_star_post(&self, destination: u64, source: u64, copy: u64) -> anyhow::Result<bool> {
        Ok(sqlx::query("UPDATE starboard_posts p SET board_message_id=$3,claim_token=0,sync_due=now() FROM starboards b WHERE b.id=p.starboard_id AND b.channel_id=$1 AND p.message_id=$2 AND p.board_message_id=-1 AND p.claim_token=-1")
            .bind(sql_id(destination)).bind(sql_id(source)).bind(sql_id(copy)).execute(self.pool()).await?.rows_affected()==1)
    }

    pub async fn clear_star_copy(&self, board: i64, message: u64, copy: u64) -> anyhow::Result<()> {
        sqlx::query("WITH gone AS (UPDATE starboard_posts SET board_message_id=NULL,sync_due=now()+interval '2 seconds',sync_version=sync_version+1 WHERE starboard_id=$1 AND message_id=$2 AND board_message_id=$3 RETURNING starboard_id,board_message_id) INSERT INTO starboard_copy_cleanup(channel_id,copy_id) SELECT b.channel_id,$3 FROM gone JOIN starboards b ON b.id=gone.starboard_id ON CONFLICT DO NOTHING")
            .bind(board).bind(sql_id(message)).bind(sql_id(copy)).execute(self.pool()).await?;
        Ok(())
    }

    pub async fn clear_star_copy_if_below(
        &self,
        board: i64,
        message: u64,
        copy: u64,
        threshold: i32,
    ) -> anyhow::Result<bool> {
        Ok(sqlx::query("WITH gone AS (UPDATE starboard_posts SET board_message_id=NULL,sync_due=now()+interval '2 seconds',sync_version=sync_version+1 WHERE starboard_id=$1 AND message_id=$2 AND board_message_id=$3 AND star_count<$4 RETURNING starboard_id) INSERT INTO starboard_copy_cleanup(channel_id,copy_id) SELECT b.channel_id,$3 FROM gone JOIN starboards b ON b.id=gone.starboard_id ON CONFLICT DO NOTHING")
            .bind(board).bind(sql_id(message)).bind(sql_id(copy)).bind(threshold).execute(self.pool()).await?.rows_affected() == 1)
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
            sqlx::query("SELECT pg_advisory_xact_lock($1)")
                .bind(source_lock(guild, message)).execute(&mut *tx).await?;
            crate::locks::lock_users(&mut tx, &ids).await?;
            sqlx::query("INSERT INTO starboard_deleted_sources(guild_id,message_id) VALUES($1,$2) ON CONFLICT DO NOTHING")
                .bind(sql_id(guild)).bind(sql_id(message)).execute(&mut *tx).await?;
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
            let posts: Vec<StarPost> = sqlx::query_as(q)
                .bind(sql_id(guild))
                .bind(sql_id(message))
                .fetch_all(&mut *tx)
                .await?;
            for post in &posts {
                if let Some(copy) = post.board_message_id.filter(|v| *v > 0) {
                    sqlx::query("INSERT INTO starboard_copy_cleanup(channel_id,copy_id) SELECT channel_id,$2 FROM starboards WHERE id=$1 ON CONFLICT DO NOTHING")
                        .bind(post.starboard_id).bind(copy).execute(&mut *tx).await?;
                }
            }
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
            let mut tx = self.pool().begin().await?;
            let rows = sqlx::query("WITH old AS (SELECT p.starboard_id,p.message_id,b.channel_id AS destination,p.board_message_id FROM starboard_posts p JOIN starboards b ON b.id=p.starboard_id WHERE p.created_at<now()-interval '180 days' LIMIT 500 FOR UPDATE OF p SKIP LOCKED), jobs AS (INSERT INTO starboard_copy_cleanup(channel_id,copy_id) SELECT destination,board_message_id FROM old WHERE board_message_id>0 ON CONFLICT DO NOTHING) DELETE FROM starboard_posts p USING old WHERE p.starboard_id=old.starboard_id AND p.message_id=old.message_id")
                .execute(&mut *tx).await?.rows_affected();
            tx.commit().await?;
            Ok(rows)
        })
        .await
    }
}
