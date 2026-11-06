//! The roles pack's tables: auto roles, sticky roles, roles on a timer, and colour roles. Role panels
//! live in `panels.rs`.

use chrono::{DateTime, Utc};

use crate::{Db, sql_id};

/// An auto role as stored: who gets it, how long it waits, how long it lasts (0: at once, for good).
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct AutoRoleRow {
    pub id: i64,
    pub role_id: i64,
    pub kind: String,
    pub delay_secs: i32,
    pub remove_after_secs: i32,
    pub enabled: bool,
}

/// An auto role to save.
#[derive(Debug, Clone, Copy)]
pub struct NewAutoRole<'a> {
    pub role_id: u64,
    pub kind: &'a str,
    pub delay_secs: i32,
    pub remove_after_secs: i32,
    pub enabled: bool,
}

/// What the bot last saw a member hold.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct StickyRow {
    pub role_ids: Vec<i64>,
    pub left_at: Option<DateTime<Utc>>,
}

/// A role waiting to be taken away.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct TimedRoleRow {
    pub id: i64,
    pub guild_id: i64,
    pub user_id: i64,
    pub role_id: i64,
}

/// An auto role waiting out its delay.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct DelayedRoleRow {
    pub id: i64,
    pub guild_id: i64,
    pub user_id: i64,
    pub role_id: i64,
    pub remove_after_secs: i32,
}

/// A server's colour role settings.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct ColorSettings {
    pub enabled: bool,
    pub allowed_roles: Vec<i64>,
    /// `palette` or `hex`.
    pub mode: String,
    pub anchor_role_id: Option<i64>,
}

impl Default for ColorSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            allowed_roles: Vec::new(),
            mode: "palette".to_owned(),
            anchor_role_id: None,
        }
    }
}

/// A role that is a colour: `hex` is `#rrggbb`.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct ColorRoleRow {
    pub role_id: i64,
    pub hex: String,
    pub name: String,
}

/// How adding a colour role ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorAdded {
    Added,
    /// The role was a colour role already (its name and colour are now what was given).
    Updated,
    /// The server has as many colour roles as it can.
    Full,
    /// Another role of the server already has this colour.
    HexTaken,
}

impl Db {
    // /Auto roles

    pub async fn auto_roles(&self, guild_id: u64) -> anyhow::Result<Vec<AutoRoleRow>> {
        Ok(sqlx::query_as::<_, AutoRoleRow>(
            "SELECT id, role_id, kind, delay_secs, remove_after_secs, enabled
             FROM auto_roles WHERE guild_id = $1 ORDER BY id",
        )
        .bind(sql_id(guild_id))
        .fetch_all(self.pool())
        .await?)
    }

    /// The auto roles of one kind that are switched on.
    pub async fn auto_roles_of_kind(&self, guild_id: u64, kind: &str) -> anyhow::Result<Vec<AutoRoleRow>> {
        Ok(sqlx::query_as::<_, AutoRoleRow>(
            "SELECT id, role_id, kind, delay_secs, remove_after_secs, enabled
             FROM auto_roles WHERE guild_id = $1 AND kind = $2 AND enabled ORDER BY id",
        )
        .bind(sql_id(guild_id))
        .bind(kind)
        .fetch_all(self.pool())
        .await?)
    }

    /// Adds an auto role, or changes the delay, the expiry and the switch of the one this role and kind
    /// already are. `Ok(false)` when the server has `max` already and this is a new one. The server's
    /// rows are locked while the count is checked.
    pub async fn save_auto_role(&self, guild_id: u64, auto: &NewAutoRole<'_>, max: i64) -> anyhow::Result<bool> {
        let mut tx = self.pool().begin().await?;
        sqlx::query("INSERT INTO guilds (id) VALUES ($1) ON CONFLICT (id) DO NOTHING")
            .bind(sql_id(guild_id))
            .execute(&mut *tx)
            .await?;
        sqlx::query("SELECT id FROM guilds WHERE id = $1 FOR UPDATE")
            .bind(sql_id(guild_id))
            .fetch_one(&mut *tx)
            .await?;
        let saved = sqlx::query(
            "INSERT INTO auto_roles (guild_id, role_id, kind, delay_secs, remove_after_secs, enabled)
             SELECT $1, $2, $3, $4, $5, $6
             WHERE (SELECT count(*) FROM auto_roles WHERE guild_id = $1) < $7
                OR EXISTS (SELECT 1 FROM auto_roles WHERE guild_id = $1 AND role_id = $2 AND kind = $3)
             ON CONFLICT (guild_id, role_id, kind)
             DO UPDATE SET delay_secs = EXCLUDED.delay_secs, remove_after_secs = EXCLUDED.remove_after_secs,
                           enabled = EXCLUDED.enabled",
        )
        .bind(sql_id(guild_id))
        .bind(sql_id(auto.role_id))
        .bind(auto.kind)
        .bind(auto.delay_secs)
        .bind(auto.remove_after_secs)
        .bind(auto.enabled)
        .bind(max)
        .execute(&mut *tx)
        .await?
        .rows_affected();
        tx.commit().await?;
        Ok(saved > 0)
    }

