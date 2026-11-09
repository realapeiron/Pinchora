//! `/info`: Discord objects on compact pixel cards.

mod views;

use pinchora_core::t;
use pinchora_render::{InfoCard, InfoRow, Theme};
use serde_json::Value;
use std::{sync::LazyLock, time::Duration};
use twilight_interactions::command::{CommandModel, CreateCommand, ResolvedUser};
use twilight_model::{
    application::interaction::InteractionChannel,
    channel::message::{
        Component,
        component::{ActionRow, Button, ButtonStyle},
    },
    guild::{Permissions, Role},
    id::{
        Id,
        marker::{ChannelMarker, EmojiMarker, GuildMarker, UserMarker},
    },
    user::User,
};
use twilight_util::builder::embed::EmbedBuilder;

use crate::Cx;
use crate::commands::social::customize;
use crate::infra::util::BRAND;

static BOT_CARD: LazyLock<moka::future::Cache<String, Vec<u8>>> = LazyLock::new(|| {
    moka::future::Cache::builder()
        .time_to_live(Duration::from_secs(30))
        .max_capacity(256)
        .build()
});

#[derive(CommandModel, CreateCommand)]
#[command(
    name = "info",
    desc = "Information about members and this server",
    contexts = "guild"
)]
pub enum Info {
    #[command(name = "user")]
    User(InfoUser),
    #[command(name = "server")]
    Server(InfoServer),
    #[command(name = "bot")]
    Bot(InfoBot),
    #[command(name = "avatar")]
    Avatar(InfoAvatar),
    #[command(name = "banner")]
    Banner(InfoBanner),
    #[command(name = "role")]
    Role(InfoRole),
    #[command(name = "channel")]
    Channel(InfoChannel),
    #[command(name = "members")]
    Members(InfoMembers),
    #[command(name = "emoji")]
    Emoji(InfoEmoji),
    #[command(name = "invite")]
    Invite(InfoInvite),
}

#[derive(CommandModel, CreateCommand)]
#[command(name = "user", desc = "Show a member's account and server details")]
pub struct InfoUser {
    /// Member to inspect
    user: Option<ResolvedUser>,
}
#[derive(CommandModel, CreateCommand)]
#[command(name = "server", desc = "Show server details")]
pub struct InfoServer;
#[derive(CommandModel, CreateCommand)]
#[command(name = "bot", desc = "Show Pinchora's live status")]
pub struct InfoBot;
#[derive(CommandModel, CreateCommand)]
#[command(name = "avatar", desc = "Show a user's avatar")]
pub struct InfoAvatar {
    /// Member whose avatar to view
    user: Option<ResolvedUser>,
    /// Prefer the server avatar
    server: Option<bool>,
}
#[derive(CommandModel, CreateCommand)]
#[command(name = "banner", desc = "Show a user's banner")]
pub struct InfoBanner {
    /// Member whose banner to view
    user: Option<ResolvedUser>,
}
#[derive(CommandModel, CreateCommand)]
#[command(name = "role", desc = "Show a role's details")]
pub struct InfoRole {
    /// Role to inspect
    role: Role,
}
#[derive(CommandModel, CreateCommand)]
#[command(name = "channel", desc = "Show a channel's details")]
pub struct InfoChannel {
    /// Channel to inspect
    channel: Option<InteractionChannel>,
}
#[derive(CommandModel, CreateCommand)]
#[command(name = "members", desc = "Show member counts")]
pub struct InfoMembers;
#[derive(CommandModel, CreateCommand)]
#[command(name = "emoji", desc = "Show a custom emoji")]
pub struct InfoEmoji {
    /// Custom or Unicode emoji
    emoji: Option<String>,
}
#[derive(CommandModel, CreateCommand)]
#[command(name = "invite", desc = "Show an invite's details")]
pub struct InfoInvite {
    /// Invite code
    code: String,
}

