//! Giving and taking roles for the roles pack, and what to say when Discord says no.

use pinchora_core::admin::settings;
use pinchora_core::admin::settings::Grantor;
use pinchora_core::admin::settings::RoleFacts;
use pinchora_core::infra::i18n::Lang;
use pinchora_core::t;
use twilight_http::{Error, request::AuditLogReason};
use twilight_model::{guild::Role, id::Id};

use crate::App;
use crate::roles::roles;

const UNKNOWN_ROLE: u64 = 10011;
const MISSING_ACCESS: u64 = 50001;
const MISSING_PERMISSIONS: u64 = 50013;

#[derive(Debug)]
struct UnsafeRole {
    role: u64,
    why: String,
}

impl std::fmt::Display for UnsafeRole {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.why)
    }
}

impl std::error::Error for UnsafeRole {}

/// Re-read both the target role and bot hierarchy for each grant. Configuration-time checks are
/// insufficient because a manager can edit a role's permissions after a panel was posted.
pub(crate) async fn validate(app: &App, guild: u64, role: u64) -> anyhow::Result<()> {
    let (roles, top) = bot_view(app, guild)
        .await
        .ok_or_else(|| anyhow::anyhow!("Could not check the current role hierarchy"))?;
    let found = roles
        .iter()
        .find(|r| r.id.get() == role)
        .ok_or_else(|| anyhow::anyhow!("That role no longer exists"))?;
    let facts = RoleFacts {
        everyone: role == guild,
        managed: found.managed,
        position: found.position,
        permissions: found.permissions.bits(),
    };
    let label = format!("<@&{role}>");
    if let Some(why) = settings::role_refusal(facts, Some(top), &label).or_else(|| {
        settings::grant_refusal(
            facts,
            Grantor {
                owner: false,
                administrator: false,
                top_role: Some(top),
            },
            &label,
        )
    }) {
        return Err(UnsafeRole { role, why }.into());
    }
    Ok(())
}

pub(crate) async fn give(app: &App, guild: u64, user: u64, role: u64, reason: &str) -> anyhow::Result<()> {
    validate(app, guild, role).await?;
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
) -> anyhow::Result<()> {
    let _lock = app.db.lock_role_member(guild, user).await?;
    if expiry_secs > 0 {
        let at = app.now() + chrono::Duration::seconds(i64::from(expiry_secs));
        // Persist the expiry before Discord can grant the role: a database outage cannot turn a
        // temporary role into a permanent one.
        app.db.schedule_removal(guild, user, role, at).await?;
    }
    match give(app, guild, user, role, reason).await {
        Ok(()) => Ok(()),
        Err(error) => {
            if expiry_secs > 0
                && let Err(cancel) = app.db.cancel_removal(guild, user, role).await
            {
                tracing::warn!(guild, user, role, %cancel, "failed grant's timer remains for recovery");
            }
            if error.downcast_ref::<Error>().is_some_and(roles::role_is_gone) {
                forget(app, guild, role).await;
            }
            Err(error)
        }
    }
}

pub(crate) fn give_refusal_text(lang: Lang, error: &anyhow::Error) -> String {
    if let Some(unsafe_role) = error.downcast_ref::<UnsafeRole>() {
        return if lang == Lang::En {
            unsafe_role.why.clone()
        } else {
            configure_refusal(lang, unsafe_role.role, &unsafe_role.why)
        };
    }
    error
        .downcast_ref::<Error>()
        .map_or_else(|| error.to_string(), |discord| refusal_text(lang, discord))
}

pub(crate) fn configure_refusal(lang: Lang, role: u64, why: &str) -> String {
    if why.contains("above my highest role") {
        t!(lang, "roles.refusal.bot-hierarchy", role = format!("<@&{role}>"))
    } else if why.contains("managed by an integration") {
        t!(lang, "roles.refusal.managed", role = format!("<@&{role}>"))
    } else if why.contains("@everyone") {
        t!(lang, "roles.refusal.everyone")
    } else {
        t!(lang, "roles.refusal.cannot-configure", role = format!("<@&{role}>"))
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

pub(crate) fn is_final_grant(error: &anyhow::Error) -> bool {
    error.downcast_ref::<Error>().is_some_and(is_final)
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

#[cfg(test)]
mod tests {
    use crate::commands::roles::automation::give::*;

    #[test]
    fn setup_refusal_uses_the_member_language() {
        let why = "<@&7> is above my highest role. Drag my role above it in Server Settings > Roles, then try again.";
        assert!(configure_refusal(Lang::En, 7, why).contains("above my highest role"));
        assert!(configure_refusal(Lang::Tr, 7, why).contains("üstünde"));
        let managed = "<@&7> is managed by an integration or a bot, so it can't be handed out.";
        assert!(configure_refusal(Lang::En, 7, managed).contains("managed by an integration"));
        assert!(configure_refusal(Lang::Tr, 7, managed).contains("entegrasyon"));
        assert!(configure_refusal(Lang::En, 7, "@everyone can't be handed out").contains("@everyone"));
    }
}
