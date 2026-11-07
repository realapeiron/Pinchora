//! What the party games share: editing and posting card messages with the bot's own token, the names
//! kept in lobbies, and the switch for gambling-style games.

use std::time::Duration;

use pinchora_core::{i18n::Lang, t};
use pinchora_game::play::Game;
use twilight_interactions::command::{CommandModel, CreateCommand};
use twilight_model::{
    channel::message::{Component, Embed},
    http::attachment::Attachment,
    id::Id,
};
use twilight_util::builder::embed::{EmbedBuilder, ImageSource};

use super::games::{pay_win, quiet, record};
use crate::{App, Cx};

/// A display name as kept in a lobby: no line breaks, short enough for a card.
pub(crate) fn clean_name(name: &str) -> String {
    name.replace(['|', '\n', '\r'], " ").chars().take(24).collect()
}

/// An embed showing the attached file `filename` as its image.
pub(crate) fn card_embed(embed: EmbedBuilder, filename: &str) -> Embed {
    match ImageSource::attachment(filename) {
        Ok(source) => embed.image(source).build(),
        Err(_) => embed.build(),
    }
}

/// Edits a message by id with the bot's own token, replacing its picture with `file`: for what changes
/// with nobody clicking, like a lobby that ran out of time. `content` of `None` leaves the text as it
/// was. False when Discord refuses (the message is gone, or the bot cannot see the channel).
pub(crate) async fn edit_card(
    app: &App,
    (channel, message): (u64, u64),
    content: Option<&str>,
    embed: Embed,
    (filename, bytes): (&str, Vec<u8>),
    components: &[Component],
) -> bool {
    if channel == 0 || message == 0 {
        return false;
    }
    let attachment = Attachment::from_bytes(filename.to_owned(), bytes, 1);
    let embeds = [embed];
    let no_pings = quiet();
    let result = async {
        let mut request = app
            .http
            .update_message(Id::new(channel), Id::new(message))
            .embeds(Some(&embeds))
            .components(Some(components))
            .attachments(std::slice::from_ref(&attachment))
            .keep_attachment_ids(&[])
            .allowed_mentions(no_pings.as_ref());
        if let Some(content) = content {
            request = request.content(Some(content));
        }
        request.await?;
        anyhow::Ok(())
    }
    .await;
    if let Err(error) = &result {
        tracing::debug!(%error, channel, message, "party card not edited");
    }
    result.is_ok()
}

/// Posts a new message with a picture in a channel with the bot's own token; returns its id.
pub(crate) async fn post_card(
    app: &App,
    channel: u64,
    content: &str,
    embed: Embed,
    (filename, bytes): (&str, Vec<u8>),
    components: &[Component],
) -> anyhow::Result<u64> {
    let attachment = Attachment::from_bytes(filename.to_owned(), bytes, 1);
    let embeds = [embed];
    let no_pings = quiet();
    let message = app
        .http
        .create_message(Id::new(channel.max(1)))
        .content(content)
        .embeds(&embeds)
        .components(components)
        .attachments(std::slice::from_ref(&attachment))
        .allowed_mentions(no_pings.as_ref())
        .await?
        .model()
        .await?;
    Ok(message.id.get())
}

/// Learns which message the command just posted and keeps it on the session, so the janitor can edit it
/// later. Like the other games' version, but it never replaces a message the session already knows.
pub(crate) fn track_message(cx: &Cx, session: i64) {
    let app = cx.app.clone();
    let responder = cx.responder.clone();
    cx.app.spawn(async move {
        // The answer to the command may still be on its way to Discord.
        tokio::time::sleep(Duration::from_millis(1_500)).await;
        let message = responder.original().await;
        match message {
            Ok((channel, message)) => {
                if let Err(error) = app.db.remember_game_message(session, channel, message).await {
                    tracing::debug!(%error, session, "party message not remembered");
                }
            }
            Err(error) => tracing::debug!(%error, session, "could not look up the party message"),
        }
    });
}

/// A rest period after something that paid out, so the same players are not paid again at once.
pub(crate) async fn resting(app: &App, kind: &str, user: u64) -> anyhow::Result<Option<Duration>> {
    // The key holds the second the rest ends, so the answer can say how long is left.
    let Some(until) = app.cache.get(&format!("rest:{kind}:{user}")).await? else {
        return Ok(None);
    };
    let until: i64 = until.parse().unwrap_or(0);
    let left = until - chrono::Utc::now().timestamp();
    Ok((left > 0).then(|| Duration::from_secs(left as u64)))
}

pub(crate) async fn rest(app: &App, kind: &str, user: u64, secs: u64) {
    let until = chrono::Utc::now().timestamp() + secs as i64;
    if let Err(error) = app
        .cache
        .put(
            &format!("rest:{kind}:{user}"),
            &until.to_string(),
            Duration::from_secs(secs),
        )
        .await
    {
        tracing::debug!(%error, user, kind, "rest not saved");
    }
}

