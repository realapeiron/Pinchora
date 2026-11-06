use pinchora_core::settings::{self, FieldError};
use pinchora_db::{Db, GuildSettings};
use twilight_interactions::command::{CommandModel, CreateCommand, ResolvedUser};
use twilight_util::builder::embed::{EmbedBuilder, EmbedFooterBuilder};

use crate::{
    Cx,
    util::{BRAND, SUCCESS, mention, resolved_display_name, thousands},
};

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
    /// Stores what was given. `Err` carries the reason when the result would make no sense; the
    /// rules are the dashboard's too (`pinchora_core::settings`).
    pub async fn apply(self, db: &Db, guild: u64) -> anyhow::Result<Result<GuildSettings, String>> {
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
            Err(error) => return Ok(Err(error.message)),
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

/// "points" -> "Points", for titles.
pub(crate) fn capitalized(name: &str) -> String {
    let mut chars = name.chars();
    chars
        .next()
        .map_or_else(String::new, |c| c.to_uppercase().chain(chars).collect())
}

impl PointsCommand {
    pub async fn run(self, cx: &Cx) -> anyhow::Result<()> {
        let Some(guild) = cx.guild_id() else {
            return cx.reply_ephemeral("Points only exist inside a server.").await;
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
                    return cx.reply_ephemeral(format!("Bots don't earn {name}.")).await;
                }
                if super::privacy::private_profile(cx, user.id.get()).await? {
                    return Ok(());
                }
                let (points, rank) = cx.app.db.member_points(guild, user.id.get()).await?;
                let footer = match (rank, settings.points_enabled) {
                    (Some(rank), _) => format!("Rank #{rank} on this server"),
                    (None, true) => format!("Chat to earn {name}"),
                    (None, false) => format!("Chatting doesn't earn {name} on this server"),
                };
                let embed = EmbedBuilder::new()
                    .color(BRAND)
                    .title(format!("{emoji} {}", capitalized(name)))
                    .description(format!(
                        "{} ({display}) has **{}** {emoji} {name}.",
                        mention(user.id),
                        thousands(points)
                    ))
                    .footer(EmbedFooterBuilder::new(footer))
                    .build();
                cx.reply_embed(embed).await
            }
            Self::Give(c) => {
                if !cx.can_manage_guild() {
                    return cx
                        .reply_ephemeral("You need the Manage Server permission to give points.")
                        .await;
                }
                let target = &c.user.resolved;
                if target.bot {
                    return cx.reply_ephemeral(format!("Bots don't earn {name}.")).await;
                }
                let balance = cx.app.db.give_points(guild, target.id.get(), c.amount).await?;
                let embed = EmbedBuilder::new()
                    .color(SUCCESS)
                    .description(format!(
                        "Gave **{}** {emoji} {name} to {}. They have **{}** now.",
                        thousands(c.amount),
                        mention(target.id),
                        thousands(balance)
                    ))
                    .build();
                cx.reply_embed(embed).await
            }
            Self::Take(c) => {
                if !cx.can_manage_guild() {
                    return cx
                        .reply_ephemeral("You need the Manage Server permission to take points.")
                        .await;
                }
                let target = &c.user.resolved;
                let (taken, balance) = cx.app.db.take_points(guild, target.id.get(), c.amount).await?;
                if taken == 0 {
                    return cx
                        .reply_ephemeral(format!("{} has no {name} to take.", mention(target.id)))
                        .await;
                }
                let embed = EmbedBuilder::new()
                    .color(SUCCESS)
                    .description(format!(
                        "Took **{}** {emoji} {name} from {}. They have **{}** left.",
                        thousands(taken),
                        mention(target.id),
                        thousands(balance)
                    ))
                    .build();
                cx.reply_embed(embed).await
            }
        }
    }
}
