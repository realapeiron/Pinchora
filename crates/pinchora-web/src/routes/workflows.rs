use super::tag_common::{Refs, check_refs, row_id};
use crate::{
    audit::{self, Change},
    error::{ApiError, ApiJson, ApiResult},
    extract::ManagedGuild,
    state::WebState,
};
use axum::{
    Json, Router,
    extract::{Path, Query, State},
    routing::{get, post, put},
};
use pinchora_core::{
    settings::FieldError,
    t,
    workflows::{self, Definition, Event, Scope, StepAction, Trigger},
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::HashMap;
pub fn router() -> Router<WebState> {
    Router::new()
        .route("/", get(list).post(create))
        .route("/{id}", put(update).delete(remove))
        .route("/test", post(test))
        .route("/{id}/runs", get(runs))
        .route("/{id}/post", post(post_button))
        .route("/variables", get(variables).put(variable))
}
async fn list(State(state): State<WebState>, managed: ManagedGuild) -> ApiResult<Json<Value>> {
    let rows = state.app.db.workflows(managed.id).await?;
    Ok(Json(
        json!({"items":rows.iter().map(|r|r.json()).collect::<Vec<_>>(),"triggers":pinchora_commands::workflows::trigger_catalog(&state.app,managed.id).await,"message_content":state.settings.message_content,"max":workflows::MAX_WORKFLOWS}),
    ))
}
fn refs(actions: &[StepAction]) -> Vec<Refs> {
    let mut refs = Vec::new();
    for action in actions {
        match action {
            StepAction::Tag { action } => {
                if let Some(role) = action.role() {
                    refs.push(Refs {
                        action_roles: vec![role],
                        ..Default::default()
                    });
                }
            }
            StepAction::SendMessage { channel_id, .. } | StepAction::CreateThread { channel_id, .. } => {
                refs.push(Refs {
                    post_channel: channel_id.parse().ok(),
                    ..Default::default()
                });
            }
            StepAction::RandomBranch { branches } => {
                for branch in branches {
                    refs.extend(self::refs(branch));
                }
            }
            _ => {}
        }
    }
    refs
}
async fn validate(state: &WebState, managed: &ManagedGuild, def: &Definition) -> ApiResult<()> {
    let lang = state.app.guild_lang(managed.id).await;
    if let Err(field) = def.validate() {
        return Err(ApiError::invalid(vec![FieldError::new(
            field,
            t!(lang, "workflows.invalid"),
        )]));
    }
    if def.enabled && matches!(def.trigger, Trigger::MessageKeyword { .. }) && !state.settings.message_content {
        return Err(ApiError::invalid(vec![FieldError::new(
            "trigger",
            t!(lang, "workflows.content"),
        )]));
    }
    if !pinchora_commands::workflows::available(&state.app, managed.id, &def.trigger).await {
        return Err(ApiError::invalid(vec![FieldError::new(
            "trigger",
            t!(lang, "workflows.unavailable"),
        )]));
    }
    for refs in refs(&def.actions) {
        check_refs(state, managed, &refs, None).await?;
    }
    Ok(())
}
async fn save(state: &WebState, managed: &ManagedGuild, id: Option<i64>, def: Definition) -> ApiResult<Json<Value>> {
    validate(state, managed, &def).await?;
    let lang = state.app.guild_lang(managed.id).await;
    let before = if let Some(id) = id {
        state
            .app
            .db
            .workflow(managed.id, id)
            .await?
            .ok_or_else(ApiError::not_found)?
            .json()
    } else {
        Value::Null
    };
    let row = state
        .app
        .db
        .save_workflow(managed.id, id, &def, managed.current.session.user_id)
        .await
        .map_err(|_| ApiError::invalid(vec![FieldError::new("name", t!(lang, "workflows.save_failed"))]))?;
    let after = row.json();
    audit::finish(
        state,
        &managed.current.session,
        Change {
            guild: managed.id,
            action: "workflow.save",
            target: Some(&def.name),
            before,
            after: after.clone(),
        },
        None,
    )
    .await;
    Ok(Json(after))
}
async fn create(
    State(state): State<WebState>,
    managed: ManagedGuild,
    ApiJson(def): ApiJson<Definition>,
) -> ApiResult<Json<Value>> {
    save(&state, &managed, None, def).await
}
async fn update(
    State(state): State<WebState>,
    managed: ManagedGuild,
    Path(params): Path<HashMap<String, String>>,
    ApiJson(def): ApiJson<Definition>,
) -> ApiResult<Json<Value>> {
    save(&state, &managed, Some(row_id(&params)?), def).await
}
async fn remove(
    State(state): State<WebState>,
    managed: ManagedGuild,
    Path(params): Path<HashMap<String, String>>,
) -> ApiResult<Json<Value>> {
    let id = row_id(&params)?;
    let old = state
        .app
        .db
        .workflow(managed.id, id)
        .await?
        .ok_or_else(ApiError::not_found)?;
    state.app.db.delete_workflow(managed.id, id).await?;
    audit::finish(
        &state,
        &managed.current.session,
        Change {
            guild: managed.id,
            action: "workflow.delete",
            target: Some(&old.name),
            before: old.json(),
            after: Value::Null,
        },
        None,
    )
    .await;
    Ok(Json(json!({"ok":true})))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Test {
    definition: Definition,
    event: Event,
}
async fn test(
    State(state): State<WebState>,
    managed: ManagedGuild,
    ApiJson(body): ApiJson<Test>,
) -> ApiResult<Json<Value>> {
    // No Discord requests, writes, cooldowns, point claims or history inserts.
    let lang = state.app.guild_lang(managed.id).await;
    body.definition
        .validate()
        .map_err(|field| ApiError::invalid(vec![FieldError::new(field, t!(lang, "workflows.invalid"))]))?;
    Ok(Json(
        json!({"dry_run":true,"playback":pinchora_commands::workflows::engine::dry_run(&body.definition,&body.event,state.app.now())}),
    ))
}
async fn runs(
    State(state): State<WebState>,
    managed: ManagedGuild,
    Path(params): Path<HashMap<String, String>>,
) -> ApiResult<Json<Value>> {
    let id = row_id(&params)?;
    state
        .app
        .db
        .workflow(managed.id, id)
        .await?
        .ok_or_else(ApiError::not_found)?;
    let runs = state.app.db.workflow_runs(managed.id, id).await?;
    Ok(Json(
        json!({"items":runs.into_iter().map(|r|json!({"id":r.id.to_string(),"user_id":r.user_id.to_string(),"started_at":r.started_at,"outcome":r.outcome,"steps":r.steps})).collect::<Vec<_>>()}),
    ))
}
async fn variables(
    State(state): State<WebState>,
    managed: ManagedGuild,
    Query(params): Query<HashMap<String, String>>,
) -> ApiResult<Json<Value>> {
    let lang = state.app.guild_lang(managed.id).await;
    let user = match params.get("subject").filter(|s| !s.is_empty()) {
        Some(s) if workflows::id(s) => s.parse::<u64>().map_err(anyhow::Error::from)?,
        Some(_) => {
            return Err(ApiError::invalid(vec![FieldError::new(
                "subject",
                t!(lang, "workflows.invalid"),
            )]));
        }
        None => managed.current.session.user_id,
    };
    Ok(Json(
        json!({"items":state.app.db.workflow_vars(managed.id,user).await?}),
    ))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Variable {
    scope: Scope,
    key: String,
    value: String,
    subject_id: Option<String>,
}
async fn variable(
    State(state): State<WebState>,
    managed: ManagedGuild,
    ApiJson(body): ApiJson<Variable>,
) -> ApiResult<Json<Value>> {
    let lang = state.app.guild_lang(managed.id).await;
    let user = match body.subject_id.as_deref() {
        Some(s) if workflows::id(s) => s.parse::<u64>().map_err(anyhow::Error::from)?,
        Some(_) => {
            return Err(ApiError::invalid(vec![FieldError::new(
                "subject",
                t!(lang, "workflows.invalid"),
            )]));
        }
        None => managed.current.session.user_id,
    };
    let result = state
        .app
        .db
        .set_workflow_var(managed.id, body.scope, user, &body.key, &body.value, None)
        .await
        .map_err(|_| ApiError::invalid(vec![FieldError::new("value", t!(lang, "workflows.invalid"))]))?;
    audit::finish(
        &state,
        &managed.current.session,
        Change {
            guild: managed.id,
            action: "workflow.variable",
            target: Some(&body.key),
            before: Value::Null,
            after: json!({"scope":body.scope,"key":body.key}),
        },
        None,
    )
    .await;
    Ok(Json(json!({"value":result})))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PostButton {
    channel_id: String,
    text: String,
    label: String,
}
async fn post_button(
    State(state): State<WebState>,
    managed: ManagedGuild,
    Path(params): Path<HashMap<String, String>>,
    ApiJson(body): ApiJson<PostButton>,
) -> ApiResult<Json<Value>> {
    use twilight_model::{
        channel::message::{
            AllowedMentions, Component,
            component::{ActionRow, Button, ButtonStyle},
        },
        id::Id,
    };
    let id = row_id(&params)?;
    let row = state
        .app
        .db
        .workflow(managed.id, id)
        .await?
        .ok_or_else(ApiError::not_found)?;
    let lang = state.app.guild_lang(managed.id).await;
    if row.definition()?.trigger != (Trigger::Button {})
        || !workflows::id(&body.channel_id)
        || body.text.trim().is_empty()
        || body.text.chars().count() > 2000
        || body.label.trim().is_empty()
        || body.label.chars().count() > 80
    {
        return Err(ApiError::invalid(vec![FieldError::new(
            "button",
            t!(lang, "workflows.invalid"),
        )]));
    }
    let channel = body.channel_id.parse::<u64>().map_err(anyhow::Error::from)?;
    check_refs(
        &state,
        &managed,
        &Refs {
            post_channel: Some(channel),
            ..Default::default()
        },
        None,
    )
    .await?;
    let components = vec![Component::ActionRow(ActionRow {
        id: None,
        components: vec![Component::Button(Button {
            custom_id: Some(format!("wf:b:{id}:0")),
            disabled: false,
            emoji: None,
            id: None,
            label: Some(body.label),
            sku_id: None,
            style: ButtonStyle::Primary,
            url: None,
        })],
    })];
    let message = state
        .app
        .http
        .create_message(Id::new(channel))
        .content(&body.text)
        .components(&components)
        .allowed_mentions(Some(&AllowedMentions::default()))
        .await
        .map_err(anyhow::Error::from)?
        .model()
        .await
        .map_err(anyhow::Error::from)?;
    audit::finish(
        &state,
        &managed.current.session,
        Change {
            guild: managed.id,
            action: "workflow.post",
            target: Some(&row.name),
            before: Value::Null,
            after: json!({"channel_id":channel.to_string(),"message_id":message.id.to_string()}),
        },
        None,
    )
    .await;
    Ok(Json(json!({"message_id":message.id.to_string()})))
}
