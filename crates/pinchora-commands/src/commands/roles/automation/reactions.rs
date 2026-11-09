//! Reaction roles: a reaction on a posted reactions panel gives its role, and taking the reaction back
//! takes the role. The rules of the panel (one at a time, a limit, a required role) hold here too.

use std::{
    collections::HashMap,
    sync::{Arc, LazyLock},
    time::Duration,
};

use moka::future::Cache as Moka;
use pinchora_core::admin::modules::Module;
use pinchora_core::roles::automation::PanelRules;
use pinchora_core::roles::automation::PanelStyle;
use pinchora_core::roles::automation::toggle;
use twilight_model::{
    channel::message::EmojiReactionType,
    id::{
        Id,
        marker::{ChannelMarker, MessageMarker, UserMarker},
    },
};

use crate::App;
use crate::commands::roles::automation::give;
use crate::commands::roles::automation::panel::KeyEmoji;
use crate::commands::roles::automation::panel::apply;
use crate::commands::roles::automation::panel::current_roles;
use crate::commands::roles::automation::panel::emoji_key;
use crate::commands::roles::automation::panel::request_type;
use crate::commands::roles::automation::panel::rules_of;
use crate::commands::roles::automation::panel::style_of;
use crate::infra::hooks::ReactionEvent;

/// What a reactions panel is, as a reaction needs it.
struct ReactionPanel {
    rules: PanelRules,
    required_role: Option<u64>,
    /// The panel's roles, in order.
    roles: Vec<u64>,
    /// Which role each emoji key gives.
    by_key: HashMap<String, u64>,
    temp: HashMap<u64, i32>,
}

impl ReactionPanel {
    fn key_of(&self, role: u64) -> Option<&str> {
        self.by_key.iter().find(|(_, r)| **r == role).map(|(k, _)| k.as_str())
    }
}

/// Panels by message, kept for a minute; a message that is not a panel is kept as such, so a busy
/// channel's reactions cost one lookup a minute.
static PANELS: LazyLock<Moka<u64, Option<Arc<ReactionPanel>>>> = LazyLock::new(|| {
    Moka::builder()
        .max_capacity(100_000)
        .time_to_live(Duration::from_secs(60))
        .build()
});

/// Drops this pod's copy of what a message is (a panel was posted or changed).
pub(crate) async fn forget(message: u64) {
    PANELS.invalidate(&message).await;
}

async fn lookup(app: &App, guild: u64, message: u64) -> Option<Arc<ReactionPanel>> {
    let loaded = PANELS
        .try_get_with(message, async {
            let Some(panel) = app.db.panel_by_message(guild, message).await? else {
                return anyhow::Ok(None);
            };
            if style_of(&panel) != PanelStyle::Reactions {
                return Ok(None);
            }
            let entries = app.db.panel_roles(panel.id).await?;
            let reactions = app.db.panel_reactions(panel.id).await?;
            Ok(Some(Arc::new(ReactionPanel {
                rules: rules_of(&panel),
                required_role: panel.required_role.map(|r| r as u64),
                roles: entries.iter().map(|e| e.role_id as u64).collect(),
                by_key: reactions.into_iter().map(|(k, r)| (k, r as u64)).collect(),
                temp: entries
                    .iter()
                    .filter_map(|e| e.temp_secs.map(|s| (e.role_id as u64, s)))
                    .collect(),
            })))
        })
        .await;
    match loaded {
        Ok(found) => found,
        Err(error) => {
            tracing::warn!(guild, message, %error, "could not look up a reaction panel");
            None
        }
    }
}

