//! Server XP edits and MEE6 imports. All member writes take player locks first.

use crate::Db;
use crate::infra::locks::lock_users;
use crate::infra::locks::retry;
use crate::sql_id;

#[derive(Debug, Clone, Copy)]
pub enum XpEdit {
    Add,
    Remove,
    Set,
}

impl Db {
    /// Members whose reward roles must be reconciled after a bulk reset.
    pub async fn xp_member_ids(&self, guild: u64) -> anyhow::Result<Vec<u64>> {
        let ids: Vec<i64> = sqlx::query_scalar("SELECT user_id FROM members WHERE guild_id = $1 ORDER BY user_id")
            .bind(sql_id(guild))
            .fetch_all(self.pool())
            .await?;
        Ok(ids.into_iter().map(|id| id as u64).collect())
    }

    /// Edit one member's server XP without touching global XP or earned rewards.
    pub async fn edit_member_xp(&self, guild: u64, user: u64, edit: XpEdit, amount: i64) -> anyhow::Result<(i64, i64)> {
        anyhow::ensure!((0..=1_000_000_000).contains(&amount), "XP amount out of range");
        anyhow::ensure!(
            guild > 0 && user > 0 && guild <= i64::MAX as u64 && user <= i64::MAX as u64,
            "invalid Discord id"
        );
        retry("edit_member_xp", || async move {
            let mut tx = self.pool().begin().await?;
            lock_users(&mut tx, &[sql_id(user)]).await?;
            let old: Option<i64> = sqlx::query_scalar("SELECT xp FROM members WHERE guild_id = $1 AND user_id = $2")
                .bind(sql_id(guild))
                .bind(sql_id(user))
                .fetch_optional(&mut *tx)
                .await?;
            let old = old.unwrap_or(0).max(0);
            let new = match edit {
                XpEdit::Add => old.saturating_add(amount),
                XpEdit::Remove => old.saturating_sub(amount).max(0),
                XpEdit::Set => amount,
            };
            sqlx::query(
                "INSERT INTO members (guild_id, user_id, xp) VALUES ($1, $2, $3)
                ON CONFLICT (guild_id, user_id) DO UPDATE SET xp = EXCLUDED.xp",
            )
            .bind(sql_id(guild))
            .bind(sql_id(user))
            .bind(new)
            .execute(&mut *tx)
            .await?;
            tx.commit().await?;
            Ok((old, new))
        })
        .await
    }

    /// Reset members in bounded batches, taking every player's lock before their rows.
    pub async fn reset_all_member_xp(&self, guild: u64) -> anyhow::Result<u64> {
        let mut changed = 0;
        let mut cursor = 0i64;
        loop {
            let users: Vec<i64> = sqlx::query_scalar(
                "SELECT user_id FROM members WHERE guild_id = $1 AND xp > 0 AND user_id > $2 ORDER BY user_id LIMIT 100",
            )
            .bind(sql_id(guild))
            .bind(cursor)
            .fetch_all(self.pool())
            .await?;
            if users.is_empty() {
                break;
            }
            let count = retry("reset_all_member_xp", || {
                let users = users.clone();
                async move {
                    let mut tx = self.pool().begin().await?;
                    lock_users(&mut tx, &users).await?;
                    let n = sqlx::query("UPDATE members SET xp = 0 WHERE guild_id = $1 AND user_id = ANY($2)")
                        .bind(sql_id(guild))
                        .bind(&users)
                        .execute(&mut *tx)
                        .await?
                        .rows_affected();
                    tx.commit().await?;
                    Ok(n)
                }
            })
            .await?;
            cursor = *users.last().expect("nonempty batch");
            changed += count;
        }
        Ok(changed)
    }

    /// Apply one MEE6 page. Repeating a page in replace mode has the same result.
    pub async fn import_member_xp_page(
        &self,
        guild: u64,
        players: &[(u64, i64)],
        replace: bool,
    ) -> anyhow::Result<u64> {
        if players.is_empty() {
            return Ok(0);
        }
        anyhow::ensure!(
            players.len() <= 1000
                && players
                    .iter()
                    .all(|(user, xp)| *user > 0 && *user <= i64::MAX as u64 && *xp >= 0),
            "invalid import page"
        );
        let ids: Vec<i64> = players.iter().map(|(id, _)| sql_id(*id)).collect();
        let xp: Vec<i64> = players.iter().map(|(_, xp)| *xp).collect();
        retry("import_member_xp_page", || {
            let ids = ids.clone();
            let xp = xp.clone();
            async move {
                let mut tx = self.pool().begin().await?;
                lock_users(&mut tx, &ids).await?;
                let n = sqlx::query(
                    "INSERT INTO members (guild_id, user_id, xp)
                    SELECT $1, p.user_id, p.xp FROM UNNEST($2::BIGINT[], $3::BIGINT[]) p(user_id, xp)
                    ON CONFLICT (guild_id, user_id) DO UPDATE
                    SET xp = CASE WHEN $4 THEN EXCLUDED.xp ELSE GREATEST(members.xp, EXCLUDED.xp) END
                    WHERE CASE WHEN $4 THEN members.xp <> EXCLUDED.xp ELSE members.xp < EXCLUDED.xp END",
                )
                .bind(sql_id(guild))
                .bind(&ids)
                .bind(&xp)
                .bind(replace)
                .execute(&mut *tx)
                .await?
                .rows_affected();
                tx.commit().await?;
                Ok(n)
            }
        })
        .await
    }

    /// Last import attempt, from the existing audit table.
    pub async fn last_mee6_import(&self, guild: u64) -> anyhow::Result<Option<chrono::DateTime<chrono::Utc>>> {
        Ok(sqlx::query_scalar("SELECT created_at FROM dashboard_audit WHERE guild_id = $1 AND action IN ('xp.import.mee6', 'xp.import.mee6.started') ORDER BY id DESC LIMIT 1")
            .bind(sql_id(guild)).fetch_optional(self.pool()).await?)
    }
}
