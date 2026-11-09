//! Role panels: how they read as messages in each style (buttons, a menu, reactions), posting them, and
//! what a click or a pick on one does.

use std::collections::HashMap;

use pinchora_core::infra::i18n::Lang;
use pinchora_core::roles::automation::Changes;
use pinchora_core::roles::automation::PanelRules;
use pinchora_core::roles::automation::PanelStyle;
use pinchora_core::roles::automation::Refusal;
use pinchora_core::roles::automation::choose;
use pinchora_core::roles::automation::format_span;
use pinchora_core::roles::automation::toggle;
use pinchora_core::t;
use pinchora_db::{PanelRoleRow, PanelRow};
use twilight_http::{Error, request::channel::reaction::RequestReactionType};
use twilight_model::{
    channel::message::{
        Component, Embed, EmojiReactionType,
        component::{ActionRow, Button, ButtonStyle, SelectMenu, SelectMenuOption, SelectMenuType},
    },
    id::{Id, marker::ChannelMarker},
};
use twilight_util::builder::embed::{EmbedBuilder, EmbedFooterBuilder};

use crate::App;
use crate::Cx;
use crate::commands::roles::automation::give;
use crate::commands::roles::role_panels::parse_emoji;
use crate::infra::util::BRAND;
use crate::roles::roles;

const UNKNOWN_CHANNEL: u64 = 10003;
const UNKNOWN_MESSAGE: u64 = 10008;
const MISSING_ACCESS: u64 = 50001;
const MISSING_PERMISSIONS: u64 = 50013;

/// Buttons per action row, which is also Discord's limit.
const ROW_WIDTH: usize = 5;
/// Discord's limit for an embed's description.
const DESCRIPTION_MAX: usize = 4_000;

pub(crate) fn rules_of(panel: &PanelRow) -> PanelRules {
    PanelRules {
        exclusive: panel.exclusive,
        min_picks: panel.min_picks.max(0) as usize,
        max_picks: panel.max_picks.max(0) as usize,
    }
}

pub(crate) fn style_of(panel: &PanelRow) -> PanelStyle {
    PanelStyle::parse(&panel.style).unwrap_or_default()
}

/// The custom id of a role button: the panel and the role, nothing else. Panels with no rules keep the
/// `rr` prefix every panel used before; the ones with rules send `rb`.
pub(crate) fn button_id(panel: &PanelRow, role: u64) -> String {
    let prefix = if rules_of(panel).is_plain() { "rr" } else { "rb" };
    format!("{prefix}:{}:{role}", panel.id)
}

/// Reads what follows the `rr:` or `rb:` prefix of a custom id.
fn parse_button<'a>(mut parts: impl Iterator<Item = &'a str>) -> Option<(i64, u64)> {
    let panel = parts.next()?.parse().ok()?;
    let role = parts.next()?.parse().ok()?;
    parts.next().is_none().then_some((panel, role))
}

/// How a reaction is known: `u:<unicode>` or `c:<custom emoji id>`. The variation selector is dropped, so
/// `❤` and `❤️` are one emoji.
pub(crate) fn emoji_key(emoji: &EmojiReactionType) -> String {
    match emoji {
        EmojiReactionType::Custom { id, .. } => format!("c:{}", id.get()),
        EmojiReactionType::Unicode { name } => format!("u:{}", name.replace('\u{fe0f}', "")),
    }
}

/// The emoji of an entry as a reaction Discord takes.
pub(crate) fn request_type(emoji: &EmojiReactionType) -> RequestReactionType<'_> {
    match emoji {
        EmojiReactionType::Custom { id, name, .. } => RequestReactionType::Custom {
            id: *id,
            name: name.as_deref(),
        },
        EmojiReactionType::Unicode { name } => RequestReactionType::Unicode { name },
    }
}

/// The emoji an [`emoji_key`] stands for, owned so a request can borrow it.
pub(crate) enum KeyEmoji {
    Unicode(String),
    Custom(u64),
}

