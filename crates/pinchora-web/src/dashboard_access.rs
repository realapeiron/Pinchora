//! One fail-closed area map for every guild endpoint. Grants never confer Discord permissions.
use crate::{error::ApiResult, state::WebState};
use twilight_model::id::Id;

/// Shared shell/picker reads are available to every delegate. Unknown and privileged paths are not.
pub fn route_area(path: &str) -> Option<&'static str> {
    match path.trim_matches('/').split('/').next().unwrap_or("") {
        "settings" | "modules" | "xp-rules" | "xp-admin" | "level-roles" | "rep-roles" | "jobs" | "leveling-parity"
        | "store" | "cards" | "message-studio" | "preview" | "branding" | "commands" | "responders" => {
            Some("configuration")
        }
        "roles" => Some("roles"),
        "mod-parity" | "moderation" | "logs" | "cases" | "staff-log" | "automod" | "anti_nuke" | "safety" => {
            Some("safety")
        }
        "plazas" | "gatherings" | "community" | "community-pack" | "community-v2" | "suggestions" | "critters"
        | "surge" | "temp-voice" | "tickets" | "birthdays" | "events" | "starboard" | "achievements" | "utility" => {
            Some("community")
        }
        "feeds" | "trackers" | "roblox" => Some("feeds"),
        "stats" | "invites" => Some("stats"),
        "audit" | "audit-tools" => Some("audit"),
        _ => None,
    }
}

pub fn permitted(path: &str, safe: bool, areas: &[String]) -> bool {
    if areas.is_empty() {
        return false;
    }
    let path = path.trim_matches('/');
    (safe && matches!(path, "" | "discord")) || route_area(path).is_some_and(|area| areas.iter().any(|a| a == area))
}

/// No membership cache: removed roles and grants stop working on the next request, including reads.
pub async fn areas(state: &WebState, guild: u64, user: u64) -> ApiResult<Vec<String>> {
    let member = match state.app.http.guild_member(Id::new(guild), Id::new(user)).await {
        Ok(response) => response.model().await.map_err(anyhow::Error::from)?,
        Err(_) => return Ok(Vec::new()),
    };
    areas_for_roles(state, guild, &member.roles).await
}

pub async fn areas_for_roles(
    state: &WebState,
    guild: u64,
    held: &[Id<twilight_model::id::marker::RoleMarker>],
) -> ApiResult<Vec<String>> {
    let grants = state.app.db.dashboard_managers(guild).await?;
    let mut areas: Vec<String> = grants
        .into_iter()
        .filter(|g| held.iter().any(|r| r.get() == g.role_id as u64))
        .flat_map(|g| g.areas)
        .collect();
    areas.sort();
    areas.dedup();
    Ok(areas)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn grants_fail_closed_and_do_not_open_privileged_or_adjacent_paths() {
        let roles = vec!["roles".into()];
        for path in ["roles", "roles/panels/1/post", "/roles/"] {
            assert!(permitted(path, false, &roles));
        }
        for path in [
            "access",
            "backup",
            "command-control",
            "roles-extra",
            "audit/export",
            "settings",
            "new-feature",
        ] {
            assert!(!permitted(path, true, &roles));
        }
        assert!(permitted("", true, &roles));
        assert!(permitted("discord", true, &roles));
        assert!(!permitted("discord", false, &roles));
        assert!(!permitted("", true, &[]));
    }

    #[test]
    fn role_ids_follow_whitespace_tolerant_import_decoding() {
        let mut ids = Vec::new();
        role_ids(&serde_json::json!({"actions":[{"role_id":" 9001 "}]}), &mut ids);
        assert_eq!(ids, vec![9001]);
    }
}

