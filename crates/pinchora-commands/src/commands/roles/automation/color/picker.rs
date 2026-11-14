use super::*;

impl ColorCommand {
    pub async fn run(self, cx: &Cx) -> anyhow::Result<()> {
        let lang = cx.lang();
        let Some(guild) = cx.guild_id() else {
            return cx.reply_ephemeral(t!(lang, "roles.color.guild-only")).await;
        };
        match self {
            Self::Pick(_) => show(cx, guild, true).await,
            Self::List(_) => show(cx, guild, false).await,
            Self::Set(c) => set_hex(cx, guild, &c.hex).await,
            Self::Remove(_) => remove(cx, guild, false).await,
            Self::Setup(c) => {
                if !cx.can_manage_guild() {
                    return cx.reply_ephemeral(t!(lang, "roles.color.manage-needed")).await;
                }
                c.run(cx, guild).await
            }
        }
    }
}

fn held(cx: &Cx) -> Vec<u64> {
    cx.interaction
        .member
        .as_ref()
        .map(|m| m.roles.iter().map(|r| r.get()).collect())
        .unwrap_or_default()
}

/// The server's colour settings if members may use them now, or what to say instead.
async fn allowed(cx: &Cx, guild: u64) -> anyhow::Result<Result<ColorSettings, String>> {
    let lang = cx.lang();
    let settings = cx.app.db.color_settings(guild).await?;
    if !settings.enabled {
        let mut text = t!(lang, "roles.color.off");
        if cx.can_manage_guild() {
            text.push(' ');
            text.push_str(&t!(lang, "roles.color.off-manager"));
        }
        return Ok(Err(text));
    }
    let held = held(cx);
    if !settings.allowed_roles.is_empty() && !settings.allowed_roles.iter().any(|r| held.contains(&(*r as u64))) {
        let list = settings
            .allowed_roles
            .iter()
            .map(|r| format!("<@&{r}>"))
            .collect::<Vec<_>>()
            .join(", ");
        return Ok(Err(t!(lang, "roles.color.not-allowed", roles = list)));
    }
    Ok(Ok(settings))
}

/// The hex of the colour role the member holds, if any.
fn current(rows: &[ColorRoleRow], held: &[u64]) -> Option<String> {
    rows.iter()
        .find(|r| held.contains(&(r.role_id as u64)))
        .map(|r| r.hex.clone())
}

/// The colours as a card with a menu to pick from (`interactive`), or just the card.
pub(super) async fn view(
    cx: &Cx,
    settings: &ColorSettings,
    rows: &[ColorRoleRow],
    selected: Option<&str>,
    notice: Option<String>,
    interactive: bool,
) -> anyhow::Result<InteractionResponseData> {
    let lang = cx.lang();
    let offered = choices(rows);
    let at = selected.and_then(|hex| offered.iter().position(|c| c.hex == hex));
    let card = ColorCard {
        title: t!(lang, "roles.color.card-title"),
        hint: t!(lang, "roles.color.card-hint"),
        cells: offered
            .iter()
            .map(|c| ColorCell {
                name: if palette_colour(&c.hex).is_some() {
                    localized_palette_name(lang, &c.name)
                } else {
                    c.name.clone()
                },
                rgb: c.rgb,
            })
            .collect(),
        selected: at,
        sample: cx.author_display_name(),
        none: t!(lang, "roles.color.card-none"),
        theme: Theme::reef(),
    };
    let png = cx.app.render(move |r| r.color_card(&card)).await?;
    let mode = match settings.mode.as_str() {
        "hex" => t!(lang, "roles.color.mode-hex"),
        _ => t!(lang, "roles.color.mode-palette"),
    };
    let embed = EmbedBuilder::new()
        .color(BRAND)
        .title(t!(lang, "roles.color.embed-title"))
        .description(notice.unwrap_or_else(|| {
            if interactive {
                t!(lang, "roles.color.pick-line")
            } else {
                t!(lang, "roles.color.list-line")
            }
        }))
        .footer(EmbedFooterBuilder::new(mode));
    let components = if interactive {
        controls(&offered, at, lang)
    } else {
        Vec::new()
    };
    Ok(Cx::card(embed, "colors.png", png, components, true))
}

