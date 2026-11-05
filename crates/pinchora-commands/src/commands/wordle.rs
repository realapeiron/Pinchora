//! `/play wordle`: five letters, six tries, typed into a modal. The daily word is the same for everyone.

use std::{str::Split, time::Duration};

use pinchora_db::WordleGame;
use pinchora_game::play::{
    Game, PlayContent,
    wordle::{self, MAX_TRIES, Tile},
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

use super::games::{button_row, pay_win, quiet, record, today};
use crate::{
    App, Cx,
    util::{BRAND, SUCCESS, WARN, mention},
};

const DAILY: &str = "daily";
const PRACTICE: &str = "practice";

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
    let game = if daily {
        let day = today() as i32;
        let word = wordle::daily_word(&PlayContent::get().words.answers, i64::from(day));
        cx.app.db.start_wordle(me, DAILY, day, word, false).await?
    } else {
        match cx
            .app
            .db
            .wordle_game(me, PRACTICE, 0)
            .await?
            .filter(|g| g.status == "playing")
        {
            Some(unfinished) => unfinished,
            None => new_practice(&cx.app, me).await?,
        }
    };
    let data = view(&cx.app, &game, "").await?;
    cx.reply(data).await
}

async fn new_practice(app: &App, user: u64) -> anyhow::Result<WordleGame> {
    let answers = &PlayContent::get().words.answers;
    let word = &answers[rand::random_range(0..answers.len())];
    app.db.start_wordle(user, PRACTICE, 0, word, true).await
}

/// The private board: tiles, keyboard and the button for what comes next.
async fn view(app: &App, game: &WordleGame, note: &str) -> anyhow::Result<InteractionResponseData> {
    let daily = game.mode == DAILY;
    let scored: Vec<(&str, [Tile; 5])> = game
        .guesses
        .iter()
        .map(|g| (g.as_str(), wordle::score(g, &game.word)))
        .collect();
    let rows: Vec<WordleRow> = scored
        .iter()
        .map(|(word, tiles)| WordleRow {
            letters: word.chars().collect::<Vec<_>>().try_into().unwrap_or(['?'; 5]),
            marks: tiles.map(mark),
        })
        .collect();
    let mut keys = [0u8; 26];
    for (i, key) in wordle::keyboard(scored.iter().copied()).iter().enumerate() {
        keys[i] = key.map_or(0, mark);
    }
    let tries = game.guesses.len();
    let title = if daily {
        format!("Daily wordle #{}", wordle::puzzle_number(i64::from(game.day)))
    } else {
        "Practice wordle".to_owned()
    };
    let footer = match game.status.as_str() {
        "won" => format!("Solved in {tries}/{MAX_TRIES}!"),
        "lost" => format!("The word was {}", game.word),
        _ => format!("Try {} of {MAX_TRIES}", tries + 1),
    };
    let png = {
        let (title, footer) = (title.clone(), footer.clone());
        app.render(move |r| {
            r.wordle_card(&WordleCard {
                title: &title,
                rows: &rows,
                keys,
                footer: &footer,
                theme: Theme::reef(),
            })
        })
        .await?
    };
    let (color, text) = match game.status.as_str() {
        "won" => (SUCCESS, format!("You got it in **{tries}**! {note}")),
        "lost" => (WARN, format!("Out of tries. The word was **{}**. {note}", game.word)),
        _ => (
            BRAND,
            format!("Guess the five-letter word. 🟩 right spot · 🟨 wrong spot · ⬛ not in the word.\n{note}"),
        ),
    };
    let components = match (game.status.as_str(), daily) {
        ("playing", _) => vec![button_row(
            format!("wo:g:{}:{}", game.mode, game.day),
            "Guess",
            ButtonStyle::Primary,
        )],
        (_, true) => vec![button_row(
            format!("wo:s:{}", game.day),
            "Share result",
            ButtonStyle::Success,
        )],
        (_, false) => vec![button_row("wo:n".to_owned(), "New word", ButtonStyle::Primary)],
    };
    Ok(Cx::card(
        EmbedBuilder::new().color(color).title(title).description(text),
        "wordle.png",
        png,
        components,
        true,
    ))
}