/// The router enforces access before body extraction, even if a new handler forgets its extractor.
pub async fn guard(
    axum::extract::State(state): axum::extract::State<WebState>,
    request: axum::extract::Request,
    next: axum::middleware::Next,
) -> Result<axum::response::Response, crate::error::ApiError> {
    use axum::extract::FromRequestParts;
    let (mut parts, mut body) = request.into_parts();
    let managed = crate::extract::ManagedGuild::from_request_parts(&mut parts, &state).await?;
    if managed.areas.is_some() && !parts.method.is_safe() {
        let mut ids = Vec::new();
        if let Ok(axum::extract::Path(params)) =
            axum::extract::Path::<std::collections::HashMap<String, String>>::from_request_parts(&mut parts, &state)
                .await
        {
            ids.extend(params.values().filter_map(|s| s.parse::<i64>().ok()));
        }
        if parts
            .headers
            .get(axum::http::header::CONTENT_TYPE)
            .and_then(|h| h.to_str().ok())
            .is_some_and(|h| {
                let mime = h.split(';').next().unwrap_or_default().trim().to_ascii_lowercase();
                mime == "application/json" || mime.starts_with("application/") && mime.ends_with("+json")
            })
        {
            let bytes = axum::body::to_bytes(body, 2 * 1024 * 1024)
                .await
                .map_err(|_| crate::error::ApiError::malformed("Invalid request body."))?;
            if let Ok(value) = serde_json::from_slice(&bytes) {
                role_ids(&value, &mut ids);
            }
            body = axum::body::Body::from(bytes);
        }
        check_role_access(&state, &managed, &ids).await?;
    }
    parts.extensions.insert(managed);
    Ok(next.run(axum::extract::Request::from_parts(parts, body)).await)
}

fn role_ids(value: &serde_json::Value, ids: &mut Vec<i64>) {
    match value {
        serde_json::Value::Object(fields) => fields.values().for_each(|v| role_ids(v, ids)),
        serde_json::Value::Array(values) => values.iter().for_each(|v| role_ids(v, ids)),
        serde_json::Value::String(value) => {
            if let Ok(id) = value.trim().parse() {
                ids.push(id);
            }
        }
        serde_json::Value::Number(value) => {
            if let Some(id) = value.as_i64() {
                ids.push(id);
            }
        }
        _ => {}
    }
}

/// Discord permissions alone do not describe a role's dashboard powers.
pub async fn check_role_access(
    state: &WebState,
    managed: &crate::extract::ManagedGuild,
    roles: &[i64],
) -> ApiResult<()> {
    if let Some(areas) = &managed.areas {
        let grants = state.app.db.dashboard_managers(managed.id).await?;
        if grants
            .iter()
            .any(|grant| roles.contains(&grant.role_id) && grant.areas.iter().any(|a| !areas.contains(a)))
        {
            return Err(crate::error::ApiError::not_manager());
        }
    }
    Ok(())
}

/// XP writes and sync jobs can hand out saved rewards without any role ID in the request.
pub async fn check_level_rewards(state: &WebState, managed: &crate::extract::ManagedGuild) -> ApiResult<()> {
    if managed.areas.is_some() {
        let roles = state.app.db.level_roles(managed.id).await?;
        check_role_access(state, managed, &roles.iter().map(|r| r.role_id).collect::<Vec<_>>()).await?;
    }
    Ok(())
}

/// Re-enabling a module must not reactivate saved privileged role assignments.
pub async fn check_module_roles(
    state: &WebState,
    managed: &crate::extract::ManagedGuild,
    module: pinchora_core::modules::Module,
) -> ApiResult<()> {
    use pinchora_core::modules::Module;
    if managed.areas.is_none() {
        return Ok(());
    }
    let db = &state.app.db;
    let guild = managed.id;
    let mut roles = Vec::new();
    match module {
        Module::CustomCommands => {
            for row in db.custom_commands(guild).await? {
                roles.extend(row.def.actions.iter().filter_map(|a| a.role()).map(|r| r as i64));
            }
        }
        Module::RolePanels => {
            for panel in db.panels(guild).await? {
                roles.extend(db.panel_roles(panel.id).await?.iter().map(|r| r.role_id));
            }
        }
        Module::Leveling => {
            roles.extend(db.level_roles(guild).await?.iter().map(|r| r.role_id));
            roles.extend(db.voice_level_roles(guild).await?.iter().map(|r| r.role_id));
        }
        Module::Welcome => roles.extend(db.guild_settings(guild).await?.join_role_id),
        Module::AutoRoles => roles.extend(db.auto_roles(guild).await?.iter().map(|r| r.role_id)),
        Module::ColorRoles => roles.extend(db.color_roles(guild).await?.iter().map(|r| r.role_id)),
        Module::Store => roles.extend(db.store_items(guild).await?.iter().map(|r| r.role_id)),
        Module::Feeds => roles.extend(db.feeds_of(guild).await?.iter().filter_map(|r| r.feed.live_role_id)),
        Module::Social => roles.extend(db.rep_roles(guild).await?.iter().map(|r| r.role_id)),
        Module::Birthdays => roles.extend(db.birthday_settings(guild).await?.role_id.map(|r| r as i64)),
        Module::Invites => roles.extend(db.invite_rewards(guild).await?.iter().map(|(_, role)| *role)),
        Module::Roblox => {
            let settings = db.roblox_settings(guild).await?;
            roles.extend(settings.verified_role_id);
            roles.extend(settings.unverified_role_id);
            roles.extend(db.roblox_binds(guild).await?.iter().map(|r| r.role_id));
        }
        Module::Verification => {
            let config = db.safety_config(guild).await?;
            roles.extend(config.verification.verified_role_id.and_then(|r| r.parse::<i64>().ok()));
            roles.extend(
                config
                    .verification
                    .unverified_role_id
                    .and_then(|r| r.parse::<i64>().ok()),
            );
        }
        Module::Achievements => roles.extend(db.achievements(guild).await?.iter().filter_map(|r| r.reward_role_id)),
        _ => {}
    }
    check_role_access(state, managed, &roles).await
}

