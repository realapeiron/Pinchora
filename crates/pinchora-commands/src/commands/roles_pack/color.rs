//! `/color`: members colour their name with a role. The roles come from the palette and are made the
//! first time somebody picks them; a server can also hand in roles of its own, let members type any hex
//! code, and limit the command to some roles.

use pinchora_core::{
    i18n::Lang,
    roles_pack::{MAX_COLOR_ALLOWED, MAX_COLOR_ROLES},
    t,
};
use pinchora_db::{ColorAdded, ColorRoleRow, ColorSettings};
use pinchora_render::{AccentError, ColorCard, ColorCell, Theme, parse_accent};
use twilight_http::{Error, request::AuditLogReason};
use twilight_interactions::command::{CommandModel, CommandOption, CreateCommand, CreateOption};
use twilight_model::{
    channel::message::{
        Component,
        component::{ActionRow, Button, ButtonStyle, SelectMenu, SelectMenuOption, SelectMenuType},
    },
    guild::{Permissions, Role, RolePosition},
    http::interaction::InteractionResponseData,
    id::{Id, marker::RoleMarker},
};
use twilight_util::builder::embed::{EmbedBuilder, EmbedFooterBuilder};

use super::{
    give::{self, bot_view, refusal_text},
    palette::{hex_of, palette, palette_colour, rgb_of},
};
use crate::{App, Cx, roles, util::BRAND};

const MISSING_ACCESS: u64 = 50001;
const MISSING_PERMISSIONS: u64 = 50013;
const MAX_ROLES: u64 = 30005;
/// Options one menu holds, and the menus a message shows.
const MENU: usize = 25;
const MENUS: usize = 2;
/// Roles `/color setup roles:` takes in one go.
const SETUP_ROLES: usize = 10;

#[derive(CommandModel, CreateCommand)]
#[command(name = "color", desc = "Colour your name with a role", contexts = "guild")]
#[allow(clippy::large_enum_variant)] // parsed once per interaction, never stored
pub enum ColorCommand {
    #[command(name = "pick")]
    Pick(ColorPick),
    #[command(name = "set")]
    Set(ColorSet),
    #[command(name = "remove")]
    Remove(ColorRemove),
    #[command(name = "list")]
    List(ColorList),
    #[command(name = "setup")]
    Setup(ColorSetup),
}

#[derive(CommandModel, CreateCommand)]
#[command(name = "pick", desc = "Pick a colour for your name from a menu")]
pub struct ColorPick;

#[derive(CommandModel, CreateCommand)]
#[command(name = "set", desc = "Colour your name with any hex code, when the server allows it")]
pub struct ColorSet {
    /// A colour code like #ff6b4a
    #[command(min_length = 3, max_length = 9)]
    hex: String,
}

#[derive(CommandModel, CreateCommand)]
#[command(name = "remove", desc = "Take your name colour off")]
pub struct ColorRemove;

#[derive(CommandModel, CreateCommand)]
#[command(name = "list", desc = "Show the colours this server offers")]
pub struct ColorList;

#[derive(CommandOption, CreateOption, Clone, Copy)]
pub enum ColorMode {
    #[option(name = "Palette: only the server's colours", value = "palette")]
    Palette,
    #[option(name = "Any colour: members may type a hex code", value = "hex")]
    Hex,
}

impl ColorMode {
    fn key(self) -> &'static str {
        match self {
            Self::Palette => "palette",
            Self::Hex => "hex",
        }
    }
}

#[derive(CommandModel, CreateCommand)]
#[command(
    name = "setup",
    desc = "Set up colour roles: switch them on, who may pick, which roles"
)]
pub struct ColorSetup {
    /// Switch colour roles on or off
    enabled: Option<bool>,
    /// The palette only, or any hex code too
    mode: Option<ColorMode>,
    /// A role that may pick a colour (once any is named, only those roles may)
    allow: Option<Id<RoleMarker>>,
    /// Take a role off the list of those who may pick
    disallow: Option<Id<RoleMarker>>,
    /// New colour roles are made right under this role
    anchor: Option<Role>,
    /// Roles of your own to offer as colours: mention them (up to 10)
    #[command(max_length = 600)]
    roles: Option<String>,
}

/// One colour on offer.
struct Choice {
    name: String,
    /// `#rrggbb`.
    hex: String,
    rgb: u32,
}

