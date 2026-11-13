//! `GET /api/guilds/{g}/audit?before=<id>`: the server's dashboard changes, newest first, a page at
//! a time.

use axum::{
    Json,
    extract::{Query, State},
};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::routes::admin::guilds::AuditView;
use crate::{
    error::{ApiError, ApiResult},
    extract::ManagedGuild,
    state::WebState,
};
use pinchora_core::t;

/// Rows per page.
const PAGE: i64 = 50;

#[derive(Deserialize)]
pub struct AuditQuery {
    before: Option<String>,
}

pub async fn audit(
    State(state): State<WebState>,
    managed: ManagedGuild,
    Query(query): Query<AuditQuery>,
) -> ApiResult<Json<Value>> {
    let lang = managed.panel_lang;
    let before = match query.before.as_deref() {
        Some(raw) => Some(
            raw.parse::<i64>()
                .map_err(|_| ApiError::malformed(t!(lang, "log.web.errors.before-is-a-row-id")))?,
        ),
        None => None,
    };
    let rows = state.app.db.audit_page(managed.id, before, PAGE + 1).await?;
    let more = rows.len() as i64 > PAGE;
    let items: Vec<AuditView> = rows.into_iter().take(PAGE as usize).map(AuditView::from).collect();
    let next = more.then(|| items.last().map(|row| row.id.clone())).flatten();
    Ok(Json(json!({ "items": items, "next": next })))
}
