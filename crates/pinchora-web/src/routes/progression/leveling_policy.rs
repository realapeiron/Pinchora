//! Guild leveling policy and bounded offline imports.

use std::{collections::BTreeMap, time::Duration};

use axum::{
    Json, Router,
    extract::{Path, State},
    routing::{get, post, put},
};
use pinchora_core::admin::settings as level_rules;
use pinchora_core::progression::leveling::week_start;
use pinchora_core::progression::leveling_policy::LevelingPolicy;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::audit;
use crate::audit::Change;
use crate::error::ApiError;
use crate::error::ApiJson;
use crate::error::ApiResult;
use crate::extract::ManagedGuild;
use crate::guild_info;
use crate::state::WebState;

pub fn router() -> Router<WebState> {
    Router::new()
        .route("/", get(settings).put(save_settings))
        .route("/highlights", get(highlights))
        .route("/voice-roles", get(voice_roles))
        .route("/voice-roles/{role}", put(put_voice_role).delete(delete_voice_role))
        .route("/import/preview", post(preview))
        .route("/import/apply", post(apply))
}

async fn voice_roles(State(state): State<WebState>, managed: ManagedGuild) -> ApiResult<Json<Value>> {
    voice_roles_view(&state, managed.id).await
}

async fn voice_roles_view(state: &WebState, guild: u64) -> ApiResult<Json<Value>> {
    let rows = state.app.db.voice_level_roles(guild).await?;
    Ok(Json(
        json!({"items": rows.iter().map(|r| json!({"role_id": r.role_id.to_string(), "level": r.level})).collect::<Vec<_>>(), "max": 25}),
    ))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct VoiceRoleBody {
    level: i64,
}

async fn put_voice_role(
    State(state): State<WebState>,
    managed: ManagedGuild,
    Path(params): Path<BTreeMap<String, String>>,
    ApiJson(body): ApiJson<VoiceRoleBody>,
) -> ApiResult<Json<Value>> {
    let role = params
        .get("role")
        .and_then(|role| role.parse::<u64>().ok())
        .filter(|id| *id > 0 && *id <= i64::MAX as u64)
        .ok_or_else(|| ApiError::malformed("Invalid role ID."))?;
    let level = level_rules::check_level(body.level)?;
    let info = guild_info::live(&state, managed.id).await?;
    let grantor = guild_info::grantor(&state, &managed, &info.roles, true).await?;
    info.check_grant("role_id", role, grantor)?;
    let old = state
        .app
        .db
        .voice_level_roles(managed.id)
        .await?
        .into_iter()
        .find(|r| r.role_id as u64 == role)
        .map(|r| r.level);
    if old == Some(level) {
        return voice_roles_view(&state, managed.id).await;
    }
    if !state.app.db.add_voice_level_role(managed.id, role, level).await? {
        return Err(ApiError::limit("A server can have 25 voice level roles."));
    }
    audit::finish(
        &state,
        &managed.current.session,
        Change {
            guild: managed.id,
            action: "leveling.voice_role.put",
            target: Some(&role.to_string()),
            before: json!({"level": old}),
            after: json!({"level": level}),
        },
        None,
    )
    .await;
    voice_roles_view(&state, managed.id).await
}

async fn delete_voice_role(
    State(state): State<WebState>,
    managed: ManagedGuild,
    Path(params): Path<BTreeMap<String, String>>,
) -> ApiResult<Json<Value>> {
    let role = params
        .get("role")
        .and_then(|role| role.parse::<u64>().ok())
        .filter(|id| *id > 0 && *id <= i64::MAX as u64)
        .ok_or_else(|| ApiError::malformed("Invalid role ID."))?;
    let old = state
        .app
        .db
        .voice_level_roles(managed.id)
        .await?
        .into_iter()
        .find(|r| r.role_id as u64 == role)
        .map(|r| r.level);
    if old.is_some() && state.app.db.remove_voice_level_role(managed.id, role).await? {
        audit::finish(
            &state,
            &managed.current.session,
            Change {
                guild: managed.id,
                action: "leveling.voice_role.delete",
                target: Some(&role.to_string()),
                before: json!({"level": old}),
                after: Value::Null,
            },
            None,
        )
        .await;
    }
    voice_roles_view(&state, managed.id).await
}

async fn settings(State(state): State<WebState>, managed: ManagedGuild) -> ApiResult<Json<LevelingPolicy>> {
    Ok(Json(state.app.db.leveling_policy(managed.id).await?))
}

async fn save_settings(
    State(state): State<WebState>,
    managed: ManagedGuild,
    ApiJson(config): ApiJson<LevelingPolicy>,
) -> ApiResult<Json<LevelingPolicy>> {
    config.validate().map_err(ApiError::malformed)?;
    if let Some(raw) = &config.highlights_channel_id {
        let channel = raw
            .parse::<u64>()
            .map_err(|_| ApiError::malformed("Invalid highlights channel."))?;
        let info = guild_info::get_with(&state, managed.id, |info| info.channel(channel).is_some()).await?;
        info.check_post_channel("highlights_channel_id", channel)?;
    }
    if let Some(slug) = &config.vanity_slug
        && state
            .app
            .db
            .vanity_guild(slug)
            .await?
            .is_some_and(|other| other != managed.id)
    {
        return Err(ApiError::limit("This leaderboard URL name is already in use."));
    }
    let old = state.app.db.leveling_policy(managed.id).await?;
    if config.curve != old.curve {
        crate::dashboard_access::check_level_rewards(&state, &managed).await?;
    }
    if config != old {
        state.app.db.save_leveling_policy(managed.id, &config).await?;
        audit::finish(
            &state,
            &managed.current.session,
            Change {
                guild: managed.id,
                action: "leveling.parity.save",
                target: None,
                before: json!(old),
                after: json!(config),
            },
            None,
        )
        .await;
    }
    Ok(Json(config))
}

async fn highlights(State(state): State<WebState>, managed: ManagedGuild) -> ApiResult<Json<Value>> {
    let week = week_start(chrono::Utc::now());
    let rows = state.app.db.public_level_board(managed.id, Some(week), 10).await?;
    Ok(Json(
        json!({"week": week, "items": rows.iter().map(|row| json!({"user_id": row.user_id.to_string(), "xp": row.value})).collect::<Vec<_>>() }),
    ))
}

pub async fn public_board(State(state): State<WebState>, Path(slug): Path<String>) -> ApiResult<Json<Value>> {
    if slug.len() > 32 {
        return Err(ApiError::not_found());
    }
    let guild = state
        .app
        .db
        .vanity_guild(&slug)
        .await?
        .ok_or_else(ApiError::not_found)?;
    let week = week_start(chrono::Utc::now());
    let rows = state.app.db.public_level_board(guild, None, 25).await?;
    let weekly = state.app.db.public_level_board(guild, Some(week), 10).await?;
    Ok(Json(json!({
        "guild_id": guild.to_string(),
        "leaders": rows.iter().map(|r| json!({"user_id": r.user_id.to_string(), "name": r.name, "xp": r.value})).collect::<Vec<_>>(),
        "weekly": weekly.iter().map(|r| json!({"user_id": r.user_id.to_string(), "name": r.name, "xp": r.value})).collect::<Vec<_>>()
    })))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PreviewBody {
    format: String,
    data: String,
    mode: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ApplyBody {
    preview_id: String,
}

fn read_id(value: &Value) -> Option<u64> {
    value
        .as_str()
        .and_then(|s| s.parse().ok())
        .or_else(|| value.as_u64())
        .filter(|id| *id > 0 && *id <= i64::MAX as u64)
}

fn parse_json(data: &str) -> ApiResult<Vec<(u64, i64)>> {
    let value: Value = serde_json::from_str(data).map_err(|_| ApiError::malformed("Invalid JSON import."))?;
    let rows = if let Some(rows) = value.as_array() {
        rows
    } else {
        ["players", "members", "leaderboard", "users"]
            .iter()
            .find_map(|key| value.get(key).and_then(Value::as_array))
            .ok_or_else(|| ApiError::malformed("JSON must contain a players, members, leaderboard or users array."))?
    };
    let mut out = Vec::new();
    for row in rows {
        let id = ["id", "user_id", "userId", "discord_id", "discordId", "discordUserId"]
            .iter()
            .find_map(|key| row.get(key).and_then(read_id))
            .or_else(|| row.get("user").and_then(|user| user.get("id")).and_then(read_id))
            .ok_or_else(|| ApiError::malformed("A row has no valid Discord user ID."))?;
        let xp = [
            "xp",
            "total_xp",
            "totalXp",
            "experience",
            "totalExperience",
            "total_experience",
        ]
        .iter()
        .find_map(|key| {
            row.get(key)
                .and_then(|v| v.as_i64().or_else(|| v.as_str().and_then(|s| s.parse().ok())))
        })
        .ok_or_else(|| ApiError::malformed("A row has no XP total."))?;
        out.push((id, xp));
    }
    Ok(out)
}

fn csv_fields(line: &str) -> ApiResult<Vec<String>> {
    let mut fields = Vec::new();
    let mut field = String::new();
    let mut quoted = false;
    let mut closed = false;
    let mut chars = line.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '"' if quoted && chars.peek() == Some(&'"') => {
                chars.next();
                field.push('"');
            }
            '"' if quoted => {
                quoted = false;
                closed = true;
            }
            '"' if field.is_empty() && !closed => quoted = true,
            ',' if !quoted => {
                fields.push(std::mem::take(&mut field));
                closed = false;
            }
            _ if !quoted && (closed || ch == '"') => return Err(ApiError::malformed("Malformed CSV row.")),
            _ => field.push(ch),
        }
    }
    if quoted {
        return Err(ApiError::malformed("Unclosed CSV quote."));
    }
    fields.push(field);
    Ok(fields)
}

fn parse_csv(data: &str) -> ApiResult<Vec<(u64, i64)>> {
    let mut lines = data.lines().filter(|line| !line.trim().is_empty());
    let headers = csv_fields(lines.next().ok_or_else(|| ApiError::malformed("CSV is empty."))?)?;
    let names: Vec<_> = headers
        .iter()
        .map(|s| {
            s.trim()
                .trim_start_matches('\u{feff}')
                .to_ascii_lowercase()
                .replace([' ', '-'], "_")
        })
        .collect();
    let find = |keys: &[&str]| names.iter().position(|name| keys.contains(&name.as_str()));
    let id_at = find(&["id", "user_id", "userid", "discord_id", "discordid", "discord_user_id"])
        .ok_or_else(|| ApiError::malformed("CSV needs an ID column."))?;
    let xp_at = find(&["xp", "total_xp", "totalxp", "experience", "total_experience"])
        .ok_or_else(|| ApiError::malformed("CSV needs an XP column."))?;
    let mut out = Vec::new();
    for line in lines {
        let row = csv_fields(line)?;
        if row.len() != headers.len() {
            return Err(ApiError::malformed("Malformed CSV row."));
        }
        let id = row
            .get(id_at)
            .and_then(|v| v.trim().parse::<u64>().ok())
            .filter(|id| *id > 0 && *id <= i64::MAX as u64)
            .ok_or_else(|| ApiError::malformed("A CSV row has an invalid Discord user ID."))?;
        let xp = row
            .get(xp_at)
            .and_then(|v| v.trim().parse::<i64>().ok())
            .ok_or_else(|| ApiError::malformed("A CSV row has invalid XP."))?;
        out.push((id, xp));
    }
    Ok(out)
}

fn parse_import(format: &str, data: &str) -> ApiResult<Vec<(u64, i64)>> {
    if data.len() > 2_000_000 {
        return Err(ApiError::limit("Import file exceeds 2 MB."));
    }
    let rows = match format {
        "json" => parse_json(data)?,
        "csv" => parse_csv(data)?,
        _ => return Err(ApiError::malformed("Choose JSON or CSV.")),
    };
    if rows.is_empty() || rows.len() > 10_000 {
        return Err(ApiError::limit("Import must contain 1 to 10,000 members."));
    }
    let mut unique = BTreeMap::new();
    for (id, xp) in rows {
        if !(0..=1_000_000_000).contains(&xp) {
            return Err(ApiError::malformed("XP must be 0 to 1,000,000,000."));
        }
        unique
            .entry(id)
            .and_modify(|old: &mut i64| *old = (*old).max(xp))
            .or_insert(xp);
    }
    Ok(unique.into_iter().collect())
}

async fn check_import_members(state: &WebState, guild: u64, rows: &[(u64, i64)]) -> ApiResult<()> {
    use futures_util::{StreamExt, TryStreamExt};
    use twilight_model::id::Id;
    futures_util::stream::iter(rows.iter().copied().map(|(user, _)| async move {
        let response = state
            .app
            .http
            .guild_member(Id::new(guild), Id::new(user))
            .await
            .map_err(|_| ApiError::blocked("Import members must be verified in this server."))?;
        let member = response
            .model()
            .await
            .map_err(|_| ApiError::blocked("Import members must be verified in this server."))?;
        if member.user.bot {
            return Err(ApiError::blocked("Import members must be verified in this server."));
        }
        Ok(())
    }))
    .buffer_unordered(8)
    .try_collect::<Vec<_>>()
    .await?;
    Ok(())
}

async fn preview(
    State(state): State<WebState>,
    managed: ManagedGuild,
    ApiJson(body): ApiJson<PreviewBody>,
) -> ApiResult<Json<Value>> {
    if !["merge", "replace"].contains(&body.mode.as_str()) {
        return Err(ApiError::malformed("Choose merge or replace."));
    }
    let rows = parse_import(&body.format, &body.data)?;
    check_import_members(&state, managed.id, &rows).await?;
    let comparison = state
        .app
        .db
        .preview_member_xp_import(managed.id, &rows, body.mode == "replace")
        .await?;
    let changed = comparison.iter().filter(|r| r.2 != r.3).count();
    let id = format!("{:032x}", rand::random::<u128>());
    let key = format!("xp:offline-preview:{}:{}:{id}", managed.id, managed.current.sid);
    state
        .app
        .cache
        .put(
            &key,
            &serde_json::to_string(&(body.mode, &rows)).map_err(|_| ApiError::malformed("Import cannot be saved."))?,
            Duration::from_secs(600),
        )
        .await?;
    Ok(Json(json!({"preview_id": id, "count": rows.len(), "changed": changed,
            "sample": comparison.iter().take(10).map(|r| json!({"user_id": r.0.to_string(), "xp": r.1, "current_xp": r.2, "result_xp": r.3})).collect::<Vec<_>>() })))
}

async fn apply(
    State(state): State<WebState>,
    managed: ManagedGuild,
    ApiJson(body): ApiJson<ApplyBody>,
) -> ApiResult<Json<Value>> {
    crate::dashboard_access::check_level_rewards(&state, &managed).await?;
    if body.preview_id.len() != 32 || !body.preview_id.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(ApiError::malformed("Invalid preview ID."));
    }
    let key = format!(
        "xp:offline-preview:{}:{}:{}",
        managed.id, managed.current.sid, body.preview_id
    );
    let raw = state
        .app
        .cache
        .get(&key)
        .await?
        .ok_or_else(|| ApiError::blocked("Preview expired. Preview the file again."))?;
    let (mode, rows): (String, Vec<(u64, i64)>) =
        serde_json::from_str(&raw).map_err(|_| ApiError::malformed("Invalid preview."))?;
    check_import_members(&state, managed.id, &rows).await?;
    let (updated, fresh) = state
        .app
        .db
        .apply_level_import(
            &rows,
            mode == "replace",
            &pinchora_db::NewAudit {
                guild: managed.id,
                actor: managed.current.session.user_id,
                actor_name: managed.current.session.display_name(),
                source: pinchora_db::AuditSource::Web,
                action: "xp.import.offline",
                target: Some(&body.preview_id),
                before: None,
                after: None,
            },
        )
        .await?;
    let users: Vec<u64> = rows.iter().map(|row| row.0).collect();
    let app = state.app.clone();
    let guild = managed.id;
    if fresh {
        state.app.spawn(async move {
            for user in users {
                if let Err(error) = pinchora_commands::sync_member_roles(&app, guild, user).await {
                    tracing::warn!(%error, guild, user, "import role sync failed");
                }
            }
        });
    }
    Ok(Json(json!({"checked": rows.len(), "updated": updated})))
}

