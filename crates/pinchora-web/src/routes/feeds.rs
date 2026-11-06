//! `/api/guilds/{g}/feeds`: the Feeds page. Same rules as `/feeds`: the source is looked up (and the
//! address checked) by the bot's own code, the message is drawn by the function that posts it, and
//! every change lands in the audit log.

use std::collections::HashMap;

use axum::{
    Json, Router,
    extract::{Path, State},
    routing::{get, post, put},
};
use chrono::{DateTime, Utc};
use pinchora_commands::feeds::{self, Draft, Problem, available, default_message, error_text, kind_label, render};
use pinchora_core::{
    feeds::{
        FILTER_WORD_MAX, FILTER_WORDS_MAX, Item, Kind, MAX_FEEDS, MESSAGE_MAX, PLACEHOLDERS, account_label,
        check_message, filter_words, page_url,
    },
    i18n::Lang,
    settings::FieldError,
    t,
};
use pinchora_db::{FeedAdd, FeedChange, FeedEdit, FeedListRow, FeedRow, NewFeed, PostOutcome};
use serde::Deserialize;
use serde_json::{Value, json};

use super::settings::{parse_id, present};
use crate::{
    audit::{self, Change},
    error::{ApiError, ApiJson, ApiResult},
    extract::ManagedGuild,
    guild_info::{self, GuildInfo},
    state::WebState,
};

/// The dashboard speaks English.
const LANG: Lang = Lang::En;

pub fn router() -> Router<WebState> {
    Router::new()
        .route("/", get(list).post(create))
        .route("/check", post(check))
        .route("/preview", post(preview))
        .route("/{id}", put(replace).delete(delete))
        .route("/{id}/test", post(test))
}

/// A feed's health in one word and one sentence.
fn health(row: &FeedListRow) -> (&'static str, String) {
    let (feed, source) = (&row.feed, &row.source);
    if !feed.enabled {
        let why = feed.last_error.as_deref().map(|e| error_text(LANG, e));
        return (
            "off",
            why.map_or_else(|| "Switched off.".to_owned(), |w| format!("Switched off: {w}")),
        );
    }
    if let Some(error) = &feed.last_error {
        return ("cannot_post", format!("Can't post: {}", error_text(LANG, error)));
    }
    if source.fail_count > 0
        && let Some(error) = &source.last_error
    {
        return ("failing", format!("Last check failed: {}", error_text(LANG, error)));
    }
    if source.last_checked_at.is_none() {
        return ("waiting", "Waiting for its first check.".to_owned());
    }
    ("ok", "Healthy.".to_owned())
}

fn feed_json(row: &FeedListRow) -> Value {
    let feed = &row.feed;
    let kind = Kind::parse(&feed.kind).unwrap_or(Kind::Rss);
    let (state, text) = health(row);
    json!({
        "id": feed.id.to_string(),
        "kind": feed.kind,
        "source": feed.source,
        "account": account_label(kind, &feed.source, feed.display_name.as_deref()),
        "link": page_url(kind, &feed.source),
        "channel_id": feed.channel_id.to_string(),
        "message": feed.message,
        "embed": feed.embed,
        "ping_role_id": feed.ping_role_id.map(|r| r.to_string()),
        "filter_include": feed.filter_include,
        "filter_exclude": feed.filter_exclude,
        "enabled": feed.enabled,
        "created_at": feed.created_at,
        "health": {
            "state": state,
            "text": text,
            "last_checked_at": row.source.last_checked_at,
            "last_item_at": row.source.last_item_at,
            "last_sent_at": feed.last_sent_at,
            "next_poll_at": row.source.next_poll_at,
            "fail_count": row.source.fail_count,
            "interval_minutes": (row.source.interval_secs + 59) / 60,
        },
    })
}

/// What an audit row keeps of a feed.
fn audit_json(feed: &FeedRow) -> Value {
    json!({
        "kind": feed.kind,
        "source": feed.source,
        "channel_id": feed.channel_id.to_string(),
        "message": feed.message,
        "embed": feed.embed,
        "ping_role_id": feed.ping_role_id.map(|r| r.to_string()),
        "filter_include": feed.filter_include,
        "filter_exclude": feed.filter_exclude,
        "enabled": feed.enabled,
    })
}

