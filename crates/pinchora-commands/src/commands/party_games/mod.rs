//! `/party`: Hot Pearl, Acronym Tide, Caption the Postcard, Wordle Race, Would You Rather and Truth or
//! Dare. The group games share a lobby (Join, Leave, Start, Cancel) and one message that every phase
//! edits; what is on it, and when it changes, lives in `party_games` so a restart loses nothing. A move
//! made by a click, and one made by the clock, go through the same compare-and-swap in the database, so
//! only one of them gets to announce it. `/party stop` lets the host or a manager end a game that is on.

mod dare;
mod fun;
mod lobby;
mod nhie;
mod pearl;
mod postcard;
mod race;
mod rather;
mod settings;
mod stop;
mod text;
mod writing;

pub use settings::ConfigPartyGames;
pub(crate) use settings::party_summary;

use std::{future::Future, str::Split, sync::Arc, time::Duration};

use chrono::{DateTime, Duration as Span, Utc};
use pinchora_core::{i18n::Lang, t};
use pinchora_db::{PartyGame, PartyPlayer};
use pinchora_game::party::{Party, prompts::Category};
use pinchora_render::{StandingRow, StandingsCard, Theme};
use twilight_interactions::command::{CommandModel, CommandOption, CreateCommand, CreateOption};
use twilight_model::{
    channel::message::{AllowedMentions, Component, component::ButtonStyle},
    http::interaction::InteractionResponseData,
    id::Id,
};
use twilight_util::builder::embed::EmbedBuilder;

use super::games::{button, channel_of, quiet, record};
use crate::{App, Cx};

/// A server's strict setting, for the filter.
pub(crate) async fn strict(app: &App, guild: u64) -> bool {
    settings::strict(app, guild).await
}

#[derive(CommandModel, CreateCommand)]
#[command(
    name = "party",
    desc = "Party games for the channel: word games, acronyms, postcards, polls and dares",
    contexts = "guild"
)]
#[allow(clippy::large_enum_variant)] // parsed once per interaction, never stored
pub enum PartyCommand {
    #[command(name = "hotpearl")]
    HotPearl(PartyHotPearl),
    #[command(name = "acronym")]
    Acronym(PartyAcronym),
    #[command(name = "postcard")]
    Postcard(PartyPostcard),
    #[command(name = "wordrace")]
    WordRace(PartyWordRace),
    #[command(name = "wouldyourrather")]
    WouldYouRather(PartyRather),
    #[command(name = "truthordare")]
    TruthOrDare(PartyDare),
    #[command(name = "stop")]
    Stop(PartyStop),
    #[command(name = "nhie")]
    Nhie(PartyNhie),
    #[command(name = "fun")]
    Fun(PartyFun),
}

#[derive(CommandOption, CreateOption, Clone, Copy, Debug, PartialEq, Eq)]
pub enum FunMode {
    #[option(name = "Magic Conch", value = "conch")]
    Conch,
    #[option(name = "Choose", value = "choose")]
    Choose,
    #[option(name = "Roll", value = "roll")]
    Roll,
}

#[derive(CommandModel, CreateCommand)]
#[command(name = "fun", desc = "Ask the Magic Conch, choose an option or roll a die")]
pub struct PartyFun {
    /// Which toy to play
    mode: FunMode,
    /// A question, or comma-separated choices
    prompt: Option<String>,
    /// Dice sides from 2 to 100; default 6
    #[command(min_value = 2, max_value = 100)]
    sides: Option<i64>,
}

#[derive(CommandModel, CreateCommand)]
#[command(name = "nhie", desc = "Never Have I Ever: a family-friendly group question")]
pub struct PartyNhie;

#[derive(CommandModel, CreateCommand)]
#[command(
    name = "hotpearl",
    desc = "Type a word with the syllable before the pearl pops; last one with lives wins"
)]
pub struct PartyHotPearl {
    /// Lives each player starts with (default 2)
    #[command(min_value = 1, max_value = 3)]
    lives: Option<i64>,
}

