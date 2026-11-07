use crate::{
    App,
    commands::moderation::{self, Act, BOT_NAME},
    util::WARN,
};
use pinchora_core::{
    automod::{Action, Config, Rule},
    logging::LogCategory,
    moderation::{CaseKind, Punishment, Rank, may_act, reached},
    settings::{self, Grantor, RoleFacts},
    t,
};
use pinchora_db::AutoModHit;
use twilight_model::{
    channel::message::{
        AllowedMentions, Component,
        component::{ActionRow, ButtonStyle},
    },
    guild::{Permissions, Role},
    id::Id,
};
use twilight_util::builder::embed::EmbedBuilder;
fn button(id: &str, label: &str, style: ButtonStyle) -> Component {
    crate::server_events::button(id.to_owned(), label, None, style, false)
}

fn top(roles: &[Role], held: &[twilight_model::id::Id<twilight_model::id::marker::RoleMarker>]) -> i64 {
    roles
        .iter()
        .filter(|r| held.contains(&r.id))
        .map(|r| r.position)
        .max()
        .unwrap_or(0)
}
fn powers(
    roles: &[Role],
    guild: u64,
    held: &[twilight_model::id::Id<twilight_model::id::marker::RoleMarker>],
) -> Permissions {
    roles
        .iter()
        .filter(|r| r.id.get() == guild || held.contains(&r.id))
        .fold(Permissions::empty(), |p, r| p | r.permissions)
}
const STAFF: Permissions = Permissions::ADMINISTRATOR
    .union(Permissions::MANAGE_GUILD)
    .union(Permissions::MANAGE_MESSAGES)
    .union(Permissions::MODERATE_MEMBERS);

/// Recheck at detection and execution; role changes between those points fail closed.
pub(crate) async fn protected(
    app: &App,
    guild: u64,
    target: u64,
    held: &[Id<twilight_model::id::marker::RoleMarker>],
) -> anyhow::Result<bool> {
    let facts = app.permission_facts(guild).await?;
    if target == facts.owner
        || target == app.application_id.get()
        || powers(&facts.roles, guild, held).intersects(STAFF)
    {
        return Ok(true);
    }
    let bot_roles = app.my_roles(guild).await?;
    let bot_top = facts
        .roles
        .iter()
        .filter(|r| bot_roles.contains(&r.id.get()))
        .map(|r| r.position)
        .max()
        .unwrap_or(0);
    Ok(top(&facts.roles, held) >= bot_top)
}
fn facts(role: &Role, guild: u64) -> RoleFacts {
    RoleFacts {
        everyone: role.id.get() == guild,
        managed: role.managed,
        position: role.position,
        permissions: role.permissions.bits(),
    }
}
/// Role actions are checked both on save (against the manager) and at execution (against the bot).
pub async fn check_config_roles(app: &App, guild: u64, actor: u64, config: &Config) -> anyhow::Result<()> {
    let ids: Vec<&String> = config
        .rules
        .iter()
        .flat_map(|r| &r.actions)
        .chain(config.escalation.iter().map(|e| &e.action))
        .filter_map(|a| a.role.as_ref().filter(|_| a.kind == "add_role"))
        .collect();
    if ids.is_empty() {
        return Ok(());
    }
    let lang = app.guild_lang(guild).await;
    let roles = app.http.roles(Id::new(guild)).await?.models().await?;
    let member = app
        .http
        .guild_member(Id::new(guild), Id::new(actor))
        .await?
        .model()
        .await?;
    let bot = app
        .http
        .guild_member(Id::new(guild), app.application_id.cast())
        .await?
        .model()
        .await?;
    let owner = app
        .guild_facts(guild)
        .await
        .ok_or_else(|| anyhow::anyhow!(t!(lang, "automod.unavailable")))?
        .owner;
    let grantor = Grantor {
        owner: owner == actor,
        administrator: powers(&roles, guild, &member.roles).contains(Permissions::ADMINISTRATOR),
        top_role: Some(top(&roles, &member.roles)),
    };
    for id in ids {
        let role = roles
            .iter()
            .find(|r| r.id.to_string() == *id)
            .ok_or_else(|| anyhow::anyhow!(t!(lang, "automod.invalid")))?;
        if let Some(why) = settings::action_role_refusal_in(lang, facts(role, guild), grantor, &role.name)
            .or_else(|| settings::role_refusal_in(lang, facts(role, guild), Some(top(&roles, &bot.roles)), &role.name))
        {
            anyhow::bail!(why);
        }
    }
    Ok(())
}

pub(crate) async fn alert_channel(app: &App, guild: u64, config: &Config) -> Option<u64> {
    if let Some(id) = config.alert_channel_id.as_deref().and_then(|s| s.parse().ok()) {
        return Some(id);
    }
    if let Ok(s) = app.db.mod_settings(guild).await
        && let Some(id) = s.log_channel_id
    {
        return Some(id as u64);
    }
    app.log_config(guild)
        .await
        .ok()?
        .category_channel(LogCategory::Moderation)
}

