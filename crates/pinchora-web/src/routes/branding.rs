//! Guild branding editor. The existing card upload endpoint owns image decoding and limits.
use axum::{
    Json, Router,
    extract::State,
    routing::{get, post},
};
use pinchora_commands::branding::{self, ApplyResult};
use pinchora_core::{settings::FieldError, t};
use pinchora_db::GuildBranding;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::{
    audit::{self, Change},
    error::{ApiError, ApiJson, ApiResult},
    extract::ManagedGuild,
    state::WebState,
};

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
    if before.applied_at.is_none()
        && before.nickname.is_none()
        && before.avatar_upload_id.is_none()
        && before.banner_upload_id.is_none()
        && before.bio.is_none()
        && before.embed_color.is_none()
    {
        return Ok(Json(json!({ "branding": view(&before, guild), "discord_failed": [] })));
    }
    if !state.app.db.claim_branding_apply(guild).await? {
        return Err(ApiError::too_many(600));
    }
    let id = twilight_model::id::Id::new(guild);
    let mut errors = Vec::new();
    if state.app.http.update_current_member(id).nick(None).await.is_err() {
        errors.push("nickname");
    }
    if state.app.http.update_current_member(id).avatar(None).await.is_err() {
        errors.push("avatar");
    }
    if state.app.http.update_current_member(id).banner(None).await.is_err() {
        errors.push("banner");
    }
    if state.app.http.update_current_member(id).bio(None).await.is_err() {
        errors.push("bio");
    }
    state.app.db.reset_guild_branding(guild).await?;
    state.app.announce_guild_change(guild).await;
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
