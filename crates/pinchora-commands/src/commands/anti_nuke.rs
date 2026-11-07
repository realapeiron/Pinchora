//! `/antinuke`: private, owner-controlled safety configuration.
use crate::{Cx, anti_nuke};
use pinchora_core::{anti_nuke as rules, t};
use twilight_interactions::command::{CommandModel, CreateCommand, ResolvedUser};
use twilight_model::{
    application::interaction::InteractionChannel,
    guild::{Permissions, Role},
};
fn manage_guild() -> Permissions {
    Permissions::MANAGE_GUILD
}
#[derive(CommandModel, CreateCommand)]
#[command(
    name = "antinuke",
    desc = "Protect the server, trusted staff and VIP members",
    default_permissions = "manage_guild",
    contexts = "guild"
)]
pub enum AntiNuke {
    #[command(name = "status")]
    Status(Status),
    #[command(name = "enable")]
    Enable(Enable),
    #[command(name = "disable")]
    Disable(Disable),
    #[command(name = "limits")]
    Limits(Limits),
    #[command(name = "trust")]
    Trust(Trust),
    #[command(name = "vip")]
    Vip(Vip),
    #[command(name = "events")]
    Events(Events),
}
#[derive(CommandModel, CreateCommand)]
#[command(name = "status", desc = "Show protection status and recovery limits")]
pub struct Status;
#[derive(CommandModel, CreateCommand)]
#[command(name = "enable", desc = "Enable protection; alert-only unless you choose a response")]
pub struct Enable {
    /// Alert channel (otherwise the moderation log or an owner DM)
    channel: Option<InteractionChannel>,
    /// alert, quarantine, kick or ban
    response: Option<String>,
    /// Safe role assigned after stripping the actor's roles
    quarantine: Option<Role>,
    /// Automatically restore recoverable damage
    restore: Option<bool>,
    /// Automatically unban protected VIP members
    revert_vip: Option<bool>,
    /// Remove bots added in an attack
    remove_bots: Option<bool>,
    /// Only the owner can change settings (trust always belongs to the owner)
    owner_only: Option<bool>,
}
#[derive(CommandModel, CreateCommand)]
#[command(name = "disable", desc = "Pause anti-nuke detection")]
pub struct Disable;
#[derive(CommandModel, CreateCommand)]
#[command(name = "events", desc = "Show recent anti-nuke incidents")]
pub struct Events;
#[derive(CommandModel, CreateCommand)]
#[command(name = "limits", desc = "Set the maximum actions allowed per actor in a time window")]
pub struct Limits {
    /// Action key, for example channel_delete, ban, permission_grant or bot_add
    kind: String,
    /// Maximum allowed before a response (1 to 100)
    #[command(min_value = 1, max_value = 100)]
    count: i64,
    /// Window in seconds (5 to 300)
    #[command(min_value = 5, max_value = 300)]
    seconds: i64,
}
#[derive(CommandModel, CreateCommand)]
#[command(name = "trust", desc = "Owner-managed trusted users; nobody may trust themselves")]
pub enum Trust {
    #[command(name = "add")]
    Add(TrustAdd),
    #[command(name = "remove")]
    Remove(TrustRemove),
}
#[derive(CommandModel, CreateCommand)]
#[command(name = "add", desc = "Trust another member (owner only)")]
pub struct TrustAdd {
    /// Member to trust
    user: ResolvedUser,
}
#[derive(CommandModel, CreateCommand)]
#[command(name = "remove", desc = "Remove trust from a member (owner only)")]
pub struct TrustRemove {
    /// Member to remove
    user: ResolvedUser,
}
#[derive(CommandModel, CreateCommand)]
#[command(name = "vip", desc = "Protect members holding a VIP role")]
pub enum Vip {
    #[command(name = "add")]
    Add(VipAdd),
    #[command(name = "remove")]
    Remove(VipRemove),
}
#[derive(CommandModel, CreateCommand)]
#[command(name = "add", desc = "Protect a VIP role")]
pub struct VipAdd {
    /// Role to protect
    role: Role,
}
#[derive(CommandModel, CreateCommand)]
#[command(name = "remove", desc = "Remove VIP protection from a role")]
pub struct VipRemove {
    /// Protected role
    role: Role,
}
impl AntiNuke {
    pub async fn run(self, cx: &Cx) -> anyhow::Result<()> {
        let Some(guild) = cx.guild_id().filter(|_| cx.can_manage_guild()) else {
            return cx.reply_ephemeral(t!(cx.lang(), "anti_nuke.manage")).await;
        };
        let actor = cx.author()?.id.get();
        if let Err(why) = anti_nuke::authorize_antinuke(&cx.app, guild, actor).await {
            return cx.reply_ephemeral(why.to_string()).await;
        }
        let mut c = cx.app.db.antinuke_config(guild).await?;
        match self {
            Self::Status(_) => {
                return cx
                    .reply_ephemeral(t!(
                        cx.lang(),
                        "anti_nuke.status",
                        enabled = t!(cx.lang(), if c.enabled { "anti_nuke.on" } else { "anti_nuke.off" }),
                        response = t!(cx.lang(), &format!("anti_nuke.response.{}", c.response)),
                        trusted = c.trusted_users.len(),
                        vip = c.vip_roles.len(),
                        max = rules::MAX_LIST
                    ))
                    .await;
            }
            Self::Events(_) => {
                let events = cx.app.db.antinuke_events(guild).await?;
                let lines = events
                    .iter()
                    .take(10)
                    .map(|e| {
                        t!(
                            cx.lang(),
                            "anti_nuke.event_line",
                            id = e.id,
                            actor = e.actor_id,
                            kind = t!(cx.lang(), &format!("anti_nuke.kind.{}", e.kind)),
                            count = e.count,
                            action = if e.action_taken == "observed" {
                                t!(cx.lang(), "anti_nuke.observed")
                            } else {
                                crate::util::strip_mentions(&e.action_taken)
                            }
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n");
                return cx
                    .reply_ephemeral(if lines.is_empty() {
                        t!(cx.lang(), "anti_nuke.empty")
                    } else {
                        lines.chars().take(1900).collect()
                    })
                    .await;
            }
            Self::Enable(v) => {
                c.enabled = true;
                if let Some(v) = v.channel {
                    c.log_channel_id = Some(v.id.to_string());
                }
                if let Some(v) = v.response {
                    c.response = v;
                }
                if let Some(v) = v.quarantine {
                    c.quarantine_role_id = Some(v.id.to_string());
                }
                if let Some(v) = v.restore {
                    c.auto_restore = v;
                }
                if let Some(v) = v.revert_vip {
                    c.revert_vip_bans = v;
                }
                if let Some(v) = v.remove_bots {
                    c.remove_bots = v;
                }
                if let Some(v) = v.owner_only {
                    c.owner_only_config = v;
                }
            }
            Self::Disable(_) => c.enabled = false,
            Self::Limits(v) => {
                let Some(limit) = c.limits.get_mut(&v.kind) else {
                    return cx.reply_ephemeral(t!(cx.lang(), "anti_nuke.invalid")).await;
                };
                limit.count = v.count as u32;
                limit.window_secs = v.seconds as u32;
            }
            Self::Trust(v) => {
                let (user, add) = match v {
                    Trust::Add(v) => (v.user.resolved.id.to_string(), true),
                    Trust::Remove(v) => (v.user.resolved.id.to_string(), false),
                };
                c.trusted_users.retain(|id| *id != user);
                if add {
                    c.trusted_users.push(user);
                }
            }
            Self::Vip(v) => {
                let (role, add) = match v {
                    Vip::Add(v) => (v.role.id.to_string(), true),
                    Vip::Remove(v) => (v.role.id.to_string(), false),
                };
                c.vip_roles.retain(|id| *id != role);
                if add {
                    c.vip_roles.push(role);
                }
            }
        }
        if let Err(why) = anti_nuke::save_antinuke_config(&cx.app, guild, actor, &c).await {
            return cx.reply_ephemeral(why.to_string()).await;
        }
        cx.reply_ephemeral(t!(cx.lang(), "anti_nuke.saved")).await
    }
}
pub(crate) async fn on_component(cx: &Cx, prefix: &str, mut parts: std::str::Split<'_, char>) -> anyhow::Result<()> {
    let action = parts.next().unwrap_or("");
    let id = parts
        .next()
        .filter(|s| rules::valid_id(s))
        .and_then(|s| s.parse::<i64>().ok());
    let raw_guild = parts.next();
    let named_guild = raw_guild
        .filter(|s| rules::valid_id(s))
        .and_then(|s| s.parse::<u64>().ok());
    if parts.next().is_some() || (raw_guild.is_some() && named_guild.is_none()) {
        return cx.reply_ephemeral(t!(cx.lang(), "anti_nuke.invalid")).await;
    }
    let Some(guild) = cx.guild_id().or(named_guild) else {
        return cx.reply_ephemeral(t!(cx.lang(), "anti_nuke.manage")).await;
    };
    if named_guild.is_some_and(|g| g != guild) {
        return cx.reply_ephemeral(t!(cx.lang(), "anti_nuke.not_found")).await;
    }
    let actor = cx.author()?.id.get();
    if let Err(why) = anti_nuke::authorize_antinuke(&cx.app, guild, actor).await {
        return cx.reply_ephemeral(why.to_string()).await;
    }
    let Some(id) = id else {
        return cx.reply_ephemeral(t!(cx.lang(), "anti_nuke.invalid")).await;
    };
    if matches!((prefix, action), ("an", "undo") | ("vp", "roles")) {
        if cx.app.db.antinuke_event(guild, id).await?.is_none() {
            return cx.reply_ephemeral(t!(cx.lang(), "anti_nuke.not_found")).await;
        }
        return cx
            .reply(twilight_model::http::interaction::InteractionResponseData {
                content: Some(t!(cx.lang(), "anti_nuke.ui.confirm_hint")),
                flags: Some(twilight_model::channel::message::MessageFlags::EPHEMERAL),
                allowed_mentions: Some(twilight_model::channel::message::AllowedMentions::default()),
                components: Some(vec![twilight_model::channel::message::Component::ActionRow(
                    twilight_model::channel::message::component::ActionRow {
                        id: None,
                        components: vec![crate::server_events::button(
                            format!("{prefix}:confirm:{id}:{guild}"),
                            &t!(cx.lang(), "anti_nuke.ui.confirm"),
                            None,
                            twilight_model::channel::message::component::ButtonStyle::Danger,
                            false,
                        )],
                    },
                )]),
                ..Default::default()
            })
            .await;
    }
    let result = match (prefix, action) {
        ("an", "confirm") => anti_nuke::restore_antinuke(&cx.app, guild, id, actor, false).await,
        ("vp", "confirm") => anti_nuke::restore_antinuke_actor_roles(&cx.app, guild, id, actor).await,
        _ => Err(anyhow::anyhow!(t!(cx.lang(), "anti_nuke.invalid"))),
    };
    cx.reply_ephemeral(result.unwrap_or_else(|e| e.to_string())).await
}
