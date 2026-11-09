//! A server's channels and roles as the bot sees them: what the pickers list, and what the dashboard
//! checks a write against. Asked for with the bot's token and kept in Redis for a minute.

use std::time::Duration;

use axum::http::StatusCode;
use pinchora_core::admin::settings;
use pinchora_core::admin::settings::FieldError;
use pinchora_core::admin::settings::Grantor;
use pinchora_core::admin::settings::RoleFacts;
use serde::{Deserialize, Serialize};
use twilight_model::{
    channel::{Channel, ChannelType},
    guild::{Permissions, Role},
    id::Id,
};

use crate::{
    discord::ADMINISTRATOR,
    error::{ApiError, ApiResult},
    extract::ManagedGuild,
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
    /// The permission bits, as Discord sends them (a string: a browser number would lose bits).
    pub permissions: String,
    /// The [dangerous permissions](settings::DANGEROUS_PERMISSIONS) it has, by name.
    pub powers: Vec<String>,
    /// Whether it can be set up as a level, reputation, store or join role; `why_not` says why not, as
    /// `/config level-roles add` would. The cached copy knows only the bot's side; the pickers' answer
    /// adds the manager's ([`GuildInfo::for_grantor`]).
    pub assignable: bool,
    pub why_not: Option<String>,
    /// The same for a custom command's or auto-responder's role action, which never hands out a role
    /// with a dangerous permission.
    pub action_ok: bool,
    pub action_why_not: Option<String>,
}

impl RoleInfo {
    fn facts(&self) -> RoleFacts {
        RoleFacts {
            everyone: self.everyone,
            managed: self.managed,
            position: self.position,
            // Bits that cannot be read count as every permission.
            permissions: self.permissions.parse().unwrap_or(u64::MAX),
        }
    }

    fn label(&self) -> String {
        format!("@{}", self.name)
    }
}

/// Who is looking, for the pickers to grey out the roles they may not set up.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct YouInfo {
    pub owner: bool,
    pub administrator: bool,
    /// The position of their highest role; `None` when Discord would not say.
    pub top_role_position: Option<i64>,
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
    /// The manager asking; not in the cached copy.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub you: Option<YouInfo>,
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
    let (channels, roles) = tokio::join!(fetch_channels(state, guild), fetch_roles(state, guild));
    let (roles, bot) = roles?;
    Ok(GuildInfo {
        channels: channels?,
        roles,
        bot,
        you: None,
    })
}

/// The lists with the roles and the bot's own position taken straight from Discord, for a write that
/// hands a role out: the slash commands check the position live, so a save must not trust a copy up
/// to a minute old. The channels are the cached ones (asked for only when nothing is cached), and the
/// cached copy is left as it is.
pub async fn live(state: &WebState, guild: u64) -> ApiResult<GuildInfo> {
    let (roles, bot) = fetch_roles(state, guild).await?;
    let channels = match state.app.cache.get(&key(guild)).await? {
        Some(raw) => serde_json::from_str::<GuildInfo>(&raw)
            .map(|info| info.channels)
            .unwrap_or_default(),
        None => Vec::new(),
    };
    Ok(GuildInfo {
        channels,
        roles,
        bot,
        you: None,
    })
}

async fn fetch_channels(state: &WebState, guild: u64) -> ApiResult<Vec<ChannelInfo>> {
    let channels: Vec<Channel> = state
        .app
        .http
        .guild_channels(Id::new(guild.max(1)))
        .await
        .map_err(discord_down)?
        .models()
        .await
        .map_err(discord_down)?;
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
    Ok(channels)
}

