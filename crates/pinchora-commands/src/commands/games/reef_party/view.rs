use super::*;

pub(super) async fn view(app: &App, game: &GameSession, s: &State) -> anyhow::Result<InteractionResponseData> {
    let lang = s.lang;
    let title = s.kind.title(lang);
    let members = app.db.party_members(game.id).await?;
    let mut display = PartyDisplay {
        rows: score_rows(s, &members),
        subtitle: round_subtitle(s),
        footer: t!(
            lang,
            "party-v2.clock",
            seconds = (game.expires_at - app.now()).num_seconds().max(0)
        ),
        ..Default::default()
    };
    display.present(game, s, &members);
    let (subtitle, footer) = (&display.subtitle, &display.footer);
    let text = format!("{subtitle} · {footer}");
    let buttons = std::mem::take(&mut display.buttons);
    let png = if let Board::Chess { moves } = &s.board {
        chess_image(app, s, &members, moves, &display.prompt).await?
    } else {
        board_image(app, &title, lang, display).await?
    };
    let mut data = Cx::card(
        EmbedBuilder::new()
            .title(title)
            .description(text)
            .color(crate::infra::util::BRAND),
        "reef-game.png",
        png,
        buttons,
        false,
    );
    data.allowed_mentions = games::quiet();
    Ok(data)
}
#[derive(Default)]
struct PartyDisplay {
    prompt: String,
    buttons: Vec<Component>,
    cells: Vec<u8>,
    columns: usize,
    selected: Option<usize>,
    room: Option<usize>,
    critter: Option<usize>,
    subtitle: String,
    footer: String,
    rows: Vec<String>,
}

impl PartyDisplay {
    fn quiz_board(&mut self, game: &GameSession, s: &State, questions: &[String], order: &[usize; 4]) {
        let lang = s.lang;
        let button =
            |action: &str, label: &str| games::button(id(game, s, action), label, ButtonStyle::Secondary, false);
        let Self { prompt, buttons, .. } = self;

        if let Some(q) = PlayContent::for_lang(lang).question(&questions[s.round]) {
            *prompt = format!(
                "{}\n\n{}",
                q.question,
                q.shown(*order)
                    .iter()
                    .enumerate()
                    .map(|(i, o)| format!("{}. {o}", i + 1))
                    .collect::<Vec<_>>()
                    .join("\n")
            );
            buttons.push(party_games::row(
                (0..4).map(|i| button(&i.to_string(), &(i + 1).to_string())).collect(),
            ));
        }
    }

    fn words_board(
        &mut self,
        game: &GameSession,
        s: &State,
        members: &[pinchora_db::PartyMember],
        fragment: &str,
        last: &str,
    ) {
        let lang = s.lang;
        let name = |i| seat_name(lang, members, i);
        let button =
            |action: &str, label: &str| games::button(id(game, s, action), label, ButtonStyle::Secondary, false);
        let Self {
            prompt,
            buttons,
            footer,
            ..
        } = self;

        *prompt = t!(
            lang,
            "party-v2.fragment",
            letters = lang.upper(fragment),
            name = name(s.turn),
            last = last
        );
        *footer = t!(lang, &format!("party-v2.word-{}", word_mode(&s.mode, s.round)));
        buttons.push(party_games::row(vec![button("answer", &t!(lang, "party-v2.answer"))]));
    }

    fn story_board(&mut self, game: &GameSession, s: &State, members: &[pinchora_db::PartyMember], words: &[String]) {
        let lang = s.lang;
        let name = |i| seat_name(lang, members, i);
        let button =
            |action: &str, label: &str| games::button(id(game, s, action), label, ButtonStyle::Secondary, false);
        let Self {
            prompt,
            buttons,
            footer,
            ..
        } = self;

        *prompt = if words.is_empty() {
            t!(lang, "party-v2.story-seed")
        } else {
            words.join(" ")
        };
        *footer = t!(lang, "party-v2.turn-name", name = name(s.turn));
        buttons.push(party_games::row(vec![button("answer", &t!(lang, "party-v2.answer"))]));
    }

