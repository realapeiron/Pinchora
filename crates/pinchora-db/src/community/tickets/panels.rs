use super::*;

impl Db {
    // /Panels

    pub async fn ticket_panels(&self, guild: u64) -> anyhow::Result<Vec<TicketPanel>> {
        let sql = format!(
            "SELECT {PANEL_COLUMNS} FROM ticket_panels p LEFT JOIN ticket_forms f ON f.id = p.form_id
             WHERE p.guild_id = $1 ORDER BY p.id"
        );
        sqlx::query_as::<_, RawPanel>(sqlx::AssertSqlSafe(sql))
            .bind(sql_id(guild))
            .fetch_all(self.pool())
            .await?
            .into_iter()
            .map(RawPanel::panel)
            .collect()
    }

    pub async fn ticket_panel(&self, id: i64) -> anyhow::Result<Option<TicketPanel>> {
        let sql = format!(
            "SELECT {PANEL_COLUMNS} FROM ticket_panels p LEFT JOIN ticket_forms f ON f.id = p.form_id WHERE p.id = $1"
        );
        sqlx::query_as::<_, RawPanel>(sqlx::AssertSqlSafe(sql))
            .bind(id)
            .fetch_optional(self.pool())
            .await?
            .map(RawPanel::panel)
            .transpose()
    }

    /// A server's panel by its name (any case) or its number in the list `/tickets panel list` shows.
    pub async fn find_ticket_panel(&self, guild: u64, key: &str) -> anyhow::Result<Option<TicketPanel>> {
        let key = key.trim();
        let panels = self.ticket_panels(guild).await?;
        if let Ok(n) = key.parse::<usize>() {
            return Ok(n.checked_sub(1).and_then(|i| panels.get(i)).cloned());
        }
        Ok(panels
            .into_iter()
            .find(|p| p.name.eq_ignore_ascii_case(key) || p.name == key))
    }

