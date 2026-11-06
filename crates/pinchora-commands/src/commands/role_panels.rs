//! `/roles`: panels that let members pick their own roles: buttons, a menu, or reactions.

use pinchora_core::{
    i18n::Lang,
    roles_pack::{
        MAX_EXPIRY_SECS, MIN_EXPIRY_SECS, PanelStyle, RulesError, SpanError, check_rules, format_span,
        parse_span_within,
    },
    t,
};
use pinchora_db::{MAX_PANEL_ROLES, PanelEntry, PanelPatch, PanelRow};
use twilight_interactions::command::{CommandModel, CommandOption, CreateCommand, CreateOption};
use twilight_model::{
    application::interaction::InteractionChannel,
    channel::message::EmojiReactionType,
    guild::{Permissions, Role},
    id::{Id, marker::RoleMarker},
};

use super::roles_pack::{PostRefusal, post_panel};
use crate::{Cx, roles};

pub(crate) use super::roles_pack::{on_select, on_toggle};

fn manage_guild() -> Permissions {
    Permissions::MANAGE_GUILD
}

#[derive(CommandModel, CreateCommand)]
#[command(
    name = "roles",
    desc = "Role panels: members pick their own roles",
    default_permissions = "manage_guild",
    contexts = "guild"
)]
#[allow(clippy::large_enum_variant)] // parsed once per interaction, never stored
pub enum RolesCommand {
    #[command(name = "panel")]
    Panel(RolesPanel),
    #[command(name = "add")]
    Add(RolesAdd),
    #[command(name = "remove")]
    Remove(RolesRemove),
    #[command(name = "post")]
    Post(RolesPost),
    #[command(name = "temp")]
    Temp(RolesTemp),
}

#[derive(CommandOption, CreateOption, Clone, Copy)]
pub enum StyleChoice {
    #[option(name = "Buttons: a button per role", value = "buttons")]
    Buttons,
    #[option(name = "Menu: pick from a dropdown", value = "select")]
    Select,
    #[option(name = "Reactions: react to get a role", value = "reactions")]
    Reactions,
}

impl StyleChoice {
    fn style(self) -> PanelStyle {
        match self {
            Self::Buttons => PanelStyle::Buttons,
            Self::Select => PanelStyle::Select,
            Self::Reactions => PanelStyle::Reactions,
        }
    }
}

#[derive(CommandModel, CreateCommand)]
#[command(name = "panel", desc = "Start a role panel, or change its text, style and rules")]
pub struct RolesPanel {
    /// Title of the panel; it is how the other commands find it
    #[command(min_length = 1, max_length = 80)]
    title: String,
    /// Text shown above the roles
    #[command(max_length = 500)]
    description: Option<String>,
    /// Buttons, a menu or reactions
    style: Option<StyleChoice>,
    /// Members hold one role of the panel at a time: picking another swaps
    exclusive: Option<bool>,
    /// A role members need to use the panel (@everyone takes the need away)
    required: Option<Id<RoleMarker>>,
    /// The fewest roles of the panel a member keeps
    #[command(min_value = 0, max_value = 25)]
    min: Option<i64>,
    /// The most roles of the panel a member holds (0: no limit)
    #[command(min_value = 0, max_value = 25)]
    max: Option<i64>,
}

#[derive(CommandModel, CreateCommand)]
#[command(name = "add", desc = "Add a role to a panel (up to 25), or change its label")]
pub struct RolesAdd {
    /// The panel's title or number
    #[command(min_length = 1, max_length = 80)]
    panel: String,
    /// The role the button gives and takes
    role: Role,
    /// Text on the button
    #[command(min_length = 1, max_length = 80)]
    label: String,
    /// Emoji on the button: a Unicode emoji or a custom one
    #[command(max_length = 64)]
    emoji: Option<String>,
    /// A line under the label in a menu
    #[command(max_length = 100)]
    description: Option<String>,
}

#[derive(CommandModel, CreateCommand)]
#[command(name = "remove", desc = "Take a role off a panel")]
pub struct RolesRemove {
    /// The panel's title or number
    #[command(min_length = 1, max_length = 80)]
    panel: String,
    /// The role to remove
    role: Id<RoleMarker>,
}

