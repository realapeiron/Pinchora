//! `/automod`: management is always checked on the server, including alert clicks.
use crate::{Cx, automod::actions};
use pinchora_core::{
    automod::{self, Action, Input},
    t,
};
use std::str::Split;
use twilight_interactions::command::{CommandModel, CreateCommand};
use twilight_model::{
    channel::message::{
        Component, MessageFlags,
        component::{ActionRow, SelectMenu, SelectMenuType},
    },
    guild::{Permissions, Role},
    http::interaction::InteractionResponseData,
};
fn manage_guild() -> Permissions {
    Permissions::MANAGE_GUILD
}
#[derive(CommandModel, CreateCommand)]
#[command(
    name = "automod",
    desc = "Automatic message rules, profiles and safe testing",
    default_permissions = "manage_guild",
    contexts = "guild"
)]
pub enum AutoMod {
    #[command(name = "show")]
    Show(Show),
    #[command(name = "profile")]
    Profile(Profile),
    #[command(name = "toggle")]
    Toggle(Toggle),
    #[command(name = "exempt")]
    Exempt(Exempt),
    #[command(name = "test")]
    Test(Test),
}
#[derive(CommandModel, CreateCommand)]
#[command(name = "show", desc = "Show the AutoMod profile and active rules")]
pub struct Show;
#[derive(CommandModel, CreateCommand)]
#[command(name = "profile", desc = "Apply relaxed, standard (48h observe), strict or off")]
pub struct Profile {
    /// relaxed, standard, strict or off
    name: String,
}
#[derive(CommandModel, CreateCommand)]
#[command(name = "toggle", desc = "Turn a rule on or off")]
pub struct Toggle {
    /// Rule name or id
    rule: String,
    /// Whether this rule is enabled
    enabled: bool,
}
#[derive(CommandModel, CreateCommand)]
#[command(name = "test", desc = "Test text privately without acting or adding strikes")]
pub struct Test {
    /// Text to check
    #[command(max_length = 2000)]
    text: String,
}
#[derive(CommandModel, CreateCommand)]
#[command(name = "exempt", desc = "Exclude a role or channel from AutoMod")]
pub enum Exempt {
    #[command(name = "role")]
    Role(ExemptRole),
    #[command(name = "channel")]
    Channel(ExemptChannel),
}
#[derive(CommandModel, CreateCommand)]
#[command(name = "role", desc = "Add or remove an exempt role")]
pub struct ExemptRole {
    /// add or remove
    action: String,
    /// Exempt role
    role: Role,
    /// Rule name (omit for all rules)
    rule: Option<String>,
}
#[derive(CommandModel, CreateCommand)]
#[command(name = "channel", desc = "Add or remove an exempt channel")]
pub struct ExemptChannel {
    /// add or remove
    action: String,
    /// Exempt channel
    channel: twilight_model::application::interaction::InteractionChannel,
    /// Rule name (omit for all rules)
    rule: Option<String>,
}

async fn managed(cx: &Cx) -> anyhow::Result<Option<u64>> {
    if !cx.can_manage_guild() || cx.guild_id().is_none() {
        cx.reply_ephemeral(t!(cx.lang(), "automod.manage")).await?;
        return Ok(None);
    }
    Ok(cx.guild_id())
}
impl AutoMod {
    pub async fn run(self, cx: &Cx) -> anyhow::Result<()> {
        let Some(guild) = managed(cx).await? else {
            return Ok(());
        };
        let mut config = cx.app.db.automod_config(guild).await?;
        match self {
            Self::Show(_) => {
                let list = config
                    .rules
                    .iter()
                    .filter(|r| r.enabled)
                    .map(|r| r.name.clone())
                    .collect::<Vec<_>>()
                    .join(", ");
                return cx
                    .reply_ephemeral(t!(
                        cx.lang(),
                        "automod.show",
                        profile = config.profile,
                        rules = list,
                        count = config.rules.len(),
                        max = automod::MAX_RULES,
                        observation = config.observe_until.map_or_else(
                            || t!(cx.lang(), "automod.no_observation"),
                            |d| format!("<t:{}:R>", d.timestamp())
                        )
                    ))
                    .await;
            }
            Self::Profile(p) => {
                let Some(next) = automod::apply_profile(&config, &p.name, cx.app.now()) else {
                    return cx.reply_ephemeral(t!(cx.lang(), "automod.invalid_profile")).await;
                };
                config = next;
            }
            Self::Toggle(v) => {
                let Some(rule) = config
                    .rules
                    .iter_mut()
                    .find(|r| r.name == v.rule || r.id.as_deref() == Some(&v.rule))
                else {
                    return cx.reply_ephemeral(t!(cx.lang(), "automod.not_found")).await;
                };
                if v.enabled && rule.needs_content() && !cx.app.message_content && !(rule.native && rule.native_kind())
                {
                    return cx.reply_ephemeral(t!(cx.lang(), "automod.content_locked")).await;
                }
                rule.enabled = v.enabled;
            }
            Self::Test(v) => {
                let channels: Vec<String> = cx.interaction.channel.iter().map(|c| c.id.to_string()).collect();
                let roles: Vec<String> = cx
                    .interaction
                    .member
                    .iter()
                    .flat_map(|m| &m.roles)
                    .map(ToString::to_string)
                    .collect();
                let hits = config
                    .rules
                    .iter()
                    .filter(|r| r.applies(&channels, &roles) && (!r.needs_content() || cx.app.message_content))
                    .filter(|r| {
                        automod::matches(
                            r,
                            &Input {
                                content: &v.text,
                                ..Input::default()
                            },
                        )
                    })
                    .map(|r| r.name.clone())
                    .collect::<Vec<_>>();
                return cx
                    .reply_ephemeral(t!(
                        cx.lang(),
                        "automod.test_result",
                        rules = if hits.is_empty() {
                            t!(cx.lang(), "automod.no_matches")
                        } else {
                            hits.join(", ")
                        },
                        note = t!(cx.lang(), "automod.test_note")
                    ))
                    .await;
            }
            Self::Exempt(v) => {
                let (role, id, action, name) = match v {
                    Exempt::Role(v) => (true, v.role.id.to_string(), v.action, v.rule),
                    Exempt::Channel(v) => (false, v.channel.id.to_string(), v.action, v.rule),
                };
                if !matches!(action.as_str(), "add" | "remove") {
                    return cx.reply_ephemeral(t!(cx.lang(), "automod.invalid")).await;
                }
                let mut changed = false;
                for r in &mut config.rules {
                    if name.as_ref().is_none_or(|n| r.name == *n) {
                        let list = if role {
                            &mut r.exempt_roles
                        } else {
                            &mut r.exempt_channels
                        };
                        list.retain(|v| *v != id);
                        if action == "add" {
                            list.push(id.clone());
                        }
                        changed = true;
                    }
                }
                if !changed {
                    return cx.reply_ephemeral(t!(cx.lang(), "automod.not_found")).await;
                }
            }
        }
        if let Err(why) = automod::validate(&config, cx.lang()) {
            return cx.reply_ephemeral(why).await;
        }
        cx.app.db.save_automod(guild, cx.author()?.id.get(), &config).await?;
        cx.app.forget_automod_config(guild).await;
        cx.app
            .cache
            .publish_invalidation(pinchora_cache::Invalidation::Guild(guild))
            .await?;
        cx.reply_ephemeral(t!(cx.lang(), "automod.saved")).await
    }
}

