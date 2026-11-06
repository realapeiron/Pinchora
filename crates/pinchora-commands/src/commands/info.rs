//! `/info`: Discord objects on compact pixel cards.

use pinchora_core::{leveling, t};
use pinchora_render::{InfoCard, InfoRow, Theme};
use serde_json::Value;
use twilight_interactions::command::{CommandModel, CreateCommand, ResolvedUser};
use twilight_model::{
    application::interaction::InteractionChannel,
    channel::message::{
        Component,
        component::{ActionRow, Button, ButtonStyle},
    },
    guild::Role,
    id::{
        Id,
        marker::{ChannelMarker, EmojiMarker, GuildMarker, UserMarker},
    },
    user::User,
};
use twilight_util::builder::embed::EmbedBuilder;

use crate::{Cx, commands::customize, util::BRAND};

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
    emoji: String,
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
    let png = cx
        .app
        .render(move |r| {
            r.svg_to_png(
                &InfoCard {
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
                custom_id: Some(format!("nf:roles:{guild}:{user}:{target}")),
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
    let data = Cx::card(EmbedBuilder::new().color(BRAND), "info.png", png, components, false);
    if update {
        cx.update(data).await
    } else {
        cx.reply(data).await
    }
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
        row(
            t!(cx.lang(), "info.roles"),
            t!(cx.lang(), "info.role-count", count = roles.len()),
        ),
    ];
    let private = id != cx.author()?.id.get() && cx.app.db.privacy(id).await?.hide_profile;
    if private {
        rows.push(row(t!(cx.lang(), "info.pinchora"), t!(cx.lang(), "info.private")));
    } else {
        let xp = cx.app.db.member_rank(guild, id).await?.map_or(0, |r| r.xp);
        let profile = cx.app.db.user(id).await?;
        rows.push(row(t!(cx.lang(), "info.level"), leveling::level_of(xp).to_string()));
        rows.push(row(
            t!(cx.lang(), "info.badges"),
            customize::worn_badges(&cx.app, id, &profile).await?.len().to_string(),
        ));
        if let Some(title) = customize::title_name(&profile) {
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

impl Info {
    pub async fn run(self, cx: &Cx) -> anyhow::Result<()> {
        let Some(guild) = cx.guild_id() else {
            return cx.reply_ephemeral(t!(cx.lang(), "error.guild-only")).await;
        };
        cx.responder.defer(false).await?;
        let gid = Id::<GuildMarker>::new(guild);
        match self {
            Self::User(c) => {
                let user = c.user.map_or_else(|| cx.author().cloned(), |r| Ok(r.resolved))?;
                let member = cx.app.http.guild_member(gid, user.id).await?.model().await?;
                user_card(cx, guild, user, serde_json::to_value(member)?, 0, false).await
            }
            Self::Server(_) | Self::Members(_) => {
                let data = serde_json::to_value(cx.app.http.guild(gid).with_counts(true).await?.model().await?)?;
                let rows = if matches!(self, Self::Members(_)) {
                    vec![
                        row(t!(cx.lang(), "info.members"), field(&data, "approximate_member_count")),
                        row(
                            t!(cx.lang(), "info.pinchora-members"),
                            cx.app.db.guild_member_count(guild).await?.to_string(),
                        ),
                    ]
                } else {
                    vec![
                        row(t!(cx.lang(), "info.owner"), field(&data, "owner_id")),
                        row(t!(cx.lang(), "info.members"), field(&data, "approximate_member_count")),
                        row(
                            t!(cx.lang(), "info.roles"),
                            data.get("roles")
                                .and_then(Value::as_array)
                                .map_or(0, Vec::len)
                                .to_string(),
                        ),
                        row(
                            t!(cx.lang(), "info.created"),
                            chrono::DateTime::<chrono::Utc>::from_timestamp_millis(
                                ((guild >> 22) + 1_420_070_400_000) as i64,
                            )
                            .map_or_else(|| "—".into(), |d| d.format("%Y-%m-%d").to_string()),
                        ),
                    ]
                };
                send(
                    cx,
                    field(&data, "name"),
                    t!(cx.lang(), "info.server"),
                    rows,
                    None,
                    None,
                    false,
                )
                .await
            }
            Self::Avatar(c) => {
                let user = c.user.map_or_else(|| cx.author().cloned(), |r| Ok(r.resolved))?;
                let mut avatar = user
                    .avatar
                    .map(|h| format!("https://cdn.discordapp.com/avatars/{}/{}.png?size=1024", user.id, h));
                if c.server == Some(true) {
                    let member = serde_json::to_value(cx.app.http.guild_member(gid, user.id).await?.model().await?)?;
                    if let Some(hash) = member.get("avatar").and_then(Value::as_str) {
                        avatar = Some(format!(
                            "https://cdn.discordapp.com/guilds/{guild}/users/{}/avatars/{hash}.png?size=1024",
                            user.id
                        ));
                    }
                }
                let picture = if c.server == Some(true) {
                    match avatar.as_deref() {
                        Some(url) if url.contains("/guilds/") => thumbnail(cx, url).await,
                        _ => None,
                    }
                } else {
                    None
                };
                let picture = match picture {
                    Some(png) => png,
                    None => (*cx.app.avatar(&user, 160, Some(40)).await).clone(),
                };
                send(
                    cx,
                    user.name,
                    t!(cx.lang(), "info.avatar"),
                    vec![row(
                        t!(cx.lang(), "info.image-url"),
                        avatar.unwrap_or_else(|| t!(cx.lang(), "info.default-avatar")),
                    )],
                    Some(picture),
                    None,
                    false,
                )
                .await
            }
            Self::Banner(c) => {
                let user = c.user.map_or_else(|| cx.author().cloned(), |r| Ok(r.resolved))?;
                let data = serde_json::to_value(cx.app.http.user(user.id).await?.model().await?)?;
                let url = data
                    .get("banner")
                    .and_then(Value::as_str)
                    .map(|hash| format!("https://cdn.discordapp.com/banners/{}/{hash}.png?size=1024", user.id));
                let picture = match url.as_deref() {
                    Some(url) => thumbnail(cx, url).await,
                    None => None,
                };
                send(
                    cx,
                    user.name,
                    t!(cx.lang(), "info.banner"),
                    vec![row(
                        t!(cx.lang(), "info.image-url"),
                        url.unwrap_or_else(|| t!(cx.lang(), "info.no-banner")),
                    )],
                    picture,
                    None,
                    false,
                )
                .await
            }
            Self::Role(c) => {
                let data = serde_json::to_value(&c.role)?;
                send(
                    cx,
                    c.role.name.clone(),
                    t!(cx.lang(), "info.role"),
                    vec![
                        row(t!(cx.lang(), "info.id"), c.role.id.to_string()),
                        row(t!(cx.lang(), "info.color"), field(&data, "color")),
                        row(t!(cx.lang(), "info.position"), field(&data, "position")),
                        row(t!(cx.lang(), "info.mentionable"), field(&data, "mentionable")),
                        row(t!(cx.lang(), "info.hoisted"), field(&data, "hoist")),
                    ],
                    None,
                    None,
                    false,
                )
                .await
            }
            Self::Channel(c) => {
                let channel = c
                    .channel
                    .map(|c| c.id)
                    .or_else(|| cx.interaction.channel.as_ref().map(|c| c.id));
                let Some(channel) = channel else {
                    return cx.reply_ephemeral(t!(cx.lang(), "info.unavailable")).await;
                };
                let data = serde_json::to_value(
                    cx.app
                        .http
                        .channel(Id::<ChannelMarker>::new(channel.get()))
                        .await?
                        .model()
                        .await?,
                )?;
                send(
                    cx,
                    field(&data, "name"),
                    t!(cx.lang(), "info.channel"),
                    vec![
                        row(t!(cx.lang(), "info.id"), channel.to_string()),
                        row(t!(cx.lang(), "info.type"), field(&data, "type")),
                        row(t!(cx.lang(), "info.parent"), field(&data, "parent_id")),
                        row(t!(cx.lang(), "info.topic"), field(&data, "topic")),
                    ],
                    None,
                    None,
                    false,
                )
                .await
            }
            Self::Emoji(c) => {
                let id = c
                    .emoji
                    .split(':')
                    .next_back()
                    .unwrap_or("")
                    .trim_end_matches('>')
                    .parse::<u64>();
                let Ok(id) = id else {
                    return send(
                        cx,
                        c.emoji.clone(),
                        t!(cx.lang(), "info.emoji"),
                        vec![row(t!(cx.lang(), "info.unicode"), c.emoji)],
                        None,
                        None,
                        false,
                    )
                    .await;
                };
                let data = serde_json::to_value(
                    cx.app
                        .http
                        .emoji(gid, Id::<EmojiMarker>::new(id))
                        .await?
                        .model()
                        .await?,
                )?;
                send(
                    cx,
                    field(&data, "name"),
                    t!(cx.lang(), "info.emoji"),
                    vec![
                        row(t!(cx.lang(), "info.id"), id.to_string()),
                        row(t!(cx.lang(), "info.animated"), field(&data, "animated")),
                    ],
                    None,
                    None,
                    false,
                )
                .await
            }
            Self::Invite(c) => {
                let data = serde_json::to_value(cx.app.http.invite(&c.code).await?.model().await?)?;
                if data.get("guild").and_then(|v| v.get("id")).and_then(Value::as_str) != Some(&guild.to_string()) {
                    return cx.reply_ephemeral(t!(cx.lang(), "info.invite-other")).await;
                }
                send(
                    cx,
                    c.code,
                    t!(cx.lang(), "info.invite"),
                    vec![
                        row(
                            t!(cx.lang(), "info.channel"),
                            data.get("channel").map_or("—".into(), |v| field(v, "name")),
                        ),
                        row(t!(cx.lang(), "info.uses"), field(&data, "uses")),
                        row(t!(cx.lang(), "info.max-uses"), field(&data, "max_uses")),
                        row(t!(cx.lang(), "info.expires"), field(&data, "expires_at")),
                    ],
                    None,
                    None,
                    false,
                )
                .await
            }
        }
    }
}

pub(crate) async fn on_component<'a>(cx: &Cx, mut parts: impl Iterator<Item = &'a str>) -> anyhow::Result<()> {
    if parts.next() != Some("roles") {
        return cx.reply_ephemeral(t!(cx.lang(), "info.unavailable")).await;
    }
    let (guild, user, page) = (
        parts.next().and_then(|v| v.parse::<u64>().ok()),
        parts.next().and_then(|v| v.parse::<u64>().ok()),
        parts.next().and_then(|v| v.parse::<usize>().ok()),
    );
    let (Some(guild), Some(user), Some(page)) = (guild, user, page) else {
        return cx.reply_ephemeral(t!(cx.lang(), "info.unavailable")).await;
    };
    if cx.guild_id() != Some(guild) {
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
