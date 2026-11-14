//! Invite attribution, member counts, temporary links and the `/invites` commands.

mod leaderboard;
use leaderboard::*;
mod snapshot;
pub use snapshot::*;
mod rewards;
pub use rewards::*;
#[cfg(test)]
mod tests;


use std::{sync::Arc, time::Duration};

use chrono::{TimeZone, Utc};
use pinchora_core::admin::modules::Module;
use pinchora_core::admin::server_stats::grouped_in;
use pinchora_core::admin::settings;
use pinchora_core::admin::settings::RoleFacts;
use pinchora_core::t;
use pinchora_db::{GuildInvite, InviteObservation, InviteScore};
use twilight_interactions::command::{CommandModel, CommandOption, CreateCommand, CreateOption, ResolvedUser};
use twilight_model::{
    channel::message::{
        Component,
        component::{ActionRow, Button, ButtonStyle},
    },
    gateway::payload::incoming::{MemberAdd, MemberRemove},
    guild::{Permissions, Role},
    http::interaction::InteractionResponseData,
    id::Id,
};
use twilight_util::builder::embed::EmbedBuilder;

use crate::App;
use crate::Cx;
use crate::infra::text::access::channel_permissions;
use crate::infra::util::BRAND;
use crate::roles::roles;

#[derive(CommandModel, CreateCommand)]
#[command(
    name = "invites",
    desc = "Your invites and this server's invite leaderboard",
    contexts = "guild"
)]
#[allow(clippy::large_enum_variant)] // a resolved user is parsed once and not stored
pub enum InvitesCommand {
    #[command(name = "user")]
    User(InviteUser),
    #[command(name = "top")]
    Top(InviteTop),
    #[command(name = "temp")]
    Temp(InviteTemp),
}

#[derive(CommandModel, CreateCommand)]
#[command(name = "user", desc = "See a member's invite card")]
pub struct InviteUser {
    /// Whose invitations to show
    user: Option<ResolvedUser>,
}

#[derive(CommandOption, CreateOption, Clone, Copy)]
pub enum Span {
    #[option(name = "All time", value = "all")]
    All,
    #[option(name = "Last 7 days", value = "week")]
    Week,
    #[option(name = "Last 30 days", value = "month")]
    Month,
}

#[derive(CommandModel, CreateCommand)]
#[command(name = "top", desc = "See the invitation leaderboard")]
pub struct InviteTop {
    /// Time period
    period: Option<Span>,
}

#[derive(CommandModel, CreateCommand)]
#[command(name = "temp", desc = "Get a short-lived invite link for a friend")]
pub struct InviteTemp;

fn manage_guild() -> Permissions {
    Permissions::MANAGE_GUILD
}

#[derive(CommandModel, CreateCommand)]
#[command(
    name = "inviteadmin",
    desc = "Manage invite tracking, rewards and friend links",
    default_permissions = "manage_guild",
    contexts = "guild"
)]
pub enum InviteAdmin {
    #[command(name = "settings")]
    Settings(AdminSettings),
    #[command(name = "bonus-add")]
    BonusAdd(AdminBonus),
    #[command(name = "bonus-remove")]
    BonusRemove(AdminBonusRemove),
    #[command(name = "reward-add")]
    RewardAdd(AdminReward),
    #[command(name = "reward-remove")]
    RewardRemove(AdminRewardRemove),
    #[command(name = "post")]
    Post(AdminPost),
}

#[derive(CommandModel, CreateCommand)]
#[command(name = "settings", desc = "Set the tracking and friend-link options")]
pub struct AdminSettings {
    /// Count new joins
    enabled: Option<bool>,
    /// Account age below this is suspicious
    fake_days: Option<i64>,
    /// Channel for join logs
    log_channel: Option<Id<twilight_model::id::marker::ChannelMarker>>,
    /// Let members create friend links
    temp_enabled: Option<bool>,
    /// Channel friend links lead to
    temp_channel: Option<Id<twilight_model::id::marker::ChannelMarker>>,
    /// Daily links allowed per member
    temp_daily: Option<i64>,
    /// Friend link lifetime in seconds
    temp_age: Option<i64>,
    /// Maximum uses for each friend link
    temp_uses: Option<i64>,
}

