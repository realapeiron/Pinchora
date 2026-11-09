//! Personal community tools and moderated server features.

use std::{sync::Arc, time::Duration};

use chrono::{DateTime, Datelike, Utc};
use pinchora_core::t;
use twilight_interactions::command::{CommandModel, CreateCommand};
use twilight_model::{
    application::interaction::InteractionChannel,
    channel::{
        ChannelType, Message,
        message::{Component, component::ButtonStyle},
    },
    guild::Permissions,
    id::Id,
};
use twilight_util::builder::embed::EmbedBuilder;

use crate::App;
use crate::Cx;
use crate::commands::games::games::quiet;
use crate::community::server_events::button;
use crate::community::server_events::row;
use crate::community::server_events::tell;
use crate::infra::util::strip_mentions;
use crate::progression::leveling::private_channel;

fn manage_guild() -> Permissions {
    Permissions::MANAGE_GUILD
}

async fn private_review_channel(app: &App, guild: u64, channel: u64) -> anyhow::Result<bool> {
    let facts = app.permission_facts(guild).await?;
    let rules = app.channel_rules(channel).await?;
    let base = facts
        .roles
        .iter()
        .find(|role| role.id.get() == guild)
        .map_or(Permissions::empty(), |role| role.permissions);
    Ok(
        !crate::infra::text::access::channel_permissions(base, guild, 0, &[], &rules.overwrites)
            .contains(Permissions::VIEW_CHANNEL),
    )
}

#[derive(CommandModel, CreateCommand)]
#[command(
    name = "highlight",
    desc = "Follow words mentioned in this server",
    contexts = "guild"
)]
pub enum HighlightCommand {
    #[command(name = "add")]
    Add(HighlightWord),
    #[command(name = "remove")]
    Remove(HighlightRemove),
    #[command(name = "list")]
    List(HighlightList),
}
#[derive(CommandModel, CreateCommand)]
#[command(name = "add", desc = "DM me when this word is said")]
pub struct HighlightWord {
    /// Word to follow
    #[command(min_length = 2, max_length = 40)]
    word: String,
}
#[derive(CommandModel, CreateCommand)]
#[command(name = "remove", desc = "Stop following a word")]
pub struct HighlightRemove {
    /// Word to remove
    #[command(min_length = 2, max_length = 40)]
    word: String,
}
#[derive(CommandModel, CreateCommand)]
#[command(name = "list", desc = "Words I follow in this server")]
pub struct HighlightList;

impl HighlightCommand {
    pub async fn run(self, cx: &Cx) -> anyhow::Result<()> {
        let Some(guild) = cx.guild_id() else {
            return tell(cx, t!(cx.lang(), "community-v2.guild-only")).await;
        };
        let user = cx.author()?.id.get() as i64;
        match self {
            Self::Add(c) => {
                if !cx.app.message_content {
                    return tell(cx, t!(cx.lang(), "community-v2.content-intent")).await;
                }
                let word = c.word.trim().to_lowercase();
                if word.chars().count() < 2 || word.chars().count() > 40 || !word.chars().all(char::is_alphanumeric) {
                    return tell(cx, t!(cx.lang(), "community-v2.word-invalid")).await;
                }
                if !cx.app.db.add_keyword_highlight(guild, user as u64, &word).await? {
                    return tell(cx, t!(cx.lang(), "community-v2.highlight-full")).await;
                }
                tell(cx, t!(cx.lang(), "community-v2.highlight-added", word = word)).await
            }
            Self::Remove(c) => {
                sqlx::query("DELETE FROM keyword_highlights WHERE guild_id=$1 AND user_id=$2 AND keyword=$3")
                    .bind(guild as i64)
                    .bind(user)
                    .bind(c.word.trim().to_lowercase())
                    .execute(cx.app.db.pool())
                    .await?;
                tell(cx, t!(cx.lang(), "community-v2.highlight-removed")).await
            }
            Self::List(_) => {
                let words: Vec<(String,)> = sqlx::query_as(
                    "SELECT keyword FROM keyword_highlights WHERE guild_id=$1 AND user_id=$2 ORDER BY keyword",
                )
                .bind(guild as i64)
                .bind(user)
                .fetch_all(cx.app.db.pool())
                .await?;
                let list = words.into_iter().map(|r| r.0).collect::<Vec<_>>().join(", ");
                tell(
                    cx,
                    t!(
                        cx.lang(),
                        "community-v2.highlight-list",
                        words = if list.is_empty() { "—" } else { &list }
                    ),
                )
                .await
            }
        }
    }
}

