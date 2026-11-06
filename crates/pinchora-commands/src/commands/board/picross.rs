//! `/play picross`: Pixel Picross. Ten rows and ten columns of clues hide a sea creature; mark the cells the
//! clues say are filled and the picture shows. One puzzle a day is the same for everyone; practice puzzles
//! are as many as you like. Marks are set a row at a time from two menus, or typed as coordinates. A puzzle
//! can be checked a few times, which only says how many marks are wrong, and pays less once it was.

use std::{str::Split, time::Duration};

use pinchora_core::{i18n::Lang, t};
use pinchora_db::PicrossGame;
use pinchora_game::{
    lang,
    play::{
        Game,
        picross::{self, CELLS, Clues, Grid, Ink, Puzzle, SIZE, Shade, parse_marks},
    },
};
use pinchora_render::{PicrossCard, Reveal, Theme};
use twilight_model::{
    channel::message::{
        Component,
        component::{ActionRow, ButtonStyle, Label, TextInput, TextInputStyle},
    },
    http::interaction::InteractionResponseData,
};
use twilight_util::builder::embed::EmbedBuilder;

use super::{option, select_row};
use crate::{
    App, Cx,
    commands::games::{button, pay_win, quiet, record},
    util::{BRAND, SUCCESS, mention},
};

const DAILY: &str = "daily";
const PRACTICE: &str = "practice";
/// How many times a puzzle can be checked.
pub(crate) const MAX_CHECKS: i16 = 3;
/// What the daily puzzle pays once it was checked, whatever the streak.
pub(crate) const CHECKED_DAILY_REWARD: i64 = 15;
/// What a practice puzzle pays once it was checked (an unchecked one pays the game's win reward).
pub(crate) const CHECKED_PRACTICE_REWARD: i64 = 5;
/// Custom id of the text input in the "Mark cells" form.
pub(crate) const CELLS_FIELD: &str = "cells";

/// Days since 1970-01-01, UTC, as the game clock (so tests can move it) has them.
fn today(app: &App) -> i64 {
    app.now().timestamp().div_euclid(86_400)
}

/// How many days in a row, ending today, the daily puzzle was solved; `solved` lists the days, newest first.
/// A streak that ended yesterday still counts until today's puzzle is missed.
pub(crate) fn streak(solved: &[i32], today: i64) -> i64 {
    let mut expected = match solved.first() {
        Some(&day) if i64::from(day) == today || i64::from(day) == today - 1 => i64::from(day),
        _ => return 0,
    };
    let mut count = 0;
    for &day in solved {
        if i64::from(day) != expected {
            break;
        }
        count += 1;
        expected -= 1;
    }
    count
}

/// Shells for the daily puzzle: 35, and 5 more for every day of the streak up to a week. A puzzle that was
/// checked pays a flat 15 and no streak bonus (the day still counts towards the streak).
pub(crate) fn daily_reward(streak: i64, checks: i16) -> i64 {
    if checks > 0 {
        return CHECKED_DAILY_REWARD;
    }
    30 + 5 * streak.clamp(1, 7)
}

/// `/play picross`: today's shared puzzle, or a practice one.
pub(crate) async fn start(cx: &Cx, daily: bool) -> anyhow::Result<()> {
    let me = cx.author()?.id.get();
    let game = if daily {
        let day = today(&cx.app);
        let (picture, variant) = picross::daily(day);
        cx.app
            .db
            .start_picross(me, DAILY, day as i32, (picture as u8, variant), false)
            .await?
    } else {
        // An unfinished practice board is picked up again, unless its picture has since come close to the
        // daily puzzle's.
        let allowed = picross::practice_pictures(today(&cx.app));
        match cx
            .app
            .db
            .picross_game(me, PRACTICE, 0)
            .await?
            .filter(|g| !g.solved() && allowed.contains(&(g.picture as usize)))
        {
            Some(unfinished) => unfinished,
            None => new_practice(&cx.app, me).await?,
        }
    };
    let data = view(&cx.app, cx.lang(), &game, None, None, "").await?;
    cx.reply(data).await
}

/// A random practice puzzle, from the pictures the daily puzzle will not show for a week.
async fn new_practice(app: &App, user: u64) -> anyhow::Result<PicrossGame> {
    let pictures = picross::practice_pictures(today(app));
    let picture = pictures[rand::random_range(0..pictures.len())] as u8;
    let variant = rand::random_range(0..picross::VARIANTS);
    app.db.start_picross(user, PRACTICE, 0, (picture, variant), true).await
}

