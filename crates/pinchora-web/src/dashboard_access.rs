//! One fail-closed area map for every guild endpoint. Grants never confer Discord permissions.
use crate::{error::ApiResult, state::WebState};
use twilight_model::id::Id;

/// Shared shell/picker reads are available to every delegate. Unknown and privileged paths are not.
pub fn route_area(path: &str) -> Option<&'static str> {
    match path.trim_matches('/').split('/').next().unwrap_or("") {
        "settings" | "modules" | "xp-rules" | "xp-admin" | "level-roles" | "rep-roles" | "jobs" | "leveling-parity"
        | "store" | "cards" | "preview" | "branding" | "commands" | "responders" => Some("configuration"),
        "roles" => Some("roles"),
        "mod-parity" | "moderation" | "logs" | "cases" | "staff-log" | "automod" | "anti_nuke" | "safety" => {
            Some("safety")
        }
        "community" | "community-pack" | "community-v2" | "suggestions" | "critters" | "surge" | "temp-voice"
        | "tickets" | "birthdays" | "events" | "starboard" | "achievements" | "utility" => Some("community"),
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
    let grants = state.app.db.dashboard_managers(guild).await?;
    if grants.is_empty() {
        return Ok(Vec::new());
    }
    let member = match state.app.http.guild_member(Id::new(guild), Id::new(user)).await {
        Ok(response) => response.model().await.map_err(anyhow::Error::from)?,
        Err(_) => return Ok(Vec::new()),
    };
    let mut areas: Vec<String> = grants
        .into_iter()
        .filter(|g| member.roles.iter().any(|r| r.get() == g.role_id as u64))
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
}
