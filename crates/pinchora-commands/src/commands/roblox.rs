//! Member identity and Manage Server configuration for Roblox.
use crate::{Cx, roblox::RobloxApi, roles};
use pinchora_core::{roblox, t};
use twilight_interactions::command::{CommandModel, CommandOption, CreateCommand, CreateOption, ResolvedUser};
use twilight_model::{
    channel::message::{
        Component,
        component::{ActionRow, Button, ButtonStyle},
    },
    guild::{Permissions, Role},
    http::interaction::InteractionResponseData,
    id::Id,
};
use twilight_util::builder::embed::{EmbedBuilder, ImageSource};

fn manage_guild() -> Permissions {
    Permissions::MANAGE_GUILD
}
#[derive(CommandModel, CreateCommand)]
#[command(name = "roblox", desc = "Link and update your Roblox identity", contexts = "guild")]
pub enum RobloxCommand {
    #[command(name = "verify")]
    Verify(Verify),
    #[command(name = "unverify")]
    Unverify(Unverify),
    #[command(name = "whois")]
    Whois(Whois),
    #[command(name = "update")]
    Update(Update),
}
#[derive(CommandModel, CreateCommand)]
#[command(name = "verify", desc = "Start Roblox profile-code verification")]
pub struct Verify {
    /// Roblox username
    username: String,
}
#[derive(CommandModel, CreateCommand)]
#[command(name = "unverify", desc = "Remove your Roblox link")]
pub struct Unverify;
#[derive(CommandModel, CreateCommand)]
#[command(name = "whois", desc = "See a member's linked Roblox account")]
pub struct Whois {
    /// Discord member
    user: Option<ResolvedUser>,
}
#[derive(CommandModel, CreateCommand)]
#[command(name = "update", desc = "Refresh Roblox roles and nickname")]
pub struct Update {
    /// Member to refresh (requires Manage Server)
    user: Option<ResolvedUser>,
}

#[derive(CommandOption, CreateOption, Clone, Copy)]
pub enum BindKind {
    #[option(name = "Verified account", value = "verified")]
    Verified,
    #[option(name = "Group member", value = "group_member")]
    GroupMember,
    #[option(name = "Group rank", value = "group_rank")]
    GroupRank,
}
impl BindKind {
    fn key(self) -> &'static str {
        match self {
            Self::Verified => "verified",
            Self::GroupMember => "group_member",
            Self::GroupRank => "group_rank",
        }
    }
}
#[derive(CommandModel, CreateCommand)]
#[command(
    name = "robloxadmin",
    desc = "Configure Roblox verification and role binds",
    default_permissions = "manage_guild",
    contexts = "guild"
)]
pub enum RobloxAdmin {
    #[command(name = "settings")]
    Settings(AdminSettings),
    #[command(name = "bind-add")]
    BindAdd(BindAdd),
    #[command(name = "bind-remove")]
    BindRemove(BindRemove),
    #[command(name = "bind-list")]
    BindList(BindList),
}
#[derive(CommandModel, CreateCommand)]
#[command(name = "settings", desc = "Set Roblox roles, nickname and join policy")]
pub struct AdminSettings {
    /// Enable Roblox integration
    enabled: Option<bool>,
    /// Role for linked members
    verified_role: Option<Role>,
    /// Role for unlinked members
    unverified_role: Option<Role>,
    /// Nickname with {roblox-name} and {discord-name}; use 'off' to clear
    nickname_template: Option<String>,
    /// Channel where members verify
    verify_channel: Option<Id<twilight_model::id::marker::ChannelMarker>>,
    /// Hold unverified newcomers
    require_verified: Option<bool>,
}
#[derive(CommandModel, CreateCommand)]
#[command(name = "bind-add", desc = "Assign a safe role for a Roblox group rank")]
pub struct BindAdd {
    /// Condition
    kind: BindKind,
    /// Role to grant
    role: Role,
    /// Roblox group ID
    group: Option<i64>,
    /// Inclusive minimum rank
    rank_min: Option<i64>,
    /// Inclusive maximum rank
    rank_max: Option<i64>,
    /// Remove role when condition stops matching
    remove_if_unmet: Option<bool>,
}
#[derive(CommandModel, CreateCommand)]
#[command(name = "bind-remove", desc = "Remove a Roblox bind")]
pub struct BindRemove {
    /// Bind number from bind-list
    id: i64,
}
#[derive(CommandModel, CreateCommand)]
#[command(name = "bind-list", desc = "List Roblox role binds")]
pub struct BindList;

