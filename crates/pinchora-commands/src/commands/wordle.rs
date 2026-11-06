//! `/play wordle`: five letters, six tries, typed into a modal. The daily word is the same for everyone.

use std::{str::Split, time::Duration};

use pinchora_core::i18n::Lang;
use pinchora_db::WordleGame;
use pinchora_game::{
    lang,
    play::{
        Game, PlayContent,
        wordle::{self, MAX_TRIES, Tile},
    },
};
use pinchora_render::{ABSENT, CORRECT, PRESENT, Theme, WordleCard, WordleRow};
use twilight_model::{
    channel::message::{
        Component,
        component::{ButtonStyle, Label, TextInput, TextInputStyle},
    },
    http::interaction::InteractionResponseData,
};
use twilight_util::builder::embed::EmbedBuilder;

use super::games::{button_row, lang_of, pay_win, quiet, record, today};
use crate::{
    App, Cx,
    util::{BRAND, SUCCESS, WARN, mention},
};

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

/// English or Turkish text, by the board's language.
fn say<'a>(lang: Lang, en: &'a str, tr: &'a str) -> &'a str {
    if lang == Lang::Tr { tr } else { en }
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
    let lang = lang_of(cx);
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
            None => new_practice(&cx.app, me, lang).await?,
        }
    };
    let data = view(&cx.app, &game, "").await?;
    cx.reply(data).await
}

