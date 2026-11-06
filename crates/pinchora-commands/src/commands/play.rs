//! `/play`: hangman, wordle, trivia and everyone's game stats.

use pinchora_game::play::{Game, TriviaCategory};
use twilight_interactions::command::{CommandModel, CommandOption, CreateCommand, CreateOption, ResolvedUser};
use twilight_util::builder::embed::{EmbedBuilder, EmbedFieldBuilder, EmbedFooterBuilder};

use super::{derby, hangman, heist, shells, trivia, wordle};
use crate::{
    Cx,
    util::{BRAND, resolved_display_name},
};

#[derive(CommandModel, CreateCommand)]
#[command(
    name = "play",
    desc = "Games: hangman, wordle, trivia, crab derby, heist, shell game and your game stats",
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
        return cx.reply_ephemeral("Bots don't play games. They just referee.").await;
    }
    let rows = cx.app.db.game_stats(id).await?;
    let (mut played, mut won) = (0, 0);
    let mut embed = EmbedBuilder::new().color(BRAND).title(format!("{name}'s games"));
    for game in Game::ALL {
        let row = rows.iter().find(|r| r.game == game.key());
        let value = match row {
            Some(r) if r.played > 0 => {
                played += r.played;
                won += r.won;
                format!(
                    "**{}** won of **{}**\n{}% wins",
                    r.won,
                    r.played,
                    r.won * 100 / r.played
                )
            }
            _ => "Not played yet".to_owned(),
        };
        embed = embed.field(EmbedFieldBuilder::new(format!("{} {}", game.emoji(), game.label()), value).inline());
    }
    embed = embed.description(if played == 0 {
        "No games yet. Try `/duel connect4`, `/play wordle` or `/play trivia`!".to_owned()
    } else {
        format!("**{won}** wins in **{played}** games.")
    });
    let embed = embed
        .footer(EmbedFooterBuilder::new(
            "Counted when a game finishes; timeouts and declined challenges don't count.",
        ))
        .build();
    cx.reply_embed(embed).await
}
