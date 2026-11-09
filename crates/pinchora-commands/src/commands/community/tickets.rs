//! `/ticket` (inside a ticket: close, claim, add people, the transcript) and `/tickets` (Manage Server:
//! the panels members open tickets from, and how tickets run). The work is done in `crate::community::tickets`.

use pinchora_core::admin::settings::parse_hex_color;
use pinchora_core::community::tickets::AUTO_CLOSE_MAX;
use pinchora_core::community::tickets::DEFAULT_COLOR;
use pinchora_core::community::tickets::Mode;
use pinchora_core::community::tickets::OPEN_LIMIT_MAX;
use pinchora_core::community::tickets::PanelDraft;
use pinchora_core::community::tickets::PanelStyle;
use pinchora_core::community::tickets::check_auto_close;
use pinchora_core::community::tickets::check_naming;
use pinchora_core::community::tickets::check_open_limit;
use pinchora_core::community::tickets::check_panel;
use pinchora_core::community::tickets::topics_from_list;
use pinchora_core::t;
use pinchora_db::{PanelWrite, TicketPanel, TicketPanelSaved};
use twilight_interactions::command::{CommandModel, CommandOption, CreateCommand, CreateOption};
use twilight_model::{
    application::interaction::InteractionChannel,
    guild::{Permissions, Role},
    id::{Id, marker::UserMarker},
};
use twilight_util::builder::embed::{EmbedBuilder, EmbedFieldBuilder};

use crate::Cx;
use crate::community::server_events::message_link;
use crate::community::tickets::PanelPost;
use crate::community::tickets::TicketAct;
use crate::community::tickets::post_panel;
use crate::community::tickets::run_action;
use crate::community::tickets::tell;
use crate::infra::util::BRAND;

fn manage_guild() -> Permissions {
    Permissions::MANAGE_GUILD
}

#[derive(CommandModel, CreateCommand)]
#[command(
    name = "ticket",
    desc = "Run the ticket you are in: close it, claim it, add people, get its transcript",
    contexts = "guild"
)]
#[allow(clippy::large_enum_variant)] // parsed once per interaction, never stored
pub enum TicketCommand {
    #[command(name = "close")]
    Close(TicketClose),
    #[command(name = "claim")]
    Claim(TicketClaim),
    #[command(name = "unclaim")]
    Unclaim(TicketUnclaim),
    #[command(name = "add")]
    Add(TicketAdd),
    #[command(name = "remove")]
    Remove(TicketRemove),
    #[command(name = "rename")]
    Rename(TicketRename),
    #[command(name = "transcript")]
    Transcript(TicketTranscript),
    #[command(name = "reopen")]
    Reopen(TicketReopen),
    #[command(name = "move")]
    Move(TicketMove),
    #[command(name = "priority")]
    Priority(TicketPriority),
    #[command(name = "escalate")]
    Escalate(TicketEscalate),
    #[command(name = "transfer")]
    Transfer(TicketTransfer),
    #[command(name = "request-close")]
    RequestClose(TicketRequestClose),
    #[command(name = "force-open")]
    ForceOpen(TicketForceOpen),
    #[command(name = "note")]
    Note(TicketNote),
    #[command(name = "snippet")]
    Snippet(TicketSnippet),
}

#[derive(CommandOption, CreateOption, Clone, Copy, Debug, PartialEq, Eq)]
pub enum TicketPriorityLevel {
    #[option(name = "Low", value = "low")]
    Low,
    #[option(name = "Normal", value = "normal")]
    Normal,
    #[option(name = "High", value = "high")]
    High,
    #[option(name = "Urgent", value = "urgent")]
    Urgent,
}

impl TicketPriorityLevel {
    fn key(self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Normal => "normal",
            Self::High => "high",
            Self::Urgent => "urgent",
        }
    }
}

#[derive(CommandModel, CreateCommand)]
#[command(name = "priority", desc = "Set this ticket's priority (staff)")]
pub struct TicketPriority {
    /// Low, normal, high or urgent
    level: TicketPriorityLevel,
}

