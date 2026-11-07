//! Server achievements, manager commands, progress cards, and the minute evaluation.

use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, LazyLock},
    time::Duration,
};

use moka::future::Cache;
use pinchora_core::{
    achievements::{self, Kind},
    modules::{Access, Module},
    settings::{self, RoleFacts},
    t,
};
use pinchora_db::{AchievementDraft, AchievementRow, AchievementSave};
use pinchora_render::{AchievementCell, AchievementsCard, Theme};
use twilight_http::request::AuditLogReason;
use twilight_interactions::command::{CommandModel, CommandOption, CreateCommand, CreateOption, ResolvedUser};
use twilight_model::{
    channel::ChannelType,
    channel::message::{
        AllowedMentions, Component,
        component::{ActionRow, Button, ButtonStyle},
    },
    guild::{Permissions, Role},
    http::interaction::InteractionResponseData,
    id::{Id, marker::ChannelMarker},
};
use twilight_util::builder::embed::EmbedBuilder;

use crate::{
    App, Cx,
    commands::privacy,
    roles,
    util::{BRAND, resolved_display_name},
};

static RULES: LazyLock<Cache<u64, Arc<Vec<AchievementRow>>>> = LazyLock::new(|| {
    Cache::builder()
        .time_to_live(Duration::from_secs(60))
        .max_capacity(20_000)
        .build()
});

pub async fn invalidate(app: &App, guild: u64) -> anyhow::Result<()> {
    forget(guild).await;
    app.cache
        .publish_invalidation(pinchora_cache::Invalidation::Guild(guild))
        .await?;
    Ok(())
}

pub(crate) async fn forget(guild: u64) {
    RULES.invalidate(&guild).await;
}

fn manage_guild() -> Permissions {
    Permissions::MANAGE_GUILD
}

#[derive(CommandModel, CreateCommand)]
#[command(name = "achievements", desc = "Show server achievements", contexts = "guild")]
pub struct AchievementsCommand {
    /// Whose achievements to show (default: you)
    user: Option<ResolvedUser>,
}

#[derive(CommandOption, CreateOption, Clone, Copy)]
pub enum AchievementKind {
    #[option(name = "Messages", value = "messages")]
    Messages,
    #[option(name = "Voice minutes", value = "voice_minutes")]
    VoiceMinutes,
    #[option(name = "Level", value = "level")]
    Level,
    #[option(name = "Days active", value = "days_active")]
    DaysActive,
    #[option(name = "Invites", value = "invites")]
    Invites,
    #[option(name = "Stars received", value = "stars_received")]
    StarsReceived,
    #[option(name = "Tickets closed", value = "tickets_closed")]
    TicketsClosed,
    #[option(name = "Reputation", value = "reputation")]
    Reputation,
    #[option(name = "Games won", value = "games_won")]
    GamesWon,
    #[option(name = "Fish caught", value = "fish_caught")]
    FishCaught,
    #[option(name = "Custom", value = "custom")]
    Custom,
}

impl AchievementKind {
    fn kind(self) -> Kind {
        Kind::parse(match self {
            Self::Messages => "messages",
            Self::VoiceMinutes => "voice_minutes",
            Self::Level => "level",
            Self::DaysActive => "days_active",
            Self::Invites => "invites",
            Self::StarsReceived => "stars_received",
            Self::TicketsClosed => "tickets_closed",
            Self::Reputation => "reputation",
            Self::GamesWon => "games_won",
            Self::FishCaught => "fish_caught",
            Self::Custom => "custom",
        })
        .expect("command kind exists")
    }
}

#[derive(CommandModel, CreateCommand)]
#[command(
    name = "achievementadmin",
    desc = "Manage server achievements",
    default_permissions = "manage_guild",
    contexts = "guild"
)]
pub enum AchievementAdmin {
    #[command(name = "add")]
    Add(Add),
    #[command(name = "edit")]
    Edit(Edit),
    #[command(name = "remove")]
    Remove(Remove),
    #[command(name = "list")]
    List(List),
    #[command(name = "grant")]
    Grant(Grant),
}

