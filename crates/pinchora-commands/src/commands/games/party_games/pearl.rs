//! Hot Pearl: the pearl is with one player, a syllable on it, and they have a few seconds to type a word
//! that holds it. A word passes the pearl on with a new syllable; running out of time pops it, which costs
//! a life. The last player with lives wins.

use std::{str::Split, sync::Arc};

use pinchora_core::infra::i18n::Lang;
use pinchora_core::t;
use pinchora_db::{Commit, NewEntry, PartyEntry, PartyGame, PartyPlayer, Pay, PlayerChange};
use pinchora_game::party::Dice;
use pinchora_game::party::Party;
use pinchora_game::party::filter::is_profane_for;
use pinchora_game::party::hotpearl;
use pinchora_game::party::hotpearl::check_word;
use pinchora_game::party::hotpearl::dictionary_for;
use pinchora_game::party::hotpearl::earns_reward;
use pinchora_game::party::hotpearl::next_player;
use pinchora_game::party::hotpearl::turn_secs;
use pinchora_game::party::pearl_reward;
use pinchora_render::{PearlCard, PearlSeat};
use serde::{Deserialize, Serialize};
use twilight_model::{
    channel::message::{
        Component,
        component::{ButtonStyle, Label, TextInput, TextInputStyle},
    },
    http::interaction::InteractionResponseData,
};
use twilight_util::builder::embed::EmbedBuilder;

use crate::App;
use crate::Cx;
use crate::commands::games::games::channel_lang;
use crate::commands::games::party_games::after;
use crate::commands::games::party_games::card;
use crate::commands::games::party_games::count_games;
use crate::commands::games::party_games::day;
use crate::commands::games::party_games::edit_card;
use crate::commands::games::party_games::later;
use crate::commands::games::party_games::lobby;
use crate::commands::games::party_games::name_of;
use crate::commands::games::party_games::paid_note;
use crate::commands::games::party_games::push_button;
use crate::commands::games::party_games::remember;
use crate::commands::games::party_games::row;
use crate::commands::games::party_games::snapshot;
use crate::commands::games::party_games::standings_png;
use crate::commands::games::party_games::strict;
use crate::commands::games::party_games::text;
use crate::commands::games::party_games::theme;
use crate::infra::util::BRAND;
use crate::infra::util::SUCCESS;
use crate::infra::util::mention;
use crate::infra::util::relative;

/// Custom id of the word input in the form.
pub(crate) const WORD_FIELD: &str = "word";

/// A game's own state, kept as JSON in `party_games.state`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct State {
    /// The language of the game: its words and syllables.
    #[serde(default)]
    lang: Lang,
    /// Lives everyone started with.
    lives: i32,
    /// The syllable on the pearl, lowercase.
    syl: String,
    /// Who holds the pearl.
    cur: u64,
    turn: u32,
    seed: u64,
    /// What just happened, for the line above the board.
    #[serde(default)]
    moment: Moment,
    last: Option<String>,
}

/// What just happened. It is kept as what it was, not as a sentence, so it is worded in the language of the
/// table whenever the board is drawn.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum Moment {
    /// Nothing yet: the holder is simply named.
    #[default]
    Holds,
    First {
        name: String,
    },
    Played {
        name: String,
        word: String,
        next: String,
    },
    Boom {
        name: String,
        out: bool,
    },
}

impl Moment {
    fn say(&self, lang: Lang, holder: &str) -> String {
        match self {
            Self::Holds => t!(lang, "hotpearl.holds", name = holder),
            Self::First { name } => t!(lang, "hotpearl.first", name = name),
            Self::Played { name, word, next } => t!(
                lang,
                "hotpearl.played",
                name = name,
                word = lang.upper(word),
                next = next
            ),
            Self::Boom { name, out } => t!(
                lang,
                if *out { "hotpearl.boom-out" } else { "hotpearl.boom" },
                name = name
            ),
        }
    }
}

fn decode(game: &PartyGame) -> anyhow::Result<State> {
    Ok(serde_json::from_str(&game.state)?)
}

fn encode(state: &State) -> anyhow::Result<String> {
    Ok(serde_json::to_string(state)?)
}