/// Whether an unsolved practice board's picture has since come close to the daily puzzle's, so an older
/// message can no longer play it.
fn practice_closed(app: &App, game: &PicrossGame) -> bool {
    game.mode == PRACTICE
        && !game.solved()
        && !picross::practice_pictures(today(app)).contains(&(game.picture as usize))
}

fn puzzle_of(game: &PicrossGame) -> Puzzle {
    Puzzle::new(game.picture as usize, game.variant as u8)
}

fn grid_of(game: &PicrossGame) -> Grid {
    Grid::decode(&game.grid).unwrap_or_default()
}

/// The clue of a line as text: `1 1 2`.
fn clue_text(clue: &[u8]) -> String {
    clue.iter().map(u8::to_string).collect::<Vec<_>>().join(" ")
}

/// The menus and buttons under the board: pick a row, then its filled cells.
fn controls(lang: Lang, game: &PicrossGame, puzzle: &Puzzle, row: Option<usize>) -> Vec<Component> {
    let daily = game.mode == DAILY;
    let tag = format!("{}:{}", game.mode, game.day);
    if game.solved() {
        let (id, label) = if daily {
            (format!("pz:s:{tag}"), t!(lang, "picross.share"))
        } else {
            ("pz:n:practice:0".to_owned(), t!(lang, "picross.new-puzzle"))
        };
        return vec![Component::ActionRow(ActionRow {
            id: None,
            components: vec![button(id, &label, ButtonStyle::Success, false)],
        })];
    }
    let grid = grid_of(game);
    let clues = puzzle.clues();
    let rows = (0..SIZE)
        .map(|y| {
            let done = grid.row_done(&clues, y);
            option(
                t!(lang, "picross.row", number = y + 1),
                y.to_string(),
                Some(format!(
                    "{}{}",
                    clue_text(&clues.rows[y]),
                    if done { "  ✓" } else { "" }
                )),
                row == Some(y),
            )
        })
        .collect();
    let mut out = vec![select_row(
        format!("pz:r:{tag}"),
        &t!(lang, "picross.pick-row"),
        rows,
        (1, 1),
    )];
    if let Some(y) = row {
        let cells = (0..SIZE)
            .map(|x| {
                option(
                    format!("{}{}", char::from(b'A' + x as u8), y + 1),
                    x.to_string(),
                    None,
                    grid.get(y * SIZE + x),
                )
            })
            .collect();
        out.push(select_row(
            format!("pz:c:{tag}:{y}"),
            &t!(lang, "picross.row-cells", number = y + 1),
            cells,
            (0, SIZE as u8),
        ));
    }
    let left = (MAX_CHECKS - game.checks).max(0);
    out.push(Component::ActionRow(ActionRow {
        id: None,
        components: vec![
            button(
                format!("pz:m:{tag}"),
                &t!(lang, "picross.mark-cells"),
                ButtonStyle::Primary,
                false,
            ),
            button(
                format!("pz:k:{tag}"),
                &t!(lang, "picross.check", left = left),
                ButtonStyle::Secondary,
                left == 0,
            ),
        ],
    }));
    out
}