#[derive(CommandModel, CreateCommand)]
#[command(name = "checklist", desc = "A private personal checklist")]
pub enum ChecklistCommand {
    #[command(name = "add")]
    Add(ChecklistAdd),
    #[command(name = "list")]
    List(ChecklistList),
    #[command(name = "done")]
    Done(ChecklistId),
    #[command(name = "remove")]
    Remove(ChecklistRemove),
}
#[derive(CommandModel, CreateCommand)]
#[command(name = "add", desc = "Add a task to your checklist")]
pub struct ChecklistAdd {
    /// Task text
    #[command(min_length = 1, max_length = 200)]
    text: String,
}
#[derive(CommandModel, CreateCommand)]
#[command(name = "list", desc = "Show your checklist")]
pub struct ChecklistList;
#[derive(CommandModel, CreateCommand)]
#[command(name = "done", desc = "Mark one item complete")]
pub struct ChecklistId {
    /// Item number
    id: i64,
}
#[derive(CommandModel, CreateCommand)]
#[command(name = "remove", desc = "Remove a checklist item")]
pub struct ChecklistRemove {
    /// Item number
    id: i64,
}

impl ChecklistCommand {
    pub async fn run(self, cx: &Cx) -> anyhow::Result<()> {
        let user = cx.author()?.id.get() as i64;
        match self {
            Self::Add(c) => {
                let text = strip_mentions(&c.text);
                if text.trim().is_empty() {
                    return tell(cx, t!(cx.lang(), "community-v2.text-required")).await;
                }
                let Some(id) = cx.app.db.add_checklist_item(user as u64, &text).await? else {
                    return tell(cx, t!(cx.lang(), "community-v2.checklist-full")).await;
                };
                tell(cx, t!(cx.lang(), "community-v2.checklist-added", id = id)).await
            }
            Self::List(_) => {
                let rows: Vec<(i64, String, bool)> = sqlx::query_as(
                    "SELECT id,body,done FROM personal_checklist_items WHERE user_id=$1 ORDER BY done,id LIMIT 50",
                )
                .bind(user)
                .fetch_all(cx.app.db.pool())
                .await?;
                let lines = rows
                    .iter()
                    .map(|(id, body, done)| format!("{} `#{id}` {body}", if *done { "☑" } else { "☐" }))
                    .collect::<Vec<_>>()
                    .join("\n");
                tell(
                    cx,
                    if lines.is_empty() {
                        t!(cx.lang(), "community-v2.checklist-empty")
                    } else {
                        lines.chars().take(1900).collect()
                    },
                )
                .await
            }
            Self::Done(c) => {
                sqlx::query("UPDATE personal_checklist_items SET done=TRUE WHERE id=$1 AND user_id=$2")
                    .bind(c.id)
                    .bind(user)
                    .execute(cx.app.db.pool())
                    .await?;
                tell(cx, t!(cx.lang(), "community-v2.checklist-done")).await
            }
            Self::Remove(c) => {
                sqlx::query("DELETE FROM personal_checklist_items WHERE id=$1 AND user_id=$2")
                    .bind(c.id)
                    .bind(user)
                    .execute(cx.app.db.pool())
                    .await?;
                tell(cx, t!(cx.lang(), "community-v2.checklist-removed")).await
            }
        }
    }
}

#[derive(CommandModel, CreateCommand)]
#[command(
    name = "autopost",
    desc = "Automatically thread or crosspost channel messages",
    default_permissions = "manage_guild",
    contexts = "guild"
)]
pub enum AutopostCommand {
    #[command(name = "set")]
    Set(AutopostSet),
    #[command(name = "remove")]
    Remove(AutopostRemove),
    #[command(name = "list")]
    List(AutopostList),
}
#[derive(CommandModel, CreateCommand)]
#[command(name = "set", desc = "Set automatic actions in a channel")]
pub struct AutopostSet {
    /// Channel to configure
    #[command(channel_types = "guild_text guild_announcement")]
    channel: InteractionChannel,
    /// Create a discussion thread for each new message
    thread: bool,
    /// Crosspost each announcement
    crosspost: bool,
}
#[derive(CommandModel, CreateCommand)]
#[command(name = "remove", desc = "Stop automatic actions in a channel")]
pub struct AutopostRemove {
    /// Channel to stop
    #[command(channel_types = "guild_text guild_announcement")]
    channel: InteractionChannel,
}
#[derive(CommandModel, CreateCommand)]
#[command(name = "list", desc = "Show automatic channel actions")]
pub struct AutopostList;

