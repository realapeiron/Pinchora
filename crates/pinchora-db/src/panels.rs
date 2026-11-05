use crate::{Db, sql_id};

/// Most roles (buttons) one panel can hold: five rows of five.
pub const MAX_PANEL_ROLES: i64 = 25;

/// A message of role buttons, possibly still a draft.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct PanelRow {
    pub id: i64,
    pub title: String,
    pub description: Option<String>,
    /// Where the panel was posted; `None` while it is a draft.
    pub channel_id: Option<i64>,
    pub message_id: Option<i64>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct PanelRoleRow {
    pub role_id: i64,
    pub label: String,
    /// A Unicode emoji or Discord's `<:name:id>` markup.
    pub emoji: Option<String>,
}

#[derive(sqlx::FromRow)]
struct SavedPanel {
    #[sqlx(flatten)]
    panel: PanelRow,
    created: bool,
}

macro_rules! panel_columns {
    () => {
        "id, title, description, channel_id, message_id"
    };
}

impl Db {
    /// Creates a panel, or changes the description of the one with this title (ignoring case).
    /// The flag is true when the panel is new.
    pub async fn save_panel(
        &self,
        guild_id: u64,
        title: &str,
        description: Option<&str>,
    ) -> anyhow::Result<(PanelRow, bool)> {
        let saved = sqlx::query_as::<_, SavedPanel>(concat!(
            "INSERT INTO role_panels (guild_id, title, description) VALUES ($1, $2, $3)
             ON CONFLICT (guild_id, lower(title))
             DO UPDATE SET description = COALESCE(EXCLUDED.description, role_panels.description)
             RETURNING ",
            panel_columns!(),
            ", (xmax = 0) AS created"
        ))
        .bind(sql_id(guild_id))
        .bind(title)
        .bind(description)
        .fetch_one(self.pool())
        .await?;
        Ok((saved.panel, saved.created))
    }

    /// Finds a panel of this server by title (ignoring case), or else by its number.
    pub async fn find_panel(&self, guild_id: u64, key: &str) -> anyhow::Result<Option<PanelRow>> {
        let by_title = sqlx::query_as::<_, PanelRow>(concat!(
            "SELECT ",
            panel_columns!(),
            " FROM role_panels WHERE guild_id = $1 AND lower(title) = lower($2)"
        ))
        .bind(sql_id(guild_id))
        .bind(key.trim())
        .fetch_optional(self.pool())
        .await?;
        if by_title.is_some() {
            return Ok(by_title);
        }
        let Ok(id) = key.trim().trim_start_matches('#').parse::<i64>() else {
            return Ok(None);
        };
        Ok(sqlx::query_as::<_, PanelRow>(concat!(
            "SELECT ",
            panel_columns!(),
            " FROM role_panels WHERE guild_id = $1 AND id = $2"
        ))
        .bind(sql_id(guild_id))
        .bind(id)
        .fetch_optional(self.pool())
        .await?)
    }

    /// The panel's buttons in the order they were added.
    pub async fn panel_roles(&self, panel_id: i64) -> anyhow::Result<Vec<PanelRoleRow>> {
        Ok(sqlx::query_as::<_, PanelRoleRow>(
            "SELECT role_id, label, emoji FROM role_panel_roles WHERE panel_id = $1 ORDER BY position",
        )
        .bind(panel_id)
        .fetch_all(self.pool())
        .await?)
    }

    /// The button for `role_id` on panel `panel_id`, but only when that panel belongs to `guild_id`.
    /// A click is checked against this, so a stale or forged id gives nothing.
    pub async fn panel_role(&self, guild_id: u64, panel_id: i64, role_id: u64) -> anyhow::Result<Option<PanelRoleRow>> {
        Ok(sqlx::query_as::<_, PanelRoleRow>(
            "SELECT r.role_id, r.label, r.emoji
             FROM role_panel_roles r JOIN role_panels p ON p.id = r.panel_id
             WHERE p.guild_id = $1 AND p.id = $2 AND r.role_id = $3",
        )
        .bind(sql_id(guild_id))
        .bind(panel_id)
        .bind(sql_id(role_id))
        .fetch_optional(self.pool())
        .await?)
    }

    /// Adds a button for a role, or changes the label and emoji of the one it already has.
    /// `Ok(false)` when the panel is full. The panel row is locked while the count is checked, so
    /// two managers adding at once cannot go past the limit.
    pub async fn add_panel_role(
        &self,
        panel_id: i64,
        role_id: u64,
        label: &str,
        emoji: Option<&str>,
    ) -> anyhow::Result<bool> {
        let mut tx = self.pool().begin().await?;
        sqlx::query("SELECT id FROM role_panels WHERE id = $1 FOR UPDATE")
            .bind(panel_id)
            .fetch_optional(&mut *tx)
            .await?;
        let added = sqlx::query(
            "INSERT INTO role_panel_roles (panel_id, role_id, label, emoji, position)
             SELECT $1, $2, $3, $4, COALESCE((SELECT max(position) FROM role_panel_roles WHERE panel_id = $1), 0) + 1
             WHERE (SELECT count(*) FROM role_panel_roles WHERE panel_id = $1) < $5
                OR EXISTS (SELECT 1 FROM role_panel_roles WHERE panel_id = $1 AND role_id = $2)
             ON CONFLICT (panel_id, role_id) DO UPDATE SET label = EXCLUDED.label, emoji = EXCLUDED.emoji",
        )
        .bind(panel_id)
        .bind(sql_id(role_id))
        .bind(label)
        .bind(emoji)
        .bind(MAX_PANEL_ROLES)
        .execute(&mut *tx)
        .await?
        .rows_affected();
        tx.commit().await?;
        Ok(added > 0)
    }

    pub async fn remove_panel_role(&self, panel_id: i64, role_id: u64) -> anyhow::Result<bool> {
        let removed = sqlx::query("DELETE FROM role_panel_roles WHERE panel_id = $1 AND role_id = $2")
            .bind(panel_id)
            .bind(sql_id(role_id))
            .execute(self.pool())
            .await?
            .rows_affected();
        Ok(removed > 0)
    }

    /// Remembers where the panel was posted, so the next post updates that message.
    pub async fn set_panel_message(&self, panel_id: i64, channel_id: u64, message_id: u64) -> anyhow::Result<()> {
        sqlx::query("UPDATE role_panels SET channel_id = $2, message_id = $3 WHERE id = $1")
            .bind(panel_id)
            .bind(sql_id(channel_id))
            .bind(sql_id(message_id))
            .execute(self.pool())
            .await?;
        Ok(())
    }
}
