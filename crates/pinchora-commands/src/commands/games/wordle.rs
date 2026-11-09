//! `/play wordle`: five letters, six tries, typed into a modal. The daily word is the same for everyone.

use std::{str::Split, time::Duration};

use pinchora_core::infra::i18n::Lang;
use pinchora_core::t;
use pinchora_db::WordleGame;
use pinchora_game::lang;
use pinchora_game::play::Game;
use pinchora_game::play::PlayContent;
use pinchora_game::play::wordle;
use pinchora_game::play::wordle::MAX_TRIES;
use pinchora_game::play::wordle::Tile;
use pinchora_render::{ABSENT, CORRECT, PRESENT, Theme, WordleCard, WordleRow};
use twilight_model::{
    channel::message::{
        Component,
        component::{ButtonStyle, Label, TextInput, TextInputStyle},
    },
    http::interaction::InteractionResponseData,
};
use twilight_util::builder::embed::EmbedBuilder;

use crate::App;
use crate::Cx;
use crate::commands::games::games::button_row;
use crate::commands::games::games::pay_win;
use crate::commands::games::games::quiet;
use crate::commands::games::games::record;
use crate::commands::games::games::today;
use crate::infra::util::BRAND;
use crate::infra::util::SUCCESS;
use crate::infra::util::WARN;
use crate::infra::util::mention;

const DAILY: &str = "daily";
const PRACTICE: &str = "practice";
/// Custom id of the guess text input in the modal.
pub(crate) const GUESS_FIELD: &str = "guess";

/// The mode a board is kept under: Turkish boards have their own daily word and practice board.
fn mode_name(base: &str, lang: Lang) -> String {
    match lang {
        Lang::En => base.to_owned(),
        Lang::Tr => format!("{base}-tr"),
    }
}

/// The kind of board (`daily` or `practice`) and the language of a stored mode.
fn parse_mode(mode: &str) -> Option<(&'static str, Lang)> {
    match mode {
        DAILY => Some((DAILY, Lang::En)),
        PRACTICE => Some((PRACTICE, Lang::En)),
        "daily-tr" => Some((DAILY, Lang::Tr)),
        "practice-tr" => Some((PRACTICE, Lang::Tr)),
        _ => None,
    }
}

fn mark(tile: Tile) -> u8 {
    match tile {
        Tile::Absent => ABSENT,
        Tile::Present => PRESENT,
        Tile::Correct => CORRECT,
    }
}

/// `/play wordle`: today's shared word, or a random practice word.
pub(crate) async fn start(cx: &Cx, daily: bool) -> anyhow::Result<()> {
    let me = cx.author()?.id.get();
    let lang = cx.lang();
    let game = if daily {
        let day = today() as i32;
        let word = wordle::daily_word(&PlayContent::for_lang(lang).words.answers, i64::from(day));
        cx.app
            .db
            .start_wordle(me, &mode_name(DAILY, lang), day, word, false)
            .await?
    } else {
        match cx
            .app
            .db
            .wordle_game(me, &mode_name(PRACTICE, lang), 0)
            .await?
            .filter(|g| g.status == "playing")
        {
            Some(unfinished) => unfinished,
            None => new_practice(&cx.app, me, lang).await?.0,
        }
    };
    let data = view(&cx.app, &game, "").await?;
    cx.reply(data).await
}

/// A fresh practice board in `lang`, and whether it replaced one that was being played (a guess in, not
/// finished).
async fn new_practice(app: &App, user: u64, lang: Lang) -> anyhow::Result<(WordleGame, bool)> {
    let answers = &PlayContent::for_lang(lang).words.answers;
    let word = &answers[rand::random_range(0..answers.len())];
    app.db
        .restart_practice_wordle(user, &mode_name(PRACTICE, lang), word)
        .await
}