#[derive(CommandModel, CreateCommand)]
#[command(name = "add", desc = "Create an achievement (up to 25)")]
pub struct Add {
    /// Name shown on the card
    #[command(min_length = 1, max_length = 40)]
    name: String,
    /// What earns it
    kind: AchievementKind,
    /// Progress needed
    #[command(min_value = 1, max_value = 1_000_000_000)]
    target: i64,
    /// Short description
    #[command(max_length = 100)]
    description: Option<String>,
    /// Icon name from the dashboard picker
    icon: Option<String>,
    /// Role reward
    role: Option<Role>,
    /// Server points reward
    #[command(min_value = 0, max_value = 1_000_000_000)]
    points: Option<i64>,
    /// Channel to announce grants
    announce_channel: Option<Id<ChannelMarker>>,
    /// Hide until earned
    secret: Option<bool>,
}

#[derive(CommandModel, CreateCommand)]
#[command(name = "edit", desc = "Edit an achievement")]
pub struct Edit {
    /// Achievement ID from list
    id: i64,
    /// New name
    #[command(min_length = 1, max_length = 40)]
    name: Option<String>,
    /// New description
    #[command(max_length = 100)]
    description: Option<String>,
    /// New kind
    kind: Option<AchievementKind>,
    /// New icon
    icon: Option<String>,
    /// New progress target
    #[command(min_value = 1, max_value = 1_000_000_000)]
    target: Option<i64>,
    /// New role reward
    role: Option<Role>,
    /// Remove the current role reward
    clear_role: Option<bool>,
    /// New points reward
    #[command(min_value = 0, max_value = 1_000_000_000)]
    points: Option<i64>,
    /// New announcement channel
    announce_channel: Option<Id<ChannelMarker>>,
    /// Remove the current announcement channel
    clear_announce_channel: Option<bool>,
    /// Hide until earned
    secret: Option<bool>,
    /// Enable or disable
    enabled: Option<bool>,
}

#[derive(CommandModel, CreateCommand)]
#[command(name = "remove", desc = "Remove an achievement and its grants")]
pub struct Remove {
    /// Achievement ID
    id: i64,
}
#[derive(CommandModel, CreateCommand)]
#[command(name = "list", desc = "List configured achievements")]
pub struct List;
#[derive(CommandModel, CreateCommand)]
#[command(name = "grant", desc = "Give a custom achievement to a member")]
pub struct Grant {
    /// Achievement ID
    id: i64,
    /// Member who earns it
    user: ResolvedUser,
}

impl AchievementsCommand {
    pub async fn run(self, cx: &Cx) -> anyhow::Result<()> {
        let Some(guild) = cx.guild_id() else {
            return cx.reply_ephemeral(t!(cx.lang(), "achievements.guild-only")).await;
        };
        if self.user.as_ref().is_some_and(|u| u.resolved.bot) {
            return cx.reply_ephemeral(t!(cx.lang(), "achievements.no-bots")).await;
        }
        let (user, name) = match self.user {
            Some(u) => (u.resolved.id.get(), resolved_display_name(&u)),
            None => (cx.author()?.id.get(), cx.author_display_name()),
        };
        if privacy::private_profile(cx, user).await? {
            return Ok(());
        }
        cx.reply(card(cx, guild, user, &name, 0).await?).await
    }
}

