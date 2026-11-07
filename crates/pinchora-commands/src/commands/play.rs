//! `/play`: hangman, wordle, trivia and everyone's game stats.

use pinchora_core::t;
use pinchora_game::play::{Game, TriviaCategory};
use twilight_interactions::command::{CommandModel, CommandOption, CreateCommand, CreateOption, ResolvedUser};
use twilight_model::http::interaction::InteractionResponseData;
use twilight_util::builder::embed::{EmbedBuilder, EmbedFieldBuilder, EmbedFooterBuilder};

use super::{
    board::{picross, tide2048},
    derby, hangman, heist, shells, trivia, wordle,
};
use crate::{
    Cx,
    util::{BRAND, resolved_display_name, thousands_in},
};

/// How many fields Discord takes in one embed.
const MAX_EMBED_FIELDS: usize = 25;

#[derive(CommandModel, CreateCommand)]
#[command(
    name = "play",
    desc = "Games: hangman, wordle, trivia, picross, 2048, derby, heist, shell game and stats",
    contexts = "guild"
)]
#[allow(clippy::large_enum_variant)] // parsed once per interaction, never stored
pub enum PlayCommand {
    #[command(name = "hangman")]
    Hangman(PlayHangman),
    #[command(name = "wordle")]
    Wordle(PlayWordle),
    #[command(name = "trivia")]
    Trivia(PlayTrivia),
    #[command(name = "picross")]
    Picross(PlayPicross),
    #[command(name = "2048")]
    Tide2048(PlayTide2048),
    #[command(name = "derby")]
    Derby(PlayDerby),
    #[command(name = "heist")]
    Heist(PlayHeist),
    #[command(name = "shells")]
    Shells(PlayShells),
    #[command(name = "stats")]
    Stats(PlayStats),
}

#[derive(CommandModel, CreateCommand)]
#[command(
    name = "picross",
    desc = "Pixel Picross: fill in the grid from its clues to find the sea creature"
)]
pub struct PlayPicross {
    /// The puzzle everyone shares today (the default), or a random one to practice on
    mode: Option<PicrossMode>,
}

#[derive(CommandOption, CreateOption, Clone, Copy, Debug, PartialEq, Eq)]
pub enum PicrossMode {
    #[option(name = "Daily puzzle (the same for everyone)", value = "daily")]
    Daily,
    #[option(name = "Practice (a random puzzle)", value = "practice")]
    Practice,
}

#[derive(CommandModel, CreateCommand)]
#[command(
    name = "2048",
    desc = "Tide 2048: slide the sea creatures and merge them into bigger ones"
)]
pub struct PlayTide2048;

#[derive(CommandModel, CreateCommand)]
#[command(name = "hangman", desc = "Guess the word before the tide takes the sandcastle")]
pub struct PlayHangman;

#[derive(CommandModel, CreateCommand)]
#[command(name = "wordle", desc = "Guess the five-letter word in six tries")]
pub struct PlayWordle {
    /// The word everyone shares today (the default), or a random one to practice on
    mode: Option<WordleMode>,
}

#[derive(CommandOption, CreateOption, Clone, Copy, Debug, PartialEq, Eq)]
pub enum WordleMode {
    #[option(name = "Daily word (the same for everyone)", value = "daily")]
    Daily,
    #[option(name = "Practice (a random word)", value = "practice")]
    Practice,
}

#[derive(CommandModel, CreateCommand)]
#[command(
    name = "trivia",
    desc = "A trivia round for the channel: 20 seconds, anyone can answer"
)]
pub struct PlayTrivia {
    /// Pick a topic (default: anything)
    category: Option<TriviaTopic>,
}

#[derive(CommandOption, CreateOption, Clone, Copy, Debug, PartialEq, Eq)]
pub enum TriviaTopic {
    #[option(name = "Ocean", value = "ocean")]
    Ocean,
    #[option(name = "Animals", value = "animals")]
    Animals,
    #[option(name = "Nature", value = "nature")]
    Nature,
    #[option(name = "Science", value = "science")]
    Science,
    #[option(name = "Geography", value = "geography")]
    Geography,
    #[option(name = "Food", value = "food")]
    Food,
    #[option(name = "History", value = "history")]
    History,
    #[option(name = "Home", value = "home")]
    Home,
}

impl TriviaTopic {
    fn category(self) -> TriviaCategory {
        match self {
            Self::Ocean => TriviaCategory::Ocean,
            Self::Animals => TriviaCategory::Animals,
            Self::Nature => TriviaCategory::Nature,
            Self::Science => TriviaCategory::Science,
            Self::Geography => TriviaCategory::Geography,
            Self::Food => TriviaCategory::Food,
            Self::History => TriviaCategory::History,
            Self::Home => TriviaCategory::Home,
        }
    }
}

