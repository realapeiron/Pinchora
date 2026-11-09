//! Auto-responders: messages the bot sends by itself when a member joins, leaves, levels up, gets or loses
//! a role, or says a keyword. Keywords need the Message Content intent; without it they can be written but
//! not switched on.

use std::collections::HashMap;

use axum::{
    Json,
    extract::{Path, State},
};
use pinchora_cache::Invalidation;
use pinchora_commands::def_diff;
use pinchora_core::community::tags::Destination;
use pinchora_core::community::tags::Export;
use pinchora_core::community::tags::MAX_RESPONDERS;
use pinchora_core::community::tags::ResponderDef;
use pinchora_db::{ResponderRow, TagWrite};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::audit;
use crate::audit::Change;
use crate::error::ApiError;
use crate::error::ApiJson;
use crate::error::ApiResult;
use crate::extract::ManagedGuild;
use crate::guild_info;
use crate::routes::admin::commands::EnabledBody;
use crate::routes::admin::commands::merged_info;
use crate::routes::admin::tag_common::Refs;
use crate::routes::admin::tag_common::Sample;
use crate::routes::admin::tag_common::check_refs;
use crate::routes::admin::tag_common::preview_json;
use crate::routes::admin::tag_common::preview_responder;
use crate::routes::admin::tag_common::prune_responder;
use crate::routes::admin::tag_common::responder_json;
use crate::routes::admin::tag_common::row_id;
use crate::state::WebState;

/// The most items one import may carry.
const IMPORT_MAX: usize = 100;

const LOCKED: &str = "Keyword triggers need the Message Content intent, which this bot runs without. Ask whoever runs the bot to turn it on (PINCHORA_GATEWAY__MESSAGE_CONTENT), or save this one switched off.";

fn full() -> ApiError {
    ApiError::limit(format!(
        "A server has at most {MAX_RESPONDERS} auto-responders. Remove one first."
    ))
}

/// A keyword responder cannot be switched on while the bot cannot read chat.
fn check_listening(state: &WebState, def: &ResponderDef) -> ApiResult<()> {
    if def.trigger_kind.needs_message_content() && def.enabled && !state.settings.message_content {
        return Err(ApiError::blocked(LOCKED));
    }
    Ok(())
}

async fn load(state: &WebState, guild: u64, id: i64) -> ApiResult<ResponderRow> {
    state
        .app
        .db
        .custom_responder(guild, id)
        .await?
        .ok_or_else(ApiError::not_found)
}

/// `GET /responders`.
pub async fn list(State(state): State<WebState>, managed: ManagedGuild) -> ApiResult<Json<Value>> {
    let rows = state.app.db.custom_responders(managed.id).await?;
    Ok(Json(json!({
        "items": rows.iter().map(responder_json).collect::<Vec<_>>(),
        "max": MAX_RESPONDERS,
        "message_content": state.settings.message_content,
    })))
}

/// `GET /responders/{id}`.
pub async fn get(
    State(state): State<WebState>,
    managed: ManagedGuild,
    Path(params): Path<HashMap<String, String>>,
) -> ApiResult<Json<Value>> {
    Ok(Json(responder_json(&load(&state, managed.id, row_id(&params)?).await?)))
}

/// `POST /responders`.
pub async fn create(
    State(state): State<WebState>,
    managed: ManagedGuild,
    ApiJson(def): ApiJson<ResponderDef>,
) -> ApiResult<Json<Value>> {
    let guild = managed.id;
    let def = def.clean()?;
    check_listening(&state, &def)?;
    check_refs(&state, &managed, &Refs::of_responder(&def), None).await?;
    let author = managed.current.session.user_id;
    match state.app.db.add_custom_responder(guild, &def, author).await? {
        TagWrite::Saved(row) => {
            audit::finish(
                &state,
                &managed.current.session,
                Change {
                    guild,
                    action: "responder.create",
                    target: Some(&row.def.name),
                    before: Value::Null,
                    after: serde_json::to_value(&row.def).map_err(anyhow::Error::from)?,
                },
                Some(Invalidation::Guild(guild)),
            )
            .await;
            Ok(Json(responder_json(&row)))
        }
        TagWrite::Full => Err(full()),
        TagWrite::NameTaken | TagWrite::Gone => Err(ApiError::not_found()),
    }
}