#[cfg(test)]
mod tests {
    use crate::routes::progression::leveling_policy::*;
    #[test]
    fn rejects_oversized_and_malformed_imports() {
        assert!(parse_import("json", &" ".repeat(2_000_001)).is_err());
        for csv in [
            "id,xp\n123,4,5",
            "id,xp\n123",
            "id,xp\n12\"3,4",
            "id,xp\n\"123\"junk,4",
            "id,xp\n\"123,4",
            "id,xp\n0,3",
            "id,xp\n123,1000000001",
        ] {
            assert!(parse_import("csv", csv).is_err(), "{csv}");
        }
        for json in [
            "[]",
            "{}",
            "[null]",
            "[{\"id\":\"123\",\"xp\":1.5}]",
            "[{\"id\":\"123\",\"xp\":-1}]",
        ] {
            assert!(parse_import("json", json).is_err(), "{json}");
        }
    }

    #[test]
    fn json_and_csv_imports() {
        assert_eq!(
            parse_import(
                "json",
                r#"{"players":[{"id":"123","xp":50},{"user_id":"123","total_xp":70}]}"#
            )
            .unwrap(),
            vec![(123, 70)]
        );
        assert_eq!(
            parse_import("csv", "user_id,total_xp\n123,80\n").unwrap(),
            vec![(123, 80)]
        );
        assert_eq!(
            parse_import("csv", "Discord ID,Total XP\n123,80\n").unwrap(),
            vec![(123, 80)]
        );
        assert_eq!(
            parse_import("json", r#"{"members":[{"user":{"id":"123"},"totalExperience":"90"}]}"#).unwrap(),
            vec![(123, 90)]
        );
        assert!(parse_import("csv", "id,xp\n123,-1").is_err());
    }
}
