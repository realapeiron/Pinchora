use super::*;

impl State {
    pub(super) fn act_quiz(&mut self, seat: usize, input: &str) -> Result<(), &'static str> {
        let Board::Quiz {
            questions,
            order,
            answered,
        } = &mut self.board
        else {
            unreachable!()
        };
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
        Ok(())
    }

    pub(super) fn act_room(&mut self, seat: usize, input: &str) -> Result<(), &'static str> {
        let Board::Room { rooms, order, answered } = &mut self.board else {
            unreachable!()
        };
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
        Ok(())
    }

    pub(super) fn act_words(&mut self, seat: usize, input: &str, strict: bool) -> Result<(), &'static str> {
        let Board::Words {
            fragment,
            used,
            last,
            best,
            longest,
        } = &mut self.board
        else {
            unreachable!()
        };
        let mode = word_mode(&self.mode, self.round);
        if (mode == "relay" || mode == "chain") && seat != self.turn {
            return Err("turn");
        }
        let word =
            hotpearl::check_word(hotpearl::dictionary_for(self.lang), fragment, input, used).map_err(|_| "word")?;
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
            *fragment = crate::games::reef_party::fragment(self.lang, self.seed + self.round as u64).into();
            self.next_turn();
            if self.round >= 24 {
                self.finish();
            }
        }
        Ok(())
    }

    pub(super) fn act_story(&mut self, seat: usize, input: &str, strict: bool) -> Result<(), &'static str> {
        let Board::Story { words } = &mut self.board else {
            unreachable!()
        };
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
        Ok(())
    }

    pub(super) fn act_critter(&mut self, seat: usize, input: &str) -> Result<(), &'static str> {
        let Board::Critter { bits, step } = &mut self.board else {
            unreachable!()
        };
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
        Ok(())
    }

    pub(super) fn act_chess(&mut self, seat: usize, input: &str) -> Result<(), &'static str> {
        let Board::Chess { moves } = &mut self.board else {
            unreachable!()
        };
        let mut game = chess::Game::from_moves(moves).ok_or("invalid")?;
        game.play(chess::Move::from_uci(&input.to_ascii_lowercase()).ok_or("move")?)
            .ok_or("move")?;
        if game.outcome().is_none() {
            let level = match self.mode.as_str() {
                "easy" => 1,
                "hard" => 3,
                _ => 2,
            };
            if let Some(mv) = crate::games::reef_party::chess_ai::choose(&game.position, level).mv {
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
        Ok(())
    }

    pub(super) fn act_maze(&mut self, seat: usize, input: &str) -> Result<(), &'static str> {
        let Board::Maze(board) = &mut self.board else {
            unreachable!()
        };
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
        Ok(())
    }

    pub(super) fn act_match(&mut self, seat: usize, input: &str) -> Result<(), &'static str> {
        let Board::Match(board) = &mut self.board else {
            unreachable!()
        };
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
        Ok(())
    }

    pub(super) fn act_survival(&mut self, seat: usize, input: &str) -> Result<(), &'static str> {
        let Board::Survival { choices, .. } = &mut self.board else {
            unreachable!()
        };
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
        Ok(())
    }
}