impl AchievementAdmin {
    pub async fn run(self, cx: &Cx) -> anyhow::Result<()> {
        let Some(guild) = cx.guild_id() else {
            return cx.reply_ephemeral(t!(cx.lang(), "achievements.guild-only")).await;
        };
        if !cx.can_manage_guild() {
            return cx.reply_ephemeral(t!(cx.lang(), "achievements.manage-needed")).await;
        }
        match self {
            Self::Add(c) => {
                if let Some(ref role) = c.role
                    && let Some(why) = roles::unassignable(cx, guild, role).await
                {
                    return cx.reply_ephemeral(why).await;
                }
                if let Some(ref role) = c.role
                    && let Some(why) =
                        settings::run_time_refusal_in(cx.lang(), role.permissions.bits(), &format!("<@&{}>", role.id))
                {
                    return cx.reply_ephemeral(why).await;
                }
                if let Some(channel) = c.announce_channel
                    && !valid_channel(cx, guild, channel).await
                {
                    return cx.reply_ephemeral(t!(cx.lang(), "achievements.bad-channel")).await;
                }
                let icon = c.icon.as_deref().unwrap_or("star");
                if !valid_icon(icon) {
                    return cx.reply_ephemeral(t!(cx.lang(), "achievements.bad-icon")).await;
                }
                let draft = AchievementDraft {
                    name: &c.name,
                    description: c.description.as_deref().unwrap_or(""),
                    icon,
                    kind: c.kind.kind(),
                    target: c.target,
                    reward_role_id: c.role.map(|r| r.id.get()),
                    reward_points: c.points.unwrap_or(0),
                    announce_channel_id: c.announce_channel.map(Id::get),
                    secret: c.secret.unwrap_or(false),
                    enabled: true,
                };
                let result = cx.app.db.save_achievement(guild, None, &draft).await?;
                invalidate(&cx.app, guild).await?;
                cx.reply_ephemeral(save_text(cx, result)).await
            }
            Self::Edit(c) => {
                let Some(old) = cx.app.db.achievement(guild, c.id).await? else {
                    return cx.reply_ephemeral(t!(cx.lang(), "achievements.missing")).await;
                };
                if let Some(ref role) = c.role
                    && let Some(why) = roles::unassignable(cx, guild, role).await
                {
                    return cx.reply_ephemeral(why).await;
                }
                if let Some(ref role) = c.role
                    && let Some(why) =
                        settings::run_time_refusal_in(cx.lang(), role.permissions.bits(), &format!("<@&{}>", role.id))
                {
                    return cx.reply_ephemeral(why).await;
                }
                if let Some(channel) = c.announce_channel
                    && !valid_channel(cx, guild, channel).await
                {
                    return cx.reply_ephemeral(t!(cx.lang(), "achievements.bad-channel")).await;
                }
                let name = c.name.as_deref().unwrap_or(&old.name);
                let description = c.description.as_deref().unwrap_or(&old.description);
                let icon = c.icon.as_deref().unwrap_or(&old.icon);
                if !valid_icon(icon) {
                    return cx.reply_ephemeral(t!(cx.lang(), "achievements.bad-icon")).await;
                }
                let draft = AchievementDraft {
                    name,
                    description,
                    icon,
                    kind: c
                        .kind
                        .map(AchievementKind::kind)
                        .unwrap_or_else(|| Kind::parse(&old.kind).expect("stored kind")),
                    target: c.target.unwrap_or(old.target),
                    reward_role_id: if c.clear_role.unwrap_or(false) {
                        None
                    } else {
                        c.role.map(|r| r.id.get()).or(old.reward_role_id.map(|r| r as u64))
                    },
                    reward_points: c.points.unwrap_or(old.reward_points),
                    announce_channel_id: if c.clear_announce_channel.unwrap_or(false) {
                        None
                    } else {
                        c.announce_channel
                            .map(Id::get)
                            .or(old.announce_channel_id.map(|id| id as u64))
                    },
                    secret: c.secret.unwrap_or(old.secret),
                    enabled: c.enabled.unwrap_or(old.enabled),
                };
                let result = cx.app.db.save_achievement(guild, Some(c.id), &draft).await?;
                invalidate(&cx.app, guild).await?;
                cx.reply_ephemeral(save_text(cx, result)).await
            }
            Self::Remove(c) => {
                let removed = cx.app.db.remove_achievement(guild, c.id).await?;
                invalidate(&cx.app, guild).await?;
                cx.reply_ephemeral(t!(
                    cx.lang(),
                    if removed {
                        "achievements.removed"
                    } else {
                        "achievements.missing"
                    }
                ))
                .await
            }
            Self::List(_) => {
                let rows = cx.app.db.achievements(guild).await?;
                let mut text = t!(
                    cx.lang(),
                    "achievements.list",
                    used = rows.len(),
                    max = achievements::MAX_ACHIEVEMENTS
                );
                for row in rows {
                    let kind = t!(cx.lang(), &format!("achievements.kind.{}", row.kind));
                    text.push_str(&format!(
                        "\n`{}` {} · {} {}/{}",
                        row.id,
                        row.name,
                        kind,
                        row.target,
                        if row.enabled { "✓" } else { "○" }
                    ));
                }
                cx.reply_ephemeral(text).await
            }
            Self::Grant(c) => {
                if c.user.resolved.bot || !human_member(&cx.app, guild, c.user.resolved.id.get()).await? {
                    return cx.reply_ephemeral(t!(cx.lang(), "achievements.no-bots")).await;
                }
                let Some(row) = cx.app.db.achievement(guild, c.id).await? else {
                    return cx.reply_ephemeral(t!(cx.lang(), "achievements.missing")).await;
                };
                if row.kind != "custom" {
                    return cx.reply_ephemeral(t!(cx.lang(), "achievements.custom-only")).await;
                }
                let user = c.user.resolved.id.get();
                let Some(rule) = cx.app.db.grant_achievement(guild, user, c.id, Kind::Custom).await? else {
                    return cx.reply_ephemeral(t!(cx.lang(), "achievements.already-earned")).await;
                };
                deliver(&cx.app, guild, user, &rule).await;
                cx.reply_ephemeral(t!(cx.lang(), "achievements.granted", name = row.name))
                    .await
            }
        }
    }
}