fn field(value: &Value, key: &str) -> String {
    match value.get(key) {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Number(n)) => n.to_string(),
        Some(Value::Bool(b)) => b.to_string(),
        _ => "—".into(),
    }
}

fn row(label: String, value: impl Into<String>) -> InfoRow {
    InfoRow {
        label,
        value: value.into(),
    }
}

async fn thumbnail(cx: &Cx, url: &str) -> Option<Vec<u8>> {
    let bytes = cx
        .app
        .web
        .get(url)
        .send()
        .await
        .ok()?
        .error_for_status()
        .ok()?
        .bytes()
        .await
        .ok()?;
    cx.app
        .render(move |_| pinchora_render::prepare_avatar(&bytes, 160, Some(40)))
        .await
        .ok()
}

async fn send(
    cx: &Cx,
    title: String,
    subtitle: String,
    rows: Vec<InfoRow>,
    image: Option<Vec<u8>>,
    page: Option<(u64, u64, usize, usize)>,
    update: bool,
) -> anyhow::Result<()> {
    let private = subtitle == t!(cx.lang(), "info.channel") || subtitle == t!(cx.lang(), "info.invite");
    let lang = cx.lang();
    let actor = cx.author()?.id.get();
    let expires = chrono::Utc::now().timestamp() + 300;
    let png = cx
        .app
        .render(move |r| {
            r.svg_to_png(
                &InfoCard {
                    lang,
                    title,
                    subtitle,
                    rows,
                    image,
                    theme: Theme::reef(),
                }
                .to_svg(),
            )
        })
        .await?;
    let components = page.map_or_else(Vec::new, |(guild, user, current, pages)| {
        let button = |label: String, target: usize, disabled: bool| {
            Component::Button(Button {
                custom_id: Some(format!("nf:roles:{guild}:{user}:{actor}:{expires}:{target}")),
                disabled,
                emoji: None,
                id: None,
                label: Some(label),
                sku_id: None,
                style: ButtonStyle::Secondary,
                url: None,
            })
        };
        vec![Component::ActionRow(ActionRow {
            id: None,
            components: vec![
                button(t!(cx.lang(), "info.previous"), current.saturating_sub(1), current == 0),
                button(t!(cx.lang(), "info.next"), current + 1, current + 1 >= pages),
            ],
        })]
    });
    let data = Cx::card(EmbedBuilder::new().color(BRAND), "info.png", png, components, private);
    if update {
        cx.update(data).await
    } else {
        cx.reply(data).await
    }
}

