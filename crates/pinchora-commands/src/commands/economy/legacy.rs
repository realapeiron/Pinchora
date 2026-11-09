use chrono::Utc;
use pinchora_core::economy::legacy::DAILY_COOLDOWN;
use pinchora_core::economy::legacy::DAILY_STREAK_WINDOW;
use pinchora_core::economy::legacy::Period;
use pinchora_core::t;
use pinchora_db::TransferOutcome;
use pinchora_game::Content;
use pinchora_render::WalletCard;
use twilight_interactions::command::{CommandModel, CreateCommand, ResolvedUser};
use twilight_util::builder::embed::EmbedBuilder;

use crate::Cx;
use crate::commands::admin::privacy;
use crate::commands::community::periodic::ready_in;
use crate::commands::social::customize;
use crate::commands::social::customize::Cosmetics;
use crate::commands::social::customize::theme;
use crate::infra::util::SUCCESS;
use crate::infra::util::mention;
use crate::infra::util::resolved_display_name;
use crate::infra::util::thousands_in;

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
        if privacy::private_profile(cx, user.id.get()).await? {
            return Ok(());
        }
        let png = wallet_card_for(&cx.app, &user, name, cx.lang()).await?;
        cx.reply_png("wallet.png", png).await
    }
}

/// The same wallet picture for Discord and the player's web wallet.
pub async fn wallet_card_for(
    app: &crate::App,
    user: &twilight_model::user::User,
    name: String,
    lang: pinchora_core::infra::i18n::Lang,
) -> anyhow::Result<Vec<u8>> {
    wallet_card(app, user, name, lang, false).await
}

/// Wallet preview with the shared bounded renderer; assembly stays identical to Discord.
pub async fn wallet_preview_for(
    app: &crate::App,
    user: &twilight_model::user::User,
    name: String,
    lang: pinchora_core::infra::i18n::Lang,
) -> anyhow::Result<Vec<u8>> {
    wallet_card(app, user, name, lang, true).await
}

async fn wallet_card(
    app: &crate::App,
    user: &twilight_model::user::User,
    name: String,
    lang: pinchora_core::infra::i18n::Lang,
    preview: bool,
) -> anyhow::Result<Vec<u8>> {
    let row = app.db.user(user.id.get()).await?;
    let now = Utc::now();
    let streak = match row.last_daily_at {
        Some(t) if now - t <= DAILY_STREAK_WINDOW => row.daily_streak,
        _ => 0,
    };
    let next_daily = ready_in(row.last_daily_at, DAILY_COOLDOWN, now, lang);
    // The weekly and monthly rewards run on the game clock like their commands do.
    let game_now = app.now();
    let next_weekly = ready_in(
        app.db.period_claimed_at(user.id.get(), Period::Weekly).await?,
        Period::Weekly.cooldown(),
        game_now,
        lang,
    );
    let next_monthly = ready_in(
        app.db.period_claimed_at(user.id.get(), Period::Monthly).await?,
        Period::Monthly.cooldown(),
        game_now,
        lang,
    );
    let pets = app.db.pets(user.id.get()).await?.len() as i64;
    let fish_found = app.db.fish_book_count(user.id.get()).await?;
    let fish_total = pinchora_game::Content::get().fish.len() as i64;
    let avatar = app.avatar(user, WalletCard::AVATAR_SIZE, Some(34)).await;
    let background = row.background.as_deref().and_then(|id| Content::get().background(id));
    let cosmetics = Cosmetics::new(&app.sprites, background, &[]);
    let theme = theme(&row, background);
    let draw = move |r: &pinchora_render::Renderer| {
        r.wallet_card(&WalletCard {
            lang,
            display_name: &name,
            avatar_png: &avatar,
            shells: row.credits,
            pearls: row.pearls,
            streak,
            next_daily: &next_daily,
            next_weekly: &next_weekly,
            next_monthly: &next_monthly,
            pets,
            fish_found,
            fish_total,
            backdrop: cosmetics.backdrop(),
            theme,
        })
    };
    if preview {
        Ok(app.render_preview(draw).await?)
    } else {
        app.render(draw).await
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
            return cx.reply_ephemeral(t!(cx.lang(), "give.self")).await;
        }
        if self.user.resolved.bot {
            return cx.reply_ephemeral(t!(cx.lang(), "give.bot")).await;
        }
        match cx.app.db.transfer_credits(from.get(), to.get(), self.amount).await? {
            TransferOutcome::Done { sender_balance } => {
                let embed = EmbedBuilder::new()
                    .color(SUCCESS)
                    .description(t!(
                        cx.lang(),
                        "give.sent",
                        amount = thousands_in(cx.lang(), self.amount),
                        user = mention(to),
                        balance = thousands_in(cx.lang(), sender_balance)
                    ))
                    .build();
                cx.reply_embed(embed).await?;
                // Not a stat that goes up for the sender, but a catch-up for whatever other commands
                // (a stall sale, a quest) moved since the last notice.
                customize::announce(cx).await;
                Ok(())
            }
            TransferOutcome::Insufficient { balance } => {
                cx.reply_ephemeral(t!(
                    cx.lang(),
                    "give.insufficient",
                    balance = thousands_in(cx.lang(), balance)
                ))
                .await
            }
        }
    }
}
