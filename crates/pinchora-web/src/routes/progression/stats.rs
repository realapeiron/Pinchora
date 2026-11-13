//! Server stats on the dashboard (`/api/guilds/{g}/stats/…`): the numbers of a period, the counter
//! channels, the public switch and the `/stats` card; and on the public site (`/api/public/stats…`):
//! the whole bot's totals and the page of a server that opened its stats to everyone.

use std::{
    collections::{HashMap, HashSet},
    time::Duration,
};

use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::{HeaderValue, header},
    response::{IntoResponse, Response},
    routing::{delete, get, post, put},
};
use chrono::{Duration as Span, NaiveDate, NaiveTime};
use pinchora_cache::Invalidation;
use pinchora_commands::{
    stats_card_png, stats_counter_problem, stats_counter_value, stats_missing_permission, stats_refresh_counters,
};
use pinchora_core::admin::modules::Module;
use pinchora_core::admin::server_stats;
use pinchora_core::admin::server_stats::CounterKind;
use pinchora_core::admin::server_stats::DayTotals;
use pinchora_core::admin::server_stats::MAX_COUNTERS;
use pinchora_core::admin::server_stats::Period;
use pinchora_core::admin::server_stats::TEMPLATE_MAX;
use pinchora_core::admin::server_stats::check_template;
use pinchora_core::admin::server_stats::counter_name;
use pinchora_core::admin::settings::FieldError;
use pinchora_core::infra::i18n::Lang;
use pinchora_core::t;
use pinchora_db::{CounterRow, CounterSave};
use serde::Deserialize;
use serde_json::{Value, json};
use twilight_model::{
    channel::{ChannelType, permission_overwrite::PermissionOverwriteType},
    guild::Permissions,
    id::Id,
};

use crate::audit;
use crate::audit::Change;
use crate::error::ApiError;
use crate::error::ApiJson;
use crate::error::ApiResult;
use crate::extract::ManagedGuild;
use crate::guild_info;
use crate::ratelimit;
use crate::routes::admin::settings::parse_id;
use crate::routes::infra::me::icon_url;
use crate::state::WebState;

/// Card renders per session per minute.
const RENDERS_PER_MINUTE: i64 = 20;
/// How long the public pages' numbers are kept: a crawler or a busy link costs one count a while.
const PUBLIC_TTL: Duration = Duration::from_secs(300);
/// Rows in the top lists.
const TOP: i64 = 8;
/// Channel types a counter may rename: text, voice, category, announcement, stage.
const COUNTER_CHANNELS: [u8; 5] = [0, 2, 4, 5, 13];

pub fn router() -> Router<WebState> {
    Router::new()
        .route("/", get(summary))
        .route("/settings", put(put_settings))
        .route("/card", get(card))
        .route("/counters", get(counters).post(add_counter))
        .route("/counters/preview", post(preview_counter))
        .route("/counters/{channel}", delete(delete_counter))
}

/// The public site's routes, under `/api/public/stats`.
pub fn public_router() -> Router<WebState> {
    Router::new().route("/", get(global)).route("/{g}", get(public_guild))
}

#[derive(Debug, Deserialize)]
pub struct PeriodQuery {
    days: Option<i64>,
}

fn period_of(query: &PeriodQuery, lang: Lang) -> ApiResult<Period> {
    match query.days {
        None => Ok(Period::default()),
        Some(days) => {
            Period::of_days(days).ok_or_else(|| ApiError::malformed(t!(lang, "stats.web.errors.days-is-7-30-or-90")))
        }
    }
}

fn day_json(d: &DayTotals) -> Value {
    json!({
        "day": d.day,
        "messages": d.messages,
        "voice_minutes": d.voice_minutes,
        "joins": d.joins,
        "leaves": d.leaves,
        "commands": d.commands,
    })
}

fn totals_json(d: &DayTotals, active: Option<i64>) -> Value {
    json!({
        "messages": d.messages,
        "voice_minutes": d.voice_minutes,
        "joins": d.joins,
        "leaves": d.leaves,
        "commands": d.commands,
        "active_members": active,
    })
}

fn midnight(day: NaiveDate) -> chrono::DateTime<chrono::Utc> {
    day.and_time(NaiveTime::MIN).and_utc()
}