#[derive(CommandModel, CreateCommand)]
#[command(name = "escalate", desc = "Escalate this ticket as urgent (staff)")]
pub struct TicketEscalate;

#[derive(CommandModel, CreateCommand)]
#[command(name = "transfer", desc = "Assign this ticket to a staff member or support role")]
pub struct TicketTransfer {
    /// The staff member taking over
    user: Option<Id<UserMarker>>,
    /// A support role taking over
    role: Option<Role>,
}

#[derive(CommandModel, CreateCommand)]
#[command(name = "request-close", desc = "Ask the opener to confirm closing this ticket")]
pub struct TicketRequestClose;

#[derive(CommandModel, CreateCommand)]
#[command(name = "force-open", desc = "Reopen a closed ticket as staff")]
pub struct TicketForceOpen;

#[derive(CommandModel, CreateCommand)]
#[command(name = "note", desc = "Save a private staff note, never shown in transcripts")]
pub struct TicketNote {
    /// Only staff can see this note in the dashboard
    #[command(min_length = 1, max_length = 2000)]
    text: String,
}

#[derive(CommandModel, CreateCommand)]
#[command(name = "snippet", desc = "Send a saved reply with ticket placeholders")]
pub struct TicketSnippet {
    /// Saved reply name
    #[command(min_length = 1, max_length = 50)]
    name: String,
}

#[derive(CommandModel, CreateCommand)]
#[command(name = "close", desc = "Close this ticket and keep its transcript")]
pub struct TicketClose {
    /// Why it is closed: shown in the ticket, the log and the transcript
    #[command(max_length = 200)]
    reason: Option<String>,
}

#[derive(CommandModel, CreateCommand)]
#[command(name = "claim", desc = "Say you handle this ticket (staff)")]
pub struct TicketClaim;

#[derive(CommandModel, CreateCommand)]
#[command(name = "unclaim", desc = "Hand this ticket back to the queue (staff)")]
pub struct TicketUnclaim;

#[derive(CommandModel, CreateCommand)]
#[command(name = "add", desc = "Let a member see and write in this ticket (staff)")]
pub struct TicketAdd {
    /// Who to add
    user: Id<UserMarker>,
}

#[derive(CommandModel, CreateCommand)]
#[command(name = "remove", desc = "Take a member out of this ticket (staff)")]
pub struct TicketRemove {
    /// Who to take out
    user: Id<UserMarker>,
}

#[derive(CommandModel, CreateCommand)]
#[command(name = "rename", desc = "Rename this ticket's channel (staff)")]
pub struct TicketRename {
    /// The new name; {number}, {user} and {topic} are filled in
    #[command(min_length = 1, max_length = 60)]
    name: String,
}

#[derive(CommandModel, CreateCommand)]
#[command(name = "transcript", desc = "Get this ticket's messages as a web page, only for you")]
pub struct TicketTranscript;

#[derive(CommandModel, CreateCommand)]
#[command(name = "reopen", desc = "Open this closed ticket again (staff)")]
pub struct TicketReopen;

#[derive(CommandModel, CreateCommand)]
#[command(name = "move", desc = "Move this ticket's channel to another category (staff)")]
pub struct TicketMove {
    /// The category to move it to
    #[command(channel_types = "guild_category")]
    category: InteractionChannel,
}

impl TicketCommand {
    pub async fn run(self, cx: &Cx) -> anyhow::Result<()> {
        if cx.guild_id().is_none() {
            return tell(cx, t!(cx.lang(), "tickets.guild-only")).await;
        }
        let act = match self {
            Self::Close(c) => TicketAct::Close(c.reason),
            Self::Claim(_) => TicketAct::Claim,
            Self::Unclaim(_) => TicketAct::Unclaim,
            Self::Add(c) => TicketAct::Add(c.user.get()),
            Self::Remove(c) => TicketAct::Remove(c.user.get()),
            Self::Rename(c) => TicketAct::Rename(c.name),
            Self::Transcript(_) => TicketAct::Transcript,
            Self::Reopen(_) => TicketAct::Reopen,
            Self::Move(c) => TicketAct::Move(c.category.id.get()),
            Self::Priority(c) => TicketAct::Priority(c.level.key().to_owned()),
            Self::Escalate(_) => TicketAct::Escalate,
            Self::Transfer(c) => TicketAct::Transfer(c.user.map(Id::get), c.role.map(|r| r.id.get())),
            Self::RequestClose(_) => TicketAct::RequestClose,
            Self::ForceOpen(_) => TicketAct::ForceOpen,
            Self::Note(c) => TicketAct::Note(c.text),
            Self::Snippet(c) => TicketAct::Snippet(c.name),
        };
        run_action(cx, act).await
    }
}