async fn new_practice(app: &App, user: u64, lang: Lang) -> anyhow::Result<WordleGame> {
    let answers = &PlayContent::for_lang(lang).words.answers;
    let word = &answers[rand::random_range(0..answers.len())];
    app.db
        .start_wordle(user, &mode_name(PRACTICE, lang), 0, word, true)
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
    let title = match (daily, lang) {
        (true, Lang::En) => format!("Daily wordle #{}", wordle::puzzle_number(i64::from(game.day))),
        (true, Lang::Tr) => format!("Günlük wordle #{}", wordle::puzzle_number(i64::from(game.day))),
        (false, Lang::En) => "Practice wordle".to_owned(),
        (false, Lang::Tr) => "Serbest wordle".to_owned(),
    };
    let shown_word = lang::upper(lang, &game.word);
    let footer = match game.status.as_str() {
        "won" => match lang {
            Lang::En => format!("Solved in {tries}/{MAX_TRIES}!"),
            Lang::Tr => format!("{tries}/{MAX_TRIES} ile çözüldü!"),
        },
        "lost" => match lang {
            Lang::En => format!("The word was {}", game.word),
            Lang::Tr => format!("Kelime: {shown_word}"),
        },
        _ => match lang {
            Lang::En => format!("Try {} of {MAX_TRIES}", tries + 1),
            Lang::Tr => format!("Deneme {} / {MAX_TRIES}", tries + 1),
        },
    };
    let png = {
        let (title, footer) = (lang::upper(lang, &title), lang::upper(lang, &footer));
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
    let (color, text) = match (game.status.as_str(), lang) {
        ("won", Lang::En) => (SUCCESS, format!("You got it in **{tries}**! {note}")),
        ("won", Lang::Tr) => (SUCCESS, format!("**{tries}** denemede buldun! {note}")),
        ("lost", Lang::En) => (WARN, format!("Out of tries. The word was **{}**. {note}", game.word)),
        ("lost", Lang::Tr) => (WARN, format!("Hakkın bitti. Kelime **{shown_word}** idi. {note}")),
        (_, Lang::En) => (
            BRAND,
            format!("Guess the five-letter word. 🟩 right spot · 🟨 wrong spot · ⬛ not in the word.\n{note}"),
        ),
        (_, Lang::Tr) => (
            BRAND,
            format!("Beş harfli kelimeyi tahmin et. 🟩 doğru yer · 🟨 yanlış yer · ⬛ kelimede yok.\n{note}"),
        ),
    };
    let components = match (game.status.as_str(), daily) {
        ("playing", _) => vec![button_row(
            format!("wo:g:{}:{}", game.mode, game.day),
            say(lang, "Guess", "Tahmin et"),
            ButtonStyle::Primary,
        )],
        (_, true) => vec![button_row(
            format!("wo:s:{}:{}", game.mode, game.day),
            say(lang, "Share result", "Sonucu paylaş"),
            ButtonStyle::Success,
        )],
        (_, false) => vec![button_row(
            format!("wo:n:{}", game.mode),
            say(lang, "New word", "Yeni kelime"),
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
                return cx.reply_ephemeral("This board has expired.").await;
            };
            let Some((kind, lang)) = parse_mode(mode) else {
                return cx.reply_ephemeral("This board has expired.").await;
            };
            if kind == DAILY && i64::from(day_no) != today() {
                return cx
                    .reply_ephemeral(say(
                        lang,
                        "That puzzle has ended. Start today's with `/play wordle`!",
                        "Bu bulmaca sona erdi. Bugünküyü `/play wordle` ile başlat!",
                    ))
                    .await;
            }
            match cx.app.db.wordle_game(me, mode, day_no).await? {
                Some(game) if game.status == "playing" => {
                    cx.modal(
                        format!("wo:m:{mode}:{day_no}"),
                        say(lang, "Wordle guess", "Wordle tahmini"),
                        vec![guess_field(lang)],
                    )
                    .await
                }
                _ => {
                    cx.reply_ephemeral(say(
                        lang,
                        "This board is finished. Start another with `/play wordle`!",
                        "Bu tahta bitti. `/play wordle` ile yenisini başlat!",
                    ))
                    .await
                }
            }
        }
        Some("n") => {
            let lang = args.next().and_then(parse_mode).map_or(Lang::En, |(_, lang)| lang);
            let game = new_practice(&cx.app, me, lang).await?;
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
                return cx.reply_ephemeral("This board has expired.").await;
            };
            share(cx, me, mode, day).await
        }
        _ => cx.reply_ephemeral("This button has expired.").await,
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
            placeholder: Some(say(lang, "pearl", "inci").to_owned()),
            required: Some(true),
            style: TextInputStyle::Short,
            value: None,
        })),
        description: None,
        id: None,
        label: say(lang, "Your five-letter guess", "Beş harfli tahminin").to_owned(),
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
        return cx
            .reply_ephemeral(say(
                lang,
                "Finish today's wordle first!",
                "Önce bugünkü kelimeyi bitir!",
            ))
            .await;
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
        return cx
            .reply_ephemeral(say(
                lang,
                "You already shared this result.",
                "Bu sonucu zaten paylaştın.",
            ))
            .await;
    }
    let rows: Vec<[Tile; 5]> = game.guesses.iter().map(|g| wordle::score(g, &game.word)).collect();
    let solved = (game.status == "won").then_some(game.guesses.len());
    cx.reply(InteractionResponseData {
        content: Some(format!(
            "{} {}\n{}",
            mention(me),
            say(lang, "played the daily wordle.", "günlük kelime oyununu oynadı."),
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
        return cx.reply_ephemeral("This board has expired.").await;
    };
    let Some(guess) = wordle::normalize(lang, text) else {
        return cx
            .reply_ephemeral(say(
                lang,
                "A guess is exactly five letters, a to z.",
                "Bir tahmin tam olarak beş harf olmalı (Türk alfabesi).",
            ))
            .await;
    };
    if !PlayContent::for_lang(lang).words.is_allowed(&guess) {
        return cx
            .reply_ephemeral(match lang {
                Lang::En => format!("**{}** isn't in my word list. Try another word!", guess.to_uppercase()),
                Lang::Tr => format!(
                    "**{}** kelime listemde yok. Başka bir kelime dene!",
                    lang::upper(lang, &guess)
                ),
            })
            .await;
    }
    let Some(game) = cx.app.db.wordle_guess(me, mode, day, &guess).await? else {
        return cx
            .reply_ephemeral(say(
                lang,
                "This board is finished. Start another with `/play wordle`!",
                "Bu tahta bitti. `/play wordle` ile yenisini başlat!",
            ))
            .await;
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
                match lang {
                    Lang::En => format!("+**{paid}** shells!"),
                    Lang::Tr => format!("+**{paid}** deniz kabuğu!"),
                }
            } else if twin {
                say(
                    lang,
                    "(You already earned today's daily reward in the other language.)",
                    "(Bugünün günlük ödülünü diğer dilde zaten aldın.)",
                )
                .to_owned()
            } else if replayed {
                say(
                    lang,
                    "(You played today's daily before deleting your data, so it pays nothing again.)",
                    "(Bugünkü günlüğü verilerini silmeden önce oynadın, bu yüzden tekrar ödül vermez.)",
                )
                .to_owned()
            } else {
                say(
                    lang,
                    "(Today's reward limit for practice is reached.)",
                    "(Alıştırma için bugünkü ödül sınırına ulaşıldı.)",
                )
                .to_owned()
            };
        }
    }
    let data = view(&cx.app, &game, &note).await?;
    cx.update(data).await
}