/// The syllable for turn `turn`: fixed by the game's seed, and never the one before it.
fn syllable(lang: Lang, seed: u64, turn: u32, avoid: Option<&str>) -> String {
    let mut dice = Dice::new(seed ^ u64::from(turn).wrapping_mul(0x9e37_79b9_7f4a_7c15));
    dictionary_for(lang).syllable(turn, &mut dice, avoid).to_owned()
}

pub(crate) async fn open(cx: &Cx, lives: i32) -> anyhow::Result<()> {
    let lives = lives.clamp(*hotpearl::LIVES.start(), *hotpearl::LIVES.end());
    let state = encode(&State {
        // The table is public and keeps playing in the server's language, whoever opened it.
        lang: channel_lang(cx).await,
        lives,
        ..Default::default()
    })?;
    lobby::open(cx, Party::HotPearl, lives, state).await
}

/// Seats the players in a random order and gives the first one the pearl.
pub(crate) async fn start(cx: &Cx, game: &PartyGame, players: &[PartyPlayer]) -> anyhow::Result<lobby::Started> {
    let app = &cx.app;
    let mut state = decode(game)?;
    state.seed = rand::random();
    let mut order: Vec<&PartyPlayer> = players.iter().collect();
    Dice::new(state.seed).shuffle(&mut order);
    let first = order[0];
    state.cur = first.id();
    state.turn = 0;
    state.syl = syllable(state.lang, state.seed, 0, None);
    state.moment = Moment::First {
        name: first.name.clone(),
    };
    let seats: Vec<PlayerChange> = order
        .iter()
        .enumerate()
        .map(|(seat, p)| PlayerChange {
            user: p.id(),
            seat: Some(seat as i32),
            lives: Some(state.lives),
            ..Default::default()
        })
        .collect();
    let commit = Commit {
        phase: "turn",
        round: 0,
        state: &encode(&state)?,
        deadline: after(app, turn_secs(0)),
        status: "playing",
        players: &seats,
        entries: &[],
        pay: None,
    };
    let Some(done) = app.db.commit_party(game.id, game.seq, commit).await? else {
        return Ok(lobby::Started::Stale);
    };
    let players = app.db.party_players(game.id).await?;
    cx.update(view(app, &done.game, &players, &state, None).await?).await?;
    Ok(lobby::Started::Yes)
}

/// The table: the pearl and the players round it, and who has to type.
async fn view(
    app: &App,
    game: &PartyGame,
    players: &[PartyPlayer],
    state: &State,
    headline: Option<&str>,
) -> anyhow::Result<InteractionResponseData> {
    let lang = state.lang;
    let holder = players.iter().find(|p| p.id() == state.cur);
    let banner = state.moment.say(
        lang,
        &holder.map_or_else(|| t!(lang, "party.somebody"), |p| p.name.clone()),
    );
    let png = {
        // The card draws these in capitals, which Turkish spells with its own İ.
        let (syllable, banner, last) = (
            lang.upper(&state.syl),
            lang.upper(&banner),
            state.last.as_deref().map(|word| lang.upper(word)),
        );
        let seats: Vec<(String, i32, bool, bool)> = players
            .iter()
            .map(|p| {
                (
                    p.name.clone(),
                    p.lives,
                    p.eliminated,
                    p.id() == state.cur && !p.eliminated,
                )
            })
            .collect();
        let max_lives = state.lives;
        app.render(move |r| {
            let seats: Vec<PearlSeat> = seats
                .iter()
                .map(|(name, lives, out, current)| PearlSeat {
                    name,
                    lives: *lives,
                    max_lives,
                    out: *out,
                    current: *current,
                })
                .collect();
            r.pearl_card(&PearlCard {
                lang,
                syllable: &syllable,
                seats: &seats,
                banner: &banner,
                last_word: last.as_deref(),
                theme: theme(),
            })
        })
        .await?
    };
    let secs = turn_secs(state.turn);
    let embed = EmbedBuilder::new()
        .color(BRAND)
        .title(format!(
            "{} {}",
            Party::HotPearl.emoji(),
            Party::HotPearl.label_in(lang)
        ))
        .description(t!(
            lang,
            "hotpearl.turn",
            headline = headline.unwrap_or(&banner),
            holder = mention(state.cur),
            syllable = lang.upper(&state.syl),
            when = relative(game.deadline),
            secs = secs
        ));
    let button = push_button(
        Party::HotPearl,
        &[&"w", &game.id, &state.turn],
        &t!(lang, "hotpearl.btn-word"),
        ButtonStyle::Primary,
    );
    Ok(card(None, embed, "pearl.png", png, vec![row(vec![button])]))
}