    /// Takes an auto role away: of one kind, or of every kind with `None`. Returns how many went.
    pub async fn remove_auto_role(&self, guild_id: u64, role_id: u64, kind: Option<&str>) -> anyhow::Result<u64> {
        Ok(sqlx::query(
            "DELETE FROM auto_roles WHERE guild_id = $1 AND role_id = $2 AND ($3::TEXT IS NULL OR kind = $3)",
        )
        .bind(sql_id(guild_id))
        .bind(sql_id(role_id))
        .bind(kind)
        .execute(self.pool())
        .await?
        .rows_affected())
    }

    // /Sticky roles

    pub async fn sticky_enabled(&self, guild_id: u64) -> anyhow::Result<bool> {
        let on: Option<(bool,)> = sqlx::query_as("SELECT sticky_roles FROM guilds WHERE id = $1")
            .bind(sql_id(guild_id))
            .fetch_optional(self.pool())
            .await?;
        Ok(on.is_some_and(|(on,)| on))
    }

    pub async fn set_sticky(&self, guild_id: u64, on: bool) -> anyhow::Result<()> {
        sqlx::query(
            "INSERT INTO guilds (id, sticky_roles) VALUES ($1, $2)
             ON CONFLICT (id) DO UPDATE SET sticky_roles = EXCLUDED.sticky_roles",
        )
        .bind(sql_id(guild_id))
        .bind(on)
        .execute(self.pool())
        .await?;
        if !on {
            // Nothing is kept for a feature that is off.
            sqlx::query("DELETE FROM sticky_roles WHERE guild_id = $1")
                .bind(sql_id(guild_id))
                .execute(self.pool())
                .await?;
        }
        Ok(())
    }

    /// Notes what a member holds now (a member still here: `left_at` stays empty). No roles means there
    /// is nothing to keep, so the row goes.
    pub async fn remember_roles(&self, guild_id: u64, user_id: u64, roles: &[u64]) -> anyhow::Result<()> {
        if roles.is_empty() {
            sqlx::query("DELETE FROM sticky_roles WHERE guild_id = $1 AND user_id = $2 AND left_at IS NULL")
                .bind(sql_id(guild_id))
                .bind(sql_id(user_id))
                .execute(self.pool())
                .await?;
            return Ok(());
        }
        let ids: Vec<i64> = roles.iter().map(|r| sql_id(*r)).collect();
        sqlx::query(
            "INSERT INTO sticky_roles (guild_id, user_id, role_ids) VALUES ($1, $2, $3)
             ON CONFLICT (guild_id, user_id) DO UPDATE SET role_ids = EXCLUDED.role_ids, left_at = NULL",
        )
        .bind(sql_id(guild_id))
        .bind(sql_id(user_id))
        .bind(ids)
        .execute(self.pool())
        .await?;
        Ok(())
    }

    /// Marks that a member left, so what they held waits for them.
    pub async fn note_left(&self, guild_id: u64, user_id: u64, now: DateTime<Utc>) -> anyhow::Result<()> {
        sqlx::query("UPDATE sticky_roles SET left_at = $3 WHERE guild_id = $1 AND user_id = $2")
            .bind(sql_id(guild_id))
            .bind(sql_id(user_id))
            .bind(now)
            .execute(self.pool())
            .await?;
        Ok(())
    }