pub(crate) async fn on_component(
    cx: &Cx,
    prefix: &str,
    mut parts: Split<'_, char>,
    values: &[String],
) -> anyhow::Result<()> {
    let Some(guild) = managed(cx).await? else {
        return Ok(());
    };
    let action = parts.next().unwrap_or("");
    let first = parts.next().and_then(|s| s.parse::<i64>().ok()).unwrap_or(0);
    let id = if prefix == "ax" {
        parts.next().and_then(|s| s.parse::<i64>().ok()).unwrap_or(0)
    } else {
        first
    };
    let Some(hit) = cx.app.db.automod_hit(guild, id).await? else {
        return cx.reply_ephemeral(t!(cx.lang(), "automod.not_found")).await;
    };
    let mut config = cx.app.db.automod_config(guild).await?;
    let Some(index) = config
        .rules
        .iter()
        .position(|r| r.id.as_deref() == hit.rule_id.map(|i| i.to_string()).as_deref())
    else {
        return cx.reply_ephemeral(t!(cx.lang(), "automod.not_found")).await;
    };
    if prefix == "ax" {
        if hit.rule_id != Some(first) {
            return cx.reply_ephemeral(t!(cx.lang(), "automod.not_found")).await;
        }
        match action {
            "c" => {
                let value = hit.channel_id.to_string();
                if !config.rules[index].exempt_channels.contains(&value) {
                    config.rules[index].exempt_channels.push(value);
                }
            }
            "r" => {
                let Some(value) = values.first() else {
                    return cx
                        .reply(InteractionResponseData {
                            content: Some(t!(cx.lang(), "automod.pick_role")),
                            flags: Some(MessageFlags::EPHEMERAL),
                            components: Some(vec![Component::ActionRow(ActionRow {
                                id: None,
                                components: vec![Component::SelectMenu(SelectMenu {
                                    channel_types: None,
                                    custom_id: format!("ax:r:{first}:{id}"),
                                    default_values: None,
                                    disabled: false,
                                    id: None,
                                    kind: SelectMenuType::Role,
                                    min_values: Some(1),
                                    max_values: Some(1),
                                    options: None,
                                    placeholder: Some(t!(cx.lang(), "automod.pick_role")),
                                    required: None,
                                })],
                            })]),
                            ..Default::default()
                        })
                        .await;
                };
                if !automod::valid_id(value) {
                    return cx.reply_ephemeral(t!(cx.lang(), "automod.invalid")).await;
                }
                if !config.rules[index].exempt_roles.contains(value) {
                    config.rules[index].exempt_roles.push(value.clone());
                }
            }
            _ => return cx.reply_ephemeral(t!(cx.lang(), "automod.invalid")).await,
        }
        if let Err(why) = automod::validate(&config, cx.lang()) {
            return cx.reply_ephemeral(why).await;
        }
        cx.app.db.save_automod(guild, cx.author()?.id.get(), &config).await?;
        cx.app.forget_automod_config(guild).await;
        cx.app
            .cache
            .publish_invalidation(pinchora_cache::Invalidation::Guild(guild))
            .await?;
    } else if action == "ok" {
        cx.app.db.review_automod_hit(guild, id).await?;
    } else {
        let kind = match action {
            "del" => "delete",
            "to" => "timeout",
            "ban" => "ban",
            _ => return cx.reply_ephemeral(t!(cx.lang(), "automod.invalid")).await,
        };
        if !cx.app.db.claim_automod_action(guild, id, kind).await? {
            return cx.reply_ephemeral(t!(cx.lang(), "automod.busy")).await;
        }
        if let Err(why) = actions::execute(
            &cx.app,
            &config,
            &config.rules[index],
            &hit,
            &Action::new(kind),
            Some(cx.author()?.id.get()),
        )
        .await
        {
            return cx.reply_ephemeral(why.to_string()).await;
        }
        cx.app.db.append_automod_action(guild, id, kind).await?;
    }
    cx.reply_ephemeral(t!(cx.lang(), "automod.done")).await
}