/// `PUT /responders/{id}`.
pub async fn replace(
    State(state): State<WebState>,
    managed: ManagedGuild,
    Path(params): Path<HashMap<String, String>>,
    ApiJson(def): ApiJson<ResponderDef>,
) -> ApiResult<Json<Value>> {
    let guild = managed.id;
    let old = load(&state, guild, row_id(&params)?).await?;
    let def = def.clean()?;
    if def == old.def {
        return Ok(Json(responder_json(&old)));
    }
    // A keyword responder that already was on may be edited while the intent is off (it just stays quiet);
    // switching one on is what is refused.
    if !(old.def.enabled && old.def.trigger_kind == def.trigger_kind) {
        check_listening(&state, &def)?;
    }
    check_refs(
        &state,
        &managed,
        &Refs::of_responder(&def),
        Some(&Refs::of_responder(&old.def)),
    )
    .await?;
    match state.app.db.replace_custom_responder(guild, old.id, &def).await? {
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
                    action: "responder.update",
                    target: Some(&row.def.name),
                    before,
                    after,
                },
                Some(Invalidation::Guild(guild)),
            )
            .await;
            Ok(Json(responder_json(&row)))
        }
        _ => Err(ApiError::not_found()),
    }
}

/// `PATCH /responders/{id}`: switch it on or off.
pub async fn set_enabled(
    State(state): State<WebState>,
    managed: ManagedGuild,
    Path(params): Path<HashMap<String, String>>,
    ApiJson(body): ApiJson<EnabledBody>,
) -> ApiResult<Json<Value>> {
    let guild = managed.id;
    let old = load(&state, guild, row_id(&params)?).await?;
    let wanted = body.enabled;
    if old.def.enabled == wanted {
        return Ok(Json(responder_json(&old)));
    }
    if wanted {
        check_listening(
            &state,
            &ResponderDef {
                enabled: true,
                ..old.def.clone()
            },
        )?;
        let refs = Refs::of_responder(&old.def);
        check_refs(&state, &managed, &refs, Some(&refs)).await?;
    }
    let row = state
        .app
        .db
        .set_custom_responder_enabled(guild, old.id, wanted)
        .await?
        .ok_or_else(ApiError::not_found)?;
    audit::finish(
        &state,
        &managed.current.session,
        Change {
            guild,
            action: "responder.update",
            target: Some(&row.def.name),
            before: json!({ "enabled": old.def.enabled }),
            after: json!({ "enabled": row.def.enabled }),
        },
        Some(Invalidation::Guild(guild)),
    )
    .await;
    Ok(Json(responder_json(&row)))
}