/// Ends a rest early, for a click that turned out to do nothing.
pub(crate) async fn unrest(app: &App, kind: &str, user: u64) {
    if let Err(error) = app.cache.forget(&format!("rest:{kind}:{user}")).await {
        tracing::debug!(%error, user, kind, "rest not ended");
    }
}

/// "3 min" or "40 s": how long is left, for a refusal, in `lang`; in Turkish `3 dk` or `40 sn`.
pub(crate) fn wait_text_in(lang: Lang, left: Duration) -> String {
    let secs = left.as_secs() + 1;
    if secs > 90 {
        t!(lang, "party.wait-min", n = secs.div_ceil(60))
    } else {
        t!(lang, "party.wait-sec", n = secs)
    }
}

/// Pays the winner of a friendly game a reward (capped per day through the Redis counter the other
/// calm games use) and counts the game; returns the shells paid.
pub(crate) async fn reward_win(app: &App, user: u64, game: Game, reason: &str) -> i64 {
    match pay_win(app, user, game, game.win_reward(), reason).await {
        Ok(paid) => paid,
        Err(error) => {
            tracing::warn!(%error, user, "friendly win reward not paid");
            0
        }
    }
}

/// Counts a finished game for every player in `players`; `won` says who won.
pub(crate) async fn count(app: &App, game: Game, players: &[(u64, bool)]) {
    for (user, won) in players {
        record(app, game, *user, *won).await;
    }
}

#[derive(CommandModel, CreateCommand)]
#[command(
    name = "gambling",
    desc = "Switch the shell game, staked derbies and other gambling-style games on or off for this server"
)]
pub struct ConfigGambling {
    /// On lets members stake shells on games of chance; off (the default) closes them
    enabled: bool,
}

impl ConfigGambling {
    pub(crate) async fn run(self, cx: &Cx, guild: u64) -> anyhow::Result<()> {
        // Discord's default permissions can be changed by a server's admins; the switch checks too.
        if !cx.can_manage_guild() {
            return cx.reply_ephemeral(t!(cx.lang(), "party.gambling.denied")).await;
        }
        cx.app.db.set_gambling(guild, self.enabled).await?;
        let key = if self.enabled {
            "party.gambling.on"
        } else {
            "party.gambling.off"
        };
        cx.reply_ephemeral(t!(cx.lang(), key)).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lobby_names_are_short_and_single_line() {
        assert_eq!(clean_name("Cap|tain\nPinch"), "Cap tain Pinch");
        assert_eq!(clean_name(&"x".repeat(80)).len(), 24);
    }

    /// Discord refuses a command whose names or descriptions are too long, or that has too many options,
    /// and only says so when the commands are registered.
    #[test]
    fn every_command_stays_within_discords_limits() {
        use twilight_model::application::command::CommandOption;

        fn check(path: &str, options: &[CommandOption]) {
            assert!(options.len() <= 25, "`{path}` has {} options", options.len());
            for option in options {
                let here = format!("{path} {}", option.name);
                assert!((1..=32).contains(&option.name.len()), "`{here}`: name length");
                assert!(
                    (1..=100).contains(&option.description.chars().count()),
                    "`{here}`: description"
                );
                assert!(
                    option.choices.as_deref().unwrap_or_default().len() <= 25,
                    "`{here}`: choices"
                );
                check(&here, option.options.as_deref().unwrap_or_default());
            }
        }
        let commands = crate::command_definitions();
        assert!(commands.len() <= 100, "{} commands", commands.len());
        for command in &commands {
            assert!(
                (1..=32).contains(&command.name.len()),
                "`{}`: name length",
                command.name
            );
            assert!(
                (1..=100).contains(&command.description.chars().count()),
                "`{}`: description is {} characters",
                command.name,
                command.description.chars().count()
            );
            check(&command.name, &command.options);
        }
        for name in ["surge", "play", "duel", "setup"] {
            assert!(commands.iter().any(|c| c.name == name), "{name} is registered");
        }
    }

    #[test]
    fn waits_read_in_the_unit_that_fits() {
        assert_eq!(wait_text_in(Lang::En, Duration::from_secs(39)), "40 s");
        assert_eq!(wait_text_in(Lang::En, Duration::from_secs(299)), "5 min");
        assert_eq!(wait_text_in(Lang::En, Duration::from_secs(0)), "1 s");
        assert_eq!(wait_text_in(Lang::Tr, Duration::from_secs(39)), "40 sn");
        assert_eq!(wait_text_in(Lang::Tr, Duration::from_secs(299)), "5 dk");
    }
}