async fn bot_card(cx: &Cx) -> anyhow::Result<()> {
    use crate::admin::status::format_number;
    use crate::admin::status::snapshot;
    let s = snapshot(
        &cx.app.db,
        &cx.app.cache,
        cx.app.application_id.get(),
        &cx.app.public_url,
    )
    .await;
    let tr = cx.lang() == pinchora_core::infra::i18n::Lang::Tr;
    let number = |n| format_number(n, tr);
    let duration = t!(
        cx.lang(),
        "info.bot-uptime-value",
        hours = s.uptime_seconds / 3600,
        minutes = s.uptime_seconds / 60 % 60
    );
    let this_shard = cx.guild_id().and_then(|guild| {
        let total = s.shards.first()?.total.max(1);
        s.shards
            .iter()
            .find(|shard| shard.id == ((guild >> 22) % u64::from(total)) as u32)
    });
    let latency = this_shard
        .and_then(|shard| shard.latency_ms)
        .map_or_else(|| "—".into(), |ms| format!("{ms} ms"));
    let rows = vec![
        row(t!(cx.lang(), "info.bot-servers"), number(s.servers)),
        row(t!(cx.lang(), "info.bot-members"), number(s.approximate_members)),
        row(t!(cx.lang(), "info.bot-players"), number(s.players)),
        row(t!(cx.lang(), "info.bot-shards"), number(s.shards.len() as u64)),
        row(
            t!(cx.lang(), "info.bot-this-shard"),
            this_shard.map_or_else(|| "—".into(), |shard| shard.id.to_string()),
        ),
        row(t!(cx.lang(), "info.bot-latency"), latency),
        row(t!(cx.lang(), "info.bot-uptime"), duration),
        row(t!(cx.lang(), "info.bot-version"), s.version.clone()),
        row(t!(cx.lang(), "info.bot-rust"), s.rust_version.clone()),
        row(t!(cx.lang(), "info.bot-memory"), format!("{} MB", number(s.rss_mb))),
        row(
            t!(cx.lang(), "info.bot-postgres"),
            format!("{} MB", number(s.postgres_mb)),
        ),
        row(t!(cx.lang(), "info.bot-redis"), format!("{} MB", number(s.redis_mb))),
        row(t!(cx.lang(), "info.bot-commands-24h"), number(s.commands_24h)),
        row(t!(cx.lang(), "info.bot-commands-total"), number(s.commands_total)),
        row(
            t!(cx.lang(), "info.bot-interactions"),
            number(s.interactions_per_minute),
        ),
        row(t!(cx.lang(), "info.bot-games"), number(s.active_games)),
        row(t!(cx.lang(), "info.bot-tickets"), number(s.open_tickets)),
    ];
    let lang = cx.lang();
    let icon = pinchora_render::prepare_avatar(
        include_bytes!("../../../../../assets/brand/icon-256.png"),
        160,
        Some(40),
    )?;
    let key = format!("{tr}:{}", this_shard.map_or(u32::MAX, |shard| shard.id));
    let png = if let Some(png) = BOT_CARD.get(&key).await {
        png
    } else {
        let png = cx
            .app
            .render(move |r| {
                r.svg_to_png(
                    &InfoCard {
                        lang,
                        title: "Pinchora".into(),
                        subtitle: t!(lang, "info.bot"),
                        rows,
                        image: Some(icon),
                        theme: Theme::reef(),
                    }
                    .to_svg(),
                )
            })
            .await?;
        BOT_CARD.insert(key, png.clone()).await;
        png
    };
    let mut links = format!(
        "[{}]({}) · [{}]({}) · [{}]({})",
        t!(cx.lang(), "info.bot-invite"),
        s.invite_url,
        t!(cx.lang(), "info.bot-website"),
        s.website_url,
        t!(cx.lang(), "info.bot-status"),
        s.status_url
    );
    if !s.support_url.is_empty() {
        links.push_str(&format!(
            " · [{}]({})",
            t!(cx.lang(), "info.bot-support"),
            s.support_url
        ));
    }
    if !s.shards.is_empty() {
        links.push_str(&format!("\n{}: ", t!(cx.lang(), "info.bot-latency")));
        links.push_str(
            &s.shards
                .iter()
                .take(64)
                .map(|shard| {
                    format!(
                        "#{} {}",
                        shard.id,
                        shard.latency_ms.map_or_else(|| "—".into(), |ms| format!("{ms}ms"))
                    )
                })
                .collect::<Vec<_>>()
                .join(" · "),
        );
        if s.shards.len() > 64 {
            links.push_str(&format!(" · +{}", s.shards.len() - 64));
        }
    }
    cx.reply(Cx::card(
        EmbedBuilder::new().color(BRAND).description(links),
        "bot-info.png",
        png,
        vec![],
        false,
    ))
    .await
}