/// The form's one field: `ui` is the language of the player who opens it, `lang` the table's (its syllable
/// and its example word).
#[allow(deprecated)] // the label belongs on the Label component, so the input's own stays empty
fn word_field(ui: Lang, lang: Lang, syllable: &str) -> Component {
    Component::Label(Label {
        component: Box::new(Component::TextInput(TextInput {
            custom_id: WORD_FIELD.to_owned(),
            id: None,
            label: None,
            max_length: Some(hotpearl::MAX_WORD as u16),
            min_length: Some(1),
            placeholder: Some(t!(lang, "hotpearl.placeholder")),
            required: Some(true),
            style: TextInputStyle::Short,
            value: None,
        })),
        description: None,
        id: None,
        label: t!(ui, "hotpearl.field-label", syllable = lang.upper(syllable)),
    })
}

/// What the table is in, after the checks every click and form shares: the game, its players, its
/// state, and the player holding the pearl; or the private answer to give instead.
enum Turn {
    Live(Box<(PartyGame, Vec<PartyPlayer>, State)>),
    Refused(String),
}

/// Whether `user` can play now on turn `turn` of game `id`. A pearl that ran out of time is popped, so a
/// click never plays on after the clock: after the answer for the button that opens the form (`opens_form`,
/// which has to be answered in time), and before it for the form itself, which can be deferred.
async fn check(cx: &Cx, id: i64, turn: u32, user: u64, opens_form: bool) -> anyhow::Result<Turn> {
    let lang = cx.lang();
    let Some((game, players)) = snapshot(&cx.app, Party::HotPearl, id).await? else {
        return Ok(Turn::Refused(t!(lang, "party.expired-game")));
    };
    let state = decode(&game)?;
    if !game.is_playing() || game.phase != "turn" || state.turn != turn {
        return Ok(Turn::Refused(t!(lang, "hotpearl.moved-on")));
    }
    if !players.iter().any(|p| p.id() == user) {
        return Ok(Turn::Refused(t!(lang, "party.not-playing")));
    }
    if user != state.cur {
        return Ok(Turn::Refused(t!(
            lang,
            "hotpearl.not-your-turn",
            name = name_of(lang, &players, state.cur)
        )));
    }
    if game.late(cx.app.now()) {
        if opens_form {
            later(&cx.app, id, move |app| async move { pop(&app, game).await });
        } else {
            pop(&cx.app, game).await?;
        }
        return Ok(Turn::Refused(t!(lang, "hotpearl.too-slow")));
    }
    Ok(Turn::Live(Box::new((game, players, state))))
}

fn ids(mut parts: Split<'_, char>) -> Option<(i64, u32)> {
    let id = parts.next()?.parse().ok()?;
    let turn = parts.next()?.parse().ok()?;
    Some((id, turn))
}

/// `hl:<action>:...`: the lobby's buttons, and `w:<id>:<turn>` to open the word form.
pub(crate) async fn on_button(cx: &Cx, mut parts: Split<'_, char>) -> anyhow::Result<()> {
    let action = parts.next().unwrap_or_default();
    if matches!(action, "j" | "l" | "s" | "x") {
        let Some(id) = parts.next().and_then(|v| v.parse().ok()) else {
            return cx.reply_ephemeral(t!(cx.lang(), "party.expired-button")).await;
        };
        lobby::on_button(cx, Party::HotPearl, action, id).await?;
        return Ok(());
    }
    let (Some((id, turn)), "w") = (ids(parts), action) else {
        return cx.reply_ephemeral(t!(cx.lang(), "party.expired-button")).await;
    };
    let me = cx.author()?.id.get();
    match check(cx, id, turn, me, true).await? {
        Turn::Refused(why) => cx.reply_ephemeral(why).await,
        Turn::Live(live) => {
            let (game, _, state) = *live;
            remember(cx, &game).await;
            cx.modal(
                format!("hl:m:{id}:{turn}"),
                &Party::HotPearl.label_in(cx.lang()),
                vec![word_field(cx.lang(), state.lang, &state.syl)],
            )
            .await
        }
    }
}