/// The roles and where the bot's own highest role sits, as Discord has them now.
async fn fetch_roles(state: &WebState, guild: u64) -> ApiResult<(Vec<RoleInfo>, BotInfo)> {
    let http = &state.app.http;
    let id = Id::new(guild.max(1));
    let bot = Id::new(state.app.application_id.get());
    let (roles, me) = tokio::join!(http.roles(id), http.guild_member(id, bot));
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

    let mut roles: Vec<RoleInfo> = roles
        .into_iter()
        .map(|r| {
            let facts = RoleFacts {
                everyone: r.id.get() == guild,
                managed: r.managed,
                position: r.position,
                permissions: r.permissions.bits(),
            };
            let name = r.name.trim_start_matches('@').to_owned();
            let label = format!("@{name}");
            let why_not = settings::role_refusal(facts, top, &label);
            let action_why_not = why_not
                .clone()
                .or_else(|| settings::run_time_refusal(facts.permissions, &label));
            RoleInfo {
                id: r.id.to_string(),
                color: r.colors.primary_color,
                position: r.position,
                managed: r.managed,
                everyone: facts.everyone,
                permissions: facts.permissions.to_string(),
                powers: settings::dangerous_permissions(facts.permissions)
                    .into_iter()
                    .map(str::to_owned)
                    .collect(),
                assignable: why_not.is_none(),
                why_not,
                action_ok: action_why_not.is_none(),
                action_why_not,
                name,
            }
        })
        .collect();
    // Discord's order: the highest role first.
    roles.sort_by(|a, b| b.position.cmp(&a.position).then_with(|| a.id.cmp(&b.id)));
    Ok((
        roles,
        BotInfo {
            top_role_position: top,
            manage_roles,
        },
    ))
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
    fn check_assignable(&self, field: &'static str, id: u64) -> Result<&RoleInfo, FieldError> {
        let role = self.check_role(field, id)?;
        match settings::role_refusal(role.facts(), self.bot.top_role_position, &role.label()) {
            Some(why) => Err(FieldError::new(field, why)),
            None => Ok(role),
        }
    }

    /// A level, reputation, store or join role `grantor` sets up: the bot must be able to hand it out and
    /// the manager may set it up ([`settings::grant_refusal`]), as `/config` checks it.
    pub fn check_grant(&self, field: &'static str, id: u64, grantor: Grantor) -> Result<&RoleInfo, FieldError> {
        let role = self.check_assignable(field, id)?;
        match settings::grant_refusal(role.facts(), grantor, &role.label()) {
            Some(why) => Err(FieldError::new(field, why)),
            None => Ok(role),
        }
    }

    /// A role a custom action hands out or takes: [`GuildInfo::check_grant`] with the stricter rule of
    /// [`settings::action_role_refusal`].
    pub fn check_action_role(&self, field: &'static str, id: u64, grantor: Grantor) -> Result<&RoleInfo, FieldError> {
        let role = self.check_assignable(field, id)?;
        self.check_action_grant(field, role, grantor)
    }

    /// Only the manager's side of [`GuildInfo::check_action_role`], for a role a command already had: one
    /// the bot can no longer reach must not stop the command from being edited, but nobody may keep a
    /// role on it they could not put there.
    pub fn check_action_grant<'a>(
        &self,
        field: &'static str,
        role: &'a RoleInfo,
        grantor: Grantor,
    ) -> Result<&'a RoleInfo, FieldError> {
        match settings::action_role_refusal(role.facts(), grantor, &role.label()) {
            Some(why) => Err(FieldError::new(field, why)),
            None => Ok(role),
        }
    }

    /// The lists as `grantor` sees them in the pickers: every role marked with whether they may set it up,
    /// and why not.
    pub fn for_grantor(mut self, grantor: Grantor) -> Self {
        let top = self.bot.top_role_position;
        for role in &mut self.roles {
            let facts = role.facts();
            let label = role.label();
            role.why_not =
                settings::role_refusal(facts, top, &label).or_else(|| settings::grant_refusal(facts, grantor, &label));
            role.assignable = role.why_not.is_none();
            role.action_why_not = settings::role_refusal(facts, top, &label)
                .or_else(|| settings::action_role_refusal(facts, grantor, &label));
            role.action_ok = role.action_why_not.is_none();
        }
        self.you = Some(YouInfo {
            owner: grantor.owner,
            administrator: grantor.administrator,
            top_role_position: grantor.top_role,
        });
        self
    }
}

fn member_roles_key(guild: u64, user: u64) -> String {
    format!("web:member-roles:{guild}:{user}")
}

/// The roles the manager holds, from Discord with the bot's token (`fresh`), or from the minute-old copy
/// the last lookup left; `None` when Discord would not say.
async fn member_roles(state: &WebState, guild: u64, user: u64, fresh: bool) -> ApiResult<Option<Vec<String>>> {
    let cache = &state.app.cache;
    let key = member_roles_key(guild, user);
    if !fresh
        && let Some(raw) = cache.get(&key).await?
        && let Ok(roles) = serde_json::from_str(&raw)
    {
        return Ok(Some(roles));
    }
    let found = async {
        let member = state
            .app
            .http
            .guild_member(Id::new(guild.max(1)), Id::new(user.max(1)))
            .await?
            .model()
            .await?;
        anyhow::Ok(member.roles.iter().map(|r| r.to_string()).collect::<Vec<String>>())
    }
    .await;
    match found {
        Ok(roles) => {
            let raw = serde_json::to_string(&roles).map_err(anyhow::Error::from)?;
            cache.put(&key, &raw, TTL).await?;
            Ok(Some(roles))
        }
        Err(error) => {
            tracing::warn!(%error, guild, user, "could not look up the manager's roles");
            Ok(None)
        }
    }
}

/// The manager as a [`Grantor`]: owner and Administrator from their server list, their highest role from
/// their member in the server, placed among `roles`. A write asks Discord now (`fresh`), like the bot's own
/// position; the pickers may use a minute-old copy.
pub async fn grantor(state: &WebState, managed: &ManagedGuild, roles: &[RoleInfo], fresh: bool) -> ApiResult<Grantor> {
    let owner = managed.guild.owner;
    let bits: u64 = managed.guild.permissions.parse().unwrap_or(0);
    let top_role = if owner {
        None
    } else {
        member_roles(state, managed.id, managed.current.session.user_id, fresh)
            .await?
            .map(|held| {
                roles
                    .iter()
                    .filter(|r| held.contains(&r.id))
                    .map(|r| r.position)
                    .max()
                    .unwrap_or(0)
            })
    };
    Ok(Grantor {
        owner,
        administrator: owner || bits & ADMINISTRATOR != 0,
        top_role,
    })
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