impl KeyEmoji {
    /// Reads `u:<unicode>` or `c:<id>`.
    pub(crate) fn parse(key: &str) -> Option<Self> {
        if let Some(name) = key.strip_prefix("u:") {
            return Some(Self::Unicode(name.to_owned()));
        }
        key.strip_prefix("c:")?
            .parse::<u64>()
            .ok()
            .filter(|id| *id > 0)
            .map(Self::Custom)
    }

    pub(crate) fn request(&self) -> RequestReactionType<'_> {
        match self {
            Self::Unicode(name) => RequestReactionType::Unicode { name },
            Self::Custom(id) => RequestReactionType::Custom {
                id: Id::new(*id),
                name: None,
            },
        }
    }
}

fn mentions(roles: &[u64]) -> String {
    roles.iter().map(|r| format!("<@&{r}>")).collect::<Vec<_>>().join(", ")
}

fn entry_line(entry: &PanelRoleRow, style: PanelStyle) -> String {
    let emoji = entry.emoji.as_deref().map_or_else(String::new, |e| format!("{e} "));
    let mut line = format!("{emoji}**{}** · <@&{}>", entry.label, entry.role_id);
    // A menu shows the line itself; the other styles have no other place for it.
    if style != PanelStyle::Select
        && let Some(description) = entry.description.as_deref()
    {
        line.push_str(&format!(" — {}", description.chars().take(80).collect::<String>()));
    }
    if let Some(secs) = entry.temp_secs {
        line.push_str(&format!(" · ⏳ {}", format_span(i64::from(secs))));
    }
    line
}

fn footer(panel: &PanelRow, style: PanelStyle, lang: Lang) -> String {
    let mut text = match style {
        PanelStyle::Buttons => t!(lang, "roles.panel.footer.buttons"),
        PanelStyle::Select => t!(lang, "roles.panel.footer.select"),
        PanelStyle::Reactions => t!(lang, "roles.panel.footer.reactions"),
    };
    let rules = rules_of(panel);
    if rules.exclusive {
        text.push_str(" · ");
        text.push_str(&t!(lang, "roles.panel.footer.exclusive"));
    } else if rules.max_picks > 0 {
        text.push_str(" · ");
        text.push_str(&t!(lang, "roles.panel.footer.max", max = rules.max_picks));
    }
    text
}

/// The panel as a message: its text with a line per role, and the buttons, the menu or (for reactions)
/// nothing else, as its style says.
pub(crate) fn panel_message(panel: &PanelRow, roles: &[PanelRoleRow], lang: Lang) -> (Embed, Vec<Component>) {
    let style = style_of(panel);
    let mut lines: Vec<String> = Vec::new();
    lines.extend(panel.description.clone());
    if !lines.is_empty() {
        lines.push(String::new());
    }
    lines.extend(roles.iter().map(|r| entry_line(r, style)));
    if let Some(needed) = panel.required_role {
        lines.push(String::new());
        lines.push(t!(lang, "roles.panel.needs", role = format!("<@&{needed}>")));
    }
    let mut description = lines.join("\n");
    if description.chars().count() > DESCRIPTION_MAX {
        description = description.chars().take(DESCRIPTION_MAX).collect::<String>() + "…";
    }
    let embed = EmbedBuilder::new()
        .color(BRAND)
        .title(&panel.title)
        .description(description)
        .footer(EmbedFooterBuilder::new(footer(panel, style, lang)))
        .build();
    let components = match style {
        PanelStyle::Buttons => buttons(panel, roles),
        PanelStyle::Select => vec![select_menu(panel, roles, lang)],
        PanelStyle::Reactions => Vec::new(),
    };
    (embed, components)
}

fn buttons(panel: &PanelRow, roles: &[PanelRoleRow]) -> Vec<Component> {
    roles
        .chunks(ROW_WIDTH)
        .map(|row| {
            Component::ActionRow(ActionRow {
                id: None,
                components: row
                    .iter()
                    .map(|r| {
                        Component::Button(Button {
                            custom_id: Some(button_id(panel, r.role_id as u64)),
                            disabled: false,
                            emoji: r.emoji.as_deref().and_then(parse_emoji),
                            id: None,
                            label: Some(r.label.clone()),
                            sku_id: None,
                            style: ButtonStyle::Secondary,
                            url: None,
                        })
                    })
                    .collect(),
            })
        })
        .collect()
}

