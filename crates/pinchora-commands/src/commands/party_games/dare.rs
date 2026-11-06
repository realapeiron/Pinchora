//! Truth or Dare: players take turns. The bot picks who is up, that player chooses truth or dare, gets
//! one from the family friendly pool and says when they have done it or passed. Nobody has to answer
//! anything: a pass is always fine. A table that goes quiet ends the game by itself.

use std::{str::Split, sync::Arc};

use pinchora_core::{i18n::Lang, t};
use pinchora_db::{Commit, PartyGame, PartyPlayer, Pay, PlayerChange};
use pinchora_game::{
    lang::upper,
    party::{
        Dice, Party, TURN_REWARD,
        prompts::{Category, PartyContent, Pick},
    },
};
use pinchora_render::{PromptCard, PromptTone};
use serde::{Deserialize, Serialize};
use twilight_model::{channel::message::component::ButtonStyle, http::interaction::InteractionResponseData};
use twilight_util::builder::embed::EmbedBuilder;

use super::{
    after, card, count_games, day, edit_card, game_lang, lobby, name_of, paid_text, push_button, remember, row,
    snapshot, standings_png, theme,
};
use crate::{
    App, Cx,
    util::{BRAND, SUCCESS, mention, relative},
};

/// Seconds a player has to pick truth or dare, and to do what they got.
const PICK_SECS: i64 = 90;
const DO_SECS: i64 = 180;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct State {
    /// The language of the game: its truths and dares.
    #[serde(default)]
    lang: Lang,
    seed: u64,
    /// The category the host asked for, if any.
    topic: Option<String>,
    /// Everyone's turn, in order.
    order: Vec<u64>,
    /// Who is up.
    cur: u64,
    turn: u32,
    /// `truth` or `dare` once they picked.
    pick: Option<String>,
    /// The id of what they got.
    prompt: String,
    /// What was already asked, so it does not come round again soon.
    seen: Vec<String>,
    /// Turns in a row that nobody answered.
    quiet: u32,
    /// What happened last turn, shown above this one.
    #[serde(default)]
    news: News,
}

/// What happened last turn. It is kept as what it was, not as a sentence, so it is worded in the language of
/// the table whenever the turn is drawn.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum News {
    #[default]
    Nothing,
    /// `paid` is the shells the turn paid, once known (0 when the day's limit was reached); it is only ever
    /// shown on the message that announces the turn, never kept.
    Did {
        name: String,
        #[serde(skip)]
        paid: Option<i64>,
    },
    Passed {
        name: String,
    },
    Quiet {
        name: String,
    },
}

impl News {
    fn say(&self, lang: Lang) -> String {
        match self {
            Self::Nothing => String::new(),
            Self::Did { name, paid } => {
                let line = t!(lang, "dare.did", name = name);
                match paid {
                    Some(shells) => format!("{line} ({})", paid_text(lang, *shells)),
                    None => line,
                }
            }
            Self::Passed { name } => t!(lang, "dare.passed", name = name),
            Self::Quiet { name } => t!(lang, "dare.quiet", name = name),
        }
    }
}

/// Why a game ended.
#[derive(Debug, Clone, Copy)]
enum Ended {
    Host,
    Quiet,
}

fn decode(game: &PartyGame) -> anyhow::Result<State> {
    Ok(serde_json::from_str(&game.state)?)
}

fn encode(state: &State) -> anyhow::Result<String> {
    Ok(serde_json::to_string(state)?)
}

fn category(state: &State) -> Option<Category> {
    state.topic.as_deref().and_then(Category::from_key)
}

pub(crate) async fn open(cx: &Cx, topic: Option<Category>) -> anyhow::Result<()> {
    let state = State {
        lang: cx.lang(),
        topic: topic.map(|c| c.key().to_owned()),
        ..Default::default()
    };
    lobby::open(cx, Party::TruthOrDare, 1, encode(&state)?).await
}

/// Puts the players in a random order and gives the first one the turn.
pub(crate) async fn start(cx: &Cx, game: &PartyGame, players: &[PartyPlayer]) -> anyhow::Result<lobby::Started> {
    let app = &cx.app;
    let mut state = decode(game)?;
    state.seed = rand::random();
    state.order = players.iter().map(PartyPlayer::id).collect();
    Dice::new(state.seed).shuffle(&mut state.order);
    state.cur = state.order[0];
    let commit = Commit {
        phase: "pick",
        round: 0,
        state: &encode(&state)?,
        deadline: after(app, PICK_SECS),
        status: "playing",
        players: &[],
        entries: &[],
        pay: None,
    };
    let Some(done) = app.db.commit_party(game.id, game.seq, commit).await? else {
        return Ok(lobby::Started::Stale);
    };
    cx.update(view(app, &done.game, players, &state).await?).await?;
    Ok(lobby::Started::Yes)
}