async fn user_card(cx: &Cx, guild: u64, user: User, member: Value, page: usize, update: bool) -> anyhow::Result<()> {
    let id = user.id.get();
    let name = member
        .get("nick")
        .and_then(Value::as_str)
        .unwrap_or_else(|| user.global_name.as_deref().unwrap_or(&user.name));
    let roles: Vec<u64> = member
        .get("roles")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|v| v.as_str()?.parse().ok())
        .collect();
    let created = chrono::DateTime::<chrono::Utc>::from_timestamp_millis(((id >> 22) + 1_420_070_400_000) as i64)
        .map_or_else(|| "—".to_owned(), |dt| dt.format("%Y-%m-%d").to_string());
    let mut rows = vec![
        row(t!(cx.lang(), "info.account-created"), created),
        row(
            t!(cx.lang(), "info.joined"),
            field(&member, "joined_at").chars().take(10).collect::<String>(),
        ),
        row(
            t!(cx.lang(), "info.boost"),
            if member.get("premium_since").is_some_and(|v| !v.is_null()) {
                t!(cx.lang(), "info.yes")
            } else {
                t!(cx.lang(), "info.no")
            },
        ),
        row(t!(cx.lang(), "info.roles"), roles.len().to_string()),
    ];
    let private = cx.app.db.privacy(id).await?.hide_profile;
    if private {
        rows.push(row(t!(cx.lang(), "info.pinchora"), t!(cx.lang(), "info.private")));
    } else {
        let xp = cx.app.db.member_rank(guild, id).await?.map_or(0, |r| r.xp);
        let profile = cx.app.db.user(id).await?;
        rows.push(row(
            t!(cx.lang(), "info.level"),
            cx.app.leveling_policy(guild).await?.level_of(xp).to_string(),
        ));
        rows.push(row(
            t!(cx.lang(), "info.badges"),
            customize::worn_badges(&cx.app, id, &profile).await?.len().to_string(),
        ));
        if let Some(title) = customize::title_name(&profile, cx.lang()) {
            rows.push(row(t!(cx.lang(), "info.title"), title));
        }
    }
    let per_page = 9usize.saturating_sub(rows.len()).max(1);
    let pages = roles.len().div_ceil(per_page).max(1);
    let page = page.min(pages - 1);
    let names = cx.app.guild_roles(guild).await;
    for role in roles.iter().skip(page * per_page).take(per_page) {
        let name = names
            .as_ref()
            .and_then(|r| r.get(role))
            .cloned()
            .unwrap_or_else(|| role.to_string());
        rows.push(row(t!(cx.lang(), "info.role"), name));
    }
    let avatar = cx.app.avatar(&user, 160, Some(40)).await;
    send(
        cx,
        name.to_owned(),
        t!(cx.lang(), "info.user"),
        rows,
        Some((*avatar).clone()),
        (pages > 1).then_some((guild, id, page, pages)),
        update,
    )
    .await
}

fn channel_kind(lang: pinchora_core::infra::i18n::Lang, kind: twilight_model::channel::ChannelType) -> String {
    use twilight_model::channel::ChannelType;
    t!(
        lang,
        match kind {
            ChannelType::GuildText => "info.text-channel",
            ChannelType::GuildVoice => "info.voice-channel",
            ChannelType::GuildCategory => "info.category-channel",
            ChannelType::GuildAnnouncement => "info.announcement-channel",
            ChannelType::GuildForum => "info.forum-channel",
            ChannelType::GuildStageVoice => "info.stage-channel",
            kind if kind.is_thread() => "info.thread-channel",
            _ => "info.channel",
        }
    )
}

