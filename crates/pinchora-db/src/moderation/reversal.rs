//! Database operations used while reversing a numbered case.

use pinchora_core::moderation::moderation::CaseKind;
use sqlx::{Postgres, Transaction};

use super::CaseRow;
use super::{NewStaffAction, record_staff_in};
use crate::{Db, NewCase, OpenedCase, sql_id};

pub(super) async fn open_case_in(
    tx: &mut sqlx::Transaction<'_, Postgres>,
    new: &NewCase<'_>,
    source: &str,
) -> anyhow::Result<OpenedCase> {
    let guild = sql_id(new.guild);
    let (number,): (i32,) = sqlx::query_as(
        "INSERT INTO mod_case_counters (guild_id, last_case)
         SELECT $1, COALESCE(MAX(number), 0) + 1 FROM mod_cases WHERE guild_id = $1
         ON CONFLICT (guild_id) DO UPDATE SET last_case = mod_case_counters.last_case + 1
         RETURNING last_case",
    )
    .bind(guild)
    .fetch_one(&mut **tx)
    .await?;
    // What a new ban, unban, timeout or removed timeout ends.
    let ends: &[&str] = match new.kind {
        CaseKind::Ban | CaseKind::Unban => &["ban"],
        CaseKind::Timeout | CaseKind::Untimeout => &["timeout"],
        _ => &[],
    };
    if !ends.is_empty() {
        sqlx::query(
            "UPDATE mod_cases SET active = FALSE, updated_at = now()
             WHERE guild_id = $1 AND target_id = $2 AND active AND kind = ANY($3)",
        )
        .bind(guild)
        .bind(sql_id(new.target))
        .bind(ends)
        .execute(&mut **tx)
        .await?;
    }
    let case = sqlx::query_as::<_, CaseRow>(concat!(
        "INSERT INTO mod_cases (guild_id, number, kind, target_id, target_name, moderator_id, moderator_name, reason,
                                duration_secs, expires_at, details, active)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12) RETURNING ",
        case_columns!()
    ))
    .bind(guild)
    .bind(number)
    .bind(new.kind.key())
    .bind(sql_id(new.target))
    .bind(new.target_name)
    .bind(sql_id(new.moderator))
    .bind(new.moderator_name)
    .bind(new.reason)
    .bind(new.duration.map(|d| d.num_seconds()))
    .bind(new.expires_at)
    .bind(new.details)
    // Only bans, warns and timeouts stand until something ends them.
    .bind(matches!(new.kind, CaseKind::Ban | CaseKind::Warn | CaseKind::Timeout))
    .fetch_one(&mut **tx)
    .await?;
    let (active_warns,): (i64,) = sqlx::query_as(
        "SELECT count(*) FROM mod_cases WHERE guild_id = $1 AND target_id = $2 AND kind = 'warn' AND active",
    )
    .bind(guild)
    .bind(sql_id(new.target))
    .fetch_one(&mut **tx)
    .await?;
    record_staff_in(
        tx,
        &NewStaffAction {
            guild: new.guild,
            actor: new.moderator,
            actor_name: new.moderator_name,
            source,
            action: new.kind.key(),
            target: Some(new.target),
            target_name: Some(new.target_name),
            case_number: Some(number),
            reason: new.reason,
            details: new.details,
        },
    )
    .await?;
    Ok(OpenedCase { case, active_warns })
}