    /// Whether something is kept for a member who left: what they held when they were last seen.
    pub async fn sticky_waiting(&self, guild_id: u64, user_id: u64) -> anyhow::Result<bool> {
        let (waiting,): (bool,) = sqlx::query_as(
            "SELECT EXISTS (SELECT 1 FROM sticky_roles WHERE guild_id = $1 AND user_id = $2 AND left_at IS NOT NULL)",
        )
        .bind(sql_id(guild_id))
        .bind(sql_id(user_id))
        .fetch_one(self.pool())
        .await?;
        Ok(waiting)
    }

    /// What a member held when they left, taken out of the table; `None` when they left with nothing kept
    /// or were never seen leaving.
    pub async fn take_sticky(&self, guild_id: u64, user_id: u64) -> anyhow::Result<Option<StickyRow>> {
        Ok(sqlx::query_as::<_, StickyRow>(
            "DELETE FROM sticky_roles WHERE guild_id = $1 AND user_id = $2 AND left_at IS NOT NULL
             RETURNING role_ids, left_at",
        )
        .bind(sql_id(guild_id))
        .bind(sql_id(user_id))
        .fetch_optional(self.pool())
        .await?)
    }

    /// Forgets members who left before `before`.
    pub async fn prune_sticky(&self, before: DateTime<Utc>) -> anyhow::Result<u64> {
        Ok(
            sqlx::query("DELETE FROM sticky_roles WHERE left_at IS NOT NULL AND left_at < $1")
                .bind(before)
                .execute(self.pool())
                .await?
                .rows_affected(),
        )
    }

    // /Roles on a timer

    /// Takes `role_id` from the member at `remove_at` (a later call moves the time).
    pub async fn schedule_removal(
        &self,
        guild_id: u64,
        user_id: u64,
        role_id: u64,
        remove_at: DateTime<Utc>,
    ) -> anyhow::Result<()> {
        sqlx::query(
            "INSERT INTO timed_roles (guild_id, user_id, role_id, remove_at) VALUES ($1, $2, $3, $4)
             ON CONFLICT (guild_id, user_id, role_id) DO UPDATE SET remove_at = EXCLUDED.remove_at, lease_until = NULL",
        )
        .bind(sql_id(guild_id))
        .bind(sql_id(user_id))
        .bind(sql_id(role_id))
        .bind(remove_at)
        .execute(self.pool())
        .await?;
        Ok(())
    }

    /// Cancels a timed removal (the member took the role off themselves, or it was given for good).
    pub async fn cancel_removal(&self, guild_id: u64, user_id: u64, role_id: u64) -> anyhow::Result<()> {
        sqlx::query("DELETE FROM timed_roles WHERE guild_id = $1 AND user_id = $2 AND role_id = $3")
            .bind(sql_id(guild_id))
            .bind(sql_id(user_id))
            .bind(sql_id(role_id))
            .execute(self.pool())
            .await?;
        Ok(())
    }

    /// The roles of one member whose time is up, which are no longer theirs to have.
    pub async fn lapsed_roles(&self, guild_id: u64, user_id: u64, now: DateTime<Utc>) -> anyhow::Result<Vec<i64>> {
        let rows: Vec<(i64,)> =
            sqlx::query_as("SELECT role_id FROM timed_roles WHERE guild_id = $1 AND user_id = $2 AND remove_at <= $3")
                .bind(sql_id(guild_id))
                .bind(sql_id(user_id))
                .bind(now)
                .fetch_all(self.pool())
                .await?;
        Ok(rows.into_iter().map(|(r,)| r).collect())
    }

    /// Claims the removals that are due for a few minutes, so no two pods take the same one.
    pub async fn claim_due_removals(&self, now: DateTime<Utc>, limit: i64) -> anyhow::Result<Vec<TimedRoleRow>> {
        Ok(sqlx::query_as::<_, TimedRoleRow>(
            "UPDATE timed_roles SET lease_until = now() + interval '5 minutes'
             WHERE id IN (
                 SELECT id FROM timed_roles
                 WHERE remove_at <= $1 AND (lease_until IS NULL OR lease_until < now())
                 ORDER BY remove_at, id LIMIT $2
                 FOR UPDATE SKIP LOCKED
             )
             RETURNING id, guild_id, user_id, role_id",
        )
        .bind(now)
        .bind(limit)
        .fetch_all(self.pool())
        .await?)
    }

    pub async fn finish_removal(&self, id: i64) -> anyhow::Result<()> {
        sqlx::query("DELETE FROM timed_roles WHERE id = $1")
            .bind(id)
            .execute(self.pool())
            .await?;
        Ok(())
    }

