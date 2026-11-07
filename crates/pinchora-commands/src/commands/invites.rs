//! Invite attribution, member counts, temporary links and the `/invites` commands.

use std::{sync::Arc, time::Duration};

use chrono::{TimeZone, Utc};
use pinchora_core::{
    modules::Module,
    server_stats::grouped_in,
    settings::{self, RoleFacts},
    t,
};
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

use crate::{App, Cx, roles, text::access::channel_permissions, util::BRAND};

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
                    Some(u) => (u.resolved.id.get(), crate::util::resolved_display_name(&u)),
                    None => (cx.author()?.id.get(), cx.author_display_name()),
                };
                if super::privacy::private_profile(cx, user).await? {
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

async fn bonus(cx: &Cx, guild: u64, user: u64, amount: i64, remove: bool) -> anyhow::Result<()> {
    if !(1..=1000).contains(&amount) {
        return cx.reply_ephemeral(t!(cx.lang(), "invites.bonus-range")).await;
    }
    let by = if remove { -amount } else { amount };
    let total = cx.app.db.change_invite_bonus(guild, user, by as i32).await?;
    reconcile_invite_rewards(&cx.app, guild, user).await?;
    cx.reply_ephemeral(t!(
        cx.lang(),
        "invites.bonus-saved",
        total = grouped_in(cx.lang(), i64::from(total))
    ))
    .await
}

async fn card_png(
    app: &App,
    title: &str,
    score: &InviteScore,
    lang: pinchora_core::i18n::Lang,
) -> anyhow::Result<Vec<u8>> {
    let title = title.to_owned();
    let labels = [
        t!(lang, "invites.card.regular"),
        t!(lang, "invites.card.left"),
        t!(lang, "invites.card.fake"),
        t!(lang, "invites.card.bonus"),
    ];
    let values = [score.regular, score.left, score.fake, i64::from(score.bonus)];
    let total = t!(lang, "invites.card.total");
    let score = score.clone();
    app.render(move |r| {
        let mut svg = String::from("<svg xmlns='http://www.w3.org/2000/svg' width='900' height='420' viewBox='0 0 900 420'><rect width='900' height='420' fill='#101936'/><rect x='16' y='16' width='868' height='388' rx='12' fill='#172a53' stroke='#68aeea' stroke-width='4'/><rect x='38' y='38' width='824' height='72' fill='#233d70'/>");
        use std::fmt::Write;
        let _ = write!(svg, "<text x='58' y='84' font-size='32' font-family='Silkscreen' font-weight='bold' fill='#fff'>{}</text>", escape_svg(&title));
        for (i, (label, value)) in labels.iter().zip(values).enumerate() {
            let x = 40 + i * 210;
            let value = grouped_in(lang, value);
            let _ = write!(svg, "<rect x='{x}' y='134' width='194' height='140' fill='#203a6c' stroke='#426ca3' stroke-width='2'/><text x='{}' y='180' font-family='Silkscreen' font-size='20' fill='#accff2'>{}</text><text x='{}' y='244' font-family='Silkscreen' font-size='44' font-weight='bold' fill='#fff'>{value}</text>", x+16, escape_svg(label), x+16);
        }
        let _ = write!(svg, "<rect x='40' y='298' width='820' height='80' fill='#0d9274'/><text x='64' y='350' font-family='Silkscreen' font-size='26' font-weight='bold' fill='#fff'>{}</text><text x='826' y='350' text-anchor='end' font-family='Silkscreen' font-size='38' font-weight='bold' fill='#fff'>{}</text></svg>", escape_svg(&total), grouped_in(lang, score.total));
        r.svg_to_png(&svg)
    }).await
}

async fn top(cx: &Cx, guild: u64, span: Span, page: u64) -> anyhow::Result<()> {
    let since = match span {
        Span::All => None,
        Span::Week => Some(Utc::now() - chrono::Duration::days(7)),
        Span::Month => Some(Utc::now() - chrono::Duration::days(30)),
    };
    let leaders = cx.app.db.invite_leaders(guild, since, 10, (page * 10) as i64).await?;
    let mut lines = Vec::new();
    for (i, row) in leaders.iter().enumerate() {
        lines.push(format!(
            "**{}.** <@{}> · **{}**",
            page * 10 + i as u64 + 1,
            row.user_id,
            grouped_in(cx.lang(), row.total)
        ));
    }
    let description = if lines.is_empty() {
        t!(cx.lang(), "invites.top.empty")
    } else {
        lines.join("\n")
    };
    let embed = EmbedBuilder::new()
        .color(BRAND)
        .title(t!(cx.lang(), "invites.top.title"))
        .description(description)
        .build();
    let buttons = [
        ("prev", page.saturating_sub(1), page == 0),
        ("next", page + 1, leaders.len() < 10),
    ]
    .into_iter()
    .map(|(kind, n, disabled)| {
        Component::Button(Button {
            custom_id: Some(format!("in:{kind}:{n}:{}", span.key())),
            disabled,
            emoji: None,
            id: None,
            label: Some(t!(
                cx.lang(),
                if kind == "next" {
                    "invites.next"
                } else {
                    "invites.previous"
                }
            )),
            style: ButtonStyle::Secondary,
            url: None,
            sku_id: None,
        })
    })
    .collect();
    cx.reply(InteractionResponseData {
        embeds: Some(vec![embed]),
        components: Some(vec![Component::ActionRow(ActionRow {
            id: None,
            components: buttons,
        })]),
        ..Default::default()
    })
    .await
}

impl Span {
    fn key(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::Week => "week",
            Self::Month => "month",
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

pub async fn refresh_snapshot(app: &Arc<App>, guild: u64) -> anyhow::Result<()> {
    let invites = fetch_invites(app, guild).await?;
    remember_vanity(app, guild).await;
    app.db.reset_invite_baseline(guild, &invites).await?;
    write_snapshot(app, guild, &invites).await
}

pub async fn refresh_created_snapshot(app: &Arc<App>, guild: u64) -> anyhow::Result<()> {
    let invites = fetch_invites(app, guild).await?;
    app.db.baseline_invites(guild, &invites).await?;
    write_snapshot(app, guild, &invites).await
}

pub async fn refresh_deleted_snapshot(app: &Arc<App>, guild: u64) -> anyhow::Result<()> {
    let invites = fetch_invites(app, guild).await?;
    write_snapshot(app, guild, &invites).await
}

async fn fetch_invites(app: &App, guild: u64) -> anyhow::Result<Vec<GuildInvite>> {
    let found = app.http.guild_invites(Id::new(guild)).await?.models().await?;
    Ok(found
        .into_iter()
        .map(|i| GuildInvite {
            code: i.code,
            inviter_id: i.inviter.map(|u| u.id.get() as i64),
            uses: i.uses.unwrap_or(0).min(i32::MAX as u64) as i32,
            max_uses: i.max_uses.unwrap_or(0).min(i32::MAX as u64) as i32,
            max_age: i.max_age.unwrap_or(0).min(i32::MAX as u64) as i32,
            created_at: i
                .created_at
                .and_then(|t| Utc.timestamp_opt(t.as_secs(), 0).single())
                .unwrap_or_else(Utc::now),
        })
        .collect())
}

async fn remember_vanity(app: &App, guild: u64) {
    if let Ok(response) = app.http.guild_vanity_url(Id::new(guild)).await
        && let Ok(vanity) = response.model().await
    {
        let _ = app
            .cache
            .put(
                &format!("invites:vanity:{guild}"),
                &vanity.uses.to_string(),
                Duration::from_secs(86400),
            )
            .await;
    }
}

async fn vanity_advanced(app: &App, guild: u64) -> bool {
    let key = format!("invites:vanity:{guild}");
    let Ok(response) = app.http.guild_vanity_url(Id::new(guild)).await else {
        return false;
    };
    let Ok(vanity) = response.model().await else {
        return false;
    };
    app.cache
        .claim_observed_value(&key, vanity.uses, Duration::from_secs(86400))
        .await
        .unwrap_or(false)
}

async fn write_snapshot(app: &App, guild: u64, invites: &[GuildInvite]) -> anyhow::Result<()> {
    let rows = invites
        .iter()
        .map(|i| (i.code.clone(), i.uses.to_string()))
        .collect::<Vec<_>>();
    app.cache
        .map_replace(&format!("invites:snapshot:{guild}"), &rows, Duration::from_secs(86400))
        .await
}

pub(crate) async fn on_member_join(app: &Arc<App>, event: &MemberAdd) {
    let app = app.clone();
    let event = event.clone();
    app.clone().spawn_event("invite_join", async move {
        if let Err(error) = track_join(&app, &event).await {
            tracing::warn!(guild=event.guild_id.get(), %error, "invite join tracking failed");
        }
    });
}

async fn track_join(app: &Arc<App>, event: &MemberAdd) -> anyhow::Result<()> {
    let guild = event.guild_id.get();
    let saved = app.db.invite_settings(guild).await?;
    if event.user.bot {
        return Ok(());
    }
    if !saved.enabled || !app.module_allowed(guild, None, Module::Invites).await {
        app.db.require_invite_baseline(guild).await?;
        return Ok(());
    }
    let now = event
        .joined_at
        .and_then(|joined| Utc.timestamp_opt(joined.as_secs(), 0).single())
        .unwrap_or_else(Utc::now);
    let fake = is_fake_account(event.user.id.get(), saved.fake_days, now);
    let mut rebaselined = false;
    if saved.baseline_required && !app.invite_baseline_pending(guild) {
        let invites = fetch_invites(app, guild).await?;
        rebaselined = app.db.reset_required_invite_baseline(guild, &invites).await?;
        if rebaselined {
            remember_vanity(app, guild).await;
            write_snapshot(app, guild, &invites).await?;
        }
    }
    let observed = if rebaselined || app.invite_baseline_pending(guild) {
        None
    } else {
        match fetch_invites(app, guild).await {
            Ok(v) => Some(v),
            Err(error) => {
                tracing::warn!(guild, %error, "Manage Server permission is required to read invites");
                None
            }
        }
    };
    let vanity_changed = if observed.is_some() {
        vanity_advanced(app, guild).await
    } else {
        false
    };
    let credit = app
        .db
        .record_invite_join(
            guild,
            event.user.id.get(),
            now,
            fake,
            InviteObservation {
                invites: observed.as_deref().unwrap_or(&[]),
                vanity_changed,
                list_available: observed.is_some(),
                preferred_code: None,
            },
        )
        .await?;
    if observed.is_some() {
        let credited = app.db.invite_snapshot(guild).await?;
        write_snapshot(app, guild, &credited).await?;
    }
    if let Some(channel) = saved.log_channel_id {
        let lang = app.guild_lang(guild).await;
        let source = match credit.source {
            "invite" => t!(lang, "invites.source.invite"),
            "vanity" => t!(lang, "invites.source.vanity"),
            "temporary" => t!(lang, "invites.source.temporary"),
            "widget" => t!(lang, "invites.source.widget"),
            _ => t!(lang, "invites.source.unknown"),
        };
        let content = t!(lang, "invites.join-log", user = event.user.id.get(), source = source);
        let _ = app.http.create_message(Id::new(channel as u64)).content(&content).await;
    }
    if let Some(inviter) = credit.inviter_id
        && let Err(error) = reconcile_invite_rewards(app, guild, inviter).await
    {
        tracing::warn!(guild, inviter, %error, "invite reward reconciliation failed");
    }
    Ok(())
}

/// Re-check the role before Discord grants an invitation milestone.
pub async fn grant_invite_reward(app: &App, guild: u64, user: u64, role: u64) -> anyhow::Result<()> {
    let roles = app.http.roles(Id::new(guild)).await?.models().await?;
    let found = roles
        .iter()
        .find(|r| r.id.get() == role)
        .ok_or_else(|| anyhow::anyhow!("role deleted"))?;
    let mine = app
        .http
        .guild_member(Id::new(guild), Id::new(app.application_id.get()))
        .await?
        .model()
        .await?;
    let top = roles
        .iter()
        .filter(|r| mine.roles.contains(&r.id))
        .map(|r| r.position)
        .max()
        .unwrap_or(0);
    let facts = RoleFacts {
        everyone: role == guild,
        managed: found.managed,
        position: found.position,
        permissions: found.permissions.bits(),
    };
    if let Some(why) = settings::role_refusal(facts, Some(top), &format!("<@&{role}>"))
        .or_else(|| settings::run_time_refusal(found.permissions.bits(), &format!("<@&{role}>")))
    {
        anyhow::bail!("{why}");
    }
    app.http
        .add_guild_member_role(Id::new(guild), Id::new(user), Id::new(role))
        .await?;
    Ok(())
}

async fn grantor_can_grant(app: &App, guild: u64, role: &Role, authority: Option<i64>) -> anyhow::Result<bool> {
    let Some(authority) = authority else { return Ok(false) };
    let guild_data = app.http.guild(Id::new(guild)).await?.model().await?;
    let roles = app.http.roles(Id::new(guild)).await?.models().await?;
    let Some(role) = roles.iter().find(|current| current.id == role.id) else {
        return Ok(false);
    };
    let facts = RoleFacts {
        everyone: role.id.get() == guild,
        managed: role.managed,
        position: role.position,
        permissions: role.permissions.bits(),
    };
    let owner = guild_data.owner_id.get() == authority as u64;
    let member = match app.http.guild_member(Id::new(guild), Id::new(authority as u64)).await {
        Ok(response) => response.model().await.ok(),
        Err(_) => None,
    };
    let Some(member) = member else { return Ok(false) };
    let permissions = roles
        .iter()
        .filter(|r| r.id.get() == guild || member.roles.contains(&r.id))
        .fold(Permissions::empty(), |bits, r| bits | r.permissions);
    if !owner && !permissions.intersects(Permissions::MANAGE_GUILD | Permissions::ADMINISTRATOR) {
        return Ok(false);
    }
    let grantor = settings::Grantor {
        owner,
        administrator: permissions.contains(Permissions::ADMINISTRATOR),
        top_role: Some(
            roles
                .iter()
                .filter(|r| member.roles.contains(&r.id))
                .map(|r| r.position)
                .max()
                .unwrap_or(0),
        ),
    };
    Ok(settings::grant_refusal(facts, grantor, &format!("<@&{}>", role.id)).is_none())
}

async fn reconcile_invite_rewards(app: &App, guild: u64, user: u64) -> anyhow::Result<()> {
    let guard = app.db.invite_reward_guard(guild, user).await?;
    let score = app.db.invite_score(guild, user).await?.total;
    let rewards = app.db.invite_rewards_with_grantor(guild).await?;
    let granted = app.db.invite_granted_roles(guild, user).await?;
    let member = app
        .http
        .guild_member(Id::new(guild), Id::new(user))
        .await?
        .model()
        .await?;
    let roles = app.http.roles(Id::new(guild)).await?.models().await?;
    let mut desired = std::collections::HashSet::new();
    for (threshold, role, authority) in &rewards {
        if score < i64::from(*threshold) {
            continue;
        }
        let Some(found) = roles.iter().find(|r| r.id.get() == *role as u64) else {
            continue;
        };
        if settings::run_time_refusal(found.permissions.bits(), &format!("<@&{role}>")).is_none()
            && grantor_can_grant(app, guild, found, *authority).await?
        {
            desired.insert(*role as u64);
        }
    }
    for role in granted {
        if !desired.contains(&role) {
            app.http
                .remove_guild_member_role(Id::new(guild), Id::new(user), Id::new(role))
                .await?;
            app.db.mark_invite_role(guild, user, role, false).await?;
        }
    }
    for role in desired {
        if member.roles.iter().any(|held| held.get() == role) {
            continue;
        }
        // Re-read the score before the remote write; a concurrent leave can lower it.
        let current_score = app.db.invite_score(guild, user).await?.total;
        let Some(found) = roles.iter().find(|r| r.id.get() == role) else {
            continue;
        };
        let mut authorized = false;
        for (_, _, authority) in rewards
            .iter()
            .filter(|(threshold, id, _)| *id as u64 == role && current_score >= i64::from(*threshold))
        {
            if grantor_can_grant(app, guild, found, *authority).await? {
                authorized = true;
                break;
            }
        }
        if !authorized {
            continue;
        }
        // Record ownership first so a crash after Discord accepts the role cannot
        // leave an untracked reward that future score drops fail to revoke.
        app.db.mark_invite_role(guild, user, role, true).await?;
        if let Err(error) = grant_invite_reward(app, guild, user, role).await {
            tracing::warn!(guild, user, role, %error, "invite reward refused");
            continue;
        }
    }
    guard.commit().await?;
    Ok(())
}

pub async fn reconcile_all_invite_rewards(app: &App, guild: u64) -> anyhow::Result<()> {
    for user in app.db.invite_reward_candidates(guild).await? {
        if let Err(error) = reconcile_invite_rewards(app, guild, user).await {
            tracing::warn!(guild, user, %error, "invite reward reconciliation failed");
        }
    }
    Ok(())
}

/// Repairs failed Discord grants and revokes roles after score, reward, or role changes.
pub fn spawn_reward_ticker(app: Arc<App>) {
    app.clone().spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(300));
        loop {
            interval.tick().await;
            match app.db.invite_reward_guilds().await {
                Ok(guilds) => {
                    for guild in guilds {
                        let key = format!("invites:reward-reconcile:{guild}");
                        if app.cache.once(&key, Duration::from_secs(240)).await.unwrap_or(false)
                            && let Err(error) = reconcile_all_invite_rewards(&app, guild).await
                        {
                            tracing::warn!(guild, %error, "invite reward sweep failed");
                        }
                    }
                }
                Err(error) => tracing::warn!(%error, "invite reward guild lookup failed"),
            }
        }
    });
}