    fn room_board(&mut self, game: &GameSession, s: &State, rooms: &[usize], order: &[usize; 4]) {
        let lang = s.lang;
        let button =
            |action: &str, label: &str| games::button(id(game, s, action), label, ButtonStyle::Secondary, false);
        let Self {
            prompt, buttons, room, ..
        } = self;

        *room = Some(rooms[s.round]);
        *prompt = t!(lang, "party-v2.room-question");
        buttons.push(party_games::row(
            order
                .iter()
                .enumerate()
                .map(|(i, r)| button(&i.to_string(), &t!(lang, &format!("party-v2.room-{r}"))))
                .collect(),
        ));
    }

    fn critter_board(&mut self, game: &GameSession, s: &State, bits: &usize, step: &usize) {
        let lang = s.lang;
        let button =
            |action: &str, label: &str| games::button(id(game, s, action), label, ButtonStyle::Secondary, false);
        let Self {
            prompt,
            buttons,
            critter,
            ..
        } = self;

        *prompt = if *step < 3 {
            format!(
                "{}\n\n{}",
                t!(lang, "party-v2.think"),
                t!(lang, &format!("party-v2.clue-{step}"))
            )
        } else {
            *critter = Some(*bits);
            t!(
                lang,
                "party-v2.critter-result",
                name = t!(lang, &format!("party-v2.critter-{bits}"))
            )
        };
        buttons.push(party_games::row(vec![
            button("yes", &t!(lang, "party-v2.yes")),
            button("no", &t!(lang, "party-v2.no")),
        ]));
    }

    fn chess_board(&mut self, game: &GameSession, s: &State) {
        let lang = s.lang;
        let button =
            |action: &str, label: &str| games::button(id(game, s, action), label, ButtonStyle::Secondary, false);
        let Self { prompt, buttons, .. } = self;

        *prompt = t!(lang, "party-v2.chess-prompt");
        buttons.push(party_games::row(vec![button("answer", &t!(lang, "party-v2.answer"))]));
    }

    fn maze_board(&mut self, game: &GameSession, s: &State, board: &pinchora_game::games::reef_party::puzzles::Maze) {
        let lang = s.lang;
        let button =
            |action: &str, label: &str| games::button(id(game, s, action), label, ButtonStyle::Secondary, false);
        let Self {
            prompt,
            buttons,
            cells,
            columns,
            ..
        } = self;

        *cells = board.cells.iter().map(|v| if *v { 5 } else { 0 }).collect();
        cells[48] = 3;
        cells[board.at] = 1;
        *columns = 7;
        *prompt = t!(lang, "party-v2.maze-prompt");
        buttons.push(party_games::row(
            ["up", "left", "down", "right"]
                .iter()
                .map(|a| button(a, &t!(lang, &format!("party-v2.{a}"))))
                .collect(),
        ));
    }

    fn match_board(&mut self, game: &GameSession, s: &State, board: &pinchora_game::games::reef_party::puzzles::Match) {
        let lang = s.lang;
        let button =
            |action: &str, label: &str| games::button(id(game, s, action), label, ButtonStyle::Secondary, false);
        let Self {
            prompt,
            buttons,
            cells,
            columns,
            selected,
            ..
        } = self;

        *cells = board.cells.clone();
        *columns = 4;
        *selected = board.selected;
        *prompt = t!(lang, "party-v2.match-prompt", score = board.score, moves = board.moves);
        for row in 0..4 {
            buttons.push(party_games::row(
                (row * 4..row * 4 + 4)
                    .map(|i| button(&i.to_string(), &(i + 1).to_string()))
                    .collect(),
            ));
        }
    }