#[derive(CommandModel, CreateCommand)]
#[command(
    name = "tickets",
    desc = "Set up tickets: the panels members open them from, and how they run",
    default_permissions = "manage_guild",
    contexts = "guild"
)]
#[allow(clippy::large_enum_variant)] // parsed once per interaction, never stored
pub enum TicketsCommand {
    #[command(name = "panel")]
    Panel(TicketsPanel),
    #[command(name = "settings")]
    Settings(TicketsSettings),
}

#[derive(CommandModel, CreateCommand)]
#[command(name = "panel", desc = "The panels members open tickets from")]
#[allow(clippy::large_enum_variant)] // parsed once per interaction, never stored
pub enum TicketsPanel {
    #[command(name = "create")]
    Create(PanelCreate),
    #[command(name = "edit")]
    Edit(PanelEdit),
    #[command(name = "post")]
    Post(PanelPostCommand),
    #[command(name = "delete")]
    Delete(PanelDelete),
    #[command(name = "list")]
    List(PanelList),
}

#[derive(CommandOption, CreateOption, Clone, Copy, Debug, PartialEq, Eq)]
pub enum StyleChoice {
    #[option(name = "Buttons (up to 5 topics)", value = "buttons")]
    Buttons,
    #[option(name = "Menu (up to 25 topics)", value = "select")]
    Select,
}

impl StyleChoice {
    fn style(self) -> PanelStyle {
        match self {
            Self::Buttons => PanelStyle::Buttons,
            Self::Select => PanelStyle::Select,
        }
    }
}

#[derive(CommandModel, CreateCommand)]
#[command(name = "create", desc = "Make a panel (up to 10); post it with /tickets panel post")]
pub struct PanelCreate {
    /// How the other commands and the dashboard find it
    #[command(min_length = 1, max_length = 32)]
    name: String,
    /// Where it is posted, and where private threads open in thread mode
    #[command(channel_types = "guild_text guild_announcement")]
    channel: InteractionChannel,
    /// Its title (default: Need help?)
    #[command(max_length = 100)]
    title: Option<String>,
    /// The text above the buttons
    #[command(max_length = 2000)]
    description: Option<String>,
    /// Topics, one button or option each: Support, Billing, Report a member
    #[command(max_length = 1000)]
    topics: Option<String>,
    /// Buttons or a menu
    style: Option<StyleChoice>,
    /// The role that answers these tickets
    support_role: Option<Role>,
}

#[derive(CommandModel, CreateCommand)]
#[command(name = "edit", desc = "Change a panel; posted already, post it again to update it")]
pub struct PanelEdit {
    /// The panel's name or number
    #[command(min_length = 1, max_length = 32)]
    panel: String,
    /// A new name
    #[command(min_length = 1, max_length = 32)]
    name: Option<String>,
    /// A new title
    #[command(max_length = 100)]
    title: Option<String>,
    /// A new text above the buttons
    #[command(max_length = 2000)]
    description: Option<String>,
    /// New topics, one button or option each: Support, Billing, Report a member
    #[command(max_length = 1000)]
    topics: Option<String>,
    /// Buttons or a menu
    style: Option<StyleChoice>,
    /// The role that answers these tickets (replaces the panel's roles)
    support_role: Option<Role>,
    /// The panel's colour, like #ff6b4a
    #[command(max_length = 7)]
    color: Option<String>,
    /// What the ticket says first; {user}, {server}, {number} and {topic} are filled in
    #[command(max_length = 1500)]
    welcome: Option<String>,
    /// Where it is posted
    #[command(channel_types = "guild_text guild_announcement")]
    channel: Option<InteractionChannel>,
}