fn save_text(cx: &Cx, result: AchievementSave) -> String {
    match result {
        AchievementSave::Saved(id) => t!(cx.lang(), "achievements.saved", id = id),
        AchievementSave::Full => t!(cx.lang(), "achievements.full"),
        AchievementSave::NameTaken => t!(cx.lang(), "achievements.name-taken"),
        AchievementSave::Missing => t!(cx.lang(), "achievements.missing"),
    }
}

fn valid_icon(icon: &str) -> bool {
    achievements::valid_icon(icon)
        && icon
            .strip_prefix("b_")
            .is_none_or(|id| pinchora_game::Content::get().badge(id).is_some())
}

async fn valid_channel(cx: &Cx, guild: u64, id: Id<ChannelMarker>) -> bool {
    let Ok(response) = cx.app.http.channel(id).await else {
        return false;
    };
    let Ok(channel) = response.model().await else {
        return false;
    };
    channel.guild_id.is_some_and(|g| g.get() == guild)
        && matches!(channel.kind, ChannelType::GuildText | ChannelType::GuildAnnouncement)
}

async fn rules(app: &App, guild: u64) -> anyhow::Result<Arc<Vec<AchievementRow>>> {
    RULES
        .try_get_with(guild, async { anyhow::Ok(Arc::new(app.db.achievements(guild).await?)) })
        .await
        .map_err(|error| anyhow::anyhow!("{error:#}"))
}

async fn visible(app: &App, guild: u64, rules: &[AchievementRow]) -> Vec<AchievementRow> {
    let mut allowed = HashMap::new();
    let mut out = Vec::new();
    for row in rules.iter().filter(|r| r.enabled) {
        let Some(kind) = Kind::parse(&row.kind) else { continue };
        if let Some(module) = kind.module() {
            let on = match allowed.get(&module) {
                Some(on) => *on,
                None => {
                    let on = source_enabled(app, guild, kind).await;
                    allowed.insert(module, on);
                    on
                }
            };
            if !on {
                continue;
            }
        }
        out.push(row.clone());
    }
    out
}

async fn card(cx: &Cx, guild: u64, user: u64, name: &str, page: usize) -> anyhow::Result<InteractionResponseData> {
    // A privacy edit must take effect even if this pod missed a cache notification.
    let all = cx.app.db.achievements(guild).await?;
    let earned: HashSet<i64> = cx
        .app
        .db
        .earned_achievements(guild, user)
        .await?
        .into_iter()
        .map(|(id, _)| id)
        .collect();
    let visible = visible(&cx.app, guild, &all).await;
    let visible: Vec<_> = visible
        .into_iter()
        .filter(|r| !r.secret || earned.contains(&r.id))
        .collect();
    let pages = visible.len().div_ceil(5).max(1);
    let page = page.min(pages - 1);
    let mut cells = Vec::new();
    for row in visible.iter().skip(page * 5).take(5) {
        let kind = Kind::parse(&row.kind).expect("stored kind");
        let got = earned.contains(&row.id);
        let progress = if got {
            row.target
        } else {
            cx.app.db.achievement_progress(guild, user, kind).await?
        };
        let icon_png = row
            .icon
            .strip_prefix("b_")
            .and_then(|id| cx.app.sprites.badge(id))
            .map(|art| art.png.clone());
        cells.push(AchievementCell {
            name: row.name.clone(),
            description: row.description.clone(),
            icon: row.icon.clone(),
            icon_png,
            progress,
            target: row.target,
            earned: got,
        });
    }
    let lang = cx.app.guild_lang(guild).await;
    let title = t!(lang, "achievements.card.title", user = name);
    let page_label = t!(lang, "achievements.card.page", page = page + 1, pages = pages);
    let png = cx
        .app
        .render(move |r| {
            r.achievements_card(&AchievementsCard {
                lang,
                title,
                kicker: t!(lang, "achievements.card.kicker"),
                earned_label: t!(lang, "achievements.card.earned"),
                locked_label: t!(lang, "achievements.card.locked"),
                page_label,
                empty_label: t!(lang, "achievements.card.empty"),
                cells,
                theme: Theme::reef(),
            })
        })
        .await?;
    let button = |label: String, index: usize, disabled: bool| {
        Component::Button(Button {
            custom_id: Some(format!("ah:{user}:{index}")),
            disabled,
            emoji: None,
            id: None,
            label: Some(label),
            style: ButtonStyle::Secondary,
            url: None,
            sku_id: None,
        })
    };
    let components = if pages > 1 {
        vec![Component::ActionRow(ActionRow {
            id: None,
            components: vec![
                button(t!(lang, "achievements.previous"), page.saturating_sub(1), page == 0),
                button(t!(lang, "achievements.next"), page + 1, page + 1 >= pages),
            ],
        })]
    } else {
        vec![]
    };
    Ok(Cx::card(
        EmbedBuilder::new().color(BRAND),
        "achievements.png",
        png,
        components,
        false,
    ))
}

