//! Reef party adapters: shared public boards, seat ownership, durable CAS, clock and capped rewards.
mod view;
use view::view;

use crate::commands::games::games;
use crate::commands::games::party_games;
use crate::{App, Cx};
use chrono::Duration;
use pinchora_core::infra::i18n::Lang;
use pinchora_core::t;
use pinchora_db::{GameSession, NewGame, ReefMove};
use pinchora_game::games::reef_party::Board;
use pinchora_game::games::reef_party::Kind;
use pinchora_game::games::reef_party::State;
use pinchora_game::games::reef_party::word_mode;
use pinchora_game::play::PlayContent;
use pinchora_game::play::chess;
use pinchora_render::ChessCard;
use pinchora_render::Theme;
use pinchora_render::games::reef_party::ReefCard;
use std::{str::Split, sync::Arc};
use twilight_interactions::command::{CommandModel, CommandOption, CreateCommand, CreateOption};
use twilight_model::{
    channel::message::{
        Component,
        component::{ButtonStyle, Label, TextInput, TextInputStyle},
    },
    http::interaction::InteractionResponseData,
    id::Id,
};
use twilight_util::builder::embed::EmbedBuilder;

#[derive(CommandOption, CreateOption, Clone, Copy)]
pub enum QuizMode {
    #[option(name = "Teams", value = "team")]
    Team,
    #[option(name = "Quickfire", value = "quick")]
    Quick,
}
#[derive(CommandOption, CreateOption, Clone, Copy)]
pub enum Category {
    #[option(name = "All", value = "all")]
    All,
    #[option(name = "Ocean", value = "ocean")]
    Ocean,
    #[option(name = "Science", value = "science")]
    Science,
    #[option(name = "Nature", value = "nature")]
    Nature,
}
#[derive(CommandOption, CreateOption, Clone, Copy)]
pub enum WordMode {
    #[option(name = "Relay", value = "relay")]
    Relay,
    #[option(name = "Chain", value = "chain")]
    Chain,
    #[option(name = "Longest", value = "longest")]
    Longest,
    #[option(name = "Many words", value = "shoal")]
    Shoal,
    #[option(name = "Mixed", value = "mixed")]
    Mixed,
}
#[derive(CommandOption, CreateOption, Clone, Copy)]
pub enum Level {
    #[option(name = "Easy", value = "easy")]
    Easy,
    #[option(name = "Medium", value = "medium")]
    Medium,
    #[option(name = "Hard", value = "hard")]
    Hard,
}
#[derive(CommandModel, CreateCommand)]
#[command(name = "reefquiz", desc = "Reef Signals: five rounds of team or quickfire trivia")]
pub struct Quiz {
    /// Teams or a quickfire race
    mode: Option<QuizMode>,
    /// Question category
    category: Option<Category>,
}
#[derive(CommandModel, CreateCommand)]
#[command(
    name = "wordwake",
    desc = "Wordwake: timed letter fragments, longest words and word chains"
)]
pub struct Words {
    /// Word round rules
    mode: Option<WordMode>,
}
#[derive(CommandModel, CreateCommand)]
#[command(name = "driftstory", desc = "Drift Tale: build a sea story one word at a time")]
pub struct Story;
#[derive(CommandModel, CreateCommand)]
#[command(name = "roomwake", desc = "Cabin Glimpse: identify rooms from Pinchora house cards")]
pub struct Room;
#[derive(CommandModel, CreateCommand)]
#[command(
    name = "crittercompass",
    desc = "Critter Compass: think of a reef creature and answer its clues"
)]
pub struct Critter;
#[derive(CommandModel, CreateCommand)]
#[command(
    name = "lastreef",
    desc = "Last Reef: survive the tides, form alliances and reach the winners card"
)]
pub struct Survival;
#[derive(CommandModel, CreateCommand)]
#[command(name = "reefmind", desc = "Reefmind: play chess against a local sea captain")]
pub struct Chess {
    /// Captain difficulty
    level: Option<Level>,
}
#[derive(CommandModel, CreateCommand)]
#[command(name = "kelpmaze", desc = "Kelp Trail: navigate the pixel maze to the golden reef")]
pub struct Maze;
#[derive(CommandModel, CreateCommand)]
#[command(
    name = "gemtide",
    desc = "Gem Tide: swap adjacent shells to match three and score 30"
)]
pub struct Match;
impl Quiz {
    pub async fn run(self, cx: &Cx) -> anyhow::Result<()> {
        open(
            cx,
            Kind::Quiz,
            if matches!(self.mode, Some(QuizMode::Quick)) {
                "quick"
            } else {
                "team"
            },
            match self.category {
                Some(Category::Ocean) => "ocean",
                Some(Category::Science) => "science",
                Some(Category::Nature) => "nature",
                _ => "all",
            },
        )
        .await
    }
}
impl Words {
    pub async fn run(self, cx: &Cx) -> anyhow::Result<()> {
        open(
            cx,
            Kind::Words,
            match self.mode {
                Some(WordMode::Chain) => "chain",
                Some(WordMode::Longest) => "longest",
                Some(WordMode::Shoal) => "shoal",
                Some(WordMode::Mixed) => "mixed",
                _ => "relay",
            },
            "all",
        )
        .await
    }
}
impl Chess {
    pub async fn run(self, cx: &Cx) -> anyhow::Result<()> {
        open(
            cx,
            Kind::Chess,
            match self.level {
                Some(Level::Easy) => "easy",
                Some(Level::Hard) => "hard",
                _ => "medium",
            },
            "all",
        )
        .await
    }
}
fn id(game: &GameSession, s: &State, action: &str) -> String {
    format!("{}:{action}:{}:{}", s.kind.prefix(), game.id, s.seq)
}
#[allow(deprecated)]
fn field(lang: Lang) -> Component {
    Component::Label(Label {
        component: Box::new(Component::TextInput(TextInput {
            custom_id: "answer".into(),
            id: None,
            label: None,
            max_length: Some(40),
            min_length: Some(1),
            placeholder: None,
            required: Some(true),
            style: TextInputStyle::Short,
            value: None,
        })),
        description: None,
        id: None,
        label: t!(lang, "party-v2.answer"),
    })
}
pub(crate) async fn open(cx: &Cx, kind: Kind, mode: &str, category: &str) -> anyhow::Result<()> {
    let Some(guild) = cx.guild_id() else {
        return cx.reply_ephemeral(t!(cx.lang(), "party-v2.denied")).await;
    };
    let lang = games::channel_lang(cx).await;
    let state = State::new(kind, lang, mode, category, rand::random());
    let text = serde_json::to_string(&state)?;
    let name = cx.author_display_name();
    let new = NewGame {
        kind: &format!("d9:{}", kind.key()),
        guild_id: guild,
        channel_id: games::channel_of(cx),
        host: cx.author()?.id.get(),
        guest: None,
        bet: 0,
        status: if state.lobby { "pending" } else { "active" },
        state: &text,
        ttl_secs: state.seconds(),
    };
    let Some(game) = cx.app.db.reef_open(new, &name, cx.app.now()).await? else {
        return cx.reply_ephemeral(t!(cx.lang(), "party-v2.busy")).await;
    };
    let result = cx.reply(view(&cx.app, &game, &state).await?).await;
    games::track_message(cx, game.id);
    result
}
async fn save(
    app: &App,
    game: &GameSession,
    s: &State,
    join: Option<(u64, &str, i16)>,
    timeout: bool,
) -> anyhow::Result<Option<GameSession>> {
    let reward = s.reward_seats();
    let results: Vec<_> = if s.done && !s.cancelled {
        s.seats
            .iter()
            .enumerate()
            .map(|(i, p)| (i as i16, p.score as i32, s.winners.contains(&i), reward.contains(&i)))
            .collect()
    } else {
        vec![]
    };
    let old: State = serde_json::from_str(&game.state)?;
    // Answers, selections and alliances never buy extra time for the round.
    let advance = old.lobby != s.lobby
        || old.round != s.round
        || old.turn != s.turn
            && matches!(s.kind, Kind::Words | Kind::Story)
            && !(s.kind == Kind::Words && word_mode(&s.mode, s.round) == "longest")
        || matches!(s.kind, Kind::Chess | Kind::Maze) && s.seats[0].actions > old.seats[0].actions
        || matches!((&old.board, &s.board), (Board::Match(before), Board::Match(after)) if after.moves > before.moves);
    let deadline = if advance || timeout {
        app.now() + Duration::seconds(s.seconds())
    } else {
        game.expires_at
    };
    app.db
        .reef_move(
            game,
            ReefMove {
                state: &serde_json::to_string(s)?,
                status: if s.done {
                    "done"
                } else if s.lobby {
                    "pending"
                } else {
                    "active"
                },
                deadline,
                now: app.now(),
                timeout,
                join,
                results: &results,
            },
        )
        .await
}
pub(crate) async fn on_component(cx: &Cx, prefix: &str, mut parts: Split<'_, char>) -> anyhow::Result<()> {
    let action = parts.next().unwrap_or("");
    let game = parts.next().and_then(|x| x.parse::<i64>().ok());
    let seq = parts.next().and_then(|x| x.parse::<u32>().ok());
    interact(cx, prefix, game, seq, action, None).await
}
pub(crate) async fn on_modal(
    cx: &Cx,
    prefix: &str,
    mut parts: Split<'_, char>,
    fields: &[(String, String)],
) -> anyhow::Result<()> {
    let action = parts.next().unwrap_or("");
    let game = parts.next().and_then(|x| x.parse::<i64>().ok());
    let seq = parts.next().and_then(|x| x.parse::<u32>().ok());
    if action != "submit" {
        return cx.reply_ephemeral(t!(cx.lang(), "party-v2.expired")).await;
    }
    interact(
        cx,
        prefix,
        game,
        seq,
        "submit",
        Some(
            fields
                .iter()
                .find(|(k, _)| k == "answer")
                .map_or("", |(_, v)| v.as_str()),
        ),
    )
    .await
}
async fn interact(
    cx: &Cx,
    prefix: &str,
    game: Option<i64>,
    seq: Option<u32>,
    action: &str,
    input: Option<&str>,
) -> anyhow::Result<()> {
    let Some(game) = game else {
        return cx.reply_ephemeral(t!(cx.lang(), "party-v2.expired")).await;
    };
    let Some(game) = cx.app.db.game(game).await?.filter(|g| {
        g.kind.starts_with("d9:")
            && Some(g.guild_id as u64) == cx.guild_id()
            && g.channel_id as u64 == games::channel_of(cx)
    }) else {
        return cx.reply_ephemeral(t!(cx.lang(), "party-v2.expired")).await;
    };
    let mut s: State = serde_json::from_str(&game.state)?;
    if prefix != s.kind.prefix() || seq != Some(s.seq) || s.done || game.status == "done" {
        return cx.reply_ephemeral(t!(cx.lang(), "party-v2.expired")).await;
    }
    if game.expires_at <= cx.app.now() {
        cx.reply_ephemeral(t!(cx.lang(), "party-v2.expired")).await?;
        expire(&cx.app, &game).await?;
        return Ok(());
    }
    let user = cx.author()?.id.get();
    let members = cx.app.db.party_members(game.id).await?;
    let seat = members
        .iter()
        .find(|p| p.user_id as u64 == user)
        .map(|p| p.choice as usize);
    if let Some((_, message)) = cx.clicked_message() {
        if game.message_id.is_some_and(|id| id as u64 != message) {
            return cx.reply_ephemeral(t!(cx.lang(), "party-v2.denied")).await;
        }
        if game.message_id.is_none()
            && (seat.is_some_and(|i| s.seats.get(i).is_some_and(|p| p.active)) || action == "join" && s.lobby)
        {
            cx.app
                .db
                .remember_game_message(game.id, game.channel_id as u64, message)
                .await?;
        }
    }
    let mut join = None;
    let name = cx.author_display_name();
    if action == "join" && s.lobby && seat.is_none() {
        let Some(seat) = s.add_seat() else {
            return cx.reply_ephemeral(t!(cx.lang(), "party-v2.full")).await;
        };
        join = Some((user, name.as_str(), seat as i16));
    } else {
        let Some(seat) = seat.filter(|&i| s.seats.get(i).is_some_and(|p| p.active)) else {
            return cx.reply_ephemeral(t!(cx.lang(), "party-v2.denied")).await;
        };
        match action {
            "start" if game.host_id as u64 == user => {
                if !s.start() {
                    return cx.reply_ephemeral(t!(cx.lang(), "party-v2.minimum")).await;
                }
            }
            "leave" => {
                if s.lobby && game.host_id as u64 == user {
                    s.cancel();
                } else {
                    s.forfeit(seat);
                }
            }
            "cancel" if game.host_id as u64 == user => s.cancel(),
            "answer" if !s.lobby && matches!(s.kind, Kind::Words | Kind::Story | Kind::Chess) => {
                if (matches!(s.kind, Kind::Story)
                    || s.kind == Kind::Words && matches!(word_mode(&s.mode, s.round), "relay" | "chain"))
                    && seat != s.turn
                {
                    return cx.reply_ephemeral(t!(cx.lang(), "party-v2.turn")).await;
                }
                return cx
                    .modal(id(&game, &s, "submit"), &s.kind.title(s.lang), vec![field(s.lang)])
                    .await;
            }
            _ => {
                let action = input.unwrap_or(action);
                let strict = party_games::strict(&cx.app, game.guild_id as u64).await;
                // A bounded blocking task keeps chess work off the async executor.
                let result = if s.kind == Kind::Chess {
                    let mut owned = s.clone();
                    let action = action.to_owned();
                    let result = tokio::task::spawn_blocking(move || {
                        let r = owned.act(seat, &action, strict);
                        (owned, r)
                    })
                    .await?;
                    s = result.0;
                    result.1
                } else {
                    s.act(seat, action, strict)
                };
                if let Err(key) = result {
                    return cx.reply_ephemeral(t!(cx.lang(), &format!("party-v2.{key}"))).await;
                }
            }
        }
    }
    let Some(moved) = save(&cx.app, &game, &s, join, false).await? else {
        return cx.reply_ephemeral(t!(cx.lang(), "party-v2.expired")).await;
    };
    let data = view(&cx.app, &moved, &s).await?;
    // Modal submissions can update the originating public message, like existing word games.
    cx.update(data).await
}
async fn edit(app: &App, game: &GameSession, data: InteractionResponseData) -> anyhow::Result<()> {
    if let Some(message) = game.message_id {
        let mut embeds = data.embeds.unwrap_or_default();
        app.brand_embeds(game.guild_id as u64, &mut embeds).await;
        app.http
            .update_message(Id::new(game.channel_id as u64), Id::new(message as u64))
            .embeds(Some(&embeds))
            .components(Some(&data.components.unwrap_or_default()))
            .attachments(&data.attachments.unwrap_or_default())
            .allowed_mentions(games::quiet().as_ref())
            .await?;
    }
    Ok(())
}
pub(crate) async fn expire(app: &Arc<App>, game: &GameSession) -> anyhow::Result<bool> {
    let mut s: State = serde_json::from_str(&game.state)?;
    s.timeout();
    let Some(done) = save(app, game, &s, None, true).await? else {
        return Ok(false);
    };
    // Settlement is durable even if the message was deleted or Discord is unavailable.
    let display = async { edit(app, &done, view(app, &done, &s).await?).await }.await;
    if let Err(error) = display {
        tracing::warn!(%error, session = game.id, "reef expiry display failed");
    }
    Ok(true)
}
pub async fn tick(app: &Arc<App>) -> anyhow::Result<usize> {
    let mut count = 0;
    for game in app.db.reef_due(app.now()).await? {
        match expire(app, &game).await {
            Ok(true) => count += 1,
            Ok(false) => {}
            Err(error) => tracing::warn!(%error, session = game.id, "reef expiry failed"),
        }
    }
    Ok(count)
}
