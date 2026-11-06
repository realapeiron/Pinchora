//! `/roles`: messages of buttons that give and take roles.

use pinchora_core::{i18n::Lang, t};
use pinchora_db::{MAX_PANEL_ROLES, PanelRoleRow, PanelRow};
use twilight_http::{Error, request::AuditLogReason};
use twilight_interactions::command::{CommandModel, CreateCommand};
use twilight_model::{
    application::interaction::InteractionChannel,
    channel::message::{
        Component, Embed, EmojiReactionType,
        component::{ActionRow, Button, ButtonStyle},
    },
    guild::{Permissions, Role},
    id::{
        Id,
        marker::{ChannelMarker, RoleMarker},
    },
};
use twilight_util::builder::embed::{EmbedBuilder, EmbedFooterBuilder};

use crate::{Cx, roles, util::BRAND};

const UNKNOWN_CHANNEL: u64 = 10003;
const UNKNOWN_MESSAGE: u64 = 10008;
const MISSING_ACCESS: u64 = 50001;
const MISSING_PERMISSIONS: u64 = 50013;

/// Buttons per action row, which is also Discord's limit.
const ROW_WIDTH: usize = 5;

fn manage_guild() -> Permissions {
    Permissions::MANAGE_GUILD
}

#[derive(CommandModel, CreateCommand)]
#[command(
    name = "roles",
    desc = "Role buttons: members pick their own roles",
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
}

#[derive(CommandModel, CreateCommand)]
#[command(name = "panel", desc = "Start a panel of role buttons, or change its description")]
pub struct RolesPanel {
    /// Title of the panel; it is how the other commands find it
    #[command(min_length = 1, max_length = 80)]
    title: String,
    /// Text shown above the buttons
    #[command(max_length = 500)]
    description: Option<String>,
}

#[derive(CommandModel, CreateCommand)]
#[command(name = "add", desc = "Add a role button to a panel (up to 25), or change its label")]
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
}

#[derive(CommandModel, CreateCommand)]
#[command(name = "remove", desc = "Take a role button off a panel")]
pub struct RolesRemove {
    /// The panel's title or number
    #[command(min_length = 1, max_length = 80)]
    panel: String,
    /// The role whose button to remove
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

/// The custom id of a role button: the panel and the role, nothing else.
fn button_id(panel: i64, role: u64) -> String {
    format!("rr:{panel}:{role}")
}

/// Reads what follows the `rr:` prefix of a custom id.
fn parse_button<'a>(mut parts: impl Iterator<Item = &'a str>) -> Option<(i64, u64)> {
    let panel = parts.next()?.parse().ok()?;
    let role = parts.next()?.parse().ok()?;
    parts.next().is_none().then_some((panel, role))
}