/// The menus that hold the colours, and the button that takes the colour off.
pub(super) fn controls(offered: &[Choice], at: Option<usize>, lang: Lang) -> Vec<Component> {
    let mut rows: Vec<Component> = offered
        .chunks(MENU)
        .enumerate()
        .map(|(page, chunk)| {
            let first = page * MENU;
            Component::ActionRow(ActionRow {
                id: None,
                components: vec![Component::SelectMenu(SelectMenu {
                    channel_types: None,
                    custom_id: format!("cr:s:{page}"),
                    default_values: None,
                    disabled: false,
                    id: None,
                    kind: SelectMenuType::Text,
                    max_values: Some(1),
                    min_values: Some(1),
                    options: Some(
                        chunk
                            .iter()
                            .enumerate()
                            .map(|(i, c)| SelectMenuOption {
                                default: at == Some(first + i),
                                description: None,
                                emoji: None,
                                label: if palette_colour(&c.hex).is_some() {
                                    localized_palette_name(lang, &c.name)
                                } else {
                                    c.name.clone()
                                }
                                .chars()
                                .take(100)
                                .collect(),
                                value: c.hex.trim_start_matches('#').to_owned(),
                            })
                            .collect(),
                    ),
                    placeholder: Some(t!(lang, "roles.color.menu", from = first + 1, to = first + chunk.len())),
                    required: None,
                })],
            })
        })
        .collect();
    rows.push(Component::ActionRow(ActionRow {
        id: None,
        components: vec![Component::Button(Button {
            custom_id: Some("cr:x".to_owned()),
            disabled: at.is_none(),
            emoji: None,
            id: None,
            label: Some(t!(lang, "roles.color.remove-button")),
            sku_id: None,
            style: ButtonStyle::Secondary,
            url: None,
        })],
    }));
    rows
}

/// `/color pick` and `/color list`.
async fn show(cx: &Cx, guild: u64, interactive: bool) -> anyhow::Result<()> {
    let settings = match allowed(cx, guild).await? {
        Ok(settings) => settings,
        Err(text) => return cx.reply_ephemeral(text).await,
    };
    let rows = cx.app.db.color_roles(guild).await?;
    let mine = current(&rows, &held(cx));
    let data = view(cx, &settings, &rows, mine.as_deref(), None, interactive).await?;
    cx.reply(data).await
}

/// `/color set`: any colour, when the server's mode is `hex`.
async fn set_hex(cx: &Cx, guild: u64, text: &str) -> anyhow::Result<()> {
    let lang = cx.lang();
    let rgb = match parse_accent(text) {
        Ok(rgb) => rgb,
        Err(AccentError::Malformed) => return cx.reply_ephemeral(t!(lang, "roles.color.hex-malformed")).await,
        Err(AccentError::TooDark) => return cx.reply_ephemeral(t!(lang, "roles.color.hex-too-dark")).await,
    };
    match choose(cx, guild, &hex_of(rgb)).await? {
        Ok(name) => cx.reply_ephemeral(t!(lang, "roles.color.now", name = name)).await,
        Err(text) => cx.reply_ephemeral(text).await,
    }
}