pub async fn on_component(cx: &Cx, mut parts: std::str::Split<'_, char>) -> anyhow::Result<()> {
    let Some(guild) = cx.guild_id() else {
        return cx.reply_ephemeral(t!(cx.lang(), "achievements.guild-only")).await;
    };
    let Some(user) = parts.next().and_then(|v| v.parse::<u64>().ok()) else {
        return cx.reply_ephemeral(t!(cx.lang(), "component.button-expired")).await;
    };
    let Some(page) = parts.next().and_then(|v| v.parse::<usize>().ok()) else {
        return cx.reply_ephemeral(t!(cx.lang(), "component.button-expired")).await;
    };
    if privacy::private_profile(cx, user).await? {
        return Ok(());
    }
    let name = if user == cx.author()?.id.get() {
        cx.author_display_name()
    } else {
        format!("<@{user}>")
    };
    cx.update(card(cx, guild, user, &name, page).await?).await
}

/// Called after moderation accepts a message; Redis limits evaluation to once per member per minute.
pub async fn on_message(app: &Arc<App>, guild: u64, user: u64) {
    if !matches!(
        app.module_access(guild, None, Module::Achievements).await,
        Ok(Access::Allowed)
    ) {
        return;
    }
    match rules(app, guild).await {
        Ok(rows) if rows.iter().any(|row| row.enabled && row.kind != "custom") => {}
        Ok(_) => return,
        Err(error) => {
            tracing::debug!(guild, %error, "achievement rules unavailable");
            return;
        }
    }
    static SLOTS: LazyLock<Arc<tokio::sync::Semaphore>> = LazyLock::new(|| Arc::new(tokio::sync::Semaphore::new(32)));
    let Ok(permit) = SLOTS.clone().try_acquire_owned() else {
        return;
    };
    match app
        .cache
        .try_cooldown(&format!("ah:tick:{guild}:{user}"), Duration::from_secs(60))
        .await
    {
        Ok(None) => {
            // A shared fixed-minute budget bounds work even across gateway pods.
            let minute = app.now().timestamp().div_euclid(60);
            if !matches!(
                app.cache
                    .incr(&format!("ah:budget:{guild}:{minute}"), Duration::from_secs(120))
                    .await,
                Ok(1..=100)
            ) {
                return;
            }
            let task_app = app.clone();
            app.spawn_event("achievement", async move {
                let _permit = permit;
                if let Err(error) = evaluate(&task_app, guild, user).await {
                    tracing::warn!(guild, user, %error, "achievement evaluation failed");
                }
            });
        }
        Ok(Some(_)) => {}
        Err(error) => tracing::debug!(guild, user, %error, "achievement cooldown unavailable"),
    }
}

pub async fn on_level_up(app: &Arc<App>, guild: u64, user: u64) {
    on_message(app, guild, user).await;
}

