//! Filtered audit history and a keyset-paged CSV stream with hard row/byte budgets.
use super::guilds::AuditView;
use crate::{
    error::{ApiError, ApiResult},
    extract::ManagedGuild,
    state::WebState,
};
use axum::{
    Json, Router,
    body::Body,
    extract::{Query, State},
    http::header,
    response::{IntoResponse, Response},
    routing::get,
};
use chrono::{DateTime, Utc};
use futures_util::{StreamExt, stream};
use pinchora_core::{i18n::Lang, t};
use pinchora_db::{AuditFilter, AuditRow};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{collections::VecDeque, io, time::Duration};
use tokio::time::Instant;

const PAGE: i64 = 50;
const EXPORT_ROWS: usize = 10_000;
const EXPORT_TIME: Duration = Duration::from_secs(10);
const EXPORT_SCANNED: usize = 100_000;
const EXPORT_BYTES: usize = 8 * 1024 * 1024;
const HEADER: &str = "id,actor_id,actor_name,source,area,action,target,before,after,created_at\r\n";

pub fn router() -> Router<WebState> {
    Router::new().route("/", get(page)).route("/export", get(export))
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct Filters {
    before: Option<String>,
    actor: Option<String>,
    area: Option<String>,
    from: Option<String>,
    until: Option<String>,
    text: Option<String>,
}
impl Filters {
    fn parse(self, lang: Lang) -> ApiResult<(Option<i64>, AuditFilter)> {
        let bad = || ApiError::malformed(t!(lang, "server_tools.audit-invalid"));
        let id = |s: Option<String>| -> ApiResult<Option<i64>> {
            s.filter(|s| !s.is_empty())
                .map(|s| s.parse().ok().filter(|n| *n > 0).ok_or_else(bad))
                .transpose()
        };
        let date = |s: Option<String>| -> ApiResult<Option<DateTime<Utc>>> {
            s.filter(|s| !s.is_empty())
                .map(|s| {
                    DateTime::parse_from_rfc3339(&s)
                        .map(|d| d.with_timezone(&Utc))
                        .map_err(|_| bad())
                })
                .transpose()
        };
        let before = id(self.before)?;
        let actor = id(self.actor)?;
        let from = date(self.from)?;
        let until = date(self.until)?;
        let area = self.area.filter(|s| !s.is_empty());
        let text = self.text.filter(|s| !s.is_empty());
        if from.zip(until).is_some_and(|(a, b)| a >= b)
            || area.as_ref().is_some_and(|s| {
                s.len() > 64 || !s.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'))
            })
            || text.as_ref().is_some_and(|s| s.chars().count() > 200)
        {
            return Err(bad());
        }
        Ok((
            before,
            AuditFilter {
                actor,
                area,
                from,
                until,
                text,
            },
        ))
    }
}
async fn page(
    State(state): State<WebState>,
    managed: ManagedGuild,
    Query(query): Query<Filters>,
) -> ApiResult<Json<Value>> {
    let lang = state
        .app
        .user_lang_choice(managed.current.session.user_id)
        .await
        .unwrap_or(state.app.guild_lang(managed.id).await);
    let (before, filter) = query.parse(lang)?;
    let rows = state
        .app
        .db
        .filtered_audit_page(managed.id, before, PAGE, &filter)
        .await?;
    let next = rows.next.map(|id| id.to_string());
    let items: Vec<_> = rows.items.into_iter().map(AuditView::from).collect();
    Ok(Json(json!({"items": items, "next": next})))
}

/// Quote every cell and neutralise formulas, including whitespace-prefixed formula payloads.
fn cell(value: &str) -> String {
    let unsafe_start = value
        .trim_start_matches(|c: char| c.is_whitespace() || c.is_control() || c == '\u{feff}')
        .starts_with(['=', '+', '-', '@']);
    format!(
        "\"{}{}\"",
        if unsafe_start { "'" } else { "" },
        value.replace('"', "\"\"")
    )
}
fn csv(mut row: AuditRow) -> String {
    row.redact();
    [
        row.id.to_string(),
        row.actor_id.to_string(),
        row.actor_name,
        row.source,
        row.action.split('.').next().unwrap_or_default().to_owned(),
        row.action,
        row.target.unwrap_or_default(),
        row.before.unwrap_or_default(),
        row.after.unwrap_or_default(),
        row.created_at.to_rfc3339(),
    ]
    .iter()
    .map(|s| cell(s))
    .collect::<Vec<_>>()
    .join(",")
        + "\r\n"
}
struct Export {
    state: WebState,
    guild: u64,
    filter: AuditFilter,
    before: Option<i64>,
    pending: VecDeque<AuditRow>,
    rows: usize,
    bytes: usize,
    done: bool,
    notice: String,
    deadline: Instant,
    scanned: usize,
}
impl Export {
    fn limit(&mut self) -> String {
        self.done = true;
        self.notice.clone()
    }
    async fn next(mut self) -> Result<Option<(String, Self)>, io::Error> {
        if self.done {
            return Ok(None);
        }
        if Instant::now() >= self.deadline || self.scanned >= EXPORT_SCANNED {
            let notice = self.limit();
            return Ok(Some((notice, self)));
        }
        while self.pending.is_empty() {
            let Some(cursor) = self.before else {
                return Ok(None);
            };
            let query = self
                .state
                .app
                .db
                .filtered_audit_page(self.guild, Some(cursor), PAGE, &self.filter);
            let page = match tokio::time::timeout_at(self.deadline, query).await {
                Ok(Ok(page)) => page,
                Ok(Err(error)) => return Err(io::Error::other(error)),
                Err(_) => {
                    let notice = self.limit();
                    return Ok(Some((notice, self)));
                }
            };
            self.before = page.next;
            self.scanned += page.scanned;
            self.pending = page.items.into();
            if self.scanned >= EXPORT_SCANNED {
                let notice = self.limit();
                return Ok(Some((notice, self)));
            }
        }
        let row = self.pending.pop_front().expect("nonempty page");
        let line = csv(row);
        let chunk = if self.rows >= EXPORT_ROWS || self.bytes + line.len() + self.notice.len() > EXPORT_BYTES {
            self.limit()
        } else {
            self.rows += 1;
            self.bytes += line.len();
            line
        };
        Ok(Some((chunk, self)))
    }
}
async fn export(
    State(state): State<WebState>,
    managed: ManagedGuild,
    Query(query): Query<Filters>,
) -> ApiResult<Response> {
    let lang = state
        .app
        .user_lang_choice(managed.current.session.user_id)
        .await
        .unwrap_or(state.app.guild_lang(managed.id).await);
    let (before, filter) = query.parse(lang)?;
    let deadline = Instant::now() + EXPORT_TIME;
    let pending = tokio::time::timeout_at(
        deadline,
        state.app.db.filtered_audit_page(managed.id, before, PAGE, &filter),
    )
    .await
    .map_err(|error| ApiError::internal(anyhow::Error::from(error)))??;
    let notice = format!(
        "{},{},,,,,,,,\r\n",
        cell("EXPORT_LIMIT"),
        cell(&t!(lang, "server_tools.export-limit"))
    );
    let export = Export {
        state,
        guild: managed.id,
        filter,
        before: pending.next,
        scanned: pending.scanned,
        pending: pending.items.into(),
        deadline,
        rows: 0,
        bytes: HEADER.len(),
        done: false,
        notice,
    };
    let body =
        stream::once(async { Ok::<_, io::Error>(HEADER.to_owned()) }).chain(stream::try_unfold(export, Export::next));
    Ok((
        [
            (header::CONTENT_TYPE, "text/csv; charset=utf-8"),
            (header::CONTENT_DISPOSITION, "attachment; filename=pinchora-audit.csv"),
            (header::CACHE_CONTROL, "no-store"),
        ],
        Body::from_stream(body),
    )
        .into_response())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn csv_quotes_and_neutralises_spreadsheet_formulas() {
        for raw in [
            "=cmd()",
            "+SUM(1)",
            "-2",
            "@evil",
            " +SUM(1)",
            "\t@evil",
            "\n-2",
            "\u{feff}=cmd()",
        ] {
            assert!(cell(raw).starts_with("\"'"));
        }
        assert_eq!(cell("hello,\"reef\"\nworld"), "\"hello,\"\"reef\"\"\nworld\"");
    }
    #[test]
    fn filters_validate_ids_ranges_and_bounded_literal_searches() {
        assert!(
            Filters {
                actor: Some("0".into()),
                ..Default::default()
            }
            .parse(Lang::En)
            .is_err()
        );
        assert!(
            Filters {
                area: Some("roles%".into()),
                ..Default::default()
            }
            .parse(Lang::Tr)
            .is_err()
        );
        assert!(
            Filters {
                text: Some("x".repeat(201)),
                ..Default::default()
            }
            .parse(Lang::En)
            .is_err()
        );
        assert!(
            Filters {
                from: Some("2026-01-02T00:00:00Z".into()),
                until: Some("2026-01-01T00:00:00Z".into()),
                ..Default::default()
            }
            .parse(Lang::En)
            .is_err()
        );
        assert!(
            Filters {
                text: Some("100%_literal".into()),
                ..Default::default()
            }
            .parse(Lang::En)
            .is_ok()
        );
    }
}
