//! Renders the game cards with sample data: `cargo run -p pinchora-render --example games -- <out-dir>`.

use pinchora_core::infra::i18n::Lang;
use pinchora_render::{
    ABSENT, CORRECT, Connect4Card, HangmanCard, HangmanLetter, PRESENT, Renderer, Theme, WordleCard, WordleRow,
};

fn main() -> anyhow::Result<()> {
    let out = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "target/render-samples".into());
    std::fs::create_dir_all(&out)?;
    let r = Renderer::new();

    // Connect four: a game in progress and a won one.
    let board = |rows: [&str; 6]| -> Vec<u8> {
        rows.iter()
            .flat_map(|row| row.bytes().map(|b| if b == b'.' { 0 } else { b - b'0' }))
            .collect()
    };
    let midgame = board([".......", ".......", "...2...", "...1...", "..212..", ".12121."]);
    let card = Connect4Card {
        lang: pinchora_core::infra::i18n::Lang::En,
        cells: &midgame,
        names: ["Captain Pinch", "Marlowe"],
        turn: Some(1),
        winner: None,
        win_cells: &[],
        last: Some(3 * 7 + 3),
        status: "Column 1-7: your move",
        pot: 0,
        theme: Theme::reef(),
    };
    std::fs::write(format!("{out}/connect4-midgame.png"), r.connect4_card(&card)?)?;
    let won = board([".......", ".......", "...1...", "..12...", ".122...", "12121.."]);
    let card = Connect4Card {
        lang: pinchora_core::infra::i18n::Lang::En,
        cells: &won,
        names: ["Captain Pinch", "A very long display name"],
        turn: None,
        winner: Some(1),
        win_cells: &[2 * 7 + 3, 3 * 7 + 2, 4 * 7 + 1, 5 * 7],
        last: Some(2 * 7 + 3),
        status: "Captain Pinch wins!",
        pot: 400,
        theme: Theme::reef(),
    };
    std::fs::write(format!("{out}/connect4-won.png"), r.connect4_card(&card)?)?;

    // Hangman: the sandcastle at every stage.
    let word = |text: &str, found: &str, lost: bool| -> Vec<Vec<HangmanLetter>> {
        text.split(' ')
            .map(|w| {
                w.chars()
                    .map(|c| match (found.contains(c), lost) {
                        (true, _) => HangmanLetter::Found(c),
                        (false, true) => HangmanLetter::Missed(c),
                        (false, false) => HangmanLetter::Hidden,
                    })
                    .collect()
            })
            .collect()
    };
    for misses in 0..=6usize {
        let lines = word("sea turtle", "etl", misses == 6);
        let wrong: Vec<char> = "xqzjwk".chars().take(misses).collect();
        let card = HangmanCard {
            lines: &lines,
            category: "Sea life",
            wrong: &wrong,
            misses,
            status: if misses == 6 {
                "The sea took it!"
            } else {
                "Pick a letter below"
            },
            lang: Lang::En,
            theme: Theme::reef(),
        };
        std::fs::write(format!("{out}/hangman-{misses}.png"), r.hangman_card(&card)?)?;
    }
    let long = word("watering can", "aeg", false);
    let card = HangmanCard {
        lines: &long,
        category: "House items",
        wrong: &['x'],
        misses: 1,
        status: "Pick a letter below",
        lang: Lang::En,
        theme: Theme::reef(),
    };
    std::fs::write(format!("{out}/hangman-long.png"), r.hangman_card(&card)?)?;

    // Wordle: a board being played.
    let row = |word: &str, marks: [u8; 5]| WordleRow {
        letters: word.chars().collect::<Vec<_>>().try_into().expect("five letters"),
        marks,
    };
    let rows = [
        row("crane", [ABSENT, PRESENT, CORRECT, ABSENT, PRESENT]),
        row("pearl", [CORRECT, CORRECT, CORRECT, CORRECT, CORRECT]),
    ];
    let keys_of = |rows: &[WordleRow]| -> Vec<Vec<(char, u8)>> {
        ["QWERTYUIOP", "ASDFGHJKL", "ZXCVBNM"]
            .iter()
            .map(|line| {
                line.chars()
                    .map(|k| {
                        let mark = rows
                            .iter()
                            .flat_map(|r| r.letters.iter().zip(r.marks))
                            .filter(|(c, _)| c.to_ascii_uppercase() == k)
                            .map(|(_, m)| m)
                            .max()
                            .unwrap_or(0);
                        (k, mark)
                    })
                    .collect()
            })
            .collect()
    };
    let card = WordleCard {
        title: "Daily wordle #63",
        rows: &rows,
        keyboard: &keys_of(&rows),
        footer: "Solved in 2/6",
        theme: Theme::reef(),
    };
    std::fs::write(format!("{out}/wordle-won.png"), r.wordle_card(&card)?)?;
    let card = WordleCard {
        title: "Practice wordle",
        rows: &rows[..1],
        keyboard: &keys_of(&rows[..1]),
        footer: "Try 2 of 6",
        theme: Theme::reef(),
    };
    std::fs::write(format!("{out}/wordle-playing.png"), r.wordle_card(&card)?)?;
    println!("wrote game samples to {out}");
    Ok(())
}
