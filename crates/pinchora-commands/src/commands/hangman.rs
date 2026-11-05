//! `/play hangman`: guess letters from two selects while the tide takes a sandcastle.

use std::str::Split;

use pinchora_db::{ACTIVE, GameSession, NewGame, Payout};
use pinchora_game::play::{
    Game, PlayContent,
    hangman::{Guess, Hangman, MAX_MISSES, Slot},
};
use pinchora_render::{HangmanCard, HangmanLetter, Theme};
use twilight_model::channel::message::{
    Component,
    component::{ActionRow, SelectMenu, SelectMenuOption, SelectMenuType},
};
use twilight_model::http::interaction::InteractionResponseData;
use twilight_util::builder::embed::EmbedBuilder;

use super::games::{channel_of, edit_message, location, pay_win, quiet, record, track_message};
use crate::{
    App, Cx,
    util::{BRAND, SUCCESS, WARN, mention},
};
use std::{sync::Arc, time::Duration};

/// A game left alone this long is closed.
const IDLE_SECS: i64 = 900;

pub(crate) async fn start(cx: &Cx) -> anyhow::Result<()> {
    let me = cx.author()?.id.get();
    if let Some(left) = cx
        .app
        .cache
        .try_cooldown(&format!("hangman:{me}"), Duration::from_secs(5))
        .await?
    {
        return cx
            .reply_ephemeral(format!(
                "One sandcastle at a time! Try again in {} s.",
                left.as_secs() + 1
            ))
            .await;
    }
    let words = &PlayContent::get().hangman;
    let word = &words[rand::random_range(0..words.len())];
    let game = Hangman::new(&word.word);
    let session = cx
        .app
        .db
        .create_game(NewGame {
            kind: "hangman",
            guild_id: cx.guild_id().unwrap_or(0),
            channel_id: channel_of(cx),
            host: me,
            guest: None,
            bet: 0,
            status: ACTIVE,
            state: &game.encode(),
            ttl_secs: IDLE_SECS,
        })
        .await?;
    let data = message(&cx.app, session.id, &game, me, "").await?;
    cx.reply(data).await?;
    track_message(cx, session.id);
    Ok(())
}

/// The picture, the line above it and the two letter menus.
async fn message(
    app: &App,
    id: i64,
    game: &Hangman,
    owner: u64,
    note: &str,
) -> anyhow::Result<InteractionResponseData> {
    let lost = game.is_lost();
    let lines: Vec<Vec<HangmanLetter>> = game
        .lines()
        .into_iter()
        .map(|line| {
            line.into_iter()
                .map(|slot| match slot {
                    Slot::Guessed(c) => HangmanLetter::Found(c),
                    Slot::Hidden(c) if lost => HangmanLetter::Missed(c),
                    Slot::Hidden(_) => HangmanLetter::Hidden,
                })
                .collect()
        })
        .collect();
    let category = PlayContent::get()
        .hangman
        .iter()
        .find(|h| h.word == game.word())
        .map_or("Mystery", |h| h.category.label());
    let wrong = game.wrong();
    let misses = game.misses();
    let status = if game.is_won() {
        "You saved it!".to_owned()
    } else if lost {
        "The sea took it!".to_owned()
    } else {
        "Pick a letter below".to_owned()
    };
    let png = app
        .render(move |r| {
            r.hangman_card(&HangmanCard {
                lines: &lines,
                category,
                wrong: &wrong,
                misses,
                status: &status,
                theme: Theme::reef(),
            })
        })
        .await?;
    let (title, color, content) = if game.is_won() {
        (
            "Sandcastle saved!",
            SUCCESS,
            format!(
                "🎉 {} saved the sandcastle! The word was **{}**. {note}",
                mention(owner),
                game.word()
            ),
        )
    } else if lost {
        (
            "The sea took the castle",
            WARN,
            format!("🌊 The tide won this time. The word was **{}**. {note}", game.word()),
        )
    } else {
        (
            "Save the sandcastle",
            BRAND,
            format!(
                "🏰 {} is saving the sandcastle! **{}** wrong guess{} left.",
                mention(owner),
                MAX_MISSES - misses,
                if MAX_MISSES - misses == 1 { "" } else { "es" }
            ),
        )
    };
    let components = if game.is_over() {
        Vec::new()
    } else {
        letter_menus(id, game)
    };
    let mut data = Cx::card(
        EmbedBuilder::new().color(color).title(title),
        "hangman.png",
        png,
        components,
        false,
    );
    data.content = Some(content);
    data.allowed_mentions = quiet();
    Ok(data)
}

