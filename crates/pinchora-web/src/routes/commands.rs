//! Custom commands: what `/tag` runs. The same rules and the same writes as `/customcommand`, plus what
//! only the dashboard has: embeds, actions, role and channel restrictions, a preview, import and export.

use std::collections::HashMap;

use axum::{
    Json,
    extract::{Path, State},
};
use pinchora_cache::Invalidation;
use pinchora_commands::{def_diff, reserved_command_names};
use pinchora_core::{
    settings::FieldError,
    tags::{CommandDef, Export, MAX_COMMANDS, tidy_name},
};
use pinchora_db::{CommandRow, TagWrite};
use serde::Deserialize;
use serde_json::{Value, json};

use super::tag_common::{Refs, Sample, check_refs, command_json, preview_command, preview_json, prune_command, row_id};
use crate::{
    audit::{self, Change},
    error::{ApiError, ApiJson, ApiResult},
    extract::ManagedGuild,
    guild_info,
    state::WebState,
};

/// The most items one import may carry.
const IMPORT_MAX: usize = 200;

fn name_taken(name: &str) -> ApiError {
    ApiError::invalid(vec![FieldError::new(
        "name",
        format!("A command called `{name}` already exists."),
    )])
}

fn full() -> ApiError {
    ApiError::limit(format!(
        "A server has at most {MAX_COMMANDS} custom commands. Remove one first."
    ))
}

async fn load(state: &WebState, guild: u64, id: i64) -> ApiResult<CommandRow> {
    state
        .app
        .db
        .custom_command(guild, id)
        .await?
        .ok_or_else(ApiError::not_found)
}

/// `GET /commands`.
pub async fn list(State(state): State<WebState>, managed: ManagedGuild) -> ApiResult<Json<Value>> {
    let rows = state.app.db.custom_commands(managed.id).await?;
    Ok(Json(json!({
        "items": rows.iter().map(command_json).collect::<Vec<_>>(),
        "max": MAX_COMMANDS,
    })))
}

/// `GET /commands/{id}`.
pub async fn get(
    State(state): State<WebState>,
    managed: ManagedGuild,
    Path(params): Path<HashMap<String, String>>,
) -> ApiResult<Json<Value>> {
    Ok(Json(command_json(&load(&state, managed.id, row_id(&params)?).await?)))
}

/// `POST /commands`: a new command.
pub async fn create(
    State(state): State<WebState>,
    managed: ManagedGuild,
    ApiJson(def): ApiJson<CommandDef>,
) -> ApiResult<Json<Value>> {
    let guild = managed.id;
    let def = def.clean(reserved_command_names())?;
    check_refs(&state, guild, &Refs::of_command(&def), None).await?;
    let author = managed.current.session.user_id;
    match state.app.db.add_custom_command(guild, &def, author).await? {
        TagWrite::Saved(row) => {
            audit::finish(
                &state,
                &managed.current.session,
                Change {
                    guild,
                    action: "command.create",
                    target: Some(&row.def.name),
                    before: Value::Null,
                    after: serde_json::to_value(&row.def).map_err(anyhow::Error::from)?,
                },
                Some(Invalidation::Guild(guild)),
            )
            .await;
            Ok(Json(command_json(&row)))
        }
        TagWrite::NameTaken => Err(name_taken(&def.name)),
        TagWrite::Full => Err(full()),
        TagWrite::Gone => Err(ApiError::not_found()),
    }
}

