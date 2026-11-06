use twilight_interactions::command::{CommandModel, CreateCommand};
use twilight_util::builder::embed::{EmbedBuilder, EmbedFieldBuilder, EmbedFooterBuilder};

use crate::{Cx, util::BRAND};

#[derive(CommandModel, CreateCommand)]
#[command(name = "help", desc = "What Pinchora can do")]
pub struct Help;

impl Help {
    pub async fn run(self, cx: &Cx) -> anyhow::Result<()> {
        let embed = EmbedBuilder::new()
            .color(BRAND)
            .title("Pinchora")
            .description("A cozy pixel world for your server: levels, shells, profiles and soon your own house.")
            .field(EmbedFieldBuilder::new(
                "Profile",
                "`/profile` your card\n`/rank` server level card\n`/character` design your pixel character\n`/customize background`, `badges`, `title`, `style` and `showcase` dress up your cards\n`/bio` set your about-me\n`/rep` give someone reputation",
            ))
            .field(EmbedFieldBuilder::new(
                "Friends & partners",
                "`/friends add` `/friends list` and a quick pick in `/house visit`\n`/partner propose` pair up, then decorate each other's house with `/house edit owner:`\n`/hug` `/pat` `/wave` `/highfive` send an animated GIF of your characters",
            ))
            .field(EmbedFieldBuilder::new(
                "Shells",
                "`/daily` daily shells with streak bonus\n`/balance` your wallet\n`/give` send shells to a friend",
            ))
            .field(EmbedFieldBuilder::new(
                "Games",
                "`/duel connect4`, `/duel rps` and `/duel tictactoe` challenge a friend (optional wager)\n`/play hangman`, `/play wordle` and `/play trivia` play on your own or with the channel\n`/play stats` your wins",
            ))
            .field(EmbedFieldBuilder::new(
                "Board & puzzle games",
                "`/duel seafleet` hide your ships and sink theirs\n`/duel chess` and `/duel checkers` on pixel boards (all three take an optional wager)\n`/play picross` a daily nonogram that hides a sea creature, or a practice one\n`/play 2048` slide and merge sea creatures",
            ))
            .field(EmbedFieldBuilder::new(
                "Leaderboards",
                "`/leaderboard` server XP, server points, global XP or global shells",
            ))
            .field(EmbedFieldBuilder::new(
                "Server points",
                "`/points show` your balance (chatting earns points)\n`/store` buy roles with points",
            ))
            .field(EmbedFieldBuilder::new(
                "Server events",
                "`/poll` ask the server a question\n`/suggest` suggest something (managers decide with `/suggestion`)\n`/giveaway` managers run prize draws",
            ))
            .field(EmbedFieldBuilder::new(
                "Server admins",
                "`/config welcome` welcome cards\n`/config leveling` level-up messages\n`/config level-roles` roles for reaching a level\n`/config points` and `/config store` points and role store\n`/config suggestions` where suggestions go\n`/points give` and `/points take` adjust balances\n`/config show` current settings",
            ))
            .footer(EmbedFooterBuilder::new("Houses, pets and mini games are on the way."))
            .build();
        cx.reply_embed(embed).await
    }
}