/// The private board: tiles, keyboard and the button for what comes next.
async fn view(app: &App, game: &WordleGame, note: &str) -> anyhow::Result<InteractionResponseData> {
    let (kind, lang) = parse_mode(&game.mode).unwrap_or((DAILY, Lang::En));
    let daily = kind == DAILY;
    let scored: Vec<(&str, [Tile; 5])> = game
        .guesses
        .iter()
        .map(|g| (g.as_str(), wordle::score(g, &game.word)))
        .collect();
    let rows: Vec<WordleRow> = scored
        .iter()
        .map(|(word, tiles)| WordleRow {
            letters: word
                .chars()
                .map(|c| lang::upper_char(lang, c))
                .collect::<Vec<_>>()
                .try_into()
                .unwrap_or(['?'; 5]),
            marks: tiles.map(mark),
        })
        .collect();
    let best = wordle::keyboard(scored.iter().copied());
    let keyboard: Vec<Vec<(char, u8)>> = lang::keyboard_rows(lang)
        .iter()
        .map(|row| {
            row.chars()
                .map(|c| (lang::upper_char(lang, c), best.get(&c).map_or(0, |t| mark(*t))))
                .collect()
        })
        .collect();
    let tries = game.guesses.len();
    let title = if daily {
        t!(
            lang,
            "wordle.title-daily",
            number = wordle::puzzle_number(i64::from(game.day))
        )
    } else {
        t!(lang, "wordle.title-practice")
    };
    // English shows the word as it is stored; Turkish in capitals with the dotted İ.
    let shown_word = match lang {
        Lang::En => game.word.clone(),
        Lang::Tr => lang.upper(&game.word),
    };
    let footer = match game.status.as_str() {
        "won" => t!(lang, "wordle.footer-won", tries = tries, max = MAX_TRIES),
        "lost" => t!(lang, "wordle.footer-lost", word = shown_word),
        _ => t!(lang, "wordle.footer-playing", number = tries + 1, max = MAX_TRIES),
    };
    let png = {
        let (title, footer) = (lang.upper(&title), lang.upper(&footer));
        app.render(move |r| {
            r.wordle_card(&WordleCard {
                title: &title,
                rows: &rows,
                keyboard: &keyboard,
                footer: &footer,
                theme: Theme::reef(),
            })
        })
        .await?
    };
    let (color, text) = match game.status.as_str() {
        "won" => (SUCCESS, t!(lang, "wordle.won", tries = tries, note = note)),
        "lost" => (WARN, t!(lang, "wordle.lost", word = shown_word, note = note)),
        _ => (BRAND, t!(lang, "wordle.playing", note = note)),
    };
    let components = match (game.status.as_str(), daily) {
        ("playing", _) => vec![button_row(
            format!("wo:g:{}:{}", game.mode, game.day),
            &t!(lang, "wordle.guess"),
            ButtonStyle::Primary,
        )],
        (_, true) => vec![button_row(
            format!("wo:s:{}:{}", game.mode, game.day),
            &t!(lang, "wordle.share"),
            ButtonStyle::Success,
        )],
        (_, false) => vec![button_row(
            format!("wo:n:{}", game.mode),
            &t!(lang, "wordle.new-word"),
            ButtonStyle::Primary,
        )],
    };
    Ok(Cx::card(
        EmbedBuilder::new().color(color).title(title).description(text),
        "wordle.png",
        png,
        components,
        true,
    ))
}

/// `wo:g:<mode>:<day>` opens the guess form, `wo:s:<mode>:<day>` shares a finished daily, `wo:n:<mode>`
/// starts a new practice word. (Buttons from before the Turkish boards have no mode in the last two and mean
/// the English one.)
pub(crate) async fn on_button(cx: &Cx, mut args: Split<'_, char>) -> anyhow::Result<()> {
    let me = cx.author()?.id.get();
    match args.next() {
        Some("g") => {
            let (mode, day) = (args.next().unwrap_or_default(), args.next().unwrap_or_default());
            let Ok(day_no) = day.parse::<i32>() else {
                return cx.reply_ephemeral(t!(cx.lang(), "wordle.board-expired")).await;
            };
            let Some((kind, lang)) = parse_mode(mode) else {
                return cx.reply_ephemeral(t!(cx.lang(), "wordle.board-expired")).await;
            };
            if kind == DAILY && i64::from(day_no) != today() {
                return cx.reply_ephemeral(t!(lang, "wordle.puzzle-ended")).await;
            }
            // The form opens first, before anything is read: over the HTTP endpoint a slow read would make the
            // responder defer, and a modal can only be the first answer. A board that is over is told when the
            // guess is sent (`on_guess`).
            cx.modal(
                format!("wo:m:{mode}:{day_no}"),
                &t!(lang, "wordle.modal-title"),
                vec![guess_field(lang)],
            )
            .await
        }
        Some("n") => {
            let lang = args.next().and_then(parse_mode).map_or(Lang::En, |(_, lang)| lang);
            let (game, abandoned) = new_practice(&cx.app, me, lang).await?;
            if abandoned {
                // The board it replaced was being played: walking away from it is a lost game.
                record(&cx.app, Game::Wordle, me, false).await;
            }
            let data = view(&cx.app, &game, "").await?;
            cx.update(data).await
        }
        Some("s") => {
            let first = args.next().unwrap_or_default();
            let (mode, day) = match first.parse::<i32>() {
                Ok(day) => (DAILY, Some(day)),
                Err(_) => (first, args.next().and_then(|v| v.parse::<i32>().ok())),
            };
            let (Some(day), Some(_)) = (day, parse_mode(mode)) else {
                return cx.reply_ephemeral(t!(cx.lang(), "wordle.board-expired")).await;
            };
            share(cx, me, mode, day).await
        }
        _ => cx.reply_ephemeral(t!(cx.lang(), "wordle.button-expired")).await,
    }
}