#[derive(CommandModel, CreateCommand)]
#[command(name = "bonus-add", desc = "Add invitations to a member's score")]
pub struct AdminBonus {
    /// Member
    user: ResolvedUser,
    /// Amount
    amount: i64,
}

#[derive(CommandModel, CreateCommand)]
#[command(name = "bonus-remove", desc = "Remove a bonus from a member's score")]
pub struct AdminBonusRemove {
    /// Member
    user: ResolvedUser,
    /// Amount
    amount: i64,
}

#[derive(CommandModel, CreateCommand)]
#[command(name = "reward-add", desc = "Give a role at an invite milestone")]
pub struct AdminReward {
    /// Invite threshold
    invites: i64,
    /// Role to give
    role: Role,
}

#[derive(CommandModel, CreateCommand)]
#[command(name = "reward-remove", desc = "Remove an invite reward")]
pub struct AdminRewardRemove {
    /// Invite threshold
    invites: i64,
}

#[derive(CommandModel, CreateCommand)]
#[command(name = "post", desc = "Post an Invite a friend button in this channel")]
pub struct AdminPost;

impl InvitesCommand {
    pub async fn run(self, cx: &Cx) -> anyhow::Result<()> {
        let Some(guild) = cx.guild_id() else {
            return cx.reply_ephemeral(t!(cx.lang(), "invites.guild-only")).await;
        };
        match self {
            Self::User(command) => {
                let (user, name) = match command.user {
                    Some(u) => (u.resolved.id.get(), crate::infra::util::resolved_display_name(&u)),
                    None => (cx.author()?.id.get(), cx.author_display_name()),
                };
                if crate::commands::admin::privacy::private_profile(cx, user).await? {
                    return Ok(());
                }
                let score = cx.app.db.invite_score(guild, user).await?;
                let name = name.chars().take(24).collect::<String>();
                let title = t!(cx.lang(), "invites.card.title", user = name);
                let png = card_png(&cx.app, &title, &score, cx.lang()).await?;
                cx.reply_png("invites.png", png).await
            }
            Self::Top(command) => top(cx, guild, command.period.unwrap_or(Span::All), 0).await,
            Self::Temp(_) => create_temp(cx, guild).await,
        }
    }
}