/// Gives the member the colour `hex` (`#rrggbb`) and takes their other colour away: its role is made
/// first when it does not exist yet. Returns the colour's name, or what to tell the member.
pub(super) async fn choose(cx: &Cx, guild: u64, hex: &str) -> anyhow::Result<Result<String, String>> {
    let lang = cx.lang();
    let user = cx.author()?.id.get();
    let _lock = cx.app.db.lock_role_member(guild, user).await?;
    let settings = match allowed(cx, guild).await? {
        Ok(settings) => settings,
        Err(text) => return Ok(Err(text)),
    };
    let db = &cx.app.db;
    let rows = db.color_roles(guild).await?;
    let known = rows.iter().find(|r| r.hex == hex);
    let named = palette_colour(hex).map(|c| localized_palette_name(lang, &c.name));
    if known.is_none() && named.is_none() && settings.mode != "hex" {
        return Ok(Err(t!(lang, "roles.color.palette-only")));
    }
    let (role, name) = match known {
        Some(row) => (row.role_id as u64, named.unwrap_or_else(|| row.name.clone())),
        None => {
            if rows.len() >= MAX_COLOR_ROLES {
                return Ok(Err(stop_text(lang, ColorStop::Full)));
            }
            let name = named.unwrap_or_else(|| hex.to_uppercase());
            let discord_name = palette_colour(hex).map_or(name.as_str(), |c| c.name.as_str());
            match make_role(&cx.app, guild, &settings, discord_name, hex).await {
                Ok(role) => (role, name),
                Err(stop) => return Ok(Err(stop_text(lang, stop))),
            }
        }
    };
    let held = current_roles(&cx.app, guild, user).await?;
    if !held.contains(&role)
        && let Err(error) = give::validate(&cx.app, guild, role).await
    {
        return Ok(Err(t!(
            lang,
            "roles.color.cannot-give",
            role = format!("<@&{role}>"),
            why = give::give_refusal_text(lang, &error)
        )));
    }
    // One colour at a time: the others the member holds come off first.
    let mut removed = Vec::new();
    for other in rows
        .iter()
        .map(|r| r.role_id as u64)
        .filter(|r| *r != role && held.contains(r))
    {
        if let Err(error) = give::take(&cx.app, guild, user, other, "Colour role").await {
            tracing::warn!(guild, user, role = other, %error, "could not take an old colour role");
            if roles::role_is_gone(&error) {
                give::forget(&cx.app, guild, other).await;
            }
            for previous in removed {
                if let Err(restore) = give::give(&cx.app, guild, user, previous, "Colour swap rollback").await {
                    tracing::error!(guild, user, role = previous, %restore, "could not restore an old colour");
                }
            }
            return Ok(Err(t!(
                lang,
                "roles.color.cannot-take",
                role = format!("<@&{other}>"),
                why = refusal_text(lang, &error)
            )));
        }
        removed.push(other);
    }
    if held.contains(&role) {
        return Ok(Ok(name));
    }
    if let Err(error) = give::give(&cx.app, guild, user, role, "Colour role").await {
        tracing::warn!(guild, user, role, %error, "could not give a colour role");
        if error.downcast_ref::<Error>().is_some_and(roles::role_is_gone) {
            give::forget(&cx.app, guild, role).await;
        }
        for previous in removed {
            if let Err(restore) = give::give(&cx.app, guild, user, previous, "Colour swap rollback").await {
                tracing::error!(guild, user, role = previous, %restore, "could not restore an old colour");
            }
        }
        return Ok(Err(t!(
            lang,
            "roles.color.cannot-give",
            role = format!("<@&{role}>"),
            why = give::give_refusal_text(lang, &error)
        )));
    }
    Ok(Ok(name))
}

/// Takes the member's colour off. A click on the menu's button (`on_menu`) updates the menu.
pub(super) async fn remove(cx: &Cx, guild: u64, on_menu: bool) -> anyhow::Result<()> {
    let lang = cx.lang();
    let user = cx.author()?.id.get();
    let _lock = cx.app.db.lock_role_member(guild, user).await?;
    let settings = cx.app.db.color_settings(guild).await?;
    let rows = cx.app.db.color_roles(guild).await?;
    let held = current_roles(&cx.app, guild, user).await?;
    let mine: Vec<u64> = rows
        .iter()
        .map(|r| r.role_id as u64)
        .filter(|r| held.contains(r))
        .collect();
    if mine.is_empty() {
        return cx.reply_ephemeral(t!(lang, "roles.color.none-to-remove")).await;
    }
    for role in &mine {
        if let Err(error) = give::take(&cx.app, guild, user, *role, "Colour role").await {
            tracing::warn!(guild, user, role, %error, "could not take a colour role");
            return cx
                .reply_ephemeral(t!(
                    lang,
                    "roles.color.cannot-take",
                    role = format!("<@&{role}>"),
                    why = refusal_text(lang, &error)
                ))
                .await;
        }
    }
    if on_menu {
        let data = view(cx, &settings, &rows, None, Some(t!(lang, "roles.color.removed")), true).await?;
        return cx.update(data).await;
    }
    cx.reply_ephemeral(t!(lang, "roles.color.removed")).await
}

