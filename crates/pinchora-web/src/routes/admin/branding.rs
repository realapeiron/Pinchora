//! Guild branding editor. The existing card upload endpoint owns image decoding and limits.
use axum::{
    Json, Router,
    extract::State,
    routing::{get, post},
};
use pinchora_commands::admin::branding;
use pinchora_commands::admin::branding::ApplyResult;
use pinchora_core::admin::settings::FieldError;
use pinchora_core::t;
use pinchora_db::GuildBranding;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::audit;
use crate::audit::Change;
use crate::error::ApiError;
use crate::error::ApiJson;
use crate::error::ApiResult;
use crate::extract::ManagedGuild;
use crate::state::WebState;

pub fn router() -> Router<WebState> {
    Router::new()
        .route("/", get(show).put(save))
        .route("/apply", post(apply))
        .route("/reset", post(reset))
}

fn view(row: &GuildBranding, guild: u64) -> Value {
    json!({
        "nickname": row.nickname, "avatar_upload_id": row.avatar_upload_id,
        "banner_upload_id": row.banner_upload_id, "bio": row.bio,
        "embed_color": row.embed_color.map(|c| format!("#{c:06X}")),
        "active_nickname": row.active_nickname, "active_bio": row.active_bio,
        "active_avatar_upload_id": row.active_avatar_upload_id, "active_banner_upload_id": row.active_banner_upload_id,
        "active_embed_color": row.active_embed_color.map(|c| format!("#{c:06X}")),
        "applied_at": row.applied_at, "last_error": row.last_error,
        "avatar_supported": row.avatar_supported, "banner_supported": row.banner_supported,
        "bio_supported": row.bio_supported,
        "avatar_url": row.avatar_upload_id.as_ref().map(|id| format!("/api/guilds/{guild}/cards/uploads/{id}")),
        "banner_url": row.banner_upload_id.as_ref().map(|id| format!("/api/guilds/{guild}/cards/uploads/{id}")),
    })
}

pub async fn show(State(state): State<WebState>, managed: ManagedGuild) -> ApiResult<Json<Value>> {
    Ok(Json(view(&state.app.db.guild_branding(managed.id).await?, managed.id)))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Draft {
    nickname: Option<String>,
    avatar_upload_id: Option<String>,
    banner_upload_id: Option<String>,
    bio: Option<String>,
    embed_color: Option<String>,
}

pub async fn save(
    State(state): State<WebState>,
    managed: ManagedGuild,
    ApiJson(draft): ApiJson<Draft>,
) -> ApiResult<Json<Value>> {
    let guild = managed.id;
    let lang = state.app.guild_lang(guild).await;
    let before = state.app.db.guild_branding(guild).await?;
    let color = draft
        .embed_color
        .as_deref()
        .map(|s| {
            branding::parse_color(s).ok_or_else(|| FieldError::new("embed_color", t!(lang, "branding.error.colour")))
        })
        .transpose()?;
    let row = GuildBranding {
        guild_id: guild as i64,
        nickname: draft.nickname,
        avatar_upload_id: draft.avatar_upload_id,
        banner_upload_id: draft.banner_upload_id,
        bio: draft.bio,
        embed_color: color,
        ..before.clone()
    };
    branding::validate(&row).map_err(|e| {
        ApiError::invalid(vec![FieldError::new(
            "branding",
            t!(lang, &format!("branding.error.{e}")),
        )])
    })?;
    for id in [&row.avatar_upload_id, &row.banner_upload_id].into_iter().flatten() {
        if state.app.db.card_upload(guild, id).await?.is_none() {
            return Err(FieldError::new("upload", t!(lang, "branding.error.upload")).into());
        }
    }
    state.app.db.save_guild_branding(guild, &row).await?;
    audit::finish(
        &state,
        &managed.current.session,
        Change {
            guild,
            action: "branding.save",
            target: None,
            before: view(&before, guild),
            after: view(&row, guild),
        },
        None,
    )
    .await;
    Ok(Json(view(&state.app.db.guild_branding(guild).await?, guild)))
}

pub async fn apply(State(state): State<WebState>, managed: ManagedGuild) -> ApiResult<Json<Value>> {
    let guild = managed.id;
    let row = state.app.db.guild_branding(guild).await?;
    let result = branding::apply(&state.app, guild, &row).await?;
    if matches!(result, ApplyResult::Cooldown) {
        return Err(ApiError::too_many(600));
    }
    let after = state.app.db.guild_branding(guild).await?;
    audit::finish(
        &state,
        &managed.current.session,
        Change {
            guild,
            action: "branding.apply",
            target: None,
            before: view(&row, guild),
            after: view(&after, guild),
        },
        None,
    )
    .await;
    Ok(Json(view(&after, guild)))
}

pub async fn reset(State(state): State<WebState>, managed: ManagedGuild) -> ApiResult<Json<Value>> {
    let guild = managed.id;
    let before = state.app.db.guild_branding(guild).await?;
    let result = branding::reset(&state.app, guild).await?;
    let errors = match result {
        ApplyResult::Cooldown => return Err(ApiError::too_many(600)),
        ApplyResult::Applied => Vec::new(),
        ApplyResult::Partial(fields) => fields,
    };
    let after = state.app.db.guild_branding(guild).await?;
    audit::finish(
        &state,
        &managed.current.session,
        Change {
            guild,
            action: "branding.reset",
            target: None,
            before: view(&before, guild),
            after: view(&after, guild),
        },
        None,
    )
    .await;
    Ok(Json(
        json!({ "branding": view(&after, guild), "discord_failed": errors }),
    ))
}
