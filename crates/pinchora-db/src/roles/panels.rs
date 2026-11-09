use crate::{Db, sql_id};

/// Most roles (buttons) one panel can hold: five rows of five.
pub const MAX_PANEL_ROLES: i64 = 25;

/// A message of role buttons, a role menu or reactions, possibly still a draft.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct PanelRow {
    pub id: i64,
    pub title: String,
    pub description: Option<String>,
    /// Where the panel was posted; `None` while it is a draft.
    pub channel_id: Option<i64>,
    pub message_id: Option<i64>,
    /// `buttons`, `select` or `reactions`.
    pub style: String,
    /// At most one role of the panel at a time.
    pub exclusive: bool,
    /// A role a member needs to use the panel.
    pub required_role: Option<i64>,
    pub min_picks: i32,
    pub max_picks: i32,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct PanelRoleRow {
    pub role_id: i64,
    pub label: String,
    /// A Unicode emoji or Discord's `<:name:id>` markup.
    pub emoji: Option<String>,
    /// A line under the label in a role menu.
    pub description: Option<String>,
    /// The custom emoji's id, when `emoji` is one.
    pub emoji_id: Option<i64>,
    /// How long the role lasts once a member took it from this panel; `None` is for good.
    pub temp_secs: Option<i32>,
}

/// What a command or the dashboard sets on a panel besides its title and description; `None` leaves a
/// column as it is.
#[derive(Debug, Clone, Default)]
pub struct PanelPatch {
    pub style: Option<String>,
    pub exclusive: Option<bool>,
    /// `Some(None)` takes the required role away.
    pub required_role: Option<Option<u64>>,
    pub min_picks: Option<i32>,
    pub max_picks: Option<i32>,
}

/// One entry of a panel as it is written: a role with the way it shows.
#[derive(Debug, Clone, Default)]
pub struct PanelEntry {
    pub label: String,
    pub emoji: Option<String>,
    pub emoji_id: Option<u64>,
    pub description: Option<String>,
}

/// A whole panel, for the dashboard's editor: saved in one go.
#[derive(Debug, Clone)]
pub struct PanelDoc {
    pub title: String,
    pub description: Option<String>,
    pub style: String,
    pub exclusive: bool,
    pub required_role: Option<u64>,
    pub min_picks: i32,
    pub max_picks: i32,
    pub roles: Vec<PanelRoleDoc>,
}

#[derive(Debug, Clone)]
pub struct PanelRoleDoc {
    pub role_id: u64,
    pub entry: PanelEntry,
    pub temp_secs: Option<i32>,
}

/// How saving a whole panel ended.
#[derive(Debug, Clone)]
pub enum PanelSaved {
    Saved(PanelRow),
    /// There is no such panel in this server.
    Missing,
    /// Another panel of the server already has the title.
    TitleTaken,
}

#[derive(sqlx::FromRow)]
struct SavedPanel {
    #[sqlx(flatten)]
    panel: PanelRow,
    created: bool,
}

macro_rules! panel_columns {
    () => {
        "id, title, description, channel_id, message_id, style, exclusive, required_role, min_picks, max_picks"
    };
}

macro_rules! role_columns {
    () => {
        "role_id, label, emoji, description, emoji_id, temp_secs"
    };
}