/// The word form was submitted: `hl:m:<id>:<turn>`.
pub(crate) async fn on_modal(cx: &Cx, mut parts: Split<'_, char>, fields: &[(String, String)]) -> anyhow::Result<()> {
    parts.next();
    let Some((id, turn)) = ids(parts) else {
        return cx.reply_ephemeral(t!(cx.lang(), "party.expired-form")).await;
    };
    let me = cx.author()?.id.get();
    let typed = fields
        .iter()
        .find(|(field, _)| field == WORD_FIELD)
        .map_or("", |(_, value)| value.as_str());
    let (game, players, state) = match check(cx, id, turn, me, false).await? {
        Turn::Refused(why) => return cx.reply_ephemeral(why).await,
        Turn::Live(live) => *live,
    };
    let app = &cx.app;
    let used: Vec<String> = app
        .db
        .party_all_entries(id)
        .await?
        .into_iter()
        .map(|e| e.text)
        .collect();
    let word = match check_word(dictionary_for(state.lang), &state.syl, typed, &used) {
        Ok(word) => word,
        Err(refusal) => return cx.reply_ephemeral(refusal.say_in(cx.lang(), &state.syl)).await,
    };
    // The list holds mild words ("damn" is out, "hell" is not); a strict server turns those away too.
    if is_profane_for(&word, true, state.lang) && strict(app, game.guild_id as u64).await {
        return cx.reply_ephemeral(text::Refused::Profane.say(cx.lang())).await;
    }
    let order: Vec<(u64, bool)> = players.iter().map(|p| (p.id(), !p.eliminated)).collect();
    let Some(next) = next_player(&order, state.cur) else {
        return cx.reply_ephemeral(t!(cx.lang(), "hotpearl.nobody-left")).await;
    };
    let mut after_state = state.clone();
    after_state.cur = next;
    after_state.turn = state.turn + 1;
    after_state.syl = syllable(state.lang, state.seed, after_state.turn, Some(&state.syl));
    after_state.moment = Moment::Played {
        name: name_of(state.lang, &players, me),
        word: word.clone(),
        next: name_of(state.lang, &players, next),
    };
    after_state.last = Some(word.clone());
    let commit = Commit {
        phase: "turn",
        round: after_state.turn as i32,
        state: &encode(&after_state)?,
        deadline: after(app, turn_secs(after_state.turn)),
        status: "playing",
        players: &[],
        entries: &[NewEntry {
            round: state.turn as i32,
            user: me,
            text: &word,
        }],
        pay: None,
    };
    let Some(done) = app.db.commit_party(id, game.seq, commit).await? else {
        return cx.reply_ephemeral(t!(cx.lang(), "hotpearl.moved-before")).await;
    };
    cx.update(view(app, &done.game, &players, &after_state, None).await?)
        .await
}

