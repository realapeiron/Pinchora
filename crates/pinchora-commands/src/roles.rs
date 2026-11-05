//! Handing roles out on Discord: level rewards and what the server store sells.

use pinchora_core::leveling::{self, LevelReward};
use pinchora_db::LevelRoleRow;
use twilight_http::{Error, api_error::ApiError, error::ErrorType, request::AuditLogReason};
use twilight_model::{guild::Role, id::Id};

use crate::App;

const UNKNOWN_MEMBER: u64 = 10007;
const UNKNOWN_ROLE: u64 = 10011;
const MISSING_PERMISSIONS: u64 = 50013;

/// Discord's error code, when it sent one.
pub(crate) fn error_code(error: &Error) -> Option<u64> {
    match error.kind() {
        ErrorType::Response {
            error: ApiError::General(general),
            ..
        } => Some(general.code),
        _ => None,
    }
}

pub(crate) fn role_is_gone(error: &Error) -> bool {
    error_code(error) == Some(UNKNOWN_ROLE)
}

pub(crate) fn member_is_gone(error: &Error) -> bool {
    error_code(error) == Some(UNKNOWN_MEMBER)
}

/// Why Discord refused to give a role, worded to follow "I couldn't give you <role>:".
pub(crate) fn refusal(error: &Error) -> String {
    match error_code(error) {
        Some(MISSING_PERMISSIONS) => {
            "my highest role is not above it, or I lack the Manage Roles permission".to_owned()
        }
        Some(UNKNOWN_ROLE) => "that role no longer exists".to_owned(),
        Some(_) => "Discord refused it".to_owned(),
        None => "Discord could not be reached".to_owned(),
    }
}

pub(crate) fn rewards(rows: &[LevelRoleRow]) -> Vec<LevelReward> {
    rows.iter()
        .map(|r| LevelReward {
            level: r.level.max(0) as u32,
            role_id: r.role_id as u64,
        })
        .collect()
}

pub(crate) async fn level_rewards(app: &App, guild: u64) -> anyhow::Result<Vec<LevelReward>> {
    Ok(rewards(&app.db.level_roles(guild).await?))
}

/// The roles the bot's own highest role sits above are the ones it can hand out.
async fn bot_position(app: &App, guild: u64) -> anyhow::Result<i64> {
    let guild = Id::new(guild.max(1));
    let me = app
        .http
        .guild_member(guild, Id::new(app.application_id.get()))
        .await?
        .model()
        .await?;
    let roles = app.http.roles(guild).await?.models().await?;
    Ok(roles
        .iter()
        .filter(|r| me.roles.contains(&r.id))
        .map(|r| r.position)
        .max()
        .unwrap_or(0))
}

/// Why the bot cannot hand out `role`, or `None` when it can.
pub(crate) async fn unassignable(app: &App, guild: u64, role: &Role) -> Option<String> {
    if role.id.get() == guild {
        return Some("@everyone can't be handed out: every member already has it.".to_owned());
    }
    if role.managed {
        return Some(format!(
            "<@&{}> is managed by an integration or a bot, so it can't be handed out.",
            role.id
        ));
    }
    match bot_position(app, guild).await {
        Ok(top) if role.position < top => None,
        Ok(_) => Some(format!(
            "<@&{}> is above my highest role. Drag my role above it in Server Settings > Roles, then try again.",
            role.id
        )),
        Err(error) => {
            tracing::warn!(guild, %error, "could not look up the bot's roles");
            Some("I couldn't check my own role position just now. Try again in a moment.".to_owned())
        }
    }
}

/// Gives and takes the level roles a member at `level` should hold, as `(added, removed)`.
/// Discord refusals are logged and skipped; a role that no longer exists is dropped from the rewards.
pub(crate) async fn apply_level_roles(
    app: &App,
    guild: u64,
    user: u64,
    level: u32,
    current: &[u64],
    rewards: &[LevelReward],
    stack: bool,
) -> (usize, usize) {
    let (add, remove) = leveling::role_changes(rewards, level, stack, current);
    let (guild_id, user_id) = (Id::new(guild.max(1)), Id::new(user.max(1)));
    let (mut added, mut removed) = (0, 0);
    for role in add {
        match app
            .http
            .add_guild_member_role(guild_id, user_id, Id::new(role.max(1)))
            .reason("Level reward")
            .await
        {
            Ok(_) => added += 1,
            Err(error) => {
                tracing::warn!(guild, user, role, %error, "could not give a level role");
                if role_is_gone(&error)
                    && let Err(error) = app.db.forget_role(guild, role).await
                {
                    tracing::warn!(guild, role, %error, "could not drop a deleted level role");
                }
            }
        }
    }
    for role in remove {
        match app
            .http
            .remove_guild_member_role(guild_id, user_id, Id::new(role.max(1)))
            .reason("Level reward replaced")
            .await
        {
            Ok(_) => removed += 1,
            Err(error) => tracing::warn!(guild, user, role, %error, "could not take a level role"),
        }
    }
    (added, removed)
}

/// Applies the server's level roles after `user` reached `level`. Never fails the caller.
pub(crate) async fn level_up_roles(app: &App, guild: u64, user: u64, level: u32, current: &[u64], stack: bool) {
    match level_rewards(app, guild).await {
        Ok(rewards) if !rewards.is_empty() => {
            apply_level_roles(app, guild, user, level, current, &rewards, stack).await;
        }
        Ok(_) => {}
        Err(error) => tracing::warn!(guild, %error, "could not load the level roles"),
    }
}

/// The nearest level role above `level` that still exists, as its name and level.
/// `None` when there is none or Discord cannot name the roles.
pub(crate) async fn next_role(app: &App, guild: u64, level: u32) -> anyhow::Result<Option<(String, u32)>> {
    let rewards = level_rewards(app, guild).await?;
    if !rewards.iter().any(|r| r.level > level) {
        return Ok(None);
    }
    let Some(names) = app.guild_roles(guild).await else {
        return Ok(None);
    };
    let existing: Vec<LevelReward> = rewards.into_iter().filter(|r| names.contains_key(&r.role_id)).collect();
    Ok(leveling::next_reward(&existing, level).map(|r| (names[&r.role_id].clone(), r.level)))
}
