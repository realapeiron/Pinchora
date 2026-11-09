//! Achievement editor and private holder list for server managers.

use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    routing::{get, put},
};
use pinchora_core::admin::settings;
use pinchora_core::admin::settings::FieldError;
use pinchora_core::admin::settings::RoleFacts;
use pinchora_core::progression::achievements;
use pinchora_core::progression::achievements::Kind;
use pinchora_core::t;
use pinchora_db::{AchievementDraft, AchievementRow, AchievementSave};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::audit;
use crate::audit::Change;
use crate::error::ApiError;
use crate::error::ApiResult;
use crate::extract::ManagedGuild;
use crate::guild_info;
use crate::routes::admin::settings::parse_id;
use crate::state::WebState;

pub fn router() -> Router<WebState> {
    Router::new()
        .route("/", get(list).post(create))
        .route("/{id}", put(update).delete(remove))
        .route("/{id}/holders", get(holders))
}

fn row_json(row: &AchievementRow, module_off: bool) -> Value {
    json!({"id":row.id.to_string(),"name":row.name,"description":row.description,"icon":row.icon,
        "kind":row.kind,"target":row.target,"reward_role_id":row.reward_role_id.map(|v|v.to_string()),
        "reward_points":row.reward_points,"announce_channel_id":row.announce_channel_id.map(|v|v.to_string()),
        "secret":row.secret,"enabled":row.enabled,"module_off":module_off})
}

async fn list(State(state): State<WebState>, managed: ManagedGuild) -> ApiResult<Json<Value>> {
    let lang = state.app.guild_lang(managed.id).await;
    let mut items = Vec::new();
    for row in state.app.db.achievements(managed.id).await? {
        let module_off = match Kind::parse(&row.kind) {
            Some(kind) => !pinchora_commands::achievement_source_enabled(&state.app, managed.id, kind).await,
            None => false,
        };
        items.push(row_json(&row, module_off));
    }
    let strings: std::collections::BTreeMap<_, _> = pinchora_core::infra::i18n::keys(lang)
        .into_iter()
        .filter(|key| key.starts_with("achievements.ui."))
        .map(|key| (key.trim_start_matches("achievements.ui."), t!(lang, key)))
        .collect();
    Ok(Json(
        json!({"items":items,"max":achievements::MAX_ACHIEVEMENTS,"icons":achievements::ICONS,
        "kinds":Kind::ALL.iter().map(|k| k.key()).collect::<Vec<_>>(),"language":lang.code(),"strings":strings}),
    ))
}

#[derive(Deserialize)]
struct Body {
    name: String,
    description: String,
    icon: String,
    kind: String,
    target: i64,
    reward_role_id: Option<String>,
    reward_points: i64,
    announce_channel_id: Option<String>,
    secret: bool,
    enabled: bool,
}