impl AutopostCommand {
    pub async fn run(self, cx: &Cx) -> anyhow::Result<()> {
        let Some(guild) = cx.guild_id() else {
            return tell(cx, t!(cx.lang(), "community-v2.guild-only")).await;
        };
        if !cx.can_manage_guild() {
            return tell(cx, t!(cx.lang(), "community-v2.manage-only")).await;
        }
        match self {
            Self::Set(c) => {
                if !c.thread && !c.crosspost {
                    return tell(cx, t!(cx.lang(), "community-v2.action-required")).await;
                }
                if c.crosspost && c.channel.kind != ChannelType::GuildAnnouncement {
                    return tell(cx, t!(cx.lang(), "community-v2.announcement-only")).await;
                }
                sqlx::query("INSERT INTO community_auto_channels(guild_id,channel_id,auto_thread,auto_crosspost) VALUES($1,$2,$3,$4) ON CONFLICT(guild_id,channel_id) DO UPDATE SET auto_thread=$3,auto_crosspost=$4")
                    .bind(guild as i64).bind(c.channel.id.get() as i64).bind(c.thread).bind(c.crosspost)
                    .execute(cx.app.db.pool()).await?;
                tell(cx, t!(cx.lang(), "community-v2.autopost-saved")).await
            }
            Self::Remove(c) => {
                sqlx::query("DELETE FROM community_auto_channels WHERE guild_id=$1 AND channel_id=$2")
                    .bind(guild as i64)
                    .bind(c.channel.id.get() as i64)
                    .execute(cx.app.db.pool())
                    .await?;
                tell(cx, t!(cx.lang(), "community-v2.autopost-removed")).await
            }
            Self::List(_) => {
                let rows: Vec<(i64,bool,bool)> = sqlx::query_as("SELECT channel_id,auto_thread,auto_crosspost FROM community_auto_channels WHERE guild_id=$1 ORDER BY channel_id")
                    .bind(guild as i64).fetch_all(cx.app.db.pool()).await?;
                let lines = rows
                    .iter()
                    .map(|(id, thread, cross)| {
                        format!(
                            "<#{id}> · {} {}",
                            if *thread {
                                t!(cx.lang(), "community-v2.action-thread")
                            } else {
                                String::new()
                            },
                            if *cross {
                                t!(cx.lang(), "community-v2.action-crosspost")
                            } else {
                                String::new()
                            }
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n");
                tell(
                    cx,
                    if lines.is_empty() {
                        t!(cx.lang(), "community-v2.autopost-empty")
                    } else {
                        lines.chars().take(1900).collect()
                    },
                )
                .await
            }
        }
    }
}

#[derive(CommandModel, CreateCommand)]
#[command(
    name = "anniversary",
    desc = "Member join anniversaries",
    default_permissions = "manage_guild",
    contexts = "guild"
)]
pub enum AnniversaryCommand {
    #[command(name = "set")]
    Set(AnniversarySet),
    #[command(name = "off")]
    Off(AnniversaryOff),
}
#[derive(CommandModel, CreateCommand)]
#[command(name = "set", desc = "Post member anniversaries in a channel")]
pub struct AnniversarySet {
    /// Announcement channel
    #[command(channel_types = "guild_text guild_announcement")]
    channel: InteractionChannel,
}
#[derive(CommandModel, CreateCommand)]
#[command(name = "off", desc = "Stop anniversary announcements")]
pub struct AnniversaryOff;

impl AnniversaryCommand {
    pub async fn run(self, cx: &Cx) -> anyhow::Result<()> {
        let Some(guild) = cx.guild_id() else {
            return tell(cx, t!(cx.lang(), "community-v2.guild-only")).await;
        };
        if !cx.can_manage_guild() {
            return tell(cx, t!(cx.lang(), "community-v2.manage-only")).await;
        }
        let channel = match self {
            Self::Set(c) => Some(c.channel.id.get() as i64),
            Self::Off(_) => None,
        };
        sqlx::query("INSERT INTO anniversary_settings(guild_id,channel_id,enabled) VALUES($1,$2,$3) ON CONFLICT(guild_id) DO UPDATE SET channel_id=$2,enabled=$3")
            .bind(guild as i64).bind(channel).bind(channel.is_some()).execute(cx.app.db.pool()).await?;
        tell(cx, t!(cx.lang(), "community-v2.anniversary-saved")).await
    }
}

#[derive(CommandModel, CreateCommand)]
#[command(
    name = "confess",
    desc = "Send an anonymous confession for staff review",
    contexts = "guild"
)]
pub enum ConfessCommand {
    #[command(name = "submit")]
    Submit(ConfessSubmit),
    #[command(name = "review")]
    Review(ConfessReview),
    #[command(name = "reveal")]
    Reveal(ConfessReveal),
    #[command(name = "settings")]
    Settings(ConfessSettings),
    #[command(name = "report")]
    Report(ConfessReport),
    #[command(name = "ban")]
    Ban(ConfessBan),
}
#[derive(CommandModel, CreateCommand)]
#[command(name = "submit", desc = "Submit privately to the review queue")]
pub struct ConfessSubmit {
    /// Confession text
    #[command(min_length = 3, max_length = 1000)]
    text: String,
}
#[derive(CommandModel, CreateCommand)]
#[command(name = "review", desc = "Approve or deny a queued confession")]
pub struct ConfessReview {
    /// Confession number
    id: i64,
    /// Approve for public posting
    approve: bool,
}
#[derive(CommandModel, CreateCommand)]
#[command(name = "reveal", desc = "Server owner only: reveal an author with an audit entry")]
pub struct ConfessReveal {
    /// Confession number
    id: i64,
}
#[derive(CommandModel, CreateCommand)]
#[command(name = "settings", desc = "Set review and public channels")]
pub struct ConfessSettings {
    /// Private staff review channel
    #[command(channel_types = "guild_text guild_announcement")]
    review_channel: InteractionChannel,
    /// Public channel for approved posts
    #[command(channel_types = "guild_text guild_announcement")]
    publish_channel: InteractionChannel,
}