/// `DELETE /responders/{id}`.
pub async fn delete(
    State(state): State<WebState>,
    managed: ManagedGuild,
    Path(params): Path<HashMap<String, String>>,
) -> ApiResult<Json<Value>> {
    let guild = managed.id;
    let removed = state
        .app
        .db
        .delete_custom_responder(guild, row_id(&params)?)
        .await?
        .ok_or_else(ApiError::not_found)?;
    audit::finish(
        &state,
        &managed.current.session,
        Change {
            guild,
            action: "responder.delete",
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
    responder: ResponderDef,
}

async fn preview_of(state: &WebState, managed: &ManagedGuild, def: &ResponderDef) -> ApiResult<Json<Value>> {
    let session = &managed.current.session;
    let name = session.display_name();
    let settings = state.app.db.guild_settings(managed.id).await?;
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
    let mut preview = preview_responder(def, &sample, settings.points_enabled);
    if def.trigger_kind.needs_message_content() && !state.settings.message_content {
        preview.notes.insert(
            0,
            "Keyword triggers are locked: the bot can't read chat without the Message Content intent.".to_owned(),
        );
    }
    if def.destination == Destination::Dm {
        preview
            .notes
            .push("It arrives as a direct message to the member.".to_owned());
    }
    Ok(Json(preview_json(&preview, (session.user_id, name), info.as_ref())))
}

/// `POST /responders/preview`: what a responder being written would say, nothing sent, nothing saved.
pub async fn preview(
    State(state): State<WebState>,
    managed: ManagedGuild,
    ApiJson(body): ApiJson<PreviewBody>,
) -> ApiResult<Json<Value>> {
    let def = body.responder.clean()?;
    preview_of(&state, &managed, &def).await
}

/// `POST /responders/{id}/test`: what a saved responder would say, nothing sent.
pub async fn test(
    State(state): State<WebState>,
    managed: ManagedGuild,
    Path(params): Path<HashMap<String, String>>,
) -> ApiResult<Json<Value>> {
    let row = load(&state, managed.id, row_id(&params)?).await?;
    preview_of(&state, &managed, &row.def).await
}

// /Export and import

/// `GET /responders/export`.
pub async fn export(State(state): State<WebState>, managed: ManagedGuild) -> ApiResult<Json<Value>> {
    let rows = state.app.db.custom_responders(managed.id).await?;
    let export = Export::new(rows.into_iter().map(|r| r.def).collect());
    Ok(Json(serde_json::to_value(export).map_err(anyhow::Error::from)?))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ImportBody {
    #[serde(default)]
    version: Option<u32>,
    items: Vec<Value>,
}

/// `POST /responders/import`: adds the responders of an export. Responders can share names, so every one is
/// added (up to the limit). Channels this server lacks fail that responder; keyword ones come in switched
/// off while the bot cannot read chat.
pub async fn import(
    State(state): State<WebState>,
    managed: ManagedGuild,
    ApiJson(body): ApiJson<ImportBody>,
) -> ApiResult<Json<Value>> {
    let guild = managed.id;
    if body.version.is_some_and(|v| v != Export::<ResponderDef>::VERSION) {
        return Err(ApiError::malformed(
            "This file comes from a version of the export that I can't read.",
        ));
    }
    if body.items.len() > IMPORT_MAX {
        return Err(ApiError::limit(format!(
            "An import carries at most {IMPORT_MAX} responders."
        )));
    }
    let db = &state.app.db;
    let author = managed.current.session.user_id;
    let known = merged_info(&state, guild).await?;
    let grantor = guild_info::grantor(&state, &managed, &known.roles, true).await?;
    let (mut created, mut failed): (Vec<String>, Vec<Value>) = Default::default();
    for (index, item) in body.items.into_iter().enumerate() {
        let fail = |name: &str, message: String| json!({ "index": index, "name": name, "message": message });
        let mut def: ResponderDef = match serde_json::from_value(item) {
            Ok(def) => def,
            Err(error) => {
                failed.push(fail("", format!("That entry isn't an auto-responder: {error}.")));
                continue;
            }
        };
        let label = def.name.clone();
        let mut dropped = prune_responder(&mut def, &known, grantor);
        if def.trigger_kind.needs_message_content() && def.enabled && !state.settings.message_content {
            def.enabled = false;
            dropped.push("keyword triggers are locked, so it came in switched off".to_owned());
        }
        let def = match def.clean() {
            Ok(def) => def,
            Err(errors) => {
                failed.push(fail(&label, errors[0].message.clone()));
                continue;
            }
        };
        // What it cannot work without must exist here: the channel it posts in, the role it watches.
        let refs = Refs::of_responder(&def);
        let missing_channel = refs
            .post_channel
            .is_some_and(|id| known.check_post_channel("channel_id", id).is_err());
        let missing_role = refs.watched_role.is_some_and(|id| known.role(id).is_none());
        if missing_channel || missing_role {
            let what = if missing_channel {
                "the channel it posts in"
            } else {
                "the role it watches"
            };
            failed.push(fail(&label, format!("This server doesn't have {what}.")));
            continue;
        }
        match db.add_custom_responder(guild, &def, author).await? {
            TagWrite::Saved(row) => {
                let mut name = row.def.name.clone();
                if !dropped.is_empty() {
                    name.push_str(&format!(" ({})", dropped.join("; ")));
                }
                created.push(name);
            }
            TagWrite::Full => failed.push(fail(&label, full().message)),
            TagWrite::NameTaken | TagWrite::Gone => failed.push(fail(&label, "It could not be saved.".to_owned())),
        }
    }
    if !created.is_empty() {
        audit::finish(
            &state,
            &managed.current.session,
            Change {
                guild,
                action: "responders.import",
                target: None,
                before: Value::Null,
                after: json!({ "created": created }),
            },
            Some(Invalidation::Guild(guild)),
        )
        .await;
    }
    Ok(Json(
        json!({ "created": created, "replaced": [], "skipped": [], "failed": failed }),
    ))
}

#[cfg(test)]
mod tests {
    use crate::routes::admin::responders::*;

    #[test]
    fn the_locked_sentence_names_the_setting() {
        assert!(LOCKED.contains("PINCHORA_GATEWAY__MESSAGE_CONTENT"));
    }
}