/// The private board: clues, marks, and the menus; or the finished picture. `wrong` is what a check just
/// found: how many marks are not part of the picture, never which.
async fn view(
    app: &App,
    lang: Lang,
    game: &PicrossGame,
    row: Option<usize>,
    wrong: Option<usize>,
    note: &str,
) -> anyhow::Result<InteractionResponseData> {
    let puzzle = puzzle_of(game);
    let grid = grid_of(game);
    let clues: Clues = puzzle.clues();
    let daily = game.mode == DAILY;
    let title = if daily {
        t!(
            lang,
            "picross.title-daily",
            number = picross::puzzle_number(i64::from(game.day))
        )
    } else {
        t!(lang, "picross.title-practice")
    };
    let total = puzzle.solution().iter().filter(|&&c| c).count();
    let subtitle = if game.solved() {
        t!(lang, "picross.subtitle-solved")
    } else {
        t!(lang, "picross.subtitle-progress", count = grid.count(), total = total)
    };
    let footer = if game.solved() && daily {
        t!(lang, "picross.footer-daily")
    } else if game.solved() {
        t!(lang, "picross.footer-practice")
    } else if let Some(wrong) = wrong {
        t!(lang, "picross.footer-wrong", count = wrong)
    } else {
        t!(lang, "picross.footer-pick")
    };
    let row_done: [bool; SIZE] = std::array::from_fn(|y| grid.row_done(&clues, y));
    let col_done: [bool; SIZE] = std::array::from_fn(|x| grid.col_done(&clues, x));
    // Per cell of the finished picture: what it is painted in, and lit, plain or shaded by what is above and
    // below it (the render's codes: 1 to 3 the colour, 4 to 6 the accent, 7 to 9 white, 10 an eye).
    let paint: [u8; CELLS] = std::array::from_fn(|cell| match puzzle.ink(cell) {
        None => 0,
        Some(Ink::Eye) => 10,
        Some(ink) => {
            let tone = match puzzle.shade(cell) {
                Shade::Light => 1,
                Shade::Main => 2,
                Shade::Dark => 3,
            };
            tone + match ink {
                Ink::Body | Ink::Eye => 0,
                Ink::Accent => 3,
                Ink::White => 6,
            }
        }
    });
    let color = format!("#{:06x}", puzzle.color());
    let accent = format!("#{:06x}", puzzle.accent());
    let solved = game.solved();
    let name = puzzle.name_in(lang);
    let cells = *grid.cells();
    let png = {
        // The card draws its words in capitals; Turkish needs its dotted İ.
        let (title, subtitle, footer) = (
            lang::upper(lang, &title),
            lang::upper(lang, &subtitle),
            lang::upper(lang, &footer),
        );
        let name = name.clone();
        app.render(move |r| {
            r.picross_card(&PicrossCard {
                title: &title,
                subtitle: &subtitle,
                grid: &cells,
                rows: &clues.rows,
                cols: &clues.cols,
                row_done,
                col_done,
                footer: &footer,
                reveal: solved.then_some(Reveal {
                    name: &name,
                    color: &color,
                    accent: &accent,
                    cells: &paint,
                }),
                theme: Theme::reef(),
            })
        })
        .await?
    };
    let description = if solved {
        t!(lang, "picross.found", name = name, note = note)
    } else {
        t!(lang, "picross.how-to", max = MAX_CHECKS, note = note)
    };
    let embed = EmbedBuilder::new()
        .color(if solved { SUCCESS } else { BRAND })
        .title(title)
        .description(description);
    Ok(Cx::card(
        embed,
        "picross.png",
        png,
        controls(lang, game, &puzzle_of(game), row),
        true,
    ))
}

/// `pz:r:<mode>:<day>` picks a row (the value), `pz:c:<mode>:<day>:<row>` sets that row's cells (the values),
/// `pz:m:<mode>:<day>` opens the form to type cells, `pz:k:<mode>:<day>` checks for mistakes, `pz:s:daily:<day>`
/// shares a solved daily and `pz:n:practice:0` starts another practice puzzle.
pub(crate) async fn on_component(cx: &Cx, mut args: Split<'_, char>, values: &[String]) -> anyhow::Result<()> {
    let me = cx.author()?.id.get();
    let lang = cx.lang();
    let action = args.next();
    let (Some(mode), Some(day)) = (
        args.next().filter(|m| *m == DAILY || *m == PRACTICE),
        args.next().and_then(|d| d.parse::<i32>().ok()),
    ) else {
        return cx.reply_ephemeral(t!(lang, "picross.board-expired")).await;
    };
    if action == Some("n") {
        let game = new_practice(&cx.app, me).await?;
        let data = view(&cx.app, lang, &game, None, None, "").await?;
        return cx.update(data).await;
    }
    let Some(game) = cx.app.db.picross_game(me, mode, day).await? else {
        return cx.reply_ephemeral(t!(lang, "picross.not-yours")).await;
    };
    if mode == DAILY && i64::from(day) != today(&cx.app) && !game.solved() {
        return cx.reply_ephemeral(t!(lang, "picross.daily-ended")).await;
    }
    if practice_closed(&cx.app, &game) {
        return cx.reply_ephemeral(t!(lang, "picross.practice-ended")).await;
    }
    match action {
        Some("r") => {
            let Some(row) = values
                .first()
                .and_then(|v| v.parse::<usize>().ok())
                .filter(|r| *r < SIZE)
            else {
                return cx.reply_ephemeral(t!(lang, "picross.menu-expired")).await;
            };
            // Only the menus change, so the picture stays as it is.
            cx.update(InteractionResponseData {
                components: Some(controls(lang, &game, &puzzle_of(&game), Some(row))),
                ..Default::default()
            })
            .await
        }
        Some("c") => {
            let Some(row) = args.next().and_then(|r| r.parse::<usize>().ok()).filter(|r| *r < SIZE) else {
                return cx.reply_ephemeral(t!(lang, "picross.menu-expired")).await;
            };
            let picked: Vec<usize> = values.iter().filter_map(|v| v.parse().ok()).collect();
            mark(cx, me, mode, day, Some(row), move |grid| {
                for x in 0..SIZE {
                    grid.set(row * SIZE + x, picked.contains(&x));
                }
            })
            .await
        }
        Some("m") if !game.solved() => {
            cx.modal(
                format!("pz:m:{mode}:{day}"),
                &t!(lang, "picross.mark-cells"),
                vec![cells_field(lang)],
            )
            .await
        }
        Some("k") if !game.solved() => {
            let Some(checked) = cx.app.db.picross_check(me, mode, day, MAX_CHECKS).await? else {
                return cx.reply_ephemeral(t!(lang, "picross.no-checks")).await;
            };
            let wrong = grid_of(&checked).wrong(&puzzle_of(&checked));
            let left = (MAX_CHECKS - checked.checks).max(0);
            let note = match wrong {
                0 => t!(lang, "picross.no-mistakes"),
                n => t!(lang, "picross.mistakes", count = n),
            };
            let note = format!("{note} {}", t!(lang, "picross.checks-left", count = left));
            let data = view(&cx.app, lang, &checked, None, Some(wrong), &note).await?;
            cx.update(data).await
        }
        Some("s") if game.solved() && mode == DAILY => share(cx, me, day).await,
        Some("m" | "k" | "s") => cx.reply_ephemeral(t!(lang, "picross.finished")).await,
        _ => cx.reply_ephemeral(t!(lang, "picross.button-expired")).await,
    }
}