#[derive(CommandModel, CreateCommand)]
#[command(
    name = "post",
    desc = "Post a panel in a channel, or update it if it is already there"
)]
pub struct RolesPost {
    /// The panel's title or number
    #[command(min_length = 1, max_length = 80)]
    panel: String,
    /// Where to post it
    #[command(channel_types = "guild_text guild_announcement")]
    channel: InteractionChannel,
}

#[derive(CommandModel, CreateCommand)]
#[command(
    name = "temp",
    desc = "Make a panel's role last only for a while after a member takes it"
)]
pub struct RolesTemp {
    /// The role
    role: Id<RoleMarker>,
    /// How long it lasts, like 7d or 12h (off: for good)
    #[command(min_length = 1, max_length = 40)]
    duration: String,
    /// Only on this panel's title or number (default: every panel that has the role)
    #[command(max_length = 80)]
    panel: Option<String>,
}

/// Reads the emoji a manager typed: a Unicode emoji, or a custom one as Discord writes it
/// (`<:name:id>`, `<a:name:id>`). Words are not emoji.
pub(crate) fn parse_emoji(text: &str) -> Option<EmojiReactionType> {
    let text = text.trim();
    if let Some(inner) = text.strip_prefix('<').and_then(|t| t.strip_suffix('>')) {
        let (animated, inner) = match inner.strip_prefix("a:") {
            Some(rest) => (true, rest),
            None => (false, inner.strip_prefix(':')?),
        };
        let (name, id) = inner.split_once(':')?;
        let id: u64 = id.parse().ok().filter(|id| *id > 0)?;
        let name_ok = (2..=32).contains(&name.len()) && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
        return name_ok.then(|| EmojiReactionType::Custom {
            animated,
            id: Id::new(id),
            name: Some(name.to_owned()),
        });
    }
    // Emoji sequences are a handful of characters; a letter or a space means it is text. The
    // non-ASCII character rules out "1" and "#" on their own.
    let emoji =
        !text.is_ascii() && text.chars().count() <= 16 && !text.chars().any(|c| c.is_alphabetic() || c.is_whitespace());
    emoji.then(|| EmojiReactionType::Unicode { name: text.to_owned() })
}

/// Whether `text` is an emoji a panel can show: `Some` with the custom emoji's id (or `None` for a
/// Unicode one), `None` when it is not an emoji at all. The dashboard checks what it is given with this.
pub fn check_emoji(text: &str) -> Option<Option<u64>> {
    parse_emoji(text).map(|emoji| match emoji {
        EmojiReactionType::Custom { id, .. } => Some(id.get()),
        EmojiReactionType::Unicode { .. } => None,
    })
}

/// What a bad set of rules says, in `lang`.
pub fn rules_problem(lang: Lang, error: RulesError) -> String {
    match error {
        RulesError::TooMany => t!(lang, "roles.panel.rules-too-many", max = MAX_PANEL_ROLES),
        RulesError::MinAboveMax => t!(lang, "roles.panel.rules-min-above-max"),
        RulesError::ExclusiveMin => t!(lang, "roles.panel.rules-exclusive-min"),
    }
}

/// A style in words, in `lang`.
pub(crate) fn style_name(lang: Lang, style: PanelStyle) -> String {
    match style {
        PanelStyle::Buttons => t!(lang, "roles.panel.style.buttons"),
        PanelStyle::Select => t!(lang, "roles.panel.style.select"),
        PanelStyle::Reactions => t!(lang, "roles.panel.style.reactions"),
    }
}

/// The panel called `key`, or a reply saying there is none (and `None`).
async fn find_panel(cx: &Cx, guild: u64, key: &str) -> anyhow::Result<Option<PanelRow>> {
    let panel = cx.app.db.find_panel(guild, key).await?;
    if panel.is_none() {
        cx.reply_ephemeral(t!(cx.lang(), "roles.panel.no-such", title = key.trim()))
            .await?;
    }
    Ok(panel)
}