    fn survival_board(&mut self, game: &GameSession, s: &State, members: &[pinchora_db::PartyMember], event: &usize) {
        let lang = s.lang;
        let name = |i| seat_name(lang, members, i);
        let button =
            |action: &str, label: &str| games::button(id(game, s, action), label, ButtonStyle::Secondary, false);
        let Self {
            prompt, buttons, rows, ..
        } = self;

        *prompt = if s.round == 0 {
            t!(lang, "party-v2.rules-lastreef")
        } else {
            t!(lang, &format!("party-v2.event-{event}"))
        };
        prompt.push_str(&format!("\n\n{}", t!(lang, "party-v2.alliance-rule")));
        for (i, seat) in s.seats.iter().enumerate().filter(|(_, p)| p.active) {
            rows[i] = format!(
                "{}. {} ♥{} {}",
                i + 1,
                name(i),
                seat.lives,
                seat.ally.map_or(String::new(), |a| format!("↔{}", a + 1))
            );
        }
        buttons.push(party_games::row(
            ["shelter", "forage", "explore"]
                .iter()
                .map(|a| button(a, &t!(lang, &format!("party-v2.{a}"))))
                .collect(),
        ));
        let allies: Vec<_> = s
            .seats
            .iter()
            .enumerate()
            .filter(|(_, p)| p.active)
            .map(|(i, _)| button(&format!("ally-{i}"), &t!(lang, "party-v2.ally", seat = i + 1)))
            .collect();
        for chunk in allies.chunks(5) {
            buttons.push(party_games::row(chunk.to_vec()));
        }
    }

    fn lobby(&mut self, game: &GameSession, s: &State) {
        let lang = s.lang;
        let button =
            |action: &str, label: &str| games::button(id(game, s, action), label, ButtonStyle::Secondary, false);
        let Self {
            prompt,
            buttons,
            subtitle,
            ..
        } = self;

        *prompt = t!(lang, &format!("party-v2.rules-{}", s.kind.key()));
        buttons.push(party_games::row(vec![
            button("join", &t!(lang, "party-v2.join")),
            button("start", &t!(lang, "party-v2.start")),
            button("leave", &t!(lang, "party-v2.leave")),
        ]));
        *subtitle = t!(lang, "party-v2.lobby");
    }

    fn finished(&mut self, s: &State, members: &[pinchora_db::PartyMember]) {
        let lang = s.lang;
        let name = |i| seat_name(lang, members, i);
        let Self {
            prompt,
            critter,
            subtitle,
            footer,
            ..
        } = self;

        *subtitle = t!(lang, "party-v2.finished");
        *prompt = if s.cancelled {
            t!(lang, "party-v2.cancelled")
        } else if s.winners.is_empty() {
            t!(lang, "party-v2.no-winner")
        } else {
            t!(
                lang,
                "party-v2.winners",
                names = s.winners.iter().map(|&i| name(i)).collect::<Vec<_>>().join(", ")
            )
        };
        if let Board::Story { words } = &s.board {
            *prompt = format!("{prompt}\n\n{}", words.join(" "));
        }
        if let Board::Critter { bits, .. } = &s.board {
            *critter = Some(*bits);
        }
        if let Board::Survival { .. } = &s.board {
            *prompt = format!("{prompt}\n\n{}", s.notice);
        }
        *footer = t!(
            lang,
            if s.kind == Kind::Critter {
                "party-v2.no-reward"
            } else {
                "party-v2.reward"
            }
        );
    }

    fn present(&mut self, game: &GameSession, s: &State, members: &[pinchora_db::PartyMember]) {
        if s.lobby {
            self.lobby(game, s);
        } else if s.done {
            self.finished(s, members);
        } else {
            match &s.board {
                Board::Quiz { questions, order, .. } => self.quiz_board(game, s, questions, order),
                Board::Words { fragment, last, .. } => self.words_board(game, s, members, fragment, last),
                Board::Story { words } => self.story_board(game, s, members, words),
                Board::Room { rooms, order, .. } => self.room_board(game, s, rooms, order),
                Board::Critter { bits, step } => self.critter_board(game, s, bits, step),
                Board::Chess { .. } => self.chess_board(game, s),
                Board::Maze(board) => self.maze_board(game, s, board),
                Board::Match(board) => self.match_board(game, s, board),
                Board::Survival { event, .. } => self.survival_board(game, s, members, event),
            }
            let lang = s.lang;
            let button =
                |action: &str, label: &str| games::button(id(game, s, action), label, ButtonStyle::Secondary, false);
            self.buttons.push(party_games::row(vec![
                button("leave", &t!(lang, "party-v2.forfeit")),
                button("cancel", &t!(lang, "party-v2.cancel")),
            ]));
        }
    }
}