impl RobloxCommand {
    pub async fn run(self, cx: &Cx) -> anyhow::Result<()> {
        let Some(guild) = cx.guild_id() else {
            return cx.reply_ephemeral(t!(cx.lang(), "roblox.guild-only")).await;
        };
        let me = cx.author()?.id.get();
        let _lease = if matches!(&self, Self::Verify(_) | Self::Unverify(_)) {
            let Some(lease) = cx
                .app
                .cache
                .verification_lease(
                    &format!("roblox:user:{me}"),
                    &format!("{:032x}", rand::random::<u128>()),
                )
                .await?
            else {
                return cx.reply_ephemeral(t!(cx.lang(), "roblox.busy")).await;
            };
            Some(lease)
        } else {
            None
        };
        match self {
            Self::Verify(command) => {
                if cx.app.db.roblox_link(me).await?.is_some() {
                    return cx.reply_ephemeral(t!(cx.lang(), "roblox.already-linked")).await;
                }
                if cx
                    .app
                    .cache
                    .try_cooldown(&format!("roblox:verify:{me}"), std::time::Duration::from_secs(60))
                    .await?
                    .is_some()
                {
                    return cx.reply_ephemeral(t!(cx.lang(), "roblox.verify-cooldown")).await;
                }
                let (id, name) = match RobloxApi::with_cache(&cx.app.cache).lookup(&command.username).await {
                    Ok(v) => v,
                    Err(_) => return cx.reply_ephemeral(t!(cx.lang(), "roblox.not-found")).await,
                };
                if cx.app.db.roblox_link_by_id(id).await?.is_some() {
                    return cx.reply_ephemeral(t!(cx.lang(), "roblox.taken")).await;
                }
                let code = format!("pinchora-{me:x}-{id:x}-{:032x}", rand::random::<u128>());
                cx.app.db.roblox_start(me, id, &name, &code).await?;
                let button = Component::ActionRow(ActionRow {
                    id: None,
                    components: vec![Component::Button(Button {
                        id: None,
                        custom_id: Some("rx:check".into()),
                        disabled: false,
                        emoji: None,
                        label: Some(t!(cx.lang(), "roblox.check")),
                        style: ButtonStyle::Primary,
                        url: None,
                        sku_id: None,
                    })],
                });
                cx.reply(InteractionResponseData {
                    content: Some(t!(cx.lang(), "roblox.instructions", name = name, code = code)),
                    components: Some(vec![button]),
                    flags: Some(twilight_model::channel::message::MessageFlags::EPHEMERAL),
                    ..Default::default()
                })
                .await
            }
            Self::Unverify(_) => {
                cx.app.db.roblox_unlink(me).await?;
                cx.app.db.roblox_queue_linked_guilds(me).await?;
                cx.app.db.roblox_queue(guild, me).await?;
                cx.reply_ephemeral(t!(cx.lang(), "roblox.unlinked")).await
            }
            Self::Whois(command) => {
                let user = command.user.map_or(me, |u| u.resolved.id.get());
                if user != me && super::privacy::private_profile(cx, user).await? {
                    return Ok(());
                }
                match cx.app.db.roblox_link(user).await? {
                    Some(mut link) => {
                        match RobloxApi::with_cache(&cx.app.cache)
                            .profile(link.roblox_id as u64)
                            .await
                        {
                            Ok(Some(profile)) if !profile.is_banned => {
                                cx.app
                                    .db
                                    .roblox_refresh_name(user, link.roblox_id as u64, &profile.name)
                                    .await?;
                                link.roblox_name = profile.name;
                            }
                            Ok(_) => return cx.reply_ephemeral(t!(cx.lang(), "roblox.no-link")).await,
                            Err(_) => return cx.reply_ephemeral(t!(cx.lang(), "roblox.api-error")).await,
                        }
                        let url = format!("https://www.roblox.com/users/{}/profile", link.roblox_id);
                        let headshot = RobloxApi::with_cache(&cx.app.cache)
                            .headshot(link.roblox_id as u64)
                            .await
                            .ok()
                            .flatten();
                        let mut card = EmbedBuilder::new()
                            .title(&link.roblox_name)
                            .description(t!(
                                cx.lang(),
                                "roblox.whois",
                                user = user,
                                name = link.roblox_name,
                                id = link.roblox_id
                            ))
                            .url(url);
                        if let Some(headshot) = headshot.as_deref().and_then(|url| ImageSource::url(url).ok()) {
                            card = card.thumbnail(headshot);
                        }
                        cx.reply(InteractionResponseData {
                            embeds: Some(vec![card.build()]),
                            flags: Some(twilight_model::channel::message::MessageFlags::EPHEMERAL),
                            ..Default::default()
                        })
                        .await
                    }
                    None => cx.reply_ephemeral(t!(cx.lang(), "roblox.no-link")).await,
                }
            }
            Self::Update(command) => {
                let user = command.user.map_or(me, |u| u.resolved.id.get());
                if user != me && !cx.can_manage_guild() {
                    return cx.reply_ephemeral(t!(cx.lang(), "roblox.manage")).await;
                }
                // Manual requests have their own fleet-wide limit, independent of the scheduled queue.
                if cx
                    .app
                    .cache
                    .try_cooldown(
                        &format!("roblox:update:{guild}:{user}"),
                        std::time::Duration::from_secs(60),
                    )
                    .await?
                    .is_some()
                {
                    return cx.reply_ephemeral(t!(cx.lang(), "roblox.cooldown")).await;
                }
                match crate::roblox::sync(&cx.app, guild, user).await {
                    Ok(_) => cx.reply_ephemeral(t!(cx.lang(), "roblox.updated")).await,
                    Err(error) => {
                        tracing::warn!(%error,"Roblox manual update failed");
                        cx.reply_ephemeral(t!(cx.lang(), "roblox.update-failed")).await
                    }
                }
            }
        }
    }
}

