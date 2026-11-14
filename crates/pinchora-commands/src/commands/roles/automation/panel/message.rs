use super::*;

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
pub(super) fn parse_button<'a>(mut parts: impl Iterator<Item = &'a str>) -> Option<(i64, u64)> {
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

pub(super) fn entry_line(entry: &PanelRoleRow, style: PanelStyle) -> String {
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