/// Executes one action; a warn may append the existing moderation ladder's action, once.
pub(crate) async fn execute(
    app: &App,
    config: &Config,
    rule: &Rule,
    hit: &AutoModHit,
    action: &Action,
    actor: Option<u64>,
) -> anyhow::Result<Vec<String>> {
    let guild = hit.guild_id as u64;
    let target = hit.user_id as u64;
    let lang = app.guild_lang(guild).await;
    let unavailable = || anyhow::anyhow!(t!(lang, "automod.unavailable"));
    let owner = app.guild_facts(guild).await.ok_or_else(unavailable)?.owner;
    if owner == target || target == app.application_id.get() {
        anyhow::bail!(t!(lang, "automod.protected"));
    }
    if action.kind == "log" {
        alert(app, config, rule, hit).await?;
        return Ok(vec![]);
    }
    if action.kind == "dm" {
        if let Some(channel) = crate::leveling::private_channel(app, target).await {
            let reason = t!(lang, "automod.reason", rule = crate::util::strip_mentions(&rule.name));
            app.http
                .create_message(channel)
                .content(&reason)
                .allowed_mentions(Some(&AllowedMentions::default()))
                .await?;
        }
        return Ok(vec![]);
    }
    let member = app
        .http
        .guild_member(Id::new(guild), Id::new(target))
        .await?
        .model()
        .await?;
    if member.user.bot {
        anyhow::bail!(t!(lang, "automod.protected"));
    }
    let roles = app.http.roles(Id::new(guild)).await?.models().await?;
    let bot = app
        .http
        .guild_member(Id::new(guild), app.application_id.cast())
        .await?
        .model()
        .await?;
    if actor.is_none() && powers(&roles, guild, &member.roles).intersects(STAFF) {
        anyhow::bail!(t!(lang, "automod.protected"));
    }
    let bot_powers = powers(&roles, guild, &bot.roles);
    let (needed, permission) = match action.kind.as_str() {
        "delete" => (Permissions::MANAGE_MESSAGES, "manage_messages"),
        "timeout" | "warn" => (Permissions::MODERATE_MEMBERS, "moderate_members"),
        "kick" => (Permissions::KICK_MEMBERS, "kick_members"),
        "ban" => (Permissions::BAN_MEMBERS, "ban_members"),
        "add_role" => (Permissions::MANAGE_ROLES, "manage_roles"),
        _ => (Permissions::empty(), "manage_guild"),
    };
    if !bot_powers.contains(Permissions::ADMINISTRATOR) && !bot_powers.contains(needed) {
        anyhow::bail!(t!(
            lang,
            "automod.missing_permission",
            permission = t!(lang, &format!("automod.perm.{permission}"))
        ));
    }
    let reason = t!(lang, "automod.reason", rule = crate::util::strip_mentions(&rule.name));
    if matches!(action.kind.as_str(), "timeout" | "kick" | "ban")
        && let Some(why) = crate::anti_nuke::vip_refusal(app, guild, target).await?
    {
        anyhow::bail!(why);
    }
    let hierarchy = matches!(action.kind.as_str(), "warn" | "timeout" | "kick" | "ban" | "add_role");
    if hierarchy {
        let bot_rank = Rank {
            id: app.application_id.get(),
            top: top(&roles, &bot.roles),
        };
        let actor_rank = if let Some(actor) = actor {
            let m = app
                .http
                .guild_member(Id::new(guild), Id::new(actor))
                .await?
                .model()
                .await?;
            Rank {
                id: actor,
                top: top(&roles, &m.roles),
            }
        } else {
            bot_rank
        };
        if may_act(
            owner,
            actor_rank,
            bot_rank,
            Rank {
                id: target,
                top: top(&roles, &member.roles),
            },
        )
        .is_err()
            || (action.kind == "timeout" && powers(&roles, guild, &member.roles).contains(Permissions::ADMINISTRATOR))
        {
            anyhow::bail!(t!(lang, "automod.hierarchy"));
        }
    }
    let _ban_guard = if action.kind == "ban" {
        Some(app.db.lock_ban(guild, target).await?)
    } else {
        None
    };
    let kind = match action.kind.as_str() {
        "delete" => {
            if let Some(message) = hit.message_id {
                match app
                    .http
                    .delete_message(Id::new(hit.channel_id as u64), Id::new(message as u64))
                    .await
                {
                    Ok(_) => {}
                    Err(e) if crate::roles::error_code(&e) == Some(10008) => {}
                    Err(e) => return Err(e.into()),
                }
            }
            return Ok(vec![]);
        }
        "warn" => CaseKind::Warn,
        "timeout" => {
            let until = twilight_model::util::Timestamp::from_secs(
                (app.now() + chrono::Duration::seconds(i64::from(action.secs))).timestamp(),
            )?;
            app.http
                .update_guild_member(Id::new(guild), Id::new(target))
                .communication_disabled_until(Some(until))
                .await?;
            CaseKind::Timeout
        }
        "kick" => {
            app.http.remove_guild_member(Id::new(guild), Id::new(target)).await?;
            CaseKind::Kick
        }
        "ban" => {
            app.http.create_ban(Id::new(guild), Id::new(target)).await?;
            CaseKind::Ban
        }
        "add_role" => {
            let id = action
                .role
                .as_deref()
                .and_then(|s| s.parse::<u64>().ok())
                .ok_or_else(unavailable)?;
            let role = roles.iter().find(|r| r.id.get() == id).ok_or_else(unavailable)?;
            if let Some(why) = settings::run_time_refusal_in(lang, role.permissions.bits(), &role.name).or_else(|| {
                settings::role_refusal_in(lang, facts(role, guild), Some(top(&roles, &bot.roles)), &role.name)
            }) {
                anyhow::bail!(why);
            }
            app.http
                .add_guild_member_role(Id::new(guild), Id::new(target), role.id)
                .await?;
            CaseKind::RoleAdd
        }
        _ => anyhow::bail!(t!(lang, "automod.invalid")),
    };
    let act = Act {
        kind,
        target,
        target_name: member.user.name.clone(),
        reason: Some(reason),
        duration: (matches!(kind, CaseKind::Timeout | CaseKind::Ban) && action.secs > 0)
            .then(|| chrono::Duration::seconds(i64::from(action.secs))),
        details: Some(t!(lang, "automod.case_detail", hit = hit.id)),
    };
    let opened = moderation::record(
        app,
        guild,
        (actor.unwrap_or(app.application_id.get()), BOT_NAME),
        &act,
        if config.dm_on_action { None } else { Some(false) },
    )
    .await?;
    if kind == CaseKind::Warn {
        let ladder = app.db.warn_thresholds(guild).await?;
        if let Some(rung) = reached(&ladder, opened.active_warns) {
            let kind = match rung.action {
                Punishment::Timeout => "timeout",
                Punishment::Kick => "kick",
                Punishment::Ban => "ban",
            };
            let next = Action {
                kind: kind.into(),
                secs: rung
                    .duration
                    .map_or(if kind == "timeout" { 600 } else { 0 }, |d| d.num_seconds() as u32),
                role: None,
            };
            Box::pin(execute(app, config, rule, hit, &next, None)).await?;
            return Ok(vec![format!("ladder:{kind}")]);
        }
    }
    Ok(vec![])
}

