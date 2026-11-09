//! Board and puzzle games. Sea Fleet, Pearl Checkers and Reef Chess are duels for two with an optional
//! wager (`/duel seafleet`, `/duel checkers`, `/duel chess`); Pixel Picross and Tide 2048 are played alone
//! (`/play picross`, `/play 2048`).

use pinchora_core::infra::i18n::Lang;
use pinchora_core::t;
use pinchora_db::GameSession;
use pinchora_game::play::Game;
use twilight_model::{
    channel::message::{
        Component,
        component::{ActionRow, ButtonStyle, SelectMenu, SelectMenuOption, SelectMenuType},
    },
    http::interaction::InteractionResponseData,
    id::Id,
};

use crate::App;
use crate::Cx;
use crate::commands::games::games::button;
use crate::infra::util::thousands_in;

pub(crate) mod checkers;
pub(crate) mod chess;
pub(crate) mod fleet;
pub(crate) mod othello;
pub(crate) mod picross;
pub(crate) mod tide2048;

/// How long both captains have to place their fleets.
pub(crate) const FLEET_PLACE_SECS: i64 = 300;
/// How long a captain has to fire.
pub(crate) const FLEET_TURN_SECS: i64 = 120;
/// How long a player has for one move of checkers.
pub(crate) const CHECKERS_SECS: i64 = 180;
/// How long a player has for one move of chess.
pub(crate) const CHESS_SECS: i64 = 300;

/// The two players of a session: the host, then the guest.
pub(crate) fn players(session: &GameSession) -> [u64; 2] {
    [session.host_id as u64, session.guest_id.unwrap_or(0) as u64]
}

/// A player's name cut short for the status line of a card, where the words that follow it matter more
/// than the end of the name.
pub(crate) fn tag(name: &str) -> String {
    const MAX: usize = 14;
    if name.chars().count() <= MAX {
        return name.to_owned();
    }
    let mut short: String = name.chars().take(MAX - 1).collect();
    short.push('…');
    short
}

/// Ends a two-player game exactly once and settles it: the pot goes to the winner (or both wagers back on
/// a draw), both players' stats are counted and, without a wager, the winner earns the game's reward
/// (limited per day) if the game had `moves` enough (see `Game::min_moves`): a game cut short by an
/// early resignation pays nothing, so two friends cannot trade wins for shells. The final state is the one
/// the game ends in; the session's own state is the one the caller saw, so a move that landed first wins
/// the race. Returns the sentence for the message, in `lang`, or `None` when somebody else ended the game first.
pub(crate) async fn conclude(
    app: &App,
    lang: Lang,
    session: &GameSession,
    final_state: &str,
    game: Game,
    winner: Option<u64>,
    moves: usize,
) -> anyhow::Result<Option<String>> {
    let Some(paid) = app
        .db
        .settle_board_game(
            session,
            final_state,
            winner,
            if moves >= game.min_moves() {
                game.win_reward()
            } else {
                0
            },
            game.daily_cap(),
        )
        .await?
    else {
        return Ok(None);
    };
    let note = match winner {
        Some(_) if session.bet > 0 => t!(lang, "duel.pot-won", pot = thousands_in(lang, session.bet * 2)),
        Some(_) if moves < game.min_moves() => t!(lang, "board.too-short"),
        Some(_) => {
            if paid > 0 {
                t!(lang, "duel.paid", paid = paid)
            } else {
                t!(lang, "duel.limit-reached")
            }
        }
        None if session.bet > 0 => t!(lang, "duel.wagers-back"),
        None => String::new(),
    };
    Ok(Some(note))
}

/// Edits a message by id with the bot's token, pictures included, for changes made by a click on some other
/// message (a private one, say). False when Discord refuses.
pub(crate) async fn edit_card(app: &App, channel: u64, message: u64, data: &InteractionResponseData) -> bool {
    if channel == 0 || message == 0 {
        return false;
    }
    let result = async {
        let mut branded = data.embeds.clone().unwrap_or_default();
        app.brand_embeds_in_channel(channel, &mut branded).await;
        let mut request = app.http.update_message(Id::new(channel), Id::new(message));
        if let Some(content) = &data.content {
            request = request.content(Some(content));
        }
        if data.embeds.is_some() {
            request = request.embeds(Some(&branded));
        }
        if let Some(components) = &data.components {
            request = request.components(Some(components));
        }
        if let Some(attachments) = &data.attachments {
            request = request.attachments(attachments);
        }
        if let Some(mentions) = &data.allowed_mentions {
            request = request.allowed_mentions(Some(mentions));
        }
        request.await?;
        anyhow::Ok(())
    }
    .await;
    if let Err(error) = &result {
        tracing::debug!(%error, channel, message, "game card not edited");
    }
    result.is_ok()
}

/// A select menu in a row of its own.
pub(crate) fn select_row(
    custom_id: String,
    placeholder: &str,
    options: Vec<SelectMenuOption>,
    values: (u8, u8),
) -> Component {
    Component::ActionRow(ActionRow {
        id: None,
        components: vec![Component::SelectMenu(SelectMenu {
            channel_types: None,
            custom_id,
            default_values: None,
            disabled: false,
            id: None,
            kind: SelectMenuType::Text,
            max_values: Some(values.1),
            min_values: Some(values.0),
            options: Some(options),
            placeholder: Some(placeholder.to_owned()),
            required: None,
        })],
    })
}