#[allow(deprecated)] // the label belongs on the Label component, so the input's own stays empty
fn cells_field(lang: Lang) -> Component {
    Component::Label(Label {
        component: Box::new(Component::TextInput(TextInput {
            custom_id: CELLS_FIELD.to_owned(),
            id: None,
            label: None,
            max_length: Some(300),
            min_length: Some(2),
            placeholder: Some("A1 B3-B7 -C2".to_owned()),
            required: Some(true),
            style: TextInputStyle::Paragraph,
            value: None,
        })),
        description: Some(t!(lang, "picross.cells-help")),
        id: None,
        label: t!(lang, "picross.cells-label"),
    })
}

/// The "Mark cells" form was submitted: `pz:m:<mode>:<day>`.
pub(crate) async fn on_modal(cx: &Cx, mut args: Split<'_, char>, fields: &[(String, String)]) -> anyhow::Result<()> {
    let me = cx.author()?.id.get();
    let (Some(mode), Some(day)) = (
        args.next().filter(|m| *m == DAILY || *m == PRACTICE),
        args.next().and_then(|d| d.parse::<i32>().ok()),
    ) else {
        return cx.reply_ephemeral(t!(cx.lang(), "picross.form-expired")).await;
    };
    let text = fields
        .iter()
        .find(|(id, _)| id == CELLS_FIELD)
        .map_or("", |(_, value)| value.as_str());
    let Some(marks) = parse_marks(text) else {
        return cx.reply_ephemeral(t!(cx.lang(), "picross.unreadable")).await;
    };
    mark(cx, me, mode, day, None, move |grid| {
        for &cell in &marks.fill {
            grid.set(cell, true);
        }
        for &cell in &marks.clear {
            grid.set(cell, false);
        }
    })
    .await
}

/// Changes the player's marks with `edit` and shows the board. Two clicks at once can race for the grid,
/// so the loser tries again on what the winner left. A solved board is paid once, by whoever solved it.
async fn mark(
    cx: &Cx,
    me: u64,
    mode: &str,
    day: i32,
    row: Option<usize>,
    edit: impl Fn(&mut Grid),
) -> anyhow::Result<()> {
    let lang = cx.lang();
    for _ in 0..4 {
        let Some(game) = cx.app.db.picross_game(me, mode, day).await? else {
            return cx.reply_ephemeral(t!(lang, "picross.not-yours")).await;
        };
        if game.solved() {
            return cx.reply_ephemeral(t!(lang, "picross.finished")).await;
        }
        // A form opened before midnight may arrive after it; yesterday's daily is closed by then.
        if mode == DAILY && i64::from(day) != today(&cx.app) {
            return cx.reply_ephemeral(t!(lang, "picross.daily-ended")).await;
        }
        if practice_closed(&cx.app, &game) {
            return cx.reply_ephemeral(t!(lang, "picross.practice-ended")).await;
        }
        let puzzle = puzzle_of(&game);
        let mut grid = grid_of(&game);
        edit(&mut grid);
        let solved = grid.solves(&puzzle.clues());
        let Some(updated) = cx
            .app
            .db
            .picross_marks(me, mode, day, &game.grid, &grid.encode(), solved)
            .await?
        else {
            continue;
        };
        let note = if solved {
            finish(&cx.app, lang, me, &updated).await?
        } else {
            String::new()
        };
        let data = view(&cx.app, lang, &updated, row, None, &note).await?;
        return cx.update(data).await;
    }
    cx.reply_ephemeral(t!(lang, "picross.board-changed")).await
}