#[derive(CommandModel, CreateCommand)]
#[command(name = "report", desc = "Report a published confession to moderators")]
pub struct ConfessReport {
    /// Confession number
    #[command(min_value = 1)]
    id: i64,
}
#[derive(CommandModel, CreateCommand)]
#[command(name = "ban", desc = "Block or unblock a confession author without revealing them")]
pub struct ConfessBan {
    /// Confession number
    #[command(min_value = 1)]
    id: i64,
    /// Block this author from submitting confessions
    banned: bool,
}

impl ConfessCommand {
    pub async fn run(self, cx: &Cx) -> anyhow::Result<()> {
        let Some(guild) = cx.guild_id() else {
            return tell(cx, t!(cx.lang(), "community-v2.guild-only")).await;
        };
        match self {
            Self::Submit(c) => submit_confession(cx, guild, &c.text).await,
            Self::Review(c) => review_confession(cx, guild, c.id, c.approve).await,
            Self::Reveal(c) => reveal_confession(cx, guild, c.id).await,
            Self::Ban(c) => {
                if !cx.can_manage_guild() {
                    return tell(cx, t!(cx.lang(), "community-v2.manage-only")).await;
                }
                let changed = cx
                    .app
                    .db
                    .ban_confession_author(guild, c.id, cx.author()?.id.get(), c.banned)
                    .await?;
                tell(
                    cx,
                    t!(
                        cx.lang(),
                        if changed {
                            "community-v2.confession-ban-saved"
                        } else {
                            "community-v2.confession-missing"
                        }
                    ),
                )
                .await
            }
            Self::Report(c) => {
                let user = cx.author()?.id.get();
                if cx
                    .app
                    .cache
                    .try_cooldown(&format!("confess-report:{guild}:{user}"), Duration::from_secs(60))
                    .await?
                    .is_some()
                {
                    return tell(cx, t!(cx.lang(), "community-v2.confession-slow")).await;
                }
                if cx.app.db.report_confession(guild, c.id, user).await? {
                    let settings: Option<(Option<i64>,)> = sqlx::query_as(
                        "SELECT review_channel_id FROM confession_settings WHERE guild_id=$1 AND enabled",
                    )
                    .bind(guild as i64)
                    .fetch_optional(cx.app.db.pool())
                    .await?;
                    if let Some((Some(channel),)) = settings {
                        let lang = cx.app.guild_lang(guild).await;
                        let _ = cx
                            .app
                            .http
                            .create_message(Id::new(channel as u64))
                            .content(&t!(lang, "community-v2.confession-report-alert", id = c.id))
                            .allowed_mentions(quiet().as_ref())
                            .await;
                    }
                }
                tell(cx, t!(cx.lang(), "community-v2.confession-reported")).await
            }
            Self::Settings(c) => {
                if !cx.can_manage_guild() {
                    return tell(cx, t!(cx.lang(), "community-v2.manage-only")).await;
                }
                if c.review_channel.id == c.publish_channel.id {
                    return tell(cx, t!(cx.lang(), "community-v2.channels-differ")).await;
                }
                if !private_review_channel(&cx.app, guild, c.review_channel.id.get()).await? {
                    return tell(cx, t!(cx.lang(), "community-v2.review-private")).await;
                }
                sqlx::query("INSERT INTO confession_settings(guild_id,review_channel_id,publish_channel_id,enabled) VALUES($1,$2,$3,TRUE) ON CONFLICT(guild_id) DO UPDATE SET review_channel_id=$2,publish_channel_id=$3,enabled=TRUE")
                    .bind(guild as i64).bind(c.review_channel.id.get() as i64).bind(c.publish_channel.id.get() as i64)
                    .execute(cx.app.db.pool()).await?;
                tell(cx, t!(cx.lang(), "community-v2.confession-saved")).await
            }
        }
    }
}

async fn submit_confession(cx: &Cx, guild: u64, text: &str) -> anyhow::Result<()> {
    let body = strip_mentions(text).trim().to_owned();
    if body.chars().count() < 3 {
        return tell(cx, t!(cx.lang(), "community-v2.text-required")).await;
    }
    let settings: Option<(Option<i64>,)> =
        sqlx::query_as("SELECT review_channel_id FROM confession_settings WHERE guild_id=$1 AND enabled")
            .bind(guild as i64)
            .fetch_optional(cx.app.db.pool())
            .await?;
    let Some((Some(channel),)) = settings else {
        return tell(cx, t!(cx.lang(), "community-v2.confession-off")).await;
    };
    if !private_review_channel(&cx.app, guild, channel as u64)
        .await
        .unwrap_or(false)
    {
        return tell(cx, t!(cx.lang(), "community-v2.confession-unavailable")).await;
    }
    let user = cx.author()?.id.get();
    if cx
        .app
        .cache
        .try_cooldown(&format!("confess:{guild}:{user}"), Duration::from_secs(3600))
        .await?
        .is_some()
    {
        return tell(cx, t!(cx.lang(), "community-v2.confession-slow")).await;
    }
    let Some(id) = cx.app.db.submit_confession(guild, user, &body).await? else {
        return tell(cx, t!(cx.lang(), "community-v2.confession-slow")).await;
    };
    let lang = cx.app.guild_lang(guild).await;
    let embed = EmbedBuilder::new()
        .title(t!(lang, "community-v2.confession-review-title", id = id))
        .description(&body)
        .build();
    let buttons = vec![row(vec![
        button(
            format!("cv:a:{id}"),
            &t!(lang, "community-v2.approve"),
            None,
            ButtonStyle::Success,
            false,
        ),
        button(
            format!("cv:d:{id}"),
            &t!(lang, "community-v2.deny"),
            None,
            ButtonStyle::Danger,
            false,
        ),
    ])];
    match cx
        .app
        .http
        .create_message(Id::new(channel as u64))
        .embeds(&[cx.app.brand_embed(guild, embed).await])
        .components(&buttons)
        .allowed_mentions(quiet().as_ref())
        .await
    {
        Ok(posted) => {
            let message = posted.model().await?;
            sqlx::query("UPDATE confessions SET review_message_id=$2 WHERE id=$1")
                .bind(id)
                .bind(message.id.get() as i64)
                .execute(cx.app.db.pool())
                .await?;
            tell(cx, t!(cx.lang(), "community-v2.confession-queued", id = id)).await
        }
        Err(error) => {
            sqlx::query("DELETE FROM confessions WHERE id=$1")
                .bind(id)
                .execute(cx.app.db.pool())
                .await?;
            tracing::warn!(guild,id,%error,"confession queue post failed");
            tell(cx, t!(cx.lang(), "community-v2.confession-unavailable")).await
        }
    }
}