/// How a panel's style and rules read after its title: "Style **menu**, one at a time, at most 3."
fn settings_line(lang: Lang, panel: &PanelRow) -> String {
    let style = PanelStyle::parse(&panel.style).unwrap_or_default();
    let mut text = t!(lang, "roles.panel.settings-style", style = style_name(lang, style));
    if panel.exclusive {
        text.push(' ');
        text.push_str(&t!(lang, "roles.panel.settings-exclusive"));
    }
    if panel.min_picks > 0 {
        text.push(' ');
        text.push_str(&t!(lang, "roles.panel.settings-min", min = panel.min_picks));
    }
    if panel.max_picks > 0 {
        text.push(' ');
        text.push_str(&t!(lang, "roles.panel.settings-max", max = panel.max_picks));
    }
    if let Some(needed) = panel.required_role {
        text.push(' ');
        text.push_str(&t!(
            lang,
            "roles.panel.settings-required",
            role = format!("<@&{needed}>")
        ));
    }
    text
}

impl RolesCommand {
    pub async fn run(self, cx: &Cx) -> anyhow::Result<()> {
        let lang = cx.lang();
        let Some(guild) = cx.guild_id() else {
            return cx.reply_ephemeral(t!(lang, "roles.panel.guild-only")).await;
        };
        if !cx.can_manage_guild() {
            return cx.reply_ephemeral(t!(lang, "roles.panel.manage-needed")).await;
        }
        let db = &cx.app.db;
        match self {
            Self::Panel(c) => {
                let title = c.title.trim();
                if title.is_empty() {
                    return cx.reply_ephemeral(t!(lang, "roles.panel.needs-title")).await;
                }
                let description = c.description.as_deref().map(str::trim).filter(|d| !d.is_empty());
                // The rules as they will stand: what was already set, changed by what was given.
                let before = db.find_panel(guild, title).await?;
                let exclusive = c.exclusive.unwrap_or(before.as_ref().is_some_and(|p| p.exclusive));
                let min = c.min.or(before.as_ref().map(|p| i64::from(p.min_picks))).unwrap_or(0);
                let max = c.max.or(before.as_ref().map(|p| i64::from(p.max_picks))).unwrap_or(0);
                if let Err(error) = check_rules(exclusive, min as usize, max as usize) {
                    return cx.reply_ephemeral(rules_problem(lang, error)).await;
                }
                let patch = PanelPatch {
                    style: c.style.map(|s| s.style().key().to_owned()),
                    exclusive: c.exclusive,
                    required_role: c.required.map(|r| (r.get() != guild).then_some(r.get())),
                    min_picks: c.min.map(|v| v as i32),
                    max_picks: c.max.map(|v| v as i32),
                };
                let (panel, created) = db.save_panel_with(guild, title, description, &patch).await?;
                let custom = !patch_is_default(&panel);
                let mut text = if created {
                    t!(
                        lang,
                        "roles.panel.draft",
                        title = panel.title.as_str(),
                        number = panel.id
                    )
                } else {
                    t!(lang, "roles.panel.existed", title = panel.title.as_str())
                };
                if custom {
                    text.push('\n');
                    text.push_str(&settings_line(lang, &panel));
                }
                cx.reply_ephemeral(text).await
            }
            Self::Add(c) => {
                let Some(panel) = find_panel(cx, guild, &c.panel).await? else {
                    return Ok(());
                };
                let emoji = c.emoji.as_deref().map(str::trim).filter(|e| !e.is_empty());
                let parsed = emoji.map(check_emoji);
                if matches!(parsed, Some(None)) {
                    return cx.reply_ephemeral(t!(lang, "roles.panel.bad-emoji")).await;
                }
                if let Some(why) = roles::unassignable(cx, guild, &c.role).await {
                    return cx
                        .reply_ephemeral(crate::commands::roles_pack::configure_refusal(
                            lang,
                            c.role.id.get(),
                            &why,
                        ))
                        .await;
                }
                let entry = PanelEntry {
                    label: c.label.trim().to_owned(),
                    emoji: emoji.map(str::to_owned),
                    emoji_id: parsed.flatten().flatten(),
                    description: c
                        .description
                        .as_deref()
                        .map(str::trim)
                        .filter(|d| !d.is_empty())
                        .map(str::to_owned),
                };
                if !db.add_panel_entry(panel.id, c.role.id.get(), &entry).await? {
                    return cx
                        .reply_ephemeral(t!(lang, "roles.panel.full", max = MAX_PANEL_ROLES))
                        .await;
                }
                let count = db.panel_roles(panel.id).await?.len();
                cx.reply_ephemeral(t!(
                    lang,
                    "roles.panel.added-to",
                    role = format!("<@&{}>", c.role.id),
                    title = panel.title.as_str(),
                    label = c.label.trim(),
                    count = count,
                    max = MAX_PANEL_ROLES
                ))
                .await
            }
            Self::Remove(c) => {
                let Some(panel) = find_panel(cx, guild, &c.panel).await? else {
                    return Ok(());
                };
                cx.reply_ephemeral(if db.remove_panel_role(panel.id, c.role.get()).await? {
                    t!(
                        lang,
                        "roles.panel.removed-from",
                        role = format!("<@&{}>", c.role),
                        title = panel.title.as_str()
                    )
                } else {
                    t!(
                        lang,
                        "roles.panel.not-on",
                        role = format!("<@&{}>", c.role),
                        title = panel.title.as_str()
                    )
                })
                .await
            }
            Self::Post(c) => post(cx, guild, c).await,
            Self::Temp(c) => temp(cx, guild, c).await,
        }
    }
}