async fn view(state: &WebState, guild: u64) -> ApiResult<Value> {
    let items: Vec<Value> = state.app.db.feeds_of(guild).await?.iter().map(feed_json).collect();
    let kinds: Vec<Value> = Kind::ALL
        .into_iter()
        .map(|kind| {
            json!({
                "key": kind.key(),
                "label": kind_label(LANG, kind),
                "available": available(kind),
                "why_not": (!available(kind)).then(|| t!(LANG, &format!("feeds.unavailable.{}", kind.key()))),
                "default_message": default_message(LANG, kind),
                "interval_minutes": (kind.interval_secs() + 59) / 60,
            })
        })
        .collect();
    let placeholders: Vec<Value> = PLACEHOLDERS
        .iter()
        .map(|token| json!({ "token": token, "hint": placeholder_hint(token) }))
        .collect();
    Ok(json!({
        "items": items,
        "max": MAX_FEEDS,
        "message_max": MESSAGE_MAX,
        "filter_words_max": FILTER_WORDS_MAX,
        "filter_word_max": FILTER_WORD_MAX,
        "kinds": kinds,
        "placeholders": placeholders,
    }))
}

fn placeholder_hint(token: &str) -> &'static str {
    match token {
        "{title}" => "The post's title",
        "{url}" => "The link to the post",
        "{author}" => "Who wrote it",
        "{account}" => "The channel, feed, subreddit or account",
        _ => "When it was published, in each reader's time zone",
    }
}

/// `GET /feeds`.
pub async fn list(State(state): State<WebState>, managed: ManagedGuild) -> ApiResult<Json<Value>> {
    Ok(Json(view(&state, managed.id).await?))
}

fn kind_of(raw: &str) -> ApiResult<Kind> {
    Kind::parse(raw).ok_or_else(|| ApiError::invalid(vec![FieldError::new("kind", "Pick where the posts come from.")]))
}

/// Looks the source up, within the server's lookup budget.
async fn look_up(state: &WebState, guild: u64, kind: Kind, source: &str) -> ApiResult<feeds::Found> {
    if !feeds::spend_lookup(&state.app, guild).await {
        return Err(ApiError::too_many(60));
    }
    feeds::look_up(kind, source).await.map_err(|problem| {
        let field = if matches!(problem, Problem::Unavailable(_)) {
            "kind"
        } else {
            "source"
        };
        ApiError::invalid(vec![FieldError::new(field, problem.text(LANG))])
    })
}

fn item_json(item: &Item) -> Value {
    json!({
        "title": item.title,
        "url": item.url,
        "author": item.author,
        "published": item.published,
        "image": item.image,
        "summary": item.summary,
    })
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CheckBody {
    kind: String,
    source: String,
}

/// `POST /feeds/check`: what a source is, before following it: its name and its latest post.
pub async fn check(
    State(state): State<WebState>,
    managed: ManagedGuild,
    ApiJson(body): ApiJson<CheckBody>,
) -> ApiResult<Json<Value>> {
    let kind = kind_of(&body.kind)?;
    let found = look_up(&state, managed.id, kind, &body.source).await?;
    Ok(Json(json!({
        "kind": kind.key(),
        "source": found.source,
        "account": found.account(),
        "title": found.title,
        "link": page_url(kind, &found.source),
        "latest": found.latest().map(item_json),
        "interval_minutes": (found.interval_secs + 59) / 60,
    })))
}

/// The parts of a feed the form sends; a field left out keeps its value on an edit.
#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct FeedBody {
    #[serde(default)]
    kind: Option<String>,
    #[serde(default)]
    source: Option<String>,
    #[serde(default)]
    channel_id: Option<String>,
    /// `null` is the default message.
    #[serde(default, deserialize_with = "present")]
    message: Option<Option<String>>,
    #[serde(default)]
    embed: Option<bool>,
    /// `null` pings nobody.
    #[serde(default, deserialize_with = "present")]
    ping_role_id: Option<Option<String>>,
    #[serde(default)]
    filter_include: Option<Vec<String>>,
    #[serde(default)]
    filter_exclude: Option<Vec<String>>,
    #[serde(default)]
    enabled: Option<bool>,
}