async fn review_confession(cx: &Cx, guild: u64, id: i64, approve: bool) -> anyhow::Result<()> {
    if !cx.can_manage_guild() {
        return tell(cx, t!(cx.lang(), "community-v2.manage-only")).await;
    }
    let row: Option<(String,Option<i64>,i64)> = sqlx::query_as(
        "UPDATE confessions SET status=$3,reviewer_id=$4,reviewed_at=now() WHERE id=$1 AND guild_id=$2 AND status='pending' RETURNING body,review_message_id,author_id")
        .bind(id).bind(guild as i64).bind(if approve { "approved" } else { "denied" })
        .bind(cx.author()?.id.get() as i64).fetch_optional(cx.app.db.pool()).await?;
    let Some((body, review_message, author)) = row else {
        return tell(cx, t!(cx.lang(), "community-v2.confession-missing")).await;
    };
    let settings: Option<(Option<i64>, Option<i64>)> =
        sqlx::query_as("SELECT review_channel_id,publish_channel_id FROM confession_settings WHERE guild_id=$1")
            .bind(guild as i64)
            .fetch_optional(cx.app.db.pool())
            .await?;
    if approve && settings.as_ref().and_then(|s| s.1).is_none() {
        sqlx::query("UPDATE confessions SET status='pending',reviewer_id=NULL,reviewed_at=NULL WHERE id=$1")
            .bind(id)
            .execute(cx.app.db.pool())
            .await?;
        return tell(cx, t!(cx.lang(), "community-v2.confession-unavailable")).await;
    }
    if approve && let Some((_, Some(channel))) = settings.as_ref() {
        let lang = cx.app.guild_lang(guild).await;
        let embed = EmbedBuilder::new()
            .title(t!(lang, "community-v2.confession-public-title", id = id))
            .description(&body)
            .build();
        match cx
            .app
            .http
            .create_message(Id::new(*channel as u64))
            .embeds(&[cx.app.brand_embed(guild, embed).await])
            .allowed_mentions(quiet().as_ref())
            .await
        {
            Ok(posted) => {
                let message = posted.model().await?;
                sqlx::query("UPDATE confessions SET publish_message_id=$2 WHERE id=$1")
                    .bind(id)
                    .bind(message.id.get() as i64)
                    .execute(cx.app.db.pool())
                    .await?;
            }
            Err(error) => {
                sqlx::query("UPDATE confessions SET status='pending',reviewer_id=NULL,reviewed_at=NULL WHERE id=$1")
                    .bind(id)
                    .execute(cx.app.db.pool())
                    .await?;
                tracing::warn!(guild,id,%error,"approved confession not published");
                return tell(cx, t!(cx.lang(), "community-v2.confession-unavailable")).await;
            }
        }
    }
    if let Some((Some(review_channel), _)) = settings.as_ref()
        && let Some(message) = review_message
    {
        let empty: Vec<Component> = Vec::new();
        let _ = cx
            .app
            .http
            .update_message(Id::new(*review_channel as u64), Id::new(message as u64))
            .components(Some(&empty))
            .await;
    }
    if let Some(dm) = private_channel(&cx.app, author as u64).await {
        let lang = cx.app.user_lang_choice(author as u64).await.unwrap_or_default();
        let _ = cx
            .app
            .http
            .create_message(dm)
            .content(&t!(
                lang,
                if approve {
                    "community-v2.confession-approved"
                } else {
                    "community-v2.confession-denied"
                },
                id = id
            ))
            .allowed_mentions(quiet().as_ref())
            .await;
    }
    tell(cx, t!(cx.lang(), "community-v2.confession-reviewed")).await
}

