//! Invite settings, rewards, leaderboard and CSV for server managers.

use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::{StatusCode, header},
    response::IntoResponse,
    routing::{get, put},
};
use chrono::{Duration, Utc};
use pinchora_core::{
    i18n::Lang,
    settings::{self, FieldError, RoleFacts},
    t,
};
use pinchora_db::{InviteScore, InviteSettings};
use serde::Deserialize;
use serde_json::{Value, json};

use super::settings::parse_id;
use crate::{
    error::{ApiError, ApiResult},
    extract::ManagedGuild,
    guild_info,
    state::WebState,
};

pub fn router() -> Router<WebState> {
    Router::new()
        .route("/", get(overview))
        .route("/settings", put(save_settings))
        .route("/rewards/{count}", put(save_reward).delete(delete_reward))
        .route("/leaders", get(leaders))
        .route("/member/{id}", get(member))
        .route("/export.csv", get(export))
}

fn id(id: Option<i64>) -> Option<String> {
    id.map(|n| n.to_string())
}

fn settings_json(s: &InviteSettings) -> Value {
    json!({"enabled":s.enabled,"log_channel_id":id(s.log_channel_id),"fake_days":s.fake_days,
        "temp_enabled":s.temp_enabled,"temp_channel_id":id(s.temp_channel_id),
        "temp_max_age_secs":s.temp_max_age_secs,"temp_max_uses":s.temp_max_uses,
        "temp_per_user_daily":s.temp_per_user_daily,"temp_message_id":id(s.temp_message_id)})
}

fn score_json(s: &InviteScore) -> Value {
    json!({"user_id":s.user_id.to_string(),"regular":s.regular,"left":s.left,"fake":s.fake,"bonus":s.bonus,"total":s.total})
}

async fn overview(State(state): State<WebState>, managed: ManagedGuild) -> ApiResult<Json<Value>> {
    let settings = state.app.db.invite_settings(managed.id).await?;
    let can_read_invites = state
        .app
        .http
        .guild_invites(twilight_model::id::Id::new(managed.id))
        .await
        .is_ok();
    let rewards = state.app.db.invite_rewards(managed.id).await?;
    Ok(Json(
        json!({"settings":settings_json(&settings),"rewards":rewards.iter().map(|(count,role)| json!({"invites":count,"role_id":role.to_string()})).collect::<Vec<_>>(),"can_read_invites":can_read_invites,"limits":{"rewards":20,"fake_days":365,"temp_daily":20}}),
    ))
}

#[derive(Deserialize)]
struct SettingsBody {
    enabled: bool,
    log_channel_id: Option<String>,
    fake_days: i32,
    temp_enabled: bool,
    temp_channel_id: Option<String>,
    temp_max_age_secs: i32,
    temp_max_uses: i32,
    temp_per_user_daily: i32,
}

fn field(name: &'static str, text: impl Into<String>) -> ApiError {
    ApiError::invalid(vec![FieldError::new(name, text)])
}

async fn save_settings(
    State(state): State<WebState>,
    managed: ManagedGuild,
    Json(body): Json<SettingsBody>,
) -> ApiResult<Json<Value>> {
    if !(0..=365).contains(&body.fake_days) {
        return Err(field("fake_days", t!(Lang::En, "invites.days-range")));
    }
    if !(1..=20).contains(&body.temp_per_user_daily) {
        return Err(field("temp_per_user_daily", t!(Lang::En, "invites.quota-range")));
    }
    if !(60..=604800).contains(&body.temp_max_age_secs) {
        return Err(field("temp_max_age_secs", t!(Lang::En, "invites.web.age-range")));
    }
    if !(1..=100).contains(&body.temp_max_uses) {
        return Err(field("temp_max_uses", t!(Lang::En, "invites.web.uses-range")));
    }
    let parse = |key: &'static str, raw: &Option<String>| -> ApiResult<Option<i64>> {
        raw.as_ref()
            .map(|s| {
                parse_id(s)
                    .map(|id| id as i64)
                    .ok_or_else(|| field(key, t!(Lang::En, "invites.web.choose-channel")))
            })
            .transpose()
    };
    let log = parse("log_channel_id", &body.log_channel_id)?;
    let temp = parse("temp_channel_id", &body.temp_channel_id)?;
    let info = guild_info::get(&state, managed.id, true).await?;
    for (key, channel) in [("log_channel_id", log), ("temp_channel_id", temp)] {
        if let Some(channel) = channel
            && !info
                .channels
                .iter()
                .any(|c| c.id == channel.to_string() && c.takes_posts())
        {
            return Err(field(key, t!(Lang::En, "invites.web.visible-channel")));
        }
    }
    let mut saved = state.app.db.invite_settings(managed.id).await?;
    saved.enabled = body.enabled;
    saved.log_channel_id = log;
    saved.fake_days = body.fake_days;
    saved.temp_enabled = body.temp_enabled;
    saved.temp_channel_id = temp;
    saved.temp_max_age_secs = body.temp_max_age_secs;
    saved.temp_max_uses = body.temp_max_uses;
    saved.temp_per_user_daily = body.temp_per_user_daily;
    state.app.db.save_invite_settings(managed.id, &saved).await?;
    Ok(Json(settings_json(&saved)))
}

