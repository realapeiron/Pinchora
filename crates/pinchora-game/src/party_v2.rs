//! Original reef games. Serializable rules contain seat numbers, never Discord identities.
pub mod chess_ai;
pub mod puzzles;
use crate::{
    party::{filter, hotpearl},
    play::{PlayContent, chess, trivia},
};
use pinchora_core::{i18n::Lang, t};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Kind {
    Quiz,
    Words,
    Story,
    Room,
    Critter,
    Chess,
    Maze,
    Match,
    Survival,
}
impl Kind {
    pub fn key(self) -> &'static str {
        match self {
            Self::Quiz => "reefquiz",
            Self::Words => "wordwake",
            Self::Story => "driftstory",
            Self::Room => "roomwake",
            Self::Critter => "crittercompass",
            Self::Chess => "reefmind",
            Self::Maze => "kelpmaze",
            Self::Match => "gemtide",
            Self::Survival => "lastreef",
        }
    }
    pub fn solo(self) -> bool {
        matches!(self, Self::Chess | Self::Maze | Self::Match)
    }
    pub fn prefix(self) -> &'static str {
        if self == Self::Chess {
            "d9c"
        } else if self.solo() {
            "d9s"
        } else {
            "d9p"
        }
    }
    pub fn title(self, lang: Lang) -> String {
        t!(lang, &format!("party-v2.{}", self.key()))
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Seat {
    pub active: bool,
    pub score: u32,
    pub lives: u8,
    pub team: u8,
    pub actions: u32,
    pub ally: Option<usize>,
}
impl Default for Seat {
    fn default() -> Self {
        Self {
            active: true,
            score: 0,
            lives: 3,
            team: 0,
            actions: 0,
            ally: None,
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Board {
    Quiz {
        questions: Vec<String>,
        order: [usize; 4],
        answered: Vec<usize>,
    },
    Words {
        fragment: String,
        used: Vec<String>,
        last: String,
        best: usize,
        #[serde(default)]
        longest: Vec<(usize, usize)>,
    },
    Story {
        words: Vec<String>,
    },
    Room {
        rooms: Vec<usize>,
        order: [usize; 4],
        answered: Vec<usize>,
    },
    Critter {
        bits: usize,
        step: usize,
    },
    Chess {
        moves: String,
    },
    Maze(puzzles::Maze),
    Match(puzzles::Match),
    Survival {
        choices: Vec<(usize, usize)>,
        event: usize,
    },
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct State {
    pub kind: Kind,
    pub lang: Lang,
    pub mode: String,
    pub seq: u32,
    pub round: usize,
    pub turn: usize,
    pub lobby: bool,
    pub done: bool,
    pub cancelled: bool,
    pub seats: Vec<Seat>,
    pub winners: Vec<usize>,
    pub board: Board,
    pub seed: u64,
    pub notice: String,
}
impl State {
    pub fn new(kind: Kind, lang: Lang, mode: &str, category: &str, seed: u64) -> Self {
        let mut rng = seed;
        let board = match kind {
            Kind::Quiz => {
                let mut qs: Vec<_> = PlayContent::for_lang(lang)
                    .trivia
                    .iter()
                    .filter(|q| category == "all" || q.category.key() == category)
                    .collect();
                let mut seen = std::collections::HashSet::new();
                let mut topics = std::collections::HashSet::new();
                qs.retain(|q| seen.insert(crate::lang::lower(lang, &q.question)) && topics.insert(quiz_topic(&q.id)));
                for i in (1..qs.len()).rev() {
                    let j = (next(&mut rng) as usize) % (i + 1);
                    qs.swap(i, j);
                }
                Board::Quiz {
                    questions: qs.into_iter().take(5).map(|q| q.id.clone()).collect(),
                    order: trivia::permutation(seed),
                    answered: vec![],
                }
            }
            Kind::Words => Board::Words {
                fragment: fragment(lang, seed).into(),
                used: vec![],
                last: String::new(),
                best: 0,
                longest: vec![],
            },
            Kind::Story => Board::Story { words: vec![] },
            Kind::Room => {
                let mut rooms = vec![0, 1, 2, 3];
                for i in (1..4).rev() {
                    let j = next(&mut rng) as usize % (i + 1);
                    rooms.swap(i, j);
                }
                Board::Room {
                    rooms,
                    order: trivia::permutation(seed),
                    answered: vec![],
                }
            }
            Kind::Critter => Board::Critter { bits: 0, step: 0 },
            Kind::Chess => Board::Chess { moves: String::new() },
            Kind::Maze => Board::Maze(puzzles::Maze::new(seed)),
            Kind::Match => Board::Match(puzzles::Match::new(seed)),
            Kind::Survival => Board::Survival {
                choices: vec![],
                event: 0,
            },
        };
        Self {
            kind,
            lang,
            mode: mode.into(),
            seq: 0,
            round: 0,
            turn: 0,
            lobby: !kind.solo(),
            done: false,
            cancelled: false,
            seats: vec![Seat::default()],
            winners: vec![],
            board,
            seed: rng,
            notice: String::new(),
        }
    }
    pub fn seconds(&self) -> i64 {
        if self.lobby {
            90
        } else {
            match self.kind {
                Kind::Quiz if self.mode == "quick" => 12,
                Kind::Quiz | Kind::Room => 25,
                Kind::Words => 20,
                Kind::Story | Kind::Survival | Kind::Critter => 35,
                _ => 120,
            }
        }
    }
    pub fn add_seat(&mut self) -> Option<usize> {
        if !self.lobby || self.seats.len() >= 12 {
            return None;
        }
        let i = self.seats.len();
        self.seats.push(Seat {
            team: (i % 2) as u8,
            ..Seat::default()
        });
        self.seq += 1;
        Some(i)
    }
    pub fn start(&mut self) -> bool {
        if !self.lobby || self.seats.iter().filter(|s| s.active).count() < 2 {
            return false;
        }
        // Balance teams after lobby departures.
        for (i, s) in self.seats.iter_mut().filter(|s| s.active).enumerate() {
            s.team = (i % 2) as u8;
        }
        self.lobby = false;
        self.turn = self.seats.iter().position(|s| s.active).unwrap_or(0);
        self.seq += 1;
        true
    }
    pub fn forfeit(&mut self, seat: usize) {
        if let Some(s) = self.seats.get_mut(seat) {
            s.active = false;
            s.ally = None;
        }
        for s in &mut self.seats {
            if s.ally == Some(seat) {
                s.ally = None;
            }
        }
        match &mut self.board {
            Board::Quiz { answered, .. } | Board::Room { answered, .. } => {
                answered.retain(|&i| i != seat);
            }
            Board::Survival { choices, .. } => choices.retain(|&(i, _)| i != seat),
            Board::Words { best, longest, .. } => {
                // Older saved rounds only remember their leader; keep that candidate on resume.
                if longest.is_empty() && *best > 0 {
                    longest.push((self.turn, *best));
                }
                longest.retain(|&(i, _)| i != seat);
                *best = 0;
                for &(i, len) in longest.iter() {
                    if len > *best {
                        *best = len;
                        self.turn = i;
                    }
                }
            }
            _ => {}
        }
        if !self.lobby {
            if self.kind.solo() {
                self.done = true;
            } else if self.active_count() <= 1 {
                self.finish();
            } else {
                if self.turn == seat {
                    self.next_turn();
                }
                match &self.board {
                    Board::Quiz { answered, .. } | Board::Room { answered, .. }
                        if answered.len() == self.active_count() =>
                    {
                        self.advance_round()
                    }
                    Board::Survival { choices, .. } if choices.len() == self.active_count() => self.survive(),
                    _ => {}
                }
            }
        }
        self.seq += 1;
    }
    fn active_count(&self) -> usize {
        self.seats.iter().filter(|s| s.active).count()
    }
    fn next_turn(&mut self) {
        for n in 1..=self.seats.len() {
            let i = (self.turn + n) % self.seats.len();
            if self.seats[i].active {
                self.turn = i;
                break;
            }
        }
    }
    pub fn cancel(&mut self) {
        self.cancelled = true;
        self.done = true;
        self.winners.clear();
        self.seq += 1;
    }
    pub fn finish(&mut self) {
        self.done = true;
        let best = self
            .seats
            .iter()
            .filter(|s| s.active)
            .map(|s| s.score)
            .max()
            .unwrap_or(0);
        if self.kind == Kind::Quiz && self.mode == "team" {
            let mut totals = [0u32; 2];
            for s in &self.seats {
                if s.active {
                    totals[s.team as usize] += s.score;
                }
            }
            self.winners = self
                .seats
                .iter()
                .enumerate()
                .filter(|(_, s)| {
                    s.active
                        && s.actions > 0
                        && totals[s.team as usize] > 0
                        && totals[s.team as usize] == *totals.iter().max().unwrap()
                })
                .map(|(i, _)| i)
                .collect();
        } else {
            self.winners = self
                .seats
                .iter()
                .enumerate()
                .filter(|(_, s)| s.active && s.actions > 0 && (self.kind == Kind::Story || s.score == best && best > 0))
                .map(|(i, _)| i)
                .collect();
        }
    }
    /// Only genuine participation is paid; all modes share a database daily allowance.
    pub fn reward_seats(&self) -> Vec<usize> {
        // Critter is a self-reported solo guess even though it has a party lobby.
        if !self.done
            || self.cancelled
            || self.kind == Kind::Critter
            || (!self.kind.solo() && self.seats.iter().filter(|s| s.actions > 0).count() < 2)
        {
            return vec![];
        }
        self.winners
            .iter()
            .copied()
            .filter(|&i| {
                self.seats[i].actions
                    >= match self.kind {
                        Kind::Chess => 4,
                        Kind::Story | Kind::Words => 2,
                        _ => 1,
                    }
            })
            .collect()
    }
    pub fn act(&mut self, seat: usize, input: &str, strict: bool) -> Result<(), &'static str> {
        if self.lobby || self.done || !self.seats.get(seat).is_some_and(|s| s.active) {
            return Err("denied");
        }
        self.notice.clear();
        match &mut self.board {
            Board::Quiz {
                questions,
                order,
                answered,
            } => {
                if answered.contains(&seat) {
                    return Err("answered");
                }
                let q = PlayContent::for_lang(self.lang)
                    .question(&questions[self.round])
                    .ok_or("invalid")?;
                let pick: usize = input.parse().map_err(|_| "invalid")?;
                if pick >= 4 {
                    return Err("invalid");
                }
                answered.push(seat);
                self.seats[seat].actions += 1;
                if q.correct_slot(*order) == Some(pick) {
                    self.seats[seat].score += if self.mode == "quick" {
                        (4usize.saturating_sub(answered.len())).max(1) as u32
                    } else {
                        1
                    };
                }
                if answered.len() >= self.seats.iter().filter(|s| s.active).count() {
                    self.advance_round();
                }
            }
            Board::Room { rooms, order, answered } => {
                if answered.contains(&seat) {
                    return Err("answered");
                }
                let pick: usize = input.parse().map_err(|_| "invalid")?;
                if pick >= 4 {
                    return Err("invalid");
                }
                answered.push(seat);
                self.seats[seat].actions += 1;
                if order[pick] == rooms[self.round] {
                    self.seats[seat].score += 1;
                }
                if answered.len() >= self.seats.iter().filter(|s| s.active).count() {
                    self.advance_round();
                }
            }
            Board::Words {
                fragment,
                used,
                last,
                best,
                longest,
            } => {
                let mode = word_mode(&self.mode, self.round);
                if (mode == "relay" || mode == "chain") && seat != self.turn {
                    return Err("turn");
                }
                let word = hotpearl::check_word(hotpearl::dictionary_for(self.lang), fragment, input, used)
                    .map_err(|_| "word")?;
                if filter::is_profane_for(&word, strict, self.lang) {
                    return Err("word");
                }
                if mode == "chain" && !last.is_empty() && word.chars().next() != last.chars().last() {
                    return Err("word");
                }
                let len = word.chars().count();
                used.push(word.clone());
                *last = word;
                self.seats[seat].actions += 1;
                if mode == "longest" {
                    if longest.is_empty() && *best > 0 {
                        longest.push((self.turn, *best));
                    }
                    longest.push((seat, len));
                    if len > *best {
                        *best = len;
                        self.turn = seat;
                    }
                } else {
                    self.seats[seat].score += 1;
                }
                if mode == "relay" || mode == "chain" {
                    self.round += 1;
                    *fragment = crate::party_v2::fragment(self.lang, self.seed + self.round as u64).into();
                    self.next_turn();
                    if self.round >= 24 {
                        self.finish();
                    }
                }
            }
            Board::Story { words } => {
                if seat != self.turn {
                    return Err("turn");
                }
                let word = input.trim();
                if word.is_empty()
                    || word.chars().count() > 24
                    || !word.chars().all(char::is_alphabetic)
                    || filter::is_profane_for(word, strict, self.lang)
                {
                    return Err("word");
                }
                let joined = format!("{} {word}", words.join(" "));
                if filter::is_profane_for(&joined, strict, self.lang) {
                    return Err("word");
                }
                words.push(word.into());
                self.seats[seat].score += 1;
                self.seats[seat].actions += 1;
                self.round += 1;
                self.next_turn();
                if self.round >= 18 {
                    self.finish();
                }
            }
            Board::Critter { bits, step } => {
                if seat != self.turn {
                    return Err("turn");
                }
                if input != "yes" && input != "no" {
                    return Err("invalid");
                }
                if *step < 3 {
                    if input == "yes" {
                        *bits |= 1 << *step;
                    }
                    *step += 1;
                } else {
                    self.seats[seat].actions += 1;
                    if input == "yes" {
                        self.seats[seat].score += 1;
                    }
                    self.finish();
                }
            }
            Board::Chess { moves } => {
                let mut game = chess::Game::from_moves(moves).ok_or("invalid")?;
                game.play(chess::Move::from_uci(&input.to_ascii_lowercase()).ok_or("move")?)
                    .ok_or("move")?;
                if game.outcome().is_none() {
                    let level = match self.mode.as_str() {
                        "easy" => 1,
                        "hard" => 3,
                        _ => 2,
                    };
                    if let Some(mv) = chess_ai::choose(&game.position, level).mv {
                        game.play(mv).ok_or("move")?;
                    }
                }
                *moves = game.moves_text();
                self.seats[seat].actions += 1;
                if let Some(outcome) = game.outcome() {
                    self.notice = outcome.describe(self.lang);
                    self.done = true;
                    if outcome.winner() == Some(chess::Color::White) {
                        self.seats[seat].score = 1;
                        self.winners = vec![seat];
                    }
                }
            }
            Board::Maze(board) => {
                if !board.step(input) {
                    return Err("move");
                }
                self.seats[seat].actions += 1;
                if board.won() {
                    self.seats[seat].score = 1;
                    self.finish();
                } else if board.moves >= 150 {
                    self.done = true;
                }
            }
            Board::Match(board) => {
                let cell: usize = input.parse().map_err(|_| "invalid")?;
                if !board.select(cell) {
                    return Err("move");
                }
                self.seats[seat].actions += 1;
                if board.score >= 30 {
                    self.seats[seat].score = board.score;
                    self.finish();
                } else if board.moves >= 20 {
                    self.done = true;
                }
            }
            Board::Survival { choices, .. } => {
                if let Some(target) = input.strip_prefix("ally-") {
                    let target: usize = target.parse().map_err(|_| "invalid")?;
                    if target == seat || !self.seats.get(target).is_some_and(|s| s.active) {
                        return Err("invalid");
                    }
                    self.seats[seat].ally = Some(target);
                } else {
                    let action = match input {
                        "shelter" => 0,
                        "forage" => 1,
                        "explore" => 2,
                        _ => return Err("invalid"),
                    };
                    if choices.iter().any(|(s, _)| *s == seat) {
                        return Err("answered");
                    }
                    choices.push((seat, action));
                    self.seats[seat].actions += 1;
                    if choices.len() == self.seats.iter().filter(|s| s.active).count() {
                        self.survive();
                    }
                }
            }
        }
        self.seq += 1;
        Ok(())
    }
    fn advance_round(&mut self) {
        self.round += 1;
        let len = match &mut self.board {
            Board::Quiz {
                questions,
                order,
                answered,
            } => {
                answered.clear();
                *order = trivia::permutation(self.seed + self.round as u64);
                questions.len()
            }
            Board::Room { rooms, order, answered } => {
                answered.clear();
                *order = trivia::permutation(self.seed + self.round as u64);
                rooms.len()
            }
            _ => 0,
        };
        if self.round >= len {
            self.finish();
        }
    }
    fn survive(&mut self) {
        let Board::Survival { choices, event } = &mut self.board else {
            return;
        };
        *event = (next(&mut self.seed) % 4) as usize;
        let snapshot = self.seats.clone();
        for (i, s) in self.seats.iter_mut().enumerate().filter(|(_, s)| s.active) {
            let action = choices.iter().find(|(seat, _)| *seat == i).map(|(_, a)| *a);
            let protected = s
                .ally
                .is_some_and(|a| snapshot.get(a).is_some_and(|ally| ally.active && ally.ally == Some(i)));
            let damage: u8 = match action {
                None => 2,
                Some(0) if *event == 0 => 0,
                Some(1) if *event == 1 => 0,
                Some(2) if *event == 2 => 0,
                _ => 1,
            };
            let damage = if protected && self.round.is_multiple_of(2) {
                damage.saturating_sub(1)
            } else {
                damage
            };
            s.lives = s.lives.saturating_sub(damage);
            s.active = s.lives > 0;
            s.score += u32::from(s.active);
        }
        choices.clear();
        self.round += 1;
        self.notice = t!(self.lang, &format!("party-v2.event-{}", event));
        if self.active_count() <= 1 || self.round >= 8 {
            self.finish();
        }
    }
    pub fn timeout(&mut self) {
        if self.done {
            return;
        }
        if self.lobby {
            self.cancel();
            return;
        }
        match self.kind {
            Kind::Quiz | Kind::Room => self.advance_round(),
            Kind::Survival => self.survive(),
            Kind::Words => {
                let mode = word_mode(&self.mode, self.round);
                if mode == "relay" || mode == "chain" {
                    let s = &mut self.seats[self.turn];
                    s.lives = s.lives.saturating_sub(1);
                    s.active = s.lives > 0;
                    self.next_turn();
                } else if mode == "longest"
                    && let Board::Words { best, .. } = &self.board
                    && *best > 0
                {
                    self.seats[self.turn].score += 1;
                }
                self.round += 1;
                if let Board::Words {
                    fragment,
                    best,
                    last,
                    longest,
                    ..
                } = &mut self.board
                {
                    *fragment = crate::party_v2::fragment(self.lang, self.seed + self.round as u64).into();
                    *best = 0;
                    longest.clear();
                    last.clear();
                }
                if self.active_count() < 2 || self.round >= 12 {
                    self.finish();
                }
            }
            Kind::Story => {
                self.round += 1;
                self.next_turn();
                if self.round >= 18 {
                    self.finish();
                }
            }
            _ => {
                self.done = true;
                self.winners.clear();
            }
        }
        self.seq += 1;
    }
}
// Alternate Turkish questions can ask the same fact with different punctuation or wording.
fn quiz_topic(id: &str) -> &str {
    match id {
        "science_tr_t01" => "science_tr_015",
        "science_tr_t02" => "nature_tr_001",
        "science_tr_t03" => "nature_tr_035",
        "science_tr_t05" => "nature_tr_031",
        "science_tr_t07" => "nature_tr_036",
        "ocean_tr_t05" => "ocean_tr_012",
        "ocean_tr_t08" => "ocean_tr_003",
        "ocean_tr_t06" => "geography_tr_t10",
        _ => id,
    }
}

pub fn word_mode(mode: &str, round: usize) -> &str {
    if mode == "mixed" {
        ["relay", "longest", "shoal"][round % 3]
    } else {
        mode
    }
}
fn fragment(lang: Lang, seed: u64) -> &'static str {
    let words = if lang == Lang::Tr {
        ["ar", "en", "al", "de", "an", "in"]
    } else {
        ["ar", "en", "al", "st", "an", "in"]
    };
    words[seed as usize % words.len()]
}
pub fn next(seed: &mut u64) -> u64 {
    *seed = seed.wrapping_add(0x9e3779b97f4a7c15);
    let mut z = *seed;
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d049bb133111eb);
    z ^ (z >> 31)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn started(kind: Kind, lang: Lang, mode: &str) -> State {
        let mut s = State::new(kind, lang, mode, "all", 42);
        if s.lobby {
            s.add_seat();
            assert!(s.start());
        }
        s
    }
    #[test]
    fn party_rewards_require_two_actual_players_and_exclude_self_reported_critter() {
        for kind in [
            Kind::Quiz,
            Kind::Words,
            Kind::Story,
            Kind::Room,
            Kind::Survival,
            Kind::Critter,
        ] {
            let mut s = started(kind, Lang::En, "quick");
            s.seats[0].actions = 4;
            s.seats[0].score = 4;
            s.finish();
            assert!(
                s.reward_seats().is_empty(),
                "{kind:?}: an idle second seat is not participation"
            );
            s.seats[1].actions = 2;
            if kind == Kind::Critter {
                assert!(s.reward_seats().is_empty(), "self-reported guesses cannot earn shells");
            } else {
                assert_eq!(s.reward_seats(), vec![0]);
            }
        }
    }

    #[test]
    fn departing_answers_do_not_skip_other_players() {
        for kind in [Kind::Quiz, Kind::Room, Kind::Survival] {
            let mut s = State::new(kind, Lang::En, "quick", "all", 42);
            s.add_seat();
            s.add_seat();
            s.start();
            let action = if kind == Kind::Survival { "shelter" } else { "0" };
            s.act(0, action, false).unwrap();
            s.forfeit(0);
            s.act(1, action, false).unwrap();
            assert_eq!(s.round, 0, "{kind:?}: seat 2 still needs a turn");
            s.act(2, action, false).unwrap();
            assert_eq!(s.round, 1);
        }
    }

    #[test]
    fn last_unanswered_departure_advances_the_round() {
        for kind in [Kind::Quiz, Kind::Room, Kind::Survival] {
            let mut s = State::new(kind, Lang::En, "quick", "all", 42);
            s.add_seat();
            s.add_seat();
            s.start();
            let action = if kind == Kind::Survival { "shelter" } else { "0" };
            s.act(0, action, false).unwrap();
            s.act(1, action, false).unwrap();
            s.forfeit(2);
            assert_eq!(s.round, 1, "{kind:?}: all remaining players have answered");
        }
    }

    #[test]
    fn longest_word_departure_awards_the_remaining_best_word_after_resume() {
        let mut s = State::new(Kind::Words, Lang::En, "longest", "all", 42);
        s.add_seat();
        s.add_seat();
        s.start();
        let Board::Words { fragment, .. } = &mut s.board else {
            panic!()
        };
        *fragment = "ea".into();
        s.act(0, "pearl", false).unwrap();
        s.act(2, "sea", false).unwrap();
        let mut s: State = serde_json::from_str(&serde_json::to_string(&s).unwrap()).unwrap();
        s.forfeit(0);
        s.timeout();
        assert_eq!(s.seats[0].score, 0);
        assert_eq!(s.seats[1].score, 0);
        assert_eq!(s.seats[2].score, 1);
        let Board::Words { longest, .. } = &s.board else {
            panic!()
        };
        assert!(longest.is_empty());
    }

    #[test]
    fn old_word_rounds_without_candidate_history_still_resume() {
        let s = started(Kind::Words, Lang::En, "longest");
        let mut saved = serde_json::to_value(s).unwrap();
        saved["board"]["Words"].as_object_mut().unwrap().remove("longest");
        let mut restored: State = serde_json::from_value(saved).unwrap();
        restored.timeout();
        assert_eq!(restored.round, 1);
    }

    #[test]
    fn turkish_wordwake_preserves_every_turkish_letter() {
        let mut s = started(Kind::Words, Lang::Tr, "shoal");
        for (fragment, word, normalized) in [
            ("iş", "İŞÇİ", "işçi"),
            ("ış", "IŞIK", "ışık"),
            ("ağ", "AĞAÇ", "ağaç"),
            ("gö", "GÖZ", "göz"),
            ("gü", "GÜNEŞ", "güneş"),
        ] {
            if let Board::Words { fragment: f, .. } = &mut s.board {
                *f = fragment.into();
            }
            s.act(0, word, false).unwrap();
            let Board::Words { last, .. } = &s.board else { panic!() };
            assert_eq!(last, normalized);
            assert_eq!(s.act(1, normalized, false), Err("word"));
        }
    }

    #[test]
    fn quiz_packs_do_not_repeat_paraphrased_facts() {
        let repeated = [
            ["science_tr_015", "science_tr_t01"],
            ["nature_tr_001", "science_tr_t02"],
            ["nature_tr_035", "science_tr_t03"],
            ["nature_tr_031", "science_tr_t05"],
            ["nature_tr_036", "science_tr_t07"],
            ["ocean_tr_012", "ocean_tr_t05"],
            ["ocean_tr_003", "ocean_tr_t08"],
            ["geography_tr_t10", "ocean_tr_t06"],
        ];
        for (category, seed) in [("science", 712), ("ocean", 55), ("all", 547)] {
            let s = State::new(Kind::Quiz, Lang::Tr, "team", category, seed);
            let Board::Quiz { questions, .. } = s.board else {
                panic!()
            };
            for pair in repeated {
                assert!(
                    !pair.iter().all(|id| questions.iter().any(|q| q == id)),
                    "{category}: {pair:?}"
                );
            }
        }
    }

    #[test]
    fn reviewed_nature_answers_match_the_question_in_both_languages() {
        // USGS: magnitude is distinct from shaking intensity. NOAA: hail grows in thunderstorms.
        for (lang, prefix, magnitude, storm, richter, hail) in [
            (
                Lang::En,
                "nature_",
                "magnitude",
                "thunderstorm",
                "Richter scale",
                "Hail",
            ),
            (
                Lang::Tr,
                "nature_tr_",
                "büyüklüğünü",
                "fırtına",
                "Richter ölçeği",
                "Dolu",
            ),
        ] {
            let content = PlayContent::for_lang(lang);
            let q = content.question(&format!("{prefix}041")).unwrap();
            assert!(q.question.contains(magnitude));
            assert_eq!(q.answer, richter);
            let q = content.question(&format!("{prefix}047")).unwrap();
            assert!(q.question.contains(storm));
            assert_eq!(q.answer, hail);
        }
    }

    #[test]
    fn quiz_packs_unique_and_complete_in_both_languages() {
        for lang in [Lang::En, Lang::Tr] {
            for cat in ["all", "ocean", "science", "nature"] {
                let s = State::new(Kind::Quiz, lang, "team", cat, 1);
                let Board::Quiz { questions, .. } = s.board else {
                    panic!()
                };
                assert_eq!(questions.len(), 5);
                let qs: Vec<_> = questions
                    .iter()
                    .map(|id| PlayContent::for_lang(lang).question(id).unwrap())
                    .collect();
                assert_eq!(
                    qs.iter()
                        .map(|q| &q.question)
                        .collect::<std::collections::HashSet<_>>()
                        .len(),
                    5
                );
                for q in qs {
                    assert_eq!(q.options.len(), 4);
                    assert!(q.options.contains(&q.answer));
                }
            }
        }
    }
    #[test]
    fn team_scores_and_quickfire_finish() {
        for lang in [Lang::En, Lang::Tr] {
            for mode in ["team", "quick"] {
                let mut s = started(Kind::Quiz, lang, mode);
                for _ in 0..5 {
                    let Board::Quiz { questions, order, .. } = &s.board else {
                        panic!()
                    };
                    let correct = PlayContent::for_lang(lang)
                        .question(&questions[s.round])
                        .unwrap()
                        .correct_slot(*order)
                        .unwrap();
                    s.act(0, &correct.to_string(), false).unwrap();
                    assert_eq!(s.act(0, "0", false), Err("answered"));
                    s.act(1, &((correct + 1) % 4).to_string(), false).unwrap();
                }
                assert!(s.done);
                assert_eq!(s.winners, vec![0]);
            }
        }
    }
    #[test]
    fn story_is_single_word_turn_based_and_filtered() {
        for lang in [Lang::En, Lang::Tr] {
            let mut s = started(Kind::Story, lang, "");
            assert_eq!(s.act(1, "sea", false), Err("turn"));
            assert_eq!(s.act(0, "two words", false), Err("word"));
            assert_eq!(s.act(0, "@everyone", false), Err("word"));
            for _ in 0..18 {
                let seat = s.turn;
                s.act(seat, if lang == Lang::Tr { "deniz" } else { "sea" }, false)
                    .unwrap();
            }
            assert!(s.done);
            assert_eq!(s.winners.len(), 2);
        }
    }
    #[test]
    fn words_reject_duplicates_and_wrong_turns() {
        for lang in [Lang::En, Lang::Tr] {
            for mode in ["relay", "chain", "longest", "shoal", "mixed"] {
                let mut s = started(Kind::Words, lang, mode);
                let Board::Words { fragment, .. } = &mut s.board else {
                    panic!()
                };
                *fragment = if lang == Lang::Tr { "de" } else { "ea" }.into();
                let word = if lang == Lang::Tr { "deniz" } else { "pearl" };
                s.act(0, word, false).unwrap();
                if let Board::Words { fragment, .. } = &mut s.board {
                    *fragment = if lang == Lang::Tr { "de" } else { "ea" }.into();
                }
                assert!(s.act(s.turn, word, false).is_err());
                for _ in 0..40 {
                    if s.done {
                        break;
                    }
                    s.timeout();
                }
                assert!(s.done);
            }
        }
    }
    #[test]
    fn room_rounds_never_repeat() {
        for lang in [Lang::En, Lang::Tr] {
            let mut s = started(Kind::Room, lang, "");
            let Board::Room { rooms, .. } = &s.board else { panic!() };
            assert_eq!(rooms.iter().collect::<std::collections::HashSet<_>>().len(), 4);
            for _ in 0..4 {
                let Board::Room { rooms, order, .. } = &s.board else {
                    panic!()
                };
                let pick = order.iter().position(|r| *r == rooms[s.round]).unwrap();
                s.act(0, &pick.to_string(), false).unwrap();
                s.timeout();
            }
            assert!(s.done);
            assert_eq!(s.winners, vec![0]);
        }
    }
    #[test]
    fn compass_covers_every_creature() {
        for lang in [Lang::En, Lang::Tr] {
            for creature in 0..8 {
                let mut s = started(Kind::Critter, lang, "");
                for bit in 0..3 {
                    s.act(0, if creature & (1 << bit) > 0 { "yes" } else { "no" }, false)
                        .unwrap();
                }
                let Board::Critter { bits, .. } = &s.board else {
                    panic!()
                };
                assert_eq!(*bits, creature);
                s.act(0, "yes", false).unwrap();
                assert!(s.done);
            }
        }
    }
    #[test]
    fn chess_does_not_end_a_legal_game_at_an_arbitrary_ply_limit() {
        let mut game = chess::Game::new();
        let mut seed = 17;
        for _ in 0..238 {
            let mut candidates = game.position.legal_moves();
            let mut chosen = None;
            while !candidates.is_empty() {
                let index = next(&mut seed) as usize % candidates.len();
                let mv = candidates.swap_remove(index);
                let mut after = game.clone();
                after.play(mv).unwrap();
                if after.outcome().is_none() {
                    chosen = Some(after);
                    break;
                }
            }
            game = chosen.expect("the fixture can continue legally");
        }
        let mut s = State::new(Kind::Chess, Lang::En, "easy", "all", 1);
        s.board = Board::Chess {
            moves: game.moves_text(),
        };
        for mv in game.position.legal_moves() {
            let mut candidate = s.clone();
            candidate.act(0, &mv.uci(), false).unwrap();
            let Board::Chess { moves } = &candidate.board else {
                panic!()
            };
            let after = chess::Game::from_moves(moves).unwrap();
            if after.outcome().is_none() {
                assert_eq!(after.plies(), 240);
                assert!(!candidate.done, "only a rules outcome, forfeit or timeout ends chess");
                return;
            }
        }
        panic!("the fixture has a non-terminal continuation");
    }

    #[test]
    fn chess_moves_ai_and_forfeit() {
        for lang in [Lang::En, Lang::Tr] {
            let mut s = started(Kind::Chess, lang, "easy");
            assert_eq!(s.act(0, "e2e5", false), Err("move"));
            s.act(0, "e2e4", false).unwrap();
            let Board::Chess { moves } = &s.board else { panic!() };
            assert_eq!(chess::Game::from_moves(moves).unwrap().plies(), 2);
            s.forfeit(0);
            assert!(s.done);
            assert!(s.reward_seats().is_empty());
        }
    }
    #[test]
    fn survival_alliances_require_reciprocity_and_finish() {
        for lang in [Lang::En, Lang::Tr] {
            let mut s = started(Kind::Survival, lang, "");
            s.act(0, "ally-1", false).unwrap();
            assert_eq!(s.seats[1].ally, None);
            s.act(1, "ally-0", false).unwrap();
            for _ in 0..8 {
                if s.done {
                    break;
                }
                for seat in 0..2 {
                    if !s.done && s.seats[seat].active {
                        s.act(seat, "shelter", false).unwrap();
                    }
                }
            }
            assert!(s.done);
        }
    }
    #[test]
    fn expiry_and_serialization_for_every_mode() {
        for lang in [Lang::En, Lang::Tr] {
            for kind in [
                Kind::Quiz,
                Kind::Words,
                Kind::Story,
                Kind::Room,
                Kind::Critter,
                Kind::Chess,
                Kind::Maze,
                Kind::Match,
                Kind::Survival,
            ] {
                let s = State::new(kind, lang, "relay", "all", 3);
                let mut restored: State = serde_json::from_str(&serde_json::to_string(&s).unwrap()).unwrap();
                restored.timeout();
                assert!(restored.done);
                assert!(restored.reward_seats().is_empty());
            }
        }
    }
}