/// Whether a panel is the plain buttons panel `/roles panel` makes with no options.
fn patch_is_default(panel: &PanelRow) -> bool {
    panel.style == "buttons"
        && !panel.exclusive
        && panel.required_role.is_none()
        && panel.min_picks == 0
        && panel.max_picks == 0
}

/// Posts the panel, or updates its message when it already sits in that channel.
async fn post(cx: &Cx, guild: u64, c: RolesPost) -> anyhow::Result<()> {
    let lang = cx.lang();
    let Some(panel) = find_panel(cx, guild, &c.panel).await? else {
        return Ok(());
    };
    let channel = c.channel.id;
    let title = panel.title.as_str();
    match post_panel(&cx.app, guild, &panel, channel.get()).await {
        Ok(posted) if posted.updated => {
            cx.reply_ephemeral(t!(
                lang,
                "roles.panel.updated",
                title = title,
                channel = format!("<#{}>", posted.channel)
            ))
            .await
        }
        Ok(posted) => {
            let mut text = t!(
                lang,
                "roles.panel.posted",
                title = title,
                channel = format!("<#{channel}>"),
                link = format!("https://discord.com/channels/{guild}/{channel}/{}", posted.message)
            );
            if let Some(old) = posted.moved_from {
                text.push(' ');
                text.push_str(&t!(lang, "roles.panel.moved", channel = format!("<#{old}>")));
            }
            if !posted.unusable.is_empty() {
                text.push(' ');
                text.push_str(&t!(
                    lang,
                    "roles.panel.unusable-emojis",
                    emojis = posted.unusable.join(" ")
                ));
            }
            cx.reply_ephemeral(text).await
        }
        Err(refusal) => {
            cx.reply_ephemeral(post_refusal(lang, title, channel.get(), &refusal))
                .await
        }
    }
}

/// Why a panel was not posted, in `lang`.
fn post_refusal(lang: Lang, title: &str, channel: u64, refusal: &PostRefusal) -> String {
    match refusal {
        PostRefusal::NoRoles => t!(lang, "roles.panel.no-roles", title = title),
        PostRefusal::MissingEmoji(label) => t!(lang, "roles.panel.missing-emoji", label = label.as_str()),
        PostRefusal::SameEmoji(label) => t!(lang, "roles.panel.same-emoji", label = label.as_str()),
        PostRefusal::NoAccess => t!(lang, "roles.panel.no-access", channel = format!("<#{channel}>")),
        PostRefusal::UnknownChannel => t!(lang, "roles.panel.unknown-channel"),
        PostRefusal::Refused => t!(lang, "roles.panel.refused"),
        PostRefusal::Unreachable => t!(lang, "roles.panel.unreachable"),
    }
}