pub(crate) async fn on_member_leave(app: &Arc<App>, event: &MemberRemove) {
    let app = app.clone();
    let guild = event.guild_id.get();
    let user = event.user.id.get();
    app.clone().spawn_event("invite_leave", async move {
        let inviter = app.db.invite_inviter_for_member(guild, user).await.ok().flatten();
        if let Err(error) = app.db.leave_invite(guild, user).await {
            tracing::warn!(guild, %error, "invite leave tracking failed");
            return;
        }
        if let Some(inviter) = inviter
            && let Err(error) = reconcile_invite_rewards(&app, guild, inviter).await
        {
            tracing::warn!(guild, inviter, %error, "invite reward reconciliation failed");
        }
        if let Ok(settings) = app.db.invite_settings(guild).await
            && let Some(channel) = settings.log_channel_id
        {
            let content = t!(app.guild_lang(guild).await, "invites.leave-log", user = user);
            let _ = app.http.create_message(Id::new(channel as u64)).content(&content).await;
        }
    });
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

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn age_boundary_and_zero_window() {
        let now = Utc.timestamp_millis_opt(1_700_000_000_000).single().unwrap();
        let snowflake =
            |age_days: i64| (((now.timestamp_millis() - age_days * 86_400_000 - 1_420_070_400_000) as u64) << 22) | 1;
        assert!(is_fake_account(snowflake(6), 7, now));
        assert!(!is_fake_account(snowflake(7), 7, now));
        assert!(!is_fake_account(snowflake(0), 0, now));
    }
}