impl Db {
    pub async fn lock_case(&self, guild: u64, number: i32) -> anyhow::Result<sqlx::Transaction<'_, Postgres>> {
        let mut tx = self.pool().begin().await?;
        Self::lock_case_in(&mut tx, guild, number).await?;
        Ok(tx)
    }

    pub async fn lock_case_in(tx: &mut Transaction<'_, Postgres>, guild: u64, number: i32) -> anyhow::Result<()> {
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
            .bind(format!("moderation-case:{guild}:{number}"))
            .execute(&mut **tx)
            .await?;
        Ok(())
    }

    pub async fn lock_reversal_member_in(
        tx: &mut Transaction<'_, Postgres>,
        guild: u64,
        target: u64,
        kind: CaseKind,
    ) -> anyhow::Result<()> {
        let category = if kind == CaseKind::Ban { "ban" } else { "timeout" };
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
            .bind(format!("moderation-{category}:{guild}:{target}"))
            .execute(&mut **tx)
            .await?;
        Ok(())
    }

    pub async fn lock_timeout(&self, guild: u64, target: u64) -> anyhow::Result<Transaction<'_, Postgres>> {
        let mut tx = self.pool().begin().await?;
        Self::lock_reversal_member_in(&mut tx, guild, target, CaseKind::Timeout).await?;
        Ok(tx)
    }

    pub async fn case_in(
        tx: &mut Transaction<'_, Postgres>,
        guild: u64,
        number: i32,
    ) -> anyhow::Result<Option<CaseRow>> {
        Ok(sqlx::query_as::<_, CaseRow>(concat!(
            "SELECT ",
            case_columns!(),
            " FROM mod_cases WHERE guild_id = $1 AND number = $2"
        ))
        .bind(sql_id(guild))
        .bind(number)
        .fetch_optional(&mut **tx)
        .await?)
    }

    pub async fn active_case_for(&self, guild: u64, target: u64, kind: CaseKind) -> anyhow::Result<Option<CaseRow>> {
        Ok(sqlx::query_as::<_, CaseRow>(concat!("SELECT ", case_columns!(),
            " FROM mod_cases WHERE guild_id = $1 AND target_id = $2 AND kind = $3 AND active ORDER BY number DESC LIMIT 1"))
            .bind(sql_id(guild)).bind(sql_id(target)).bind(kind.key()).fetch_optional(self.pool()).await?)
    }

    pub async fn active_case_for_in(
        tx: &mut Transaction<'_, Postgres>,
        guild: u64,
        target: u64,
        kind: CaseKind,
    ) -> anyhow::Result<Option<CaseRow>> {
        Ok(sqlx::query_as::<_, CaseRow>(concat!("SELECT ", case_columns!(),
            " FROM mod_cases WHERE guild_id = $1 AND target_id = $2 AND kind = $3 AND active ORDER BY number DESC LIMIT 1"))
            .bind(sql_id(guild)).bind(sql_id(target)).bind(kind.key()).fetch_optional(&mut **tx).await?)
    }

    pub async fn close_reversal_in(tx: &mut Transaction<'_, Postgres>, case: &CaseRow) -> anyhow::Result<bool> {
        let kind = case.kind().ok_or_else(|| anyhow::anyhow!("unknown case kind"))?;
        let closed = if kind == CaseKind::Ban {
            sqlx::query(
                "UPDATE mod_cases SET active = FALSE, updated_at = now()
                WHERE guild_id = $1 AND number = $2 AND created_at = $3 AND kind = 'ban' AND active
                  AND NOT EXISTS (SELECT 1 FROM mod_cases newer WHERE newer.guild_id = $1
                    AND newer.target_id = $4 AND newer.kind = 'ban' AND newer.active AND newer.number > $2)",
            )
            .bind(case.guild_id)
            .bind(case.number)
            .bind(case.created_at)
            .bind(case.target_id)
            .execute(&mut **tx)
            .await?
            .rows_affected()
        } else {
            sqlx::query(
                "UPDATE mod_cases SET active = FALSE, updated_at = now()
                WHERE guild_id = $1 AND number = $2 AND active AND kind = $3",
            )
            .bind(case.guild_id)
            .bind(case.number)
            .bind(kind.key())
            .execute(&mut **tx)
            .await?
            .rows_affected()
        };
        Ok(closed == 1)
    }

    pub async fn open_reversal_in(
        tx: &mut Transaction<'_, Postgres>,
        new: &NewCase<'_>,
        source: &str,
    ) -> anyhow::Result<OpenedCase> {
        open_case_in(tx, new, source).await
    }

    pub async fn reopen_ban(&self, ban: &CaseRow) -> anyhow::Result<()> {
        sqlx::query("UPDATE mod_cases SET active = TRUE, updated_at = now(), expire_lease = NULL WHERE guild_id = $1 AND number = $2 AND kind = 'ban' AND NOT EXISTS (SELECT 1 FROM mod_cases n WHERE n.guild_id = $1 AND n.target_id = $3 AND n.number > $2 AND n.kind IN ('ban', 'unban'))")
            .bind(ban.guild_id).bind(ban.number).bind(ban.target_id).execute(self.pool()).await?;
        Ok(())
    }
}