#[derive(CommandModel, CreateCommand)]
#[command(name = "post", desc = "Post a panel, or update it where it is")]
pub struct PanelPostCommand {
    /// The panel's name or number
    #[command(min_length = 1, max_length = 32)]
    panel: String,
}

#[derive(CommandModel, CreateCommand)]
#[command(name = "delete", desc = "Delete a panel; its tickets stay")]
pub struct PanelDelete {
    /// The panel's name or number
    #[command(min_length = 1, max_length = 32)]
    panel: String,
}

#[derive(CommandModel, CreateCommand)]
#[command(name = "list", desc = "The server's panels")]
pub struct PanelList;

#[derive(CommandOption, CreateOption, Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModeChoice {
    #[option(name = "Automatic (threads past 400 channels)", value = "auto")]
    Auto,
    #[option(name = "Channels", value = "channel")]
    Channel,
    #[option(name = "Private threads", value = "thread")]
    Thread,
}

#[derive(CommandOption, CreateOption, Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClearChoice {
    #[option(name = "Ticket category", value = "category")]
    Category,
    #[option(name = "Closed category", value = "closed_category")]
    ClosedCategory,
    #[option(name = "Log channel", value = "log_channel")]
    LogChannel,
}

#[derive(CommandModel, CreateCommand)]
#[command(name = "settings", desc = "How tickets run; with no options it shows the settings")]
pub struct TicketsSettings {
    /// The category new ticket channels go in
    #[command(channel_types = "guild_category")]
    category: Option<InteractionChannel>,
    /// The category closed tickets move to
    #[command(channel_types = "guild_category")]
    closed_category: Option<InteractionChannel>,
    /// Where opened and closed tickets are logged, with transcripts
    #[command(channel_types = "guild_text")]
    log_channel: Option<InteractionChannel>,
    /// Close a ticket nobody wrote in for this many hours (0: never)
    #[command(min_value = 0, max_value = 720)]
    auto_close_hours: Option<i64>,
    /// Send the member their transcript by direct message
    dm_transcript: Option<bool>,
    /// Ask the member for 1 to 5 stars after a close
    ratings: Option<bool>,
    /// Channels or private threads
    mode: Option<ModeChoice>,
    /// The channel name: {number}, {user} and {topic} are filled in
    #[command(min_length = 1, max_length = 60)]
    naming: Option<String>,
    /// Tickets one member may have open at once (1 to 10)
    #[command(min_value = 1, max_value = 10)]
    max_open: Option<i64>,
    /// Unset one of the channels above
    clear: Option<ClearChoice>,
}

impl TicketsCommand {
    pub async fn run(self, cx: &Cx) -> anyhow::Result<()> {
        let lang = cx.lang();
        let Some(guild) = cx.guild_id() else {
            return tell(cx, t!(lang, "tickets.guild-only")).await;
        };
        if !cx.can_manage_guild() {
            return tell(cx, t!(lang, "tickets.admin-only")).await;
        }
        match self {
            Self::Panel(TicketsPanel::Create(c)) => create(cx, guild, c).await,
            Self::Panel(TicketsPanel::Edit(c)) => edit(cx, guild, c).await,
            Self::Panel(TicketsPanel::Post(c)) => post(cx, guild, &c.panel).await,
            Self::Panel(TicketsPanel::Delete(c)) => delete(cx, guild, &c.panel).await,
            Self::Panel(TicketsPanel::List(_)) => list(cx, guild).await,
            Self::Settings(c) => settings(cx, guild, c).await,
        }
    }
}

/// The panel named by `key`, or a private answer saying there is none.
async fn find(cx: &Cx, guild: u64, key: &str) -> anyhow::Result<Option<TicketPanel>> {
    let found = cx.app.db.find_ticket_panel(guild, key).await?;
    if found.is_none() {
        tell(cx, t!(cx.lang(), "tickets.admin.no-panel", panel = key)).await?;
    }
    Ok(found)
}

