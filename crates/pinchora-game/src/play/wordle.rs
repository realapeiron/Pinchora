//! Wordle: five letters, six tries, one shared word a day.

pub const WORD_LEN: usize = 5;
pub const MAX_TRIES: usize = 6;
/// Day number (days since 1970-01-01) of puzzle #0.
const LAUNCH_DAY: i64 = 20_700;

/// How a letter of a guess matched the answer. Better matches sort higher.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Tile {
    Absent,
    Present,
    Correct,
}

impl Tile {
    pub fn emoji(self) -> &'static str {
        match self {
            Self::Absent => "⬛",
            Self::Present => "🟨",
            Self::Correct => "🟩",
        }
    }
}

/// Lowercases a typed guess; `None` unless it is exactly five letters a-z.
pub fn normalize(input: &str) -> Option<String> {
    let word = input.trim().to_ascii_lowercase();
    (word.len() == WORD_LEN && word.bytes().all(|b| b.is_ascii_lowercase())).then_some(word)
}

/// Colours a guess against the answer. Letters in the right place are correct first; each
/// remaining answer letter can then mark only one other tile, so a repeated letter in the guess
/// is yellow no more often than it occurs in the answer.
pub fn score(guess: &str, answer: &str) -> [Tile; WORD_LEN] {
    let (g, a) = (guess.as_bytes(), answer.as_bytes());
    let mut tiles = [Tile::Absent; WORD_LEN];
    if g.len() != WORD_LEN || a.len() != WORD_LEN || !g.iter().chain(a).all(u8::is_ascii_lowercase) {
        return tiles;
    }
    let mut unmatched = [0u8; 26];
    for i in 0..WORD_LEN {
        if g[i] == a[i] {
            tiles[i] = Tile::Correct;
        } else {
            unmatched[(a[i] - b'a') as usize] += 1;
        }
    }
    for i in 0..WORD_LEN {
        if tiles[i] == Tile::Correct {
            continue;
        }
        let slot = &mut unmatched[(g[i] - b'a') as usize];
        if *slot > 0 {
            *slot -= 1;
            tiles[i] = Tile::Present;
        }
    }
    tiles
}

/// The best result each letter a-z has had over the guesses, `None` for letters not tried.
pub fn keyboard<'a>(guesses: impl IntoIterator<Item = (&'a str, [Tile; WORD_LEN])>) -> [Option<Tile>; 26] {
    let mut keys: [Option<Tile>; 26] = [None; 26];
    for (word, tiles) in guesses {
        for (b, tile) in word.bytes().zip(tiles) {
            if b.is_ascii_lowercase() {
                let key = &mut keys[(b - b'a') as usize];
                *key = Some(key.map_or(tile, |old| old.max(tile)));
            }
        }
    }
    keys
}

fn gcd(a: i64, b: i64) -> i64 {
    if b == 0 { a } else { gcd(b, a % b) }
}

/// Today's word for everyone. Walking the list with a step coprime to its length visits every
/// word once before any repeats, and consecutive days land far apart in the list.
pub fn daily_word(answers: &[String], day: i64) -> &str {
    let n = answers.len() as i64;
    assert!(n > 0, "no answer words");
    let mut step = (n * 38 / 100).max(1);
    while gcd(step, n) != 1 {
        step += 1;
    }
    &answers[((day.rem_euclid(n) * step + 17) % n) as usize]
}

/// The number shown on a day's puzzle.
pub fn puzzle_number(day: i64) -> i64 {
    day - LAUNCH_DAY
}

/// Shells for a win. The daily word pays more, and finishing in fewer tries pays more.
pub fn reward(daily: bool, tries: usize) -> i64 {
    if daily {
        20 + 5 * (MAX_TRIES.saturating_sub(tries.max(1)) as i64)
    } else {
        10
    }
}