    /// Creates a panel (`id` `None`) or replaces one of the server's, its form with it.
    pub async fn save_ticket_panel(&self, id: Option<i64>, panel: &PanelWrite<'_>) -> anyhow::Result<TicketPanelSaved> {
        let mut tx = self.pool().begin().await?;
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended('tickets:' || $1::text, 0))")
            .bind(sql_id(panel.guild_id))
            .execute(&mut *tx)
            .await?;
        let guild = sql_id(panel.guild_id);
        let taken: Option<(i64,)> =
            sqlx::query_as("SELECT id FROM ticket_panels WHERE guild_id = $1 AND lower(name) = lower($2)")
                .bind(guild)
                .bind(panel.name.trim())
                .fetch_optional(&mut *tx)
                .await?;
        if taken.is_some_and(|(other,)| Some(other) != id) {
            return Ok(TicketPanelSaved::NameTaken);
        }
        let form = save_form(&mut tx, guild, id, panel).await?;
        let topics = serde_json::to_string(panel.topics)?;
        let roles: Vec<i64> = panel.support_roles.iter().map(|r| sql_id(*r)).collect();
        let saved: Option<(i64,)> = match id {
            None => {
                let (count,): (i64,) = sqlx::query_as("SELECT count(*) FROM ticket_panels WHERE guild_id = $1")
                    .bind(guild)
                    .fetch_one(&mut *tx)
                    .await?;
                if count >= MAX_PANELS as i64 {
                    return Ok(TicketPanelSaved::Full);
                }
                sqlx::query_as(
                    "INSERT INTO ticket_panels (guild_id, name, channel_id, title, description, color, style, buttons,
                         form_id, support_roles, welcome_text, open_category_id, log_channel_id, enabled)
                     VALUES ($1, $2, $3, $4, $5, $6, $7, $8::jsonb, $9, $10, $11, $12, $13, $14) RETURNING id",
                )
                .bind(guild)
                .bind(panel.name.trim())
                .bind(sql_id(panel.channel_id))
                .bind(panel.title)
                .bind(panel.description)
                .bind(panel.color as i32)
                .bind(panel.style.key())
                .bind(&topics)
                .bind(form)
                .bind(&roles)
                .bind(panel.welcome_text)
                .bind(panel.open_category_id.map(sql_id))
                .bind(panel.log_channel_id.map(sql_id))
                .bind(panel.enabled)
                .fetch_optional(&mut *tx)
                .await?
            }
            Some(id) => {
                sqlx::query_as(
                    "UPDATE ticket_panels SET name = $3, channel_id = $4, title = $5, description = $6, color = $7,
                         style = $8, buttons = $9::jsonb, form_id = $10, support_roles = $11, welcome_text = $12,
                         open_category_id = $13, log_channel_id = $14, enabled = $15, updated_at = now(),
                         message_id = CASE WHEN channel_id = $4 THEN message_id END
                     WHERE id = $1 AND guild_id = $2 RETURNING id",
                )
                .bind(id)
                .bind(guild)
                .bind(panel.name.trim())
                .bind(sql_id(panel.channel_id))
                .bind(panel.title)
                .bind(panel.description)
                .bind(panel.color as i32)
                .bind(panel.style.key())
                .bind(&topics)
                .bind(form)
                .bind(&roles)
                .bind(panel.welcome_text)
                .bind(panel.open_category_id.map(sql_id))
                .bind(panel.log_channel_id.map(sql_id))
                .bind(panel.enabled)
                .fetch_optional(&mut *tx)
                .await?
            }
        };
        let Some((id,)) = saved else {
            return Ok(TicketPanelSaved::Gone);
        };
        // A form nobody points at any more (its panel dropped its questions) goes.
        sqlx::query(
            "DELETE FROM ticket_forms f WHERE f.guild_id = $1
             AND NOT EXISTS (SELECT 1 FROM ticket_panels p WHERE p.form_id = f.id)",
        )
        .bind(guild)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        let panel = self
            .ticket_panel(id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("panel {id} vanished"))?;
        Ok(TicketPanelSaved::Saved(Box::new(panel)))
    }

    /// Deletes one of the server's panels; its tickets stay, without a panel.
    pub async fn delete_ticket_panel(&self, guild: u64, id: i64) -> anyhow::Result<Option<TicketPanel>> {
        let Some(panel) = self.ticket_panel(id).await?.filter(|p| p.guild_id == sql_id(guild)) else {
            return Ok(None);
        };
        let mut tx = self.pool().begin().await?;
        sqlx::query("DELETE FROM ticket_panels WHERE id = $1")
            .bind(id)
            .execute(&mut *tx)
            .await?;
        if let Some(form) = panel.form_id {
            sqlx::query("DELETE FROM ticket_forms WHERE id = $1")
                .bind(form)
                .execute(&mut *tx)
                .await?;
        }
        tx.commit().await?;
        Ok(Some(panel))
    }

    /// Remembers where a panel was posted, so posting again edits that message.
    pub async fn set_ticket_panel_message(&self, id: i64, channel: u64, message: Option<u64>) -> anyhow::Result<()> {
        sqlx::query("UPDATE ticket_panels SET channel_id = $2, message_id = $3 WHERE id = $1")
            .bind(id)
            .bind(sql_id(channel))
            .bind(message.map(sql_id))
            .execute(self.pool())
            .await?;
        Ok(())
    }
}

/// The form of a panel being saved: the existing one rewritten, a new one, or none.
async fn save_form(
    tx: &mut Transaction<'_, Postgres>,
    guild: i64,
    panel: Option<i64>,
    write: &PanelWrite<'_>,
) -> anyhow::Result<Option<i64>> {
    if write.questions.is_empty() {
        return Ok(None);
    }
    let questions = serde_json::to_string(write.questions)?;
    let existing: Option<(Option<i64>,)> = match panel {
        Some(id) => {
            sqlx::query_as("SELECT form_id FROM ticket_panels WHERE id = $1 AND guild_id = $2")
                .bind(id)
                .bind(guild)
                .fetch_optional(&mut **tx)
                .await?
        }
        None => None,
    };
    if let Some((Some(form),)) = existing {
        sqlx::query("UPDATE ticket_forms SET questions = $2::jsonb, name = $3 WHERE id = $1")
            .bind(form)
            .bind(&questions)
            .bind(form_name(write.name))
            .execute(&mut **tx)
            .await?;
        return Ok(Some(form));
    }
    let (form,): (i64,) = sqlx::query_as(
        "INSERT INTO ticket_forms (guild_id, name, questions) VALUES ($1, $2, $3::jsonb)
         ON CONFLICT (guild_id, name) DO UPDATE SET questions = EXCLUDED.questions RETURNING id",
    )
    .bind(guild)
    .bind(form_name(write.name))
    .bind(&questions)
    .fetch_one(&mut **tx)
    .await?;
    Ok(Some(form))
}

/// A panel's form is named after it; the name only has to be unique in the server.
fn form_name(panel: &str) -> String {
    format!("{} form", panel.trim().to_lowercase())
}