    /// Gives `role_id` to the member at `grant_at`; once given it lasts `remove_after_secs` (0: for good).
    pub async fn schedule_grant(
        &self,
        guild_id: u64,
        user_id: u64,
        role_id: u64,
        grant_at: DateTime<Utc>,
        remove_after_secs: i32,
    ) -> anyhow::Result<()> {
        sqlx::query(
            "INSERT INTO delayed_roles (guild_id, user_id, role_id, grant_at, remove_after_secs)
             VALUES ($1, $2, $3, $4, $5)
             ON CONFLICT (guild_id, user_id, role_id)
             DO UPDATE SET grant_at = EXCLUDED.grant_at, remove_after_secs = EXCLUDED.remove_after_secs,
                           lease_until = NULL",
        )
        .bind(sql_id(guild_id))
        .bind(sql_id(user_id))
        .bind(sql_id(role_id))
        .bind(grant_at)
        .bind(remove_after_secs)
        .execute(self.pool())
        .await?;
        Ok(())
    }

    pub async fn claim_due_grants(&self, now: DateTime<Utc>, limit: i64) -> anyhow::Result<Vec<DelayedRoleRow>> {
        Ok(sqlx::query_as::<_, DelayedRoleRow>(
            "UPDATE delayed_roles SET lease_until = now() + interval '5 minutes'
             WHERE id IN (
                 SELECT id FROM delayed_roles
                 WHERE grant_at <= $1 AND (lease_until IS NULL OR lease_until < now())
                 ORDER BY grant_at, id LIMIT $2
                 FOR UPDATE SKIP LOCKED
             )
             RETURNING id, guild_id, user_id, role_id, remove_after_secs",
        )
        .bind(now)
        .bind(limit)
        .fetch_all(self.pool())
        .await?)
    }

    pub async fn finish_grant(&self, id: i64) -> anyhow::Result<()> {
        sqlx::query("DELETE FROM delayed_roles WHERE id = $1")
            .bind(id)
            .execute(self.pool())
            .await?;
        Ok(())
    }

    /// Drops the waiting grants of a member who left.
    pub async fn cancel_grants(&self, guild_id: u64, user_id: u64) -> anyhow::Result<()> {
        sqlx::query("DELETE FROM delayed_roles WHERE guild_id = $1 AND user_id = $2")
            .bind(sql_id(guild_id))
            .bind(sql_id(user_id))
            .execute(self.pool())
            .await?;
        Ok(())
    }

    // /Colour roles

    pub async fn color_settings(&self, guild_id: u64) -> anyhow::Result<ColorSettings> {
        let found = sqlx::query_as::<_, ColorSettings>(
            "SELECT enabled, allowed_roles, mode, anchor_role_id FROM color_settings WHERE guild_id = $1",
        )
        .bind(sql_id(guild_id))
        .fetch_optional(self.pool())
        .await?;
        Ok(found.unwrap_or_default())
    }

    pub async fn save_color_settings(&self, guild_id: u64, settings: &ColorSettings) -> anyhow::Result<()> {
        sqlx::query(
            "INSERT INTO color_settings (guild_id, enabled, allowed_roles, mode, anchor_role_id)
             VALUES ($1, $2, $3, $4, $5)
             ON CONFLICT (guild_id) DO UPDATE
                SET enabled = EXCLUDED.enabled, allowed_roles = EXCLUDED.allowed_roles, mode = EXCLUDED.mode,
                    anchor_role_id = EXCLUDED.anchor_role_id",
        )
        .bind(sql_id(guild_id))
        .bind(settings.enabled)
        .bind(&settings.allowed_roles)
        .bind(&settings.mode)
        .bind(settings.anchor_role_id)
        .execute(self.pool())
        .await?;
        Ok(())
    }

    /// The server's colour roles, oldest role first.
    pub async fn color_roles(&self, guild_id: u64) -> anyhow::Result<Vec<ColorRoleRow>> {
        Ok(sqlx::query_as::<_, ColorRoleRow>(
            "SELECT role_id, hex, name FROM color_roles WHERE guild_id = $1 ORDER BY role_id",
        )
        .bind(sql_id(guild_id))
        .fetch_all(self.pool())
        .await?)
    }

