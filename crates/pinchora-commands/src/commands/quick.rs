//! Small private canvas games backed by the shared solo session table.
use crate::{
    App, Cx,
    commands::games::{button, pay_win},
    util::{BRAND, SUCCESS},
};
use pinchora_core::{i18n::Lang, t};
use pinchora_db::{SoloGame, SoloStep};
use pinchora_game::play::{
    Game, PlayContent,
    quick::{self as rules, Memory, Mines},
};
use pinchora_render::{QuickCard, QuickDetails, QuickHeat, QuickKind, Theme};
use rand::seq::SliceRandom;
use serde::{Deserialize, Serialize};
use std::str::Split;
use twilight_model::{
    channel::message::{
        Component,
        component::{ActionRow, ButtonStyle, Label, TextInput, TextInputStyle},
    },
    http::interaction::InteractionResponseData,
};
use twilight_util::builder::embed::EmbedBuilder;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    Memory,
    Mines,
    HigherLower,
    Anagram,
    Typing,
    GuessNumber,
}
impl Kind {
    pub fn key(self) -> &'static str {
        match self {
            Self::Memory => "memory",
            Self::Mines => "minesweeper",
            Self::HigherLower => "higherlower",
            Self::Anagram => "anagram",
            Self::Typing => "typing",
            Self::GuessNumber => "guessnumber",
        }
    }
    pub fn prefix(self) -> &'static str {
        match self {
            Self::Memory => "mm",
            Self::Mines => "mi",
            Self::HigherLower => "yl",
            Self::Anagram => "ag",
            Self::Typing => "yt",
            Self::GuessNumber => "gn",
        }
    }
    pub fn game(self) -> Game {
        match self {
            Self::Memory => Game::Memory,
            Self::Mines => Game::Minesweeper,
            Self::HigherLower => Game::HigherLower,
            Self::Anagram => Game::Anagram,
            Self::Typing => Game::Typing,
            Self::GuessNumber => Game::GuessNumber,
        }
    }
    pub fn from_prefix(prefix: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|k| k.prefix() == prefix)
    }
    pub const ALL: [Self; 6] = [
        Self::Memory,
        Self::Mines,
        Self::HigherLower,
        Self::Anagram,
        Self::Typing,
        Self::GuessNumber,
    ];
    fn seconds(self) -> i64 {
        match self {
            Self::Memory => 300,
            Self::Mines => 600,
            Self::HigherLower => 120,
            Self::Anagram => 90,
            Self::Typing => 90,
            Self::GuessNumber => 180,
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
struct State {
    lang: Lang,
    expires: i64,
    started: i64,
    nonce: u64,
    payload: Payload,
    note: String,
    /// Card-only metadata; defaults preserve sessions created before illustrated cards.
    #[serde(default)]
    display_last: Option<usize>,
    #[serde(default)]
    display_wpm: Option<u32>,
}
#[derive(Clone, Serialize, Deserialize)]
enum Payload {
    Memory(Memory),
    Mines(Mines),
    HigherLower { card: u8, streak: u8 },
    Anagram { word: String, scrambled: String, tries: u8 },
    Typing { sentence: String },
    GuessNumber { answer: u8, tries: u8, last: Option<u8> },
}

fn new_state(app: &App, kind: Kind, lang: Lang, size: usize) -> State {
    let now = app.now().timestamp();
    let payload = match kind {
        Kind::Memory => {
            let mut pairs: Vec<u8> = (0..8).flat_map(|n| [n, n]).collect();
            pairs.shuffle(&mut rand::rng());
            Payload::Memory(Memory::new(pairs).expect("valid pairs"))
        }
        Kind::Mines => Payload::Mines(Mines::new(size).expect("validated size")),
        Kind::HigherLower => Payload::HigherLower {
            card: rand::random_range(1..=13),
            streak: 0,
        },
        Kind::Anagram => {
            let words = &PlayContent::for_lang(lang).words.answers;
            let word = words[rand::random_range(0..words.len())].clone();
            let mut letters: Vec<char> = word.chars().collect();
            for _ in 0..8 {
                letters.shuffle(&mut rand::rng());
                if letters.iter().collect::<String>() != word {
                    break;
                }
            }
            if letters.iter().collect::<String>() == word
                && let Some(other) = letters.iter().position(|&c| c != letters[0])
            {
                letters.swap(0, other);
            }
            Payload::Anagram {
                word,
                scrambled: letters.into_iter().collect(),
                tries: 0,
            }
        }
        Kind::Typing => Payload::Typing {
            sentence: t!(lang, &format!("quick.sentences.item-{}", rand::random_range(0..6))),
        },
        Kind::GuessNumber => Payload::GuessNumber {
            answer: rand::random_range(1..=100),
            tries: 0,
            last: None,
        },
    };
    State {
        lang,
        expires: now + kind.seconds(),
        started: now,
        nonce: rand::random(),
        payload,
        note: String::new(),
        display_last: None,
        display_wpm: None,
    }
}

pub(crate) async fn start(cx: &Cx, kind: Kind, size: usize) -> anyhow::Result<()> {
    let me = cx.author()?.id.get();
    let existing = cx.app.db.solo_game(me, kind.key()).await?;
    if let Some(ref old) = existing
        && !old.over()
        && serde_json::from_str::<State>(&old.state).is_ok_and(|s| s.expires <= cx.app.now().timestamp())
        && cx.app.db.expire_solo(me, kind.key(), &old.state).await?
    {
        cx.app.db.record_game(me, kind.key(), false).await?;
    }
    let game = match existing {
        Some(ref g)
            if !g.over()
                && serde_json::from_str::<State>(&g.state).is_ok_and(|s| s.expires > cx.app.now().timestamp()) =>
        {
            g.clone()
        }
        _ => {
            let state = new_state(&cx.app, kind, cx.lang(), size);
            cx.app
                .db
                .start_solo(me, kind.key(), &serde_json::to_string(&state)?, true)
                .await?
        }
    };
    cx.reply(view(&cx.app, kind, &game).await?).await
}

fn id(kind: Kind, game: &SoloGame, state: &State, action: &str) -> String {
    format!("{}:{action}:{}:{}", kind.prefix(), game.moves, state.nonce)
}
fn row(buttons: Vec<Component>) -> Component {
    Component::ActionRow(ActionRow {
        id: None,
        components: buttons,
    })
}
fn small_button(
    kind: Kind,
    game: &SoloGame,
    state: &State,
    action: &str,
    label: &str,
    style: ButtonStyle,
) -> Component {
    button(id(kind, game, state, action), label, style, false)
}
async fn view(app: &App, kind: Kind, game: &SoloGame) -> anyhow::Result<InteractionResponseData> {
    let s: State = serde_json::from_str(&game.state)?;
    let lang = s.lang;
    let over = game.over() || s.expires <= app.now().timestamp();
    let title = kind.game().name(lang);
    let (cells, columns, prompt, status, distort) = match &s.payload {
        Payload::Memory(m) => (
            m.pairs
                .iter()
                .enumerate()
                .map(|(i, p)| {
                    if m.shown[i] || m.first == Some(i) || m.last.is_some_and(|(a, b)| a == i || b == i) || over {
                        format!("{}", p + 1)
                    } else {
                        "?".to_owned()
                    }
                })
                .collect(),
            4,
            String::new(),
            t!(lang, "quick.memory-status", misses = m.misses),
            false,
        ),
        Payload::Mines(m) => (
            m.open
                .iter()
                .enumerate()
                .map(|(i, open)| {
                    if (over && m.mines[i]) || (*open && m.mines[i]) {
                        "*".to_owned()
                    } else if *open {
                        let n = m.count(i);
                        if n == 0 { "·".to_owned() } else { n.to_string() }
                    } else if m.flags[i] {
                        "F".to_owned()
                    } else {
                        "?".to_owned()
                    }
                })
                .collect(),
            m.size,
            String::new(),
            t!(
                lang,
                "quick.mines-status",
                flags = m.flags.iter().filter(|&&v| v).count()
            ),
            false,
        ),
        Payload::HigherLower { card, streak } => (
            vec![],
            0,
            card.to_string(),
            t!(lang, "quick.higher-status", streak = streak),
            false,
        ),
        Payload::Anagram { scrambled, tries, .. } => (
            vec![],
            0,
            scrambled.clone(),
            t!(lang, "quick.tries", tries = tries),
            false,
        ),
        Payload::Typing { sentence } => (vec![], 0, sentence.clone(), t!(lang, "quick.type-status"), true),
        Payload::GuessNumber { tries, last, .. } => (
            vec![],
            0,
            last.map_or_else(|| "?".to_owned(), |n| n.to_string()),
            t!(lang, "quick.tries", tries = tries),
            false,
        ),
    };
    let subtitle = t!(lang, "quick.subtitle");
    let card_kind = match kind {
        Kind::Memory => QuickKind::Memory,
        Kind::Mines => QuickKind::Minesweeper,
        Kind::HigherLower => QuickKind::HigherLower,
        Kind::Anagram => QuickKind::Anagram,
        Kind::Typing => QuickKind::Typing,
        Kind::GuessNumber => QuickKind::GuessNumber,
    };
    let matched: Vec<usize> = match &s.payload {
        Payload::Memory(m) => m
            .shown
            .iter()
            .enumerate()
            .filter_map(|(i, shown)| shown.then_some(i))
            .collect(),
        _ => Vec::new(),
    };
    let last = match &s.payload {
        Payload::Memory(m) => m.first.or_else(|| m.last.map(|(_, b)| b)),
        _ => s.display_last,
    };
    let heat = match &s.payload {
        Payload::GuessNumber {
            answer, last: Some(n), ..
        } => Some(if answer == n {
            QuickHeat::Found
        } else {
            match rules::temperature(*answer, *n) {
                rules::Temperature::Cold => QuickHeat::Cold,
                rules::Temperature::Warm => QuickHeat::Warm,
                rules::Temperature::Hot => QuickHeat::Hot,
            }
        }),
        _ => None,
    };
    let progress = match &s.payload {
        Payload::HigherLower { streak, .. } => *streak,
        _ => 0,
    };
    let wpm = s.display_wpm;
    let status = if over {
        t!(lang, "quick.finished", note = s.note)
    } else {
        status
    };
    let png = app
        .render(move |r| {
            r.quick_card(&QuickCard {
                kind: card_kind,
                details: QuickDetails {
                    last,
                    matched: &matched,
                    finished: over,
                    heat,
                    progress,
                    wpm,
                    ..Default::default()
                },
                lang,
                title: &title,
                subtitle: &subtitle,
                cells: &cells,
                columns,
                prompt: &prompt,
                status: &status,
                theme: Theme::reef(),
                distort,
            })
        })
        .await?;
    let mut buttons = Vec::new();
    if !over {
        match &s.payload {
            Payload::Memory(m) => {
                for y in 0..4 {
                    buttons.push(row((0..4)
                        .map(|x| {
                            let n = y * 4 + x;
                            button(
                                id(kind, game, &s, &n.to_string()),
                                &format!("{}{}", (b'A' + y as u8) as char, x + 1),
                                ButtonStyle::Secondary,
                                m.shown[n] || m.first == Some(n),
                            )
                        })
                        .collect()));
                }
            }
            Payload::Mines(m) => {
                buttons.push(row(vec![
                    small_button(
                        kind,
                        game,
                        &s,
                        "coord",
                        &t!(lang, "quick.coordinate"),
                        ButtonStyle::Primary,
                    ),
                    small_button(
                        kind,
                        game,
                        &s,
                        "mode",
                        &t!(lang, if m.flag_mode { "quick.flag-on" } else { "quick.flag-off" }),
                        ButtonStyle::Secondary,
                    ),
                ]));
            }
            Payload::HigherLower { .. } => buttons.push(row(vec![
                small_button(kind, game, &s, "hi", &t!(lang, "quick.higher"), ButtonStyle::Primary),
                small_button(kind, game, &s, "lo", &t!(lang, "quick.lower"), ButtonStyle::Secondary),
            ])),
            _ => buttons.push(row(vec![small_button(
                kind,
                game,
                &s,
                "answer",
                &t!(lang, "quick.answer"),
                ButtonStyle::Primary,
            )])),
        }
    } else {
        buttons.push(row(vec![small_button(
            kind,
            game,
            &s,
            "new",
            &t!(lang, "quick.new"),
            ButtonStyle::Primary,
        )]));
    }
    let description = if over {
        t!(lang, "quick.finished", note = s.note)
    } else {
        t!(lang, &format!("quick.{}.intro", kind.key()), note = s.note)
    };
    Ok(Cx::card(
        EmbedBuilder::new()
            .color(if over { SUCCESS } else { BRAND })
            .title(kind.game().name(lang))
            .description(description),
        &format!("{}.png", kind.key()),
        png,
        buttons,
        true,
    ))
}

#[allow(deprecated)]
fn field(lang: Lang, kind: Kind) -> Component {
    Component::Label(Label {
        component: Box::new(Component::TextInput(TextInput {
            custom_id: "answer".to_owned(),
            id: None,
            label: None,
            max_length: Some(if kind == Kind::Typing { 150 } else { 30 }),
            min_length: Some(1),
            placeholder: None,
            required: Some(true),
            style: TextInputStyle::Short,
            value: None,
        })),
        description: None,
        id: None,
        label: t!(lang, "quick.answer"),
    })
}

pub(crate) async fn on_button(cx: &Cx, kind: Kind, mut parts: Split<'_, char>) -> anyhow::Result<()> {
    let (action, moves, nonce) = (
        parts.next().unwrap_or_default(),
        parts.next().and_then(|v| v.parse::<i32>().ok()),
        parts.next().and_then(|v| v.parse::<u64>().ok()),
    );
    let me = cx.author()?.id.get();
    let Some(game) = cx.app.db.solo_game(me, kind.key()).await? else {
        return cx.reply_ephemeral(t!(cx.lang(), "quick.expired")).await;
    };
    let s: State = serde_json::from_str(&game.state)?;
    if moves != Some(game.moves) || nonce != Some(s.nonce) {
        return cx.reply_ephemeral(t!(cx.lang(), "quick.expired")).await;
    }
    if action == "new" && (game.over() || s.expires <= cx.app.now().timestamp()) {
        if !game.over() && cx.app.db.expire_solo(me, kind.key(), &game.state).await? {
            cx.app.db.record_game(me, kind.key(), false).await?;
        }
        let size = if let Payload::Mines(m) = s.payload { m.size } else { 8 };
        let next = new_state(&cx.app, kind, s.lang, size);
        if !cx
            .app
            .db
            .restart_solo(me, kind.key(), &game.state, game.moves, &serde_json::to_string(&next)?)
            .await?
        {
            return cx.reply_ephemeral(t!(cx.lang(), "quick.expired")).await;
        }
        let game = cx
            .app
            .db
            .solo_game(me, kind.key())
            .await?
            .ok_or_else(|| anyhow::anyhow!("quick game vanished"))?;
        return cx.update(view(&cx.app, kind, &game).await?).await;
    }
    if game.over() || s.expires <= cx.app.now().timestamp() {
        return cx.update(view(&cx.app, kind, &game).await?).await;
    }
    if action == "answer" || action == "coord" {
        return cx
            .modal(
                id(kind, &game, &s, "modal"),
                &kind.game().name(s.lang),
                vec![field(s.lang, kind)],
            )
            .await;
    }
    step(cx, kind, game, s, action).await
}

pub(crate) async fn on_modal(
    cx: &Cx,
    kind: Kind,
    mut parts: Split<'_, char>,
    fields: &[(String, String)],
) -> anyhow::Result<()> {
    let (action, moves, nonce) = (
        parts.next(),
        parts.next().and_then(|v| v.parse::<i32>().ok()),
        parts.next().and_then(|v| v.parse::<u64>().ok()),
    );
    if action != Some("modal") {
        return cx.reply_ephemeral(t!(cx.lang(), "quick.expired")).await;
    }
    let me = cx.author()?.id.get();
    let Some(game) = cx.app.db.solo_game(me, kind.key()).await? else {
        return cx.reply_ephemeral(t!(cx.lang(), "quick.expired")).await;
    };
    let s: State = serde_json::from_str(&game.state)?;
    if game.over() || s.expires <= cx.app.now().timestamp() || moves != Some(game.moves) || nonce != Some(s.nonce) {
        return cx.reply_ephemeral(t!(cx.lang(), "quick.expired")).await;
    }
    let answer = fields
        .iter()
        .find(|(id, _)| id == "answer")
        .map_or("", |(_, text)| text.as_str())
        .trim();
    step(cx, kind, game, s, answer).await
}

async fn step(cx: &Cx, kind: Kind, game: SoloGame, mut s: State, action: &str) -> anyhow::Result<()> {
    let mut won = false;
    let mut over = false;
    match &mut s.payload {
        Payload::Memory(m) => {
            let Ok(cell) = action.parse::<usize>() else {
                return cx.reply_ephemeral(t!(s.lang, "quick.invalid")).await;
            };
            let Some(matched) = m.pick(cell) else {
                return cx.reply_ephemeral(t!(s.lang, "quick.invalid")).await;
            };
            s.note = if matched {
                t!(s.lang, "quick.match")
            } else {
                String::new()
            };
            won = m.won();
            over = won || m.misses >= 24;
        }
        Payload::Mines(m) => {
            if action == "mode" {
                m.flag_mode = !m.flag_mode;
            } else {
                let Some((&first, rest)) = action.as_bytes().split_first() else {
                    return cx.reply_ephemeral(t!(s.lang, "quick.invalid")).await;
                };
                let col = first.to_ascii_uppercase() as usize;
                let row = std::str::from_utf8(rest).ok().and_then(|v| v.parse::<usize>().ok());
                let cell = row
                    .and_then(|r| r.checked_sub(1))
                    .and_then(|r| r.checked_mul(m.size))
                    .and_then(|r| r.checked_add(col.saturating_sub('A' as usize)));
                let Some(cell) =
                    cell.filter(|&c| c < m.size * m.size && col >= 'A' as usize && col < 'A' as usize + m.size)
                else {
                    return cx.reply_ephemeral(t!(s.lang, "quick.invalid")).await;
                };
                if m.flag_mode {
                    if !m.flag(cell) {
                        return cx.reply_ephemeral(t!(s.lang, "quick.invalid")).await;
                    }
                } else {
                    let mut draws: Vec<usize> = (0..m.size * m.size).collect();
                    draws.shuffle(&mut rand::rng());
                    if m.reveal(cell, &draws).is_none() {
                        return cx.reply_ephemeral(t!(s.lang, "quick.invalid")).await;
                    }
                }
                s.display_last = Some(cell);
                won = m.won();
                over = won || m.lost;
            }
        }
        Payload::HigherLower { card, streak } => {
            if action != "hi" && action != "lo" {
                return cx.reply_ephemeral(t!(s.lang, "quick.invalid")).await;
            }
            let next = rand::random_range(1..=13);
            let correct = rules::higher_correct(*card, next, action == "hi");
            s.note = t!(s.lang, "quick.drew", card = next);
            *card = next;
            if correct {
                *streak += 1;
                won = *streak >= 8;
                over = won;
            } else {
                over = true;
            }
        }
        Payload::Anagram { word, tries, .. } => {
            *tries += 1;
            won = rules::anagram_correct(word, action);
            over = won || *tries >= 3;
            if over {
                s.note = t!(s.lang, "quick.word", word = word);
            }
        }
        Payload::Typing { sentence } => {
            let elapsed = (cx.app.now().timestamp() - s.started).max(1);
            let (correct, wpm) = rules::typing_score(sentence, action, elapsed);
            won = correct;
            over = true;
            s.display_wpm = Some(wpm.max(0) as u32);
            s.note = t!(s.lang, "quick.wpm", wpm = wpm);
        }
        Payload::GuessNumber { answer, tries, last } => {
            let Ok(n) = action.parse::<u8>() else {
                return cx.reply_ephemeral(t!(s.lang, "quick.invalid")).await;
            };
            if !(1..=100).contains(&n) {
                return cx.reply_ephemeral(t!(s.lang, "quick.invalid")).await;
            }
            *tries += 1;
            *last = Some(n);
            won = n == *answer;
            over = won || *tries >= 8;
            s.note = if won {
                t!(s.lang, "quick.correct")
            } else {
                match rules::temperature(*answer, n) {
                    rules::Temperature::Hot => t!(s.lang, "quick.hot"),
                    rules::Temperature::Warm => t!(s.lang, "quick.warm"),
                    rules::Temperature::Cold => t!(s.lang, "quick.cold"),
                }
            };
            if over {
                s.note.push_str(&format!(" · {}", answer));
            }
        }
    }
    let text = serde_json::to_string(&s)?;
    if !cx
        .app
        .db
        .advance_solo(
            cx.author()?.id.get(),
            kind.key(),
            game.moves,
            &SoloStep {
                state: &text,
                score: game.score + i32::from(won),
                over,
            },
        )
        .await?
    {
        return cx.reply_ephemeral(t!(s.lang, "quick.expired")).await;
    }
    if over {
        cx.app.db.record_game(cx.author()?.id.get(), kind.key(), won).await?;
        if won {
            let _ = pay_win(&cx.app, cx.author()?.id.get(), kind.game(), 8, "quick_game").await?;
        }
    }
    let updated = SoloGame {
        state: text,
        moves: game.moves + 1,
        score: game.score + i32::from(won),
        status: if over { "over" } else { "playing" }.to_owned(),
        ..game
    };
    cx.update(view(&cx.app, kind, &updated).await?).await
}