fn select_menu(panel: &PanelRow, roles: &[PanelRoleRow], lang: Lang) -> Component {
    let rules = rules_of(panel);
    let count = roles.len().min(25);
    let max = rules.cap().unwrap_or(count).clamp(1, count.max(1));
    let min = rules.min_picks.min(max);
    let options = roles
        .iter()
        .take(25)
        .map(|r| SelectMenuOption {
            default: false,
            description: r.description.as_ref().map(|d| d.chars().take(100).collect()),
            emoji: r.emoji.as_deref().and_then(parse_emoji),
            label: r.label.chars().take(100).collect(),
            value: r.role_id.to_string(),
        })
        .collect();
    Component::ActionRow(ActionRow {
        id: None,
        components: vec![Component::SelectMenu(SelectMenu {
            channel_types: None,
            custom_id: format!("rs:{}", panel.id),
            default_values: None,
            disabled: false,
            id: None,
            kind: SelectMenuType::Text,
            max_values: Some(max as u8),
            min_values: Some(min as u8),
            options: Some(options),
            placeholder: Some(t!(lang, "roles.panel.placeholder")),
            required: None,
        })],
    })
}

// /Posting

/// Where a panel was posted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PostedPanel {
    pub channel: u64,
    pub message: u64,
    /// The message that was already there was edited in place.
    pub updated: bool,
    /// The channel the panel was in before, when it moved.
    pub moved_from: Option<u64>,
    /// Emojis Discord would not take as reactions.
    pub unusable: Vec<String>,
}

/// Why a panel was not posted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PostRefusal {
    /// The panel has no roles yet.
    NoRoles,
    /// A reactions panel needs an emoji on every role; this one has none (the label).
    MissingEmoji(String),
    /// Two roles of a reactions panel share an emoji (the second label).
    SameEmoji(String),
    /// The bot may not post there.
    NoAccess,
    UnknownChannel,
    /// Discord refused the message (an emoji on a button it does not know, say).
    Refused,
    /// Discord could not be reached, or the database failed.
    Unreachable,
}

fn refused_by(error: &Error) -> PostRefusal {
    match roles::error_code(error) {
        Some(MISSING_ACCESS | MISSING_PERMISSIONS) => PostRefusal::NoAccess,
        Some(UNKNOWN_CHANNEL) => PostRefusal::UnknownChannel,
        Some(_) => PostRefusal::Refused,
        None => PostRefusal::Unreachable,
    }
}

