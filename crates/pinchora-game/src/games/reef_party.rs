//! Original reef games. Serializable rules contain seat numbers, never Discord identities.
mod actions;
pub mod chess_ai;
pub mod puzzles;
use crate::{
    party::{filter, hotpearl},
    play::{PlayContent, chess, trivia},
};
use pinchora_core::infra::i18n::Lang;
use pinchora_core::t;
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
    Maze(crate::games::reef_party::puzzles::Maze),
    Match(crate::games::reef_party::puzzles::Match),
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
            Kind::Maze => Board::Maze(crate::games::reef_party::puzzles::Maze::new(seed)),
            Kind::Match => Board::Match(crate::games::reef_party::puzzles::Match::new(seed)),
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
        match &self.board {
            Board::Quiz { .. } => self.act_quiz(seat, input)?,
            Board::Room { .. } => self.act_room(seat, input)?,
            Board::Words { .. } => self.act_words(seat, input, strict)?,
            Board::Story { .. } => self.act_story(seat, input, strict)?,
            Board::Critter { .. } => self.act_critter(seat, input)?,
            Board::Chess { .. } => self.act_chess(seat, input)?,
            Board::Maze(..) => self.act_maze(seat, input)?,
            Board::Match(..) => self.act_match(seat, input)?,
            Board::Survival { .. } => self.act_survival(seat, input)?,
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
                    *fragment = crate::games::reef_party::fragment(self.lang, self.seed + self.round as u64).into();
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
#[path = "reef_party/tests.rs"]
mod tests;