/// The colours on offer: the palette, then the server's own colour roles that are not in it.
fn choices(rows: &[ColorRoleRow]) -> Vec<Choice> {
    let mut out: Vec<Choice> = palette()
        .iter()
        .map(|c| Choice {
            name: c.name.clone(),
            hex: c.hex.clone(),
            rgb: c.rgb,
        })
        .collect();
    for row in rows {
        if let Some(rgb) = rgb_of(&row.hex).filter(|_| palette_colour(&row.hex).is_none()) {
            out.push(Choice {
                name: row.name.clone(),
                hex: row.hex.clone(),
                rgb,
            });
        }
    }
    out.truncate(MENU * MENUS);
    out
}

/// Why a colour could not be made, for whoever asked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorStop {
    /// The bot lacks Manage Roles (or its role is not above the anchor).
    Permission,
    /// The server is at Discord's 250 roles.
    RoleLimit,
    /// The server has as many colour roles as it can.
    Full,
    Discord,
    Unreachable,
}

fn stop_of(error: &Error) -> ColorStop {
    match roles::error_code(error) {
        Some(MISSING_ACCESS | MISSING_PERMISSIONS) => ColorStop::Permission,
        Some(MAX_ROLES) => ColorStop::RoleLimit,
        Some(_) => ColorStop::Discord,
        None => ColorStop::Unreachable,
    }
}

/// What a [`ColorStop`] says, in `lang`.
pub fn stop_text(lang: Lang, stop: ColorStop) -> String {
    match stop {
        ColorStop::Permission => t!(lang, "roles.color.cannot-create-permission"),
        ColorStop::RoleLimit => t!(lang, "roles.color.cannot-create-limit"),
        ColorStop::Full => t!(lang, "roles.color.full", max = MAX_COLOR_ROLES),
        ColorStop::Discord => t!(lang, "roles.color.cannot-create-discord"),
        ColorStop::Unreachable => t!(lang, "roles.refusal.unreachable"),
    }
}

/// Makes the Discord role for a colour, under the anchor when the server set one, and records it.
/// A colour another role already has (two members asking at once) gives that role back and the new one
/// goes away again.
async fn make_role(app: &App, guild: u64, settings: &ColorSettings, name: &str, hex: &str) -> Result<u64, ColorStop> {
    let Some(rgb) = rgb_of(hex) else {
        return Err(ColorStop::Discord);
    };
    let guild_id = Id::new(guild.max(1));
    let made = app
        .http
        .create_role(guild_id)
        .name(name)
        .color(rgb)
        .permissions(Permissions::empty())
        .hoist(false)
        .mentionable(false)
        .reason("Colour role")
        .await;
    let role = match made {
        Ok(response) => response.model().await.map_err(|error| {
            tracing::warn!(guild, %error, "could not read a new colour role");
            ColorStop::Unreachable
        })?,
        Err(error) => {
            tracing::warn!(guild, %error, "could not make a colour role");
            return Err(stop_of(&error));
        }
    };
    if let Some(anchor) = settings.anchor_role_id {
        place_under(app, guild, role.id, anchor as u64).await;
    }
    match app
        .db
        .add_color_role(guild, role.id.get(), hex, name, MAX_COLOR_ROLES as i64)
        .await
    {
        Ok(ColorAdded::Added | ColorAdded::Updated) => Ok(role.id.get()),
        Ok(ColorAdded::Full | ColorAdded::HexTaken) | Err(_) => {
            // Somebody else got there first, or the server filled up meanwhile: this one is not needed.
            let _ = app.http.delete_role(guild_id, role.id).await;
            match app.db.color_roles(guild).await {
                Ok(rows) => rows
                    .iter()
                    .find(|r| r.hex == hex)
                    .map(|r| r.role_id as u64)
                    .ok_or(ColorStop::Full),
                Err(_) => Err(ColorStop::Unreachable),
            }
        }
    }
}

/// Moves a new role to just under the anchor role. Best effort: a role in the wrong place still works.
async fn place_under(app: &App, guild: u64, role: Id<RoleMarker>, anchor: u64) {
    let guild_id = Id::new(guild.max(1));
    let found = match app.http.roles(guild_id).await {
        Ok(response) => response.models().await.ok(),
        Err(_) => None,
    };
    let Some(position) = found
        .as_ref()
        .and_then(|roles| roles.iter().find(|r| r.id.get() == anchor))
        .map(|anchor| anchor.position)
    else {
        // The anchor is gone: forget it so the next role is not looked for under nothing.
        if let Err(error) = app.db.forget_pack_role(guild, anchor).await {
            tracing::warn!(guild, %error, "could not drop a deleted anchor role");
        }
        return;
    };
    let positions = [RolePosition {
        id: role,
        position: position.max(1) as u64,
    }];
    if let Err(error) = app.http.update_role_positions(guild_id, &positions).await {
        tracing::warn!(guild, %error, "could not place a colour role under its anchor");
    }
}