async fn reveal_confession(cx: &Cx, guild: u64, id: i64) -> anyhow::Result<()> {
    let owner = cx.app.http.guild(Id::new(guild)).await?.model().await?.owner_id.get();
    if owner != cx.author()?.id.get() {
        return tell(cx, t!(cx.lang(), "community-v2.owner-only")).await;
    }
    let row: Option<(i64,)> = sqlx::query_as("SELECT author_id FROM confessions WHERE id=$1 AND guild_id=$2")
        .bind(id)
        .bind(guild as i64)
        .fetch_optional(cx.app.db.pool())
        .await?;
    let Some((author,)) = row else {
        return tell(cx, t!(cx.lang(), "community-v2.confession-missing")).await;
    };
    sqlx::query("INSERT INTO confession_reveals(guild_id,confession_id,owner_id) VALUES($1,$2,$3)")
        .bind(guild as i64)
        .bind(id)
        .bind(owner as i64)
        .execute(cx.app.db.pool())
        .await?;
    tell(
        cx,
        t!(
            cx.lang(),
            "community-v2.confession-revealed",
            author = format!("<@{author}>")
        ),
    )
    .await
}

pub(crate) async fn on_confession_button<'a>(cx: &Cx, mut parts: impl Iterator<Item = &'a str>) -> anyhow::Result<()> {
    let approve = match parts.next() {
        Some("a") => true,
        Some("d") => false,
        _ => return tell(cx, t!(cx.lang(), "community-v2.confession-missing")).await,
    };
    let Some(id) = parts.next().and_then(|s| s.parse::<i64>().ok()) else {
        return tell(cx, t!(cx.lang(), "community-v2.confession-missing")).await;
    };
    if parts.next().is_some() {
        return tell(cx, t!(cx.lang(), "community-v2.confession-missing")).await;
    }
    let Some(guild) = cx.guild_id() else {
        return tell(cx, t!(cx.lang(), "community-v2.guild-only")).await;
    };
    review_confession(cx, guild, id, approve).await
}

/// Message content is read only for opted-in words. The DM includes a link, never a content excerpt.
pub(crate) async fn on_message(app: &Arc<App>, msg: &Message) {
    if msg.author.bot || msg.webhook_id.is_some() {
        return;
    }
    let Some(guild) = msg.guild_id else {
        return;
    };
    let guild = guild.get();
    if !app
        .module_allowed(
            guild,
            Some(msg.channel_id.get()),
            pinchora_core::admin::modules::Module::Utility,
        )
        .await
    {
        return;
    }
    let channel = msg.channel_id.get();
    let user = msg.author.id.get();
    if app.message_content && !msg.content.is_empty() {
        let words: Vec<String> = msg
            .content
            .to_lowercase()
            .split(|c: char| !c.is_alphanumeric())
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
            .collect();
        let matches: Vec<(i64, String)> =
            match sqlx::query_as("SELECT DISTINCT ON (user_id) user_id,keyword FROM keyword_highlights WHERE guild_id=$1 AND user_id<>$2 AND keyword=ANY($3) ORDER BY user_id,keyword LIMIT 100")
                .bind(guild as i64)
                .bind(user as i64)
                .bind(&words)
                .fetch_all(app.db.pool())
                .await
            {
                Ok(rows) => rows,
                Err(error) => {
                    tracing::warn!(%error,"highlights query failed");
                    return;
                }
            };
        for (follower, keyword) in matches {
            if !highlight_can_view(app, guild, channel, follower as u64)
                .await
                .unwrap_or(false)
            {
                continue;
            }
            let key = format!("highlight:{guild}:{follower}");
            if !matches!(app.cache.try_cooldown(&key, Duration::from_secs(600)).await, Ok(None)) {
                continue;
            }
            if let Some(dm) = private_channel(app, follower as u64).await {
                let lang = app.user_lang_choice(follower as u64).await.unwrap_or_default();
                let text = t!(
                    lang,
                    "community-v2.highlight-dm",
                    word = keyword,
                    link = format!("https://discord.com/channels/{guild}/{channel}/{}", msg.id)
                );
                let _ = app
                    .http
                    .create_message(dm)
                    .content(&text)
                    .allowed_mentions(quiet().as_ref())
                    .await;
            }
        }
    }
    if let Err(error) = sqlx::query("INSERT INTO community_autopost_jobs(message_id,guild_id,channel_id,thread_pending,crosspost_pending,thread_name) SELECT $1,guild_id,channel_id,auto_thread,auto_crosspost,thread_name FROM community_auto_channels WHERE guild_id=$2 AND channel_id=$3 ON CONFLICT DO NOTHING")
        .bind(msg.id.get() as i64).bind(guild as i64).bind(channel as i64).execute(app.db.pool()).await {
        tracing::warn!(%error,"autopost enqueue failed");
    }
}