impl InviteAdmin {
    pub async fn run(self, cx: &Cx) -> anyhow::Result<()> {
        let Some(guild) = cx.guild_id() else {
            return cx.reply_ephemeral(t!(cx.lang(), "invites.guild-only")).await;
        };
        if !cx.can_manage_guild() {
            return cx.reply_ephemeral(t!(cx.lang(), "invites.manage-needed")).await;
        }
        match self {
            Self::Settings(change) => {
                let mut saved = cx.app.db.invite_settings(guild).await?;
                if let Some(enabled) = change.enabled {
                    saved.enabled = enabled;
                }
                if let Some(n) = change.fake_days {
                    if !(0..=365).contains(&n) {
                        return cx.reply_ephemeral(t!(cx.lang(), "invites.days-range")).await;
                    }
                    saved.fake_days = n as i32;
                }
                if let Some(channel) = change.log_channel {
                    saved.log_channel_id = Some(channel.get() as i64);
                }
                if let Some(enabled) = change.temp_enabled {
                    saved.temp_enabled = enabled;
                }
                if let Some(channel) = change.temp_channel {
                    saved.temp_channel_id = Some(channel.get() as i64);
                }
                if let Some(n) = change.temp_daily {
                    if !(1..=20).contains(&n) {
                        return cx.reply_ephemeral(t!(cx.lang(), "invites.quota-range")).await;
                    }
                    saved.temp_per_user_daily = n as i32;
                }
                if let Some(n) = change.temp_age {
                    if !(60..=604800).contains(&n) {
                        return cx.reply_ephemeral(t!(cx.lang(), "invites.web.age-range")).await;
                    }
                    saved.temp_max_age_secs = n as i32;
                }
                if let Some(n) = change.temp_uses {
                    if !(1..=100).contains(&n) {
                        return cx.reply_ephemeral(t!(cx.lang(), "invites.web.uses-range")).await;
                    }
                    saved.temp_max_uses = n as i32;
                }
                cx.app.db.save_invite_settings(guild, &saved).await?;
                cx.reply_ephemeral(t!(cx.lang(), "invites.saved")).await
            }
            Self::BonusAdd(c) => bonus(cx, guild, c.user.resolved.id.get(), c.amount, false).await,
            Self::BonusRemove(c) => bonus(cx, guild, c.user.resolved.id.get(), c.amount, true).await,
            Self::RewardAdd(c) => {
                if !(1..=1_000_000).contains(&c.invites) {
                    return cx.reply_ephemeral(t!(cx.lang(), "invites.reward-range")).await;
                }
                let rewards = cx.app.db.invite_rewards(guild).await?;
                if rewards.len() >= 20 && !rewards.iter().any(|(n, _)| *n == c.invites as i32) {
                    return cx.reply_ephemeral(t!(cx.lang(), "invites.reward-limit")).await;
                }
                if let Some(why) = roles::unassignable(cx, guild, &c.role).await {
                    return cx.reply_ephemeral(why).await;
                }
                cx.app
                    .db
                    .put_invite_reward_by(guild, c.invites as i32, c.role.id.get(), Some(cx.author()?.id.get()))
                    .await?;
                reconcile_all_invite_rewards(&cx.app, guild).await?;
                cx.reply_ephemeral(t!(cx.lang(), "invites.saved")).await
            }
            Self::RewardRemove(c) => {
                cx.app.db.remove_invite_reward(guild, c.invites as i32).await?;
                reconcile_all_invite_rewards(&cx.app, guild).await?;
                cx.reply_ephemeral(t!(cx.lang(), "invites.saved")).await
            }
            Self::Post(_) => {
                let button = Button {
                    custom_id: Some("tl:new".to_owned()),
                    disabled: false,
                    emoji: None,
                    id: None,
                    label: Some(t!(cx.lang(), "invites.friend-button")),
                    style: ButtonStyle::Primary,
                    url: None,
                    sku_id: None,
                };
                let message = cx
                    .app
                    .http
                    .create_message(Id::new(cx.interaction.channel.as_ref().unwrap().id.get()))
                    .content(&t!(cx.lang(), "invites.friend-post"))
                    .components(&[Component::ActionRow(ActionRow {
                        id: None,
                        components: vec![Component::Button(button)],
                    })])
                    .await?
                    .model()
                    .await?;
                let mut saved = cx.app.db.invite_settings(guild).await?;
                saved.temp_message_id = Some(message.id.get() as i64);
                cx.app.db.save_invite_settings(guild, &saved).await?;
                cx.reply_ephemeral(t!(cx.lang(), "invites.posted")).await
            }
        }
    }
}

pub(crate) async fn on_component(cx: &Cx, mut parts: std::str::Split<'_, char>) -> anyhow::Result<()> {
    let _kind = parts.next();
    let page = parts.next().and_then(|p| p.parse::<u64>().ok()).unwrap_or(0).min(999);
    let span = match parts.next() {
        Some("week") => Span::Week,
        Some("month") => Span::Month,
        _ => Span::All,
    };
    let Some(guild) = cx.guild_id() else {
        return cx.reply_ephemeral(t!(cx.lang(), "invites.guild-only")).await;
    };
    top(cx, guild, span, page).await
}

pub(crate) async fn on_temp_component(cx: &Cx, mut parts: std::str::Split<'_, char>) -> anyhow::Result<()> {
    if parts.next() != Some("new") || parts.next().is_some() {
        return cx.reply_ephemeral(t!(cx.lang(), "invites.temp.invalid-button")).await;
    }
    let Some(guild) = cx.guild_id() else {
        return cx.reply_ephemeral(t!(cx.lang(), "invites.guild-only")).await;
    };
    create_temp(cx, guild).await
}

