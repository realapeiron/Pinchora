//! Giving and taking roles for the roles pack, and what to say when Discord says no.

use pinchora_core::{i18n::Lang, t};
use twilight_http::{Error, request::AuditLogReason};
use twilight_model::{guild::Role, id::Id};

use crate::{App, roles};

const UNKNOWN_ROLE: u64 = 10011;
const MISSING_ACCESS: u64 = 50001;
const MISSING_PERMISSIONS: u64 = 50013;

pub(crate) async fn give(app: &App, guild: u64, user: u64, role: u64, reason: &str) -> Result<(), Error> {
    app.http
        .add_guild_member_role(Id::new(guild.max(1)), Id::new(user.max(1)), Id::new(role.max(1)))
        .reason(reason)
        .await?;
    Ok(())
}

pub(crate) async fn take(app: &App, guild: u64, user: u64, role: u64, reason: &str) -> Result<(), Error> {
    app.http
        .remove_guild_member_role(Id::new(guild.max(1)), Id::new(user.max(1)), Id::new(role.max(1)))
        .reason(reason)
        .await?;
    Ok(())
}

/// Gives a role and, when it lasts a while (`expiry_secs` above 0), sets when it is taken away again. A role
/// that no longer exists is dropped from everything that named it.
pub(crate) async fn give_timed(
    app: &App,
    guild: u64,
    user: u64,
    role: u64,
    expiry_secs: i32,
    reason: &str,
) -> Result<(), Error> {
    match give(app, guild, user, role, reason).await {
        Ok(()) => {
            if expiry_secs > 0 {
                let at = app.now() + chrono::Duration::seconds(i64::from(expiry_secs));
                if let Err(error) = app.db.schedule_removal(guild, user, role, at).await {
                    tracing::warn!(guild, user, role, %error, "could not time a role");
                }
            }
            Ok(())
        }
        Err(error) => {
            if roles::role_is_gone(&error) {
                forget(app, guild, role).await;
            }
            Err(error)
        }
    }
}

/// Drops a role that no longer exists on Discord from everything that named it.
pub(crate) async fn forget(app: &App, guild: u64, role: u64) {
    if let Err(error) = app.db.forget_role(guild, role).await {
        tracing::warn!(guild, role, %error, "could not drop a deleted role");
    }
    if let Err(error) = app.db.forget_pack_role(guild, role).await {
        tracing::warn!(guild, role, %error, "could not drop a deleted role from the roles pack");
    }
    app.forget_guild_settings(guild).await;
}

/// Why Discord refused to give or take a role, worded to follow "I couldn't give you <role>:".
pub fn refusal_text(lang: Lang, error: &Error) -> String {
    match roles::error_code(error) {
        Some(MISSING_PERMISSIONS) => t!(lang, "roles.refusal.hierarchy"),
        Some(UNKNOWN_ROLE) => t!(lang, "roles.refusal.gone"),
        Some(_) => t!(lang, "roles.refusal.discord"),
        None => t!(lang, "roles.refusal.unreachable"),
    }
}

/// Whether a failed role change will fail again however often it is tried: the member is gone, the role
/// is gone, or the bot may not. A failure with no answer from Discord at all is worth another try.
pub(crate) fn is_final(error: &Error) -> bool {
    roles::member_is_gone(error)
        || roles::role_is_gone(error)
        || matches!(roles::error_code(error), Some(MISSING_ACCESS | MISSING_PERMISSIONS))
}

/// The server's roles, and the position of the bot's highest one (the roles below it are the ones it
/// can hand out); `None` when Discord cannot be asked.
pub(crate) async fn bot_view(app: &App, guild: u64) -> Option<(Vec<Role>, i64)> {
    let guild_id = Id::new(guild.max(1));
    let me = app
        .http
        .guild_member(guild_id, Id::new(app.application_id.get()))
        .await
        .ok()?
        .model()
        .await
        .ok()?;
    let roles = app.http.roles(guild_id).await.ok()?.models().await.ok()?;
    let top = roles
        .iter()
        .filter(|r| me.roles.contains(&r.id))
        .map(|r| r.position)
        .max()
        .unwrap_or(0);
    Some((roles, top))
}
