//! What the calm games share: reward caps, win counts, names kept in a game's state, message
//! edits, and the janitor that ends games whose clock ran out (and keeps storm surges going).

use std::{sync::Arc, time::Duration};

use pinchora_db::GameSession;
use pinchora_game::play::Game;
use twilight_model::{
    channel::message::{
        AllowedMentions, Component, Embed,
        component::{ActionRow, Button, ButtonStyle},
    },
    id::Id,
};

use crate::{App, Cx};

/// Days since 1970-01-01, UTC: what "today" means for daily caps and the daily wordle.
pub(crate) fn today() -> i64 {
    chrono::Utc::now().timestamp().div_euclid(86_400)
}

/// Pays `shells` for a win unless the player was already paid for `game` wins today (the cap);
/// returns what was paid, 0 when capped.
pub(crate) async fn pay_win(app: &App, user: u64, game: Game, shells: i64, reason: &str) -> anyhow::Result<i64> {
    let key = format!("gamewin:{}:{}:{user}", game.key(), today());
    let wins = app.cache.incr(&key, Duration::from_secs(2 * 86_400)).await?;
    if wins > game.daily_cap() {
        return Ok(0);
    }
    app.db.earn_shells(user, shells, reason).await?;
    Ok(shells)
}

/// Counts a finished game for a player. A failure is logged and swallowed: stats never break a game.
pub(crate) async fn record(app: &App, game: Game, user: u64, won: bool) {
    if let Err(error) = app.db.record_game(user, game.key(), won).await {
        tracing::warn!(%error, user, game = game.key(), "game stats not saved");
    }
}

/// A game's state is its own fields, then the two players' names: `<fields>|<name>|<name>`. The
/// names ride along so the board can show them without asking Discord again.
pub(crate) fn pack(fields: &str, names: [&str; 2]) -> String {
    let clean = |name: &str| {
        name.replace(['|', '\n', '\r'], " ")
            .chars()
            .take(24)
            .collect::<String>()
    };
    format!("{fields}|{}|{}", clean(names[0]), clean(names[1]))
}

/// The fields and the two names of a packed state.
pub(crate) fn unpack(state: &str) -> (&str, [String; 2]) {
    let mut parts = state.splitn(3, '|');
    let fields = parts.next().unwrap_or_default();
    let host = parts.next().unwrap_or("Player 1").to_owned();
    let guest = parts.next().unwrap_or("Player 2").to_owned();
    (fields, [host, guest])
}

/// The channel an interaction came from, 0 when Discord did not say.
pub(crate) fn channel_of(cx: &Cx) -> u64 {
    cx.interaction.channel.as_ref().map_or(0, |c| c.id.get())
}

/// Mentions nobody: players are already in the game, so a move does not ping.
pub(crate) fn quiet() -> Option<AllowedMentions> {
    Some(AllowedMentions::default())
}

/// A row with one button.
pub(crate) fn button_row(custom_id: String, label: &str, style: ButtonStyle) -> Component {
    Component::ActionRow(ActionRow {
        id: None,
        components: vec![button(custom_id, label, style, false)],
    })
}

pub(crate) fn button(custom_id: String, label: &str, style: ButtonStyle, disabled: bool) -> Component {
    Component::Button(Button {
        custom_id: Some(custom_id),
        disabled,
        emoji: None,
        id: None,
        label: Some(label.to_owned()),
        sku_id: None,
        style,
        url: None,
    })
}

/// Edits a message by id with the bot's own token, for changes nobody clicked for: a timeout, or
/// the other player's secret pick. Whatever is `None` stays as it was. False when Discord refuses
/// (the message is gone, or the bot cannot see the channel).
pub(crate) async fn edit_message(
    app: &App,
    channel: u64,
    message: u64,
    content: Option<&str>,
    embeds: Option<&[Embed]>,
    components: Option<&[Component]>,
) -> bool {
    if channel == 0 || message == 0 {
        return false;
    }
    let result = async {
        let mut request = app.http.update_message(Id::new(channel), Id::new(message));
        if let Some(content) = content {
            request = request.content(Some(content));
        }
        if let Some(embeds) = embeds {
            request = request.embeds(Some(embeds));
        }
        if let Some(components) = components {
            request = request.components(Some(components));
        }
        request.await?;
        anyhow::Ok(())
    }
    .await;
    if let Err(error) = &result {
        tracing::debug!(%error, channel, message, "game message not edited");
    }
    result.is_ok()
}