/// `wo:g:<mode>:<day>` opens the guess form, `wo:s:<day>` shares a finished daily, `wo:n` starts a new
/// practice word.
pub(crate) async fn on_button(cx: &Cx, mut args: Split<'_, char>) -> anyhow::Result<()> {
    let me = cx.author()?.id.get();
    match args.next() {
        Some("g") => {
            let (mode, day) = (args.next().unwrap_or_default(), args.next().unwrap_or_default());
            let Ok(day_no) = day.parse::<i32>() else {
                return cx.reply_ephemeral("This board has expired.").await;
            };
            if mode != DAILY && mode != PRACTICE {
                return cx.reply_ephemeral("This board has expired.").await;
            }
            if mode == DAILY && i64::from(day_no) != today() {
                return cx
                    .reply_ephemeral("That puzzle has ended. Start today's with `/play wordle`!")
                    .await;
            }
            match cx.app.db.wordle_game(me, mode, day_no).await? {
                Some(game) if game.status == "playing" => cx.modal(guess_form(mode, day_no)).await,
                _ => {
                    cx.reply_ephemeral("This board is finished. Start another with `/play wordle`!")
                        .await
                }
            }
        }
        Some("n") => {
            let game = new_practice(&cx.app, me).await?;
            let data = view(&cx.app, &game, "").await?;
            cx.update(data).await
        }
        Some("s") => {
            let Some(day) = args.next().and_then(|v| v.parse::<i32>().ok()) else {
                return cx.reply_ephemeral("This board has expired.").await;
            };
            share(cx, me, day).await
        }
        _ => cx.reply_ephemeral("This button has expired.").await,
    }
}

#[allow(deprecated)] // the label belongs on the Label component, so the input's own stays empty
fn guess_form(mode: &str, day: i32) -> InteractionResponseData {
    InteractionResponseData {
        custom_id: Some(format!("wo:m:{mode}:{day}")),
        title: Some("Wordle guess".to_owned()),
        components: Some(vec![Component::Label(Label {
            component: Box::new(Component::TextInput(TextInput {
                custom_id: "guess".to_owned(),
                id: None,
                label: None,
                max_length: Some(5),
                min_length: Some(5),
                placeholder: Some("pearl".to_owned()),
                required: Some(true),
                style: TextInputStyle::Short,
                value: None,
            })),
            description: None,
            id: None,
            label: "Your five-letter guess".to_owned(),
        })]),
        ..Default::default()
    }
}

/// Posts a finished daily to the channel: the number, the tries and coloured squares, no letters.
async fn share(cx: &Cx, me: u64, day: i32) -> anyhow::Result<()> {
    let Some(game) = cx
        .app
        .db
        .wordle_game(me, DAILY, day)
        .await?
        .filter(|g| g.status != "playing")
    else {
        return cx.reply_ephemeral("Finish today's wordle first!").await;
    };
    if !cx
        .app
        .cache
        .once(&format!("wordleshare:{me}:{day}"), Duration::from_secs(2 * 86_400))
        .await?
    {
        return cx.reply_ephemeral("You already shared this result.").await;
    }
    let rows: Vec<[Tile; 5]> = game.guesses.iter().map(|g| wordle::score(g, &game.word)).collect();
    let solved = (game.status == "won").then_some(game.guesses.len());
    cx.reply(InteractionResponseData {
        content: Some(format!(
            "{} played the daily wordle.\n{}",
            mention(me),
            wordle::share_text(wordle::puzzle_number(i64::from(day)), solved, &rows)
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
    let Some(day) = day.filter(|_| mode == DAILY || mode == PRACTICE) else {
        return cx.reply_ephemeral("This board has expired.").await;
    };
    let Some(guess) = wordle::normalize(text) else {
        return cx.reply_ephemeral("A guess is exactly five letters, a to z.").await;
    };
    if !PlayContent::get().words.is_allowed(&guess) {
        return cx
            .reply_ephemeral(format!(
                "**{}** isn't in my word list. Try another word!",
                guess.to_uppercase()
            ))
            .await;
    }
    let Some(game) = cx.app.db.wordle_guess(me, mode, day, &guess).await? else {
        return cx
            .reply_ephemeral("This board is finished. Start another with `/play wordle`!")
            .await;
    };
    let mut note = String::new();
    if game.status != "playing" {
        let won = game.status == "won";
        record(&cx.app, Game::Wordle, me, won).await;
        if won {
            let tries = game.guesses.len();
            let paid = if mode == DAILY {
                let shells = wordle::reward(true, tries);
                cx.app.db.earn_shells(me, shells, "wordle_daily").await?;
                shells
            } else {
                pay_win(&cx.app, me, Game::Wordle, wordle::reward(false, tries), "wordle_win").await?
            };
            note = if paid > 0 {
                format!("+**{paid}** shells!")
            } else {
                "(Today's reward limit for practice is reached.)".to_owned()
            };
        }
    }
    let data = view(&cx.app, &game, &note).await?;
    cx.update(data).await
}
