use super::*;

/// A panel with its topics and the questions of its form.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TicketPanel {
    pub id: i64,
    pub guild_id: i64,
    pub name: String,
    pub channel_id: i64,
    pub message_id: Option<i64>,
    pub title: String,
    pub description: String,
    pub color: i32,
    pub style: PanelStyle,
    pub topics: Vec<Topic>,
    pub form_id: Option<i64>,
    pub questions: Vec<Question>,
    pub support_roles: Vec<i64>,
    pub welcome_text: String,
    pub open_category_id: Option<i64>,
    pub log_channel_id: Option<i64>,
    pub enabled: bool,
    pub updated_at: DateTime<Utc>,
}

#[derive(sqlx::FromRow)]
pub(super) struct RawPanel {
    id: i64,
    guild_id: i64,
    name: String,
    channel_id: i64,
    message_id: Option<i64>,
    title: String,
    description: String,
    color: i32,
    style: String,
    buttons: String,
    form_id: Option<i64>,
    questions: Option<String>,
    support_roles: Vec<i64>,
    welcome_text: String,
    open_category_id: Option<i64>,
    log_channel_id: Option<i64>,
    enabled: bool,
    updated_at: DateTime<Utc>,
}

impl RawPanel {
    pub(super) fn panel(self) -> anyhow::Result<TicketPanel> {
        Ok(TicketPanel {
            id: self.id,
            guild_id: self.guild_id,
            name: self.name,
            channel_id: self.channel_id,
            message_id: self.message_id,
            title: self.title,
            description: self.description,
            color: self.color,
            style: PanelStyle::parse(&self.style).unwrap_or_default(),
            topics: serde_json::from_str(&self.buttons)?,
            form_id: self.form_id,
            questions: self
                .questions
                .map(|q| serde_json::from_str(&q))
                .transpose()?
                .unwrap_or_default(),
            support_roles: self.support_roles,
            welcome_text: self.welcome_text,
            open_category_id: self.open_category_id,
            log_channel_id: self.log_channel_id,
            enabled: self.enabled,
            updated_at: self.updated_at,
        })
    }
}

pub(super) const PANEL_COLUMNS: &str = "p.id, p.guild_id, p.name, p.channel_id, p.message_id, p.title, p.description, p.color, \
     p.style, p.buttons::TEXT AS buttons, p.form_id, f.questions::TEXT AS questions, p.support_roles, p.welcome_text, \
     p.open_category_id, p.log_channel_id, p.enabled, p.updated_at";

/// What a panel is made of, to create or replace one.
#[derive(Debug, Clone)]
pub struct PanelWrite<'a> {
    pub guild_id: u64,
    pub name: &'a str,
    pub channel_id: u64,
    pub title: &'a str,
    pub description: &'a str,
    pub color: u32,
    pub style: PanelStyle,
    pub topics: &'a [Topic],
    /// No questions is no form.
    pub questions: &'a [Question],
    pub support_roles: &'a [u64],
    pub welcome_text: &'a str,
    pub open_category_id: Option<u64>,
    pub log_channel_id: Option<u64>,
    pub enabled: bool,
}

/// How saving a panel went.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TicketPanelSaved {
    Saved(Box<TicketPanel>),
    /// The server has [`MAX_PANELS`] already.
    Full,
    /// Another panel of the server has that name.
    NameTaken,
    /// No such panel (it was deleted).
    Gone,
}