/// Use fresh guild roles and member roles; bot visibility does not imply caller visibility.
async fn visible_channel(cx: &Cx, guild: u64, channel: &twilight_model::channel::Channel) -> anyhow::Result<bool> {
    if channel.guild_id.map(|id| id.get()) != Some(guild) {
        return Ok(false);
    }
    let user = cx.author()?.id;
    let owner = cx.app.http.guild(Id::new(guild)).await?.model().await?.owner_id;
    let member = cx.app.http.guild_member(Id::new(guild), user).await?.model().await?;
    let roles = cx.app.http.roles(Id::new(guild)).await?.models().await?;
    let base = roles
        .iter()
        .filter(|r| r.id.get() == guild || member.roles.contains(&r.id))
        .fold(Permissions::empty(), |p, r| p | r.permissions);
    if user == owner || base.contains(Permissions::ADMINISTRATOR) {
        return Ok(true);
    }
    let parent;
    let overwrites = if channel.kind.is_thread() {
        let Some(id) = channel.parent_id else {
            return Ok(false);
        };
        parent = cx.app.http.channel(id).await?.model().await?;
        parent.permission_overwrites.as_deref().unwrap_or_default()
    } else {
        channel.permission_overwrites.as_deref().unwrap_or_default()
    };
    let held: Vec<_> = member.roles.iter().map(|id| id.get()).collect();
    let effective = crate::infra::text::access::channel_permissions(base, guild, user.get(), &held, overwrites);
    if !effective.contains(Permissions::VIEW_CHANNEL) {
        return Ok(false);
    }
    // The parent's overwrites can deny Manage Threads even when a guild role grants it.
    if channel.kind == twilight_model::channel::ChannelType::PrivateThread
        && !effective.contains(Permissions::MANAGE_THREADS)
        && cx.app.http.thread_member(channel.id, user).await.is_err()
    {
        return Ok(false);
    }
    Ok(true)
}

impl Info {
    pub async fn run(self, cx: &Cx) -> anyhow::Result<()> {
        if matches!(self, Self::Bot(_)) {
            cx.responder.defer(false).await?;
            return bot_card(cx).await;
        }
        let Some(guild) = cx.guild_id() else {
            return cx.reply_ephemeral(t!(cx.lang(), "error.guild-only")).await;
        };
        cx.responder
            .defer(matches!(self, Self::Channel(_) | Self::Invite(_)))
            .await?;
        match self {
            Self::Bot(_) => unreachable!("bot status handled before guild lookup"),
            Self::User(c) => views::show_user(cx, guild, c).await,
            Self::Server(_) | Self::Members(_) => views::show_server(cx, guild, matches!(self, Self::Members(_))).await,
            Self::Avatar(c) => views::show_avatar(cx, guild, c).await,
            Self::Banner(c) => views::show_banner(cx, c).await,
            Self::Role(c) => views::show_role(cx, c).await,
            Self::Channel(c) => views::show_channel(cx, guild, c).await,
            Self::Emoji(c) => views::show_emoji(cx, guild, c).await,
            Self::Invite(c) => views::show_invite(cx, guild, c).await,
        }
    }
}

pub(crate) async fn on_component<'a>(cx: &Cx, mut parts: impl Iterator<Item = &'a str>) -> anyhow::Result<()> {
    if parts.next() != Some("roles") {
        return cx.reply_ephemeral(t!(cx.lang(), "info.unavailable")).await;
    }
    let (guild, user, actor, expires, page) = (
        parts.next().and_then(|v| v.parse::<u64>().ok()),
        parts.next().and_then(|v| v.parse::<u64>().ok()),
        parts.next().and_then(|v| v.parse::<u64>().ok()),
        parts.next().and_then(|v| v.parse::<i64>().ok()),
        parts.next().and_then(|v| v.parse::<usize>().ok()),
    );
    let (Some(guild), Some(user), Some(actor), Some(expires), Some(page)) = (guild, user, actor, expires, page) else {
        return cx.reply_ephemeral(t!(cx.lang(), "info.unavailable")).await;
    };
    if cx.guild_id() != Some(guild)
        || user == 0
        || actor != cx.author()?.id.get()
        || expires < chrono::Utc::now().timestamp()
        || expires > chrono::Utc::now().timestamp() + 300
        || parts.next().is_some()
    {
        return cx.reply_ephemeral(t!(cx.lang(), "info.unavailable")).await;
    }
    let member = cx
        .app
        .http
        .guild_member(Id::<GuildMarker>::new(guild), Id::<UserMarker>::new(user))
        .await?
        .model()
        .await?;
    let user_data = member.user.clone();
    user_card(cx, guild, user_data, serde_json::to_value(member)?, page, true).await
}