pub async fn evaluate(app: &App, guild: u64, user: u64) -> anyhow::Result<()> {
    if !matches!(
        app.module_access(guild, None, Module::Achievements).await,
        Ok(Access::Allowed)
    ) {
        return Ok(());
    }
    let all = rules(app, guild).await?;
    let visible = visible(app, guild, &all).await;
    if !visible.iter().any(|r| r.kind != "custom") {
        return Ok(());
    }
    // Resolve the recipient at grant time; cached counters are not proof of a human member.
    if !human_member(app, guild, user).await? {
        return Ok(());
    }
    let earned: HashSet<i64> = app
        .db
        .earned_achievements(guild, user)
        .await?
        .into_iter()
        .map(|(id, _)| id)
        .collect();
    let mut progress = HashMap::new();
    for rule in visible
        .iter()
        .filter(|r| !earned.contains(&r.id))
        .take(achievements::MAX_ACHIEVEMENTS)
    {
        let Some(kind) = Kind::parse(&rule.kind) else { continue };
        if kind == Kind::Custom {
            continue;
        }
        let value = match progress.get(&kind.key()) {
            Some(value) => *value,
            None => {
                let value = app.db.achievement_progress(guild, user, kind).await?;
                progress.insert(kind.key(), value);
                value
            }
        };
        if value >= rule.target
            && let Some(granted) = app.db.grant_achievement(guild, user, rule.id, kind).await?
        {
            deliver(app, guild, user, &granted).await;
        }
    }
    Ok(())
}

async fn deliver(app: &App, guild: u64, user: u64, rule: &AchievementRow) {
    if let Some(role_id) = rule.reward_role_id.map(|id| id as u64)
        && let Err(error) = grant_role(app, guild, user, role_id).await
    {
        tracing::warn!(guild, user, role_id, %error, "achievement role not granted");
    }
    if let Some(channel) = rule.announce_channel_id
        && app
            .cache
            .once(&format!("ah:announce:{guild}:{channel}"), Duration::from_secs(60))
            .await
            .unwrap_or(false)
    {
        let lang = app.guild_lang(guild).await;
        let text = t!(lang, "achievements.announcement-batch");
        if let Err(error) = app
            .http
            .create_message(Id::new(channel as u64))
            .content(&text)
            .allowed_mentions(Some(&AllowedMentions::default()))
            .await
        {
            tracing::warn!(guild, channel, %error, "achievement announcement failed");
        }
    }
}

async fn grant_role(app: &App, guild: u64, user: u64, role_id: u64) -> anyhow::Result<()> {
    let roles = app.http.roles(Id::new(guild)).await?.models().await?;
    let role = roles
        .iter()
        .find(|r| r.id.get() == role_id)
        .ok_or_else(|| anyhow::anyhow!("role missing"))?;
    let me = app
        .http
        .guild_member(Id::new(guild), Id::new(app.application_id.get()))
        .await?
        .model()
        .await?;
    let top = roles
        .iter()
        .filter(|r| me.roles.contains(&r.id))
        .map(|r| r.position)
        .max()
        .unwrap_or(0);
    let facts = RoleFacts {
        everyone: role_id == guild,
        managed: role.managed,
        position: role.position,
        permissions: role.permissions.bits(),
    };
    let label = format!("<@&{role_id}>");
    if let Some(why) = settings::role_refusal(facts, Some(top), &label)
        .or_else(|| settings::run_time_refusal(facts.permissions, &label))
    {
        anyhow::bail!("{why}")
    }
    app.http
        .add_guild_member_role(Id::new(guild), Id::new(user), Id::new(role_id))
        .reason("Achievement reward")
        .await?;
    Ok(())
}

async fn human_member(app: &App, guild: u64, user: u64) -> anyhow::Result<bool> {
    if user == app.application_id.get() {
        return Ok(false);
    }
    let member = app
        .http
        .guild_member(Id::new(guild), Id::new(user))
        .await?
        .model()
        .await?;
    Ok(!member.user.bot)
}

/// The legacy leveling switch lives in guild settings, outside the module channel rules.
/// Reward evaluation fails closed when either source of configuration is unavailable.
pub async fn source_enabled(app: &App, guild: u64, kind: Kind) -> bool {
    let Some(module) = kind.module() else { return true };
    // These are server totals: a source restricted to selected channels is still enabled.
    let Ok(rules) = app.guild_modules(guild).await else {
        return false;
    };
    if !rules.rule(module).enabled {
        return false;
    }
    module != Module::Leveling
        || app
            .guild_settings(guild)
            .await
            .is_ok_and(|settings| settings.leveling_enabled)
}