#[derive(CommandModel, CreateCommand)]
#[command(
    name = "acronym",
    desc = "Write a phrase from a few letters, then vote for the best one"
)]
pub struct PartyAcronym {
    /// How many rounds to play (default 3)
    #[command(min_value = 1, max_value = 5)]
    rounds: Option<i64>,
}

#[derive(CommandModel, CreateCommand)]
#[command(
    name = "postcard",
    desc = "Caption a pixel postcard, then vote for the funniest caption"
)]
pub struct PartyPostcard {
    /// How many rounds to play (default 3)
    #[command(min_value = 1, max_value = 5)]
    rounds: Option<i64>,
}

#[derive(CommandModel, CreateCommand)]
#[command(
    name = "wordrace",
    desc = "Race two to four players to find the same five-letter word on private boards"
)]
pub struct PartyWordRace;

#[derive(CommandOption, CreateOption, Clone, Copy, Debug, PartialEq, Eq)]
pub enum Topic {
    #[option(name = "Silly", value = "silly")]
    Silly,
    #[option(name = "Food", value = "food")]
    Food,
    #[option(name = "Adventure", value = "adventure")]
    Adventure,
    #[option(name = "Ocean", value = "ocean")]
    Ocean,
    #[option(name = "Brainy", value = "brainy")]
    Brainy,
    #[option(name = "Cozy", value = "cozy")]
    Cozy,
}

impl Topic {
    pub(crate) fn category(self) -> Category {
        match self {
            Self::Silly => Category::Silly,
            Self::Food => Category::Food,
            Self::Adventure => Category::Adventure,
            Self::Ocean => Category::Ocean,
            Self::Brainy => Category::Brainy,
            Self::Cozy => Category::Cozy,
        }
    }
}

#[derive(CommandModel, CreateCommand)]
#[command(
    name = "wouldyourrather",
    desc = "Post a would-you-rather question for the channel to vote on"
)]
pub struct PartyRather {
    /// Pick a topic (default: anything)
    topic: Option<Topic>,
}

#[derive(CommandModel, CreateCommand)]
#[command(
    name = "truthordare",
    desc = "Take turns picking truth or dare; a random player is chosen each round"
)]
pub struct PartyDare {
    /// Pick a topic (default: anything)
    topic: Option<Topic>,
}

#[derive(CommandModel, CreateCommand)]
#[command(
    name = "stop",
    desc = "Stop the party game running in this channel (its host or a server manager)"
)]
pub struct PartyStop;

impl PartyCommand {
    pub async fn run(self, cx: &Cx) -> anyhow::Result<()> {
        match self {
            Self::HotPearl(c) => {
                pearl::open(
                    cx,
                    c.lives
                        .map_or(pinchora_game::party::hotpearl::DEFAULT_LIVES, |n| n as i32),
                )
                .await
            }
            Self::Acronym(c) => {
                writing::open(
                    cx,
                    Party::Acronym,
                    c.rounds
                        .map_or(pinchora_game::party::acronym::DEFAULT_ROUNDS, |n| n as i32),
                )
                .await
            }
            Self::Postcard(c) => {
                writing::open(
                    cx,
                    Party::Postcard,
                    c.rounds
                        .map_or(pinchora_game::party::postcard::DEFAULT_ROUNDS, |n| n as i32),
                )
                .await
            }
            Self::WordRace(_) => race::open(cx).await,
            Self::WouldYouRather(c) => rather::open(cx, c.topic.map(Topic::category)).await,
            Self::TruthOrDare(c) => dare::open(cx, c.topic.map(Topic::category)).await,
            Self::Stop(_) => stop::run(cx).await,
            Self::Nhie(_) => nhie::open(cx).await,
            Self::Fun(c) => fun::run(cx, c.mode, c.prompt, c.sides).await,
        }
    }
}

// /Routing