#[derive(Deserialize)]
struct RewardBody {
    role_id: String,
}

async fn save_reward(
    State(state): State<WebState>,
    managed: ManagedGuild,
    Path((_, count)): Path<(String, i32)>,
    Json(body): Json<RewardBody>,
) -> ApiResult<Json<Value>> {
    if !(1..=1_000_000).contains(&count) {
        return Err(field("invites", t!(Lang::En, "invites.reward-range")));
    }
    let role_id = parse_id(&body.role_id).ok_or_else(|| field("role_id", t!(Lang::En, "invites.web.choose-role")))?;
    let info = guild_info::get(&state, managed.id, true).await?;
    if !info.bot.manage_roles {
        return Err(field("role_id", t!(Lang::En, "invites.web.manage-roles")));
    }
    let role = info
        .roles
        .iter()
        .find(|r| r.id == body.role_id)
        .ok_or_else(|| field("role_id", t!(Lang::En, "invites.web.role-gone")))?;
    let facts = RoleFacts {
        everyone: role.everyone,
        managed: role.managed,
        position: role.position,
        permissions: role.permissions.parse().unwrap_or(u64::MAX),
    };
    if let Some(why) = settings::role_refusal(facts, info.bot.top_role_position, &format!("@{}", role.name)) {
        return Err(field("role_id", why));
    }
    let grantor = guild_info::grantor(&state, &managed, &info.roles, true).await?;
    if !grantor.owner
        && let Some(why) = settings::grant_refusal(facts, grantor, &format!("@{}", role.name))
    {
        return Err(field("role_id", why));
    }
    let rewards = state.app.db.invite_rewards(managed.id).await?;
    if rewards.len() >= 20 && !rewards.iter().any(|(n, _)| *n == count) {
        return Err(ApiError::limit(t!(Lang::En, "invites.web.rewards-limit")));
    }
    state.app.db.put_invite_reward(managed.id, count, role_id).await?;
    Ok(Json(json!({"invites":count,"role_id":body.role_id})))
}

async fn delete_reward(
    State(state): State<WebState>,
    managed: ManagedGuild,
    Path((_, count)): Path<(String, i32)>,
) -> ApiResult<StatusCode> {
    state.app.db.remove_invite_reward(managed.id, count).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
struct LeaderQuery {
    period: Option<String>,
    page: Option<u32>,
}

fn since(period: Option<&str>) -> Option<chrono::DateTime<Utc>> {
    match period {
        Some("week") => Some(Utc::now() - Duration::days(7)),
        Some("month") => Some(Utc::now() - Duration::days(30)),
        _ => None,
    }
}

async fn leaders(
    State(state): State<WebState>,
    managed: ManagedGuild,
    Query(query): Query<LeaderQuery>,
) -> ApiResult<Json<Value>> {
    let page = query.page.unwrap_or(0).min(999);
    let rows = state
        .app
        .db
        .invite_leaders(managed.id, since(query.period.as_deref()), 25, i64::from(page) * 25)
        .await?;
    Ok(Json(
        json!({"page":page,"rows":rows.iter().map(score_json).collect::<Vec<_>>()}),
    ))
}

async fn member(
    State(state): State<WebState>,
    managed: ManagedGuild,
    Path((_, id)): Path<(String, String)>,
) -> ApiResult<Json<Value>> {
    let user = parse_id(&id).ok_or_else(|| ApiError::malformed(t!(Lang::En, "invites.web.user-id")))?;
    let score = state.app.db.invite_score(managed.id, user).await?;
    let joins = state.app.db.invite_joins(managed.id, user, 100).await?;
    Ok(Json(
        json!({"score":score_json(&score),"joins":joins.iter().map(|j| json!({"user_id":j.user_id.to_string(),"joined_at":j.joined_at,"left_at":j.left_at,"fake":j.fake,"source":j.source,"code":j.code})).collect::<Vec<_>>()}),
    ))
}

async fn export(State(state): State<WebState>, managed: ManagedGuild) -> ApiResult<impl IntoResponse> {
    let rows = state.app.db.invite_leaders(managed.id, None, 10_000, 0).await?;
    let mut csv = String::from("user_id,regular,left,fake,bonus,total\r\n");
    for r in rows {
        csv.push_str(&format!(
            "{},{},{},{},{},{}\r\n",
            r.user_id, r.regular, r.left, r.fake, r.bonus, r.total
        ));
    }
    Ok((
        [
            (header::CONTENT_TYPE, "text/csv; charset=utf-8"),
            (header::CONTENT_DISPOSITION, "attachment; filename=invites.csv"),
        ],
        csv,
    ))
}
