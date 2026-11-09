//! A Discord member proves ownership of a Twitch or Kick channel using a short public code.

use axum::{
    Json, Router,
    extract::{Path, State},
    routing::{get, post},
};
use pinchora_commands::community::feeds;
use pinchora_commands::community::feeds::live;
use pinchora_core::admin::settings::FieldError;
use pinchora_core::community::feeds::Kind;
use pinchora_core::community::feeds::Parsed;
use pinchora_core::community::feeds::parse_source;
use pinchora_core::infra::i18n::Lang;
use pinchora_core::t;
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

fn kind(raw: &str, lang: Lang) -> ApiResult<Kind> {
    match Kind::parse(raw) {
        Some(kind @ (Kind::Twitch | Kind::Kick)) => Ok(kind),
        _ => Err(ApiError::malformed(t!(lang, "feeds.link.kind"))),
    }
}

async fn list(State(state): State<WebState>, current: CurrentSession) -> ApiResult<Json<Value>> {
    let lang = state
        .app
        .db
        .user_language(current.session.user_id)
        .await?
        .unwrap_or(Lang::En);
    let rows = state.app.db.channel_links(current.session.user_id).await?;
    let kinds: Vec<Value> = [Kind::Twitch, Kind::Kick]
        .iter()
        .map(|kind| {
            json!({
                "key": kind.key(), "available": feeds::available(*kind),
                "why_not": (!feeds::available(*kind)).then(|| t!(lang, &format!("feeds.unavailable.{}", kind.key())))
            })
        })
        .collect();
    let strings: std::collections::BTreeMap<_, _> = pinchora_core::infra::i18n::keys(lang)
        .into_iter()
        .filter(|key| key.starts_with("feeds.ui."))
        .map(|key| (key.trim_start_matches("feeds.ui."), t!(lang, key)))
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
    let _guard = state.app.db.lock_feed_link(current.session.user_id).await?;
    let lang = state
        .app
        .db
        .user_language(current.session.user_id)
        .await?
        .unwrap_or(Lang::En);
    lookup_budget(&state, current.session.user_id).await?;
    let kind = kind(&body.kind, lang)?;
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
            if !feeds::change_live_role_for(&state.app, &feed, current.session.user_id, false).await {
                return Err(ApiError::blocked(t!(lang, "feeds.link.role-retry")));
            }
        }
    }
    let code = format!("pinchora-{:032x}", rand::random::<u128>());
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
    let _guard = state.app.db.lock_feed_link(current.session.user_id).await?;
    let lang = state
        .app
        .db
        .user_language(current.session.user_id)
        .await?
        .unwrap_or(Lang::En);
    let kind = kind(&raw, lang)?;
    if !feeds::available(kind) {
        return Err(ApiError::blocked(t!(
            lang,
            &format!("feeds.unavailable.{}", kind.key())
        )));
    }
    lookup_budget(&state, current.session.user_id).await?;
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
        return Err(ApiError::blocked(t!(lang, "feeds.link.expired")));
    }
    let read = live::lookup(kind, &row.source)
        .await
        .map_err(|_| ApiError::blocked(t!(lang, "feeds.link.check-failed")))?;
    let title = read.item.as_ref().map(|item| item.title.as_str()).unwrap_or_default();
    if !read.bio.contains(&code) && !title.contains(&code) {
        return Err(ApiError::blocked(t!(lang, "feeds.link.code-missing")));
    }
    if !state
        .app
        .db
        .verify_channel_link(current.session.user_id, kind.key(), &row.source, &code)
        .await?
    {
        return Err(ApiError::blocked(t!(lang, "feeds.link.expired")));
    }
    for feed in state.app.db.live_feeds_for_channel(kind.key(), &row.source).await? {
        if state.app.db.feed_channel_is_live(kind.key(), &row.source).await? {
            feeds::change_live_role_for(&state.app, &feed, current.session.user_id, true).await;
        }
    }
    Ok(Json(json!({"verified": true})))
}

async fn unlink(
    State(state): State<WebState>,
    current: CurrentSession,
    Path(raw): Path<String>,
) -> ApiResult<Json<Value>> {
    let _guard = state.app.db.lock_feed_link(current.session.user_id).await?;
    let lang = state
        .app
        .db
        .user_language(current.session.user_id)
        .await?
        .unwrap_or(Lang::En);
    let kind = kind(&raw, lang)?;
    if let Some(row) = state
        .app
        .db
        .channel_links(current.session.user_id)
        .await?
        .into_iter()
        .find(|row| row.kind == kind.key() && row.verified_at.is_some())
    {
        for feed in state.app.db.live_feeds_for_channel(kind.key(), &row.source).await? {
            if !feeds::change_live_role_for(&state.app, &feed, current.session.user_id, false).await {
                return Err(ApiError::blocked(t!(lang, "feeds.link.role-retry")));
            }
        }
    }
    state.app.db.unlink_channel(current.session.user_id, kind.key()).await?;
    Ok(Json(json!({"removed": true})))
}

async fn lookup_budget(state: &WebState, user: u64) -> ApiResult<()> {
    let count = state
        .app
        .cache
        .incr(
            &format!("feeds:link-lookups:{user}"),
            std::time::Duration::from_secs(60),
        )
        .await?;
    if count > 10 {
        return Err(ApiError::too_many(60));
    }
    Ok(())
}
