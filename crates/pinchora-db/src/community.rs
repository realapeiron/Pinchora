//! Community pack persistence. Updates that can race are serialized in PostgreSQL.
use crate::{Db, sql_id};
use pinchora_core::community::{CountResult, judge_count};
use sqlx::FromRow;

#[derive(Clone, Debug, FromRow)]
pub struct CountingChannel {
    pub channel_id: i64,
    pub guild_id: i64,
    pub current: i64,
    pub last_user_id: Option<i64>,
    pub record: i64,
    pub reset_on_fail: bool,
    pub allow_math: bool,
    pub fail_role_id: Option<i64>,
}
#[derive(Clone, Debug, FromRow)]
pub struct QotdQuestion {
    pub id: i64,
    pub guild_id: i64,
    pub text: String,
    pub text_tr: Option<String>,
    pub suggested_by: Option<i64>,
    pub approved: bool,
    pub asked_at: Option<chrono::DateTime<chrono::Utc>>,
}
#[derive(Clone, Debug, FromRow)]
pub struct FormRow {
    pub id: i64,
    pub guild_id: i64,
    pub name: String,
    pub questions: serde_json::Value,
    pub submit_channel_id: Option<i64>,
    pub review_roles: Vec<i64>,
    pub accept_role_id: Option<i64>,
    pub dm_result: bool,
    pub open: bool,
    pub message_id: Option<i64>,
}
#[derive(Clone, Debug, FromRow)]
pub struct ModmailSettings {
    pub guild_id: i64,
    pub enabled: bool,
    pub forum_channel_id: Option<i64>,
    pub staff_roles: Vec<i64>,
    pub greeting: String,
    pub close_after_hours: i32,
    pub anonymous_staff: bool,
    pub log_channel_id: Option<i64>,
}
#[derive(Clone, Debug, FromRow)]
pub struct ModmailThread {
    pub id: i64,
    pub guild_id: i64,
    pub user_id: i64,
    pub thread_id: i64,
    pub status: String,
    pub opened_at: chrono::DateTime<chrono::Utc>,
    pub closed_at: Option<chrono::DateTime<chrono::Utc>>,
    pub closed_by: Option<i64>,
}
impl Db {
    pub async fn counting_channel(&self, channel: u64) -> anyhow::Result<Option<CountingChannel>> {
        Ok(sqlx::query_as("SELECT channel_id,guild_id,current,last_user_id,record,reset_on_fail,allow_math,fail_role_id FROM counting_channels WHERE channel_id=$1")
            .bind(sql_id(channel)).fetch_optional(self.pool()).await?)
    }
    pub async fn counting_for_guild(&self, guild: u64) -> anyhow::Result<Option<CountingChannel>> {
        Ok(sqlx::query_as("SELECT channel_id,guild_id,current,last_user_id,record,reset_on_fail,allow_math,fail_role_id FROM counting_channels WHERE guild_id=$1")
            .bind(sql_id(guild)).fetch_optional(self.pool()).await?)
    }
    pub async fn save_counting(
        &self,
        guild: u64,
        channel: u64,
        reset: bool,
        math: bool,
        role: Option<u64>,
    ) -> anyhow::Result<()> {
        sqlx::query("INSERT INTO counting_channels(guild_id,channel_id,reset_on_fail,allow_math,fail_role_id) VALUES($1,$2,$3,$4,$5) ON CONFLICT(guild_id) DO UPDATE SET channel_id=$2,reset_on_fail=$3,allow_math=$4,fail_role_id=$5")
            .bind(sql_id(guild)).bind(sql_id(channel)).bind(reset).bind(math).bind(role.map(sql_id))
            .execute(self.pool()).await?;
        Ok(())
    }
    pub async fn reset_counting(&self, guild: u64) -> anyhow::Result<bool> {
        Ok(
            sqlx::query("UPDATE counting_channels SET current=0,last_user_id=NULL WHERE guild_id=$1")
                .bind(sql_id(guild))
                .execute(self.pool())
                .await?
                .rows_affected()
                > 0,
        )
    }
    pub async fn count_message(
        &self,
        channel: u64,
        user: u64,
        text: &str,
    ) -> anyhow::Result<Option<(CountResult, i64, i64)>> {
        let mut tx = self.pool().begin().await?;
        let Some(row)=sqlx::query_as::<_,CountingChannel>("SELECT channel_id,guild_id,current,last_user_id,record,reset_on_fail,allow_math,fail_role_id FROM counting_channels WHERE channel_id=$1 FOR UPDATE")
            .bind(sql_id(channel)).fetch_optional(&mut *tx).await? else { return Ok(None); };
        let result = judge_count(
            text,
            row.allow_math,
            row.current,
            row.last_user_id.map(|n| n as u64),
            user,
        );
        let next = match result {
            CountResult::Correct => row.current.saturating_add(1),
            CountResult::Wrong | CountResult::SameUser if row.reset_on_fail => 0,
            _ => row.current,
        };
        if result != CountResult::Ignored {
            let last_user = if result == CountResult::Correct {
                Some(sql_id(user))
            } else if next == 0 {
                None
            } else {
                row.last_user_id
            };
            sqlx::query("UPDATE counting_channels SET current=$2,last_user_id=$3,record=GREATEST(record,$2),record_at=CASE WHEN $2>record THEN now() ELSE record_at END WHERE channel_id=$1")
                .bind(sql_id(channel)).bind(next).bind(last_user)
                .execute(&mut *tx).await?;
            sqlx::query("INSERT INTO counting_stats(guild_id,user_id,correct,fails) VALUES($1,$2,$3,$4) ON CONFLICT(guild_id,user_id) DO UPDATE SET correct=counting_stats.correct+$3,fails=counting_stats.fails+$4")
                .bind(row.guild_id).bind(sql_id(user)).bind(i64::from(result==CountResult::Correct))
                .bind(i64::from(result!=CountResult::Correct)).execute(&mut *tx).await?;
        }
        tx.commit().await?;
        Ok(Some((result, next, row.record.max(next))))
    }
    pub async fn counting_stats(&self, guild: u64, user: u64) -> anyhow::Result<(i64, i64)> {
        Ok(
            sqlx::query_as("SELECT correct,fails FROM counting_stats WHERE guild_id=$1 AND user_id=$2")
                .bind(sql_id(guild))
                .bind(sql_id(user))
                .fetch_optional(self.pool())
                .await?
                .unwrap_or((0, 0)),
        )
    }
    pub async fn suggest_qotd(&self, guild: u64, user: u64, text: &str) -> anyhow::Result<Option<i64>> {
        if text.trim().is_empty() || text.chars().count() > 300 {
            return Ok(None);
        }
        let mut tx = self.pool().begin().await?;
        sqlx::query("INSERT INTO qotd_settings(guild_id) VALUES($1) ON CONFLICT DO NOTHING")
            .bind(sql_id(guild))
            .execute(&mut *tx)
            .await?;
        sqlx::query("SELECT guild_id FROM qotd_settings WHERE guild_id=$1 FOR UPDATE")
            .bind(sql_id(guild))
            .fetch_one(&mut *tx)
            .await?;
        let count: i64 = sqlx::query_scalar("SELECT count(*) FROM qotd_questions WHERE guild_id=$1")
            .bind(sql_id(guild))
            .fetch_one(&mut *tx)
            .await?;
        if count >= 500 {
            return Ok(None);
        }
        let id =
            sqlx::query_scalar("INSERT INTO qotd_questions(guild_id,text,suggested_by) VALUES($1,$2,$3) RETURNING id")
                .bind(sql_id(guild))
                .bind(text.trim())
                .bind(sql_id(user))
                .fetch_one(&mut *tx)
                .await?;
        tx.commit().await?;
        Ok(Some(id))
    }
    pub async fn qotd_queue(&self, guild: u64) -> anyhow::Result<Vec<QotdQuestion>> {
        Ok(sqlx::query_as("SELECT id,guild_id,text,text_tr,suggested_by,approved,asked_at FROM qotd_questions WHERE guild_id=$1 ORDER BY asked_at NULLS FIRST,id LIMIT 500")
            .bind(sql_id(guild)).fetch_all(self.pool()).await?)
    }
    pub async fn seed_qotd(&self, guild: u64) -> anyhow::Result<usize> {
        let mut tx = self.pool().begin().await?;
        sqlx::query("INSERT INTO qotd_settings(guild_id) VALUES($1) ON CONFLICT DO NOTHING")
            .bind(sql_id(guild))
            .execute(&mut *tx)
            .await?;
        sqlx::query("SELECT guild_id FROM qotd_settings WHERE guild_id=$1 FOR UPDATE")
            .bind(sql_id(guild))
            .fetch_one(&mut *tx)
            .await?;
        let count: i64 =
            sqlx::query_scalar("SELECT count(*) FROM qotd_questions WHERE guild_id=$1 AND suggested_by IS NULL")
                .bind(sql_id(guild))
                .fetch_one(&mut *tx)
                .await?;
        if count > 0 {
            return Ok(0);
        }
        let total: i64 = sqlx::query_scalar("SELECT count(*) FROM qotd_questions WHERE guild_id=$1")
            .bind(sql_id(guild))
            .fetch_one(&mut *tx)
            .await?;
        let available = (500 - total).max(0) as usize;
        let pack: Vec<serde_json::Value> = serde_json::from_str(include_str!("../../../content/qotd.json"))?;
        for row in pack.iter().take(available) {
            sqlx::query("INSERT INTO qotd_questions(guild_id,text,text_tr,approved) VALUES($1,$2,$3,TRUE)")
                .bind(sql_id(guild))
                .bind(row["en"].as_str())
                .bind(row["tr"].as_str())
                .execute(&mut *tx)
                .await?;
        }
        tx.commit().await?;
        Ok(pack.len().min(available))
    }
    pub async fn decide_qotd(&self, guild: u64, id: i64, approve: bool) -> anyhow::Result<bool> {
        let sql = if approve {
            "UPDATE qotd_questions SET approved=TRUE WHERE guild_id=$1 AND id=$2 AND NOT approved"
        } else {
            "DELETE FROM qotd_questions WHERE guild_id=$1 AND id=$2 AND NOT approved"
        };
        Ok(sqlx::query(sql)
            .bind(sql_id(guild))
            .bind(id)
            .execute(self.pool())
            .await?
            .rows_affected()
            > 0)
    }
    pub async fn next_qotd(&self, guild: u64) -> anyhow::Result<Option<QotdQuestion>> {
        let mut tx = self.pool().begin().await?;
        sqlx::query("INSERT INTO qotd_settings(guild_id) VALUES($1) ON CONFLICT DO NOTHING")
            .bind(sql_id(guild))
            .execute(&mut *tx)
            .await?;
        sqlx::query("SELECT guild_id FROM qotd_settings WHERE guild_id=$1 FOR UPDATE")
            .bind(sql_id(guild))
            .fetch_one(&mut *tx)
            .await?;
        let next = next_question(&mut tx, guild).await?;
        tx.commit().await?;
        Ok(next)
    }
    /// Claims a local calendar day and consumes its question in one durable transaction.
    pub async fn claim_qotd(&self, guild: u64, day: chrono::NaiveDate) -> anyhow::Result<Option<QotdQuestion>> {
        let mut tx = self.pool().begin().await?;
        let claimed = sqlx::query("UPDATE qotd_settings SET last_posted_on=$2 WHERE guild_id=$1 AND (last_posted_on IS NULL OR last_posted_on<$2)")
            .bind(sql_id(guild)).bind(day).execute(&mut *tx).await?.rows_affected() > 0;
        if !claimed {
            return Ok(None);
        }
        let next = next_question(&mut tx, guild).await?;
        if next.is_some() {
            tx.commit().await?;
        }
        Ok(next)
    }
    pub async fn save_form(&self, form: &FormRow) -> anyhow::Result<Option<i64>> {
        if !pinchora_core::community::valid_form(&form.name, &form.questions) {
            return Ok(None);
        }
        let mut tx = self.pool().begin().await?;
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,0))")
            .bind(format!("community:forms:{}", form.guild_id))
            .execute(&mut *tx)
            .await?;
        let id = if form.id == 0 {
            sqlx::query_scalar("INSERT INTO forms(guild_id,name,questions,submit_channel_id,review_roles,accept_role_id,dm_result,open) SELECT $1,$2,$3,$4,$5,$6,$7,$8 WHERE (SELECT count(*) FROM forms WHERE guild_id=$1)<10 RETURNING id")
                .bind(form.guild_id).bind(&form.name).bind(&form.questions).bind(form.submit_channel_id)
                .bind(&form.review_roles).bind(form.accept_role_id).bind(form.dm_result).bind(form.open)
                .fetch_optional(&mut *tx).await?
        } else {
            sqlx::query_scalar("UPDATE forms SET name=$3,questions=$4,submit_channel_id=$5,review_roles=$6,accept_role_id=$7,dm_result=$8,open=$9 WHERE guild_id=$1 AND id=$2 RETURNING id")
                .bind(form.guild_id).bind(form.id).bind(&form.name).bind(&form.questions).bind(form.submit_channel_id)
                .bind(&form.review_roles).bind(form.accept_role_id).bind(form.dm_result).bind(form.open)
                .fetch_optional(&mut *tx).await?
        };
        tx.commit().await?;
        Ok(id)
    }
    pub async fn form(&self, guild: u64, id: i64) -> anyhow::Result<Option<FormRow>> {
        Ok(sqlx::query_as("SELECT * FROM forms WHERE guild_id=$1 AND id=$2")
            .bind(sql_id(guild))
            .bind(id)
            .fetch_optional(self.pool())
            .await?)
    }
    pub async fn forms(&self, guild: u64) -> anyhow::Result<Vec<FormRow>> {
        Ok(sqlx::query_as("SELECT * FROM forms WHERE guild_id=$1 ORDER BY id")
            .bind(sql_id(guild))
            .fetch_all(self.pool())
            .await?)
    }
    pub async fn submit_form(
        &self,
        guild: u64,
        id: i64,
        user: u64,
        answers: &serde_json::Value,
    ) -> anyhow::Result<Option<i64>> {
        if answers.as_array().is_none_or(|a| {
            a.is_empty()
                || a.len() > 5
                || a.iter().any(|value| {
                    value
                        .as_str()
                        .is_none_or(|text| text.trim().is_empty() || text.chars().count() > 1000)
                })
        }) {
            return Ok(None);
        }
        let mut tx = self.pool().begin().await?;
        // Lock the form so close/edit and simultaneous submissions cannot race this validation.
        let form: Option<FormRow> = sqlx::query_as("SELECT * FROM forms WHERE guild_id=$1 AND id=$2 FOR UPDATE")
            .bind(sql_id(guild))
            .bind(id)
            .fetch_optional(&mut *tx)
            .await?;
        let Some(form) = form.filter(|f| f.open && pinchora_core::community::valid_form(&f.name, &f.questions)) else {
            return Ok(None);
        };
        if form.questions.as_array().map(Vec::len) != answers.as_array().map(Vec::len) {
            return Ok(None);
        }
        let submission = sqlx::query_scalar("INSERT INTO form_submissions(form_id,user_id,answers) SELECT $1,$2,$3 WHERE NOT EXISTS(SELECT 1 FROM form_submissions WHERE form_id=$1 AND user_id=$2 AND (status='pending' OR created_at>now()-INTERVAL '1 hour')) RETURNING id")
            .bind(id).bind(sql_id(user)).bind(answers).fetch_optional(&mut *tx).await?;
        tx.commit().await?;
        Ok(submission)
    }
    pub async fn decide_form(
        &self,
        guild: u64,
        id: i64,
        reviewer: u64,
        accept: bool,
    ) -> anyhow::Result<Option<(i64, i64, Option<i64>, bool)>> {
        use sqlx::Row;
        let row=sqlx::query("UPDATE form_submissions s SET status=$3,decided_by=$2 FROM forms f WHERE s.id=$1 AND s.status='pending' AND f.id=s.form_id AND f.guild_id=$4 RETURNING s.user_id,f.guild_id,f.accept_role_id,f.dm_result")
            .bind(id).bind(sql_id(reviewer)).bind(if accept{"accepted"}else{"denied"}).bind(sql_id(guild)).fetch_optional(self.pool()).await?;
        Ok(row.map(|r| {
            (
                r.get("user_id"),
                r.get("guild_id"),
                r.get("accept_role_id"),
                r.get("dm_result"),
            )
        }))
    }
    pub async fn modmail_settings(&self, guild: u64) -> anyhow::Result<Option<ModmailSettings>> {
        Ok(sqlx::query_as("SELECT * FROM modmail_settings WHERE guild_id=$1")
            .bind(sql_id(guild))
            .fetch_optional(self.pool())
            .await?)
    }
    pub async fn modmail_enabled_guilds(&self) -> anyhow::Result<Vec<i64>> {
        Ok(
            sqlx::query_scalar("SELECT guild_id FROM modmail_settings WHERE enabled AND forum_channel_id IS NOT NULL")
                .fetch_all(self.pool())
                .await?,
        )
    }
    pub async fn modmail_open(&self, guild: u64, user: u64) -> anyhow::Result<Option<ModmailThread>> {
        Ok(
            sqlx::query_as("SELECT * FROM modmail_threads WHERE guild_id=$1 AND user_id=$2 AND status='open'")
                .bind(sql_id(guild))
                .bind(sql_id(user))
                .fetch_optional(self.pool())
                .await?,
        )
    }
    pub async fn modmail_thread(&self, thread: u64) -> anyhow::Result<Option<ModmailThread>> {
        Ok(
            sqlx::query_as("SELECT * FROM modmail_threads WHERE thread_id=$1 AND status='open'")
                .bind(sql_id(thread))
                .fetch_optional(self.pool())
                .await?,
        )
    }
    pub async fn modmail_blocked(&self, guild: u64, user: u64) -> anyhow::Result<bool> {
        Ok(
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM modmail_blocks WHERE guild_id=$1 AND user_id=$2)")
                .bind(sql_id(guild))
                .bind(sql_id(user))
                .fetch_one(self.pool())
                .await?,
        )
    }
    pub async fn modmail_create(&self, guild: u64, user: u64, thread: u64) -> anyhow::Result<i64> {
        Ok(
            sqlx::query_scalar("INSERT INTO modmail_threads(guild_id,user_id,thread_id) VALUES($1,$2,$3) RETURNING id")
                .bind(sql_id(guild))
                .bind(sql_id(user))
                .bind(sql_id(thread))
                .fetch_one(self.pool())
                .await?,
        )
    }
    pub async fn modmail_close(&self, guild: u64, thread: u64, actor: u64) -> anyhow::Result<bool> {
        Ok(sqlx::query("UPDATE modmail_threads SET status='closed',closed_at=now(),closed_by=$3 WHERE guild_id=$1 AND thread_id=$2 AND status='open'")
            .bind(sql_id(guild)).bind(sql_id(thread)).bind(sql_id(actor)).execute(self.pool()).await?.rows_affected()>0)
    }
}

async fn next_question(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    guild: u64,
) -> anyhow::Result<Option<QotdQuestion>> {
    let query = "SELECT id,guild_id,text,text_tr,suggested_by,approved,asked_at FROM qotd_questions WHERE guild_id=$1 AND approved AND asked_at IS NULL ORDER BY (suggested_by IS NULL),id LIMIT 1";
    let mut next: Option<QotdQuestion> = sqlx::query_as(query)
        .bind(sql_id(guild))
        .fetch_optional(&mut **tx)
        .await?;
    if next.is_none() {
        sqlx::query("UPDATE qotd_questions SET asked_at=NULL WHERE guild_id=$1 AND approved")
            .bind(sql_id(guild))
            .execute(&mut **tx)
            .await?;
        next = sqlx::query_as(query)
            .bind(sql_id(guild))
            .fetch_optional(&mut **tx)
            .await?;
    }
    if let Some(ref question) = next {
        sqlx::query("UPDATE qotd_questions SET asked_at=now() WHERE id=$1")
            .bind(question.id)
            .execute(&mut **tx)
            .await?;
    }
    Ok(next)
}