/// Routes a click on a party game's message, by the prefix of its custom id.
pub(crate) async fn on_component(cx: &Cx, prefix: &str, parts: Split<'_, char>) -> anyhow::Result<()> {
    match Party::from_prefix(prefix) {
        Some(Party::HotPearl) => pearl::on_button(cx, parts).await,
        Some(party @ (Party::Acronym | Party::Postcard)) => writing::on_button(cx, party, parts).await,
        Some(Party::WordRace) => race::on_button(cx, parts).await,
        Some(Party::TruthOrDare) => dare::on_button(cx, parts).await,
        Some(Party::WouldYouRather) => rather::on_button(cx, parts).await,
        Some(Party::NeverHaveIEver) => nhie::on_button(cx, parts).await,
        None => cx.reply_ephemeral(t!(cx.lang(), "party.expired-button")).await,
    }
}

/// Routes a submitted form of a party game.
pub(crate) async fn on_modal(
    cx: &Cx,
    prefix: &str,
    parts: Split<'_, char>,
    fields: &[(String, String)],
) -> anyhow::Result<()> {
    match Party::from_prefix(prefix) {
        Some(Party::HotPearl) => pearl::on_modal(cx, parts, fields).await,
        Some(party @ (Party::Acronym | Party::Postcard)) => writing::on_modal(cx, party, parts, fields).await,
        Some(Party::WordRace) => race::on_modal(cx, parts, fields).await,
        _ => cx.reply_ephemeral(t!(cx.lang(), "party.expired-form")).await,
    }
}

// /Shared

/// The language a game is played and shown in: the one its state records (every game keeps it in `lang`),
/// English for one that has none. What the whole table sees (the lobby, the cards, what the clock posts) is
/// in this language, so a message does not change language with whoever clicked last; what only one player
/// sees is in theirs.
pub(crate) fn game_lang(game: &PartyGame) -> Lang {
    serde_json::from_str::<serde_json::Value>(&game.state)
        .ok()
        .and_then(|state| serde_json::from_value(state.get("lang")?.clone()).ok())
        .unwrap_or_default()
}

pub(crate) fn theme() -> Theme {
    Theme::reef()
}

/// The `prefix:action:...` id of a button or form of `party`.
pub(crate) fn custom_id(party: Party, parts: &[&dyn std::fmt::Display]) -> String {
    let mut id = party.prefix().to_owned();
    for part in parts {
        id.push(':');
        id.push_str(&part.to_string());
    }
    id
}

/// When a wait of `secs` from now ends, as the game's clock sees it.
pub(crate) fn after(app: &App, secs: i64) -> DateTime<Utc> {
    app.now() + Span::seconds(secs)
}

/// Days since 1970, for the daily cap on paid games.
pub(crate) fn day(app: &App) -> i32 {
    app.now().timestamp().div_euclid(86_400) as i32
}

/// A public message with a picture: the embed, the PNG as its image, and the buttons. Nobody is pinged.
pub(crate) fn card(
    content: Option<String>,
    embed: EmbedBuilder,
    file: &str,
    png: Vec<u8>,
    components: Vec<Component>,
) -> InteractionResponseData {
    let mut data = Cx::card(embed, file, png, components, false);
    data.content = Some(content.unwrap_or_default());
    data.allowed_mentions = quiet();
    data
}

/// One row of up to five buttons.
pub(crate) fn row(buttons: Vec<Component>) -> Component {
    crate::server_events::row(buttons)
}

pub(crate) fn push_button(
    party: Party,
    parts: &[&dyn std::fmt::Display],
    label: &str,
    style: ButtonStyle,
) -> Component {
    button(custom_id(party, parts), label, style, false)
}

/// Shows `data` on the game's message by editing it with the bot's own token, for changes nobody clicked
/// for: the clock, or somebody else's private move. False when Discord refuses (the message is gone).
pub(crate) async fn edit_card(app: &App, game: &PartyGame, data: &InteractionResponseData) -> bool {
    let Some((channel, message)) = game.location() else {
        return false;
    };
    let result = async {
        let none = AllowedMentions::default();
        let mut request = app.http.update_message(Id::new(channel), Id::new(message));
        if let Some(content) = &data.content {
            request = request.content(Some(content));
        }
        if let Some(embeds) = &data.embeds {
            request = request.embeds(Some(embeds));
        }
        if let Some(components) = &data.components {
            request = request.components(Some(components));
        }
        if let Some(attachments) = &data.attachments {
            request = request.attachments(attachments);
        }
        request.allowed_mentions(Some(&none)).await?;
        anyhow::Ok(())
    }
    .await;
    if let Err(error) = &result {
        tracing::debug!(%error, channel, message, game = game.id, "party message not edited");
    }
    result.is_ok()
}