/// `/roles temp`: how long a panel's role lasts once a member took it.
async fn temp(cx: &Cx, guild: u64, c: RolesTemp) -> anyhow::Result<()> {
    let lang = cx.lang();
    let secs = match parse_span_within(&c.duration, MIN_EXPIRY_SECS, MAX_EXPIRY_SECS) {
        Ok(secs) => secs,
        Err(SpanError::Unreadable) => return cx.reply_ephemeral(t!(lang, "roles.panel.temp-unreadable")).await,
        Err(SpanError::TooShort(min)) => {
            return cx
                .reply_ephemeral(t!(lang, "roles.panel.temp-short", span = format_span(min)))
                .await;
        }
        Err(SpanError::TooLong(max)) => {
            return cx
                .reply_ephemeral(t!(lang, "roles.panel.temp-long", span = format_span(max)))
                .await;
        }
    };
    let panel = match c.panel.as_deref().map(str::trim).filter(|p| !p.is_empty()) {
        Some(key) => match find_panel(cx, guild, key).await? {
            Some(panel) => Some(panel),
            None => return Ok(()),
        },
        None => None,
    };
    let role = format!("<@&{}>", c.role);
    let changed = cx
        .app
        .db
        .set_panel_role_expiry(
            guild,
            c.role.get(),
            panel.as_ref().map(|p| p.id),
            (secs > 0).then_some(secs as i32),
        )
        .await?;
    if changed == 0 {
        return cx
            .reply_ephemeral(t!(lang, "roles.panel.temp-not-on", role = role))
            .await;
    }
    cx.reply_ephemeral(if secs > 0 {
        t!(lang, "roles.panel.temp-set", role = role, span = format_span(secs))
    } else {
        t!(lang, "roles.panel.temp-cleared", role = role)
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_unicode_and_custom_emoji() {
        assert!(matches!(
            parse_emoji("🎮"),
            Some(EmojiReactionType::Unicode { name }) if name == "🎮"
        ));
        assert!(matches!(
            parse_emoji(" 1️⃣ "),
            Some(EmojiReactionType::Unicode { name }) if name == "1️⃣"
        ));
        assert!(matches!(parse_emoji("👨‍👩‍👧"), Some(EmojiReactionType::Unicode { .. })));
        match parse_emoji("<:blobwave:123456789>") {
            Some(EmojiReactionType::Custom { animated, id, name }) => {
                assert!(!animated);
                assert_eq!(id.get(), 123_456_789);
                assert_eq!(name.as_deref(), Some("blobwave"));
            }
            other => panic!("custom emoji read as {other:?}"),
        }
        assert!(matches!(
            parse_emoji("<a:party:42>"),
            Some(EmojiReactionType::Custom { animated: true, .. })
        ));
    }

    #[test]
    fn words_and_broken_markup_are_not_emoji() {
        for text in [
            "",
            "gaming",
            "a",
            "1",
            "#",
            "é",
            "🎮 games",
            ":video_game:",
            "<:x:1>",
            "<:ok:0>",
            "<:ok:x>",
            "<ok>",
        ] {
            assert!(parse_emoji(text).is_none(), "{text:?} was accepted");
        }
    }

    #[test]
    fn the_dashboard_can_ask_what_an_emoji_is() {
        assert_eq!(check_emoji("🎮"), Some(None));
        assert_eq!(check_emoji("<:blobwave:123456789>"), Some(Some(123_456_789)));
        assert_eq!(check_emoji("blobwave"), None);
    }

    #[test]
    fn a_panel_with_no_options_is_the_plain_buttons_panel() {
        let mut panel = PanelRow {
            id: 1,
            title: "T".to_owned(),
            description: None,
            channel_id: None,
            message_id: None,
            style: "buttons".to_owned(),
            exclusive: false,
            required_role: None,
            min_picks: 0,
            max_picks: 0,
        };
        assert!(patch_is_default(&panel));
        assert!(settings_line(Lang::En, &panel).starts_with("Style **buttons**"));
        panel.max_picks = 3;
        panel.exclusive = true;
        panel.required_role = Some(9);
        assert!(!patch_is_default(&panel));
        let line = settings_line(Lang::En, &panel);
        assert!(
            line.contains("one at a time") && line.contains("at most 3") && line.contains("<@&9>"),
            "{line}"
        );
    }
}