/// Posts the panel in `channel`, or edits its message when it already sits there; a message somebody
/// deleted is posted afresh. A reactions panel gets its emojis put under the message.
pub async fn post_panel(app: &App, guild: u64, panel: &PanelRow, channel: u64) -> Result<PostedPanel, PostRefusal> {
    let entries = app.db.panel_roles(panel.id).await.map_err(|error| {
        tracing::warn!(guild, panel = panel.id, %error, "could not read a role panel");
        PostRefusal::Unreachable
    })?;
    if entries.is_empty() {
        return Err(PostRefusal::NoRoles);
    }
    let style = style_of(panel);
    let mut wanted: Vec<(String, u64, EmojiReactionType)> = Vec::new();
    if style == PanelStyle::Reactions {
        for entry in &entries {
            let Some(emoji) = entry.emoji.as_deref().and_then(parse_emoji) else {
                return Err(PostRefusal::MissingEmoji(entry.label.clone()));
            };
            let key = emoji_key(&emoji);
            if wanted.iter().any(|(k, ..)| *k == key) {
                return Err(PostRefusal::SameEmoji(entry.label.clone()));
            }
            wanted.push((key, entry.role_id as u64, emoji));
        }
    }
    let lang = app.guild_lang(guild).await;
    let (embed, components) = panel_message(panel, &entries, lang);
    let embed = app.brand_embed(guild, embed).await;
    let channel_id = Id::new(channel.max(1));
    let before = panel.channel_id.zip(panel.message_id);

    // Same channel: edit the message in place. A message someone deleted is posted afresh.
    let mut posted: Option<(u64, bool)> = None;
    if let Some((_, message)) = before.filter(|(posted_in, _)| *posted_in as u64 == channel) {
        match app
            .http
            .update_message(channel_id, Id::new(message as u64))
            .embeds(Some(std::slice::from_ref(&embed)))
            .components(Some(&components))
            .await
        {
            Ok(_) => posted = Some((message as u64, true)),
            Err(error) if roles::error_code(&error) == Some(UNKNOWN_MESSAGE) => {}
            Err(error) => {
                tracing::warn!(guild, panel = panel.id, %error, "could not update a role panel");
                return Err(refused_by(&error));
            }
        }
    }
    let (message, updated) = match posted {
        Some(found) => found,
        None => {
            let made = app
                .http
                .create_message(channel_id)
                .embeds(&[embed])
                .components(&components)
                .await;
            let message = match made {
                Ok(response) => response.model().await.map_err(|error| {
                    tracing::warn!(guild, panel = panel.id, %error, "could not read the posted role panel");
                    PostRefusal::Unreachable
                })?,
                Err(error) => {
                    tracing::warn!(guild, panel = panel.id, %error, "could not post a role panel");
                    return Err(refused_by(&error));
                }
            };
            app.db
                .set_panel_message(panel.id, channel, message.id.get())
                .await
                .map_err(|error| {
                    tracing::warn!(guild, panel = panel.id, %error, "could not remember a role panel's message");
                    PostRefusal::Unreachable
                })?;
            (message.id.get(), false)
        }
    };
    let moved_from = before
        .filter(|(posted_in, _)| *posted_in as u64 != channel)
        .map(|(posted_in, _)| posted_in as u64);
    let unusable = sync_reactions(app, guild, panel, channel_id, message, updated, &wanted).await;
    crate::commands::roles::automation::reactions::forget(message).await;
    Ok(PostedPanel {
        channel,
        message,
        updated,
        moved_from,
        unusable,
    })
}

/// Puts the panel's emojis under its message and remembers which gives what; takes away the emojis of
/// roles that are no longer on the panel, and all of them when the panel is not a reactions panel any more.
async fn sync_reactions(
    app: &App,
    guild: u64,
    panel: &PanelRow,
    channel: Id<ChannelMarker>,
    message: u64,
    updated: bool,
    wanted: &[(String, u64, EmojiReactionType)],
) -> Vec<String> {
    let message_id = Id::new(message.max(1));
    let before = match app.db.panel_reactions(panel.id).await {
        Ok(rows) => rows,
        Err(error) => {
            tracing::warn!(guild, panel = panel.id, %error, "could not read a panel's reactions");
            Vec::new()
        }
    };
    if updated {
        if wanted.is_empty() && !before.is_empty() {
            if let Err(error) = app.http.delete_all_reactions(channel, message_id).await {
                tracing::warn!(guild, panel = panel.id, %error, "could not clear a panel's reactions");
            }
        } else {
            for (key, _) in before.iter().filter(|(k, _)| !wanted.iter().any(|(w, ..)| w == k)) {
                let Some(emoji) = KeyEmoji::parse(key) else { continue };
                if let Err(error) = app
                    .http
                    .delete_all_reaction(channel, message_id, &emoji.request())
                    .await
                {
                    tracing::warn!(guild, panel = panel.id, %error, "could not clear an old reaction");
                }
            }
        }
    }
    let mut unusable = Vec::new();
    for (key, _, emoji) in wanted {
        if let Err(error) = app
            .http
            .create_reaction(channel, message_id, &request_type(emoji))
            .await
        {
            tracing::warn!(guild, panel = panel.id, %error, "could not add a panel's reaction");
            unusable.push(key.clone());
        }
    }
    let saved: Vec<(String, u64)> = wanted.iter().map(|(k, r, _)| (k.clone(), *r)).collect();
    if let Err(error) = app.db.replace_panel_reactions(panel.id, &saved).await {
        tracing::warn!(guild, panel = panel.id, %error, "could not remember a panel's reactions");
    }
    unusable
}

// /Picking

/// What changing a member's roles did.
pub(crate) struct Applied {
    pub added: Vec<u64>,
    pub removed: Vec<u64>,
    /// The first change Discord refused; the ones after it were not tried.
    pub failed: Option<Failed>,
}