/// `PUT /commands/{id}`: the whole command again.
pub async fn replace(
    State(state): State<WebState>,
    managed: ManagedGuild,
    Path(params): Path<HashMap<String, String>>,
    ApiJson(def): ApiJson<CommandDef>,
) -> ApiResult<Json<Value>> {
    let guild = managed.id;
    let old = load(&state, guild, row_id(&params)?).await?;
    // A name that stays is not held against the bot's commands again.
    let reserved = if tidy_name(&def.name) == old.def.name {
        &[]
    } else {
        reserved_command_names()
    };
    let def = def.clean(reserved)?;
    if def == old.def {
        return Ok(Json(command_json(&old)));
    }
    check_refs(
        &state,
        guild,
        &Refs::of_command(&def),
        Some(&Refs::of_command(&old.def)),
    )
    .await?;
    match state.app.db.replace_custom_command(guild, old.id, &def).await? {
        TagWrite::Saved(row) => {
            let (before, after) = def_diff(
                &serde_json::to_value(&old.def).map_err(anyhow::Error::from)?,
                &serde_json::to_value(&row.def).map_err(anyhow::Error::from)?,
            );
            audit::finish(
                &state,
                &managed.current.session,
                Change {
                    guild,
                    action: "command.update",
                    target: Some(&row.def.name),
                    before,
                    after,
                },
                Some(Invalidation::Guild(guild)),
            )
            .await;
            Ok(Json(command_json(&row)))
        }
        TagWrite::NameTaken => Err(name_taken(&def.name)),
        TagWrite::Gone | TagWrite::Full => Err(ApiError::not_found()),
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EnabledBody {
    pub enabled: bool,
}

/// `PATCH /commands/{id}`: switch it on or off.
pub async fn set_enabled(
    State(state): State<WebState>,
    managed: ManagedGuild,
    Path(params): Path<HashMap<String, String>>,
    ApiJson(body): ApiJson<EnabledBody>,
) -> ApiResult<Json<Value>> {
    let guild = managed.id;
    let old = load(&state, guild, row_id(&params)?).await?;
    if old.def.enabled == body.enabled {
        return Ok(Json(command_json(&old)));
    }
    let row = state
        .app
        .db
        .set_custom_command_enabled(guild, old.id, body.enabled)
        .await?
        .ok_or_else(ApiError::not_found)?;
    audit::finish(
        &state,
        &managed.current.session,
        Change {
            guild,
            action: "command.update",
            target: Some(&row.def.name),
            before: json!({ "enabled": old.def.enabled }),
            after: json!({ "enabled": row.def.enabled }),
        },
        Some(Invalidation::Guild(guild)),
    )
    .await;
    Ok(Json(command_json(&row)))
}

/// `DELETE /commands/{id}`.
pub async fn delete(
    State(state): State<WebState>,
    managed: ManagedGuild,
    Path(params): Path<HashMap<String, String>>,
) -> ApiResult<Json<Value>> {
    let guild = managed.id;
    let removed = state
        .app
        .db
        .delete_custom_command(guild, row_id(&params)?)
        .await?
        .ok_or_else(ApiError::not_found)?;
    audit::finish(
        &state,
        &managed.current.session,
        Change {
            guild,
            action: "command.delete",
            target: Some(&removed.def.name),
            before: serde_json::to_value(&removed.def).map_err(anyhow::Error::from)?,
            after: Value::Null,
        },
        Some(Invalidation::Guild(guild)),
    )
    .await;
    Ok(Json(json!({ "deleted": removed.id.to_string() })))
}

// /Preview

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PreviewBody {
    command: CommandDef,
    #[serde(default)]
    args: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TestBody {
    #[serde(default)]
    args: String,
}

async fn preview_of(state: &WebState, managed: &ManagedGuild, def: &CommandDef, args: &str) -> ApiResult<Json<Value>> {
    let session = &managed.current.session;
    let name = session.display_name();
    let settings = state.app.db.guild_settings(managed.id).await?;
    // The pickers' copy is enough for names; a Discord outage only turns mentions into plain words.
    let info = guild_info::get(state, managed.id, false).await.ok();
    let sample = Sample {
        user: session.user_id,
        name,
        avatar: format!(
            "https://cdn.discordapp.com/embed/avatars/{}.png",
            (session.user_id >> 22) % 6
        ),
        guild_name: &managed.guild.name,
        points_name: &settings.points_name,
        info: info.as_ref(),
    };
    let preview = preview_command(def, args, &sample, settings.points_enabled);
    Ok(Json(preview_json(&preview, (session.user_id, name), info.as_ref())))
}

/// `POST /commands/preview`: what a command being written would answer, nothing run, nothing saved.
pub async fn preview(
    State(state): State<WebState>,
    managed: ManagedGuild,
    ApiJson(body): ApiJson<PreviewBody>,
) -> ApiResult<Json<Value>> {
    let def = body.command.clean(&[])?;
    preview_of(&state, &managed, &def, &body.args).await
}

/// `POST /commands/{id}/test`: what a saved command answers the requester. No side effects: no actions,
/// no cooldown, no use counted.
pub async fn test(
    State(state): State<WebState>,
    managed: ManagedGuild,
    Path(params): Path<HashMap<String, String>>,
    ApiJson(body): ApiJson<TestBody>,
) -> ApiResult<Json<Value>> {
    let row = load(&state, managed.id, row_id(&params)?).await?;
    preview_of(&state, &managed, &row.def, &body.args).await
}

// /Export and import

/// `GET /commands/export`: every command, as a file another server can import.
pub async fn export(State(state): State<WebState>, managed: ManagedGuild) -> ApiResult<Json<Value>> {
    let rows = state.app.db.custom_commands(managed.id).await?;
    let export = Export::new(rows.into_iter().map(|r| r.def).collect());
    Ok(Json(serde_json::to_value(export).map_err(anyhow::Error::from)?))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ImportBody {
    #[serde(default)]
    version: Option<u32>,
    items: Vec<Value>,
    /// `skip` (the default) keeps a command that already has the name; `replace` overwrites it.
    #[serde(default)]
    on_conflict: Option<String>,
}

/// `POST /commands/import`: adds the commands of an export. Each is checked like a new command; one that
/// is refused is reported and the rest still go in. Roles and channels this server lacks are left out.
pub async fn import(
    State(state): State<WebState>,
    managed: ManagedGuild,
    ApiJson(body): ApiJson<ImportBody>,
) -> ApiResult<Json<Value>> {
    let guild = managed.id;
    if body.version.is_some_and(|v| v != Export::<CommandDef>::VERSION) {
        return Err(ApiError::malformed(
            "This file comes from a version of the export that I can't read.",
        ));
    }
    if body.items.len() > IMPORT_MAX {
        return Err(ApiError::limit(format!(
            "An import carries at most {IMPORT_MAX} commands."
        )));
    }
    let replace = match body.on_conflict.as_deref() {
        None | Some("skip") => false,
        Some("replace") => true,
        Some(_) => return Err(ApiError::malformed("on_conflict is `skip` or `replace`.")),
    };
    let db = &state.app.db;
    let author = managed.current.session.user_id;
    let known = merged_info(&state, guild).await?;
    let mut ids: HashMap<String, i64> = db
        .custom_commands(guild)
        .await?
        .into_iter()
        .map(|c| (c.def.name, c.id))
        .collect();
    let (mut created, mut replaced, mut skipped, mut failed): (Vec<String>, Vec<String>, Vec<String>, Vec<Value>) =
        Default::default();
    for (index, item) in body.items.into_iter().enumerate() {
        let fail = |name: &str, message: String| json!({ "index": index, "name": name, "message": message });
        let mut def: CommandDef = match serde_json::from_value(item) {
            Ok(def) => def,
            Err(error) => {
                failed.push(fail("", format!("That entry isn't a command: {error}.")));
                continue;
            }
        };
        let label = def.name.clone();
        let dropped = prune_command(&mut def, &known);
        let def = match def.clean(reserved_command_names()) {
            Ok(def) => def,
            Err(errors) => {
                failed.push(fail(&label, errors[0].message.clone()));
                continue;
            }
        };
        let (written, replacing) = match ids.get(&def.name) {
            Some(_) if !replace => {
                skipped.push(def.name.clone());
                continue;
            }
            Some(id) => (db.replace_custom_command(guild, *id, &def).await?, true),
            None => (db.add_custom_command(guild, &def, author).await?, false),
        };
        match written {
            TagWrite::Saved(row) => {
                ids.insert(row.def.name.clone(), row.id);
                let mut name = row.def.name.clone();
                if !dropped.is_empty() {
                    name.push_str(&format!(" ({})", dropped.join("; ")));
                }
                if replacing {
                    replaced.push(name)
                } else {
                    created.push(name)
                }
            }
            TagWrite::NameTaken => skipped.push(def.name.clone()),
            TagWrite::Full => failed.push(fail(&def.name, full().message)),
            TagWrite::Gone => failed.push(fail(&def.name, "It was removed while importing.".to_owned())),
        }
    }
    if !created.is_empty() || !replaced.is_empty() {
        audit::finish(
            &state,
            &managed.current.session,
            Change {
                guild,
                action: "commands.import",
                target: None,
                before: Value::Null,
                after: json!({ "created": created, "replaced": replaced }),
            },
            Some(Invalidation::Guild(guild)),
        )
        .await;
    }
    Ok(Json(json!({
        "created": created,
        "replaced": replaced,
        "skipped": skipped,
        "failed": failed,
    })))
}

/// The lists import checks against: roles read live (a role action needs the bot's position now), channels
/// from the pickers' copy.
pub async fn merged_info(state: &WebState, guild: u64) -> ApiResult<guild_info::GuildInfo> {
    let live = guild_info::live(state, guild).await?;
    let cached = guild_info::get(state, guild, false).await.ok();
    Ok(guild_info::GuildInfo {
        channels: cached.map_or_else(|| live.channels.clone(), |c| c.channels),
        roles: live.roles,
        bot: live.bot,
    })
}
