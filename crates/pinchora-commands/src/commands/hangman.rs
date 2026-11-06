//! `/play hangman`: guess letters from two selects while the tide takes a sandcastle.

use std::str::Split;

use pinchora_core::{i18n::Lang, t};
use pinchora_db::{ACTIVE, GameSession, NewGame, Payout};
use pinchora_game::{
    lang,
    play::{
        Game, PlayContent, find_hangman,
        hangman::{Guess, Hangman, MAX_MISSES, Slot},
    },
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
    let lang = cx.lang();
    if let Some(left) = cx
        .app
        .cache
        .try_cooldown(&format!("hangman:{me}"), Duration::from_secs(5))
        .await?
    {
        return cx
            .reply_ephemeral(t!(lang, "hangman.cooldown", secs = left.as_secs() + 1))
            .await;
    }
    let words = &PlayContent::for_lang(lang).hangman;
    let word = &words[rand::random_range(0..words.len())];
    let game = Hangman::new(&word.word, lang);
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
    let tongue = game.lang();
    let up = |c: char| lang::upper_char(tongue, c);
    let lines: Vec<Vec<HangmanLetter>> = game
        .lines()
        .into_iter()
        .map(|line| {
            line.into_iter()
                .map(|slot| match slot {
                    Slot::Guessed(c) => HangmanLetter::Found(up(c)),
                    Slot::Hidden(c) if lost => HangmanLetter::Missed(up(c)),
                    Slot::Hidden(_) => HangmanLetter::Hidden,
                })
                .collect()
        })
        .collect();
    let mystery = t!(tongue, "hangman.mystery");
    let category = lang::upper(
        tongue,
        find_hangman(game.word()).map_or(mystery.as_str(), |h| h.category.label_in(tongue)),
    );
    let wrong: Vec<char> = game.wrong().into_iter().map(up).collect();
    let misses = game.misses();
    let status = lang::upper(
        tongue,
        &if game.is_won() {
            t!(tongue, "hangman.status-won")
        } else if lost {
            t!(tongue, "hangman.status-lost")
        } else {
            t!(tongue, "hangman.status-playing")
        },
    );
    let png = app
        .render(move |r| {
            r.hangman_card(&HangmanCard {
                lines: &lines,
                category: &category,
                wrong: &wrong,
                misses,
                status: &status,
                lang: tongue,
                theme: Theme::reef(),
            })
        })
        .await?;
    // English shows the word as it is stored; Turkish in capitals with the dotted İ.
    let shown_word = match tongue {
        Lang::En => game.word().to_owned(),
        Lang::Tr => lang::upper(tongue, game.word()),
    };
    let left = MAX_MISSES - misses;
    let user = mention(owner);
    let (title, color, content) = if game.is_won() {
        (
            t!(tongue, "hangman.title-won"),
            SUCCESS,
            t!(tongue, "hangman.won", user = user, word = shown_word, note = note),
        )
    } else if lost {
        (
            t!(tongue, "hangman.title-lost"),
            WARN,
            t!(tongue, "hangman.lost", word = shown_word, note = note),
        )
    } else {
        (
            t!(tongue, "hangman.title-playing"),
            BRAND,
            t!(tongue, "hangman.playing", user = user, count = left),
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

/// The language's letters in two menus (A-M and N-Z in English; 15 and 14 letters in Turkish) of the ones
/// still unguessed, one menu each (a menu with nothing in it is left out).
fn letter_menus(id: i64, game: &Hangman) -> Vec<Component> {
    let tongue = game.lang();
    let letters: Vec<char> = lang::alphabet(tongue).chars().collect();
    let (first, second) = letters.split_at(letters.len().div_ceil(2));
    let range = |half: &[char]| {
        let (a, b) = (half[0], half[half.len() - 1]);
        let (a, b) = (lang::upper_char(tongue, a), lang::upper_char(tongue, b));
        t!(tongue, "hangman.pick-range", from = a, to = b)
    };
    [("a", first), ("n", second)]
        .into_iter()
        .filter_map(|(half, letters)| {
            let placeholder = range(letters);
            let options: Vec<SelectMenuOption> = letters
                .iter()
                .filter(|c| !game.has_guessed(**c))
                .map(|c| SelectMenuOption {
                    default: false,
                    description: None,
                    emoji: None,
                    label: lang::upper_char(tongue, *c).to_string(),
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
                        placeholder: Some(placeholder),
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
        return cx.reply_ephemeral(t!(cx.lang(), "hangman.expired")).await;
    };
    // Only a hangman game: another game's id would pass another module's switch.
    let Some(session) = cx
        .app
        .db
        .game(id)
        .await?
        .filter(|s| s.kind == "hangman" && s.is_active())
    else {
        return cx.reply_ephemeral(t!(cx.lang(), "hangman.over-start")).await;
    };
    if session.host_id as u64 != me {
        return cx
            .reply_ephemeral(t!(cx.lang(), "hangman.not-yours", owner = mention(session.host_id)))
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
            None => cx.reply_ephemeral(t!(cx.lang(), "hangman.over")).await,
        };
    }
    let Some(mut game) = Hangman::decode(&session.state) else {
        anyhow::bail!("bad hangman state {:?}", session.state);
    };
    let tongue = game.lang();
    match game.guess(letter) {
        Guess::Repeat => {
            let shown = lang::upper_char(tongue, letter);
            return cx
                .reply_ephemeral(t!(tongue, "hangman.already-tried", letter = shown))
                .await;
        }
        Guess::Invalid => {
            return cx.reply_ephemeral(t!(tongue, "hangman.bad-pick")).await;
        }
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
            return cx.reply_ephemeral(t!(tongue, "hangman.too-quick")).await;
        }
        record(&cx.app, Game::Hangman, me, game.is_won()).await;
        if game.is_won() {
            let paid = pay_win(&cx.app, me, Game::Hangman, Game::Hangman.win_reward(), "hangman_win").await?;
            note = if paid > 0 {
                t!(tongue, "hangman.paid", paid = paid)
            } else {
                t!(tongue, "hangman.limit-reached")
            };
        }
    } else if !cx.app.db.advance_game(id, &session.state, &next, IDLE_SECS).await? {
        return cx.reply_ephemeral(t!(tongue, "hangman.too-quick")).await;
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
    let game = Hangman::decode(&before.state);
    let tongue = game.as_ref().map_or(Lang::En, Hangman::lang);
    let word = game.map(|g| g.word().to_owned()).unwrap_or_default();
    let shown_word = match tongue {
        Lang::En => word,
        Lang::Tr => lang::upper(tongue, &word),
    };
    Ok(Some(t!(tongue, "hangman.timeout", word = shown_word)))
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
