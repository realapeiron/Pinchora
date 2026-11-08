//! Atomic quotas shared by slash commands and the dashboard.
use crate::{Db, locks::lock_users, sql_id};
use chrono::{DateTime, Utc};

impl Db {
    pub async fn add_keyword_highlight(&self, guild: u64, user: u64, word: &str) -> anyhow::Result<bool> {
        anyhow::ensure!(
            (2..=40).contains(&word.chars().count()) && word.chars().all(char::is_alphanumeric),
            "invalid keyword"
        );
        let mut tx = self.pool().begin().await?;
        lock_users(&mut tx, &[sql_id(user)]).await?;
        let (count, exists): (i64, bool) = sqlx::query_as("SELECT count(*), COALESCE(bool_or(keyword=$3),false) FROM keyword_highlights WHERE guild_id=$1 AND user_id=$2")
            .bind(sql_id(guild)).bind(sql_id(user)).bind(word).fetch_one(&mut *tx).await?;
        if exists {
            return Ok(true);
        }
        if count >= 10 {
            return Ok(false);
        }
        sqlx::query("INSERT INTO keyword_highlights(guild_id,user_id,keyword) VALUES($1,$2,$3)")
            .bind(sql_id(guild))
            .bind(sql_id(user))
            .bind(word)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(true)
    }

    pub async fn add_checklist_item(&self, user: u64, text: &str) -> anyhow::Result<Option<i64>> {
        let mut tx = self.pool().begin().await?;
        lock_users(&mut tx, &[sql_id(user)]).await?;
        let (count,): (i64,) =
            sqlx::query_as("SELECT count(*) FROM personal_checklist_items WHERE user_id=$1 AND NOT done")
                .bind(sql_id(user))
                .fetch_one(&mut *tx)
                .await?;
        if count >= 50 {
            return Ok(None);
        }
        let (id,): (i64,) =
            sqlx::query_as("INSERT INTO personal_checklist_items(user_id,body) VALUES($1,$2) RETURNING id")
                .bind(sql_id(user))
                .bind(text)
                .fetch_one(&mut *tx)
                .await?;
        tx.commit().await?;
        Ok(Some(id))
    }

    pub async fn add_recurring_reminder(
        &self,
        user: u64,
        text: &str,
        minutes: i32,
        next: DateTime<Utc>,
        language: &str,
    ) -> anyhow::Result<Option<i64>> {
        let mut tx = self.pool().begin().await?;
        lock_users(&mut tx, &[sql_id(user)]).await?;
        let (count,): (i64,) =
            sqlx::query_as("SELECT count(*) FROM personal_recurring_reminders WHERE user_id=$1 AND enabled")
                .bind(sql_id(user))
                .fetch_one(&mut *tx)
                .await?;
        if count >= 25 {
            return Ok(None);
        }
        let (id,): (i64,) = sqlx::query_as("INSERT INTO personal_recurring_reminders(user_id,body,interval_minutes,next_at,language) VALUES($1,$2,$3,$4,$5) RETURNING id")
            .bind(sql_id(user)).bind(text).bind(minutes).bind(next).bind(language).fetch_one(&mut *tx).await?;
        tx.commit().await?;
        Ok(Some(id))
    }

    /// Ban by confession number: moderators never need to learn the author's identity.
    pub async fn ban_confession_author(
        &self,
        guild: u64,
        confession: i64,
        moderator: u64,
        banned: bool,
    ) -> anyhow::Result<bool> {
        let author: Option<(i64,)> = sqlx::query_as("SELECT author_id FROM confessions WHERE guild_id=$1 AND id=$2")
            .bind(sql_id(guild))
            .bind(confession)
            .fetch_optional(self.pool())
            .await?;
        let Some((author,)) = author else {
            return Ok(false);
        };
        let mut tx = self.pool().begin().await?;
        lock_users(&mut tx, &[author, sql_id(moderator)]).await?;
        // Recheck after the privacy lock, before retaining a user id.
        let exists: Option<(i64,)> =
            sqlx::query_as("SELECT author_id FROM confessions WHERE guild_id=$1 AND id=$2 AND author_id=$3 FOR SHARE")
                .bind(sql_id(guild))
                .bind(confession)
                .bind(author)
                .fetch_optional(&mut *tx)
                .await?;
        if exists.is_none() {
            return Ok(false);
        }
        if banned {
            sqlx::query("INSERT INTO confession_bans(guild_id,user_id) VALUES($1,$2) ON CONFLICT DO NOTHING")
                .bind(sql_id(guild))
                .bind(author)
                .execute(&mut *tx)
                .await?;
        } else {
            sqlx::query("DELETE FROM confession_bans WHERE guild_id=$1 AND user_id=$2")
                .bind(sql_id(guild))
                .bind(author)
                .execute(&mut *tx)
                .await?;
        }
        sqlx::query(
            "INSERT INTO confession_moderation(guild_id,confession_id,moderator_id,banned) VALUES($1,$2,$3,$4)",
        )
        .bind(sql_id(guild))
        .bind(confession)
        .bind(sql_id(moderator))
        .bind(banned)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(true)
    }

    pub async fn report_confession(&self, guild: u64, confession: i64, reporter: u64) -> anyhow::Result<bool> {
        let mut tx = self.pool().begin().await?;
        lock_users(&mut tx, &[sql_id(reporter)]).await?;
        let n = sqlx::query("INSERT INTO confession_reports(confession_id,reporter_id) SELECT id,$3 FROM confessions WHERE guild_id=$1 AND id=$2 AND status='approved' ON CONFLICT DO NOTHING")
            .bind(sql_id(guild)).bind(confession).bind(sql_id(reporter)).execute(&mut *tx).await?.rows_affected();
        tx.commit().await?;
        Ok(n > 0)
    }

    /// Persistent hourly and daily quotas survive Redis eviction/restarts.
    pub async fn submit_confession(&self, guild: u64, user: u64, body: &str) -> anyhow::Result<Option<i64>> {
        let mut tx = self.pool().begin().await?;
        lock_users(&mut tx, &[sql_id(user)]).await?;
        let (blocked,): (bool,) = sqlx::query_as("SELECT EXISTS(SELECT 1 FROM confession_bans WHERE guild_id=$1 AND user_id=$2) OR EXISTS(SELECT 1 FROM confessions WHERE guild_id=$1 AND author_id=$2 AND created_at>now()-interval '1 hour') OR (SELECT count(*) FROM confessions WHERE guild_id=$1 AND author_id=$2 AND created_at>now()-interval '1 day')>=3")
            .bind(sql_id(guild)).bind(sql_id(user)).fetch_one(&mut *tx).await?;
        if blocked {
            return Ok(None);
        }
        let (id,): (i64,) =
            sqlx::query_as("INSERT INTO confessions(guild_id,author_id,body) VALUES($1,$2,$3) RETURNING id")
                .bind(sql_id(guild))
                .bind(sql_id(user))
                .bind(body)
                .fetch_one(&mut *tx)
                .await?;
        tx.commit().await?;
        Ok(Some(id))
    }
}