impl RobloxAdmin {
    pub async fn run(self, cx: &Cx) -> anyhow::Result<()> {
        let Some(guild) = cx.guild_id() else {
            return cx.reply_ephemeral(t!(cx.lang(), "roblox.guild-only")).await;
        };
        if !cx.can_manage_guild() {
            return cx.reply_ephemeral(t!(cx.lang(), "roblox.manage")).await;
        }
        match self {
            Self::Settings(change) => {
                for role in [&change.verified_role, &change.unverified_role].into_iter().flatten() {
                    if let Some(why) = roles::unassignable(cx, guild, role).await.or_else(|| {
                        pinchora_core::settings::run_time_refusal_in(cx.lang(), role.permissions.bits(), "Roblox role")
                    }) {
                        return cx.reply_ephemeral(why).await;
                    }
                }
                let mut saved = cx.app.db.roblox_settings(guild).await?;
                if let Some(v) = change.enabled {
                    saved.enabled = v;
                }
                if let Some(v) = change.verified_role {
                    saved.verified_role_id = Some(v.id.get() as i64);
                }
                if let Some(v) = change.unverified_role {
                    saved.unverified_role_id = Some(v.id.get() as i64);
                }
                if let Some(v) = change.nickname_template {
                    saved.nickname_template = if v == "off" { None } else { Some(v) };
                }
                if let Some(v) = change.verify_channel {
                    let channel = cx.app.http.channel(v).await?.model().await?;
                    if channel.guild_id != Some(Id::new(guild))
                        || !matches!(
                            channel.kind,
                            twilight_model::channel::ChannelType::GuildText
                                | twilight_model::channel::ChannelType::GuildAnnouncement
                        )
                    {
                        return cx.reply_ephemeral(t!(cx.lang(), "roblox.web.channel")).await;
                    }
                    saved.verify_channel_id = Some(v.get() as i64);
                }
                if let Some(v) = change.require_verified {
                    saved.require_verified = v;
                }
                if !crate::roblox::valid_settings(&saved) {
                    return cx.reply_ephemeral(t!(cx.lang(), "roblox.bad-template")).await;
                }
                if saved.verified_role_id.is_some() && saved.verified_role_id == saved.unverified_role_id {
                    return cx.reply_ephemeral(t!(cx.lang(), "roblox.bad-bind")).await;
                }
                cx.app.db.roblox_save_settings(guild, &saved).await?;
                cx.reply_ephemeral(t!(cx.lang(), "roblox.saved")).await
            }
            Self::BindAdd(change) => {
                if let Some(why) = roles::unassignable(cx, guild, &change.role).await.or_else(|| {
                    pinchora_core::settings::run_time_refusal_in(
                        cx.lang(),
                        change.role.permissions.bits(),
                        "Roblox role",
                    )
                }) {
                    return cx.reply_ephemeral(why).await;
                }
                let group = change.group.and_then(|n| u64::try_from(n).ok()).filter(|n| *n > 0);
                let min = change.rank_min.and_then(|n| i16::try_from(n).ok());
                let max = change.rank_max.and_then(|n| i16::try_from(n).ok());
                let valid = match change.kind {
                    BindKind::Verified => {
                        change.group.is_none() && change.rank_min.is_none() && change.rank_max.is_none()
                    }
                    BindKind::GroupMember => group.is_some() && change.rank_min.is_none() && change.rank_max.is_none(),
                    BindKind::GroupRank => {
                        group.is_some()
                            && min.is_some_and(|v| (1..=255).contains(&v))
                            && max.is_some_and(|v| v >= min.unwrap_or(256) && v <= 255)
                    }
                };
                if !valid {
                    return cx.reply_ephemeral(t!(cx.lang(), "roblox.bad-bind")).await;
                }
                let id = cx
                    .app
                    .db
                    .roblox_add_bind(
                        guild,
                        change.kind.key(),
                        group,
                        min,
                        max,
                        change.role.id.get(),
                        change.remove_if_unmet.unwrap_or(true),
                    )
                    .await?;
                cx.reply_ephemeral(t!(cx.lang(), "roblox.bind-added", id = id)).await
            }
            Self::BindRemove(change) => {
                if cx.app.db.roblox_remove_bind(guild, change.id).await? {
                    cx.reply_ephemeral(t!(cx.lang(), "roblox.bind-removed")).await
                } else {
                    cx.reply_ephemeral(t!(cx.lang(), "roblox.bind-missing")).await
                }
            }
            Self::BindList(_) => {
                let binds = cx.app.db.roblox_binds(guild).await?;
                let text = if binds.is_empty() {
                    t!(cx.lang(), "roblox.bind-empty")
                } else {
                    binds
                        .iter()
                        .take(25)
                        .map(|b| {
                            let kind = t!(cx.lang(), &format!("roblox.kind.{}", b.kind));
                            t!(
                                cx.lang(),
                                "roblox.bind-line",
                                id = b.id,
                                kind = kind,
                                group = b.group_id.map_or_else(|| "–".to_owned(), |v| v.to_string()),
                                min = b.rank_min.map_or_else(|| "–".to_owned(), |v| v.to_string()),
                                max = b.rank_max.map_or_else(|| "–".to_owned(), |v| v.to_string()),
                                role = b.role_id
                            )
                        })
                        .collect::<Vec<_>>()
                        .join("\n")
                };
                cx.reply(InteractionResponseData {
                    embeds: Some(vec![EmbedBuilder::new().description(text).build()]),
                    flags: Some(twilight_model::channel::message::MessageFlags::EPHEMERAL),
                    ..Default::default()
                })
                .await
            }
        }
    }
}