/// What making the palette's roles did.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ColorRoleReport {
    /// The names of the roles just made.
    pub created: Vec<String>,
    /// How many of the palette's colours had a role already.
    pub existing: usize,
    /// Why it stopped early, if it did.
    pub stopped: Option<ColorStop>,
}

/// Makes the role of every palette colour that has none yet (the dashboard's "create missing"), up to
/// the server's 32 colour roles, and stops at the first thing Discord refuses.
pub async fn create_palette_roles(app: &App, guild: u64) -> anyhow::Result<ColorRoleReport> {
    let settings = app.db.color_settings(guild).await?;
    let rows = app.db.color_roles(guild).await?;
    let mut report = ColorRoleReport::default();
    let mut count = rows.len();
    for colour in palette() {
        if rows.iter().any(|r| r.hex == colour.hex) {
            report.existing += 1;
            continue;
        }
        if count >= MAX_COLOR_ROLES {
            report.stopped = Some(ColorStop::Full);
            break;
        }
        match make_role(app, guild, &settings, &colour.name, &colour.hex).await {
            Ok(_) => {
                count += 1;
                report.created.push(colour.name.clone());
            }
            Err(stop) => {
                report.stopped = Some(stop);
                break;
            }
        }
    }
    Ok(report)
}

/// Takes a role off the server's colour roles, and off Discord too when `delete` says so. Returns whether
/// it was a colour role.
pub async fn delete_color_role(app: &App, guild: u64, role: u64, delete: bool) -> anyhow::Result<bool> {
    let was = app.db.remove_color_role(guild, role).await?;
    if was
        && delete
        && let Err(error) = app
            .http
            .delete_role(Id::new(guild.max(1)), Id::new(role.max(1)))
            .reason("Colour role removed")
            .await
        && !roles::role_is_gone(&error)
    {
        tracing::warn!(guild, role, %error, "could not delete a colour role");
    }
    Ok(was)
}

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
async fn view(
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
                name: c.name.clone(),
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
fn controls(offered: &[Choice], at: Option<usize>, lang: Lang) -> Vec<Component> {
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
                                label: c.name.chars().take(100).collect(),
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
async fn choose(cx: &Cx, guild: u64, hex: &str) -> anyhow::Result<Result<String, String>> {
    let lang = cx.lang();
    let settings = match allowed(cx, guild).await? {
        Ok(settings) => settings,
        Err(text) => return Ok(Err(text)),
    };
    let db = &cx.app.db;
    let rows = db.color_roles(guild).await?;
    let known = rows.iter().find(|r| r.hex == hex);
    let named = palette_colour(hex).map(|c| c.name.clone());
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
            match make_role(&cx.app, guild, &settings, &name, hex).await {
                Ok(role) => (role, name),
                Err(stop) => return Ok(Err(stop_text(lang, stop))),
            }
        }
    };
    let user = cx.author()?.id.get();
    let held = held(cx);
    // One colour at a time: the others the member holds come off first.
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
        }
    }
    if held.contains(&role) {
        return Ok(Ok(name));
    }
    if let Err(error) = give::give(&cx.app, guild, user, role, "Colour role").await {
        tracing::warn!(guild, user, role, %error, "could not give a colour role");
        if roles::role_is_gone(&error) {
            give::forget(&cx.app, guild, role).await;
        }
        return Ok(Err(t!(
            lang,
            "roles.color.cannot-give",
            role = format!("<@&{role}>"),
            why = refusal_text(lang, &error)
        )));
    }
    Ok(Ok(name))
}

