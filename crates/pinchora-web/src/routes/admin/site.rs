//! What the public website needs from the server: the totals and the viewer behind the landing page
//! (`GET /api/public/landing`) and the link that adds the bot (`GET /invite`).

use std::time::Duration;

use axum::{
    Json,
    extract::State,
    http::{HeaderMap, header},
    response::{IntoResponse, Redirect, Response},
};
use pinchora_db::PublicCounts;
use serde_json::{Value, json};
use tokio::sync::Mutex;

use crate::auth::session;
use crate::auth::session::SESSION_COOKIE;
use crate::discord;
use crate::routes::infra::me::avatar_url;
use crate::state::WebState;

/// Where the totals are kept, shared by every web pod.
const COUNTS_KEY: &str = "web:landing-counts";
/// How long a count is trusted: the counts scan whole tables, and a visitor sees no difference.
const COUNTS_TTL: Duration = Duration::from_secs(600);
/// One refresh at a time per pod, so a cold cache costs one scan however many visitors arrive.
static REFRESH: Mutex<()> = Mutex::const_new(());

fn counts_json(counts: &PublicCounts) -> Value {
    json!({
        "servers": counts.servers,
        "players": counts.players,
        "houses": counts.houses,
        "pets": counts.pets,
    })
}

async fn cached(state: &WebState) -> Option<Value> {
    let text = state.app.cache.get(COUNTS_KEY).await.ok().flatten()?;
    serde_json::from_str(&text).ok()
}

/// The totals, from the cache or counted now; `None` while the database cannot answer.
async fn counts(state: &WebState) -> Option<Value> {
    if let Some(known) = cached(state).await {
        return Some(known);
    }
    let _turn = REFRESH.lock().await;
    // Someone ahead of us in the queue may have filled it.
    if let Some(known) = cached(state).await {
        return Some(known);
    }
    match state.app.db.public_counts().await {
        Ok(counts) => {
            let value = counts_json(&counts);
            if let Err(error) = state.app.cache.put(COUNTS_KEY, &value.to_string(), COUNTS_TTL).await {
                tracing::debug!(%error, "landing totals not cached");
            }
            Some(value)
        }
        Err(error) => {
            tracing::warn!(%error, "could not count for the landing page");
            None
        }
    }
}

/// Who is looking, when they are logged in: enough to say "Dashboard" and show their avatar.
async fn viewer(state: &WebState, headers: &HeaderMap) -> Option<Value> {
    let sid = session::cookie(headers, SESSION_COOKIE)?;
    let now = chrono::Utc::now().timestamp();
    let session = session::load(&state.app.cache, sid, now).await.ok().flatten()?;
    Some(json!({
        "name": session.display_name(),
        "avatar_url": avatar_url(&session),
    }))
}

/// `GET /api/public/landing`: `{ stats: { servers, players, houses, pets } | null, viewer: { name,
/// avatar_url } | null }`. No login needed; the totals are cached for ten minutes.
pub async fn landing(State(state): State<WebState>, headers: HeaderMap) -> Json<Value> {
    let (stats, viewer) = tokio::join!(counts(&state), viewer(&state, &headers));
    Json(json!({ "stats": stats, "viewer": viewer }))
}

/// Public 30-second snapshot shared with `/info bot`.
pub async fn status(State(state): State<WebState>) -> Json<pinchora_commands::admin::status::PublicStatus> {
    Json(
        pinchora_commands::admin::status::snapshot(
            &state.app.db,
            &state.app.cache,
            state.app.application_id.get(),
            &state.app.public_url,
        )
        .await,
    )
}

/// `GET /invite`: sends the visitor to Discord's page for adding the bot, with the commands scope
/// and the permissions it uses. A link on the site and in the README never goes stale.
pub async fn invite(State(state): State<WebState>) -> Response {
    let url = discord::bot_invite_url(&state.settings, state.app.application_id.get());
    ([(header::CACHE_CONTROL, "no-cache")], Redirect::temporary(&url)).into_response()
}