pub(crate) struct Failed {
    pub role: u64,
    pub giving: bool,
    pub error: anyhow::Error,
    /// The role no longer exists, and was dropped from everything that named it.
    pub gone: bool,
}

/// Takes and gives the roles of `changes` (takes first, so a swap frees its place), keeping the timers
/// of roles that expire: `temp` is how long each role lasts, in seconds.
pub(crate) async fn apply(
    app: &App,
    guild: u64,
    user: u64,
    changes: &Changes,
    temp: &HashMap<u64, i32>,
    reason: &str,
) -> Applied {
    let mut done = Applied {
        added: Vec::new(),
        removed: Vec::new(),
        failed: None,
    };
    // Reject a role whose current permissions or position changed before touching a held role.
    for &role in &changes.add {
        if let Err(error) = give::validate(app, guild, role).await {
            done.failed = Some(failed(app, guild, role, true, error).await);
            return done;
        }
    }
    let mut scheduled = Vec::new();
    for &role in &changes.add {
        if let Some(secs) = temp.get(&role) {
            let at = app.now() + chrono::Duration::seconds(i64::from(*secs));
            if let Err(error) = app.db.schedule_removal(guild, user, role, at).await {
                for prior in scheduled {
                    if let Err(cancel) = app.db.cancel_removal(guild, user, prior).await {
                        tracing::warn!(guild, user, role = prior, %cancel, "could not clear a prepared timer");
                    }
                }
                done.failed = Some(failed(app, guild, role, true, error).await);
                return done;
            }
            scheduled.push(role);
        }
    }
    for &role in &changes.remove {
        match give::take(app, guild, user, role, reason).await {
            Ok(()) => {
                done.removed.push(role);
                if let Err(error) = app.db.cancel_removal(guild, user, role).await {
                    tracing::warn!(guild, user, role, %error, "could not cancel a role's timer");
                }
            }
            Err(error) => {
                for &pending in &scheduled {
                    if let Err(cancel) = app.db.cancel_removal(guild, user, pending).await {
                        tracing::warn!(guild, user, role = pending, %cancel, "could not clear a prepared timer");
                    }
                }
                done.failed = Some(failed(app, guild, role, false, error.into()).await);
                return done;
            }
        }
    }
    for &role in &changes.add {
        match give::give(app, guild, user, role, reason).await {
            Ok(()) => {
                done.added.push(role);
            }
            Err(error) => {
                for &pending in &scheduled {
                    if !done.added.contains(&pending)
                        && let Err(cancel) = app.db.cancel_removal(guild, user, pending).await
                    {
                        tracing::warn!(guild, user, role = pending, %cancel, "could not clear a prepared timer");
                    }
                }
                done.failed = Some(failed(app, guild, role, true, error).await);
                return done;
            }
        }
    }
    done
}

async fn failed(app: &App, guild: u64, role: u64, giving: bool, error: anyhow::Error) -> Failed {
    tracing::warn!(guild, role, %error, "could not change a role from a panel");
    let gone = error.downcast_ref::<Error>().is_some_and(roles::role_is_gone);
    if gone {
        give::forget(app, guild, role).await;
    }
    Failed {
        role,
        giving,
        error,
        gone,
    }
}

/// What to tell a member their pick did.
pub(crate) fn applied_text(lang: Lang, done: &Applied, temp: &HashMap<u64, i32>) -> String {
    let mut parts: Vec<String> = Vec::new();
    if !done.added.is_empty() {
        parts.push(t!(lang, "roles.panel.added", roles = mentions(&done.added)));
    }
    if !done.removed.is_empty() {
        parts.push(t!(lang, "roles.panel.removed", roles = mentions(&done.removed)));
    }
    for role in &done.added {
        if let Some(secs) = temp.get(role) {
            parts.push(t!(
                lang,
                "roles.panel.lasts",
                role = format!("<@&{role}>"),
                span = format_span(i64::from(*secs))
            ));
        }
    }
    if let Some(fail) = &done.failed {
        let role = format!("<@&{}>", fail.role);
        let why = give::give_refusal_text(lang, &fail.error);
        parts.push(if fail.giving {
            t!(lang, "roles.panel.cant-give", role = role, why = why)
        } else {
            t!(lang, "roles.panel.cant-take", role = role, why = why)
        });
        if fail.gone {
            parts.push(t!(lang, "roles.panel.button-removed"));
        }
    }
    if parts.is_empty() {
        t!(lang, "roles.panel.unchanged")
    } else {
        parts.join(" ")
    }
}