/// The turn: whose it is, and either the question or what they picked.
async fn view(
    app: &App,
    game: &PartyGame,
    players: &[PartyPlayer],
    state: &State,
) -> anyhow::Result<InteractionResponseData> {
    let lang = state.lang;
    let name = name_of(lang, players, state.cur);
    let prompt = PartyContent::find_prompt(&state.prompt).filter(|_| game.phase == "prompt");
    let (tone, text, topic, footer) = match (&prompt, state.pick.as_deref().and_then(Pick::from_key)) {
        (Some(prompt), Some(Pick::Truth)) => (
            PromptTone::Truth,
            prompt.text.clone(),
            prompt.category.label_in(lang),
            t!(lang, "dare.footer-truth"),
        ),
        (Some(prompt), _) => (
            PromptTone::Dare,
            prompt.text.clone(),
            prompt.category.label_in(lang),
            t!(lang, "dare.footer-dare"),
        ),
        (None, _) => (
            PromptTone::Turn,
            t!(lang, "dare.question"),
            category(state).map_or_else(String::new, |c| c.label_in(lang)),
            t!(lang, "dare.footer-turn"),
        ),
    };
    let png = {
        // The card draws the category and footer in capitals, which Turkish spells with its own İ.
        let (name, topic, footer) = (name.clone(), upper(lang, &topic), upper(lang, &footer));
        app.render(move |r| {
            r.prompt_card(&PromptCard {
                lang,
                tone,
                name: &name,
                text: &text,
                category: &topic,
                footer: &footer,
                theme: theme(),
            })
        })
        .await?
    };
    let id = game.id;
    let turn = state.turn;
    let ask = match tone {
        PromptTone::Turn => t!(
            lang,
            "dare.ask-pick",
            holder = mention(state.cur),
            when = relative(game.deadline)
        ),
        PromptTone::Truth => t!(
            lang,
            "dare.ask-truth",
            holder = mention(state.cur),
            when = relative(game.deadline)
        ),
        PromptTone::Dare => t!(
            lang,
            "dare.ask-dare",
            holder = mention(state.cur),
            when = relative(game.deadline)
        ),
    };
    let news = state.news.say(lang);
    let description = if news.is_empty() { ask } else { format!("{news}\n{ask}") };
    let embed = EmbedBuilder::new()
        .color(BRAND)
        .title(format!(
            "{} {}",
            Party::TruthOrDare.emoji(),
            Party::TruthOrDare.label_in(lang)
        ))
        .description(description);
    let end = push_button(
        Party::TruthOrDare,
        &[&"e", &id],
        &t!(lang, "dare.btn-end"),
        ButtonStyle::Secondary,
    );
    let buttons = if game.phase == "prompt" {
        vec![
            push_button(
                Party::TruthOrDare,
                &[&"ok", &id, &turn],
                &t!(lang, "dare.btn-done"),
                ButtonStyle::Success,
            ),
            push_button(
                Party::TruthOrDare,
                &[&"no", &id, &turn],
                &t!(lang, "dare.btn-pass"),
                ButtonStyle::Secondary,
            ),
            end,
        ]
    } else {
        vec![
            push_button(
                Party::TruthOrDare,
                &[&"t", &id, &turn],
                &t!(lang, "dare.btn-truth"),
                ButtonStyle::Success,
            ),
            push_button(
                Party::TruthOrDare,
                &[&"d", &id, &turn],
                &t!(lang, "dare.btn-dare"),
                ButtonStyle::Danger,
            ),
            end,
        ]
    };
    Ok(card(None, embed, "turn.png", png, vec![row(buttons)]))
}