/// The checked edit a body asks for, against the server's channels and roles.
fn checked_edit(body: &FeedBody, info: &GuildInfo, guild: u64) -> Result<FeedEdit, Vec<FieldError>> {
    let mut errors = Vec::new();
    let channel = body.channel_id.as_deref().and_then(|raw| match parse_id(raw) {
        Some(id) => match info.check_post_channel("channel_id", id) {
            Ok(()) => Some(id),
            Err(error) => {
                errors.push(error);
                None
            }
        },
        None => {
            errors.push(FieldError::new("channel_id", "Pick a channel."));
            None
        }
    });
    let message = body.message.as_ref().map(|m| match check_message(m.as_deref()) {
        Ok(message) => message,
        Err(error) => {
            errors.push(FieldError::new("message", t!(LANG, error.key(), max = MESSAGE_MAX)));
            None
        }
    });
    let ping_role = body.ping_role_id.as_ref().map(|raw| {
        let id = raw.as_deref().and_then(parse_id)?;
        match info.check_role("ping_role_id", id) {
            Ok(role) if role.everyone || id == guild => {
                errors.push(FieldError::new("ping_role_id", t!(LANG, "feeds.no-everyone")));
                None
            }
            Ok(_) => Some(id),
            Err(error) => {
                errors.push(error);
                None
            }
        }
    });
    let mut words = |field: &'static str, list: &Option<Vec<String>>| {
        list.as_ref()
            .and_then(|list| match filter_words(list.iter().map(String::as_str)) {
                Ok(words) => Some(words),
                Err(error) => {
                    let text = t!(LANG, error.key(), max = FILTER_WORDS_MAX, length = FILTER_WORD_MAX);
                    errors.push(FieldError::new(field, text));
                    None
                }
            })
    };
    let include = words("filter_include", &body.filter_include);
    let exclude = words("filter_exclude", &body.filter_exclude);
    if !errors.is_empty() {
        return Err(errors);
    }
    Ok(FeedEdit {
        channel,
        message,
        embed: body.embed,
        ping_role,
        include,
        exclude,
        enabled: body.enabled,
    })
}

fn feed_id(params: &HashMap<String, String>) -> ApiResult<i64> {
    params
        .get("id")
        .and_then(|raw| raw.parse::<i64>().ok())
        .filter(|id| *id > 0)
        .ok_or_else(|| ApiError::malformed("Feed ids are numbers."))
}

/// `POST /feeds`: follows a source.
pub async fn create(
    State(state): State<WebState>,
    managed: ManagedGuild,
    ApiJson(body): ApiJson<FeedBody>,
) -> ApiResult<Json<Value>> {
    let guild = managed.id;
    let kind = kind_of(body.kind.as_deref().unwrap_or_default())?;
    if body.channel_id.is_none() {
        return Err(ApiError::invalid(vec![FieldError::new(
            "channel_id",
            "Pick a channel.",
        )]));
    }
    let info = guild_info::get(&state, guild, false).await?;
    let edit = checked_edit(&body, &info, guild).map_err(ApiError::invalid)?;
    let found = look_up(&state, guild, kind, body.source.as_deref().unwrap_or_default()).await?;
    let seen: Vec<String> = found.items.iter().map(|i| i.id.clone()).collect();
    let (include, exclude) = (edit.include.unwrap_or_default(), edit.exclude.unwrap_or_default());
    let added = state
        .app
        .db
        .add_feed(&NewFeed {
            guild,
            kind: kind.key(),
            source: &found.source,
            title: found.title.as_deref(),
            channel: edit.channel.unwrap_or_default(),
            message: edit.message.flatten().as_deref(),
            embed: edit.embed.unwrap_or(kind != Kind::YouTube),
            ping_role: edit.ping_role.flatten(),
            include: &include,
            exclude: &exclude,
            created_by: managed.current.session.user_id,
            seen: &seen,
            interval_secs: found.interval_secs,
        })
        .await?;
    let feed = match added {
        FeedAdd::Added(feed) => feed,
        FeedAdd::Full => {
            return Err(ApiError::limit(format!(
                "A server follows at most {MAX_FEEDS} feeds. Remove one first."
            )));
        }
        FeedAdd::Duplicate => {
            return Err(ApiError::invalid(vec![FieldError::new(
                "channel_id",
                "This source is already posted in that channel.",
            )]));
        }
    };
    if edit.enabled == Some(false) {
        let off = FeedEdit {
            enabled: Some(false),
            ..FeedEdit::default()
        };
        state.app.db.edit_feed(guild, feed.id, &off).await?;
    }
    let target = feed.id.to_string();
    audit::finish(
        &state,
        &managed.current.session,
        Change {
            guild,
            action: "feeds.add",
            target: Some(&target),
            before: Value::Null,
            after: audit_json(&feed),
        },
        None,
    )
    .await;
    let mut answer = view(&state, guild).await?;
    answer["created"] = json!(target);
    Ok(Json(answer))
}