/// Writes a panel after the same checks the dashboard runs; tells the manager what went wrong otherwise.
async fn save(cx: &Cx, id: Option<i64>, write: PanelWrite<'_>) -> anyhow::Result<Option<TicketPanel>> {
    let lang = cx.lang();
    let problems = check_panel(
        &PanelDraft {
            name: write.name,
            title: write.title,
            description: write.description,
            style: write.style,
            topics: write.topics,
            questions: write.questions,
            welcome: write.welcome_text,
            support_roles: write.support_roles.len(),
        },
        lang,
    );
    if let Some(first) = problems.first() {
        tell(cx, first.message.clone()).await?;
        return Ok(None);
    }
    match cx.app.db.save_ticket_panel(id, &write).await? {
        TicketPanelSaved::Saved(panel) => Ok(Some(*panel)),
        TicketPanelSaved::Full => {
            tell(
                cx,
                t!(
                    lang,
                    "tickets.admin.full",
                    max = pinchora_core::community::tickets::MAX_PANELS
                ),
            )
            .await?;
            Ok(None)
        }
        TicketPanelSaved::NameTaken => {
            tell(cx, t!(lang, "tickets.admin.name-taken", name = write.name.trim())).await?;
            Ok(None)
        }
        TicketPanelSaved::Gone => {
            tell(cx, t!(lang, "tickets.admin.no-panel", panel = write.name.trim())).await?;
            Ok(None)
        }
    }
}

async fn create(cx: &Cx, guild: u64, c: PanelCreate) -> anyhow::Result<()> {
    let lang = cx.lang();
    let title = c.title.unwrap_or_else(|| t!(lang, "tickets.admin.default-title"));
    let description = c
        .description
        .unwrap_or_else(|| t!(lang, "tickets.admin.default-description"));
    let mut topics = c.topics.as_deref().map(topics_from_list).unwrap_or_default();
    if topics.is_empty() {
        topics.push(pinchora_core::community::tickets::Topic::new(&t!(
            lang,
            "tickets.admin.default-topic"
        )));
    }
    // @everyone as a support role would let everybody see every ticket.
    let roles: Vec<u64> = c
        .support_role
        .map(|r| r.id.get())
        .filter(|r| *r != guild)
        .into_iter()
        .collect();
    let saved = save(
        cx,
        None,
        PanelWrite {
            guild_id: guild,
            name: &c.name,
            channel_id: c.channel.id.get(),
            title: &title,
            description: &description,
            color: DEFAULT_COLOR,
            style: c.style.map_or(PanelStyle::Buttons, StyleChoice::style),
            topics: &topics,
            questions: &[],
            support_roles: &roles,
            welcome_text: "",
            open_category_id: None,
            log_channel_id: None,
            enabled: true,
        },
    )
    .await?;
    let Some(panel) = saved else { return Ok(()) };
    tell(
        cx,
        t!(
            lang,
            "tickets.admin.created",
            name = panel.name,
            channel = format!("<#{}>", panel.channel_id)
        ),
    )
    .await
}

async fn edit(cx: &Cx, guild: u64, c: PanelEdit) -> anyhow::Result<()> {
    let lang = cx.lang();
    let Some(panel) = find(cx, guild, &c.panel).await? else {
        return Ok(());
    };
    let color = match c.color.as_deref() {
        Some(hex) => match parse_hex_color(hex) {
            Some(color) => color,
            None => return tell(cx, t!(lang, "tickets.admin.color")).await,
        },
        None => panel.color as u32,
    };
    let topics = c
        .topics
        .as_deref()
        .map(topics_from_list)
        .unwrap_or(panel.topics.clone());
    let roles: Vec<u64> = match c.support_role {
        Some(role) => [role.id.get()].into_iter().filter(|r| *r != guild).collect(),
        None => panel.support_roles.iter().map(|r| *r as u64).collect(),
    };
    let name = c.name.unwrap_or(panel.name.clone());
    let title = c.title.unwrap_or(panel.title.clone());
    let description = c.description.unwrap_or(panel.description.clone());
    let welcome = c.welcome.unwrap_or(panel.welcome_text.clone());
    let saved = save(
        cx,
        Some(panel.id),
        PanelWrite {
            guild_id: guild,
            name: &name,
            channel_id: c.channel.map_or(panel.channel_id as u64, |ch| ch.id.get()),
            title: &title,
            description: &description,
            color,
            style: c.style.map_or(panel.style, StyleChoice::style),
            topics: &topics,
            questions: &panel.questions,
            support_roles: &roles,
            welcome_text: &welcome,
            open_category_id: panel.open_category_id.map(|id| id as u64),
            log_channel_id: panel.log_channel_id.map(|id| id as u64),
            enabled: panel.enabled,
        },
    )
    .await?;
    let Some(panel) = saved else { return Ok(()) };
    tell(cx, t!(lang, "tickets.admin.edited", name = panel.name)).await
}