/// `td:<action>:<id>[:<turn>]`: the lobby's buttons, `t` and `d` to pick, `ok` and `no` to finish a turn
/// and `e` to end the game.
pub(crate) async fn on_button(cx: &Cx, mut parts: Split<'_, char>) -> anyhow::Result<()> {
    let action = parts.next().unwrap_or_default();
    let lang = cx.lang();
    let Some(id) = parts.next().and_then(|v| v.parse::<i64>().ok()) else {
        return cx.reply_ephemeral(t!(lang, "party.expired-button")).await;
    };
    if lobby::on_button(cx, Party::TruthOrDare, action, id).await? {
        return Ok(());
    }
    let app = &cx.app;
    let me = cx.author()?.id.get();
    let Some((game, players)) = snapshot(app, Party::TruthOrDare, id).await? else {
        return cx.reply_ephemeral(t!(lang, "party.expired-game")).await;
    };
    remember(cx, &game).await;
    if !game.is_playing() {
        return cx.reply_ephemeral(t!(lang, "dare.over-click")).await;
    }
    if action == "e" {
        if me != game.host() && !cx.can_manage_guild() {
            return cx
                .reply_ephemeral(t!(lang, "dare.only-host-ends", host = mention(game.host())))
                .await;
        }
        return match finish(app, &game, &players, Ended::Host).await? {
            Some((_, data)) => cx.update(data).await,
            None => cx.reply_ephemeral(t!(lang, "dare.changed")).await,
        };
    }
    let state = decode(&game)?;
    let (Some(turn), true) = (
        parts.next().and_then(|v| v.parse::<u32>().ok()),
        matches!(action, "t" | "d" | "ok" | "no"),
    ) else {
        return cx.reply_ephemeral(t!(lang, "party.expired-button")).await;
    };
    let phase = if matches!(action, "t" | "d") { "pick" } else { "prompt" };
    if game.phase != phase || state.turn != turn {
        return cx.reply_ephemeral(t!(lang, "dare.turn-over")).await;
    }
    if me != state.cur {
        return cx
            .reply_ephemeral(t!(
                lang,
                "dare.not-your-turn",
                name = name_of(lang, &players, state.cur)
            ))
            .await;
    }
    if game.late(app.now()) {
        on_due(app, game).await?;
        return cx.reply_ephemeral(t!(lang, "dare.late")).await;
    }
    match action {
        "t" => choose(cx, game, players, state, Pick::Truth).await,
        "d" => choose(cx, game, players, state, Pick::Dare).await,
        _ => resolve(cx, game, players, state, action == "ok").await,
    }
}

/// The player picked truth or dare: they get one nobody had lately.
async fn choose(
    cx: &Cx,
    game: PartyGame,
    players: Vec<PartyPlayer>,
    mut state: State,
    pick: Pick,
) -> anyhow::Result<()> {
    let app = &cx.app;
    let pool = PartyContent::for_lang(state.lang).prompts(pick, category(&state));
    anyhow::ensure!(!pool.is_empty(), "no prompts to pick from");
    let mut fresh: Vec<&_> = pool.iter().filter(|p| !state.seen.contains(&p.id)).copied().collect();
    if fresh.is_empty() {
        state.seen.clear();
        fresh = pool;
    }
    let mut dice = Dice::new(state.seed ^ u64::from(state.turn).wrapping_mul(0x9e37_79b9_7f4a_7c15));
    let chosen = fresh[dice.below(fresh.len())];
    state.pick = Some(pick.key().to_owned());
    state.prompt = chosen.id.clone();
    state.seen.push(chosen.id.clone());
    let commit = Commit {
        phase: "prompt",
        round: game.round,
        state: &encode(&state)?,
        deadline: after(app, DO_SECS),
        status: "playing",
        players: &[],
        entries: &[],
        pay: None,
    };
    let Some(done) = app.db.commit_party(game.id, game.seq, commit).await? else {
        return cx.reply_ephemeral(t!(cx.lang(), "dare.changed")).await;
    };
    cx.update(view(app, &done.game, &players, &state).await?).await
}

/// The player did it, or passed: the next player is up.
async fn resolve(
    cx: &Cx,
    game: PartyGame,
    players: Vec<PartyPlayer>,
    mut state: State,
    did: bool,
) -> anyhow::Result<()> {
    let app = &cx.app;
    let who = state.cur;
    let name = name_of(state.lang, &players, who);
    let shells = if did { vec![(who, TURN_REWARD)] } else { Vec::new() };
    let change = PlayerChange {
        user: who,
        score: i32::from(did),
        ..Default::default()
    };
    next_turn(&mut state, &players);
    state.quiet = 0;
    state.news = if did {
        News::Did { name, paid: None }
    } else {
        News::Passed { name }
    };
    let commit = Commit {
        phase: "pick",
        round: state.turn as i32,
        state: &encode(&state)?,
        deadline: after(app, PICK_SECS),
        status: "playing",
        players: &[change],
        entries: &[],
        pay: Some(Pay {
            game: Party::TruthOrDare.game().key(),
            day: day(app),
            cap: Party::TruthOrDare.game().daily_cap() as i32,
            reason: "truthordare_done",
            shells: &shells,
        }),
    };
    let Some(done) = app.db.commit_party(game.id, game.seq, commit).await? else {
        return cx.reply_ephemeral(t!(cx.lang(), "dare.changed")).await;
    };
    if let News::Did { paid, .. } = &mut state.news {
        *paid = done
            .paid
            .iter()
            .find(|(user, _)| *user == who)
            .map(|(_, shells)| *shells);
    }
    cx.update(view(app, &done.game, &players, &state).await?).await
}