/// What a refused pick says.
fn refusal_line(lang: Lang, refusal: Refusal) -> String {
    match refusal {
        Refusal::NotOnPanel => t!(lang, "roles.panel.not-on-panel"),
        Refusal::TooMany(max) => t!(lang, "roles.panel.too-many", max = max),
        Refusal::TooFew(min) => t!(lang, "roles.panel.too-few", min = min),
    }
}

/// The roles of a panel and how long each lasts, as a pick needs them.
async fn panel_facts(cx: &Cx, panel: &PanelRow) -> anyhow::Result<(Vec<u64>, HashMap<u64, i32>)> {
    let entries = cx.app.db.panel_roles(panel.id).await?;
    let temp = entries
        .iter()
        .filter_map(|e| e.temp_secs.map(|s| (e.role_id as u64, s)))
        .collect();
    Ok((entries.iter().map(|e| e.role_id as u64).collect(), temp))
}

/// Interaction member roles can be older than another click. Read Discord under the member lock.
pub(crate) async fn current_roles(app: &App, guild: u64, user: u64) -> anyhow::Result<Vec<u64>> {
    let member = app
        .http
        .guild_member(Id::new(guild.max(1)), Id::new(user.max(1)))
        .await?
        .model()
        .await?;
    Ok(member.roles.iter().map(|role| role.get()).collect())
}

/// `rr` and `rb` buttons: give the member the role, or take it if they have it, as the panel's rules allow.
pub(crate) async fn on_toggle<'a>(cx: &Cx, parts: impl Iterator<Item = &'a str>) -> anyhow::Result<()> {
    let lang = cx.lang();
    let Some(guild) = cx.guild_id() else {
        return cx.reply_ephemeral(t!(lang, "roles.panel.guild-only-buttons")).await;
    };
    let Some((panel_id, role)) = parse_button(parts) else {
        return cx.reply_ephemeral(t!(lang, "roles.panel.button-expired")).await;
    };
    // The id came from the client; it counts only if that role still has a button on a panel of this server.
    let db = &cx.app.db;
    let Some(panel) = db.panel_by_id(guild, panel_id).await? else {
        return cx.reply_ephemeral(t!(lang, "roles.panel.role-gone")).await;
    };
    if db.panel_role(guild, panel_id, role).await?.is_none() {
        return cx.reply_ephemeral(t!(lang, "roles.panel.role-gone")).await;
    }
    let user = cx.author()?.id.get();
    let _lock = cx.app.db.lock_role_member(guild, user).await?;
    let held = current_roles(&cx.app, guild, user).await?;
    if let Some(needed) = panel.required_role.filter(|r| !held.contains(&(*r as u64))) {
        return cx
            .reply_ephemeral(t!(lang, "roles.panel.needs-role", role = format!("<@&{needed}>")))
            .await;
    }
    let (ids, temp) = panel_facts(cx, &panel).await?;
    let changes = match toggle(&rules_of(&panel), &ids, &held, role) {
        Ok(changes) => changes,
        Err(refusal) => return cx.reply_ephemeral(refusal_line(lang, refusal)).await,
    };
    let done = apply(&cx.app, guild, user, &changes, &temp, "Role panel").await;
    cx.reply_ephemeral(applied_text(lang, &done, &temp)).await
}

