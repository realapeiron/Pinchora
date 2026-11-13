//! Message kits: one validated payload for preview, save, send, and edit.

use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, Path, State},
    routing::{get, post, put},
};
use pinchora_core::admin::card_design::CardKind;
use pinchora_core::admin::message_studio;
use pinchora_core::admin::message_studio::KitComponent;
use pinchora_core::admin::message_studio::MessageKit;
use pinchora_core::admin::settings::FieldError;
use pinchora_core::t;
use serde::Deserialize;
use serde_json::{Value, json};
use twilight_model::{
    channel::message::{
        AllowedMentions, Component, MentionType,
        component::{ActionRow, Button, ButtonStyle, SelectMenu, SelectMenuOption, SelectMenuType},
    },
    id::Id,
};

use crate::audit;
use crate::audit::Change;
use crate::error::ApiError;
use crate::error::ApiJson;
use crate::error::ApiResult;
use crate::extract::ManagedGuild;
use crate::guild_info;
use crate::state::WebState;

pub fn router() -> Router<WebState> {
    Router::new()
        .route("/", get(list))
        .route("/events", get(events))
        .route("/events/{kind}", put(bind_event))
        .route("/preview", post(preview))
        .route("/validate", post(validate_import))
        .route("/send", post(send))
        .route("/{key}", put(save))
        .route("/{key}/history", get(history))
        .route("/{key}/rollback/{version}", post(rollback))
        .layer(DefaultBodyLimit::max(message_studio::KIT_BYTES_MAX + 1024))
}

async fn validate_import(_managed: ManagedGuild, ApiJson(value): ApiJson<Value>) -> ApiResult<Json<MessageKit>> {
    Ok(Json(checked(value)?))
}

async fn lock_guild(tx: &mut sqlx::Transaction<'_, sqlx::Postgres>, guild: u64) -> ApiResult<()> {
    // Also serialize first inserts, quota checks, and event binding against kit edits.
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
        .bind(format!("message-studio:{guild}"))
        .execute(&mut **tx)
        .await?;
    Ok(())
}