#[cfg(test)]
mod route_inventory_tests {
    use super::*;
    use std::{collections::BTreeSet, path::Path};

    fn enclosed(source: &str, open: char, close: char) -> &str {
        let begin = source.find(open).unwrap();
        let mut depth = 1;
        let mut quoted = false;
        let mut escaped = false;
        for (offset, c) in source[begin + 1..].char_indices() {
            if quoted {
                if escaped {
                    escaped = false;
                } else if c == '\\' {
                    escaped = true;
                } else if c == '"' {
                    quoted = false;
                }
            } else if c == '"' {
                quoted = true;
            } else if c == open {
                depth += 1;
            } else if c == close {
                depth -= 1;
                if depth == 0 {
                    return &source[begin + 1..begin + 1 + offset];
                }
            }
        }
        panic!("unclosed router expression");
    }

    fn registered(module: &str, prefix: &str, found: &mut BTreeSet<(String, String)>) {
        let source = std::fs::read_to_string(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("src/routes")
                .join(format!("{module}.rs")),
        )
        .unwrap();
        let (_, router) = source.split_once("pub fn router(").unwrap();
        let body = enclosed(router, '{', '}');
        for call in body.split(".nest(").skip(1) {
            let args = enclosed(&format!("({call}"), '(', ')').to_owned();
            let (path, module) = args.split_once(',').unwrap();
            let module = module
                .split("::router")
                .next()
                .unwrap()
                .trim()
                .trim_start_matches("super::");
            registered(module, &format!("{prefix}{}", path.trim().trim_matches('"')), found);
        }
        for call in body.split(".route(").skip(1) {
            let args = enclosed(&format!("({call}"), '(', ')').to_owned();
            let (path, methods) = args.split_once(',').unwrap();
            let full = format!("{prefix}{}", path.trim().trim_matches('"'));
            let path = full.trim_end_matches('/');
            let path = if path.is_empty() { "/" } else { path };
            for method in ["get", "put", "post", "patch", "delete", "head", "options"] {
                if methods.contains(&format!("{method}(")) {
                    found.insert((method.to_uppercase(), path.to_owned()));
                }
            }
        }
    }

    #[test]
    fn every_registered_guild_method_is_in_the_reviewed_area_matrix() {
        let mut actual = BTreeSet::new();
        registered("guilds", "", &mut actual);
        let mut reviewed = BTreeSet::new();
        for line in include_str!("../../../docs/w7-guild-route-matrix.tsv").lines().skip(1) {
            let fields: Vec<_> = line.split('\t').collect();
            let (method, path, area) = (fields[0], fields[1], fields[2]);
            reviewed.insert((method.to_owned(), path.to_owned()));
            for grant in pinchora_db::DASHBOARD_AREAS {
                assert_eq!(
                    permitted(path, method == "GET", &[(*grant).into()]),
                    area == *grant || area == "shell" && method == "GET",
                    "{method} {path} with {grant}"
                );
            }
            assert!(!permitted(path, method == "GET", &[]));
        }
        assert_eq!(
            actual, reviewed,
            "Update the explicit security review matrix when adding a guild route"
        );
    }
}

#[cfg(test)]
mod reef_area_tests {
    use super::*;
    #[test]
    fn wave7_guild_actions_have_explicit_community_area_mapping() {
        for path in [
            "plazas",
            "plazas/decorate",
            "plazas/moderation",
            "gatherings",
            "gatherings/1/rsvp",
        ] {
            assert_eq!(route_area(path), Some("community"));
            for area in pinchora_db::DASHBOARD_AREAS {
                assert_eq!(
                    permitted(path, false, &[(*area).into()]),
                    *area == "community",
                    "{path}: {area}"
                );
            }
            assert!(!permitted(path, false, &[]));
        }
    }
}
