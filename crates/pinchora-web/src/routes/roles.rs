//! The roles page: auto roles and sticky roles, role panels (buttons, menu, reactions) and colour roles.
//! The same rules and limits as `/autoroles`, `/roles` and `/color`, and the bot posts the panels.

use std::collections::{HashMap, HashSet};

use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{delete, get, post, put},
};
use pinchora_cache::Invalidation;
use pinchora_commands::{
    PostRefusal, check_emoji, color_palette, color_stop_text, create_palette_roles, delete_color_role, post_role_panel,
    role_rules_problem, start_sticky_snapshot, sticky_changed,
};
use pinchora_core::{
    i18n::Lang,
    roles_pack::{
        AutoKind, MAX_AUTO_ROLES, MAX_COLOR_ALLOWED, MAX_COLOR_ROLES, MAX_DELAY_SECS, MAX_EXPIRY_SECS,
        MAX_PANEL_ENTRIES, MIN_EXPIRY_SECS, PanelStyle, RulesError, check_rules,
    },
    settings::FieldError,
};
use pinchora_db::{
    ColorSettings, MAX_PANEL_ROLES, NewAutoRole, PanelDoc, PanelEntry, PanelRoleDoc, PanelRoleRow, PanelRow, PanelSaved,
};
use serde::Deserialize;
use serde_json::{Value, json};

use super::settings::{parse_id, present};
use crate::{
    audit::{self, Change},
    error::{ApiError, ApiJson, ApiResult},
    extract::ManagedGuild,
    guild_info::{self, GuildInfo},
    state::WebState,
};

pub fn router() -> Router<WebState> {
    Router::new()
        .route("/", get(page))
        .route("/auto/{role}/{kind}", put(put_auto).delete(delete_auto))
        .route("/sticky", put(put_sticky))
        .route("/panels", post(create_panel))
        .route("/panels/{id}", put(save_panel).delete(delete_panel))
        .route("/panels/{id}/post", post(post_panel))
        .route("/colors", put(put_colors))
        .route("/colors/create-missing", post(create_missing))
        .route("/colors/{role}", delete(remove_color))
}

/// The shortest delay that is not "at once": the bot's ticker looks every half minute.
const MIN_DELAY_SECS: i64 = 30;
const TITLE_MAX: usize = 80;
const DESCRIPTION_MAX: usize = 500;
const LABEL_MAX: usize = 80;
const LINE_MAX: usize = 100;

fn path_id(params: &HashMap<String, String>, name: &str) -> ApiResult<u64> {
    params
        .get(name)
        .and_then(|raw| parse_id(raw))
        .ok_or_else(|| ApiError::malformed("Discord ids are numbers."))
}

fn auto_json(row: &pinchora_db::AutoRoleRow) -> Value {
    json!({
        "role_id": row.role_id.to_string(),
        "kind": row.kind,
        "delay_secs": row.delay_secs,
        "remove_after_secs": row.remove_after_secs,
        "enabled": row.enabled,
    })
}

fn entry_json(entry: &PanelRoleRow) -> Value {
    json!({
        "role_id": entry.role_id.to_string(),
        "label": entry.label,
        "emoji": entry.emoji,
        "description": entry.description,
        "temp_secs": entry.temp_secs,
    })
}

fn panel_json(panel: &PanelRow, entries: &[PanelRoleRow]) -> Value {
    json!({
        "id": panel.id.to_string(),
        "title": panel.title,
        "description": panel.description,
        "style": panel.style,
        "exclusive": panel.exclusive,
        "required_role": panel.required_role.map(|r| r.to_string()),
        "min_picks": panel.min_picks,
        "max_picks": panel.max_picks,
        "channel_id": panel.channel_id.map(|c| c.to_string()),
        "message_id": panel.message_id.map(|m| m.to_string()),
        "roles": entries.iter().map(entry_json).collect::<Vec<_>>(),
    })
}