/// Gets a late game moving once the click that found it late has been answered. A click that opens a form
/// has to be answered within three seconds with no defer to fall back on, and the step it owes (a card to
/// draw and send, a payout) can take longer on a busy machine; the ticker would take the same step a moment
/// later, and whichever gets there first wins the compare-and-swap.
pub(crate) fn later<F, Fut>(app: &Arc<App>, game: i64, step: F)
where
    F: FnOnce(Arc<App>) -> Fut + Send + 'static,
    Fut: Future<Output = anyhow::Result<bool>> + Send + 'static,
{
    let owned = app.clone();
    app.spawn(async move {
        if let Err(error) = step(owned).await {
            tracing::warn!(%error, game, "late party game not moved on");
        }
    });
}

/// Remembers the message a click was on as the game's message, when the game does not know it yet.
pub(crate) async fn remember(cx: &Cx, game: &PartyGame) {
    if game.message_id.is_none()
        && let Some((channel, message)) = cx.clicked_message()
        && let Err(error) = cx.app.db.set_party_message(game.id, channel, message).await
    {
        tracing::debug!(%error, game = game.id, "party message not remembered");
    }
}

/// Learns which message a command just posted and keeps it on the game, so the ticker can edit it. Best
/// effort: without it nothing is shown until somebody clicks.
pub(crate) fn track_message(cx: &Cx, game: i64) {
    let app = cx.app.clone();
    let responder = cx.responder.clone();
    // The game stays keyed to the channel it was started in, which is what keeps a second one out of it.
    let channel = channel_of(cx);
    cx.app.spawn(async move {
        // The answer to the command may still be on its way to Discord.
        tokio::time::sleep(Duration::from_millis(1_500)).await;
        let message = responder.original().await;
        match message {
            Ok((_, message)) => {
                if let Err(error) = app.db.set_party_message(game, channel, message).await {
                    tracing::debug!(%error, game, "party message not remembered");
                }
            }
            Err(error) => tracing::debug!(%error, game, "could not look up the party message"),
        }
    });
}

/// The game and its players as the database has them now, when `id` is a game of `party`. A custom id is
/// only as good as the click that sends it: a forged `ac:w:<id>` of a Postcard game (or any other game's
/// id under this game's prefix) finds nothing, as if the game had expired.
pub(crate) async fn snapshot(
    app: &App,
    party: Party,
    id: i64,
) -> anyhow::Result<Option<(PartyGame, Vec<PartyPlayer>)>> {
    let Some(game) = app.db.party(id).await?.filter(|game| game.kind == party.key()) else {
        return Ok(None);
    };
    let players = app.db.party_players(id).await?;
    Ok(Some((game, players)))
}

/// The names of the players, in seat order.
pub(crate) fn names(players: &[PartyPlayer]) -> Vec<String> {
    players.iter().map(|p| p.name.clone()).collect()
}

/// The name of a player by id, or a plain label for somebody who is gone.
pub(crate) fn name_of(lang: Lang, players: &[PartyPlayer], user: u64) -> String {
    players
        .iter()
        .find(|p| p.id() == user)
        .map_or_else(|| t!(lang, "party.somebody"), |p| p.name.clone())
}

/// How a round is named on a card and over a vote: `Round 2/3`.
pub(crate) fn round_label(lang: Lang, round: i32, rounds: i32) -> String {
    t!(lang, "party.round", round = round, rounds = rounds)
}

/// Counts a finished game for each of `players`: played, and won for the ones in `winners`.
pub(crate) async fn count_games(app: &App, party: Party, players: &[PartyPlayer], winners: &[u64]) {
    for player in players {
        record(app, party.game(), player.id(), winners.contains(&player.id())).await;
    }
}