/// Pops the pearl: the holder loses a life, and the game goes on with a fresh syllable or, with one player
/// left, ends. False when somebody else moved the game first.
pub(crate) async fn pop(app: &Arc<App>, game: PartyGame) -> anyhow::Result<bool> {
    let players = app.db.party_players(game.id).await?;
    let state = decode(&game)?;
    let lang = state.lang;
    let Some(victim) = players.iter().find(|p| p.id() == state.cur) else {
        return Ok(false);
    };
    let lives = (victim.lives - 1).max(0);
    let out = lives == 0;
    let name = victim.name.clone();
    let change = PlayerChange {
        user: victim.id(),
        lives: Some(lives),
        eliminated: out.then_some(true),
        // Players who last longer rank higher: the turn they went out on is their score.
        score: if out { state.turn as i32 + 1 } else { 0 },
        ..Default::default()
    };
    let order: Vec<(u64, bool)> = players
        .iter()
        .map(|p| (p.id(), !p.eliminated && !(out && p.id() == victim.id())))
        .collect();
    let alive: Vec<u64> = order
        .iter()
        .filter(|(_, alive)| *alive)
        .map(|(user, _)| *user)
        .collect();
    let verdict = if out {
        t!(lang, "hotpearl.pop-out", name = name)
    } else {
        t!(lang, "hotpearl.pop", name = name, count = lives)
    };

    if alive.len() <= 1 {
        let winner = alive.first().copied();
        // Winning by waiting pays nothing: the table has to have played words, and so does the winner.
        let authors: Vec<u64> = app
            .db
            .party_all_entries(game.id)
            .await?
            .iter()
            .map(PartyEntry::author)
            .collect();
        let rewarded = winner.is_some_and(|w| earns_reward(&authors, w));
        let shells: Vec<(u64, i64)> = winner
            .filter(|_| rewarded)
            .map(|w| (w, pearl_reward(players.len())))
            .into_iter()
            .collect();
        let commit = Commit {
            phase: "done",
            round: game.round,
            state: &game.state,
            deadline: game.deadline,
            status: "done",
            players: &[change],
            entries: &[],
            pay: Some(Pay {
                game: Party::HotPearl.game().key(),
                day: day(app),
                cap: Party::HotPearl.game().daily_cap() as i32,
                reason: "hotpearl_win",
                shells: &shells,
            }),
        };
        let Some(done) = app.db.commit_party(game.id, game.seq, commit).await? else {
            return Ok(false);
        };
        let players = app.db.party_players(game.id).await?;
        count_games(app, Party::HotPearl, &players, winner.as_slice()).await;
        let mut ranked: Vec<&PartyPlayer> = players.iter().collect();
        ranked.sort_by_key(|p| (Some(p.id()) != winner, std::cmp::Reverse(p.score), p.seat));
        let rows = ranked
            .iter()
            .map(|p| {
                let won = Some(p.id()) == winner;
                (
                    p.name.clone(),
                    if won {
                        t!(lang, "hotpearl.last-standing")
                    } else {
                        t!(lang, "hotpearl.out-on-turn", n = p.score)
                    },
                    if !won {
                        String::new()
                    } else if rewarded {
                        paid_note(lang, &done.paid, p.id())
                    } else {
                        t!(lang, "hotpearl.no-shells")
                    },
                    won,
                )
            })
            .collect();
        let label = Party::HotPearl.label_in(lang);
        let winner_name = winner.map_or_else(|| t!(lang, "party.nobody"), |w| name_of(lang, &players, w));
        let subtitle = t!(lang, "hotpearl.wins-after", name = winner_name, turns = state.turn + 1);
        let png = standings_png(app, lang, &label, &subtitle, rows, &t!(lang, "party.thanks")).await?;
        let unpaid = if rewarded || winner.is_none() {
            String::new()
        } else {
            format!(
                "\n{}",
                t!(
                    lang,
                    "hotpearl.unpaid",
                    min_words = hotpearl::MIN_WORDS,
                    min_winner = hotpearl::MIN_WINNER_WORDS
                )
            )
        };
        let embed = EmbedBuilder::new()
            .color(SUCCESS)
            .title(t!(
                lang,
                "party.over-title",
                emoji = Party::HotPearl.emoji(),
                label = label
            ))
            .description(t!(
                lang,
                "hotpearl.over",
                verdict = verdict,
                winner = winner.map_or_else(|| t!(lang, "party.nobody"), mention),
                unpaid = unpaid
            ));
        edit_card(app, &done.game, &card(None, embed, "standings.png", png, Vec::new())).await;
        return Ok(true);
    }

    let Some(next_holder) = next_player(&order, victim.id()) else {
        return Ok(false);
    };
    let mut next = state.clone();
    next.cur = next_holder;
    next.turn = state.turn + 1;
    next.syl = syllable(state.lang, state.seed, next.turn, Some(&state.syl));
    next.moment = Moment::Boom { name, out };
    let commit = Commit {
        phase: "turn",
        round: next.turn as i32,
        state: &encode(&next)?,
        deadline: after(app, turn_secs(next.turn)),
        status: "playing",
        players: &[change],
        entries: &[],
        pay: None,
    };
    let Some(done) = app.db.commit_party(game.id, game.seq, commit).await? else {
        return Ok(false);
    };
    let players = app.db.party_players(game.id).await?;
    // The verdict goes above the new turn.
    let data = view(app, &done.game, &players, &next, Some(&verdict)).await?;
    edit_card(app, &done.game, &data).await;
    Ok(true)
}

/// The clock: a pearl whose time ran out pops.
pub(crate) async fn on_due(app: &Arc<App>, game: PartyGame) -> anyhow::Result<bool> {
    if game.phase == "turn" && game.late(app.now()) {
        return pop(app, game).await;
    }
    Ok(false)
}