/// Everything the page shows, in one answer: every write answers with it again.
async fn view(state: &WebState, guild: u64) -> ApiResult<Value> {
    let db = &state.app.db;
    let auto: Vec<Value> = db.auto_roles(guild).await?.iter().map(auto_json).collect();
    let mut panels = Vec::new();
    for panel in db.panels(guild).await? {
        let entries = db.panel_roles(panel.id).await?;
        panels.push(panel_json(&panel, &entries));
    }
    let settings = db.color_settings(guild).await?;
    let colours = db.color_roles(guild).await?;
    let palette: Vec<Value> = color_palette()
        .iter()
        .map(|c| json!({ "name": c.name, "hex": c.hex }))
        .collect();
    Ok(json!({
        "auto": {
            "items": auto,
            "max": MAX_AUTO_ROLES,
            "sticky": db.sticky_enabled(guild).await?,
            "max_delay_secs": MAX_DELAY_SECS,
            "max_remove_after_secs": MAX_EXPIRY_SECS,
        },
        "panels": { "items": panels, "max_roles": MAX_PANEL_ROLES },
        "colors": {
            "enabled": settings.enabled,
            "mode": settings.mode,
            "allowed_roles": settings.allowed_roles.iter().map(|r| r.to_string()).collect::<Vec<_>>(),
            "anchor_role_id": settings.anchor_role_id.map(|r| r.to_string()),
            "roles": colours
                .iter()
                .map(|c| json!({ "role_id": c.role_id.to_string(), "hex": c.hex, "name": c.name }))
                .collect::<Vec<_>>(),
            "palette": palette,
            "max": MAX_COLOR_ROLES,
            "max_allowed": MAX_COLOR_ALLOWED,
        },
    }))
}

/// `GET /roles`.
pub async fn page(State(state): State<WebState>, managed: ManagedGuild) -> ApiResult<Json<Value>> {
    Ok(Json(view(&state, managed.id).await?))
}

// /Auto roles

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AutoBody {
    #[serde(default)]
    delay_secs: i64,
    #[serde(default)]
    remove_after_secs: i64,
    #[serde(default = "yes")]
    enabled: bool,
}

fn yes() -> bool {
    true
}

fn kind_of(params: &HashMap<String, String>) -> ApiResult<Option<AutoKind>> {
    match params.get("kind").map(String::as_str) {
        Some("all") => Ok(None),
        Some(key) => AutoKind::parse(key).map(Some).ok_or_else(ApiError::not_found),
        None => Err(ApiError::not_found()),
    }
}

/// `PUT /roles/auto/{role}/{kind}`: gives a role to everyone of a kind, or changes its delay and expiry.
pub async fn put_auto(
    State(state): State<WebState>,
    managed: ManagedGuild,
    Path(params): Path<HashMap<String, String>>,
    ApiJson(body): ApiJson<AutoBody>,
) -> ApiResult<Json<Value>> {
    let (guild, role) = (managed.id, path_id(&params, "role")?);
    let Some(kind) = kind_of(&params)? else {
        return Err(ApiError::not_found());
    };
    let mut errors = Vec::new();
    if body.delay_secs != 0 && !(MIN_DELAY_SECS..=MAX_DELAY_SECS).contains(&body.delay_secs) {
        errors.push(FieldError::new(
            "delay_secs",
            "The delay is none, or 30 seconds to 30 days.",
        ));
    }
    if body.remove_after_secs != 0 && !(MIN_EXPIRY_SECS..=MAX_EXPIRY_SECS).contains(&body.remove_after_secs) {
        errors.push(FieldError::new(
            "remove_after_secs",
            "The time a role lasts is none, or a minute to a year.",
        ));
    }
    let info = guild_info::live(&state, guild).await?;
    let grantor = guild_info::grantor(&state, &managed, &info.roles, true).await?;
    if let Err(error) = info.check_grant("role_id", role, grantor) {
        errors.push(error);
    }
    if !errors.is_empty() {
        return Err(ApiError::invalid(errors));
    }
    let db = &state.app.db;
    let was = db
        .auto_roles(guild)
        .await?
        .into_iter()
        .find(|r| r.role_id as u64 == role && r.kind == kind.key());
    let auto = NewAutoRole {
        role_id: role,
        kind: kind.key(),
        delay_secs: body.delay_secs as i32,
        remove_after_secs: body.remove_after_secs as i32,
        enabled: body.enabled,
    };
    if !db.save_auto_role(guild, &auto, MAX_AUTO_ROLES as i64).await? {
        return Err(ApiError::limit(format!(
            "A server can have {MAX_AUTO_ROLES} auto roles. Remove one first."
        )));
    }
    let target = format!("{role}:{}", kind.key());
    audit::finish(
        &state,
        &managed.current.session,
        Change {
            guild,
            action: "roles.auto.put",
            target: Some(&target),
            before: was.map_or(Value::Null, |w| {
                json!({ "delay_secs": w.delay_secs, "remove_after_secs": w.remove_after_secs, "enabled": w.enabled })
            }),
            after: json!({
                "delay_secs": body.delay_secs,
                "remove_after_secs": body.remove_after_secs,
                "enabled": body.enabled,
            }),
        },
        Some(Invalidation::Guild(guild)),
    )
    .await;
    Ok(Json(view(&state, guild).await?))
}