fn seat_name(lang: Lang, members: &[pinchora_db::PartyMember], i: usize) -> String {
    members
        .iter()
        .find(|m| m.choice as usize == i)
        .map_or_else(|| t!(lang, "party-v2.departed"), |m| m.name.clone())
}

fn score_rows(s: &State, members: &[pinchora_db::PartyMember]) -> Vec<String> {
    let lang = s.lang;
    let name = |i| seat_name(lang, members, i);
    let mut rows = Vec::new();
    for (i, seat) in s.seats.iter().enumerate() {
        let badge = if s.winners.contains(&i) {
            "★"
        } else if !seat.active {
            "×"
        } else if s.turn == i {
            "›"
        } else {
            "·"
        };
        let team = if s.kind == Kind::Quiz && s.mode == "team" {
            format!(
                " {}",
                t!(
                    lang,
                    if seat.team == 0 {
                        "party-v2.coral"
                    } else {
                        "party-v2.kelp"
                    }
                )
            )
        } else {
            String::new()
        };
        rows.push(format!("{badge} {}. {}{}  {}", i + 1, name(i), team, seat.score));
    }
    rows
}

fn round_subtitle(s: &State) -> String {
    let lang = s.lang;
    let mut subtitle = t!(lang, "party-v2.round", round = s.round + 1);
    if s.kind == Kind::Quiz && s.mode == "team" && !s.lobby && !s.done {
        let total = |team| {
            s.seats
                .iter()
                .filter(|p| p.active && p.team == team)
                .map(|p| p.score)
                .sum::<u32>()
        };
        subtitle = t!(
            lang,
            "party-v2.team-score",
            round = s.round + 1,
            coral = total(0),
            kelp = total(1)
        );
    }
    subtitle
}

async fn chess_image(
    app: &App,
    s: &State,
    members: &[pinchora_db::PartyMember],
    moves: &str,
    prompt: &str,
) -> anyhow::Result<Vec<u8>> {
    let lang = s.lang;
    let name = |i| seat_name(lang, members, i);
    // Replaying a long persisted game also belongs off the gateway executor.
    let moves = moves.to_owned();
    let chess = tokio::task::spawn_blocking(move || chess::Game::from_moves(&moves))
        .await?
        .ok_or_else(|| anyhow::anyhow!("invalid saved chess"))?;
    let pieces = std::array::from_fn(|i| {
        chess.position.piece_at(i as u8).map_or(0, |p| {
            let k = match p.kind {
                chess::Kind::Pawn => 1,
                chess::Kind::Knight => 2,
                chess::Kind::Bishop => 3,
                chess::Kind::Rook => 4,
                chess::Kind::Queen => 5,
                chess::Kind::King => 6,
            };
            k + if p.color == chess::Color::Black { 6 } else { 0 }
        })
    });
    let host = name(0);
    let captain = t!(lang, "party-v2.captain");
    let last = chess.moves().last().map(|m| (m.from, m.to));
    let check = chess
        .position
        .in_check()
        .then(|| chess.position.king_square(chess.position.turn))
        .flatten();
    let done = s.done;
    let status = if s.done {
        prompt.to_owned()
    } else {
        t!(lang, "party-v2.chess-prompt")
    };
    app.render(move |r| {
        r.chess_card(&ChessCard {
            lang,
            pieces: &pieces,
            names: [&host, &captain],
            turn: if done { None } else { Some(0) },
            winner: None,
            last,
            selected: None,
            targets: &[],
            check,
            status: &status,
            pot: 0,
            theme: Theme::reef(),
        })
    })
    .await
}

async fn board_image(app: &App, title: &str, lang: Lang, display: PartyDisplay) -> anyhow::Result<Vec<u8>> {
    let PartyDisplay {
        subtitle,
        prompt,
        footer,
        rows,
        cells,
        columns,
        selected,
        room,
        critter,
        ..
    } = display;
    let title = lang.upper(title);
    app.render(move |r| {
        r.svg_to_png(
            &ReefCard {
                title: &title,
                subtitle: &subtitle,
                prompt: &prompt,
                footer: &footer,
                rows: &rows,
                cells: &cells,
                columns,
                selected,
                room,
                critter,
            }
            .svg()?,
        )
    })
    .await
}
