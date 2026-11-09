use pinchora_core::admin::settings;
use pinchora_core::admin::settings::FieldError;
use pinchora_core::infra::i18n::Lang;
use pinchora_core::t;
use pinchora_db::{Db, GuildSettings};
use twilight_interactions::command::{CommandModel, CreateCommand, ResolvedUser};
use twilight_util::builder::embed::{EmbedBuilder, EmbedFooterBuilder};

use crate::Cx;
use crate::infra::util::BRAND;
use crate::infra::util::SUCCESS;
use crate::infra::util::mention;
use crate::infra::util::resolved_display_name;
use crate::infra::util::thousands_in;

#[derive(CommandModel, CreateCommand)]
#[command(
    name = "points",
    desc = "Server points: earned by chatting, spent in /store",
    contexts = "guild"
)]
pub enum PointsCommand {
    #[command(name = "show")]
    Show(PointsShow),
    #[command(name = "give")]
    Give(PointsGive),
    #[command(name = "take")]
    Take(PointsTake),
}

#[derive(CommandModel, CreateCommand)]
#[command(name = "show", desc = "Show a server points balance")]
pub struct PointsShow {
    /// Whose balance to show (default: you)
    user: Option<ResolvedUser>,
}

#[derive(CommandModel, CreateCommand)]
#[command(name = "give", desc = "Give someone server points (Manage Server)")]
pub struct PointsGive {
    /// Who gets the points
    user: ResolvedUser,
    /// How many points
    #[command(min_value = 1, max_value = 1_000_000_000)]
    amount: i64,
}

#[derive(CommandModel, CreateCommand)]
#[command(name = "take", desc = "Take server points from someone (Manage Server)")]
pub struct PointsTake {
    /// Who loses the points
    user: ResolvedUser,
    /// How many points (they never go below zero)
    #[command(min_value = 1, max_value = 1_000_000_000)]
    amount: i64,
}

#[derive(CommandModel, CreateCommand)]
#[command(name = "points", desc = "Server points: what chatting earns")]
pub struct ConfigPoints {
    /// Earn points for chatting
    enabled: Option<bool>,
    /// What the points are called, e.g. coins
    #[command(min_length = 1, max_length = 32)]
    name: Option<String>,
    /// Emoji shown next to amounts: a Unicode emoji or a custom one
    #[command(min_length = 1, max_length = 64)]
    emoji: Option<String>,
    /// Least points one message can earn
    #[command(min_value = 0, max_value = 1_000)]
    min: Option<i64>,
    /// Most points one message can earn
    #[command(min_value = 0, max_value = 1_000)]
    max: Option<i64>,
}

impl ConfigPoints {
    /// Stores what was given. `Err` carries the reason, in `lang`, when the result would make no sense;
    /// the rules are the dashboard's too (`pinchora_core::admin::settings`).
    pub async fn apply(self, db: &Db, guild: u64, lang: Lang) -> anyhow::Result<Result<GuildSettings, String>> {
        let current = db.guild_settings(guild).await?;
        let min = self.min.unwrap_or(current.points_min.into());
        let max = self.max.unwrap_or(current.points_max.into());
        let checked = (|| {
            settings::check_points(min, max)?;
            let name = self.name.as_deref().map(settings::check_points_name).transpose()?;
            let emoji = self.emoji.as_deref().map(settings::check_points_emoji).transpose()?;
            Ok::<_, FieldError>((name, emoji))
        })();
        let (name, emoji) = match checked {
            Ok(values) => values,
            Err(error) => return Ok(Err(error.text(lang))),
        };
        Ok(Ok(db
            .update_points(
                guild,
                self.enabled,
                name.as_deref(),
                emoji.as_deref(),
                self.min.map(|v| v as i32),
                self.max.map(|v| v as i32),
            )
            .await?))
    }
}

/// "points" -> "Points", for titles; Turkish capitalises `i` as `İ` and `ı` as `I`.
pub(crate) fn capitalized(name: &str, lang: Lang) -> String {
    let mut chars = name.chars();
    chars.next().map_or_else(String::new, |c| {
        lang.upper(c.encode_utf8(&mut [0; 4])).chars().chain(chars).collect()
    })
}

impl PointsCommand {
    pub async fn run(self, cx: &Cx) -> anyhow::Result<()> {
        let lang = cx.lang();
        let Some(guild) = cx.guild_id() else {
            return cx.reply_ephemeral(t!(lang, "points.guild-only")).await;
        };
        let settings = cx.app.guild_settings(guild).await?;
        let (emoji, name) = (&settings.points_emoji, &settings.points_name);
        match self {
            Self::Show(c) => {
                let (user, display) = match &c.user {
                    Some(r) => (r.resolved.clone(), resolved_display_name(r)),
                    None => (cx.author()?.clone(), cx.author_display_name()),
                };
                if user.bot {
                    return cx.reply_ephemeral(t!(lang, "points.no-bots", name = name)).await;
                }
                if crate::commands::admin::privacy::private_profile(cx, user.id.get()).await? {
                    return Ok(());
                }
                let (points, rank) = cx.app.db.member_points(guild, user.id.get()).await?;
                let footer = match (rank, settings.points_enabled) {
                    (Some(rank), _) => t!(lang, "points.rank", rank = rank),
                    (None, true) => t!(lang, "points.chat-to-earn", name = name),
                    (None, false) => t!(lang, "points.chat-off", name = name),
                };
                let embed = EmbedBuilder::new()
                    .color(BRAND)
                    .title(format!("{emoji} {}", capitalized(name, lang)))
                    .description(t!(
                        lang,
                        "points.balance",
                        user = mention(user.id),
                        display = display,
                        points = thousands_in(lang, points),
                        emoji = emoji,
                        name = name
                    ))
                    .footer(EmbedFooterBuilder::new(footer))
                    .build();
                cx.reply_embed(embed).await
            }
            Self::Give(c) => {
                if !cx.can_manage_guild() {
                    return cx.reply_ephemeral(t!(lang, "points.give-denied")).await;
                }
                let target = &c.user.resolved;
                if target.bot {
                    return cx.reply_ephemeral(t!(lang, "points.no-bots", name = name)).await;
                }
                let balance = cx.app.db.give_points(guild, target.id.get(), c.amount).await?;
                let embed = EmbedBuilder::new()
                    .color(SUCCESS)
                    .description(t!(
                        lang,
                        "points.gave",
                        amount = thousands_in(lang, c.amount),
                        emoji = emoji,
                        name = name,
                        user = mention(target.id),
                        balance = thousands_in(lang, balance)
                    ))
                    .build();
                cx.reply_embed(embed).await
            }
            Self::Take(c) => {
                if !cx.can_manage_guild() {
                    return cx.reply_ephemeral(t!(lang, "points.take-denied")).await;
                }
                let target = &c.user.resolved;
                let (taken, balance) = cx.app.db.take_points(guild, target.id.get(), c.amount).await?;
                if taken == 0 {
                    return cx
                        .reply_ephemeral(t!(
                            lang,
                            "points.nothing-to-take",
                            user = mention(target.id),
                            name = name
                        ))
                        .await;
                }
                let embed = EmbedBuilder::new()
                    .color(SUCCESS)
                    .description(t!(
                        lang,
                        "points.took",
                        amount = thousands_in(lang, taken),
                        emoji = emoji,
                        name = name,
                        user = mention(target.id),
                        balance = thousands_in(lang, balance)
                    ))
                    .build();
                cx.reply_embed(embed).await
            }
        }
    }
}
