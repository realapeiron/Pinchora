//! QOTD, counting, forms and modmail interaction paths.
use crate::Cx;
use pinchora_core::t;
use twilight_interactions::command::{CommandModel, CreateCommand};
use twilight_model::{
    application::interaction::InteractionChannel,
    channel::message::{
        Component, MessageFlags,
        component::{ActionRow, Button, ButtonStyle, Label, TextInput, TextInputStyle},
    },
    guild::{Permissions, Role},
    http::interaction::InteractionResponseData,
};
use twilight_util::builder::embed::EmbedBuilder;

fn manage_guild() -> Permissions {
    Permissions::MANAGE_GUILD
}

#[derive(CommandModel, CreateCommand)]
#[command(name = "qotd", desc = "Suggest a question of the day", contexts = "guild")]
pub enum QotdCommand {
    #[command(name = "suggest")]
    Suggest(QotdSuggest),
}
#[derive(CommandModel, CreateCommand)]
#[command(name = "suggest", desc = "Suggest a question for staff review")]
pub struct QotdSuggest {
    /// Question, up to 300 characters
    #[command(max_length = 300)]
    question: String,
}

#[derive(CommandModel, CreateCommand)]
#[command(name = "counting", desc = "See your counting progress", contexts = "guild")]
pub enum CountingCommand {
    #[command(name = "stats")]
    Stats(CountingStats),
}
#[derive(CommandModel, CreateCommand)]
#[command(name = "stats", desc = "See your correct counts and failures")]
pub struct CountingStats;

#[derive(CommandModel, CreateCommand)]
#[command(name = "form", desc = "Open a server application", contexts = "guild")]
pub enum FormCommand {
    #[command(name = "list")]
    List(FormList),
    #[command(name = "open")]
    Open(FormOpen),
}
#[derive(CommandModel, CreateCommand)]
#[command(name = "list", desc = "List open application forms")]
pub struct FormList;
#[derive(CommandModel, CreateCommand)]
#[command(name = "open", desc = "Open an application form")]
pub struct FormOpen {
    /// Form ID
    id: i64,
}

#[derive(CommandModel, CreateCommand)]
#[command(name = "modmail", desc = "Handle a private support thread", contexts = "guild")]
pub enum ModmailCommand {
    #[command(name = "close")]
    Close(ModmailClose),
    #[command(name = "block")]
    Block(ModmailBlock),
    #[command(name = "snippet-add")]
    SnippetAdd(SnippetAdd),
    #[command(name = "snippet-use")]
    SnippetUse(SnippetUse),
    #[command(name = "snippet-list")]
    SnippetList(SnippetList),
}
#[derive(CommandModel, CreateCommand)]
#[command(name = "close", desc = "Close the current private support thread")]
pub struct ModmailClose {
    /// Reason shown to the member
    #[command(max_length = 300)]
    reason: Option<String>,
}
#[derive(CommandModel, CreateCommand)]
#[command(name = "block", desc = "Block a member from starting modmail")]
pub struct ModmailBlock {
    /// Member ID
    user: i64,
    /// Remove the block
    unblock: Option<bool>,
}
#[derive(CommandModel, CreateCommand)]
#[command(name = "snippet-add", desc = "Save a staff reply snippet")]
pub struct SnippetAdd {
    /// Short name
    #[command(max_length = 32)]
    name: String,
    /// Reply text
    #[command(max_length = 1000)]
    text: String,
}
#[derive(CommandModel, CreateCommand)]
#[command(name = "snippet-use", desc = "Send a saved reply to the member")]
pub struct SnippetUse {
    /// Snippet name
    #[command(max_length = 32)]
    name: String,
}
#[derive(CommandModel, CreateCommand)]
#[command(name = "snippet-list", desc = "List saved staff snippets")]
pub struct SnippetList;