    /// Makes `role_id` a colour role with this colour and name, up to `max` roles a server. One role
    /// per colour: a colour another role has already is refused. The server's settings row is locked
    /// while the count is checked.
    pub async fn add_color_role(
        &self,
        guild_id: u64,
        role_id: u64,
        hex: &str,
        name: &str,
        max: i64,
    ) -> anyhow::Result<ColorAdded> {
        let mut tx = self.pool().begin().await?;
        sqlx::query("INSERT INTO color_settings (guild_id) VALUES ($1) ON CONFLICT (guild_id) DO NOTHING")
            .bind(sql_id(guild_id))
            .execute(&mut *tx)
            .await?;
        sqlx::query("SELECT guild_id FROM color_settings WHERE guild_id = $1 FOR UPDATE")
            .bind(sql_id(guild_id))
            .fetch_one(&mut *tx)
            .await?;
        let same_colour: Option<(i64,)> =
            sqlx::query_as("SELECT role_id FROM color_roles WHERE guild_id = $1 AND hex = $2")
                .bind(sql_id(guild_id))
                .bind(hex)
                .fetch_optional(&mut *tx)
                .await?;
        if same_colour.is_some_and(|(other,)| other != sql_id(role_id)) {
            return Ok(ColorAdded::HexTaken);
        }
        let known: Option<(i64,)> =
            sqlx::query_as("SELECT role_id FROM color_roles WHERE guild_id = $1 AND role_id = $2")
                .bind(sql_id(guild_id))
                .bind(sql_id(role_id))
                .fetch_optional(&mut *tx)
                .await?;
        let (count,): (i64,) = sqlx::query_as("SELECT count(*) FROM color_roles WHERE guild_id = $1")
            .bind(sql_id(guild_id))
            .fetch_one(&mut *tx)
            .await?;
        if known.is_none() && count >= max {
            return Ok(ColorAdded::Full);
        }
        sqlx::query(
            "INSERT INTO color_roles (guild_id, role_id, hex, name) VALUES ($1, $2, $3, $4)
             ON CONFLICT (guild_id, role_id) DO UPDATE SET hex = EXCLUDED.hex, name = EXCLUDED.name",
        )
        .bind(sql_id(guild_id))
        .bind(sql_id(role_id))
        .bind(hex)
        .bind(name)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(if known.is_some() {
            ColorAdded::Updated
        } else {
            ColorAdded::Added
        })
    }

    pub async fn remove_color_role(&self, guild_id: u64, role_id: u64) -> anyhow::Result<bool> {
        Ok(
            sqlx::query("DELETE FROM color_roles WHERE guild_id = $1 AND role_id = $2")
                .bind(sql_id(guild_id))
                .bind(sql_id(role_id))
                .execute(self.pool())
                .await?
                .rows_affected()
                > 0,
        )
    }

    // /Roles that no longer exist

    /// Drops a role that no longer exists on Discord from everything the roles pack keeps: auto roles,
    /// colour roles, reactions, timers, what sticky roles remember, and the panels' required role.
    /// (`forget_role` does the same for level roles, the store and the panels' entries.)
    pub async fn forget_pack_role(&self, guild_id: u64, role_id: u64) -> anyhow::Result<()> {
        let (guild, role) = (sql_id(guild_id), sql_id(role_id));
        let mut tx = self.pool().begin().await?;
        for sql in [
            "DELETE FROM auto_roles WHERE guild_id = $1 AND role_id = $2",
            "DELETE FROM color_roles WHERE guild_id = $1 AND role_id = $2",
            "DELETE FROM timed_roles WHERE guild_id = $1 AND role_id = $2",
            "DELETE FROM delayed_roles WHERE guild_id = $1 AND role_id = $2",
            "DELETE FROM panel_reactions WHERE role_id = $2
                AND panel_id IN (SELECT id FROM role_panels WHERE guild_id = $1)",
            "UPDATE color_settings SET anchor_role_id = NULL WHERE guild_id = $1 AND anchor_role_id = $2",
            "UPDATE color_settings SET allowed_roles = array_remove(allowed_roles, $2) WHERE guild_id = $1",
            "UPDATE role_panels SET required_role = NULL WHERE guild_id = $1 AND required_role = $2",
            "UPDATE sticky_roles SET role_ids = array_remove(role_ids, $2) WHERE guild_id = $1 AND $2 = ANY(role_ids)",
        ] {
            sqlx::query(sql).bind(guild).bind(role).execute(&mut *tx).await?;
        }
        tx.commit().await?;
        Ok(())
    }
}