/// `DELETE /roles/auto/{role}/{kind}`: `kind` is a kind, or `all`.
pub async fn delete_auto(
    State(state): State<WebState>,
    managed: ManagedGuild,
    Path(params): Path<HashMap<String, String>>,
) -> ApiResult<Json<Value>> {
    let (guild, role) = (managed.id, path_id(&params, "role")?);
    let kind = kind_of(&params)?;
    let removed = state
        .app
        .db
        .remove_auto_role(guild, role, kind.map(AutoKind::key))
        .await?;
    if removed > 0 {
        let target = role.to_string();
        audit::finish(
            &state,
            &managed.current.session,
            Change {
                guild,
                action: "roles.auto.delete",
                target: Some(&target),
                before: json!({ "kind": kind.map_or("all", AutoKind::key) }),
                after: Value::Null,
            },
            Some(Invalidation::Guild(guild)),
        )
        .await;
    }
    Ok(Json(view(&state, guild).await?))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StickyBody {
    enabled: bool,
}

/// `PUT /roles/sticky`: members get their roles back when they leave and come back.
pub async fn put_sticky(
    State(state): State<WebState>,
    managed: ManagedGuild,
    ApiJson(body): ApiJson<StickyBody>,
) -> ApiResult<Json<Value>> {
    let guild = managed.id;
    let was = state.app.db.sticky_enabled(guild).await?;
    state.app.db.set_sticky(guild, body.enabled).await?;
    sticky_changed(guild).await;
    if body.enabled && !was {
        start_sticky_snapshot(&state.app, guild).await;
    }
    if was != body.enabled {
        audit::finish(
            &state,
            &managed.current.session,
            Change {
                guild,
                action: "roles.sticky",
                target: None,
                before: json!({ "enabled": was }),
                after: json!({ "enabled": body.enabled }),
            },
            Some(Invalidation::Guild(guild)),
        )
        .await;
    }
    Ok(Json(view(&state, guild).await?))
}

// /Panels

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PanelBody {
    title: String,
    #[serde(default, deserialize_with = "present")]
    description: Option<Option<String>>,
    #[serde(default = "buttons")]
    style: String,
    #[serde(default)]
    exclusive: bool,
    #[serde(default, deserialize_with = "present")]
    required_role: Option<Option<String>>,
    #[serde(default)]
    min_picks: i64,
    #[serde(default)]
    max_picks: i64,
    #[serde(default)]
    roles: Vec<PanelRoleBody>,
}

fn buttons() -> String {
    PanelStyle::Buttons.key().to_owned()
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PanelRoleBody {
    role_id: String,
    label: String,
    #[serde(default, deserialize_with = "present")]
    emoji: Option<Option<String>>,
    #[serde(default, deserialize_with = "present")]
    description: Option<Option<String>>,
    #[serde(default, deserialize_with = "present")]
    temp_secs: Option<Option<i64>>,
}

/// Trims `text`, and `None` when nothing is left.
fn trimmed(text: Option<Option<String>>) -> Option<String> {
    text.flatten().map(|t| t.trim().to_owned()).filter(|t| !t.is_empty())
}

/// Validate every role in the submitted panel against current bot and actor permissions,
/// including roles retained from the previous version.
fn check_panel(
    body: PanelBody,
    info: &GuildInfo,
    grantor: pinchora_core::settings::Grantor,
    _already: &HashSet<u64>,
) -> Result<PanelDoc, Vec<FieldError>> {
    let mut errors = Vec::new();
    let title = body.title.trim().to_owned();
    if title.is_empty() || title.chars().count() > TITLE_MAX {
        errors.push(FieldError::new(
            "title",
            format!("The title is 1 to {TITLE_MAX} characters."),
        ));
    }
    let description = trimmed(body.description);
    if description
        .as_ref()
        .is_some_and(|d| d.chars().count() > DESCRIPTION_MAX)
    {
        errors.push(FieldError::new(
            "description",
            format!("The text is at most {DESCRIPTION_MAX} characters."),
        ));
    }
    let style = PanelStyle::parse(&body.style);
    if style.is_none() {
        errors.push(FieldError::new("style", "The style is buttons, select or reactions."));
    }
    if (0..=MAX_PANEL_ENTRIES as i64).contains(&body.min_picks)
        && (0..=MAX_PANEL_ENTRIES as i64).contains(&body.max_picks)
    {
        if let Err(error) = check_rules(body.exclusive, body.min_picks as usize, body.max_picks as usize) {
            let field = if error == RulesError::TooMany {
                "max_picks"
            } else {
                "min_picks"
            };
            errors.push(FieldError::new(field, role_rules_problem(Lang::En, error)));
        }
    } else {
        errors.push(FieldError::new(
            "max_picks",
            format!("A limit is 0 to {MAX_PANEL_ENTRIES}."),
        ));
    }
    let required = match trimmed(body.required_role) {
        None => None,
        Some(raw) => match parse_id(&raw) {
            Some(id) => match info.check_role("required_role", id) {
                Ok(_) => Some(id),
                Err(error) => {
                    errors.push(error);
                    None
                }
            },
            None => {
                errors.push(FieldError::new("required_role", "That isn't a Discord id."));
                None
            }
        },
    };
    if body.roles.len() > MAX_PANEL_ROLES as usize {
        errors.push(FieldError::new(
            "roles",
            format!("A panel holds at most {MAX_PANEL_ROLES} roles."),
        ));
    }
    let mut seen: HashSet<u64> = HashSet::new();
    let mut roles = Vec::new();
    for entry in body.roles {
        let label = entry.label.trim().to_owned();
        let Some(role) = parse_id(entry.role_id.trim()) else {
            errors.push(FieldError::new("roles", "A role there isn't a Discord id."));
            continue;
        };
        if !seen.insert(role) {
            errors.push(FieldError::new("roles", "A role is on the panel twice."));
            continue;
        }
        if label.is_empty() || label.chars().count() > LABEL_MAX {
            errors.push(FieldError::new(
                "roles",
                format!("Every label is 1 to {LABEL_MAX} characters."),
            ));
        }
        if let Err(error) = info.check_grant("roles", role, grantor) {
            errors.push(error);
        }
        let emoji = trimmed(entry.emoji);
        let emoji_id = match emoji.as_deref().map(check_emoji) {
            Some(None) => {
                errors.push(FieldError::new(
                    "roles",
                    format!("I couldn't read the emoji on {label}. Use a Unicode emoji or a custom one like <:name:123456789>."),
                ));
                None
            }
            Some(Some(id)) => id,
            None => None,
        };
        let line = trimmed(entry.description);
        if line.as_ref().is_some_and(|l| l.chars().count() > LINE_MAX) {
            errors.push(FieldError::new(
                "roles",
                format!("A line under a label is at most {LINE_MAX} characters."),
            ));
        }
        let temp_secs = match entry.temp_secs.flatten() {
            None | Some(0) => None,
            Some(secs) if (MIN_EXPIRY_SECS..=MAX_EXPIRY_SECS).contains(&secs) => Some(secs as i32),
            Some(_) => {
                errors.push(FieldError::new(
                    "roles",
                    "How long a role lasts is none, or a minute to a year.",
                ));
                None
            }
        };
        roles.push(PanelRoleDoc {
            role_id: role,
            entry: PanelEntry {
                label,
                emoji,
                emoji_id,
                description: line,
            },
            temp_secs,
        });
    }
    if !errors.is_empty() {
        return Err(errors);
    }
    Ok(PanelDoc {
        title,
        description,
        style: style.unwrap_or_default().key().to_owned(),
        exclusive: body.exclusive,
        required_role: required,
        min_picks: body.min_picks as i32,
        max_picks: body.max_picks as i32,
        roles,
    })
}

fn title_taken() -> ApiError {
    ApiError::invalid(vec![FieldError::new("title", "Another panel already has that title.")])
}

/// `POST /roles/panels`: makes a panel (a draft until it is posted).
pub async fn create_panel(
    State(state): State<WebState>,
    managed: ManagedGuild,
    ApiJson(body): ApiJson<PanelBody>,
) -> ApiResult<Json<Value>> {
    let guild = managed.id;
    let info = guild_info::live(&state, guild).await?;
    let grantor = guild_info::grantor(&state, &managed, &info.roles, true).await?;
    let doc = check_panel(body, &info, grantor, &HashSet::new()).map_err(ApiError::invalid)?;
    let PanelSaved::Saved(panel) = state.app.db.create_panel_doc(guild, &doc).await? else {
        return Err(title_taken());
    };
    let target = panel.id.to_string();
    audit::finish(
        &state,
        &managed.current.session,
        Change {
            guild,
            action: "roles.panel.create",
            target: Some(&target),
            before: Value::Null,
            after: json!({ "title": doc.title, "style": doc.style, "roles": doc.roles.len() }),
        },
        None,
    )
    .await;
    Ok(Json(view(&state, guild).await?))
}

/// `PUT /roles/panels/{id}`: saves a panel's settings and roles, as the editor has them.
pub async fn save_panel(
    State(state): State<WebState>,
    managed: ManagedGuild,
    Path(params): Path<HashMap<String, String>>,
    ApiJson(body): ApiJson<PanelBody>,
) -> ApiResult<Json<Value>> {
    let (guild, id) = (managed.id, path_id(&params, "id")? as i64);
    let db = &state.app.db;
    let Some(was) = db.panel_by_id(guild, id).await? else {
        return Err(ApiError::not_found());
    };
    let now: HashSet<u64> = db.panel_roles(id).await?.iter().map(|r| r.role_id as u64).collect();
    let info = guild_info::live(&state, guild).await?;
    let grantor = guild_info::grantor(&state, &managed, &info.roles, true).await?;
    let doc = check_panel(body, &info, grantor, &now).map_err(ApiError::invalid)?;
    match db.replace_panel_doc(guild, id, &doc).await? {
        PanelSaved::Saved(_) => {}
        PanelSaved::Missing => return Err(ApiError::not_found()),
        PanelSaved::TitleTaken => return Err(title_taken()),
    }
    let target = id.to_string();
    audit::finish(
        &state,
        &managed.current.session,
        Change {
            guild,
            action: "roles.panel.save",
            target: Some(&target),
            before: json!({ "title": was.title, "style": was.style }),
            after: json!({ "title": doc.title, "style": doc.style, "roles": doc.roles.len() }),
        },
        None,
    )
    .await;
    Ok(Json(view(&state, guild).await?))
}

/// `DELETE /roles/panels/{id}`: the message stays in its channel, and its buttons stop answering.
pub async fn delete_panel(
    State(state): State<WebState>,
    managed: ManagedGuild,
    Path(params): Path<HashMap<String, String>>,
) -> ApiResult<Json<Value>> {
    let (guild, id) = (managed.id, path_id(&params, "id")? as i64);
    if let Some(was) = state.app.db.delete_panel(guild, id).await? {
        let target = id.to_string();
        audit::finish(
            &state,
            &managed.current.session,
            Change {
                guild,
                action: "roles.panel.delete",
                target: Some(&target),
                before: json!({ "title": was.title }),
                after: Value::Null,
            },
            None,
        )
        .await;
    }
    Ok(Json(view(&state, guild).await?))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PostBody {
    channel_id: String,
}

/// What a refused post says on the page.
fn post_refusal(refusal: PostRefusal) -> ApiError {
    match refusal {
        PostRefusal::NoRoles => ApiError::invalid(vec![FieldError::new("roles", "Add a role to the panel first.")]),
        PostRefusal::MissingEmoji(label) => ApiError::invalid(vec![FieldError::new(
            "roles",
            format!("A reactions panel needs an emoji on every role, and {label} has none."),
        )]),
        PostRefusal::SameEmoji(label) => ApiError::invalid(vec![FieldError::new(
            "roles",
            format!("Two roles share an emoji: {label} needs one of its own."),
        )]),
        PostRefusal::NoAccess => ApiError::blocked(
            "I can't post there: I need View Channel, Send Messages and Embed Links in that channel (and Add Reactions \
             for a reactions panel).",
        ),
        PostRefusal::UnknownChannel => ApiError::invalid(vec![FieldError::new(
            "channel_id",
            "That channel no longer exists, or I can't see it. Refresh the list.",
        )]),
        PostRefusal::Refused => ApiError::blocked("Discord refused the message. Check that the emojis are valid."),
        PostRefusal::Unreachable => ApiError::new(
            StatusCode::BAD_GATEWAY,
            "discord_unreachable",
            "Discord could not be reached. Try again in a moment.",
        ),
    }
}

/// `POST /roles/panels/{id}/post`: the bot posts the panel in a channel, or edits its message there.
pub async fn post_panel(
    State(state): State<WebState>,
    managed: ManagedGuild,
    Path(params): Path<HashMap<String, String>>,
    ApiJson(body): ApiJson<PostBody>,
) -> ApiResult<Json<Value>> {
    let (guild, id) = (managed.id, path_id(&params, "id")? as i64);
    let Some(panel) = state.app.db.panel_by_id(guild, id).await? else {
        return Err(ApiError::not_found());
    };
    let channel = parse_id(&body.channel_id)
        .ok_or_else(|| ApiError::invalid(vec![FieldError::new("channel_id", "That isn't a Discord id.")]))?;
    let info = guild_info::get_with(&state, guild, |info| info.channel(channel).is_some()).await?;
    info.check_post_channel("channel_id", channel)
        .map_err(|error| ApiError::invalid(vec![error]))?;
    // Recheck every publisher, including full managers, against current role hierarchy.
    {
        let live = guild_info::live(&state, guild).await?;
        let grantor = guild_info::grantor(&state, &managed, &live.roles, true).await?;
        let panel_roles = state.app.db.panel_roles(panel.id).await?;
        crate::dashboard_access::check_role_access(
            &state,
            &managed,
            &panel_roles.iter().map(|r| r.role_id).collect::<Vec<_>>(),
        )
        .await?;
        for role in panel_roles {
            live.check_grant("roles", role.role_id as u64, grantor)?;
        }
    }
    let posted = post_role_panel(&state.app, guild, &panel, channel)
        .await
        .map_err(post_refusal)?;
    let target = id.to_string();
    audit::finish(
        &state,
        &managed.current.session,
        Change {
            guild,
            action: "roles.panel.post",
            target: Some(&target),
            before: Value::Null,
            after: json!({ "channel_id": channel.to_string(), "updated": posted.updated }),
        },
        None,
    )
    .await;
    let mut page = view(&state, guild).await?;
    page["posted"] = json!({
        "channel_id": posted.channel.to_string(),
        "message_id": posted.message.to_string(),
        "url": format!("https://discord.com/channels/{guild}/{}/{}", posted.channel, posted.message),
        "updated": posted.updated,
        "moved_from": posted.moved_from.map(|c| c.to_string()),
        "unusable": posted.unusable,
    });
    Ok(Json(page))
}

// /Colour roles

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ColorBody {
    enabled: bool,
    mode: String,
    #[serde(default)]
    allowed_roles: Vec<String>,
    #[serde(default, deserialize_with = "present")]
    anchor_role_id: Option<Option<String>>,
}

/// `PUT /colors`: switches colour roles on, picks the mode, who may pick, and the anchor role.
pub async fn put_colors(
    State(state): State<WebState>,
    managed: ManagedGuild,
    ApiJson(body): ApiJson<ColorBody>,
) -> ApiResult<Json<Value>> {
    let guild = managed.id;
    let mut errors = Vec::new();
    if !matches!(body.mode.as_str(), "palette" | "hex") {
        errors.push(FieldError::new("mode", "The mode is palette or hex."));
    }
    if body.allowed_roles.len() > MAX_COLOR_ALLOWED {
        errors.push(FieldError::new(
            "allowed_roles",
            format!("At most {MAX_COLOR_ALLOWED} roles can be named as the ones who may pick."),
        ));
    }
    let info = guild_info::live(&state, guild).await?;
    let mut allowed: Vec<i64> = Vec::new();
    for raw in &body.allowed_roles {
        match parse_id(raw) {
            Some(id) => match info.check_role("allowed_roles", id) {
                Ok(_) if !allowed.contains(&(id as i64)) => allowed.push(id as i64),
                Ok(_) => {}
                Err(error) => errors.push(error),
            },
            None => errors.push(FieldError::new("allowed_roles", "That isn't a Discord id.")),
        }
    }
    let anchor = match trimmed(body.anchor_role_id) {
        None => None,
        Some(raw) => match parse_id(&raw) {
            Some(id) => {
                let grantor = guild_info::grantor(&state, &managed, &info.roles, true).await?;
                match info.check_grant("anchor_role_id", id, grantor) {
                    Ok(_) => Some(id as i64),
                    Err(error) => {
                        errors.push(error);
                        None
                    }
                }
            }
            None => {
                errors.push(FieldError::new("anchor_role_id", "That isn't a Discord id."));
                None
            }
        },
    };
    if !errors.is_empty() {
        return Err(ApiError::invalid(errors));
    }
    let db = &state.app.db;
    let was = db.color_settings(guild).await?;
    let now = ColorSettings {
        enabled: body.enabled,
        allowed_roles: allowed,
        mode: body.mode,
        anchor_role_id: anchor,
    };
    if was != now {
        db.save_color_settings(guild, &now).await?;
        audit::finish(
            &state,
            &managed.current.session,
            Change {
                guild,
                action: "roles.colors.save",
                target: None,
                before: json!({ "enabled": was.enabled, "mode": was.mode }),
                after: json!({ "enabled": now.enabled, "mode": now.mode }),
            },
            Some(Invalidation::Guild(guild)),
        )
        .await;
    }
    Ok(Json(view(&state, guild).await?))
}

/// `POST /colors/create-missing`: the bot makes the role of every palette colour that has none.
pub async fn create_missing(State(state): State<WebState>, managed: ManagedGuild) -> ApiResult<Json<Value>> {
    let guild = managed.id;
    let report = create_palette_roles(&state.app, guild).await?;
    if !report.created.is_empty() {
        audit::finish(
            &state,
            &managed.current.session,
            Change {
                guild,
                action: "roles.colors.create",
                target: None,
                before: Value::Null,
                after: json!({ "created": report.created }),
            },
            None,
        )
        .await;
    }
    let mut page = view(&state, guild).await?;
    page["report"] = json!({
        "created": report.created,
        "existing": report.existing,
        "stopped": report.stopped.map(|stop| color_stop_text(Lang::En, stop)),
    });
    Ok(Json(page))
}

/// `DELETE /colors/{role}`: the role stops being a colour; `?delete_role=true` deletes it on Discord too.
pub async fn remove_color(
    State(state): State<WebState>,
    managed: ManagedGuild,
    Path(params): Path<HashMap<String, String>>,
    Query(query): Query<HashMap<String, String>>,
) -> ApiResult<Json<Value>> {
    let (guild, role) = (managed.id, path_id(&params, "role")?);
    let delete = query.get("delete_role").is_some_and(|v| v == "true");
    if delete {
        let info = guild_info::live(&state, guild).await?;
        let grantor = guild_info::grantor(&state, &managed, &info.roles, true).await?;
        info.check_grant("role_id", role, grantor)
            .map_err(|error| ApiError::invalid(vec![error]))?;
    }
    if delete_color_role(&state.app, guild, role, delete).await? {
        let target = role.to_string();
        audit::finish(
            &state,
            &managed.current.session,
            Change {
                guild,
                action: "roles.colors.delete",
                target: Some(&target),
                before: json!({ "deleted_on_discord": delete }),
                after: Value::Null,
            },
            None,
        )
        .await;
    }
    Ok(Json(view(&state, guild).await?))
}
