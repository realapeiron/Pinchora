//! Evaluate historical Discord overrides with Discord precedence, separately from bot policy.
use crate::{Cx, text::access::Verdict};
use twilight_model::guild::Permissions;

pub(crate) async fn saved_permissions_denied(cx: &Cx, path: &str) -> anyhow::Result<bool> {
    let Some(route) = super::route(path).filter(|r| r.active()) else {
        return Ok(false);
    };
    let Some(guild) = cx.guild_id() else { return Ok(false) };
    let permissions = cx
        .interaction
        .member
        .as_ref()
        .and_then(|m| m.permissions)
        .unwrap_or_else(Permissions::empty);
    if permissions.contains(Permissions::ADMINISTRATOR) {
        return Ok(false);
    }
    let Some(saved) = cx.app.saved_command_overrides(guild).await? else {
        return Ok(false);
    };
    let root = route.legacy.split(' ').next().unwrap();
    let held: Vec<_> = cx
        .interaction
        .member
        .as_ref()
        .map(|m| m.roles.iter().map(|r| r.get()).collect())
        .unwrap_or_default();
    let channels = match cx.interaction.channel.as_ref() {
        Some(channel) => cx.app.place_lineage(channel.id.get()).await,
        None => vec![],
    };
    let denied = match saved.verdict(root, guild, cx.author()?.id.get(), &held, &channels) {
        Verdict::Denied => true,
        Verdict::Allowed => false,
        Verdict::Silent => {
            let tree = crate::commands::legacy_definitions();
            let required = tree.iter().find(|c| c.name == root).unwrap().default_member_permissions;
            required.is_some_and(|p| p.is_empty() || !permissions.contains(p))
        }
    };
    Ok(denied)
}

pub(crate) async fn refuses_saved_permissions(cx: &Cx, path: &str) -> anyhow::Result<bool> {
    let denied = saved_permissions_denied(cx, path).await?;
    if denied {
        cx.reply_ephemeral(pinchora_core::t!(
            cx.lang(),
            "text.server-denied",
            command = super::canonical(path)
        ))
        .await?;
    }
    Ok(denied)
}