#[allow(deprecated)] // the label belongs on the Label component, so the input's own stays empty
fn guess_field(lang: Lang) -> Component {
    Component::Label(Label {
        component: Box::new(Component::TextInput(TextInput {
            custom_id: GUESS_FIELD.to_owned(),
            id: None,
            label: None,
            max_length: Some(5),
            min_length: Some(5),
            placeholder: Some(t!(lang, "wordle.modal-placeholder")),
            required: Some(true),
            style: TextInputStyle::Short,
            value: None,
        })),
        description: None,
        id: None,
        label: t!(lang, "wordle.modal-label"),
    })
}

/// Posts a finished daily to the channel: the number, the tries and coloured squares, no letters.
async fn share(cx: &Cx, me: u64, mode: &str, day: i32) -> anyhow::Result<()> {
    let lang = parse_mode(mode).map_or(Lang::En, |(_, lang)| lang);
    let Some(game) = cx
        .app
        .db
        .wordle_game(me, mode, day)
        .await?
        .filter(|g| g.status != "playing")
    else {
        return cx.reply_ephemeral(t!(lang, "wordle.finish-first")).await;
    };
    if !cx
        .app
        .cache
        .once(
            &format!("wordleshare:{me}:{mode}:{day}"),
            Duration::from_secs(2 * 86_400),
        )
        .await?
    {
        return cx.reply_ephemeral(t!(lang, "wordle.already-shared")).await;
    }
    let rows: Vec<[Tile; 5]> = game.guesses.iter().map(|g| wordle::score(g, &game.word)).collect();
    let solved = (game.status == "won").then_some(game.guesses.len());
    cx.reply(InteractionResponseData {
        content: Some(format!(
            "{}\n{}",
            t!(lang, "wordle.shared", user = mention(me)),
            wordle::share_text(lang, wordle::puzzle_number(i64::from(day)), solved, &rows)
        )),
        allowed_mentions: quiet(),
        ..Default::default()
    })
    .await
}

/// The guess form was submitted: `wo:m:<mode>:<day>`.
pub(crate) async fn on_guess(cx: &Cx, mut args: Split<'_, char>, text: &str) -> anyhow::Result<()> {
    let me = cx.author()?.id.get();
    let (mode, day) = (
        args.next().unwrap_or_default(),
        args.next().and_then(|v| v.parse::<i32>().ok()),
    );
    let (Some(day), Some((kind, lang))) = (day, parse_mode(mode)) else {
        return cx.reply_ephemeral(t!(cx.lang(), "wordle.board-expired")).await;
    };
    let Some(guess) = wordle::normalize(lang, text) else {
        return cx.reply_ephemeral(t!(lang, "wordle.bad-guess")).await;
    };
    if !PlayContent::for_lang(lang).words.is_allowed(&guess) {
        return cx
            .reply_ephemeral(t!(lang, "wordle.not-listed", word = lang.upper(&guess)))
            .await;
    }
    let Some(game) = cx.app.db.wordle_guess(me, mode, day, &guess).await? else {
        return cx.reply_ephemeral(t!(lang, "wordle.board-finished")).await;
    };
    let mut note = String::new();
    if game.status != "playing" {
        let won = game.status == "won";
        record(&cx.app, Game::Wordle, me, won).await;
        if won {
            let tries = game.guesses.len();
            // A daily played before the player deleted their data was paid then; so is one won in the
            // other language the same day: the daily pays once a day.
            let replayed = kind == DAILY && cx.app.db.daily_wordle_played_before_deleting(me, day).await?;
            let other = mode_name(DAILY, if lang == Lang::En { Lang::Tr } else { Lang::En });
            let twin = kind == DAILY
                && cx
                    .app
                    .db
                    .wordle_game(me, &other, day)
                    .await?
                    .is_some_and(|g| g.status == "won");
            let paid = if replayed || twin {
                0
            } else if kind == DAILY {
                let shells = wordle::reward(true, tries);
                cx.app.db.earn_shells(me, shells, "wordle_daily").await?;
                shells
            } else {
                pay_win(&cx.app, me, Game::Wordle, wordle::reward(false, tries), "wordle_win").await?
            };
            note = if paid > 0 {
                t!(lang, "wordle.paid", paid = paid)
            } else if twin {
                t!(lang, "wordle.other-language")
            } else if replayed {
                t!(lang, "wordle.replayed")
            } else {
                t!(lang, "wordle.limit-reached")
            };
        }
    }
    let data = view(&cx.app, &game, &note).await?;
    cx.update(data).await
}