/// Letters A-M and N-Z that are still unguessed, one menu each (a menu with nothing in it is left out).
fn letter_menus(id: i64, game: &Hangman) -> Vec<Component> {
    [
        ("a", 'a'..='m', "Pick a letter (A-M)"),
        ("n", 'n'..='z', "Pick a letter (N-Z)"),
    ]
    .into_iter()
    .filter_map(|(half, letters, placeholder)| {
        let options: Vec<SelectMenuOption> = letters
            .filter(|c| !game.has_guessed(*c))
            .map(|c| SelectMenuOption {
                default: false,
                description: None,
                emoji: None,
                label: c.to_ascii_uppercase().to_string(),
                value: c.to_string(),
            })
            .collect();
        (!options.is_empty()).then(|| {
            Component::ActionRow(ActionRow {
                id: None,
                components: vec![Component::SelectMenu(SelectMenu {
                    channel_types: None,
                    custom_id: format!("hm:{id}:{half}"),
                    default_values: None,
                    disabled: false,
                    id: None,
                    kind: SelectMenuType::Text,
                    max_values: Some(1),
                    min_values: Some(1),
                    options: Some(options),
                    placeholder: Some(placeholder.to_owned()),
                    required: None,
                })],
            })
        })
    })
    .collect()
}

/// `hm:<id>:<half>`: a letter was picked.
pub(crate) async fn on_pick(cx: &Cx, mut args: Split<'_, char>, value: Option<&str>) -> anyhow::Result<()> {
    let me = cx.author()?.id.get();
    let (Some(id), Some(letter)) = (
        args.next().and_then(|v| v.parse::<i64>().ok()),
        value.and_then(|v| v.chars().next()),
    ) else {
        return cx.reply_ephemeral("This game has expired.").await;
    };
    let Some(session) = cx.app.db.game(id).await?.filter(|s| s.is_active()) else {
        return cx
            .reply_ephemeral("This game is over. Start a new one with `/play hangman`!")
            .await;
    };
    if session.host_id as u64 != me {
        return cx
            .reply_ephemeral(format!(
                "This is {}'s sandcastle. Start your own with `/play hangman`!",
                mention(session.host_id)
            ))
            .await;
    }
    if session.expired() {
        return match close(&cx.app, &session).await? {
            Some(text) => {
                cx.update(InteractionResponseData {
                    content: Some(text),
                    components: Some(Vec::new()),
                    allowed_mentions: quiet(),
                    ..Default::default()
                })
                .await
            }
            None => cx.reply_ephemeral("This game is over.").await,
        };
    }
    let Some(mut game) = Hangman::decode(&session.state) else {
        anyhow::bail!("bad hangman state {:?}", session.state);
    };
    match game.guess(letter) {
        Guess::Repeat => {
            return cx
                .reply_ephemeral(format!("You already tried **{}**.", letter.to_ascii_uppercase()))
                .await;
        }
        Guess::Invalid => return cx.reply_ephemeral("Pick a letter from the menus.").await,
        Guess::Hit(_) | Guess::Miss => {}
    }
    let next = game.encode();
    let mut note = String::new();
    if game.is_over() {
        if cx
            .app
            .db
            .settle_game(id, Some(&session.state), &next, Payout::None, false)
            .await?
            .is_none()
        {
            return cx.reply_ephemeral("Too quick! Look at the board and pick again.").await;
        }
        record(&cx.app, Game::Hangman, me, game.is_won()).await;
        if game.is_won() {
            let paid = pay_win(&cx.app, me, Game::Hangman, Game::Hangman.win_reward(), "hangman_win").await?;
            note = if paid > 0 {
                format!("+**{paid}** shells!")
            } else {
                "(Today's reward limit for this game is reached.)".to_owned()
            };
        }
    } else if !cx.app.db.advance_game(id, &session.state, &next, IDLE_SECS).await? {
        return cx.reply_ephemeral("Too quick! Look at the board and pick again.").await;
    }
    let data = message(&cx.app, id, &game, me, &note).await?;
    cx.update(data).await
}

/// Closes a game nobody played for a while; returns what its message should say, or `None` when it
/// was already over.
async fn close(app: &App, session: &GameSession) -> anyhow::Result<Option<String>> {
    let Some(before) = app
        .db
        .settle_game(session.id, None, "timeout", Payout::None, true)
        .await?
    else {
        return Ok(None);
    };
    let word = Hangman::decode(&before.state)
        .map(|g| g.word().to_owned())
        .unwrap_or_default();
    Ok(Some(format!(
        "🌊 The tide came in while nobody was looking. The word was **{word}**."
    )))
}

/// Closes an idle game and says so on its message.
pub(crate) async fn time_out(app: &Arc<App>, session: &GameSession) -> anyhow::Result<bool> {
    let Some(text) = close(app, session).await? else {
        return Ok(false);
    };
    if let Some((channel, message)) = location(session) {
        edit_message(app, channel, message, Some(&text), None, Some(&[])).await;
    }
    Ok(true)
}