/// Postgres' unique violation.
fn title_taken(error: &sqlx::Error) -> bool {
    error
        .as_database_error()
        .and_then(|e| e.code())
        .is_some_and(|code| code == "23505")
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
        self.save_panel_with(guild_id, title, description, &PanelPatch::default())
            .await
    }

    /// [`Db::save_panel`] that also sets the panel's style and rules, the ones `patch` names.
    pub async fn save_panel_with(
        &self,
        guild_id: u64,
        title: &str,
        description: Option<&str>,
        patch: &PanelPatch,
    ) -> anyhow::Result<(PanelRow, bool)> {
        let saved = sqlx::query_as::<_, SavedPanel>(concat!(
            "INSERT INTO role_panels (guild_id, title, description, style, exclusive, required_role, min_picks, max_picks)
             VALUES ($1, $2, $3, COALESCE($4::TEXT, 'buttons'), COALESCE($5::BOOLEAN, FALSE), $6::BIGINT,
                     COALESCE($7::INTEGER, 0), COALESCE($8::INTEGER, 0))
             ON CONFLICT (guild_id, lower(title))
             DO UPDATE SET description = COALESCE(EXCLUDED.description, role_panels.description),
                           style = COALESCE($4::TEXT, role_panels.style),
                           exclusive = COALESCE($5::BOOLEAN, role_panels.exclusive),
                           required_role = CASE WHEN $9::BOOLEAN THEN $6::BIGINT ELSE role_panels.required_role END,
                           min_picks = COALESCE($7::INTEGER, role_panels.min_picks),
                           max_picks = COALESCE($8::INTEGER, role_panels.max_picks)
             RETURNING ",
            panel_columns!(),
            ", (xmax = 0) AS created"
        ))
        .bind(sql_id(guild_id))
        .bind(title)
        .bind(description)
        .bind(patch.style.as_deref())
        .bind(patch.exclusive)
        .bind(patch.required_role.flatten().map(sql_id))
        .bind(patch.min_picks)
        .bind(patch.max_picks)
        .bind(patch.required_role.is_some())
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

    /// The panel with this number in this server: how a click finds its panel.
    pub async fn panel_by_id(&self, guild_id: u64, panel_id: i64) -> anyhow::Result<Option<PanelRow>> {
        Ok(sqlx::query_as::<_, PanelRow>(concat!(
            "SELECT ",
            panel_columns!(),
            " FROM role_panels WHERE guild_id = $1 AND id = $2"
        ))
        .bind(sql_id(guild_id))
        .bind(panel_id)
        .fetch_optional(self.pool())
        .await?)
    }

    /// Every panel of the server, oldest first.
    pub async fn panels(&self, guild_id: u64) -> anyhow::Result<Vec<PanelRow>> {
        Ok(sqlx::query_as::<_, PanelRow>(concat!(
            "SELECT ",
            panel_columns!(),
            " FROM role_panels WHERE guild_id = $1 ORDER BY id"
        ))
        .bind(sql_id(guild_id))
        .fetch_all(self.pool())
        .await?)
    }

    /// The panel whose message this is, in this server: how a reaction finds its panel.
    pub async fn panel_by_message(&self, guild_id: u64, message_id: u64) -> anyhow::Result<Option<PanelRow>> {
        Ok(sqlx::query_as::<_, PanelRow>(concat!(
            "SELECT ",
            panel_columns!(),
            " FROM role_panels WHERE guild_id = $1 AND message_id = $2"
        ))
        .bind(sql_id(guild_id))
        .bind(sql_id(message_id))
        .fetch_optional(self.pool())
        .await?)
    }

    /// Takes a panel off the list (its entries go with it); the message it was posted as stays.
    pub async fn delete_panel(&self, guild_id: u64, panel_id: i64) -> anyhow::Result<Option<PanelRow>> {
        Ok(sqlx::query_as::<_, PanelRow>(concat!(
            "DELETE FROM role_panels WHERE guild_id = $1 AND id = $2 RETURNING ",
            panel_columns!()
        ))
        .bind(sql_id(guild_id))
        .bind(panel_id)
        .fetch_optional(self.pool())
        .await?)
    }

    /// The panel's buttons in the order they were added.
    pub async fn panel_roles(&self, panel_id: i64) -> anyhow::Result<Vec<PanelRoleRow>> {
        Ok(sqlx::query_as::<_, PanelRoleRow>(concat!(
            "SELECT ",
            role_columns!(),
            " FROM role_panel_roles WHERE panel_id = $1 ORDER BY position"
        ))
        .bind(panel_id)
        .fetch_all(self.pool())
        .await?)
    }

    /// The button for `role_id` on panel `panel_id`, but only when that panel belongs to `guild_id`.
    /// A click is checked against this, so a stale or forged id gives nothing.
    pub async fn panel_role(&self, guild_id: u64, panel_id: i64, role_id: u64) -> anyhow::Result<Option<PanelRoleRow>> {
        Ok(sqlx::query_as::<_, PanelRoleRow>(
            "SELECT r.role_id, r.label, r.emoji, r.description, r.emoji_id, r.temp_secs
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
        let entry = PanelEntry {
            label: label.to_owned(),
            emoji: emoji.map(str::to_owned),
            ..Default::default()
        };
        self.add_panel_entry(panel_id, role_id, &entry).await
    }

    /// [`Db::add_panel_role`] with the rest of the entry: the custom emoji's id, and a line for the menu
    /// (`None` keeps the line the entry already has).
    pub async fn add_panel_entry(&self, panel_id: i64, role_id: u64, entry: &PanelEntry) -> anyhow::Result<bool> {
        let mut tx = self.pool().begin().await?;
        sqlx::query("SELECT id FROM role_panels WHERE id = $1 FOR UPDATE")
            .bind(panel_id)
            .fetch_optional(&mut *tx)
            .await?;
        let added = sqlx::query(
            "INSERT INTO role_panel_roles (panel_id, role_id, label, emoji, emoji_id, description, position)
             SELECT $1, $2, $3, $4, $5, $6, COALESCE((SELECT max(position) FROM role_panel_roles WHERE panel_id = $1), 0) + 1
             WHERE (SELECT count(*) FROM role_panel_roles WHERE panel_id = $1) < $7
                OR EXISTS (SELECT 1 FROM role_panel_roles WHERE panel_id = $1 AND role_id = $2)
             ON CONFLICT (panel_id, role_id) DO UPDATE
                SET label = EXCLUDED.label, emoji = EXCLUDED.emoji, emoji_id = EXCLUDED.emoji_id,
                    description = COALESCE(EXCLUDED.description, role_panel_roles.description)",
        )
        .bind(panel_id)
        .bind(sql_id(role_id))
        .bind(&entry.label)
        .bind(entry.emoji.as_deref())
        .bind(entry.emoji_id.map(sql_id))
        .bind(entry.description.as_deref())
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

    /// Sets how long `role_id` lasts once taken from a panel of this server (`None`: for good), on one
    /// panel or, with no panel, on every panel that has it. Returns how many entries changed.
    pub async fn set_panel_role_expiry(
        &self,
        guild_id: u64,
        role_id: u64,
        panel_id: Option<i64>,
        secs: Option<i32>,
    ) -> anyhow::Result<u64> {
        Ok(sqlx::query(
            "UPDATE role_panel_roles SET temp_secs = $4
             WHERE role_id = $2 AND panel_id IN (
                 SELECT id FROM role_panels WHERE guild_id = $1 AND ($3::BIGINT IS NULL OR id = $3))",
        )
        .bind(sql_id(guild_id))
        .bind(sql_id(role_id))
        .bind(panel_id)
        .bind(secs)
        .execute(self.pool())
        .await?
        .rows_affected())
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

    /// The reactions a posted reactions panel carries, as (emoji key, role).
    pub async fn panel_reactions(&self, panel_id: i64) -> anyhow::Result<Vec<(String, i64)>> {
        Ok(
            sqlx::query_as("SELECT emoji, role_id FROM panel_reactions WHERE panel_id = $1 ORDER BY emoji")
                .bind(panel_id)
                .fetch_all(self.pool())
                .await?,
        )
    }

    /// Swaps the panel's reactions for `reactions` (emoji key, role).
    pub async fn replace_panel_reactions(&self, panel_id: i64, reactions: &[(String, u64)]) -> anyhow::Result<()> {
        let mut tx = self.pool().begin().await?;
        sqlx::query("DELETE FROM panel_reactions WHERE panel_id = $1")
            .bind(panel_id)
            .execute(&mut *tx)
            .await?;
        for (emoji, role) in reactions {
            sqlx::query(
                "INSERT INTO panel_reactions (panel_id, emoji, role_id) VALUES ($1, $2, $3)
                 ON CONFLICT (panel_id, emoji) DO NOTHING",
            )
            .bind(panel_id)
            .bind(emoji)
            .bind(sql_id(*role))
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    /// Makes a panel from the dashboard's editor, entries and all.
    pub async fn create_panel_doc(&self, guild_id: u64, doc: &PanelDoc) -> anyhow::Result<PanelSaved> {
        let mut tx = self.pool().begin().await?;
        let made = sqlx::query_as::<_, PanelRow>(concat!(
            "INSERT INTO role_panels (guild_id, title, description, style, exclusive, required_role, min_picks, max_picks)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8) RETURNING ",
            panel_columns!()
        ))
        .bind(sql_id(guild_id))
        .bind(&doc.title)
        .bind(doc.description.as_deref())
        .bind(&doc.style)
        .bind(doc.exclusive)
        .bind(doc.required_role.map(sql_id))
        .bind(doc.min_picks)
        .bind(doc.max_picks)
        .fetch_one(&mut *tx)
        .await;
        let panel = match made {
            Ok(panel) => panel,
            Err(error) if title_taken(&error) => return Ok(PanelSaved::TitleTaken),
            Err(error) => return Err(error.into()),
        };
        Self::write_entries(&mut tx, panel.id, &doc.roles).await?;
        tx.commit().await?;
        Ok(PanelSaved::Saved(panel))
    }

    /// Saves a panel from the dashboard's editor: its settings and its entries, replacing what was there.
    /// The posted message stays as it is.
    pub async fn replace_panel_doc(&self, guild_id: u64, panel_id: i64, doc: &PanelDoc) -> anyhow::Result<PanelSaved> {
        let mut tx = self.pool().begin().await?;
        let saved = sqlx::query_as::<_, PanelRow>(concat!(
            "UPDATE role_panels SET title = $3, description = $4, style = $5, exclusive = $6, required_role = $7,
                    min_picks = $8, max_picks = $9
             WHERE guild_id = $1 AND id = $2 RETURNING ",
            panel_columns!()
        ))
        .bind(sql_id(guild_id))
        .bind(panel_id)
        .bind(&doc.title)
        .bind(doc.description.as_deref())
        .bind(&doc.style)
        .bind(doc.exclusive)
        .bind(doc.required_role.map(sql_id))
        .bind(doc.min_picks)
        .bind(doc.max_picks)
        .fetch_optional(&mut *tx)
        .await;
        let panel = match saved {
            Ok(Some(panel)) => panel,
            Ok(None) => return Ok(PanelSaved::Missing),
            Err(error) if title_taken(&error) => return Ok(PanelSaved::TitleTaken),
            Err(error) => return Err(error.into()),
        };
        sqlx::query("DELETE FROM role_panel_roles WHERE panel_id = $1")
            .bind(panel.id)
            .execute(&mut *tx)
            .await?;
        Self::write_entries(&mut tx, panel.id, &doc.roles).await?;
        tx.commit().await?;
        Ok(PanelSaved::Saved(panel))
    }

    async fn write_entries(
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        panel_id: i64,
        roles: &[PanelRoleDoc],
    ) -> anyhow::Result<()> {
        for (position, role) in roles.iter().enumerate() {
            sqlx::query(
                "INSERT INTO role_panel_roles (panel_id, role_id, label, emoji, emoji_id, description, temp_secs, position)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
            )
            .bind(panel_id)
            .bind(sql_id(role.role_id))
            .bind(&role.entry.label)
            .bind(role.entry.emoji.as_deref())
            .bind(role.entry.emoji_id.map(sql_id))
            .bind(role.entry.description.as_deref())
            .bind(role.temp_secs)
            .bind(position as i32 + 1)
            .execute(&mut **tx)
            .await?;
        }
        Ok(())
    }
}
