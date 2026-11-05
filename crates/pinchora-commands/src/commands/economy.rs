use chrono::Utc;
use pinchora_core::economy::{DAILY_COOLDOWN, DAILY_STREAK_WINDOW};
use pinchora_db::TransferOutcome;
use pinchora_game::Content;
use pinchora_render::WalletCard;
use twilight_interactions::command::{CommandModel, CreateCommand, ResolvedUser};
use twilight_util::builder::embed::EmbedBuilder;

use crate::{
    Cx,
    commands::customize::{self, Cosmetics, theme},
    util::{SUCCESS, mention, resolved_display_name, thousands},
};

#[derive(CommandModel, CreateCommand)]
#[command(name = "balance", desc = "Show a shell wallet")]
pub struct Balance {
    /// Whose wallet to show (default: you)
    user: Option<ResolvedUser>,
}

impl Balance {
    pub async fn run(self, cx: &Cx) -> anyhow::Result<()> {
        let (user, name) = match &self.user {
            Some(r) => (r.resolved.clone(), resolved_display_name(r)),
            None => (cx.author()?.clone(), cx.author_display_name()),
        };
        let row = cx.app.db.user(user.id.get()).await?;
        let now = Utc::now();
        let streak = match row.last_daily_at {
            Some(t) if now - t <= DAILY_STREAK_WINDOW => row.daily_streak,
            _ => 0,
        };
        let next_daily = match row.last_daily_at {
            Some(t) if now - t < DAILY_COOLDOWN => {
                let mins = (t + DAILY_COOLDOWN - now).num_minutes().max(1);
                format!("{}h {}m", mins / 60, mins % 60)
            }
            _ => "ready".to_owned(),
        };
        let pets = cx.app.db.pets(user.id.get()).await?.len() as i64;
        let fish_found = cx.app.db.fish_book_count(user.id.get()).await?;
        let fish_total = pinchora_game::Content::get().fish.len() as i64;
        let avatar = cx.app.avatar(&user, WalletCard::AVATAR_SIZE, Some(34)).await;
        let background = row.background.as_deref().and_then(|id| Content::get().background(id));
        let cosmetics = Cosmetics::new(&cx.app.sprites, background, &[]);
        let theme = theme(row.accent_color, background);
        let png = cx
            .app
            .render(move |r| {
                r.wallet_card(&WalletCard {
                    display_name: &name,
                    avatar_png: &avatar,
                    shells: row.credits,
                    pearls: row.pearls,
                    streak,
                    next_daily: &next_daily,
                    pets,
                    fish_found,
                    fish_total,
                    backdrop: cosmetics.backdrop(),
                    theme,
                })
            })
            .await?;
        cx.reply_png("wallet.png", png).await
    }
}

#[derive(CommandModel, CreateCommand)]
#[command(name = "give", desc = "Send shells to someone")]
pub struct Give {
    /// Who gets the shells
    user: ResolvedUser,
    /// How many shells
    #[command(min_value = 1, max_value = 1_000_000_000)]
    amount: i64,
}

impl Give {
    pub async fn run(self, cx: &Cx) -> anyhow::Result<()> {
        let from = cx.author()?.id;
        let to = self.user.resolved.id;
        if to == from {
            return cx.reply_ephemeral("You can't send shells to yourself.").await;
        }
        if self.user.resolved.bot {
            return cx.reply_ephemeral("Bots have no use for shells.").await;
        }
        match cx.app.db.transfer_credits(from.get(), to.get(), self.amount).await? {
            TransferOutcome::Done { sender_balance } => {
                let embed = EmbedBuilder::new()
                    .color(SUCCESS)
                    .description(format!(
                        "Sent **{}** shells to {}. You have **{}** left.",
                        thousands(self.amount),
                        mention(to),
                        thousands(sender_balance)
                    ))
                    .build();
                cx.reply_embed(embed).await?;
                // Not a stat that goes up for the sender, but a catch-up for whatever other commands
                // (a stall sale, a quest) moved since the last notice.
                customize::announce(cx).await;
                Ok(())
            }
            TransferOutcome::Insufficient { balance } => {
                cx.reply_ephemeral(format!("You only have {} shells.", thousands(balance)))
                    .await
            }
        }
    }
}