/// `rs` menus: what the member picked is what they hold of the panel afterwards.
pub(crate) async fn on_select<'a>(
    cx: &Cx,
    mut parts: impl Iterator<Item = &'a str>,
    values: &[String],
) -> anyhow::Result<()> {
    let lang = cx.lang();
    let Some(guild) = cx.guild_id() else {
        return cx.reply_ephemeral(t!(lang, "roles.panel.guild-only-menus")).await;
    };
    let panel_id = parts.next().and_then(|p| p.parse::<i64>().ok());
    let chosen: Option<Vec<u64>> = values.iter().map(|v| v.parse().ok()).collect();
    let (Some(panel_id), Some(chosen), None) = (panel_id, chosen, parts.next()) else {
        return cx.reply_ephemeral(t!(lang, "roles.panel.menu-expired")).await;
    };
    let Some(panel) = cx.app.db.panel_by_id(guild, panel_id).await? else {
        return cx.reply_ephemeral(t!(lang, "roles.panel.menu-gone")).await;
    };
    let user = cx.author()?.id.get();
    let _lock = cx.app.db.lock_role_member(guild, user).await?;
    let held = current_roles(&cx.app, guild, user).await?;
    if let Some(needed) = panel.required_role.filter(|r| !held.contains(&(*r as u64))) {
        return cx
            .reply_ephemeral(t!(lang, "roles.panel.needs-role", role = format!("<@&{needed}>")))
            .await;
    }
    let (ids, temp) = panel_facts(cx, &panel).await?;
    let changes = match choose(&rules_of(&panel), &ids, &held, &chosen) {
        Ok(changes) => changes,
        Err(refusal) => return cx.reply_ephemeral(refusal_line(lang, refusal)).await,
    };
    let done = apply(&cx.app, guild, user, &changes, &temp, "Role panel").await;
    cx.reply_ephemeral(applied_text(lang, &done, &temp)).await
}

#[cfg(test)]
mod tests {
    use crate::commands::roles::automation::panel::*;

    fn panel(style: &str) -> PanelRow {
        PanelRow {
            id: 7,
            title: "Colours".to_owned(),
            description: Some("Pick one".to_owned()),
            channel_id: None,
            message_id: None,
            style: style.to_owned(),
            exclusive: false,
            required_role: None,
            min_picks: 0,
            max_picks: 0,
        }
    }

    fn entries(n: i64) -> Vec<PanelRoleRow> {
        (1..=n)
            .map(|n| PanelRoleRow {
                role_id: 1_000 + n,
                label: format!("Role {n}"),
                emoji: (n == 1).then(|| "🎮".to_owned()),
                description: (n == 2).then(|| "The second one".to_owned()),
                emoji_id: None,
                temp_secs: None,
            })
            .collect()
    }

    #[test]
    fn button_ids_round_trip() {
        let plain = panel("buttons");
        let id = button_id(&plain, 345_678_901_234_567_890);
        let mut parts = id.split(':');
        assert_eq!(parts.next(), Some("rr"));
        assert_eq!(parse_button(parts), Some((7, 345_678_901_234_567_890)));
        assert!(id.len() <= 100, "Discord's custom id limit");
        // A panel with rules sends the newer prefix; both read the same.
        let strict = PanelRow {
            exclusive: true,
            ..plain
        };
        let id = button_id(&strict, 5);
        let mut parts = id.split(':');
        assert_eq!(parts.next(), Some("rb"));
        assert_eq!(parse_button(parts), Some((7, 5)));
    }

    #[test]
    fn bad_button_ids_are_refused() {
        assert_eq!(parse_button("1".split(':')), None, "no role");
        assert_eq!(parse_button("x:2".split(':')), None, "panel is not a number");
        assert_eq!(parse_button("1:-2".split(':')), None, "role is not an id");
        assert_eq!(parse_button("1:2:3".split(':')), None, "trailing parts");
        assert_eq!(parse_button("".split(':')), None);
    }

    #[test]
    fn a_panel_becomes_rows_of_five_buttons() {
        let (embed, rows) = panel_message(&panel("buttons"), &entries(12), Lang::En);
        assert_eq!(embed.title.as_deref(), Some("Colours"));
        let description = embed.description.unwrap();
        assert!(
            description.starts_with("Pick one\n\n🎮 **Role 1** · <@&1001>"),
            "{description}"
        );
        assert!(
            description.contains("**Role 2** · <@&1002> — The second one"),
            "{description}"
        );
        assert_eq!(
            embed.footer.map(|f| f.text).as_deref(),
            Some("Click a button to get a role; click it again to drop it")
        );
        let widths: Vec<usize> = rows
            .iter()
            .map(|row| match row {
                Component::ActionRow(r) => r.components.len(),
                _ => 0,
            })
            .collect();
        assert_eq!(widths, [5, 5, 2]);
        let Component::ActionRow(first) = &rows[0] else {
            panic!("not a row");
        };
        let Component::Button(button) = &first.components[0] else {
            panic!("not a button");
        };
        assert_eq!(button.custom_id.as_deref(), Some("rr:7:1001"));
        assert!(button.emoji.is_some());
    }