/// A reaction on a message in a server (the `on_reaction` hook).
pub(crate) async fn on_reaction(app: &Arc<App>, event: &ReactionEvent) {
    match event {
        ReactionEvent::Add(added) => {
            let Some(guild) = added.guild_id else { return };
            let who = Who {
                guild: guild.get(),
                user: added.user_id,
                channel: added.channel_id,
                message: added.message_id,
            };
            if !app
                .module_allowed(who.guild, Some(who.channel.get()), Module::RolePanels)
                .await
            {
                return;
            }
            if added.member.as_ref().is_some_and(|m| m.user.bot) || added.user_id.get() == app.application_id.get() {
                return;
            }
            picked(app, &who, &added.emoji).await;
        }
        ReactionEvent::Remove(removed) => {
            let Some(guild) = removed.guild_id else { return };
            if removed.user_id.get() == app.application_id.get() {
                return;
            }
            let who = Who {
                guild: guild.get(),
                user: removed.user_id,
                channel: removed.channel_id,
                message: removed.message_id,
            };
            if !app
                .module_allowed(who.guild, Some(who.channel.get()), Module::RolePanels)
                .await
            {
                return;
            }
            dropped(app, &who, &removed.emoji).await;
        }
        // Staff clearing a message's reactions does not take anyone's role; the panel just needs posting again.
        ReactionEvent::RemoveAll(_) | ReactionEvent::RemoveEmoji(_) => {}
    }
}

/// Who reacted, and on what.
struct Who {
    guild: u64,
    user: Id<UserMarker>,
    channel: Id<ChannelMarker>,
    message: Id<MessageMarker>,
}

/// Takes a member's reaction off the message again.
async fn undo(app: &App, who: &Who, emoji: &EmojiReactionType) {
    let result = app
        .http
        .delete_reaction(who.channel, who.message, &request_type(emoji), who.user)
        .await;
    if let Err(error) = result {
        tracing::warn!(guild = who.guild, %error, "could not take a reaction back");
    }
}

async fn picked(app: &Arc<App>, who: &Who, emoji: &EmojiReactionType) {
    let Some(panel) = lookup(app, who.guild, who.message.get()).await else {
        return;
    };
    let key = emoji_key(emoji);
    // An emoji the panel does not use is somebody else's business.
    let Some(&role) = panel.by_key.get(&key) else { return };
    let Ok(_lock) = app.db.lock_role_member(who.guild, who.user.get()).await else {
        return;
    };
    let Ok(held) = current_roles(app, who.guild, who.user.get()).await else {
        return;
    };
    if panel.required_role.is_some_and(|needed| !held.contains(&needed)) {
        undo(app, who, emoji).await;
        return;
    }
    if held.contains(&role) {
        return;
    }
    let changes = match toggle(&panel.rules, &panel.roles, &held, role) {
        Ok(changes) => changes,
        Err(_) => {
            undo(app, who, emoji).await;
            return;
        }
    };
    let done = apply(app, who.guild, who.user.get(), &changes, &panel.temp, "Reaction role").await;
    // Swapping drops the roles of the member's other reactions: their emojis come off the message too.
    for gone in &done.removed {
        if let Some(emoji) = panel.key_of(*gone).and_then(KeyEmoji::parse) {
            let result = app
                .http
                .delete_reaction(who.channel, who.message, &emoji.request(), who.user)
                .await;
            if let Err(error) = result {
                tracing::debug!(guild = who.guild, %error, "could not take a swapped reaction back");
            }
        }
    }
    if done.failed.is_some() {
        undo(app, who, emoji).await;
    }
}

async fn dropped(app: &Arc<App>, who: &Who, emoji: &EmojiReactionType) {
    let Some(panel) = lookup(app, who.guild, who.message.get()).await else {
        return;
    };
    let Some(&role) = panel.by_key.get(&emoji_key(emoji)) else {
        return;
    };
    let user = who.user.get();
    let Ok(_lock) = app.db.lock_role_member(who.guild, user).await else {
        return;
    };
    let Ok(held) = current_roles(app, who.guild, user).await else {
        return;
    };
    if !held.contains(&role) || held.iter().filter(|id| panel.roles.contains(id)).count() <= panel.rules.min_picks {
        return;
    }
    match give::take(app, who.guild, user, role, "Reaction role").await {
        Ok(()) => {
            if let Err(error) = app.db.cancel_removal(who.guild, user, role).await {
                tracing::warn!(guild = who.guild, user, role, %error, "could not cancel a role's timer");
            }
        }
        Err(error) => {
            tracing::warn!(guild = who.guild, user, role, %error, "could not take a reaction role");
            if crate::roles::roles::role_is_gone(&error) {
                give::forget(app, who.guild, role).await;
            }
        }
    }
}
