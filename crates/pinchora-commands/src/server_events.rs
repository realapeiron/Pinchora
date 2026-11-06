//! What the server events (giveaways, suggestions, polls) share: the ticker that ends giveaways, closes
//! polls and keeps entry counts fresh, and the small helpers their commands use.
//!
//! Everything the ticker does is safe on many pods at once: a giveaway ends and a poll closes inside one
//! database transaction that only one pod wins, and the announcement is claimed before it is sent.

use std::{sync::Arc, time::Duration};

use chrono::Duration as Span;
use pinchora_core::{i18n::Lang, modules::Module, t};
use twilight_http::{Error, error::ErrorType};
use twilight_model::{
    channel::message::{
        Component, EmojiReactionType, MessageFlags,
        component::{ActionRow, Button, ButtonStyle},
    },
    http::interaction::InteractionResponseData,
    id::{Id, marker::ChannelMarker},
};

use crate::{
    App, Cx,
    commands::{games::quiet, giveaway, poll},
    roles,
};

/// The ticker wakes this often.
const WAKE: Duration = Duration::from_secs(5);
/// Giveaways and polls are forgotten this long after they ended.
const KEEP: Span = Span::days(60);
/// How many due giveaways (and polls) one step looks at, to pick up to 50 of them whose module is on.
const SCAN: i64 = 500;
/// Cleanup runs every this many wakes (hourly).
const CLEANUP_EVERY: u32 = 720;

const UNKNOWN_CHANNEL: u64 = 10003;
const MISSING_ACCESS: u64 = 50001;
const MISSING_PERMISSIONS: u64 = 50013;

/// One step of the ticker: ends the giveaways and closes the polls whose time is up, tells the
/// channels that have not been told yet (the pod that ended one may have died before it could) and
/// shows the entry counts that changed too fast to show at the time. Returns how many giveaways and
/// polls it ended.
///
/// A giveaway or poll in a server (or channel) that switched its module off is paused, not ended: no
/// winner is drawn, no shells move and nothing is posted while it is off, and once it is switched on
/// again the ones that ran out meanwhile end on the next step. Announcements of ones that already ended
/// still go out. The step looks at [`SCAN`] due ones at a time, so paused ones cannot starve the others.
pub async fn tick(app: &Arc<App>) -> anyhow::Result<usize> {
    let now = app.now();
    let mut ended = 0;
    let mut turns = 0;
    for due in app.db.due_giveaways(now, SCAN).await? {
        if !app
            .module_allowed(due.guild_id as u64, Some(due.channel_id as u64), Module::Giveaways)
            .await
        {
            continue;
        }
        turns += 1;
        if turns > 50 {
            break;
        }
        match giveaway::end(app, due.id, false).await {
            Ok(Some(_)) => ended += 1,
            Ok(None) => {}
            Err(error) => tracing::warn!(%error, giveaway = due.id, "giveaway not ended"),
        }
    }
    turns = 0;
    for due in app.db.due_polls(now, SCAN).await? {
        if !app
            .module_allowed(due.guild_id as u64, Some(due.channel_id as u64), Module::Polls)
            .await
        {
            continue;
        }
        turns += 1;
        if turns > 50 {
            break;
        }
        match poll::close(app, due.id).await {
            Ok(true) => ended += 1,
            Ok(false) => {}
            Err(error) => tracing::warn!(%error, poll = due.id, "poll not closed"),
        }
    }
    giveaway::announce_pending(app).await;
    poll::announce_pending(app).await;
    giveaway::refresh_changed(app).await;
    Ok(ended)
}

/// Runs [`tick`] for the life of the process, and forgets old giveaways and polls once an hour.
pub fn spawn_ticker(app: Arc<App>) {
    tokio::spawn(async move {
        let mut wake = tokio::time::interval(WAKE);
        wake.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let mut rounds = 0u32;
        loop {
            wake.tick().await;
            if let Err(error) = Box::pin(tick(&app)).await {
                tracing::warn!(%error, "server events tick failed");
            }
            rounds = rounds.wrapping_add(1);
            if rounds.is_multiple_of(CLEANUP_EVERY) {
                let before = app.now() - KEEP;
                match (
                    app.db.forget_old_giveaways(before).await,
                    app.db.forget_old_polls(before).await,
                ) {
                    (Ok(a), Ok(b)) if a + b > 0 => tracing::debug!(giveaways = a, polls = b, "old events forgotten"),
                    (Ok(_), Ok(_)) => {}
                    (a, b) => tracing::warn!(giveaways = ?a.err(), polls = ?b.err(), "old events not forgotten"),
                }
            }
        }
    });
}

/// The language for a direct message to `user` about something in `guild`: their own `/language` pick,
/// else the server's default, else English. A DM has no Discord locale to read.
pub(crate) async fn dm_lang(app: &App, user: u64, guild: u64) -> Lang {
    match app.user_lang_choice(user).await {
        Some(lang) => lang,
        None => app.guild_lang(guild).await,
    }
}

/// An answer only the member sees, which pings nobody.
pub(crate) async fn tell(cx: &Cx, text: impl Into<String>) -> anyhow::Result<()> {
    cx.reply(InteractionResponseData {
        content: Some(text.into()),
        flags: Some(MessageFlags::EPHEMERAL),
        allowed_mentions: quiet(),
        ..Default::default()
    })
    .await
}

/// A button with an optional emoji.
pub(crate) fn button(
    custom_id: String,
    label: &str,
    emoji: Option<&str>,
    style: ButtonStyle,
    disabled: bool,
) -> Component {
    Component::Button(Button {
        custom_id: Some(custom_id),
        disabled,
        emoji: emoji.map(|name| EmojiReactionType::Unicode { name: name.to_owned() }),
        id: None,
        label: Some(label.to_owned()),
        sku_id: None,
        style,
        url: None,
    })
}

/// A row of buttons.
pub(crate) fn row(buttons: Vec<Component>) -> Component {
    Component::ActionRow(ActionRow {
        id: None,
        components: buttons,
    })
}

/// A link to a message, for replies that say where something was posted.
pub(crate) fn message_link(guild: u64, channel: u64, message: u64) -> String {
    format!("https://discord.com/channels/{guild}/{channel}/{message}")
}

/// Whether trying again could never help: Discord refused the request itself (the channel is gone,
/// the bot lost access) or it is malformed. Network trouble and server errors are worth another try.
pub(crate) fn is_permanent(error: &Error) -> bool {
    match error.kind() {
        ErrorType::Response { status, .. } => status.is_client_error() && status.get() != 429,
        ErrorType::Validation | ErrorType::Json | ErrorType::BuildingRequest => true,
        _ => false,
    }
}

/// Why Discord would not take a message, for the manager who asked for it.
pub(crate) fn post_refusal(lang: Lang, channel: Id<ChannelMarker>, error: &Error) -> String {
    match roles::error_code(error) {
        Some(MISSING_ACCESS | MISSING_PERMISSIONS) => {
            t!(lang, "post-refusal.access", channel = format!("<#{channel}>"))
        }
        Some(UNKNOWN_CHANNEL) => t!(lang, "post-refusal.gone"),
        Some(_) => t!(lang, "post-refusal.refused"),
        None => t!(lang, "post-refusal.unreachable"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn links_point_at_the_message() {
        assert_eq!(message_link(1, 2, 3), "https://discord.com/channels/1/2/3");
    }
}