/// What a select menu holds, at most.
const MENU_MAX: usize = 25;

/// A menu in a row of its own, or several when there are more options than one menu holds (a queen on an
/// empty board has 27 moves). The extra menus' custom ids end in `:2`, `:3`, so every id in a message is
/// its own while the handlers, which read the leading parts only, treat them alike.
pub(crate) fn select_rows(
    lang: Lang,
    custom_id: &str,
    placeholder: &str,
    options: Vec<SelectMenuOption>,
    values: (u8, u8),
) -> Vec<Component> {
    options
        .chunks(MENU_MAX)
        .enumerate()
        .map(|(n, chunk)| {
            if n == 0 {
                select_row(custom_id.to_owned(), placeholder, chunk.to_vec(), values)
            } else {
                select_row(
                    format!("{custom_id}:{}", n + 1),
                    &t!(lang, "board.more", placeholder = placeholder),
                    chunk.to_vec(),
                    values,
                )
            }
        })
        .collect()
}

pub(crate) fn option(
    label: impl Into<String>,
    value: impl Into<String>,
    description: Option<String>,
    default: bool,
) -> SelectMenuOption {
    SelectMenuOption {
        default,
        description,
        emoji: None,
        label: label.into(),
        value: value.into(),
    }
}

/// A row with a "Resign" button.
pub(crate) fn resign_button(lang: Lang, prefix: &str, id: i64) -> Component {
    Component::ActionRow(ActionRow {
        id: None,
        components: vec![button(
            format!("{prefix}:q:{id}"),
            &t!(lang, "board.resign"),
            ButtonStyle::Danger,
            false,
        )],
    })
}

/// The private "are you sure" that follows a click on Resign.
pub(crate) async fn confirm_resign(cx: &Cx, prefix: &str, id: i64) -> anyhow::Result<()> {
    cx.reply(InteractionResponseData {
        content: Some(t!(cx.lang(), "board.confirm-resign")),
        components: Some(vec![Component::ActionRow(ActionRow {
            id: None,
            components: vec![button(
                format!("{prefix}:R:{id}"),
                &t!(cx.lang(), "board.confirm-yes"),
                ButtonStyle::Danger,
                false,
            )],
        })]),
        flags: Some(twilight_model::channel::message::MessageFlags::EPHEMERAL),
        ..Default::default()
    })
    .await
}

/// Answers a click on the private "Yes, resign" message once the game is over.
pub(crate) async fn resigned(cx: &Cx, text: &str) -> anyhow::Result<()> {
    cx.update(InteractionResponseData {
        content: Some(text.to_owned()),
        components: Some(Vec::new()),
        ..Default::default()
    })
    .await
}

#[cfg(test)]
mod tests {
    use crate::commands::games::board::*;

    fn menus(rows: &[Component]) -> Vec<&SelectMenu> {
        rows.iter()
            .filter_map(|row| match row {
                Component::ActionRow(row) => match row.components.first() {
                    Some(Component::SelectMenu(menu)) => Some(menu),
                    _ => None,
                },
                _ => None,
            })
            .collect()
    }

    #[test]
    fn a_menu_with_too_many_options_becomes_several_with_their_own_ids() {
        let options = |n: usize| {
            (0..n)
                .map(|i| option(format!("o{i}"), i.to_string(), None, false))
                .collect()
        };
        let one = select_rows(Lang::En, "cc:t:1:0:d4", "Where?", options(25), (1, 1));
        assert_eq!(menus(&one).len(), 1);
        let two = select_rows(Lang::En, "cc:t:1:0:d4", "Where?", options(27), (1, 1));
        let menus = menus(&two);
        assert_eq!(menus.len(), 2);
        assert_eq!(menus[0].options.as_ref().map(Vec::len), Some(25));
        assert_eq!(menus[1].options.as_ref().map(Vec::len), Some(2));
        assert_eq!(menus[0].custom_id, "cc:t:1:0:d4");
        assert_eq!(menus[1].custom_id, "cc:t:1:0:d4:2");
        assert_eq!(menus[1].placeholder.as_deref(), Some("Where? (more)"));
        assert!(select_rows(Lang::En, "x", "y", Vec::new(), (1, 1)).is_empty());
    }

    #[test]
    fn long_names_are_cut_for_the_status_line() {
        assert_eq!(tag("Marlowe"), "Marlowe");
        assert_eq!(tag("exactly14chars"), "exactly14chars");
        assert_eq!(tag("Captain Pinchora of the Reef"), "Captain Pinch…");
        assert_eq!(tag("Captain Pinchora of the Reef").chars().count(), 14);
    }

    #[test]
    fn players_are_the_host_then_the_guest() {
        let session = GameSession {
            id: 1,
            kind: "chess".to_owned(),
            guild_id: 2,
            channel_id: 3,
            message_id: None,
            host_id: 10,
            guest_id: Some(20),
            bet: 0,
            status: "active".to_owned(),
            state: String::new(),
            expires_at: chrono::Utc::now(),
        };
        assert_eq!(players(&session), [10, 20]);
    }
}
