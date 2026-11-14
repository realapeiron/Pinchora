//! `/color`: members colour their name with a role. The roles come from the palette and are made the
//! first time somebody picks them; a server can also hand in roles of its own, let members type any hex
//! code, and limit the command to some roles.

mod picker;
use picker::*;
mod setup;
#[cfg(test)]
use setup::role_ids;
#[cfg(test)]
mod tests;


use pinchora_core::infra::i18n::Lang;
use pinchora_core::roles::automation::MAX_COLOR_ALLOWED;
use pinchora_core::roles::automation::MAX_COLOR_ROLES;
use pinchora_core::t;
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

use crate::App;
use crate::Cx;
use crate::commands::roles::automation::give;
use crate::commands::roles::automation::give::bot_view;
use crate::commands::roles::automation::give::refusal_text;
use crate::commands::roles::automation::palette::hex_of;
use crate::commands::roles::automation::palette::palette;
use crate::commands::roles::automation::palette::palette_colour;
use crate::commands::roles::automation::palette::rgb_of;
use crate::commands::roles::automation::panel::current_roles;
use crate::infra::util::BRAND;
use crate::roles::roles;

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

fn localized_palette_name(lang: Lang, name: &str) -> String {
    let key = format!("roles.color.palette.{}", name.to_ascii_lowercase());
    t!(lang, &key)
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
