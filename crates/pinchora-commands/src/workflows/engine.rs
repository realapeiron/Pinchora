use crate::{
    App,
    custom::{self, Actor},
};
use pinchora_core::workflows::render;
use pinchora_core::{
    modules::Module,
    tags::{Action, Vars},
    workflows::{self, Condition, Event, Plan, Scope, StepAction, Trigger},
};
use pinchora_db::WorkflowRow;
use std::time::Duration;
use twilight_model::{
    channel::{ChannelType, message::AllowedMentions},
    id::Id,
};

pub fn dry_run(def: &workflows::Definition, event: &Event, now: chrono::DateTime<chrono::Utc>) -> Plan {
    workflows::plan(def, event, now)
}
fn number(id: &str) -> anyhow::Result<u64> {
    let n = id.parse::<u64>()?;
    anyhow::ensure!(n > 0, "missing_context");
    Ok(n)
}
fn cooldown(guild: u64, id: i64, scope: Scope, user: &str, index: usize) -> String {
    format!(
        "wf:cool:{guild}:{id}:{index}:{}",
        if scope == Scope::Server { "0" } else { user }
    )
}
pub async fn run(app: &App, row: &WorkflowRow, mut event: Event) -> anyhow::Result<Plan> {
    let guild = row.guild_id as u64;
    let def = row.definition()?;
    anyhow::ensure!(event.depth == 0, "workflow_depth");
    anyhow::ensure!(
        app.module_enabled(guild, Module::Workflows).await && super::available(app, guild, &def.trigger).await,
        "module_disabled"
    );
    anyhow::ensure!(
        !matches!(def.trigger, Trigger::MessageKeyword { .. }) || app.message_content,
        "message_content_required"
    );
    def.validate().map_err(anyhow::Error::msg)?;
    let run = app
        .db
        .claim_workflow_run(guild, row.id, &event)
        .await?
        .ok_or_else(|| anyhow::anyhow!("budget"))?;
    let result = async {
        let user = event.user_id.parse::<u64>().unwrap_or(0);
        if user > 0 && !matches!(def.trigger, Trigger::MemberLeave) {
            let member = app
                .http
                .guild_member(Id::new(guild), Id::new(user))
                .await?
                .model()
                .await?;
            anyhow::ensure!(!member.user.bot, "bot_event");
            event.roles = member.roles.iter().map(ToString::to_string).collect();
            if let Some(rank) = app.db.member_rank(guild, user).await? {
                event.level = app.leveling_parity(guild).await?.level_of(rank.xp);
            }
            let created = ((user >> 22) as i64 + 1420070400000) / 1000;
            event.account_age_days = ((app.now().timestamp() - created).max(0) / 86400) as u32;
        }
        event.roll = custom::pick(1000000) as u32;
        event.variables = app
            .db
            .workflow_vars(guild, user)
            .await?
            .into_iter()
            .map(|(scope, key, value)| (format!("{scope}:{key}"), value))
            .collect();
        // Check without consuming; claim only after all other conditions pass.
        for (index, c) in def.conditions.iter().enumerate() {
            if let Condition::Cooldown { scope, .. } = c
                && app
                    .cache
                    .get(&cooldown(guild, row.id, *scope, &event.user_id, index))
                    .await?
                    .is_some()
            {
                event.cooldowns.push(index);
            }
        }
        let mut plan = workflows::plan(&def, &event, app.now());
        if plan.outcome == "ready" {
            for (index, c) in def.conditions.iter().enumerate() {
                if let Condition::Cooldown { scope, seconds } = c
                    && app
                        .cache
                        .try_cooldown(
                            &cooldown(guild, row.id, *scope, &event.user_id, index),
                            Duration::from_secs((*seconds).into()),
                        )
                        .await?
                        .is_some()
                {
                    event.cooldowns.push(index);
                }
            }
            plan = workflows::plan(&def, &event, app.now());
        }
        let mut writes = 0;
        for (index, action) in plan.actions.clone().iter().enumerate() {
            writes += action.writes();
            anyhow::ensure!(writes <= workflows::MAX_WRITES, "writes");
            // Persist before each write so interrupted runs show the last attempted step, never replay it.
            if let Some(step) = plan.steps.iter_mut().find(|s| s.phase == "action" && s.index == index) {
                step.status = "running".into();
            }
            app.db
                .finish_workflow_run(guild, run, "running", &serde_json::to_value(&plan.steps)?)
                .await?;
            let result = execute(app, guild, &mut event, action).await;
            if let Some(step) = plan.steps.iter_mut().find(|s| s.phase == "action" && s.index == index) {
                step.status = if result.is_ok() { "done" } else { "failed" }.into();
            }
            if let Err(error) = result {
                tracing::warn!(guild,workflow=row.id,%error,"workflow action failed");
                plan.outcome = "failed".into();
                break;
            }
        }
        if plan.outcome == "ready" {
            plan.outcome = "completed".into();
        }
        for s in &mut plan.steps {
            if s.status == "would_run" {
                s.status = "skipped".into();
            }
        }
        Ok::<_, anyhow::Error>(plan)
    }
    .await;
    match result {
        Ok(plan) => {
            app.db
                .finish_workflow_run(guild, run, &plan.outcome, &serde_json::to_value(&plan.steps)?)
                .await?;
            Ok(plan)
        }
        Err(error) => {
            app.db
                .finish_workflow_run(
                    guild,
                    run,
                    "failed",
                    &serde_json::json!([{"phase":"context","index":0,"status":"failed","detail":{}}]),
                )
                .await?;
            Err(error)
        }
    }
}
async fn execute(app: &App, guild: u64, event: &mut Event, action: &StepAction) -> anyhow::Result<()> {
    let user = event.user_id.parse::<u64>().unwrap_or(0);
    let none = AllowedMentions::default();
    match action {
        StepAction::SendMessage { text, embed, .. } | StepAction::Reply { text, embed } => {
            let channel_id = if let StepAction::SendMessage { channel_id, .. } = action {
                channel_id
            } else {
                &event.channel_id
            };
            let channel = Id::new(number(channel_id)?);
            anyhow::ensure!(
                app.http.channel(channel).await?.model().await?.guild_id.map(Id::get) == Some(guild),
                "foreign_channel"
            );
            let responses = vec![text.clone()];
            let spec = custom::Spec {
                responses: &responses,
                embed: embed.as_ref(),
                allow_mentions: false,
            };
            let rendered = custom::render_message(&spec, &workflows::variables(event), &mut |n| {
                event.roll as usize % n.max(1)
            });
            let (body, embed, _) = custom::to_discord(&rendered, guild, user);
            let embeds = match embed {
                Some(e) => vec![app.brand_embed(guild, e).await],
                None => vec![],
            };
            let mut request = app
                .http
                .create_message(channel)
                .content(&body)
                .embeds(&embeds)
                .allowed_mentions(Some(&none));
            if matches!(action, StepAction::Reply { .. })
                && let Ok(message) = number(&event.message_id)
            {
                request = request.reply(Id::new(message));
            }
            request.await?;
        }
        StepAction::Tag {
            action: Action::Dm { text },
        } => {
            anyhow::ensure!(user > 0 && custom::dm_allowed(app, guild, user).await, "dm_budget");
            let channel = crate::leveling::private_channel(app, user)
                .await
                .ok_or_else(|| anyhow::anyhow!("dm_unavailable"))?;
            app.http
                .create_message(channel)
                .content(&render(text, event))
                .allowed_mentions(Some(&none))
                .await?;
        }
        StepAction::Tag {
            action: Action::React { emoji },
        }
        | StepAction::AddReaction { emoji } => {
            use twilight_http::request::channel::reaction::RequestReactionType;
            let channel = Id::new(number(&event.channel_id)?);
            let message = Id::new(number(&event.message_id)?);
            let custom = emoji
                .strip_prefix('<')
                .and_then(|e| e.strip_suffix('>'))
                .map(|e| e.strip_prefix('a').unwrap_or(e))
                .and_then(|e| e.strip_prefix(':'))
                .and_then(|e| e.split_once(':'))
                .and_then(|(name, id)| Some((name, id.parse::<u64>().ok()?)));
            let reaction = match custom {
                Some((name, id)) => RequestReactionType::Custom {
                    id: Id::new(id),
                    name: Some(name),
                },
                None => RequestReactionType::Unicode { name: emoji },
            };
            app.http.create_reaction(channel, message, &reaction).await?;
        }
        StepAction::Tag {
            action: Action::GivePoints { amount },
        }
        | StepAction::TakePoints { amount } => {
            anyhow::ensure!(
                user > 0 && app.module_enabled(guild, Module::Points).await,
                "points_disabled"
            );
            let amount = if matches!(action, StepAction::TakePoints { .. }) {
                -*amount
            } else {
                *amount
            };
            let points = custom::run_points(
                app,
                app.guild_lang(guild).await,
                guild,
                user,
                &[Action::GivePoints { amount }],
            )
            .await;
            anyhow::ensure!(points.balance.is_some() && points.capped.is_none(), "points_cap");
        }
        StepAction::Tag { action } => {
            anyhow::ensure!(user > 0, "member");
            let role = action.role().ok_or_else(|| anyhow::anyhow!("action"))?;
            // Fail closed before Discord: a role callback cannot recursively start a workflow.
            app.cache
                .put(&format!("wf:own:{guild}:{user}:{role}"), "1", Duration::from_secs(120))
                .await?;
            let held: Vec<u64> = event.roles.iter().filter_map(|r| r.parse().ok()).collect();
            let actor = Actor {
                guild,
                user,
                held: &held,
                message: None,
                lang: app.guild_lang(guild).await,
                responder: true,
            };
            let notes = custom::run_discord_actions(app, &actor, std::slice::from_ref(action), &Vars::new()).await;
            anyhow::ensure!(notes.is_empty(), "role_refused");
            let give = matches!(action, Action::AddRole { .. })
                || matches!(action, Action::ToggleRole { .. }) && !held.contains(&role);
            event.roles.retain(|r| r != &role.to_string());
            if give {
                event.roles.push(role.to_string());
            }
        }
        StepAction::SetVar { scope, key, value } => {
            let value = render(value, event);
            let value = app.db.set_workflow_var(guild, *scope, user, key, &value, None).await?;
            event.variables.insert(format!("{}:{key}", scope.key()), value);
        }
        StepAction::IncrVar { scope, key, amount } => {
            let value = app
                .db
                .set_workflow_var(guild, *scope, user, key, "", Some(*amount))
                .await?;
            event.variables.insert(format!("{}:{key}", scope.key()), value);
        }
        StepAction::CreateThread { channel_id, name } => {
            let channel = Id::new(number(channel_id)?);
            anyhow::ensure!(
                app.http.channel(channel).await?.model().await?.guild_id.map(Id::get) == Some(guild),
                "foreign_channel"
            );
            app.http
                .create_thread(channel, &render(name, event), ChannelType::PublicThread)
                .await?;
        }
        StepAction::Pin => {
            app.http
                .create_pin(Id::new(number(&event.channel_id)?), Id::new(number(&event.message_id)?))
                .await?;
        }
        StepAction::OpenTicket { panel_id, topic } => {
            open_ticket(app, guild, user, *panel_id, *topic).await?;
        }
        StepAction::Log { text } => {
            tracing::info!(guild,workflow_log=%render(text,event),"workflow log");
        }
        StepAction::RandomBranch { .. } => anyhow::bail!("nested_branch"),
    }
    Ok(())
}
/// The existing ticket store and permission model, with no pings or recursive ticket event.
async fn open_ticket(app: &App, guild: u64, user: u64, panel_id: i64, topic: usize) -> anyhow::Result<()> {
    use pinchora_db::{NewTicket, TicketOpen};
    anyhow::ensure!(
        user > 0 && app.module_enabled(guild, Module::Tickets).await,
        "tickets_disabled"
    );
    let panel = app
        .db
        .ticket_panel(panel_id)
        .await?
        .filter(|p| p.guild_id == guild as i64 && p.enabled)
        .ok_or_else(|| anyhow::anyhow!("panel"))?;
    anyhow::ensure!(panel.questions.is_empty(), "ticket_form_required");
    let topic = panel.topics.get(topic).ok_or_else(|| anyhow::anyhow!("topic"))?;
    let settings = app.db.ticket_settings(guild).await?;
    anyhow::ensure!(
        app.cache
            .try_cooldown(
                &format!("tk:{guild}:{user}"),
                Duration::from_secs(pinchora_core::tickets::OPEN_COOLDOWN_SECS)
            )
            .await?
            .is_none(),
        "ticket_cooldown"
    );
    let count = if settings.mode() == pinchora_core::tickets::Mode::Auto {
        app.http.guild_channels(Id::new(guild)).await?.models().await?.len()
    } else {
        0
    };
    let new = NewTicket {
        guild_id: guild,
        panel_id: Some(panel.id),
        opener: user,
        topic: &topic.label,
        answers: &[],
        max_open: settings.max_open_per_user,
        at: app.now(),
    };
    let TicketOpen::Opened(ticket) = app.db.open_ticket(&new).await? else {
        anyhow::bail!("ticket_limit")
    };
    let name = format!("ticket-{}", ticket.number);
    let made = async {
        if settings.mode().threads(count) {
            let parent = Id::new(panel.channel_id as u64);
            anyhow::ensure!(
                app.http.channel(parent).await?.model().await?.guild_id.map(Id::get) == Some(guild),
                "foreign_channel"
            );
            let channel = app
                .http
                .create_thread(parent, &name, ChannelType::PrivateThread)
                .auto_archive_duration(twilight_model::channel::thread::AutoArchiveDuration::Week)
                .invitable(false)
                .await?
                .model()
                .await?;
            // Keep the place even if adding the member fails, so staff can recover it.
            app.db
                .set_ticket_place(ticket.id, parent.get(), Some(channel.id.get()))
                .await?;
            app.http.add_thread_member(channel.id, Id::new(user)).await?;
            Ok::<_, anyhow::Error>(channel.id)
        } else {
            let overwrites =
                crate::tickets::workflow_overwrites(guild, user, app.application_id.get(), &panel.support_roles);
            let mut request = app
                .http
                .create_guild_channel(Id::new(guild), &name)
                .kind(ChannelType::GuildText)
                .permission_overwrites(&overwrites);
            if let Some(category) = panel.open_category_id.or(settings.open_category_id) {
                request = request.parent_id(Id::new(category as u64));
            }
            let channel = request.await?.model().await?;
            app.db.set_ticket_place(ticket.id, channel.id.get(), None).await?;
            Ok(channel.id)
        }
    }
    .await;
    let channel = match made {
        Ok(channel) => channel,
        Err(error) => {
            if app.db.ticket(ticket.id).await?.is_some_and(|t| t.place().is_none()) {
                app.db.abort_ticket(ticket.id).await?;
            }
            return Err(error);
        }
    };
    let lang = app.guild_lang(guild).await;
    let template = if panel.welcome_text.trim().is_empty() {
        pinchora_core::t!(lang, "tickets.welcome.default")
    } else {
        panel.welcome_text.clone()
    };
    let text = pinchora_core::tickets::fill_welcome(
        &template,
        &format!("<@{user}>"),
        &crate::tickets::workflow_server_name(app, guild).await,
        ticket.number,
        &topic.label,
    );
    let embed = app
        .brand_embed(guild, crate::tickets::workflow_welcome_embed(&ticket, &text, &[], lang))
        .await;
    app.http
        .create_message(channel)
        .embeds(&[embed])
        .components(&crate::tickets::workflow_controls(ticket.id, false, lang))
        .allowed_mentions(Some(&AllowedMentions::default()))
        .await?;
    Ok(())
}