/// What a server did over `period`, as both pages show it. `members` adds the busiest members (the
/// dashboard only: the public page names no one).
async fn period_json(
    state: &WebState,
    guild: u64,
    period: Period,
    members: bool,
    public_channels: Option<&HashSet<u64>>,
) -> ApiResult<Value> {
    let db = &state.app.db;
    let today = state.app.now().date_naive();
    let first = period.first_day(today);
    // The period before, for the change: kept only while the 90 days reach that far back.
    let before = (period != Period::Quarter).then(|| first - Span::days(period.days()));
    let hours = db.guild_hours(guild, midnight(before.unwrap_or(first))).await?;
    let (earlier, current): (Vec<_>, Vec<_>) = hours.into_iter().partition(|h| h.hour < midnight(first));
    let mut days = server_stats::daily(&current, first, period.days());
    if let Some(visible) = public_channels {
        let ids: Vec<i64> = visible.iter().map(|id| *id as i64).collect();
        let safe = db.public_visible_channel_days(guild, first, &ids).await?;
        let by_day: HashMap<_, _> = safe.into_iter().map(|d| (d.day, d)).collect();
        for day in &mut days {
            let counts = by_day.get(&day.day);
            day.messages = counts.map_or(0, |d| d.messages);
            day.voice_minutes = counts.map_or(0, |d| d.voice_minutes);
        }
    }
    let totals = server_stats::totals(&days);
    let previous = if public_channels.is_some() {
        None
    } else {
        before.map(|b| server_stats::totals(&server_stats::daily(&earlier, b, period.days())))
    };
    let active = if public_channels.is_some() {
        None
    } else {
        Some(db.active_members(guild, first).await?)
    };
    let channels = if public_channels.is_some() {
        db.public_top_channels(guild, first, 100_000).await?
    } else {
        db.top_channels(guild, first, TOP).await?
    };
    // Names as the server has them now; one that is gone keeps its id.
    let info = guild_info::get(state, guild, false).await.ok();
    let mut top_channels: Vec<Value> = channels
        .iter()
        .filter(|c| public_channels.is_none_or(|visible| visible.contains(&(c.id as u64))))
        .take(TOP as usize)
        .map(|c| {
            let name = info
                .as_ref()
                .and_then(|i| i.channel(c.id as u64))
                .map(|ch| ch.name.clone());
            json!({
                "id": c.id.to_string(),
                "name": name,
                "messages": c.messages,
                "voice_minutes": c.voice_minutes,
            })
        })
        .collect();
    if let Some(visible) = public_channels {
        let all = db.top_channels(guild, first, 100_000).await?;
        let (all_messages, all_voice) = all
            .iter()
            .fold((0_i64, 0_i64), |(m, v), c| (m + c.messages, v + c.voice_minutes));
        let (safe_messages, safe_voice) = channels
            .iter()
            .filter(|c| visible.contains(&(c.id as u64)))
            .fold((0_i64, 0_i64), |(m, v), c| (m + c.messages, v + c.voice_minutes));
        let (messages, voice) = ((all_messages - safe_messages).max(0), (all_voice - safe_voice).max(0));
        if messages > 0 || voice > 0 {
            top_channels.push(json!({"id": null, "name": "Other", "messages": messages, "voice_minutes": voice}));
        }
    }
    let mut out = json!({
        "days": period.days(),
        "from": first,
        "to": today,
        "totals": totals_json(&totals, active),
        "previous": previous.as_ref().map(|p| totals_json(p, None)),
        "daily": days.iter().map(day_json).collect::<Vec<_>>(),
        "heatmap": if public_channels.is_some() { [[0_i64; 24]; 7] } else { server_stats::heatmap(&current) },
        "top_channels": top_channels,
    });
    if members {
        let rows = db.top_members(guild, first, TOP).await?;
        let mut top = Vec::with_capacity(rows.len());
        for m in rows {
            let user = state.app.user(m.id as u64).await;
            top.push(json!({
                "id": m.id.to_string(),
                "name": user.as_ref().map(|u| u.global_name.clone().unwrap_or_else(|| u.name.clone())),
                "avatar_url": user.as_ref().map(|u| match u.avatar {
                    Some(hash) => format!("https://cdn.discordapp.com/avatars/{}/{hash}.png?size=64", u.id),
                    None => format!("https://cdn.discordapp.com/embed/avatars/{}.png", (u.id.get() >> 22) % 6),
                }),
                "messages": m.messages,
                "voice_minutes": m.voice_minutes,
            }));
        }
        out["top_members"] = Value::Array(top);
    }
    Ok(out)
}