pub async fn on_component(cx: &Cx, mut parts: std::str::Split<'_, char>) -> anyhow::Result<()> {
    if parts.next() != Some("check") {
        return Ok(());
    }
    let user = cx.author()?.id.get();
    let Some(_lease) = cx
        .app
        .cache
        .verification_lease(
            &format!("roblox:user:{user}"),
            &format!("{:032x}", rand::random::<u128>()),
        )
        .await?
    else {
        return cx.reply_ephemeral(t!(cx.lang(), "roblox.busy")).await;
    };
    let Some(mut pending) = cx.app.db.roblox_pending(user).await? else {
        return cx.reply_ephemeral(t!(cx.lang(), "roblox.expired")).await;
    };
    if !cx.app.db.roblox_check_attempt(user, &pending.code).await? {
        return cx.reply_ephemeral(t!(cx.lang(), "roblox.check-cooldown")).await;
    }
    let profile = match RobloxApi::with_cache(&cx.app.cache)
        .profile(pending.roblox_id as u64)
        .await
    {
        Ok(Some(p)) if !p.is_banned => p,
        _ => return cx.reply_ephemeral(t!(cx.lang(), "roblox.api-error")).await,
    };
    if !roblox::description_has_code(&profile.description, &pending.code) {
        return cx.reply_ephemeral(t!(cx.lang(), "roblox.code-missing")).await;
    }
    pending.roblox_name = profile.name;
    if !cx.app.db.roblox_complete(user, &pending).await? {
        return cx.reply_ephemeral(t!(cx.lang(), "roblox.taken")).await;
    }
    cx.app.db.roblox_queue_linked_guilds(user).await?;
    if let Some(guild) = cx.guild_id() {
        cx.app.db.roblox_queue(guild, user).await?;
    }
    if let Some(guild) = cx.guild_id()
        && let Err(error) = crate::roblox::release_hold(&cx.app, guild, user).await
    {
        tracing::warn!(guild,user,%error,"Roblox join release failed");
    }
    cx.reply_ephemeral(t!(cx.lang(), "roblox.verified", name = pending.roblox_name))
        .await
}