/// `PUT /feeds/{id}`: changes a feed (its source and kind stay).
pub async fn replace(
    State(state): State<WebState>,
    managed: ManagedGuild,
    Path(params): Path<HashMap<String, String>>,
    ApiJson(body): ApiJson<FeedBody>,
) -> ApiResult<Json<Value>> {
    let (guild, id) = (managed.id, feed_id(&params)?);
    if body.kind.is_some() || body.source.is_some() {
        return Err(ApiError::malformed(
            "A feed's kind and source can't change: add a new feed instead.",
        ));
    }
    let info = guild_info::get(&state, guild, false).await?;
    let edit = checked_edit(&body, &info, guild).map_err(ApiError::invalid)?;
    match state.app.db.edit_feed(guild, id, &edit).await? {
        FeedChange::NotFound => Err(ApiError::not_found()),
        FeedChange::Duplicate => Err(ApiError::invalid(vec![FieldError::new(
            "channel_id",
            "This source is already posted in that channel.",
        )])),
        FeedChange::Changed { before, after } => {
            if before != after {
                let target = id.to_string();
                audit::finish(
                    &state,
                    &managed.current.session,
                    Change {
                        guild,
                        action: "feeds.edit",
                        target: Some(&target),
                        before: audit_json(&before),
                        after: audit_json(&after),
                    },
                    None,
                )
                .await;
            }
            Ok(Json(view(&state, guild).await?))
        }
    }
}

/// `DELETE /feeds/{id}`.
pub async fn delete(
    State(state): State<WebState>,
    managed: ManagedGuild,
    Path(params): Path<HashMap<String, String>>,
) -> ApiResult<Json<Value>> {
    let (guild, id) = (managed.id, feed_id(&params)?);
    if let Some(feed) = state.app.db.remove_feed(guild, id).await? {
        let target = id.to_string();
        audit::finish(
            &state,
            &managed.current.session,
            Change {
                guild,
                action: "feeds.remove",
                target: Some(&target),
                before: audit_json(&feed),
                after: Value::Null,
            },
            None,
        )
        .await;
    }
    Ok(Json(view(&state, guild).await?))
}