/// `GET /stats?days=7|30|90`: the period's numbers, whether the page is public, and what counts.
pub async fn summary(
    State(state): State<WebState>,
    managed: ManagedGuild,
    Query(query): Query<PeriodQuery>,
) -> ApiResult<Json<Value>> {
    let lang = managed.panel_lang;
    let period = period_of(&query, lang)?;
    let guild = managed.id;
    let mut out = period_json(&state, guild, period, true, None).await?;
    let settings = state.app.db.guild_settings(guild).await?;
    out["public"] = json!(state.app.db.stats_public(guild).await?);
    out["voice_counted"] = json!(settings.voice_xp_enabled && settings.leveling_enabled);
    out["module_on"] = json!(state.app.module_allowed(guild, None, Module::Stats).await);
    Ok(Json(out))
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SettingsBody {
    public: bool,
}

/// `PUT /stats/settings`: open or close the public stats page.
pub async fn put_settings(
    State(state): State<WebState>,
    managed: ManagedGuild,
    ApiJson(body): ApiJson<SettingsBody>,
) -> ApiResult<Json<Value>> {
    let guild = managed.id;
    if state
        .app
        .db
        .set_stats_public_audited(
            guild,
            body.public,
            managed.current.session.user_id,
            managed.current.session.display_name(),
        )
        .await?
        && let Err(error) = state.app.cache.forget(&public_key(guild)).await
    {
        tracing::warn!(%error, guild, "public stats cache not cleared");
    }
    Ok(Json(json!({ "public": body.public })))
}

/// `GET /stats/card?days=`: the `/stats server` card as the bot draws it.
pub async fn card(
    State(state): State<WebState>,
    managed: ManagedGuild,
    Query(query): Query<PeriodQuery>,
) -> ApiResult<Response> {
    let lang = managed.panel_lang;
    let period = period_of(&query, lang)?;
    let session = &managed.current.session;
    ratelimit::check(
        &state.app.cache,
        "stats-card",
        &session.user_id.to_string(),
        RENDERS_PER_MINUTE,
        Duration::from_secs(60),
    )
    .await?;
    let png = stats_card_png(&state.app, managed.id, period.days()).await?;
    let mut response = png.into_response();
    let headers = response.headers_mut();
    headers.insert(header::CONTENT_TYPE, HeaderValue::from_static("image/png"));
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("private, no-store"));
    Ok(response)
}

fn counter_json(row: &CounterRow, info: Option<&guild_info::GuildInfo>) -> Value {
    let channel = row.channel_id as u64;
    json!({
        "channel_id": channel.to_string(),
        "channel_name": info.and_then(|i| i.channel(channel)).map(|c| c.name.clone()),
        "kind": row.kind,
        "role_id": row.role_id.map(|r| r.to_string()),
        "template": row.template,
        "value": row.last_value,
        "name": row.last_name,
        "renamed_at": row.last_rename_at,
        "problem": row.problem.as_deref().map(|key| stats_counter_problem(key, Lang::En)),
    })
}

async fn counters_json(state: &WebState, guild: u64) -> ApiResult<Value> {
    let rows = state.app.db.counters(guild).await?;
    let info = guild_info::get(state, guild, false).await.ok();
    Ok(json!({
        "counters": rows.iter().map(|r| counter_json(r, info.as_ref())).collect::<Vec<_>>(),
        "max": MAX_COUNTERS,
        "template_max": TEMPLATE_MAX,
        "kinds": CounterKind::ALL.iter().map(|k| json!({
            "key": k.key(),
            "label": k.label(),
            "needs_role": k.needs_role(),
            "template": k.default_template(Lang::En),
        })).collect::<Vec<_>>(),
    }))
}