async fn post(cx: &Cx, guild: u64, key: &str) -> anyhow::Result<()> {
    let lang = cx.lang();
    let Some(panel) = find(cx, guild, key).await? else {
        return Ok(());
    };
    cx.responder.defer(true).await?;
    match post_panel(&cx.app, panel.id, lang).await? {
        PanelPost::Posted {
            channel,
            message,
            updated,
        } => {
            let key = if updated {
                "tickets.admin.updated"
            } else {
                "tickets.admin.posted"
            };
            tell(
                cx,
                t!(
                    lang,
                    key,
                    name = panel.name,
                    link = message_link(guild, channel, message)
                ),
            )
            .await
        }
        PanelPost::Refused(why) => tell(cx, why).await,
        PanelPost::Gone => tell(cx, t!(lang, "tickets.admin.no-panel", panel = key)).await,
    }
}

async fn delete(cx: &Cx, guild: u64, key: &str) -> anyhow::Result<()> {
    let lang = cx.lang();
    let Some(panel) = find(cx, guild, key).await? else {
        return Ok(());
    };
    cx.app.db.delete_ticket_panel(guild, panel.id).await?;
    if let Some(message) = panel.message_id
        && let Err(error) = cx
            .app
            .http
            .delete_message(Id::new(panel.channel_id as u64), Id::new(message as u64))
            .await
    {
        tracing::debug!(panel = panel.id, %error, "posted panel not deleted");
    }
    tell(cx, t!(lang, "tickets.admin.deleted", name = panel.name)).await
}

async fn list(cx: &Cx, guild: u64) -> anyhow::Result<()> {
    let lang = cx.lang();
    let panels = cx.app.db.ticket_panels(guild).await?;
    let open = cx.app.db.open_ticket_count(guild).await?;
    let mut embed = EmbedBuilder::new().color(BRAND).title(t!(
        lang,
        "tickets.admin.list-title",
        count = panels.len(),
        max = pinchora_core::community::tickets::MAX_PANELS
    ));
    if panels.is_empty() {
        embed = embed.description(t!(lang, "tickets.admin.list-empty"));
    } else {
        embed = embed.description(t!(lang, "tickets.admin.list-open", count = open));
    }
    for (n, panel) in panels.iter().enumerate() {
        let topics: Vec<String> = panel.topics.iter().map(|t| t.label.clone()).collect();
        let posted = if panel.message_id.is_some() {
            t!(
                lang,
                "tickets.admin.list-posted",
                channel = format!("<#{}>", panel.channel_id)
            )
        } else {
            t!(
                lang,
                "tickets.admin.list-draft",
                channel = format!("<#{}>", panel.channel_id)
            )
        };
        embed = embed.field(EmbedFieldBuilder::new(
            format!("{}. {}", n + 1, panel.name),
            format!("{posted}\n{}", crate::admin::custom::scrub_value(&topics.join(" · "))),
        ));
    }
    cx.reply(twilight_model::http::interaction::InteractionResponseData {
        embeds: Some(vec![embed.build()]),
        flags: Some(twilight_model::channel::message::MessageFlags::EPHEMERAL),
        ..Default::default()
    })
    .await
}