/// `POST /feeds/{id}/test`: posts the source's latest item in the feed's channel now, pinging nobody.
pub async fn test(
    State(state): State<WebState>,
    managed: ManagedGuild,
    Path(params): Path<HashMap<String, String>>,
) -> ApiResult<Json<Value>> {
    let (guild, id) = (managed.id, feed_id(&params)?);
    let feed = state.app.db.feed(guild, id).await?.ok_or_else(ApiError::not_found)?;
    let kind = kind_of(&feed.kind)?;
    if !feeds::spend_lookup(&state.app, guild).await {
        return Err(ApiError::too_many(60));
    }
    let read = feeds::fetch_source(kind, &feed.source, None, None)
        .await
        .map_err(|problem| {
            ApiError::blocked(t!(
                LANG,
                "feeds.test-cannot-read",
                why = error_text(LANG, &problem.code())
            ))
        })?
        .ok_or_else(|| ApiError::blocked(t!(LANG, "feeds.test-empty")))?;
    let item = read
        .parsed
        .items
        .first()
        .ok_or_else(|| ApiError::blocked(t!(LANG, "feeds.test-empty")))?;
    match feeds::send(&state.app, &feed, item, false).await {
        PostOutcome::Sent => Ok(Json(json!({
            "posted": true,
            "channel_id": feed.channel_id.to_string(),
            "item": item_json(item),
        }))),
        PostOutcome::Refused(code) | PostOutcome::Retry(code) => Err(ApiError::blocked(format!(
            "I couldn't post there: {}",
            error_text(LANG, &code)
        ))),
        PostOutcome::Dropped => Err(ApiError::not_found()),
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PreviewItem {
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    url: Option<String>,
    #[serde(default)]
    author: Option<String>,
    #[serde(default)]
    published: Option<DateTime<Utc>>,
    #[serde(default)]
    image: Option<String>,
    #[serde(default)]
    summary: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PreviewBody {
    kind: String,
    #[serde(default)]
    source: Option<String>,
    /// The source's name, as the check found it.
    #[serde(default)]
    account: Option<String>,
    #[serde(default)]
    message: Option<String>,
    #[serde(default)]
    embed: bool,
    #[serde(default)]
    ping_role_id: Option<String>,
    /// The source's latest post, as the check found it; a made-up one otherwise.
    #[serde(default)]
    item: Option<PreviewItem>,
}

/// `POST /feeds/preview`: the post a feed would make, drawn by the bot's own function (nothing is fetched,
/// sent or saved).
pub async fn preview(
    State(state): State<WebState>,
    managed: ManagedGuild,
    ApiJson(body): ApiJson<PreviewBody>,
) -> ApiResult<Json<Value>> {
    let kind = kind_of(&body.kind)?;
    let message = check_message(body.message.as_deref())
        .map_err(|e| ApiError::invalid(vec![FieldError::new("message", t!(LANG, e.key(), max = MESSAGE_MAX))]))?;
    let now = state.app.now();
    let real = body.item.is_some();
    let item = match body.item {
        Some(item) => Item {
            id: "preview".to_owned(),
            title: item.title.unwrap_or_default(),
            url: item.url,
            author: item.author,
            summary: item.summary,
            published: item.published,
            image: item.image,
            audio: None,
            duration_secs: None,
        },
        None => feeds::sample_item(LANG, kind, now),
    };
    let ping_role = body.ping_role_id.as_deref().and_then(parse_id);
    // Before a source is checked, the preview names a made-up one.
    let source = body
        .source
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| "example".to_owned());
    let account = body
        .account
        .filter(|a| !a.trim().is_empty())
        .unwrap_or_else(|| t!(LANG, "feeds.sample.account"));
    let draft = Draft {
        kind,
        source: &source,
        display_name: Some(&account),
        message: message.as_deref(),
        embed: body.embed,
        ping_role,
    };
    let rendered = render(LANG, &draft, &item);
    let info = guild_info::get(&state, managed.id, false).await.ok();
    let mut notes = Vec::new();
    if !real {
        notes.push("The post is made up; once the source is checked, its latest post is shown.".to_owned());
    }
    if let Some(role) = ping_role {
        notes.push(match info.as_ref().and_then(|i| i.role(role)) {
            Some(r) => format!("Pings @{} with every new post (a test post pings nobody).", r.name),
            None => "Pings the role with every new post (a test post pings nobody).".to_owned(),
        });
    }
    if body.embed {
        notes.push("With the card on, Discord shows no link preview of its own.".to_owned());
    }
    let readable = |text: &str| readable(text, info.as_ref());
    let embed = rendered.embed.map(|mut embed| {
        embed.description = embed.description.as_deref().map(readable);
        embed.title = embed.title.as_deref().map(readable);
        embed
    });
    Ok(Json(json!({
        "text": readable(&rendered.content),
        "embed": embed,
        "ephemeral": false,
        "notes": notes,
    })))
}

/// A message as a person reads it: role mentions as `@name`, timestamps as a date.
fn readable(text: &str, info: Option<&GuildInfo>) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find('<') {
        out.push_str(&rest[..start]);
        let here = &rest[start..];
        let Some(end) = here.find('>') else {
            out.push_str(here);
            return out;
        };
        let inner = &here[1..end];
        let shown = if let Some(role) = inner.strip_prefix("@&").and_then(|r| r.parse::<u64>().ok()) {
            Some(format!(
                "@{}",
                info.and_then(|i| i.role(role)).map_or("role", |r| r.name.as_str())
            ))
        } else if let Some(stamp) = inner.strip_prefix("t:") {
            stamp
                .split(':')
                .next()
                .and_then(|secs| secs.parse::<i64>().ok())
                .and_then(|secs| DateTime::from_timestamp(secs, 0))
                .map(|at| at.format("%-d %b %Y, %H:%M UTC").to_string())
        } else if inner.starts_with("http://") || inner.starts_with("https://") {
            Some(inner.to_owned())
        } else {
            None
        };
        match shown {
            Some(shown) => {
                out.push_str(&shown);
                rest = &here[end + 1..];
            }
            None => {
                out.push('<');
                rest = &here[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn previews_read_like_discord_shows_them() {
        assert_eq!(
            readable("<@&5> New: <https://x.test/a> at <t:0:f> <b>", None),
            "@role New: https://x.test/a at 1 Jan 1970, 00:00 UTC <b>"
        );
    }
}