/// `GET /stats/counters`: the counter channels, the limit, and the kinds a counter can be.
pub async fn counters(State(state): State<WebState>, managed: ManagedGuild) -> ApiResult<Json<Value>> {
    Ok(Json(counters_json(&state, managed.id).await?))
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CounterBody {
    channel_id: Option<String>,
    kind: String,
    role_id: Option<String>,
    /// Missing or `null`: the kind's own, in the server's language.
    template: Option<String>,
}

/// The kind, the role it counts and the template a body asks for, or what is wrong with them.
async fn counter_fields(
    state: &WebState,
    guild: u64,
    body: &CounterBody,
    info: &guild_info::GuildInfo,
) -> Result<(CounterKind, Option<u64>, String), Vec<FieldError>> {
    let mut errors = Vec::new();
    let kind = CounterKind::parse(&body.kind);
    if kind.is_none() {
        errors.push(FieldError::new("kind", "Pick what the counter counts."));
    }
    let role = match (kind.is_some_and(CounterKind::needs_role), body.role_id.as_deref()) {
        (true, Some(raw)) => match parse_id(raw).filter(|id| info.role(*id).is_some()) {
            Some(id) => Some(id),
            None => {
                errors.push(FieldError::new("role_id", "That role isn't in this server any more."));
                None
            }
        },
        (true, None) => {
            errors.push(FieldError::new("role_id", "Pick the role to count."));
            None
        }
        (false, _) => None,
    };
    let template = match (body.template.as_deref().filter(|t| !t.trim().is_empty()), kind) {
        (Some(raw), _) => check_template(raw).unwrap_or_else(|error| {
            errors.push(error);
            String::new()
        }),
        (None, Some(kind)) => kind.default_template(state.app.guild_lang(guild).await),
        (None, None) => String::new(),
    };
    match (kind, errors.is_empty()) {
        (Some(kind), true) => Ok((kind, role, template)),
        _ => Err(errors),
    }
}

/// `POST /stats/counters`: makes a channel a counter (or changes the one it is), renames it at once.
pub async fn add_counter(
    State(state): State<WebState>,
    managed: ManagedGuild,
    ApiJson(body): ApiJson<CounterBody>,
) -> ApiResult<Json<Value>> {
    let lang = managed.panel_lang;
    let guild = managed.id;
    let channel = body
        .channel_id
        .as_deref()
        .and_then(parse_id)
        .ok_or_else(|| ApiError::invalid(vec![FieldError::new("channel_id", "Pick a channel.")]))?;
    let info = guild_info::get_with(&state, guild, |info| info.channel(channel).is_some()).await?;
    let found = info.check_channel("channel_id", channel)?;
    if !COUNTER_CHANNELS.contains(&found.kind) {
        return Err(ApiError::invalid(vec![FieldError::new(
            "channel_id",
            "A counter is a voice, text or announcement channel, or a category.",
        )]));
    }
    let kind_of_channel = ChannelType::from(found.kind);
    let channel_name = found.name.clone();
    let (kind, role, template) = counter_fields(&state, guild, &body, &info)
        .await
        .map_err(ApiError::invalid)?;
    if let Some(missing) = stats_missing_permission(&state.app, guild, channel, kind_of_channel).await {
        let permission = match missing {
            "view_channel" => "View Channel",
            "connect" => "Connect",
            _ => "Manage Channels",
        };
        return Err(ApiError::invalid(vec![FieldError::new(
            "channel_id",
            format!("Pinchora needs the {permission} permission in #{channel_name} to rename it."),
        )]));
    }
    let before = state
        .app
        .db
        .counters(guild)
        .await?
        .into_iter()
        .find(|r| r.channel_id == channel as i64);
    let row = match state
        .app
        .db
        .save_counter(guild, channel, kind.key(), role, &template)
        .await?
    {
        CounterSave::Full => {
            return Err(ApiError::limit(t!(
                lang,
                "stats.web.errors.counter-channels-limit",
                max = MAX_COUNTERS
            )));
        }
        CounterSave::Saved(row) => row,
    };
    let shape = |r: &CounterRow| json!({ "kind": r.kind, "role_id": r.role_id.map(|id| id.to_string()), "template": r.template });
    let target = channel.to_string();
    audit::finish(
        &state,
        &managed.current.session,
        Change {
            guild,
            action: if before.is_some() {
                "counters.edit"
            } else {
                "counters.add"
            },
            target: Some(&target),
            before: before.as_ref().map_or(Value::Null, shape),
            after: shape(&row),
        },
        Some(Invalidation::Guild(guild)),
    )
    .await;
    // The first rename goes out now; the page shows the new name on its next read.
    let app = state.app.clone();
    tokio::spawn(async move {
        if let Err(error) = stats_refresh_counters(&app, guild).await {
            tracing::warn!(%error, guild, "counters not refreshed after a dashboard change");
        }
    });
    Ok(Json(counters_json(&state, guild).await?))
}

/// `POST /stats/counters/preview`: the name a counter would have now. Nothing is saved.
pub async fn preview_counter(
    State(state): State<WebState>,
    managed: ManagedGuild,
    ApiJson(body): ApiJson<CounterBody>,
) -> ApiResult<Json<Value>> {
    let session = &managed.current.session;
    ratelimit::check(
        &state.app.cache,
        "counter-preview",
        &session.user_id.to_string(),
        RENDERS_PER_MINUTE,
        Duration::from_secs(60),
    )
    .await?;
    let info = guild_info::get(&state, managed.id, false).await?;
    let (kind, role, template) = counter_fields(&state, managed.id, &body, &info)
        .await
        .map_err(ApiError::invalid)?;
    Ok(Json(
        match stats_counter_value(&state.app, managed.id, kind, role).await {
            Ok(value) => json!({ "name": counter_name(&template, value), "value": value, "problem": null }),
            Err(Some(key)) => json!({ "name": null, "value": null, "problem": stats_counter_problem(key, Lang::En) }),
            Err(None) => json!({
                "name": counter_name(&template, 0).replace('0', "…"),
                "value": null,
                "problem": "Discord didn't say the number just now; the channel shows it within a few minutes.",
            }),
        },
    ))
}

/// `DELETE /stats/counters/{channel}`: the channel stops being a counter and keeps its last name.
pub async fn delete_counter(
    State(state): State<WebState>,
    managed: ManagedGuild,
    Path(params): Path<HashMap<String, String>>,
) -> ApiResult<Json<Value>> {
    let lang = managed.panel_lang;
    let guild = managed.id;
    let channel = params
        .get("channel")
        .and_then(|raw| parse_id(raw))
        .ok_or_else(|| ApiError::malformed(t!(lang, "common.web.errors.channel-ids")))?;
    let before = state
        .app
        .db
        .counters(guild)
        .await?
        .into_iter()
        .find(|r| r.channel_id == channel as i64)
        .ok_or_else(ApiError::not_found)?;
    state.app.db.remove_counter(guild, channel).await?;
    let target = channel.to_string();
    audit::finish(
        &state,
        &managed.current.session,
        Change {
            guild,
            action: "counters.remove",
            target: Some(&target),
            before: json!({ "kind": before.kind, "template": before.template }),
            after: Value::Null,
        },
        Some(Invalidation::Guild(guild)),
    )
    .await;
    Ok(Json(counters_json(&state, guild).await?))
}

fn global_key() -> String {
    "web:stats:global".to_owned()
}

fn public_key(guild: u64) -> String {
    format!("web:stats:public:v2:{guild}")
}

fn everyone_can_view(
    base: Permissions,
    guild: u64,
    overwrites: &[twilight_model::channel::permission_overwrite::PermissionOverwrite],
) -> bool {
    if base.contains(Permissions::ADMINISTRATOR) {
        return true;
    }
    let here = overwrites
        .iter()
        .find(|o| o.kind == PermissionOverwriteType::Role && o.id.get() == guild);
    let effective = here.map_or(base, |o| (base - o.deny) | o.allow);
    effective.contains(Permissions::VIEW_CHANNEL)
}

async fn public_channels(state: &WebState, guild: u64) -> ApiResult<HashSet<u64>> {
    let id = Id::new(guild);
    let (channels, roles) = tokio::try_join!(
        async {
            Ok::<_, ApiError>(
                state
                    .app
                    .http
                    .guild_channels(id)
                    .await
                    .map_err(anyhow::Error::from)?
                    .models()
                    .await
                    .map_err(anyhow::Error::from)?,
            )
        },
        async {
            Ok::<_, ApiError>(
                state
                    .app
                    .http
                    .roles(id)
                    .await
                    .map_err(anyhow::Error::from)?
                    .models()
                    .await
                    .map_err(anyhow::Error::from)?,
            )
        },
    )?;
    let base = roles
        .iter()
        .find(|r| r.id.get() == guild)
        .map_or(Permissions::empty(), |r| r.permissions);
    Ok(channels
        .iter()
        .filter(|ch| {
            !ch.kind.is_thread() && everyone_can_view(base, guild, ch.permission_overwrites.as_deref().unwrap_or(&[]))
        })
        .map(|ch| ch.id.get())
        .collect())
}

async fn cached(state: &WebState, key: &str) -> Option<Value> {
    let text = state.app.cache.get(key).await.ok().flatten()?;
    serde_json::from_str(&text).ok()
}

async fn keep(state: &WebState, key: &str, value: &Value) {
    if let Err(error) = state.app.cache.put(key, &value.to_string(), PUBLIC_TTL).await {
        tracing::debug!(%error, "public stats not cached");
    }
}

/// `GET /api/public/stats`: the whole bot's totals now and a day by day history (servers, players,
/// messages). No login; kept for five minutes.
pub async fn global(State(state): State<WebState>) -> ApiResult<Json<Value>> {
    if let Some(known) = cached(&state, &global_key()).await {
        return Ok(Json(known));
    }
    let db = &state.app.db;
    let totals = db.global_totals().await?;
    let today = state.app.now().date_naive();
    let days = db.global_days(today - Span::days(29)).await?;
    let value = json!({
        "totals": {
            "servers": totals.servers,
            "players": totals.players,
            "houses": totals.houses,
            "pets": totals.pets,
            "fish": totals.fish,
            "games": totals.games,
        },
        "history": days.iter().map(|d| json!({
            "day": d.day,
            "servers": d.servers,
            "players": d.players,
            "messages": d.messages,
        })).collect::<Vec<_>>(),
    });
    keep(&state, &global_key(), &value).await;
    Ok(Json(value))
}

/// `GET /api/public/stats/{g}`: a server's last 30 days, when it opened its stats to everyone (else
/// 404, the same as a server Pinchora has never heard of). No member is named.
pub async fn public_guild(State(state): State<WebState>, Path(raw): Path<String>) -> ApiResult<Json<Value>> {
    let guild = parse_id(&raw).ok_or_else(ApiError::not_found)?;
    if !state.app.db.stats_public(guild).await? || !state.app.module_allowed(guild, None, Module::Stats).await {
        return Err(ApiError::not_found());
    }
    let visible = public_channels(&state, guild).await?;
    let mut value = period_json(&state, guild, Period::Month, false, Some(&visible)).await?;
    let found = state
        .app
        .http
        .guild(Id::new(guild))
        .await
        .map_err(anyhow::Error::from)?
        .model()
        .await
        .map_err(anyhow::Error::from)?;
    value["guild"] = json!({
        "id": guild.to_string(),
        "name": found.name,
        "icon_url": icon_url(guild, found.icon.map(|i| i.to_string()).as_deref()),
    });
    if !state.app.db.stats_public(guild).await? || !state.app.module_allowed(guild, None, Module::Stats).await {
        return Err(ApiError::not_found());
    }
    if public_channels(&state, guild).await? != visible {
        return Err(ApiError::not_found());
    }
    Ok(Json(value))
}

#[cfg(test)]
mod tests {
    use crate::routes::progression::stats::*;
    use twilight_model::channel::permission_overwrite::PermissionOverwrite;

    #[test]
    fn everyone_denial_hides_a_channel_even_when_the_bot_can_read_it() {
        let guild = 700;
        let denied = PermissionOverwrite {
            allow: Permissions::empty(),
            deny: Permissions::VIEW_CHANNEL,
            id: Id::new(guild),
            kind: PermissionOverwriteType::Role,
        };
        assert!(!everyone_can_view(Permissions::VIEW_CHANNEL, guild, &[denied]));
        assert!(everyone_can_view(Permissions::VIEW_CHANNEL, guild, &[]));
        assert!(!everyone_can_view(Permissions::empty(), guild, &[]));
    }
}