#[derive(CommandModel, CreateCommand)]
#[command(
    name = "community",
    desc = "Community settings",
    default_permissions = "manage_guild",
    contexts = "guild"
)]
pub enum CommunityAdmin {
    #[command(name = "qotd")]
    Qotd(QotdAdmin),
    #[command(name = "counting")]
    Counting(CountingAdmin),
    #[command(name = "forms")]
    Forms(FormsAdmin),
    #[command(name = "modmail")]
    Modmail(ModmailAdmin),
}
#[derive(CommandModel, CreateCommand)]
#[command(name = "qotd", desc = "Daily questions")]
#[allow(clippy::large_enum_variant)] // parsed once per interaction
pub enum QotdAdmin {
    #[command(name = "settings")]
    Settings(QotdSettings),
    #[command(name = "queue")]
    Queue(QotdQueue),
}
#[derive(CommandModel, CreateCommand)]
#[command(name = "settings", desc = "Set channel and time")]
pub struct QotdSettings {
    /// Daily channel
    #[command(channel_types = "guild_text guild_announcement")]
    channel: Option<InteractionChannel>,
    /// Post hour
    #[command(min_value = 0, max_value = 23)]
    hour: Option<i64>,
    /// Time zone
    #[command(max_length = 64)]
    timezone: Option<String>,
    /// Create thread
    thread: Option<bool>,
    /// Ping role
    role: Option<Role>,
}
#[derive(CommandModel, CreateCommand)]
#[command(name = "queue", desc = "Review questions")]
pub struct QotdQueue;
#[derive(CommandModel, CreateCommand)]
#[command(name = "counting", desc = "Counting settings")]
#[allow(clippy::large_enum_variant)] // parsed once per interaction
pub enum CountingAdmin {
    #[command(name = "setup")]
    Setup(CountingSetup),
    #[command(name = "reset")]
    Reset(CountingReset),
}
#[derive(CommandModel, CreateCommand)]
#[command(name = "setup", desc = "Set counting rules")]
pub struct CountingSetup {
    /// Counting channel
    #[command(channel_types = "guild_text")]
    channel: InteractionChannel,
    /// Reset on fail
    reset_on_fail: Option<bool>,
    /// Allow math
    allow_math: Option<bool>,
    /// Fail role
    fail_role: Option<Role>,
}
#[derive(CommandModel, CreateCommand)]
#[command(name = "reset", desc = "Reset count")]
pub struct CountingReset;

#[derive(CommandModel, CreateCommand)]
#[command(name = "forms", desc = "Application forms")]
#[allow(clippy::large_enum_variant)] // parsed once per interaction
pub enum FormsAdmin {
    #[command(name = "create")]
    Create(FormCreate),
    #[command(name = "post")]
    Post(FormPost),
    #[command(name = "close")]
    Close(FormClose),
}
#[derive(CommandModel, CreateCommand)]
#[command(name = "create", desc = "Create a form")]
pub struct FormCreate {
    /// Form name
    #[command(max_length = 45)]
    name: String,
    /// Questions, separated by |
    #[command(max_length = 500)]
    questions: String,
    /// Review channel
    #[command(channel_types = "guild_text")]
    review_channel: InteractionChannel,
    /// Accept role
    accept_role: Option<Role>,
}
#[derive(CommandModel, CreateCommand)]
#[command(name = "post", desc = "Post form")]
pub struct FormPost {
    /// Form ID
    id: i64,
}
#[derive(CommandModel, CreateCommand)]
#[command(name = "close", desc = "Close a form")]
pub struct FormClose {
    /// Form ID
    id: i64,
}

#[derive(CommandModel, CreateCommand)]
#[command(name = "modmail", desc = "Private mail")]
pub enum ModmailAdmin {
    #[command(name = "settings")]
    Settings(ModmailSettingsInput),
}
#[derive(CommandModel, CreateCommand)]
#[command(name = "settings", desc = "Set mail forum")]
pub struct ModmailSettingsInput {
    /// Enable mail
    enabled: Option<bool>,
    /// Staff forum
    #[command(channel_types = "guild_forum")]
    forum: Option<InteractionChannel>,
    /// Greeting
    #[command(max_length = 500)]
    greeting: Option<String>,
    /// Anonymous staff
    anonymous_staff: Option<bool>,
    /// Staff role
    staff_role: Option<Role>,
    /// Log channel
    #[command(channel_types = "guild_text guild_announcement")]
    log_channel: Option<InteractionChannel>,
    /// Idle close hours
    #[command(min_value = 1, max_value = 8760)]
    close_after_hours: Option<i64>,
}