/// Learns which message the command just posted and keeps it on the session, so the janitor can
/// edit it later. Best effort: without it a late game is only closed when someone clicks.
pub(crate) fn track_message(cx: &Cx, session: i64) {
    let app = cx.app.clone();
    let responder = cx.responder.clone();
    cx.app.spawn(async move {
        // The answer to the command may still be on its way to Discord.
        tokio::time::sleep(Duration::from_millis(1_500)).await;
        let message = responder.original().await;
        match message {
            Ok((channel, message)) => {
                if let Err(error) = app.db.set_game_message(session, channel, message).await {
                    tracing::debug!(%error, session, "game message not remembered");
                }
            }
            Err(error) => tracing::debug!(%error, session, "could not look up the game message"),
        }
    });
}

/// Ends every game whose clock ran out: duels give the bets back, hangman is closed, a trivia
/// round reveals its answer. Safe to run on many pods at once; each game ends exactly once.
/// Returns how many it ended.
pub async fn sweep(app: &Arc<App>) -> anyhow::Result<usize> {
    let mut ended = 0;
    for session in app.db.late_games(50).await? {
        let result = match session.kind.as_str() {
            "connect4" | "rps" | "seafleet" | "checkers" | "chess" => super::duel::time_out(app, &session).await,
            "sharegrab" => super::share::time_out(app, &session).await,
            "derby" => super::derby::time_out(app, &session).await,
            "heist" => super::heist::time_out(app, &session).await,
            "shellgame" => super::shells::time_out(app, &session).await,
            "surge" => super::surge::time_out(app, &session).await,
            "hangman" => super::hangman::time_out(app, &session).await,
            "trivia" => super::trivia::finish(app, session.id, None).await,
            other => {
                tracing::warn!(kind = other, session = session.id, "late game of an unknown kind");
                app.db
                    .settle_game(session.id, None, "timeout", pinchora_db::Payout::Refund, true)
                    .await
                    .map(|s| s.is_some())
            }
        };
        match result {
            Ok(true) => ended += 1,
            Ok(false) => {}
            Err(error) => tracing::warn!(%error, session = session.id, kind = %session.kind, "late game not ended"),
        }
    }
    // Timed storm surges start, and running ones show their wall.
    if let Err(error) = super::surge::tick(app).await {
        tracing::warn!(%error, "storm tick failed");
    }
    Ok(ended)
}

/// Runs [`sweep`] every ten seconds for the life of the process.
pub fn spawn_janitor(app: Arc<App>) {
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(Duration::from_secs(10));
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let mut rounds = 0u32;
        loop {
            tick.tick().await;
            if let Err(error) = sweep(&app).await {
                tracing::warn!(%error, "game sweep failed");
            }
            rounds = rounds.wrapping_add(1);
            if rounds.is_multiple_of(360) {
                match app.db.forget_old_games().await {
                    Ok(n) if n > 0 => tracing::debug!(n, "old games forgotten"),
                    Ok(_) => {}
                    Err(error) => tracing::warn!(%error, "old games not forgotten"),
                }
            }
        }
    });
}

/// The message and channel a session is shown in, if known.
pub(crate) fn location(session: &GameSession) -> Option<(u64, u64)> {
    session.message_id.map(|m| (session.channel_id as u64, m as u64))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_ride_in_the_state_and_cannot_break_it() {
        let state = pack("board:1:-", ["Cap|tain\nPinch", "Marlowe"]);
        let (fields, names) = unpack(&state);
        assert_eq!(fields, "board:1:-");
        assert_eq!(names, ["Cap tain Pinch".to_owned(), "Marlowe".to_owned()]);
        let (fields, names) = unpack("no names");
        assert_eq!((fields, names[0].as_str()), ("no names", "Player 1"));
        let long = pack("", ["x".repeat(80).as_str(), "y"]);
        assert_eq!(unpack(&long).1[0].len(), 24);
    }
}