async fn create_temp(cx: &Cx, guild: u64) -> anyhow::Result<()> {
    let saved = cx.app.db.invite_settings(guild).await?;
    if !saved.temp_enabled {
        return cx.reply_ephemeral(t!(cx.lang(), "invites.temp.off")).await;
    }
    let Some(channel) = saved.temp_channel_id else {
        return cx.reply_ephemeral(t!(cx.lang(), "invites.temp.no-channel")).await;
    };
    if !can_create_invite(&cx.app, guild, channel as u64, cx.author()?.id.get())
        .await
        .unwrap_or(false)
    {
        return cx.reply_ephemeral(t!(cx.lang(), "invites.temp.permission")).await;
    }
    let user = cx.author()?.id.get();
    let date = Utc::now().format("%Y%m%d");
    let key = format!("invites:quota:{guild}:{user}:{date}");
    let used = cx.app.cache.incr(&key, Duration::from_secs(2 * 86400)).await?;
    if used > i64::from(saved.temp_per_user_daily) {
        let _ = cx.app.cache.incr_by(&key, -1, Duration::from_secs(2 * 86400)).await;
        return cx
            .reply_ephemeral(t!(cx.lang(), "invites.temp.quota", limit = saved.temp_per_user_daily))
            .await;
    }
    let response = cx
        .app
        .http
        .create_invite(Id::new(channel as u64))
        .max_age(saved.temp_max_age_secs as u32)
        .max_uses(saved.temp_max_uses as u16)
        .unique(true)
        .await;
    match response {
        Ok(response) => {
            let invite = match response.model().await {
                Ok(invite) => invite,
                Err(error) => {
                    let _ = cx.app.cache.incr_by(&key, -1, Duration::from_secs(2 * 86400)).await;
                    return Err(error.into());
                }
            };
            cx.reply_ephemeral(t!(
                cx.lang(),
                "invites.temp.link",
                link = format!("https://discord.gg/{}", invite.code)
            ))
            .await
        }
        Err(error) => {
            let _ = cx.app.cache.incr_by(&key, -1, Duration::from_secs(2 * 86400)).await;
            tracing::warn!(guild, %error, "temporary invite refused");
            cx.reply_ephemeral(t!(cx.lang(), "invites.temp.permission")).await
        }
    }
}

async fn can_create_invite(app: &App, guild: u64, channel: u64, user: u64) -> anyhow::Result<bool> {
    let facts = app.http.guild(Id::new(guild)).await?.model().await?;
    let roles = app.http.roles(Id::new(guild)).await?.models().await?;
    let target = app.http.channel(Id::new(channel)).await?.model().await?;
    if target.guild_id.map(|id| id.get()) != Some(guild) || target.kind.is_thread() {
        return Ok(false);
    }
    let overwrites = target.permission_overwrites.unwrap_or_default();
    let bot = app.application_id.get();
    for member in [user, bot] {
        let held = app
            .http
            .guild_member(Id::new(guild), Id::new(member))
            .await?
            .model()
            .await?
            .roles
            .iter()
            .map(|role| role.get())
            .collect::<Vec<_>>();
        let base = if facts.owner_id.get() == member {
            Permissions::all()
        } else {
            roles
                .iter()
                .filter(|role| role.id.get() == guild || held.contains(&role.id.get()))
                .fold(Permissions::empty(), |permissions, role| permissions | role.permissions)
        };
        let here = channel_permissions(base, guild, member, &held, &overwrites);
        if !here.contains(Permissions::VIEW_CHANNEL | Permissions::CREATE_INVITE) {
            return Ok(false);
        }
    }
    Ok(true)
}

/// A Discord snowflake records account creation in milliseconds since Discord's epoch.
fn is_fake_account(user: u64, days: i32, now: chrono::DateTime<Utc>) -> bool {
    let created_ms = ((user >> 22) as i64) + 1_420_070_400_000;
    now.timestamp_millis() - created_ms < i64::from(days) * 86_400_000
}

fn escape_svg(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