impl QotdCommand {
    pub async fn run(self, cx: &Cx) -> anyhow::Result<()> {
        let Some(guild) = cx.guild_id() else {
            return cx.reply_ephemeral(t!(cx.lang(), "error.guild-only")).await;
        };
        let Self::Suggest(s) = self;
        match cx
            .app
            .db
            .suggest_qotd(guild, cx.author()?.id.get(), &s.question)
            .await?
        {
            Some(_) => cx.reply_ephemeral(t!(cx.lang(), "community-pack.qotd-suggested")).await,
            None => cx.reply_ephemeral(t!(cx.lang(), "community-pack.qotd-full")).await,
        }
    }
}
impl CountingCommand {
    pub async fn run(self, cx: &Cx) -> anyhow::Result<()> {
        let Some(guild) = cx.guild_id() else {
            return cx.reply_ephemeral(t!(cx.lang(), "error.guild-only")).await;
        };
        let (correct, fails) = cx.app.db.counting_stats(guild, cx.author()?.id.get()).await?;
        let current = cx.app.db.counting_for_guild(guild).await?;
        cx.reply_ephemeral(t!(
            cx.lang(),
            "community-pack.count-stats",
            correct = correct,
            fails = fails,
            current = current.as_ref().map_or(0, |r| r.current),
            record = current.as_ref().map_or(0, |r| r.record)
        ))
        .await
    }
}
impl FormCommand {
    pub async fn run(self, cx: &Cx) -> anyhow::Result<()> {
        let Some(guild) = cx.guild_id() else {
            return cx.reply_ephemeral(t!(cx.lang(), "error.guild-only")).await;
        };
        match self {
            Self::List(_) => {
                let rows = cx.app.db.forms(guild).await?;
                let list = rows
                    .iter()
                    .filter(|f| f.open)
                    .map(|f| format!("{} · {}", f.id, f.name))
                    .collect::<Vec<_>>()
                    .join("\n");
                cx.reply_ephemeral(if list.is_empty() {
                    t!(cx.lang(), "community-pack.no-forms")
                } else {
                    list
                })
                .await
            }
            Self::Open(f) => open_form(cx, guild, f.id).await,
        }
    }
}
#[allow(deprecated)] // Twilight still requires the legacy field on its TextInput model.
async fn open_form(cx: &Cx, guild: u64, id: i64) -> anyhow::Result<()> {
    let Some(form) = cx.app.db.form(guild, id).await?.filter(|f| f.open) else {
        return cx.reply_ephemeral(t!(cx.lang(), "community-pack.form-missing")).await;
    };
    if !pinchora_core::community::valid_form(&form.name, &form.questions) {
        return cx.reply_ephemeral(t!(cx.lang(), "community-pack.form-invalid")).await;
    }
    let questions = form.questions.as_array().cloned().unwrap_or_default();
    let mut components = Vec::new();
    for (index, question) in questions.iter().take(5).enumerate() {
        let label = question.as_str().unwrap_or_default();
        components.push(Component::Label(Label {
            id: None,
            label: label.chars().take(45).collect(),
            description: None,
            component: Box::new(Component::TextInput(TextInput {
                custom_id: format!("a{index}"),
                id: None,
                style: TextInputStyle::Paragraph,
                label: None,
                min_length: Some(1),
                max_length: Some(1000),
                placeholder: None,
                required: Some(true),
                value: None,
            })),
        }));
    }
    cx.modal(format!("fm:s:{id}"), &form.name, components).await
}
pub async fn on_component(cx: &Cx, mut parts: std::str::Split<'_, char>) -> anyhow::Result<()> {
    match parts.next() {
        Some("o") => {
            let id = parts.next().and_then(|x| x.parse().ok()).unwrap_or(0);
            open_form(cx, cx.guild_id().unwrap_or(0), id).await
        }
        Some(decision @ ("a" | "d")) => {
            let id = parts.next().and_then(|x| x.parse::<i64>().ok()).unwrap_or(0);
            let row: Option<(i64, Vec<i64>)> = sqlx::query_as(
                "SELECT f.guild_id,f.review_roles FROM form_submissions s JOIN forms f ON f.id=s.form_id WHERE s.id=$1",
            )
            .bind(id)
            .fetch_optional(cx.app.db.pool())
            .await?;
            let Some((guild, review_roles)) = row else {
                return cx.reply_ephemeral(t!(cx.lang(), "community-pack.form-missing")).await;
            };
            if cx.guild_id() != Some(guild as u64)
                || !(cx.can_manage_guild()
                    || cx
                        .interaction
                        .member
                        .as_ref()
                        .is_some_and(|m| m.roles.iter().any(|r| review_roles.contains(&(r.get() as i64)))))
            {
                return cx.reply_ephemeral(t!(cx.lang(), "community-pack.admin-only")).await;
            }
            let Some((user, guild, role, dm_result)) = cx
                .app
                .db
                .decide_form(guild as u64, id, cx.author()?.id.get(), decision == "a")
                .await?
            else {
                return cx.reply_ephemeral(t!(cx.lang(), "community-pack.form-missing")).await;
            };
            if decision == "a"
                && let Some(role) = role
                && let Err(error) = crate::commands::roles_pack::give_safe_role(
                    &cx.app,
                    guild as u64,
                    user as u64,
                    role as u64,
                    "Form accepted",
                )
                .await
            {
                tracing::warn!(%error,guild,"form accept role could not be granted");
            }
            if dm_result
                && !cx.app.db.dm_opt_out(user as u64).await?
                && let Ok(dm) = cx
                    .app
                    .http
                    .create_private_channel(twilight_model::id::Id::new(user as u64))
                    .await
                && let Ok(dm) = dm.model().await
            {
                let key = if decision == "a" {
                    "community-pack.form-accepted"
                } else {
                    "community-pack.form-denied"
                };
                let lang = crate::lang::resolve_text(&cx.app, user as u64, guild as u64).await;
                let _ = cx.app.http.create_message(dm.id).content(&t!(lang, key)).await;
            }
            cx.reply_ephemeral(t!(cx.lang(), "community-pack.form-decided")).await
        }
        _ => cx.reply_ephemeral(t!(cx.lang(), "component.button-expired")).await,
    }
}
pub async fn on_mail_select(cx: &Cx, values: &[String]) -> anyhow::Result<()> {
    let guild = values.first().and_then(|v| v.parse::<u64>().ok()).unwrap_or(0);
    let user = cx.author()?.id.get();
    let body: Option<String> = sqlx::query_scalar(
        "DELETE FROM modmail_pending WHERE user_id=$1 AND created_at>now()-INTERVAL '15 minutes' RETURNING body",
    )
    .bind(user as i64)
    .fetch_optional(cx.app.db.pool())
    .await?;
    let Some(body) = body else {
        return cx.reply_ephemeral(t!(cx.lang(), "component.button-expired")).await;
    };
    let allowed = cx.app.db.modmail_settings(guild).await?.is_some_and(|s| s.enabled)
        && cx
            .app
            .http
            .guild_member(twilight_model::id::Id::new(guild.max(1)), cx.author()?.id)
            .await
            .is_ok();
    if !allowed {
        return cx.reply_ephemeral(t!(cx.lang(), "community-pack.modmail-denied")).await;
    }
    let delivered = crate::community_pack::route_dm(&cx.app, guild, user, &body).await?;
    cx.reply_ephemeral(t!(
        cx.lang(),
        if delivered {
            "community-pack.modmail-greeting"
        } else {
            "community-pack.modmail-denied"
        }
    ))
    .await
}
pub async fn on_qotd_decision(cx: &Cx, mut parts: std::str::Split<'_, char>) -> anyhow::Result<()> {
    let Some(guild) = cx.guild_id() else {
        return cx.reply_ephemeral(t!(cx.lang(), "error.guild-only")).await;
    };
    if !cx.can_manage_guild() {
        return cx.reply_ephemeral(t!(cx.lang(), "community-pack.admin-only")).await;
    }
    let approve = matches!(parts.next(), Some("ok"));
    let id = parts.next().and_then(|s| s.parse::<i64>().ok()).unwrap_or(0);
    let changed = cx.app.db.decide_qotd(guild, id, approve).await?;
    cx.reply_ephemeral(t!(
        cx.lang(),
        if changed {
            "community-pack.qotd-decided"
        } else {
            "community-pack.qotd-missing"
        }
    ))
    .await
}
pub async fn on_modal(
    cx: &Cx,
    mut parts: std::str::Split<'_, char>,
    fields: &[(String, String)],
) -> anyhow::Result<()> {
    let Some("s") = parts.next() else {
        return cx.reply_ephemeral(t!(cx.lang(), "component.button-expired")).await;
    };
    let id = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);
    let Some(answers) = pinchora_core::community::form_answers(fields) else {
        return cx.reply_ephemeral(t!(cx.lang(), "community-pack.form-invalid")).await;
    };
    match cx
        .app
        .db
        .submit_form(cx.guild_id().unwrap_or(0), id, cx.author()?.id.get(), &answers)
        .await?
    {
        Some(submission) => {
            if let Some(guild) = cx.guild_id()
                && let Some(form) = cx.app.db.form(guild, id).await?
                && let Some(channel) = form.submit_channel_id
            {
                let body = answers
                    .as_array()
                    .unwrap()
                    .iter()
                    .enumerate()
                    .map(|(i, value)| {
                        format!(
                            "**{}**\n{}",
                            form.questions.get(i).and_then(|x| x.as_str()).unwrap_or_default(),
                            value.as_str().unwrap_or_default().chars().take(600).collect::<String>()
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n\n");
                let content = format!("<@{}>", cx.author()?.id.get());
                let embed = EmbedBuilder::new()
                    .title(t!(cx.lang(), "community-pack.review-title", id = submission))
                    .description(body)
                    .build();
                let row = Component::ActionRow(ActionRow {
                    id: None,
                    components: vec![
                        Component::Button(Button {
                            custom_id: Some(format!("fm:a:{submission}")),
                            disabled: false,
                            emoji: None,
                            id: None,
                            label: Some(t!(cx.lang(), "community-pack.accept")),
                            sku_id: None,
                            style: ButtonStyle::Success,
                            url: None,
                        }),
                        Component::Button(Button {
                            custom_id: Some(format!("fm:d:{submission}")),
                            disabled: false,
                            emoji: None,
                            id: None,
                            label: Some(t!(cx.lang(), "community-pack.deny")),
                            sku_id: None,
                            style: ButtonStyle::Danger,
                            url: None,
                        }),
                    ],
                });
                let mentions = twilight_model::channel::message::AllowedMentions::default();
                let _ = cx
                    .app
                    .http
                    .create_message(twilight_model::id::Id::new(channel as u64))
                    .content(&content)
                    .embeds(&[cx.app.brand_embed(guild, embed).await])
                    .components(&[row])
                    .allowed_mentions(Some(&mentions))
                    .await;
            }
            cx.reply_ephemeral(t!(cx.lang(), "community-pack.form-submitted")).await
        }
        None => cx.reply_ephemeral(t!(cx.lang(), "community-pack.form-missing")).await,
    }
}

impl CommunityAdmin {
    pub async fn run(self, cx: &Cx) -> anyhow::Result<()> {
        let Some(guild) = cx.guild_id() else {
            return cx.reply_ephemeral(t!(cx.lang(), "error.guild-only")).await;
        };
        if !cx.can_manage_guild() {
            return cx.reply_ephemeral(t!(cx.lang(), "community-pack.admin-only")).await;
        }
        match self {
            Self::Qotd(action) => match action {
                QotdAdmin::Settings(input) => {
                    if input
                        .timezone
                        .as_ref()
                        .is_some_and(|zone| zone.parse::<chrono_tz::Tz>().is_err())
                    {
                        return cx.reply_ephemeral(t!(cx.lang(), "birthdays.invalid-zone")).await;
                    }
                    sqlx::query("INSERT INTO qotd_settings(guild_id,channel_id,ping_role_id,hour,timezone,thread_per_question) VALUES($1,$2,$3,COALESCE($4,12),COALESCE($5,'UTC'),COALESCE($6,false)) ON CONFLICT(guild_id) DO UPDATE SET channel_id=COALESCE($2,qotd_settings.channel_id),ping_role_id=COALESCE($3,qotd_settings.ping_role_id),hour=COALESCE($4,qotd_settings.hour),timezone=COALESCE($5,qotd_settings.timezone),thread_per_question=COALESCE($6,qotd_settings.thread_per_question)")
                        .bind(guild as i64).bind(input.channel.map(|c|c.id.get() as i64))
                        .bind(input.role.map(|r|r.id.get() as i64)).bind(input.hour.map(|hour| hour as i16))
                        .bind(input.timezone).bind(input.thread).execute(cx.app.db.pool()).await?;
                    cx.app.db.seed_qotd(guild).await?;
                    cx.reply_ephemeral(t!(cx.lang(), "community-pack.qotd-settings")).await
                }
                QotdAdmin::Queue(_) => {
                    let queue = cx.app.db.qotd_queue(guild).await?;
                    let items = queue
                        .iter()
                        .take(5)
                        .map(|q| {
                            format!(
                                "#{} {} {}",
                                q.id,
                                if q.approved { "✓" } else { "?" },
                                if cx.lang() == pinchora_core::i18n::Lang::Tr {
                                    q.text_tr.as_deref().unwrap_or(&q.text)
                                } else {
                                    &q.text
                                }
                            )
                        })
                        .collect::<Vec<_>>()
                        .join("\n");
                    let pending = queue.iter().filter(|q| !q.approved).take(5);
                    let rows = pending
                        .map(|q| {
                            Component::ActionRow(ActionRow {
                                id: None,
                                components: vec![
                                    Component::Button(Button {
                                        custom_id: Some(format!("qd:ok:{}", q.id)),
                                        disabled: false,
                                        emoji: None,
                                        id: None,
                                        label: Some(format!("✓ #{}", q.id)),
                                        sku_id: None,
                                        style: ButtonStyle::Success,
                                        url: None,
                                    }),
                                    Component::Button(Button {
                                        custom_id: Some(format!("qd:no:{}", q.id)),
                                        disabled: false,
                                        emoji: None,
                                        id: None,
                                        label: Some(format!("✕ #{}", q.id)),
                                        sku_id: None,
                                        style: ButtonStyle::Danger,
                                        url: None,
                                    }),
                                ],
                            })
                        })
                        .collect();
                    cx.reply(InteractionResponseData {
                        content: Some(t!(cx.lang(), "community-pack.qotd-queue", items = items)),
                        components: Some(rows),
                        flags: Some(MessageFlags::EPHEMERAL),
                        ..Default::default()
                    })
                    .await
                }
            },
            Self::Counting(action) => match action {
                CountingAdmin::Setup(input) => {
                    if let Some(ref role) = input.fail_role
                        && let Some(reason) = crate::roles::unassignable(cx, guild, role).await
                    {
                        return cx.reply_ephemeral(reason).await;
                    }
                    cx.app
                        .db
                        .save_counting(
                            guild,
                            input.channel.id.get(),
                            input.reset_on_fail.unwrap_or(true),
                            input.allow_math.unwrap_or(false),
                            input.fail_role.map(|r| r.id.get()),
                        )
                        .await?;
                    cx.reply_ephemeral(t!(
                        cx.lang(),
                        "community-pack.count-set",
                        channel = input.channel.id.get()
                    ))
                    .await
                }
                CountingAdmin::Reset(_) => {
                    let changed = cx.app.db.reset_counting(guild).await?;
                    cx.reply_ephemeral(t!(
                        cx.lang(),
                        if changed {
                            "community-pack.count-reset"
                        } else {
                            "community-pack.count-unset"
                        }
                    ))
                    .await
                }
            },
            Self::Forms(action) => match action {
                FormsAdmin::Create(input) => {
                    let questions: Vec<_> = input
                        .questions
                        .split('|')
                        .map(str::trim)
                        .filter(|s| !s.is_empty())
                        .collect();
                    if questions.is_empty() || questions.len() > 5 || questions.iter().any(|s| s.chars().count() > 45) {
                        return cx.reply_ephemeral(t!(cx.lang(), "community-pack.form-invalid")).await;
                    }
                    if let Some(ref role) = input.accept_role
                        && let Some(reason) = crate::roles::unassignable(cx, guild, role).await
                    {
                        return cx.reply_ephemeral(reason).await;
                    }
                    let Some(id) = cx
                        .app
                        .db
                        .save_form(&pinchora_db::FormRow {
                            id: 0,
                            guild_id: guild as i64,
                            name: input.name,
                            questions: serde_json::json!(questions),
                            submit_channel_id: Some(input.review_channel.id.get() as i64),
                            review_roles: vec![],
                            accept_role_id: input.accept_role.map(|r| r.id.get() as i64),
                            dm_result: true,
                            open: true,
                            message_id: None,
                        })
                        .await?
                    else {
                        return cx.reply_ephemeral(t!(cx.lang(), "community-pack.form-invalid")).await;
                    };
                    cx.reply_ephemeral(t!(cx.lang(), "community-pack.form-created", id = id))
                        .await
                }
                FormsAdmin::Post(input) => {
                    let Some(form) = cx.app.db.form(guild, input.id).await?.filter(|f| f.open) else {
                        return cx.reply_ephemeral(t!(cx.lang(), "community-pack.form-missing")).await;
                    };
                    let channel = cx.interaction.channel.as_ref().map(|c| c.id);
                    let Some(channel) = channel else {
                        return cx.reply_ephemeral(t!(cx.lang(), "community-pack.form-missing")).await;
                    };
                    let row = Component::ActionRow(ActionRow {
                        id: None,
                        components: vec![Component::Button(Button {
                            custom_id: Some(format!("fm:o:{}", form.id)),
                            disabled: false,
                            emoji: None,
                            id: None,
                            label: Some(form.name.clone()),
                            sku_id: None,
                            style: ButtonStyle::Primary,
                            url: None,
                        })],
                    });
                    let response = cx
                        .app
                        .http
                        .create_message(channel)
                        .content(&form.name)
                        .components(&[row])
                        .await?;
                    let message = response.model().await?;
                    sqlx::query("UPDATE forms SET message_id=$2 WHERE id=$1")
                        .bind(form.id)
                        .bind(message.id.get() as i64)
                        .execute(cx.app.db.pool())
                        .await?;
                    cx.reply_ephemeral(t!(cx.lang(), "community-pack.form-posted")).await
                }
                FormsAdmin::Close(input) => {
                    sqlx::query("UPDATE forms SET open=FALSE WHERE guild_id=$1 AND id=$2")
                        .bind(guild as i64)
                        .bind(input.id)
                        .execute(cx.app.db.pool())
                        .await?;
                    cx.reply_ephemeral(t!(cx.lang(), "community-pack.form-closed")).await
                }
            },
            Self::Modmail(ModmailAdmin::Settings(input)) => {
                sqlx::query("INSERT INTO modmail_settings(guild_id,enabled,forum_channel_id,greeting,anonymous_staff,staff_roles,close_after_hours,log_channel_id) VALUES($1,COALESCE($2,false),$3,COALESCE($4,''),COALESCE($5,false),COALESCE($6,'{}'::bigint[]),COALESCE($7,72),$8) ON CONFLICT(guild_id) DO UPDATE SET enabled=COALESCE($2,modmail_settings.enabled),forum_channel_id=COALESCE($3,modmail_settings.forum_channel_id),greeting=COALESCE($4,modmail_settings.greeting),anonymous_staff=COALESCE($5,modmail_settings.anonymous_staff),staff_roles=COALESCE($6,modmail_settings.staff_roles),close_after_hours=COALESCE($7,modmail_settings.close_after_hours),log_channel_id=COALESCE($8,modmail_settings.log_channel_id)")
                    .bind(guild as i64).bind(input.enabled)
                    .bind(input.forum.map(|c|c.id.get() as i64)).bind(input.greeting)
                    .bind(input.anonymous_staff)
                    .bind(input.staff_role.map(|r|vec![r.id.get() as i64]))
                    .bind(input.close_after_hours.map(|h|h as i32))
                    .bind(input.log_channel.map(|c|c.id.get() as i64))
                    .execute(cx.app.db.pool()).await?;
                cx.reply_ephemeral(t!(cx.lang(), "community-pack.modmail-settings"))
                    .await
            }
        }
    }
}

impl ModmailCommand {
    pub async fn run(self, cx: &Cx) -> anyhow::Result<()> {
        let Some(guild) = cx.guild_id() else {
            return cx.reply_ephemeral(t!(cx.lang(), "error.guild-only")).await;
        };
        let staff = cx.app.db.modmail_settings(guild).await?.is_some_and(|settings| {
            cx.interaction
                .member
                .as_ref()
                .is_some_and(|m| m.roles.iter().any(|r| settings.staff_roles.contains(&(r.get() as i64))))
        });
        if !cx.can_manage_guild() && !staff {
            return cx.reply_ephemeral(t!(cx.lang(), "community-pack.admin-only")).await;
        }
        match self {
            Self::Close(input) => {
                let channel = cx.interaction.channel.as_ref().map(|c| c.id.get()).unwrap_or(0);
                let Some(thread) = cx.app.db.modmail_thread(channel).await? else {
                    return cx
                        .reply_ephemeral(t!(cx.lang(), "community-pack.modmail-not-thread"))
                        .await;
                };
                if thread.guild_id != guild as i64 {
                    return cx
                        .reply_ephemeral(t!(cx.lang(), "community-pack.modmail-not-thread"))
                        .await;
                }
                crate::community_pack::close_thread(
                    &cx.app,
                    guild,
                    channel,
                    cx.author()?.id.get(),
                    input.reason.as_deref(),
                )
                .await?;
                cx.reply_ephemeral(t!(cx.lang(), "community-pack.modmail-closed")).await
            }
            Self::Block(input) => {
                if input.user <= 0 {
                    return cx.reply_ephemeral(t!(cx.lang(), "community-pack.modmail-denied")).await;
                }
                if input.unblock.unwrap_or(false) {
                    sqlx::query("DELETE FROM modmail_blocks WHERE guild_id=$1 AND user_id=$2")
                        .bind(guild as i64)
                        .bind(input.user)
                        .execute(cx.app.db.pool())
                        .await?;
                } else {
                    sqlx::query("INSERT INTO modmail_blocks(guild_id,user_id) VALUES($1,$2) ON CONFLICT DO NOTHING")
                        .bind(guild as i64)
                        .bind(input.user)
                        .execute(cx.app.db.pool())
                        .await?;
                }
                cx.reply_ephemeral(t!(cx.lang(), "community-pack.modmail-blocked"))
                    .await
            }
            Self::SnippetAdd(input) => {
                sqlx::query("INSERT INTO modmail_snippets(guild_id,name,text) VALUES($1,$2,$3) ON CONFLICT(guild_id,name) DO UPDATE SET text=$3")
                    .bind(guild as i64).bind(input.name).bind(input.text).execute(cx.app.db.pool()).await?;
                cx.reply_ephemeral(t!(cx.lang(), "community-pack.modmail-snippet-saved"))
                    .await
            }
            Self::SnippetList(_) => {
                let names: Vec<String> =
                    sqlx::query_scalar("SELECT name FROM modmail_snippets WHERE guild_id=$1 ORDER BY name LIMIT 50")
                        .bind(guild as i64)
                        .fetch_all(cx.app.db.pool())
                        .await?;
                cx.reply_ephemeral(t!(
                    cx.lang(),
                    "community-pack.modmail-snippet-list",
                    items = names.join(", ")
                ))
                .await
            }
            Self::SnippetUse(input) => {
                let channel = cx.interaction.channel.as_ref().map(|c| c.id.get()).unwrap_or(0);
                let Some(thread) = cx.app.db.modmail_thread(channel).await? else {
                    return cx
                        .reply_ephemeral(t!(cx.lang(), "community-pack.modmail-not-thread"))
                        .await;
                };
                if thread.guild_id != guild as i64 {
                    return cx
                        .reply_ephemeral(t!(cx.lang(), "community-pack.modmail-not-thread"))
                        .await;
                }
                let text: Option<String> =
                    sqlx::query_scalar("SELECT text FROM modmail_snippets WHERE guild_id=$1 AND name=$2")
                        .bind(guild as i64)
                        .bind(input.name)
                        .fetch_optional(cx.app.db.pool())
                        .await?;
                let Some(text) = text else {
                    return cx
                        .reply_ephemeral(t!(cx.lang(), "community-pack.modmail-snippet-missing"))
                        .await;
                };
                let dm = cx
                    .app
                    .http
                    .create_private_channel(twilight_model::id::Id::new(thread.user_id as u64))
                    .await?
                    .model()
                    .await?;
                cx.app
                    .http
                    .create_message(dm.id)
                    .content(&text)
                    .allowed_mentions(Some(&twilight_model::channel::message::AllowedMentions::default()))
                    .await?;
                cx.reply_ephemeral(t!(cx.lang(), "community-pack.modmail-snippet-saved"))
                    .await
            }
        }
    }
}