/// Takes the member's colour off. A click on the menu's button (`on_menu`) updates the menu.
async fn remove(cx: &Cx, guild: u64, on_menu: bool) -> anyhow::Result<()> {
    let lang = cx.lang();
    let settings = cx.app.db.color_settings(guild).await?;
    let rows = cx.app.db.color_roles(guild).await?;
    let held = held(cx);
    let mine: Vec<u64> = rows
        .iter()
        .map(|r| r.role_id as u64)
        .filter(|r| held.contains(r))
        .collect();
    if mine.is_empty() {
        return cx.reply_ephemeral(t!(lang, "roles.color.none-to-remove")).await;
    }
    let user = cx.author()?.id.get();
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

/// `cr:s:<page>` menus and the `cr:x` button.
pub(crate) async fn on_component<'a>(
    cx: &Cx,
    mut parts: impl Iterator<Item = &'a str>,
    values: &[String],
) -> anyhow::Result<()> {
    let lang = cx.lang();
    let Some(guild) = cx.guild_id() else {
        return cx.reply_ephemeral(t!(lang, "roles.color.guild-only")).await;
    };
    match parts.next() {
        Some("x") => remove(cx, guild, true).await,
        Some("s") => {
            let Some(hex) = values
                .first()
                .map(|v| format!("#{v}"))
                .filter(|h| rgb_of(h).is_some() && *h == h.to_lowercase())
            else {
                return cx.reply_ephemeral(t!(lang, "roles.color.menu-expired")).await;
            };
            match choose(cx, guild, &hex).await? {
                Ok(name) => {
                    let settings = cx.app.db.color_settings(guild).await?;
                    let rows = cx.app.db.color_roles(guild).await?;
                    let data = view(
                        cx,
                        &settings,
                        &rows,
                        Some(&hex),
                        Some(t!(lang, "roles.color.now", name = name)),
                        true,
                    )
                    .await?;
                    cx.update(data).await
                }
                Err(text) => cx.reply_ephemeral(text).await,
            }
        }
        _ => cx.reply_ephemeral(t!(lang, "roles.color.menu-expired")).await,
    }
}

impl ColorSetup {
    async fn run(self, cx: &Cx, guild: u64) -> anyhow::Result<()> {
        let lang = cx.lang();
        let db = &cx.app.db;
        let mut settings = db.color_settings(guild).await?;
        let mut notes: Vec<String> = Vec::new();
        if let Some(on) = self.enabled {
            settings.enabled = on;
        }
        if let Some(mode) = self.mode {
            settings.mode = mode.key().to_owned();
        }
        if let Some(role) = self.allow {
            let id = role.get() as i64;
            if !settings.allowed_roles.contains(&id) {
                if settings.allowed_roles.len() >= MAX_COLOR_ALLOWED {
                    return cx
                        .reply_ephemeral(t!(lang, "roles.color.allowed-full", max = MAX_COLOR_ALLOWED))
                        .await;
                }
                settings.allowed_roles.push(id);
            }
        }
        if let Some(role) = self.disallow {
            settings.allowed_roles.retain(|r| *r != role.get() as i64);
        }
        if let Some(anchor) = &self.anchor {
            if let Some(why) = roles::unassignable(cx, guild, anchor).await {
                return cx.reply_ephemeral(why).await;
            }
            settings.anchor_role_id = Some(anchor.id.get() as i64);
        }
        if let Some(text) = self.roles.as_deref() {
            adopt(cx, guild, text, &mut notes).await?;
        }
        db.save_color_settings(guild, &settings).await?;
        let count = db.color_roles(guild).await?.len();
        let state = if settings.enabled {
            t!(lang, "roles.color.on")
        } else {
            t!(lang, "roles.color.off-word")
        };
        let mode = match settings.mode.as_str() {
            "hex" => t!(lang, "roles.color.mode-hex"),
            _ => t!(lang, "roles.color.mode-palette"),
        };
        let anchor = settings
            .anchor_role_id
            .map_or_else(|| t!(lang, "roles.color.anchor-none"), |r| format!("<@&{r}>"));
        let who = if settings.allowed_roles.is_empty() {
            t!(lang, "roles.color.allowed-everyone")
        } else {
            settings
                .allowed_roles
                .iter()
                .map(|r| format!("<@&{r}>"))
                .collect::<Vec<_>>()
                .join(", ")
        };
        let mut text = t!(
            lang,
            "roles.color.summary",
            state = state,
            mode = mode,
            anchor = anchor,
            who = who,
            count = count,
            max = MAX_COLOR_ROLES
        );
        for note in notes {
            text.push('\n');
            text.push_str(&note);
        }
        cx.reply_ephemeral(text).await
    }
}