    #[test]
    fn a_menu_panel_is_one_select_with_the_panels_limits() {
        let strict = PanelRow {
            min_picks: 1,
            max_picks: 3,
            required_role: Some(55),
            ..panel("select")
        };
        let (embed, rows) = panel_message(&strict, &entries(6), Lang::En);
        let [Component::ActionRow(row)] = &rows[..] else {
            panic!("one row, got {}", rows.len());
        };
        let [Component::SelectMenu(menu)] = &row.components[..] else {
            panic!("one menu");
        };
        assert_eq!(menu.custom_id, "rs:7");
        assert_eq!((menu.min_values, menu.max_values), (Some(1), Some(3)));
        let options = menu.options.as_ref().unwrap();
        assert_eq!(options.len(), 6);
        assert_eq!(options[0].value, "1001");
        assert!(options[0].emoji.is_some());
        assert_eq!(options[1].description.as_deref(), Some("The second one"));
        let description = embed.description.unwrap();
        assert!(
            !description.contains("The second one"),
            "a menu shows the line itself: {description}"
        );
        assert!(description.ends_with("Needs <@&55>"), "{description}");
        // Exclusive means one at a time.
        let exclusive = PanelRow {
            exclusive: true,
            ..panel("select")
        };
        let (_, rows) = panel_message(&exclusive, &entries(4), Lang::En);
        let Component::ActionRow(row) = &rows[0] else {
            panic!("a row")
        };
        let Component::SelectMenu(menu) = &row.components[0] else {
            panic!("a menu")
        };
        assert_eq!((menu.min_values, menu.max_values), (Some(0), Some(1)));
    }

    #[test]
    fn a_reactions_panel_has_no_components_and_says_what_each_emoji_gives() {
        let (embed, rows) = panel_message(&panel("reactions"), &entries(3), Lang::En);
        assert!(rows.is_empty());
        assert!(embed.footer.unwrap().text.starts_with("React with an emoji"));
        let timed = PanelRoleRow {
            temp_secs: Some(3_600),
            ..entries(1).remove(0)
        };
        assert!(entry_line(&timed, PanelStyle::Reactions).ends_with("· ⏳ 1h"));
    }

    #[test]
    fn a_huge_panel_still_fits_an_embed() {
        let long = PanelRoleRow {
            role_id: 1,
            label: "L".repeat(80),
            emoji: None,
            description: Some("d".repeat(100)),
            emoji_id: None,
            temp_secs: None,
        };
        let many = vec![long; 25];
        let (embed, _) = panel_message(&panel("reactions"), &many, Lang::En);
        assert!(embed.description.unwrap().chars().count() <= 4_096);
    }

    #[test]
    fn emojis_are_known_by_a_key() {
        let heart = EmojiReactionType::Unicode {
            name: "❤️".to_owned()
        };
        let bare = EmojiReactionType::Unicode { name: "❤".to_owned() };
        assert_eq!(emoji_key(&heart), emoji_key(&bare));
        let custom = EmojiReactionType::Custom {
            animated: false,
            id: Id::new(42),
            name: Some("blob".to_owned()),
        };
        assert_eq!(emoji_key(&custom), "c:42");
        assert!(matches!(KeyEmoji::parse("c:42"), Some(KeyEmoji::Custom(42))));
        assert!(
            KeyEmoji::parse("c:x").is_none() && KeyEmoji::parse("zz").is_none() && KeyEmoji::parse("c:0").is_none()
        );
        let game = KeyEmoji::parse("u:🎮").expect("a unicode key");
        assert!(matches!(game.request(), RequestReactionType::Unicode { name } if name == "🎮"));
    }
}