/// Fresh Discord membership and overwrites; any lookup failure suppresses delivery.
/// Thread membership and age-gate eligibility cannot be verified here; suppress both.
async fn highlight_can_view(app: &App, guild: u64, channel: u64, user: u64) -> anyhow::Result<bool> {
    let member = app
        .http
        .guild_member(Id::new(guild), Id::new(user))
        .await?
        .model()
        .await?;
    let channel = app.http.channel(Id::new(channel)).await?.model().await?;
    if channel.guild_id.map(|id| id.get()) != Some(guild) || channel.kind.is_thread() || channel.nsfw == Some(true) {
        return Ok(false);
    }
    let roles = app.http.roles(Id::new(guild)).await?.models().await?;
    let owner = app.http.guild(Id::new(guild)).await?.model().await?.owner_id.get();
    let held: Vec<u64> = member.roles.iter().map(|r| r.get()).collect();
    let base = if owner == user {
        Permissions::all()
    } else {
        roles
            .iter()
            .filter(|r| r.id.get() == guild || held.contains(&r.id.get()))
            .fold(Permissions::empty(), |p, r| p | r.permissions)
    };
    Ok(crate::infra::text::access::channel_permissions(
        base,
        guild,
        user,
        &held,
        channel.permission_overwrites.as_deref().unwrap_or_default(),
    )
    .contains(Permissions::VIEW_CHANNEL))
}

pub(crate) async fn on_join(app: &Arc<App>, guild: u64, user: u64, joined: DateTime<Utc>) {
    if let Err(error) = sqlx::query("INSERT INTO member_anniversaries(guild_id,user_id,joined_at) VALUES($1,$2,$3) ON CONFLICT(guild_id,user_id) DO UPDATE SET joined_at=$3")
        .bind(guild as i64).bind(user as i64).bind(joined).execute(app.db.pool()).await {
        tracing::warn!(guild,user,%error,"anniversary join not recorded");
    }
}

pub(crate) async fn on_leave(app: &Arc<App>, guild: u64, user: u64) {
    if let Err(error) = sqlx::query("DELETE FROM member_anniversaries WHERE guild_id=$1 AND user_id=$2")
        .bind(guild as i64)
        .bind(user as i64)
        .execute(app.db.pool())
        .await
    {
        tracing::warn!(guild,user,%error,"anniversary leave not removed");
    }
    if let Err(error) = sqlx::query("DELETE FROM keyword_highlights WHERE guild_id=$1 AND user_id=$2")
        .bind(guild as i64)
        .bind(user as i64)
        .execute(app.db.pool())
        .await
    {
        tracing::warn!(guild,user,%error,"highlights not removed after leave");
    }
}

/// Leases expire after a crash. Failures retry at most five times per occurrence.
pub async fn tick(app: &Arc<App>) -> anyhow::Result<usize> {
    let due: Vec<(i64,i64,String,String,DateTime<Utc>)> = sqlx::query_as(
        "UPDATE personal_recurring_reminders SET lease_until=now()+interval '2 minutes',attempts=attempts+1 WHERE id IN (SELECT id FROM personal_recurring_reminders WHERE enabled AND next_at<=now() AND (lease_until IS NULL OR lease_until<=now()) ORDER BY next_at FOR UPDATE SKIP LOCKED LIMIT 25) RETURNING id,user_id,body,language,lease_until")
        .fetch_all(app.db.pool()).await?;
    let mut sent = 0;
    for (id, user, body, language, lease) in due {
        let delivered = if let Some(dm) = crate::progression::leveling::open_private_channel(app, user as u64).await {
            let lang = pinchora_core::infra::i18n::Lang::from_code(&language).unwrap_or_default();
            app.http
                .create_message(dm)
                .content(&t!(lang, "community-v2.recurring-dm", body = body))
                .allowed_mentions(quiet().as_ref())
                .await
                .is_ok()
        } else {
            false
        };
        sent += usize::from(delivered);
        sqlx::query("UPDATE personal_recurring_reminders SET next_at=CASE WHEN $3 OR attempts>=5 THEN now()+interval_minutes*interval '1 minute' ELSE now()+interval '2 minutes' END,attempts=CASE WHEN $3 OR attempts>=5 THEN 0 ELSE attempts END,lease_until=NULL WHERE id=$1 AND lease_until=$2")
            .bind(id).bind(lease).bind(delivered).execute(app.db.pool()).await?;
    }
    tick_autopost(app).await?;
    let today = app.now();
    let anniversaries: Vec<(i64,i64,i64,i32)> = sqlx::query_as(
        "SELECT m.guild_id,m.user_id,s.channel_id,EXTRACT(YEAR FROM age(CURRENT_DATE,m.joined_at::date))::integer FROM member_anniversaries m JOIN anniversary_settings s ON s.guild_id=m.guild_id AND s.enabled AND s.channel_id IS NOT NULL WHERE EXTRACT(MONTH FROM m.joined_at)=$1 AND EXTRACT(DAY FROM m.joined_at)=$2 AND m.joined_at<now()-interval '1 year' AND NOT EXISTS(SELECT 1 FROM anniversary_sent a WHERE a.guild_id=m.guild_id AND a.user_id=m.user_id AND a.year=$3) LIMIT 50")
        .bind(today.month() as i32).bind(today.day() as i32).bind(today.year()).fetch_all(app.db.pool()).await?;
    for (guild, user, channel, years) in anniversaries {
        if !app
            .module_allowed(
                guild as u64,
                Some(channel as u64),
                pinchora_core::admin::modules::Module::Birthdays,
            )
            .await
        {
            continue;
        }
        let claimed =
            sqlx::query("INSERT INTO anniversary_sent(guild_id,user_id,year) VALUES($1,$2,$3) ON CONFLICT DO NOTHING")
                .bind(guild)
                .bind(user)
                .bind(today.year())
                .execute(app.db.pool())
                .await?
                .rows_affected();
        if claimed == 0 {
            continue;
        }
        let lang = app.guild_lang(guild as u64).await;
        let text = t!(
            lang,
            "community-v2.anniversary-post",
            user = format!("<@{user}>"),
            years = years
        );
        if let Err(error) = app
            .http
            .create_message(Id::new(channel as u64))
            .content(&text)
            .allowed_mentions(quiet().as_ref())
            .await
        {
            tracing::warn!(guild,user,%error,"anniversary post failed");
            sqlx::query("DELETE FROM anniversary_sent WHERE guild_id=$1 AND user_id=$2 AND year=$3")
                .bind(guild)
                .bind(user)
                .bind(today.year())
                .execute(app.db.pool())
                .await?;
        }
    }
    Ok(sent)
}