async fn settings(cx: &Cx, guild: u64, c: TicketsSettings) -> anyhow::Result<()> {
    let lang = cx.lang();
    let mut s = cx.app.db.ticket_settings(guild).await?;
    let before = s.clone();
    if let Some(category) = c.category {
        s.open_category_id = Some(category.id.get() as i64);
    }
    if let Some(category) = c.closed_category {
        s.closed_category_id = Some(category.id.get() as i64);
    }
    if let Some(channel) = c.log_channel {
        s.log_channel_id = Some(channel.id.get() as i64);
    }
    match c.clear {
        Some(ClearChoice::Category) => s.open_category_id = None,
        Some(ClearChoice::ClosedCategory) => s.closed_category_id = None,
        Some(ClearChoice::LogChannel) => s.log_channel_id = None,
        None => {}
    }
    if let Some(hours) = c.auto_close_hours {
        match check_auto_close(hours, lang) {
            Ok(hours) => s.auto_close_hours = hours,
            Err(problem) => return tell(cx, problem.message).await,
        }
    }
    if let Some(on) = c.dm_transcript {
        s.dm_transcript = on;
    }
    if let Some(on) = c.ratings {
        s.rating_enabled = on;
    }
    if let Some(mode) = c.mode {
        s.mode = match mode {
            ModeChoice::Auto => Mode::Auto,
            ModeChoice::Channel => Mode::Channel,
            ModeChoice::Thread => Mode::Thread,
        }
        .key()
        .to_owned();
    }
    if let Some(naming) = c.naming {
        match check_naming(&naming, lang) {
            Ok(naming) => s.naming = naming,
            Err(problem) => return tell(cx, problem.message).await,
        }
    }
    if let Some(max) = c.max_open {
        match check_open_limit(max, lang) {
            Ok(max) => s.max_open_per_user = max,
            Err(problem) => return tell(cx, problem.message).await,
        }
    }
    if s != before {
        cx.app.db.save_ticket_settings(guild, &s).await?;
    }
    let channel = |id: Option<i64>| id.map_or_else(|| t!(lang, "tickets.admin.unset"), |id| format!("<#{id}>"));
    let yes_no = |on: bool| t!(lang, if on { "tickets.admin.on" } else { "tickets.admin.off" });
    let hours = if s.auto_close_hours == 0 {
        t!(lang, "tickets.admin.never")
    } else {
        t!(lang, "tickets.admin.hours", count = s.auto_close_hours)
    };
    let field = |name: &str, value: String| EmbedFieldBuilder::new(t!(lang, name), value).inline();
    let embed = EmbedBuilder::new()
        .color(BRAND)
        .title(t!(lang, "tickets.admin.settings-title"))
        .description(if s != before {
            t!(lang, "tickets.admin.saved")
        } else {
            t!(
                lang,
                "tickets.admin.settings-hint",
                hours = AUTO_CLOSE_MAX,
                max = OPEN_LIMIT_MAX
            )
        })
        .field(field("tickets.admin.f-category", channel(s.open_category_id)))
        .field(field("tickets.admin.f-closed", channel(s.closed_category_id)))
        .field(field("tickets.admin.f-log", channel(s.log_channel_id)))
        .field(field("tickets.admin.f-auto", hours))
        .field(field("tickets.admin.f-dm", yes_no(s.dm_transcript)))
        .field(field("tickets.admin.f-ratings", yes_no(s.rating_enabled)))
        .field(field(
            "tickets.admin.f-mode",
            t!(lang, &format!("tickets.mode.{}", s.mode().key())),
        ))
        .field(field(
            "tickets.admin.f-naming",
            format!("`{}`", s.naming.replace('`', "'")),
        ))
        .field(field("tickets.admin.f-max", s.max_open_per_user.to_string()))
        .build();
    cx.reply(twilight_model::http::interaction::InteractionResponseData {
        embeds: Some(vec![embed]),
        flags: Some(twilight_model::channel::message::MessageFlags::EPHEMERAL),
        ..Default::default()
    })
    .await
}