/// Reads the emoji a manager typed: a Unicode emoji, or a custom one as Discord writes it
/// (`<:name:id>`, `<a:name:id>`). Words are not emoji.
fn parse_emoji(text: &str) -> Option<EmojiReactionType> {
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

/// The panel as a message: its text with a line per role, and a button per role, five to a row.
fn panel_message(lang: Lang, panel: &PanelRow, roles: &[PanelRoleRow]) -> (Embed, Vec<Component>) {
    let mut lines: Vec<String> = Vec::new();
    lines.extend(panel.description.clone());
    if !lines.is_empty() {
        lines.push(String::new());
    }
    lines.extend(roles.iter().map(|r| {
        let emoji = r.emoji.as_deref().map_or_else(String::new, |e| format!("{e} "));
        format!("{emoji}**{}** · <@&{}>", r.label, r.role_id)
    }));
    let embed = EmbedBuilder::new()
        .color(BRAND)
        .title(&panel.title)
        .description(lines.join("\n"))
        .footer(EmbedFooterBuilder::new(t!(lang, "roles.footer")))
        .build();
    let components = roles
        .chunks(ROW_WIDTH)
        .map(|row| {
            Component::ActionRow(ActionRow {
                id: None,
                components: row
                    .iter()
                    .map(|r| {
                        Component::Button(Button {
                            custom_id: Some(button_id(panel.id, r.role_id as u64)),
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
        .collect();
    (embed, components)
}

/// Why Discord would not take the panel message, for the manager who asked.
fn post_refusal(lang: Lang, channel: Id<ChannelMarker>, error: &Error) -> String {
    match roles::error_code(error) {
        Some(MISSING_ACCESS | MISSING_PERMISSIONS) => {
            t!(lang, "post-refusal.access", channel = format!("<#{channel}>"))
        }
        Some(UNKNOWN_CHANNEL) => t!(lang, "post-refusal.gone"),
        Some(_) => t!(lang, "roles.refused"),
        None => t!(lang, "post-refusal.unreachable"),
    }
}

/// The panel called `key`, or a reply saying there is none (and `None`).
async fn find_panel(cx: &Cx, guild: u64, key: &str) -> anyhow::Result<Option<PanelRow>> {
    let panel = cx.app.db.find_panel(guild, key).await?;
    if panel.is_none() {
        cx.reply_ephemeral(t!(cx.lang(), "roles.panel-missing", name = key.trim()))
            .await?;
    }
    Ok(panel)
}

impl RolesCommand {
    pub async fn run(self, cx: &Cx) -> anyhow::Result<()> {
        let Some(guild) = cx.guild_id() else {
            return cx.reply_ephemeral(t!(cx.lang(), "roles.guild-only")).await;
        };
        if !cx.can_manage_guild() {
            return cx.reply_ephemeral(t!(cx.lang(), "roles.needs-manage")).await;
        }
        let db = &cx.app.db;
        match self {
            Self::Panel(c) => {
                let title = c.title.trim();
                if title.is_empty() {
                    return cx.reply_ephemeral(t!(cx.lang(), "roles.needs-title")).await;
                }
                let description = c.description.as_deref().map(str::trim).filter(|d| !d.is_empty());
                let (panel, created) = db.save_panel(guild, title, description).await?;
                cx.reply_ephemeral(if created {
                    t!(cx.lang(), "roles.draft", title = panel.title, id = panel.id)
                } else {
                    t!(cx.lang(), "roles.exists", title = panel.title)
                })
                .await
            }
            Self::Add(c) => {
                let Some(panel) = find_panel(cx, guild, &c.panel).await? else {
                    return Ok(());
                };
                let emoji = c.emoji.as_deref().map(str::trim).filter(|e| !e.is_empty());
                if emoji.is_some_and(|e| parse_emoji(e).is_none()) {
                    return cx.reply_ephemeral(t!(cx.lang(), "roles.bad-emoji")).await;
                }
                if let Some(why) = roles::unassignable(cx, guild, &c.role).await {
                    return cx.reply_ephemeral(why).await;
                }
                if !db
                    .add_panel_role(panel.id, c.role.id.get(), c.label.trim(), emoji)
                    .await?
                {
                    return cx
                        .reply_ephemeral(t!(cx.lang(), "roles.full", max = MAX_PANEL_ROLES))
                        .await;
                }
                let count = db.panel_roles(panel.id).await?.len();
                cx.reply_ephemeral(t!(
                    cx.lang(),
                    "roles.added",
                    role = format!("<@&{}>", c.role.id),
                    title = panel.title,
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
                let role = format!("<@&{}>", c.role);
                cx.reply_ephemeral(if db.remove_panel_role(panel.id, c.role.get()).await? {
                    t!(cx.lang(), "roles.removed", role = role, title = panel.title)
                } else {
                    t!(cx.lang(), "roles.not-on", role = role, title = panel.title)
                })
                .await
            }
            Self::Post(c) => post(cx, guild, c).await,
        }
    }
}

/// Posts the panel, or updates its message when it already sits in that channel.
async fn post(cx: &Cx, guild: u64, c: RolesPost) -> anyhow::Result<()> {
    let Some(panel) = find_panel(cx, guild, &c.panel).await? else {
        return Ok(());
    };
    let db = &cx.app.db;
    let roles = db.panel_roles(panel.id).await?;
    if roles.is_empty() {
        return cx
            .reply_ephemeral(t!(cx.lang(), "roles.no-roles", title = panel.title))
            .await;
    }
    let channel = c.channel.id;
    // The panel's message answers nobody in particular: it speaks the server's language.
    let (embed, components) = panel_message(cx.app.guild_lang(guild).await, &panel, &roles);
    let before = panel.channel_id.zip(panel.message_id);

    // Same channel: edit the message in place. A message someone deleted is posted afresh.
    if let Some((same, message)) = before.filter(|(posted_in, _)| *posted_in as u64 == channel.get()) {
        match cx
            .app
            .http
            .update_message(channel, Id::new(message as u64))
            .embeds(Some(std::slice::from_ref(&embed)))
            .components(Some(&components))
            .await
        {
            Ok(_) => {
                return cx
                    .reply_ephemeral(t!(
                        cx.lang(),
                        "roles.updated",
                        title = panel.title,
                        channel = format!("<#{same}>")
                    ))
                    .await;
            }
            Err(error) if roles::error_code(&error) == Some(UNKNOWN_MESSAGE) => {}
            Err(error) => {
                tracing::warn!(guild, panel = panel.id, %error, "could not update a role panel");
                return cx.reply_ephemeral(post_refusal(cx.lang(), channel, &error)).await;
            }
        }
    }

    let message = match cx
        .app
        .http
        .create_message(channel)
        .embeds(&[embed])
        .components(&components)
        .await
    {
        Ok(response) => response.model().await?,
        Err(error) => {
            tracing::warn!(guild, panel = panel.id, %error, "could not post a role panel");
            return cx.reply_ephemeral(post_refusal(cx.lang(), channel, &error)).await;
        }
    };
    db.set_panel_message(panel.id, channel.get(), message.id.get()).await?;
    let moved = before
        .filter(|(posted_in, _)| *posted_in as u64 != channel.get())
        .map_or_else(String::new, |(old, _)| {
            t!(cx.lang(), "roles.moved", old = format!("<#{old}>"))
        });
    cx.reply_ephemeral(t!(
        cx.lang(),
        "roles.posted",
        title = panel.title,
        channel = format!("<#{channel}>"),
        link = format!("https://discord.com/channels/{guild}/{channel}/{}", message.id),
        moved = moved
    ))
    .await
}

/// `rr` buttons: give the member the role, or take it if they have it.
pub(crate) async fn on_toggle<'a>(cx: &Cx, parts: impl Iterator<Item = &'a str>) -> anyhow::Result<()> {
    let Some(guild) = cx.guild_id() else {
        return cx.reply_ephemeral(t!(cx.lang(), "roles.buttons-guild-only")).await;
    };
    let Some((panel, role)) = parse_button(parts) else {
        return cx.reply_ephemeral(t!(cx.lang(), "social.button-expired")).await;
    };
    // The id came from the client; it counts only if that role still has a button on a panel of this server.
    if cx.app.db.panel_role(guild, panel, role).await?.is_none() {
        return cx.reply_ephemeral(t!(cx.lang(), "roles.gone-from-panel")).await;
    }
    let user = cx.author()?.id;
    let has_it = cx
        .interaction
        .member
        .as_ref()
        .is_some_and(|m| m.roles.iter().any(|r| r.get() == role));
    let (guild_id, role_id) = (Id::new(guild), Id::new(role));
    let result = if has_it {
        cx.app
            .http
            .remove_guild_member_role(guild_id, user, role_id)
            .reason("Role panel")
            .await
    } else {
        cx.app
            .http
            .add_guild_member_role(guild_id, user, role_id)
            .reason("Role panel")
            .await
    };
    match result {
        Ok(_) => {
            cx.reply_ephemeral(t!(
                cx.lang(),
                if has_it {
                    "roles.removed-role"
                } else {
                    "roles.added-role"
                },
                role = format!("<@&{role}>")
            ))
            .await
        }
        Err(error) => {
            tracing::warn!(guild, user = user.get(), role, %error, "could not change a role from a panel");
            let mut text = t!(
                cx.lang(),
                if has_it { "roles.cant-take" } else { "roles.cant-give" },
                role = format!("<@&{role}>"),
                why = roles::refusal_in(cx.lang(), &error)
            );
            if roles::role_is_gone(&error) {
                cx.app.db.forget_role(guild, role).await?;
                cx.app.forget_guild_settings(guild).await;
                text.push_str(&t!(cx.lang(), "roles.button-removed"));
            }
            cx.reply_ephemeral(text).await
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn button_ids_round_trip() {
        let id = button_id(12, 345_678_901_234_567_890);
        let mut parts = id.split(':');
        assert_eq!(parts.next(), Some("rr"));
        assert_eq!(parse_button(parts), Some((12, 345_678_901_234_567_890)));
        assert!(id.len() <= 100, "Discord's custom id limit");
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
    fn a_panel_becomes_rows_of_five_buttons() {
        let panel = PanelRow {
            id: 7,
            title: "Colours".to_owned(),
            description: Some("Pick one".to_owned()),
            channel_id: None,
            message_id: None,
        };
        let roles: Vec<PanelRoleRow> = (1..=12)
            .map(|n| PanelRoleRow {
                role_id: 1_000 + n,
                label: format!("Role {n}"),
                emoji: (n == 1).then(|| "🎮".to_owned()),
            })
            .collect();
        let (embed, rows) = panel_message(Lang::En, &panel, &roles);
        assert_eq!(embed.title.as_deref(), Some("Colours"));
        let description = embed.description.unwrap();
        assert!(
            description.starts_with("Pick one\n\n🎮 **Role 1** · <@&1001>"),
            "{description}"
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
}