/// Moves `state` on to the next player in the order, with nothing picked yet.
fn next_turn(state: &mut State, players: &[PartyPlayer]) {
    let still: Vec<u64> = state
        .order
        .iter()
        .copied()
        .filter(|u| players.iter().any(|p| p.id() == *u))
        .collect();
    let at = still.iter().position(|u| *u == state.cur).unwrap_or(0);
    state.cur = still[(at + 1) % still.len()];
    state.turn += 1;
    state.pick = None;
    state.prompt.clear();
}

/// Ends the game: the standings are the number of turns each player did. Returns the finished game and
/// what to show, or `None` when somebody moved first.
async fn finish(
    app: &Arc<App>,
    game: &PartyGame,
    players: &[PartyPlayer],
    ended: Ended,
) -> anyhow::Result<Option<(PartyGame, InteractionResponseData)>> {
    let lang = game_lang(game);
    let why = t!(
        lang,
        match ended {
            Ended::Host => "dare.why-host",
            Ended::Quiet => "dare.why-quiet",
        }
    );
    let commit = Commit {
        phase: "done",
        round: game.round,
        state: &game.state,
        deadline: game.deadline,
        status: "done",
        players: &[],
        entries: &[],
        pay: None,
    };
    let Some(done) = app.db.commit_party(game.id, game.seq, commit).await? else {
        return Ok(None);
    };
    let mut ranked: Vec<&PartyPlayer> = players.iter().collect();
    ranked.sort_by_key(|p| (std::cmp::Reverse(p.score), p.seat));
    let top = ranked.first().map_or(0, |p| p.score);
    // One game for each player however many turns they had; the most turns done wins it, a tie shares it.
    let winners: Vec<u64> = ranked
        .iter()
        .filter(|p| top > 0 && p.score == top)
        .map(|p| p.id())
        .collect();
    count_games(app, Party::TruthOrDare, players, &winners).await;
    let rows = ranked
        .iter()
        .map(|p| {
            (
                p.name.clone(),
                t!(lang, "dare.done-count", count = p.score),
                String::new(),
                top > 0 && p.score == top,
            )
        })
        .collect();
    let label = Party::TruthOrDare.label_in(lang);
    let png = standings_png(app, lang, &label, &why, rows, &t!(lang, "party.thanks")).await?;
    let embed = EmbedBuilder::new()
        .color(SUCCESS)
        .title(t!(
            lang,
            "party.over-title",
            emoji = Party::TruthOrDare.emoji(),
            label = label
        ))
        .description(t!(lang, "dare.over", why = why));
    Ok(Some((done.game, card(None, embed, "standings.png", png, Vec::new()))))
}

/// The clock: a turn nobody answered passes to the next player, and a second quiet turn in a row ends the game.
pub(crate) async fn on_due(app: &Arc<App>, game: PartyGame) -> anyhow::Result<bool> {
    if !game.late(app.now()) || !matches!(game.phase.as_str(), "pick" | "prompt") {
        return Ok(false);
    }
    let players = app.db.party_players(game.id).await?;
    let mut state = decode(&game)?;
    if state.quiet + 1 >= 2 {
        let Some((done, data)) = finish(app, &game, &players, Ended::Quiet).await? else {
            return Ok(false);
        };
        edit_card(app, &done, &data).await;
        return Ok(true);
    }
    let name = name_of(state.lang, &players, state.cur);
    next_turn(&mut state, &players);
    state.quiet += 1;
    state.news = News::Quiet { name };
    let commit = Commit {
        phase: "pick",
        round: state.turn as i32,
        state: &encode(&state)?,
        deadline: after(app, PICK_SECS),
        status: "playing",
        players: &[],
        entries: &[],
        pay: None,
    };
    let Some(done) = app.db.commit_party(game.id, game.seq, commit).await? else {
        return Ok(false);
    };
    edit_card(app, &done.game, &view(app, &done.game, &players, &state).await?).await;
    Ok(true)
}