async fn audit_in(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    managed: &ManagedGuild,
    action: &str,
    target: &str,
    before: Value,
    after: Value,
) -> ApiResult<()> {
    sqlx::query(
        "INSERT INTO dashboard_audit (guild_id,actor_id,actor_name,source,action,target,before,after)
        VALUES ($1,$2,$3,'web',$4,$5,$6,$7)",
    )
    .bind(managed.id as i64)
    .bind(managed.current.session.user_id as i64)
    .bind(managed.current.session.display_name())
    .bind(action)
    .bind(target)
    .bind(before)
    .bind(after)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn events(State(state): State<WebState>, managed: ManagedGuild) -> ApiResult<Json<Value>> {
    let rows: Vec<(String, String)> = sqlx::query_as("SELECT kind,kit_key FROM message_event_kits WHERE guild_id=$1")
        .bind(managed.id as i64)
        .fetch_all(state.app.db.pool())
        .await?;
    Ok(Json(json!(
        rows.into_iter().collect::<std::collections::HashMap<_, _>>()
    )))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BindBody {
    key: Option<String>,
}

async fn bind_event(
    State(state): State<WebState>,
    managed: ManagedGuild,
    Path((_, raw)): Path<(String, String)>,
    ApiJson(body): ApiJson<BindBody>,
) -> ApiResult<Json<Value>> {
    let kind =
        CardKind::parse(&raw).ok_or_else(|| FieldError::new("kind", "Choose welcome, goodbye, boost or level up."))?;
    let mut tx = state.app.db.pool().begin().await?;
    lock_guild(&mut tx, managed.id).await?;
    let before: Option<(String,)> =
        sqlx::query_as("SELECT kit_key FROM message_event_kits WHERE guild_id=$1 AND kind=$2")
            .bind(managed.id as i64)
            .bind(kind.key())
            .fetch_optional(&mut *tx)
            .await?;
    if let Some(ref selected) = body.key {
        let selected = key(selected)?;
        let payload: (Value,) = sqlx::query_as("SELECT payload FROM message_kits WHERE guild_id=$1 AND key=$2")
            .bind(managed.id as i64)
            .bind(selected)
            .fetch_optional(&mut *tx)
            .await?
            .ok_or_else(|| FieldError::new("key", "Save the kit before assigning it."))?;
        let kit = checked(payload.0)?;
        if !kit.embeds.is_empty() || !kit.components.is_empty() {
            return Err(FieldError::new("key", "Event text kits cannot have embeds or components.").into());
        }
        sqlx::query(
            "INSERT INTO message_event_kits (guild_id,kind,kit_key) VALUES ($1,$2,$3)
            ON CONFLICT (guild_id,kind) DO UPDATE SET kit_key=EXCLUDED.kit_key",
        )
        .bind(managed.id as i64)
        .bind(kind.key())
        .bind(selected)
        .execute(&mut *tx)
        .await?;
    } else {
        sqlx::query("DELETE FROM message_event_kits WHERE guild_id=$1 AND kind=$2")
            .bind(managed.id as i64)
            .bind(kind.key())
            .execute(&mut *tx)
            .await?;
    }
    if before.as_ref().map(|r| &r.0) != body.key.as_ref() {
        audit_in(
            &mut tx,
            &managed,
            "message_kit.bind_event",
            kind.key(),
            json!(before.map(|r| r.0)),
            json!(body.key),
        )
        .await?;
    }
    tx.commit().await?;

    Ok(Json(json!({"kind":kind.key(),"key":body.key})))
}

fn key(raw: &str) -> ApiResult<&str> {
    if raw.len() <= 40
        && !raw.is_empty()
        && raw
            .bytes()
            .enumerate()
            .all(|(i, b)| b.is_ascii_lowercase() || b.is_ascii_digit() || (i > 0 && matches!(b, b'_' | b'-')))
    {
        Ok(raw)
    } else {
        Err(FieldError::new("key", "Use a short lowercase kit key with letters, numbers, _ or -.").into())
    }
}

fn checked(value: Value) -> ApiResult<MessageKit> {
    if serde_json::to_vec(&value).is_ok_and(|v| v.len() > message_studio::KIT_BYTES_MAX) {
        return Err(FieldError::new("kit", "A kit is at most 32 KiB of JSON.").into());
    }
    let kit: MessageKit =
        serde_json::from_value(value).map_err(|_| FieldError::new("kit", "Check the kit JSON fields and types."))?;
    message_studio::validate(kit).map_err(ApiError::invalid)
}

async fn list(State(state): State<WebState>, managed: ManagedGuild) -> ApiResult<Json<Value>> {
    let rows: Vec<(String, String, Value, i64)> =
        sqlx::query_as("SELECT key,name,payload,version FROM message_kits WHERE guild_id=$1 ORDER BY name LIMIT 50")
            .bind(managed.id as i64)
            .fetch_all(state.app.db.pool())
            .await?;
    Ok(Json(json!(
        rows.into_iter()
            .map(|(key, name, payload, version)| json!({"key":key,"name":name,"payload":payload,"version":version}))
            .collect::<Vec<_>>()
    )))
}

async fn save(
    State(state): State<WebState>,
    managed: ManagedGuild,
    Path((_, raw)): Path<(String, String)>,
    ApiJson(value): ApiJson<Value>,
) -> ApiResult<Json<Value>> {
    save_kit(state, managed, raw, value, "message_kit.save").await
}

async fn save_kit(
    state: WebState,
    managed: ManagedGuild,
    raw: String,
    value: Value,
    action: &str,
) -> ApiResult<Json<Value>> {
    let key = key(&raw)?;
    let kit = checked(value)?;
    let mut tx = state.app.db.pool().begin().await?;
    lock_guild(&mut tx, managed.id).await?;
    let bound: (bool,) =
        sqlx::query_as("SELECT EXISTS(SELECT 1 FROM message_event_kits WHERE guild_id=$1 AND kit_key=$2)")
            .bind(managed.id as i64)
            .bind(key)
            .fetch_one(&mut *tx)
            .await?;
    if bound.0 && (!kit.embeds.is_empty() || !kit.components.is_empty()) {
        return Err(FieldError::new("kit", "A kit assigned to an event cannot have embeds or components.").into());
    }
    let payload = serde_json::to_value(&kit).map_err(anyhow::Error::from)?;
    let before: Option<(Value, i64)> =
        sqlx::query_as("SELECT payload,version FROM message_kits WHERE guild_id=$1 AND key=$2 FOR UPDATE")
            .bind(managed.id as i64)
            .bind(key)
            .fetch_optional(&mut *tx)
            .await?;
    let admin = managed.guild.owner
        || managed
            .guild
            .permissions
            .parse::<u64>()
            .is_ok_and(|bits| bits & crate::discord::ADMINISTRATOR != 0);
    if kit.allow_mass_mentions && !admin {
        return Err(FieldError::new(
            "allow_mass_mentions",
            "Only a server administrator can allow everyone and role mentions.",
        )
        .into());
    }
    if before.as_ref().is_some_and(|(old, _)| old == &payload) {
        if action == "message_kit.rollback" {
            audit_in(&mut tx, &managed, action, key, payload.clone(), payload.clone()).await?;
            tx.commit().await?;
        }
        return Ok(Json(
            json!({"key":key,"name":kit.name,"payload":payload,"version":before.unwrap().1}),
        ));
    }
    if before.is_none() {
        let (count,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM message_kits WHERE guild_id=$1")
            .bind(managed.id as i64)
            .fetch_one(&mut *tx)
            .await?;
        if count >= message_studio::KIT_COUNT_MAX {
            return Err(FieldError::new("kit", "A server can save 50 kits.").into());
        }
    }
    let version = before.as_ref().map_or(1, |(_, v)| v + 1);
    sqlx::query("INSERT INTO message_kits (guild_id,key,name,payload,version) VALUES ($1,$2,$3,$4,$5)
        ON CONFLICT (guild_id,key) DO UPDATE SET name=EXCLUDED.name,payload=EXCLUDED.payload,version=EXCLUDED.version,updated_at=now()")
        .bind(managed.id as i64).bind(key).bind(&kit.name).bind(&payload).bind(version).execute(&mut *tx).await?;
    sqlx::query(
        "INSERT INTO message_kit_history (guild_id,key,name,payload,version,actor_id) VALUES ($1,$2,$3,$4,$5,$6)",
    )
    .bind(managed.id as i64)
    .bind(key)
    .bind(&kit.name)
    .bind(&payload)
    .bind(version)
    .bind(managed.current.session.user_id as i64)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "DELETE FROM message_kit_history WHERE guild_id=$1 AND key=$2 AND id NOT IN
        (SELECT id FROM message_kit_history WHERE guild_id=$1 AND key=$2 ORDER BY id DESC LIMIT 20)",
    )
    .bind(managed.id as i64)
    .bind(key)
    .execute(&mut *tx)
    .await?;
    audit_in(
        &mut tx,
        &managed,
        action,
        key,
        before.map_or(Value::Null, |(v, _)| v),
        payload.clone(),
    )
    .await?;
    tx.commit().await?;
    Ok(Json(
        json!({"key":key,"name":kit.name,"payload":payload,"version":version}),
    ))
}

async fn history(
    State(state): State<WebState>,
    managed: ManagedGuild,
    Path((_, raw)): Path<(String, String)>,
) -> ApiResult<Json<Value>> {
    let key = key(&raw)?;
    let rows: Vec<(i64, Value, i64, chrono::DateTime<chrono::Utc>)> = sqlx::query_as(
        "SELECT version,payload,actor_id,created_at FROM message_kit_history WHERE guild_id=$1 AND key=$2 ORDER BY id DESC LIMIT 20")
        .bind(managed.id as i64).bind(key).fetch_all(state.app.db.pool()).await?;
    Ok(Json(json!(rows.into_iter().map(|(version,payload,actor,created)|
        json!({"version":version,"payload":payload,"actor_id":actor.to_string(),"created_at":created})).collect::<Vec<_>>())))
}

async fn rollback(
    State(state): State<WebState>,
    managed: ManagedGuild,
    Path((_, raw, version)): Path<(String, String, i64)>,
) -> ApiResult<Json<Value>> {
    let lang = managed.panel_lang;
    let key = key(&raw)?;
    let (payload,): (Value,) =
        sqlx::query_as("SELECT payload FROM message_kit_history WHERE guild_id=$1 AND key=$2 AND version=$3")
            .bind(managed.id as i64)
            .bind(key)
            .bind(version)
            .fetch_optional(state.app.db.pool())
            .await?
            .ok_or_else(|| {
                ApiError::malformed(t!(lang, "studio.web.errors.that-kit-version-is-no-longer-available"))
            })?;
    save_kit(state, managed, key.to_owned(), payload, "message_kit.rollback").await
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PreviewBody {
    payload: Value,
    #[serde(default)]
    variant: usize,
}

fn content(kit: &MessageKit, variant: usize, user: &str, server: &str) -> String {
    message_studio::render_content(
        kit,
        variant,
        &message_studio::MemberData {
            user,
            server,
            level: None,
            booster: false,
            returning: false,
            account_age_days: 30,
            ordinal: None,
        },
    )
}

fn render(mut kit: MessageKit, variant: usize, user: &str, server: &str, brand: u32) -> MessageKit {
    kit.content = content(&kit, variant, user, server);
    for embed in &mut kit.embeds {
        if embed.color.is_none() {
            embed.color = Some(format!("#{brand:06x}"));
        }
    }
    kit
}

async fn preview(
    State(state): State<WebState>,
    managed: ManagedGuild,
    ApiJson(body): ApiJson<PreviewBody>,
) -> ApiResult<Json<Value>> {
    let kit = checked(body.payload)?;
    let brand = state.app.brand_color(managed.id).await?;
    let kit = render(
        kit,
        body.variant,
        managed.current.session.display_name(),
        &managed.guild.name,
        brand,
    );
    Ok(Json(json!({
        "content": kit.content,
        "embeds": kit.embeds,
        "components": kit.components,
        "allowed_mentions": if kit.allow_mass_mentions { "everyone and roles allowed" } else { "none" },
    })))
}

fn components(kit: &MessageKit, key: &str, version: i64) -> Vec<Component> {
    let individual: Vec<Component> = kit
        .components
        .iter()
        .enumerate()
        .map(|(i, component)| match component {
            KitComponent::Button { label, url, .. } => Component::ActionRow(ActionRow {
                id: None,
                components: vec![Component::Button(Button {
                    custom_id: url.is_none().then(|| format!("ms:{key}:{version}:{i}")),
                    disabled: false,
                    emoji: None,
                    id: None,
                    label: Some(label.clone()),
                    sku_id: None,
                    style: if url.is_some() {
                        ButtonStyle::Link
                    } else {
                        ButtonStyle::Secondary
                    },
                    url: url.clone(),
                })],
            }),
            KitComponent::Select { placeholder, options } => Component::ActionRow(ActionRow {
                id: None,
                components: vec![Component::SelectMenu(SelectMenu {
                    channel_types: None,
                    custom_id: format!("ms:{key}:{version}:{i}"),
                    default_values: None,
                    disabled: false,
                    id: None,
                    kind: SelectMenuType::Text,
                    max_values: Some(1),
                    min_values: Some(1),
                    options: Some(
                        options
                            .iter()
                            .map(|o| SelectMenuOption {
                                default: false,
                                description: None,
                                emoji: None,
                                label: o.label.clone(),
                                value: o.value.clone(),
                            })
                            .collect(),
                    ),
                    placeholder: Some(placeholder.clone()),
                    required: None,
                })],
            }),
        })
        .collect();
    message_studio::component_rows(&kit.components)
        .into_iter()
        .map(|range| {
            Component::ActionRow(ActionRow {
                id: None,
                components: individual[range]
                    .iter()
                    .flat_map(|row| match row {
                        Component::ActionRow(row) => row.components.clone(),
                        _ => Vec::new(),
                    })
                    .collect(),
            })
        })
        .collect()
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SendBody {
    key: String,
    channel_id: String,
    #[serde(default)]
    message_id: Option<String>,
    #[serde(default)]
    variant: Option<usize>,
}

async fn send(
    State(state): State<WebState>,
    managed: ManagedGuild,
    ApiJson(body): ApiJson<SendBody>,
) -> ApiResult<Json<Value>> {
    let key = key(&body.key)?;
    let channel = crate::routes::admin::settings::parse_id(&body.channel_id)
        .ok_or_else(|| FieldError::new("channel_id", "Choose a channel."))?;
    let info = guild_info::get_with(&state, managed.id, |info| info.channel(channel).is_some()).await?;
    info.check_post_channel("channel_id", channel)?;
    let (payload, version): (Value, i64) =
        sqlx::query_as("SELECT payload,version FROM message_kits WHERE guild_id=$1 AND key=$2")
            .bind(managed.id as i64)
            .bind(key)
            .fetch_optional(state.app.db.pool())
            .await?
            .ok_or_else(|| FieldError::new("key", "Save this kit before posting it."))?;
    let kit = checked(payload)?;
    let index = if let Some(index) = body.variant {
        index
    } else if kit.variants.is_empty() {
        0
    } else {
        rand::random_range(0..kit.variants.len())
    };
    let brand = state.app.brand_color(managed.id).await?;
    let kit = render(
        kit,
        index,
        managed.current.session.display_name(),
        &managed.guild.name,
        brand,
    );
    let text = &kit.content;
    let embeds = kit.embeds.iter().map(pinchora_commands::embed_of).collect::<Vec<_>>();
    let components = components(&kit, key, version);
    let mentions = if kit.allow_mass_mentions {
        AllowedMentions {
            parse: vec![MentionType::Everyone, MentionType::Roles],
            ..Default::default()
        }
    } else {
        AllowedMentions::default()
    };
    let channel = Id::new(channel);
    let result = if let Some(raw) = body.message_id.as_deref() {
        let message = crate::routes::admin::settings::parse_id(raw)
            .ok_or_else(|| FieldError::new("message_id", "Enter a message ID."))?;
        state
            .app
            .http
            .update_message(channel, Id::new(message))
            .content(Some(text))
            .embeds(Some(&embeds))
            .components(Some(&components))
            .allowed_mentions(Some(&mentions))
            .await
    } else {
        let mut request = state
            .app
            .http
            .create_message(channel)
            .embeds(&embeds)
            .components(&components)
            .allowed_mentions(Some(&mentions));
        if !text.is_empty() {
            request = request.content(text);
        }
        request.await
    }
    .map_err(|error| {
        tracing::warn!(guild = managed.id, %error, "message kit send failed");
        ApiError::new(
            axum::http::StatusCode::BAD_GATEWAY,
            "discord",
            "Discord did not accept the message.",
        )
    })?;
    let sent = result.model().await.map_err(anyhow::Error::from)?;
    audit::finish(
        &state,
        &managed.current.session,
        Change {
            guild: managed.id,
            action: if body.message_id.is_some() {
                "message_kit.edit"
            } else {
                "message_kit.send"
            },
            target: Some(key),
            before: Value::Null,
            after: json!({"channel_id":channel.get().to_string(),"message_id":sent.id.to_string(),"version":version}),
        },
        None,
    )
    .await;
    Ok(Json(json!({"message_id":sent.id.to_string(),"version":version})))
}