/// Counts a solved puzzle and pays for it; returns what to tell the player.
async fn finish(app: &App, lang: Lang, me: u64, game: &PicrossGame) -> anyhow::Result<String> {
    record(app, Game::Picross, me, true).await;
    if game.mode == DAILY {
        // A daily solved before the player deleted their data was paid then.
        if app.db.daily_picross_solved_before_deleting(me, game.day).await? {
            return Ok(t!(lang, "picross.replayed"));
        }
        let days = app.db.picross_days_solved(me, 40).await?;
        let streak = streak(&days, today(app));
        let shells = daily_reward(streak, game.checks);
        app.db.earn_shells(me, shells, "picross_daily").await?;
        return Ok(if game.checks > 0 {
            t!(lang, "picross.paid-daily-checked", shells = shells, count = streak)
        } else {
            t!(lang, "picross.paid-daily", shells = shells, count = streak)
        });
    }
    let reward = if game.checks > 0 {
        CHECKED_PRACTICE_REWARD
    } else {
        Game::Picross.win_reward()
    };
    let paid = pay_win(app, me, Game::Picross, reward, "picross_win").await?;
    Ok(if paid > 0 {
        t!(lang, "picross.paid", paid = paid)
    } else {
        t!(lang, "picross.limit-reached")
    })
}

/// Posts a solved daily to the channel: no picture, so nobody is spoiled.
async fn share(cx: &Cx, me: u64, day: i32) -> anyhow::Result<()> {
    if !cx
        .app
        .cache
        .once(&format!("picrossshare:{me}:{day}"), Duration::from_secs(2 * 86_400))
        .await?
    {
        return cx.reply_ephemeral(t!(cx.lang(), "picross.already-shared")).await;
    }
    let days = cx.app.db.picross_days_solved(me, 40).await?;
    let streak = streak(&days, i64::from(day));
    cx.reply(InteractionResponseData {
        content: Some(t!(
            cx.lang(),
            "picross.shared",
            user = mention(me),
            number = picross::puzzle_number(i64::from(day)),
            streak = streak
        )),
        allowed_mentions: quiet(),
        ..Default::default()
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_streak_counts_the_days_in_a_row_ending_today_or_yesterday() {
        assert_eq!(streak(&[], 100), 0);
        assert_eq!(streak(&[100, 99, 98], 100), 3);
        assert_eq!(
            streak(&[99, 98], 100),
            2,
            "yesterday's streak still stands until today is missed"
        );
        assert_eq!(streak(&[98, 97], 100), 0, "two days ago is a broken streak");
        assert_eq!(streak(&[100, 98, 97], 100), 1, "a gap ends it");
        assert_eq!(streak(&[100], 100), 1);
    }

    #[test]
    fn the_daily_reward_grows_with_the_streak_to_a_week() {
        assert_eq!(
            [0, 1, 2, 3, 7, 8, 30].map(|s| daily_reward(s, 0)),
            [35, 35, 40, 45, 65, 65, 65]
        );
    }

    #[test]
    fn a_checked_puzzle_pays_a_flat_smaller_reward_whatever_the_streak() {
        for streak in [0, 1, 4, 7, 30] {
            assert_eq!(daily_reward(streak, 1), CHECKED_DAILY_REWARD);
            assert_eq!(daily_reward(streak, MAX_CHECKS), CHECKED_DAILY_REWARD);
            assert!(daily_reward(streak, 1) < daily_reward(streak, 0));
        }
        assert!(CHECKED_PRACTICE_REWARD < Game::Picross.win_reward());
    }

    #[test]
    fn clues_read_as_text() {
        assert_eq!(clue_text(&[1, 1, 2]), "1 1 2");
        assert_eq!(clue_text(&[0]), "0");
    }
}