#[derive(CommandModel, CreateCommand)]
#[command(
    name = "derby",
    desc = "Hermit Crab Derby: everyone picks a crab and watches the race; winners share the stakes"
)]
pub struct PlayDerby {
    /// Shells each racer stakes on a crab (10-500, needs /config gambling); empty races for fun
    #[command(min_value = 10, max_value = 500)]
    bet: Option<i64>,
}

#[derive(CommandModel, CreateCommand)]
#[command(
    name = "heist",
    desc = "Treasure Heist: crew up against a sunken vault, nobody loses shells"
)]
pub struct PlayHeist;

#[derive(CommandModel, CreateCommand)]
#[command(
    name = "shells",
    desc = "Shell Game: find the pearl under one of three shells (needs gambling switched on)"
)]
pub struct PlayShells {
    /// Shells to stake (5-100); a right guess pays two and a half times
    #[command(min_value = 5, max_value = 100)]
    bet: i64,
}

#[derive(CommandModel, CreateCommand)]
#[command(name = "stats", desc = "How many games someone played and won")]
pub struct PlayStats {
    /// Whose stats (default: yours)
    user: Option<ResolvedUser>,
}

impl PlayCommand {
    pub async fn run(self, cx: &Cx) -> anyhow::Result<()> {
        match self {
            Self::Hangman(_) => hangman::start(cx).await,
            Self::Wordle(c) => wordle::start(cx, c.mode != Some(WordleMode::Practice)).await,
            Self::Trivia(c) => trivia::start(cx, c.category.map(TriviaTopic::category)).await,
            Self::Picross(c) => picross::start(cx, c.mode != Some(PicrossMode::Practice)).await,
            Self::Tide2048(_) => tide2048::start(cx).await,
            Self::Derby(c) => derby::start(cx, c.bet).await,
            Self::Heist(_) => heist::start(cx).await,
            Self::Shells(c) => shells::start(cx, c.bet).await,
            Self::Stats(c) => stats(cx, c.user).await,
        }
    }
}

async fn stats(cx: &Cx, user: Option<ResolvedUser>) -> anyhow::Result<()> {
    let (id, name) = match &user {
        Some(u) => (u.resolved.id.get(), resolved_display_name(u)),
        None => (cx.author()?.id.get(), cx.author_display_name()),
    };
    if user.as_ref().is_some_and(|u| u.resolved.bot) {
        return cx.reply_ephemeral(t!(cx.lang(), "play.stats.bot")).await;
    }
    if super::privacy::private_profile(cx, id).await? {
        return Ok(());
    }
    let rows = cx.app.db.game_stats(id).await?;
    let lang = cx.lang();
    let (mut played, mut won) = (0, 0);
    let mut fields = Vec::new();
    for game in Game::ALL {
        let row = rows.iter().find(|r| r.game == game.key());
        let value = match row {
            Some(r) if r.played > 0 => {
                played += r.played;
                won += r.won;
                let best = if game == Game::Tide2048 && r.best > 0 {
                    t!(lang, "play.stats.best", score = thousands_in(lang, i64::from(r.best)))
                } else {
                    String::new()
                };
                t!(
                    lang,
                    "play.stats.record",
                    won = r.won,
                    played = r.played,
                    percent = r.won * 100 / r.played,
                    best = best
                )
            }
            _ => t!(lang, "play.stats.none"),
        };
        fields.push(EmbedFieldBuilder::new(format!("{} {}", game.emoji(), game.name(lang)), value).inline());
    }
    // Discord takes 25 fields to an embed, so the games past the 25th go in a second embed.
    let embeds = fields
        .chunks(MAX_EMBED_FIELDS)
        .enumerate()
        .map(|(index, chunk)| {
            let mut embed = EmbedBuilder::new().color(BRAND);
            if index == 0 {
                embed = embed
                    .title(t!(lang, "play.stats.title", name = name))
                    .description(if played == 0 {
                        t!(lang, "play.stats.no-games")
                    } else {
                        t!(lang, "play.stats.summary", won = won, played = played)
                    });
                embed = embed.footer(EmbedFooterBuilder::new(t!(lang, "play.stats.footer")));
            }
            for field in chunk {
                embed = embed.field(field.clone());
            }
            embed.build()
        })
        .collect();
    cx.reply(InteractionResponseData {
        embeds: Some(embeds),
        ..Default::default()
    })
    .await
}
