use crate::App;
use crate::admin::custom;
use crate::admin::custom::Actor;
use crate::admin::workflows::engine::number;
use pinchora_core::admin::workflows::Event;
use pinchora_core::admin::workflows::StepAction;
use pinchora_core::admin::workflows::render;
use pinchora_core::community::tags::Action;
use pinchora_core::community::tags::Vars;
use twilight_model::{
    channel::{ChannelType, message::AllowedMentions},
    id::Id,
};
pub(super) async fn execute(app: &App, guild: u64, event: &mut Event, action: &StepAction) -> anyhow::Result<()> {
    let user = event.user_id.parse::<u64>().unwrap_or(0);
    match action {
        StepAction::SendMessage { .. } | StepAction::Reply { .. } => {
            send_message(app, guild, event, action).await?;
        }
        StepAction::Tag {
            action: Action::Dm { .. },
        } => {
            send_dm(app, guild, event, action).await?;
        }
        StepAction::Tag {
            action: Action::React { emoji },
        }
        | StepAction::AddReaction { emoji } => {
            add_reaction(app, event, emoji).await?;
        }
        StepAction::Tag {
            action: Action::GivePoints { amount },
        }
        | StepAction::TakePoints { amount } => {
            points(app, guild, event, action, *amount).await?;
        }
        StepAction::Tag { action } => {
            role(app, guild, event, action).await?;
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
                .create_thread(
                    channel,
                    &pinchora_core::community::tags::truncate(&render(name, event), 100),
                    ChannelType::PublicThread,
                )
                .await?;
        }
        StepAction::Pin {} => {
            app.http
                .create_pin(Id::new(number(&event.channel_id)?), Id::new(number(&event.message_id)?))
                .await?;
        }
        StepAction::OpenTicket { panel_id, topic } => {
            crate::admin::workflows::tickets::open_ticket(app, guild, user, *panel_id, *topic).await?;
        }
        StepAction::Log { text } => {
            tracing::info!(guild,workflow_log=%render(text,event),"workflow log");
        }
        StepAction::RandomBranch { .. } => anyhow::bail!("nested_branch"),
    }
    Ok(())
}

async fn send_message(app: &App, guild: u64, event: &mut Event, action: &StepAction) -> anyhow::Result<()> {
    let user = event.user_id.parse::<u64>().unwrap_or(0);
    let none = AllowedMentions::default();

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
    let rendered = crate::admin::workflows::messages::rendered(action, event).expect("message action");
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

    Ok(())
}

async fn send_dm(app: &App, guild: u64, event: &mut Event, action: &StepAction) -> anyhow::Result<()> {
    let user = event.user_id.parse::<u64>().unwrap_or(0);
    let none = AllowedMentions::default();

    anyhow::ensure!(user > 0 && custom::dm_allowed(app, guild, user).await, "dm_budget");
    let channel = crate::progression::leveling::private_channel(app, user)
        .await
        .ok_or_else(|| anyhow::anyhow!("dm_unavailable"))?;
    app.http
        .create_message(channel)
        .content(
            &crate::admin::workflows::messages::rendered(action, event)
                .expect("DM action")
                .content,
        )
        .allowed_mentions(Some(&none))
        .await?;

    Ok(())
}

async fn add_reaction(app: &App, event: &mut Event, emoji: &str) -> anyhow::Result<()> {
    use twilight_http::request::channel::reaction::RequestReactionType;
    let channel = Id::new(number(&event.channel_id)?);
    let message = Id::new(number(&event.message_id)?);
    let emoji = emoji.trim();
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

    Ok(())
}

async fn points(app: &App, guild: u64, event: &mut Event, action: &StepAction, amount: i64) -> anyhow::Result<()> {
    let user = event.user_id.parse::<u64>().unwrap_or(0);

    anyhow::ensure!(
        user > 0 && app.guild_settings(guild).await?.points_enabled,
        "points_disabled"
    );
    let amount = if matches!(action, StepAction::TakePoints { .. }) {
        -amount
    } else {
        amount
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

    Ok(())
}

async fn role(app: &App, guild: u64, event: &mut Event, action: &Action) -> anyhow::Result<()> {
    let user = event.user_id.parse::<u64>().unwrap_or(0);

    anyhow::ensure!(user > 0, "member");
    let role = action.role().ok_or_else(|| anyhow::anyhow!("action"))?;
    let (safe, _) = crate::roles::roles::safe_reward_roles(app, guild).await?;
    anyhow::ensure!(safe.contains(&role), "role_refused");
    let held: Vec<u64> = event.roles.iter().filter_map(|r| r.parse().ok()).collect();
    let give = matches!(action, Action::AddRole { .. })
        || matches!(action, Action::ToggleRole { .. }) && !held.contains(&role);
    if give == held.contains(&role) {
        return Ok(());
    }
    app.db.mark_workflow_role_change(guild, user, role, give).await?;
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
    event.roles.retain(|r| r != &role.to_string());
    if give {
        event.roles.push(role.to_string());
    }

    Ok(())
}