/// A finished game's standings as a picture: `rows` already ranked. The card draws its title, footer, scores
/// and notes in capitals, which Turkish spells with its own İ, so they are capitalised here.
pub(crate) async fn standings_png(
    app: &App,
    lang: Lang,
    title: &str,
    subtitle: &str,
    rows: Vec<(String, String, String, bool)>,
    footer: &str,
) -> anyhow::Result<Vec<u8>> {
    let (title, subtitle, footer) = (lang.upper(title), subtitle.to_owned(), lang.upper(footer));
    let rows: Vec<(String, String, String, bool)> = rows
        .into_iter()
        .map(|(name, score, note, winner)| (name, lang.upper(&score), lang.upper(&note), winner))
        .collect();
    app.render(move |r| {
        let rows: Vec<StandingRow> = rows
            .iter()
            .enumerate()
            .map(|(i, (name, score, note, winner))| StandingRow {
                rank: i as u32 + 1,
                name,
                score,
                note,
                winner: *winner,
            })
            .collect();
        r.standings_card(&StandingsCard {
            title: &title,
            subtitle: &subtitle,
            rows: &rows,
            footer: &footer,
            theme: theme(),
        })
    })
    .await
}

/// What a payout line says: `+25 shells`, a note that the day's limit was reached, or nothing.
pub(crate) fn paid_note(lang: Lang, paid: &[(u64, i64)], user: u64) -> String {
    match paid.iter().find(|(who, _)| *who == user) {
        Some((_, shells)) => paid_text(lang, *shells),
        None => String::new(),
    }
}

/// `+25 shells`, or a note that the day's limit was reached when nothing was paid.
pub(crate) fn paid_text(lang: Lang, shells: i64) -> String {
    if shells > 0 {
        t!(lang, "party.paid", shells = shells)
    } else {
        t!(lang, "party.daily-limit")
    }
}

// /Clock

/// Handles whatever is late: closes a lobby nobody started, pops a pearl, ends a writing or voting phase,
/// ends a race, a poll, a quiet turn. Each game moves exactly once whichever pod or click gets there
/// first. Returns how many it moved.
pub async fn tick(app: &Arc<App>) -> anyhow::Result<usize> {
    let due = app.db.due_parties(app.now(), 50).await?;
    let mut moved = 0;
    for game in due {
        let id = game.id;
        match Box::pin(due_game(app, game)).await {
            Ok(true) => moved += 1,
            Ok(false) => {}
            Err(error) => tracing::warn!(%error, game = id, "party game not moved on"),
        }
    }
    Ok(moved)
}

async fn due_game(app: &Arc<App>, game: PartyGame) -> anyhow::Result<bool> {
    if game.in_lobby() {
        return lobby::expire(app, game).await;
    }
    match Party::from_key(&game.kind) {
        Some(Party::HotPearl) => pearl::on_due(app, game).await,
        Some(Party::Acronym | Party::Postcard) => writing::on_due(app, game).await,
        Some(Party::WordRace) => race::on_due(app, game).await,
        Some(Party::TruthOrDare) => dare::on_due(app, game).await,
        Some(Party::WouldYouRather) => rather::on_due(app, game).await,
        Some(Party::NeverHaveIEver) => nhie::on_due(app, game).await,
        None => {
            tracing::warn!(kind = %game.kind, game = game.id, "late party game of an unknown kind");
            Ok(false)
        }
    }
}

/// Runs [`tick`] every two seconds for the life of the process, and forgets old games once an hour.
pub fn spawn_ticker(app: Arc<App>) {
    tokio::spawn(async move {
        let mut wake = tokio::time::interval(Duration::from_secs(2));
        wake.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let mut rounds = 0u32;
        loop {
            wake.tick().await;
            if let Err(error) = Box::pin(tick(&app)).await {
                tracing::warn!(%error, "party tick failed");
            }
            rounds = rounds.wrapping_add(1);
            if rounds.is_multiple_of(1_800) {
                match app
                    .db
                    .forget_old_parties(app.now() - Span::days(2), day(&app) - 3)
                    .await
                {
                    Ok(n) if n > 0 => tracing::debug!(n, "old party games forgotten"),
                    Ok(_) => {}
                    Err(error) => tracing::warn!(%error, "old party games not forgotten"),
                }
            }
        }
    });
}