async fn save(state: &WebState, managed: &ManagedGuild, id: Option<i64>, body: Body) -> ApiResult<Value> {
    let lang = state.app.guild_lang(managed.id).await;
    let invalid =
        |field: &'static str| ApiError::invalid(vec![FieldError::new(field, t!(lang, "achievements.web.invalid"))]);
    let kind = Kind::parse(&body.kind).ok_or_else(|| invalid("kind"))?;
    if !achievements::valid_name(&body.name) {
        return Err(invalid("name"));
    }
    if !achievements::valid_description(&body.description) {
        return Err(invalid("description"));
    }
    if !achievements::valid_icon(&body.icon)
        || body
            .icon
            .strip_prefix("b_")
            .is_some_and(|id| pinchora_game::Content::get().badge(id).is_none())
    {
        return Err(invalid("icon"));
    }
    if !(1..=1_000_000_000).contains(&body.target) {
        return Err(invalid("target"));
    }
    if !(0..=1_000_000_000).contains(&body.reward_points) {
        return Err(invalid("reward_points"));
    }
    let role = body.reward_role_id.as_deref().and_then(parse_id);
    if body.reward_role_id.is_some() && role.is_none() {
        return Err(invalid("reward_role_id"));
    }
    let channel = body.announce_channel_id.as_deref().and_then(parse_id);
    if body.announce_channel_id.is_some() && channel.is_none() {
        return Err(invalid("announce_channel_id"));
    }
    if let Some(channel) = channel {
        let info = guild_info::get(state, managed.id, true).await?;
        if !info
            .channels
            .iter()
            .any(|c| c.id == channel.to_string() && c.takes_posts())
        {
            return Err(invalid("announce_channel_id"));
        }
    }
    if let Some(role_id) = role {
        let info = guild_info::get(state, managed.id, true).await?;
        let row = info.roles.iter().find(|r| r.id == role_id.to_string()).ok_or_else(|| {
            ApiError::invalid(vec![FieldError::new(
                "reward_role_id",
                t!(lang, "achievements.web.role-missing"),
            )])
        })?;
        let facts = RoleFacts {
            everyone: row.everyone,
            managed: row.managed,
            position: row.position,
            permissions: row.permissions.parse().unwrap_or(u64::MAX),
        };
        let label = format!("@{}", row.name);
        if let Some(why) = settings::role_refusal_in(lang, facts, info.bot.top_role_position, &label) {
            return Err(ApiError::invalid(vec![FieldError::new("reward_role_id", why)]));
        }
        if let Some(why) = settings::run_time_refusal_in(lang, facts.permissions, &label) {
            return Err(ApiError::invalid(vec![FieldError::new("reward_role_id", why)]));
        }
        let grantor = guild_info::grantor(state, managed, &info.roles, true).await?;
        if !grantor.owner
            && let Some(why) = settings::grant_refusal_in(lang, facts, grantor, &label)
        {
            return Err(ApiError::invalid(vec![FieldError::new("reward_role_id", why)]));
        }
    }
    let draft = AchievementDraft {
        name: &body.name,
        description: &body.description,
        icon: &body.icon,
        kind,
        target: body.target,
        reward_role_id: role,
        reward_points: body.reward_points,
        announce_channel_id: channel,
        secret: body.secret,
        enabled: body.enabled,
    };
    let before = match id {
        Some(id) => state
            .app
            .db
            .achievement(managed.id, id)
            .await?
            .map(|row| row_json(&row, false)),
        None => None,
    };
    let outcome = state.app.db.save_achievement(managed.id, id, &draft).await?;
    let id = match outcome {
        AchievementSave::Saved(id) => id,
        AchievementSave::Full => return Err(ApiError::limit(t!(lang, "achievements.full"))),
        AchievementSave::NameTaken => return Err(invalid("name")),
        AchievementSave::Missing => return Err(ApiError::not_found()),
    };
    pinchora_commands::invalidate_achievements(&state.app, managed.id).await?;
    let row = state
        .app
        .db
        .achievement(managed.id, id)
        .await?
        .ok_or_else(ApiError::not_found)?;
    audit::finish(
        state,
        &managed.current.session,
        Change {
            guild: managed.id,
            action: "achievements.save",
            target: Some(&body.name),
            before: json!(before),
            after: row_json(&row, false),
        },
        None,
    )
    .await;
    Ok(row_json(&row, false))
}

async fn create(
    State(state): State<WebState>,
    managed: ManagedGuild,
    Json(body): Json<Body>,
) -> ApiResult<(StatusCode, Json<Value>)> {
    let row = save(&state, &managed, None, body).await?;
    Ok((StatusCode::CREATED, Json(row)))
}

async fn update(
    State(state): State<WebState>,
    managed: ManagedGuild,
    Path((_, id)): Path<(String, i64)>,
    Json(body): Json<Body>,
) -> ApiResult<Json<Value>> {
    Ok(Json(save(&state, &managed, Some(id), body).await?))
}

async fn remove(
    State(state): State<WebState>,
    managed: ManagedGuild,
    Path((_, id)): Path<(String, i64)>,
) -> ApiResult<StatusCode> {
    let before = state
        .app
        .db
        .achievement(managed.id, id)
        .await?
        .ok_or_else(ApiError::not_found)?;
    if !state.app.db.remove_achievement(managed.id, id).await? {
        return Err(ApiError::not_found());
    }
    pinchora_commands::invalidate_achievements(&state.app, managed.id).await?;
    let target = id.to_string();
    audit::finish(
        &state,
        &managed.current.session,
        Change {
            guild: managed.id,
            action: "achievements.remove",
            target: Some(&target),
            before: row_json(&before, false),
            after: json!(null),
        },
        None,
    )
    .await;
    Ok(StatusCode::NO_CONTENT)
}

async fn holders(
    State(state): State<WebState>,
    managed: ManagedGuild,
    Path((_, id)): Path<(String, i64)>,
) -> ApiResult<Json<Value>> {
    if state.app.db.achievement(managed.id, id).await?.is_none() {
        return Err(ApiError::not_found());
    }
    let rows = state.app.db.achievement_holders(managed.id, id, 100).await?;
    Ok(Json(
        json!({"members":rows.iter().map(|(user,at)| json!({"user_id":user.to_string(),"earned_at":at})).collect::<Vec<_>>() }),
    ))
}