/// Takes the roles mentioned in `text` as colour roles: each keeps its own colour and name.
async fn adopt(cx: &Cx, guild: u64, text: &str, notes: &mut Vec<String>) -> anyhow::Result<()> {
    let lang = cx.lang();
    let ids = role_ids(text, SETUP_ROLES);
    if ids.is_empty() {
        notes.push(t!(lang, "roles.color.no-roles-read"));
        return Ok(());
    }
    let Some((all, _)) = bot_view(&cx.app, guild).await else {
        notes.push(t!(lang, "roles.refusal.unreachable"));
        return Ok(());
    };
    for id in ids {
        let label = format!("<@&{id}>");
        let Some(role) = all.iter().find(|r| r.id.get() == id) else {
            notes.push(t!(lang, "roles.color.unknown-role", role = id));
            continue;
        };
        if let Some(why) = roles::unassignable(cx, guild, role).await {
            notes.push(why);
            continue;
        }
        let rgb = role.colors.primary_color;
        if rgb == 0 {
            notes.push(t!(lang, "roles.color.role-has-no-colour", role = label));
            continue;
        }
        let added = cx
            .app
            .db
            .add_color_role(guild, id, &hex_of(rgb), &role.name, MAX_COLOR_ROLES as i64)
            .await?;
        notes.push(match added {
            ColorAdded::Added | ColorAdded::Updated => t!(lang, "roles.color.adopted", role = label, hex = hex_of(rgb)),
            ColorAdded::Full => t!(lang, "roles.color.full", max = MAX_COLOR_ROLES),
            ColorAdded::HexTaken => t!(lang, "roles.color.hex-taken", role = label, hex = hex_of(rgb)),
        });
    }
    Ok(())
}

/// The role ids mentioned (`<@&123>`) or written as bare numbers in `text`, without repeats, at most `max`.
fn role_ids(text: &str, max: usize) -> Vec<u64> {
    let mut found: Vec<u64> = Vec::new();
    let mut digits = String::new();
    for c in text.chars().chain(std::iter::once(' ')) {
        if c.is_ascii_digit() {
            digits.push(c);
            continue;
        }
        if digits.len() >= 15
            && let Ok(id) = digits.parse::<u64>()
            && id > 0
            && !found.contains(&id)
        {
            found.push(id);
        }
        digits.clear();
    }
    found.truncate(max);
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn role_mentions_and_bare_ids_are_read() {
        let text = "<@&123456789012345678>, 223456789012345678 <@&123456789012345678> 12 <@&0> x";
        assert_eq!(role_ids(text, 10), [123_456_789_012_345_678, 223_456_789_012_345_678]);
        assert_eq!(role_ids("", 10), Vec::<u64>::new());
        let many: String = (1..=12u64)
            .map(|n| format!("<@&{}> ", 100_000_000_000_000_000 + n))
            .collect();
        assert_eq!(role_ids(&many, 10).len(), 10);
    }

    #[test]
    fn the_menu_offers_the_palette_then_the_servers_own_colours() {
        let own = ColorRoleRow {
            role_id: 5,
            hex: "#123456".to_owned(),
            name: "Midnight".to_owned(),
        };
        let in_palette = ColorRoleRow {
            role_id: 6,
            hex: palette()[0].hex.clone(),
            name: "Renamed".to_owned(),
        };
        let offered = choices(&[own, in_palette]);
        assert_eq!(
            offered.len(),
            palette().len() + 1,
            "a palette colour is not listed twice"
        );
        assert_eq!(offered[0].name, palette()[0].name);
        assert_eq!(offered.last().map(|c| c.name.as_str()), Some("Midnight"));
        assert!(offered.len() <= MENU * MENUS);
    }

    #[test]
    fn controls_split_into_menus_of_25_and_mark_the_current_colour() {
        let offered: Vec<Choice> = (0..30u32)
            .map(|i| Choice {
                name: format!("C{i}"),
                hex: hex_of(0x10_0000 + i),
                rgb: 0x10_0000 + i,
            })
            .collect();
        let rows = controls(&offered, Some(27), Lang::En);
        assert_eq!(rows.len(), 3, "two menus and the button");
        let menu = |row: &Component| match row {
            Component::ActionRow(r) => match &r.components[0] {
                Component::SelectMenu(m) => Some(m.clone()),
                _ => None,
            },
            _ => None,
        };
        let (first, second) = (menu(&rows[0]).unwrap(), menu(&rows[1]).unwrap());
        assert_eq!(first.custom_id, "cr:s:0");
        assert_eq!(second.custom_id, "cr:s:1");
        assert_eq!(first.options.as_ref().unwrap().len(), 25);
        assert_eq!(second.options.as_ref().unwrap().len(), 5);
        assert!(
            second.options.as_ref().unwrap()[2].default,
            "the member's colour is marked"
        );
        assert!(first.options.as_ref().unwrap().iter().all(|o| !o.default));
        assert!(first.options.as_ref().unwrap()[0].value.len() == 6);
    }
}