pub fn spawn_ticker(app: Arc<App>) {
    tokio::spawn(async move {
        let mut wake = tokio::time::interval(Duration::from_secs(60));
        wake.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            wake.tick().await;
            if let Err(error) = tick(&app).await {
                tracing::warn!(%error,"community v2 tick failed");
            }
        }
    });
}

async fn tick_autopost(app: &App) -> anyhow::Result<()> {
    type Job = (i64, i64, i64, bool, bool, String, DateTime<Utc>);
    let jobs: Vec<Job> = sqlx::query_as("UPDATE community_autopost_jobs SET lease_until=now()+interval '2 minutes',attempts=attempts+1 WHERE message_id IN (SELECT message_id FROM community_autopost_jobs WHERE attempts<5 AND (thread_pending OR crosspost_pending) AND next_at<=now() AND (lease_until IS NULL OR lease_until<=now()) ORDER BY next_at FOR UPDATE SKIP LOCKED LIMIT 25) RETURNING message_id,guild_id,channel_id,thread_pending,crosspost_pending,thread_name,lease_until")
        .fetch_all(app.db.pool()).await?;
    for (message, guild, channel, thread, crosspost, name, lease) in jobs {
        if !app
            .module_allowed(
                guild as u64,
                Some(channel as u64),
                pinchora_core::admin::modules::Module::Utility,
            )
            .await
        {
            sqlx::query("UPDATE community_autopost_jobs SET attempts=attempts-1,lease_until=NULL,next_at=now()+interval '2 minutes' WHERE message_id=$1 AND lease_until=$2").bind(message).bind(lease).execute(app.db.pool()).await?;
            continue;
        }
        let configured: Option<(bool, bool)> = sqlx::query_as(
            "SELECT auto_thread,auto_crosspost FROM community_auto_channels WHERE guild_id=$1 AND channel_id=$2",
        )
        .bind(guild)
        .bind(channel)
        .fetch_optional(app.db.pool())
        .await?;
        let (mut thread, mut crosspost) = configured.map_or((false, false), |(t, c)| (thread && t, crosspost && c));
        if thread {
            let name = if name == "Discussion" {
                t!(app.guild_lang(guild as u64).await, "community-v2.thread-name")
            } else {
                name
            };
            // A message can have only one thread; reconcile a successful send whose ACK was lost.
            let existing = app.http.channel(Id::new(message as u64)).await;
            if matches!(existing, Ok(ref r) if r.status().is_success())
                || app
                    .http
                    .create_thread_from_message(Id::new(channel as u64), Id::new(message as u64), &name)
                    .await
                    .is_ok()
            {
                thread = false;
            }
        }
        if crosspost {
            let posted = app.http.message(Id::new(channel as u64), Id::new(message as u64)).await;
            let already = match posted {
                Ok(r) => r
                    .model()
                    .await
                    .ok()
                    .and_then(|m| m.flags)
                    .is_some_and(|f| f.contains(twilight_model::channel::message::MessageFlags::CROSSPOSTED)),
                Err(_) => false,
            };
            if already
                || app
                    .http
                    .crosspost_message(Id::new(channel as u64), Id::new(message as u64))
                    .await
                    .is_ok()
            {
                crosspost = false;
            }
        }
        sqlx::query("UPDATE community_autopost_jobs SET thread_pending=$3,crosspost_pending=$4,lease_until=NULL,next_at=now()+interval '2 minutes' WHERE message_id=$1 AND lease_until=$2")
            .bind(message).bind(lease).bind(thread).bind(crosspost).execute(app.db.pool()).await?;
    }
    sqlx::query("DELETE FROM community_autopost_jobs WHERE message_id IN (SELECT message_id FROM community_autopost_jobs WHERE created_at<now()-interval '7 days' FOR UPDATE SKIP LOCKED LIMIT 100)")
        .execute(app.db.pool()).await?;
    Ok(())
}
