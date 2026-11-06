//! Temp voice: join a creator channel and a voice room appears for you, with you in it; leave the last seat
//! and it is gone. The owner runs it with `/voice` or a panel of buttons; managers set creators up with
//! `/voice creators` or on the dashboard.
//!
//! Voice events (`events`) make and delete rooms, `track` remembers who sits where (Redis), `lifecycle`
//! talks to Discord about a room's birth and death, `control` is what an owner can do to a room, `panel` is
//! the message of buttons, and `sweep` is the half-minute backstop for a leave nobody heard.

mod command;
mod control;
mod events;
mod lifecycle;
mod panel;
mod rooms;
mod setups;
mod sweep;
mod track;

pub use command::VoiceCommand;
pub(crate) use command::{on_button, on_modal, on_select};
pub(crate) use events::{on_voice_state, reconcile};
pub use sweep::{spawn_ticker, tick};

use pinchora_core::i18n::Lang;
use twilight_model::{guild::Permissions, id::Id};

use crate::App;

/// The language a room's messages (its panel, its notices) are written in.
pub(crate) async fn lang_of(app: &App, guild: u64) -> Lang {
    app.guild_lang(guild).await
}

/// How many people sit in a room right now.
pub async fn room_members(app: &App, channel: u64) -> usize {
    track::members(app, channel).await.map_or(0, |m| m.len())
}

/// Forgets what this pod read of a server's creators and rooms, after the dashboard changed them.
pub async fn forget_setup(guild: u64) {
    setups::forget(guild).await;
}

/// The permissions temp voice needs that the bot lacks in this server (by Discord's names): making and
/// deleting channels, moving members and writing overwrites. Empty when it has them all or is an administrator.
pub async fn missing_permissions(app: &App, guild: u64) -> anyhow::Result<Vec<&'static str>> {
    let server = Id::new(guild.max(1));
    let me = app
        .http
        .guild_member(server, Id::new(app.application_id.get()))
        .await?
        .model()
        .await?;
    let roles = app.http.roles(server).await?.models().await?;
    let held = roles
        .iter()
        .filter(|r| r.id.get() == guild || me.roles.contains(&r.id))
        .fold(Permissions::empty(), |all, r| all | r.permissions);
    if held.contains(Permissions::ADMINISTRATOR) {
        return Ok(Vec::new());
    }
    Ok([
        (Permissions::MANAGE_CHANNELS, rooms::MANAGE_CHANNELS),
        (Permissions::MOVE_MEMBERS, rooms::MOVE_MEMBERS),
        (Permissions::MANAGE_ROLES, rooms::MANAGE_ROLES),
    ]
    .into_iter()
    .filter(|(bit, _)| !held.contains(*bit))
    .map(|(_, name)| name)
    .collect())
}

#[cfg(test)]
mod tests {
    use pinchora_core::modules::Module;

    #[test]
    fn the_command_and_its_buttons_belong_to_the_module() {
        for sub in ["lock", "kick", "creators", "panel"] {
            assert_eq!(Module::of_command("voice", Some(sub)), Some(Module::TempVoice));
        }
        for prefix in ["vc", "vo"] {
            assert_eq!(Module::of_component_prefix(prefix), Some(Module::TempVoice));
        }
    }
}