/// A result without spoilers: the number, the tries and the coloured squares.
pub fn share_text(puzzle: i64, solved_in: Option<usize>, rows: &[[Tile; WORD_LEN]]) -> String {
    let tries = solved_in.map_or_else(|| "X".to_owned(), |n| n.to_string());
    let mut text = format!("Pinchora Wordle #{puzzle} {tries}/{MAX_TRIES}\n");
    for row in rows {
        text.push('\n');
        text.extend(row.iter().map(|t| t.emoji()));
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;
    use Tile::{Absent as A, Correct as C, Present as P};

    #[test]
    fn colours_plain_guesses() {
        assert_eq!(score("pearl", "pearl"), [C; 5]);
        assert_eq!(score("crane", "pearl"), [A, P, C, A, P]);
        assert_eq!(score("fight", "pearl"), [A; 5]);
    }

    #[test]
    fn repeated_letters_are_coloured_by_how_often_the_answer_has_them() {
        // One E in the answer: the green E takes it, so the other Es stay grey.
        assert_eq!(score("geese", "pearl"), [A, C, A, A, A]);
        assert_eq!(score("eerie", "pearl"), [A, C, P, A, A]);
        // Two Ls in the answer, one in the guess.
        assert_eq!(score("lemon", "hello"), [P, C, A, P, A]);
        // Two green Ls leave none for the first L of the guess.
        assert_eq!(score("lolly", "hello"), [A, P, C, C, A]);
        // Two Ls misplaced in the guess: both yellow.
        assert_eq!(score("llama", "hello"), [P, P, A, A, A]);
        // The exact match is taken before the misplaced one, wherever it sits in the guess.
        assert_eq!(score("allll", "cable"), [P, A, A, C, A]);
        assert_eq!(score("speed", "abide"), [A, A, P, A, P]);
        assert_eq!(score("erase", "speed"), [P, A, A, P, P]);
        assert_eq!(score("sassy", "glass"), [P, P, A, C, A]);
    }

    #[test]
    fn bad_lengths_score_nothing() {
        assert_eq!(score("hi", "pearl"), [A; 5]);
        assert_eq!(score("PEARL", "pearl"), [A; 5]);
    }

    #[test]
    fn guesses_are_normalised() {
        assert_eq!(normalize("  PeArL "), Some("pearl".into()));
        for bad in ["", "pear", "pearls", "pe rl", "pe4rl", "péarl"] {
            assert_eq!(normalize(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn the_keyboard_keeps_each_letters_best_result() {
        let first = score("crane", "pearl");
        let second = score("pearl", "pearl");
        let keys = keyboard([("crane", first), ("pearl", second)]);
        assert_eq!(keys[(b'p' - b'a') as usize], Some(C));
        assert_eq!(keys[(b'c' - b'a') as usize], Some(A));
        assert_eq!(keys[(b'e' - b'a') as usize], Some(C), "yellow then green is green");
        assert_eq!(keys[(b'z' - b'a') as usize], None);
        // Green never goes back to yellow.
        let keys = keyboard([("pearl", second), ("crane", first)]);
        assert_eq!(keys[(b'r' - b'a') as usize], Some(C));
    }

    #[test]
    fn the_daily_word_visits_every_word_before_repeating() {
        let answers: Vec<String> = (0..1259).map(|i| format!("w{i}")).collect();
        let mut seen = std::collections::HashSet::new();
        for day in 20_000..20_000 + 1259 {
            assert!(seen.insert(daily_word(&answers, day)), "repeat on day {day}");
        }
        assert_eq!(daily_word(&answers, 20_763), daily_word(&answers, 20_763));
        assert_ne!(daily_word(&answers, 20_763), daily_word(&answers, 20_764));
        // A list size sharing factors with the step still works.
        let even: Vec<String> = (0..1000).map(|i| format!("w{i}")).collect();
        let mut seen = std::collections::HashSet::new();
        assert!((0..1000).all(|d| seen.insert(daily_word(&even, d))));
    }

    #[test]
    fn rewards_and_share_lines() {
        assert_eq!(reward(true, 1), 45);
        assert_eq!(reward(true, 6), 20);
        assert_eq!(reward(false, 3), 10);
        let rows = [score("crane", "pearl"), score("pearl", "pearl")];
        let text = share_text(63, Some(2), &rows);
        assert_eq!(text, "Pinchora Wordle #63 2/6\n\n⬛🟨🟩⬛🟨\n🟩🟩🟩🟩🟩");
        assert!(!text.contains("pearl") && !text.contains("crane"), "no spoilers");
        assert!(share_text(63, None, &rows).starts_with("Pinchora Wordle #63 X/6"));
    }
}