fn display_actions(lang: pinchora_core::i18n::Lang, value: &str) -> String {
    value
        .split(" · ")
        .map(|a| {
            if a == "log_only" {
                t!(lang, "automod.ui.dry_result")
            } else if a == "pending" {
                t!(lang, "automod.pending")
            } else if let Some(kind) = a.strip_prefix("ladder:") {
                t!(
                    lang,
                    "automod.ladder_action",
                    action = t!(lang, &format!("automod.action.{kind}"))
                )
            } else if pinchora_core::automod::ACTIONS.contains(&a) {
                t!(lang, &format!("automod.action.{a}"))
            } else {
                crate::util::strip_mentions(a)
            }
        })
        .collect::<Vec<_>>()
        .join(" · ")
}

pub(crate) async fn alert(app: &App, config: &Config, rule: &Rule, hit: &AutoModHit) -> anyhow::Result<()> {
    let guild = hit.guild_id as u64;
    let Some(channel) = alert_channel(app, guild, config).await else {
        return Ok(());
    };
    let lang = app.guild_lang(guild).await;
    let text = t!(
        lang,
        "automod.alert_body",
        rule = crate::util::strip_mentions(&rule.name),
        user = format!("<@{}>", hit.user_id),
        channel = format!("<#{}>", hit.channel_id),
        action = display_actions(lang, &hit.action)
    );
    let embed = EmbedBuilder::new()
        .title(t!(lang, "automod.alert_title", hit = hit.id))
        .description(text)
        .color(WARN)
        .build();
    let actions = ["del", "to", "ban", "ok"]
        .into_iter()
        .map(|a| {
            button(
                &format!("am:{a}:{}", hit.id),
                &t!(lang, &format!("automod.button.{a}")),
                if a == "ban" {
                    ButtonStyle::Danger
                } else {
                    ButtonStyle::Secondary
                },
            )
        })
        .collect();
    let mut components = vec![Component::ActionRow(ActionRow {
        id: None,
        components: actions,
    })];
    if let Some(id) = &rule.id {
        components.push(Component::ActionRow(ActionRow {
            id: None,
            components: vec![
                button(
                    &format!("ax:c:{id}:{}", hit.id),
                    &t!(lang, "automod.button.channel"),
                    ButtonStyle::Secondary,
                ),
                button(
                    &format!("ax:r:{id}:{}", hit.id),
                    &t!(lang, "automod.button.role"),
                    ButtonStyle::Secondary,
                ),
            ],
        }));
    }
    app.http
        .create_message(Id::new(channel))
        .embeds(&[app.brand_embed(guild, embed).await])
        .components(&components)
        .allowed_mentions(Some(&AllowedMentions::default()))
        .await?;
    Ok(())
}
