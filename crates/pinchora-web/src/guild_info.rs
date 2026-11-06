//! A server's channels and roles as the bot sees them: what the pickers list, and what the dashboard
//! checks a write against. Asked for with the bot's token and kept in Redis for a minute.

use std::time::Duration;

use axum::http::StatusCode;
use pinchora_core::settings::{self, FieldError, RoleFacts};
use serde::{Deserialize, Serialize};
use twilight_model::{
    channel::{Channel, ChannelType},
    guild::{Permissions, Role},
    id::Id,
};

use crate::{
    error::{ApiError, ApiResult},
    state::WebState,
};

/// How long the lists are trusted.
const TTL: Duration = Duration::from_secs(60);
/// How often one server's lists may be asked of Discord again before they run out.
const REFRESH_COOLDOWN: Duration = Duration::from_secs(10);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelInfo {
    pub id: String,
    pub name: String,
    /// Discord's channel type number: 0 text, 2 voice, 4 category, 5 announcement, 15 forum, ...
    #[serde(rename = "type")]
    pub kind: u8,
    pub parent_id: Option<String>,
    pub position: i32,
}

impl ChannelInfo {
    /// Text and announcement channels: where the bot posts welcome cards, level-ups and logs.
    pub fn takes_posts(&self) -> bool {
        matches!(
            ChannelType::from(self.kind),
            ChannelType::GuildText | ChannelType::GuildAnnouncement
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoleInfo {
    pub id: String,
    pub name: String,
    pub color: u32,
    pub position: i64,
    pub managed: bool,
    /// `@everyone`, whose id is the server's.
    pub everyone: bool,
    /// Whether the bot can hand it out; `why_not` says why not, as `/config level-roles add` would.
    pub assignable: bool,
    pub why_not: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BotInfo {
    /// The position of the bot's highest role; `None` when Discord would not say.
    pub top_role_position: Option<i64>,
    /// Whether the bot holds Manage Roles (or Administrator), without which no role can be given.
    pub manage_roles: bool,
}

/// What `GET /api/guilds/{g}/discord` answers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GuildInfo {
    pub channels: Vec<ChannelInfo>,
    pub roles: Vec<RoleInfo>,
    pub bot: BotInfo,
}

fn key(guild: u64) -> String {
    format!("web:guild-info:{guild}")
}

/// The lists, from Redis when fresh (`force` asks Discord again).
pub async fn get(state: &WebState, guild: u64, force: bool) -> ApiResult<GuildInfo> {
    let cache = &state.app.cache;
    if !force
        && let Some(raw) = cache.get(&key(guild)).await?
        && let Ok(info) = serde_json::from_str(&raw)
    {
        return Ok(info);
    }
    let info = fetch(state, guild).await?;
    let raw = serde_json::to_string(&info).map_err(anyhow::Error::from)?;
    cache.put(&key(guild), &raw, TTL).await?;
    Ok(info)
}

fn discord_down(error: impl std::fmt::Display) -> ApiError {
    tracing::warn!(%error, "Discord lookup for the dashboard failed");
    ApiError::new(
        StatusCode::BAD_GATEWAY,
        "discord",
        "Discord didn't answer just now. Try again in a moment.",
    )
}

async fn fetch(state: &WebState, guild: u64) -> ApiResult<GuildInfo> {
    let http = &state.app.http;
    let id = Id::new(guild.max(1));
    let bot = Id::new(state.app.application_id.get());
    let (channels, roles, me) = tokio::join!(http.guild_channels(id), http.roles(id), http.guild_member(id, bot));
    let channels: Vec<Channel> = channels.map_err(discord_down)?.models().await.map_err(discord_down)?;
    let roles: Vec<Role> = roles.map_err(discord_down)?.models().await.map_err(discord_down)?;
    let held = match me {
        Ok(response) => match response.model().await {
            Ok(member) => Some(member.roles),
            Err(error) => {
                tracing::warn!(%error, guild, "could not read the bot's member");
                None
            }
        },
        Err(error) => {
            tracing::warn!(%error, guild, "could not look up the bot's member");
            None
        }
    };
    let top = held.as_ref().map(|held| {
        roles
            .iter()
            .filter(|r| held.contains(&r.id))
            .map(|r| r.position)
            .max()
            .unwrap_or(0)
    });
    let manage_roles = held.as_ref().is_some_and(|held| {
        roles
            .iter()
            .filter(|r| r.id.get() == guild || held.contains(&r.id))
            .any(|r| {
                r.permissions
                    .intersects(Permissions::ADMINISTRATOR | Permissions::MANAGE_ROLES)
            })
    });

    let mut channels: Vec<ChannelInfo> = channels
        .into_iter()
        .filter(|c| !c.kind.is_thread())
        .map(|c| ChannelInfo {
            id: c.id.to_string(),
            name: c.name.unwrap_or_default(),
            kind: c.kind.into(),
            parent_id: c.parent_id.map(|p| p.to_string()),
            position: c.position.unwrap_or(0),
        })
        .collect();
    channels.sort_by_key(|c| (c.position, c.id.parse::<u64>().unwrap_or(0)));

    let mut roles: Vec<RoleInfo> = roles
        .into_iter()
        .map(|r| {
            let facts = RoleFacts {
                everyone: r.id.get() == guild,
                managed: r.managed,
                position: r.position,
            };
            let name = r.name.trim_start_matches('@').to_owned();
            let why_not = settings::role_refusal(facts, top, &format!("@{name}"));
            RoleInfo {
                id: r.id.to_string(),
                color: r.colors.primary_color,
                position: r.position,
                managed: r.managed,
                everyone: facts.everyone,
                assignable: why_not.is_none(),
                why_not,
                name,
            }
        })
        .collect();
    // Discord's order: the highest role first.
    roles.sort_by(|a, b| b.position.cmp(&a.position).then_with(|| a.id.cmp(&b.id)));
    Ok(GuildInfo {
        channels,
        roles,
        bot: BotInfo {
            top_role_position: top,
            manage_roles,
        },
    })
}

impl GuildInfo {
    pub fn channel(&self, id: u64) -> Option<&ChannelInfo> {
        let id = id.to_string();
        self.channels.iter().find(|c| c.id == id)
    }

    pub fn role(&self, id: u64) -> Option<&RoleInfo> {
        let id = id.to_string();
        self.roles.iter().find(|r| r.id == id)
    }

    /// A channel the bot posts into: it must exist and be a text or announcement channel.
    pub fn check_post_channel(&self, field: &'static str, id: u64) -> Result<(), FieldError> {
        match self.channel(id) {
            Some(c) if c.takes_posts() => Ok(()),
            Some(c) => Err(FieldError::new(
                field,
                format!("#{} isn't a text or announcement channel.", c.name),
            )),
            None => Err(gone_channel(field)),
        }
    }

    /// Any channel or category the bot can see, for XP multipliers.
    pub fn check_channel(&self, field: &'static str, id: u64) -> Result<&ChannelInfo, FieldError> {
        self.channel(id).ok_or_else(|| gone_channel(field))
    }

    pub fn check_role(&self, field: &'static str, id: u64) -> Result<&RoleInfo, FieldError> {
        self.role(id)
            .ok_or_else(|| FieldError::new(field, "That role no longer exists. Refresh the list."))
    }

    /// A role the bot must be able to hand out: the sentence is `/config level-roles add`'s.
    pub fn check_assignable(&self, field: &'static str, id: u64) -> Result<&RoleInfo, FieldError> {
        let role = self.check_role(field, id)?;
        match &role.why_not {
            Some(why) => Err(FieldError::new(field, why.clone())),
            None => Ok(role),
        }
    }
}

fn gone_channel(field: &'static str) -> FieldError {
    FieldError::new(
        field,
        "That channel no longer exists, or I can't see it. Refresh the list.",
    )
}

/// Whether the lists may be asked of Discord again now: once per [`REFRESH_COOLDOWN`] per server,
/// whoever asks (the pickers' refresh, or a write naming something the cached lists lack).
pub async fn may_refresh(state: &WebState, guild: u64) -> ApiResult<bool> {
    let key = format!("web:guild-info-refresh:{guild}");
    Ok(state.app.cache.try_cooldown(&key, REFRESH_COOLDOWN).await?.is_none())
}

/// The lists, asking Discord again when `wanted` is not on the cached ones (it may be brand new).
pub async fn get_with(state: &WebState, guild: u64, wanted: impl Fn(&GuildInfo) -> bool) -> ApiResult<GuildInfo> {
    let info = get(state, guild, false).await?;
    if wanted(&info) || !may_refresh(state, guild).await? {
        return Ok(info);
    }
    get(state, guild, true).await
}
