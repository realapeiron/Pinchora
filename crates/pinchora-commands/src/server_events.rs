//! What the server events (giveaways, suggestions, polls) share: the ticker that ends giveaways, closes
//! polls and keeps entry counts fresh, and the small helpers their commands use.
//!
//! Everything the ticker does is safe on many pods at once: a giveaway ends and a poll closes inside one
//! database transaction that only one pod wins, and the announcement is claimed before it is sent.

use std::{sync::Arc, time::Duration};

use chrono::Duration as Span;
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
/// Cleanup runs every this many wakes (hourly).
const CLEANUP_EVERY: u32 = 720;

const UNKNOWN_CHANNEL: u64 = 10003;
const MISSING_ACCESS: u64 = 50001;
const MISSING_PERMISSIONS: u64 = 50013;

/// One step of the ticker: ends the giveaways and closes the polls whose time is up, tells the
/// channels that have not been told yet (the pod that ended one may have died before it could) and
/// shows the entry counts that changed too fast to show at the time. Returns how many giveaways and
/// polls it ended.
pub async fn tick(app: &Arc<App>) -> anyhow::Result<usize> {
    let now = app.now();
    let mut ended = 0;
    for id in app.db.due_giveaways(now, 50).await? {
        match giveaway::end(app, id, false).await {
            Ok(Some(_)) => ended += 1,
            Ok(None) => {}
            Err(error) => tracing::warn!(%error, giveaway = id, "giveaway not ended"),
        }
    }
    for id in app.db.due_polls(now, 50).await? {
        match poll::close(app, id).await {
            Ok(true) => ended += 1,
            Ok(false) => {}
            Err(error) => tracing::warn!(%error, poll = id, "poll not closed"),
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
pub(crate) fn post_refusal(channel: Id<ChannelMarker>, error: &Error) -> String {
    match roles::error_code(error) {
        Some(MISSING_ACCESS | MISSING_PERMISSIONS) => {
            format!("I can't post in <#{channel}>: I need View Channel, Send Messages and Embed Links there.")
        }
        Some(UNKNOWN_CHANNEL) => "That channel no longer exists.".to_owned(),
        Some(_) => "Discord refused the message.".to_owned(),
        None => "Discord could not be reached. Try again in a moment.".to_owned(),
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
