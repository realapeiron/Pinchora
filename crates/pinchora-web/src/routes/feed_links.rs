//! A Discord member proves ownership of a Twitch or Kick channel using a short public code.

use axum::{
    Json, Router,
    extract::{Path, State},
    routing::{get, post},
};
use pinchora_commands::feeds::{self, live};
use pinchora_core::{
    feeds::{Kind, Parsed, parse_source},
    i18n::Lang,
    settings::FieldError,
    t,
};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::{
    error::{ApiError, ApiJson, ApiResult},
    extract::CurrentSession,
    state::WebState,
};

pub fn router() -> Router<WebState> {
    Router::new()
        .route("/my-channels", get(list).post(start))
        .route("/my-channels/{kind}/verify", post(verify))
        .route("/my-channels/{kind}", axum::routing::delete(unlink))
}

fn kind(raw: &str) -> ApiResult<Kind> {
    match Kind::parse(raw) {
        Some(kind @ (Kind::Twitch | Kind::Kick)) => Ok(kind),
        _ => Err(ApiError::malformed(t!(Lang::En, "feeds.link.kind"))),
    }
}

async fn list(State(state): State<WebState>, current: CurrentSession) -> ApiResult<Json<Value>> {
    let rows = state.app.db.channel_links(current.session.user_id).await?;
    let kinds: Vec<Value> = [Kind::Twitch, Kind::Kick].iter().map(|kind| json!({
        "key": kind.key(), "available": feeds::available(*kind),
        "why_not": (!feeds::available(*kind)).then(|| t!(Lang::En, &format!("feeds.unavailable.{}", kind.key())))
    })).collect();
    let strings: std::collections::BTreeMap<_, _> = pinchora_core::i18n::keys(Lang::En)
        .into_iter()
        .filter(|key| key.starts_with("feeds.ui."))
        .map(|key| (key.trim_start_matches("feeds.ui."), t!(Lang::En, key)))
        .collect();
    Ok(Json(json!({ "items": rows.iter().map(|row| json!({
        "kind": row.kind, "source": row.source, "code": row.verification_code,
        "expires_at": row.code_expires_at, "verified_at": row.verified_at
    })).collect::<Vec<_>>(),
        "kinds": kinds, "strings": strings
    })))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct StartBody {
    kind: String,
    source: String,
}

async fn start(
    State(state): State<WebState>,
    current: CurrentSession,
    ApiJson(body): ApiJson<StartBody>,
) -> ApiResult<Json<Value>> {
    let lang = Lang::En;
    let kind = kind(&body.kind)?;
    if !feeds::available(kind) {
        return Err(ApiError::blocked(t!(
            lang,
            &format!("feeds.unavailable.{}", kind.key())
        )));
    }
    let Parsed::Ready(source) = parse_source(kind, &body.source)
        .map_err(|e| ApiError::invalid(vec![FieldError::new("source", t!(lang, e.key()))]))?
    else {
        unreachable!()
    };
    live::lookup(kind, &source)
        .await
        .map_err(|_| ApiError::invalid(vec![FieldError::new("source", t!(lang, "feeds.error.no-account"))]))?;
    if let Some(old) = state
        .app
        .db
        .channel_links(current.session.user_id)
        .await?
        .into_iter()
        .find(|row| row.kind == kind.key() && row.verified_at.is_some())
    {
        for feed in state.app.db.live_feeds_for_channel(kind.key(), &old.source).await? {
            feeds::change_live_role(&state.app, &feed, false).await;
        }
    }
    let code = format!("pinchora-{:016x}", rand::random::<u64>());
    if !state
        .app
        .db
        .start_channel_link(current.session.user_id, kind.key(), &source, &code)
        .await?
    {
        return Err(ApiError::invalid(vec![FieldError::new(
            "source",
            t!(lang, "feeds.link.taken"),
        )]));
    }
    Ok(Json(
        json!({"kind": kind.key(), "source": source, "code": code, "expires_in_minutes": 15}),
    ))
}

async fn verify(
    State(state): State<WebState>,
    current: CurrentSession,
    Path(raw): Path<String>,
) -> ApiResult<Json<Value>> {
    let kind = kind(&raw)?;
    let row = state
        .app
        .db
        .channel_links(current.session.user_id)
        .await?
        .into_iter()
        .find(|row| row.kind == kind.key())
        .ok_or_else(ApiError::not_found)?;
    let code = row.verification_code.ok_or_else(ApiError::not_found)?;
    if row.code_expires_at.is_none_or(|at| at <= chrono::Utc::now()) {
        return Err(ApiError::blocked(t!(Lang::En, "feeds.link.expired")));
    }
    let read = live::lookup(kind, &row.source)
        .await
        .map_err(|_| ApiError::blocked(t!(Lang::En, "feeds.link.check-failed")))?;
    let title = read.item.as_ref().map(|item| item.title.as_str()).unwrap_or_default();
    if !read.bio.contains(&code) && !title.contains(&code) {
        return Err(ApiError::blocked(t!(Lang::En, "feeds.link.code-missing")));
    }
    if !state
        .app
        .db
        .verify_channel_link(current.session.user_id, kind.key(), &row.source, &code)
        .await?
    {
        return Err(ApiError::blocked(t!(Lang::En, "feeds.link.expired")));
    }
    for feed in state.app.db.live_feeds_for_channel(kind.key(), &row.source).await? {
        feeds::change_live_role(&state.app, &feed, true).await;
    }
    Ok(Json(json!({"verified": true})))
}

async fn unlink(
    State(state): State<WebState>,
    current: CurrentSession,
    Path(raw): Path<String>,
) -> ApiResult<Json<Value>> {
    let kind = kind(&raw)?;
    if let Some(row) = state
        .app
        .db
        .channel_links(current.session.user_id)
        .await?
        .into_iter()
        .find(|row| row.kind == kind.key())
    {
        for feed in state.app.db.live_feeds_for_channel(kind.key(), &row.source).await? {
            feeds::change_live_role(&state.app, &feed, false).await;
        }
    }
    state.app.db.unlink_channel(current.session.user_id, kind.key()).await?;
    Ok(Json(json!({"removed": true})))
}
