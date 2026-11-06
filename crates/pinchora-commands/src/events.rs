use std::{sync::Arc, time::Duration};

use pinchora_core::leveling::{MESSAGE_XP, MESSAGE_XP_COOLDOWN_SECS, level_of, scale, week_start};
use pinchora_db::XpGrant;
use pinchora_render::{Theme, WelcomeCard};
use twilight_model::{
    channel::message::{AllowedMentions, MessageType},
    gateway::payload::incoming::{
        BanAdd, BanRemove, MemberAdd, MemberRemove, MemberUpdate, MessageCreate, VoiceStateUpdate,
    },
    http::attachment::Attachment,
    id::Id,
};

use crate::{
    App,
    commands::collection,
    leveling::{self, LevelUp},
    roles, server_log,
    util::{display_name, fill, mention},
    voice_xp,
};

pub const DEFAULT_LEVELUP: &str = "🎉 {user} reached **level {level}**!";
pub const DEFAULT_WELCOME: &str = "Welcome to **{server}**, {user}!";
pub const DEFAULT_GOODBYE: &str = "👋 **{user}** has left **{server}**.";

/// Grants chat XP (once per minute per member) and announces level-ups.
pub async fn on_message(app: Arc<App>, event: MessageCreate) -> anyhow::Result<()> {
    let msg = event.message;
    if msg.author.bot || msg.webhook_id.is_some() || !matches!(msg.kind, MessageType::Regular | MessageType::Reply) {
        return Ok(());
    }
    let Some(guild) = msg.guild_id else {
        return Ok(());
    };
    let (g, u) = (guild.get(), msg.author.id.get());
    // Chat that makes critters and bottles wash up counts here, before the XP cooldown: it never reads
    // the message, and a failure never keeps a member from their XP.
    if let Err(error) = collection::spawn::on_chat(&app, g, u, msg.channel_id.get()).await {
        tracing::debug!(%error, guild = g, "chat drops not counted");
    }
    let cooldown = Duration::from_secs(MESSAGE_XP_COOLDOWN_SECS);
    if app
        .cache
        .try_cooldown(&format!("xp:{g}:{u}"), cooldown)
        .await?
        .is_some()
    {
        return Ok(());
    }
    let settings = app.guild_settings(g).await?;
    server_log::note_names(
        &app,
        &settings,
        g,
        &msg.author,
        msg.member.as_ref().and_then(|m| m.nick.as_deref()),
    )
    .await;
    let points = if settings.points_enabled {
        rand::random_range(i64::from(settings.points_min)..=i64::from(settings.points_max))
    } else {
        0
    };
    let held: Vec<u64> = msg
        .member
        .as_ref()
        .map(|m| m.roles.iter().map(|r| r.get()).collect())
        .unwrap_or_default();
    // The server's multipliers scale its own XP and points; the global XP stays what it is.
    let multiplier = app.xp_rules(g).await?.multiplier(msg.channel_id.get(), &held);
    let points = scale(points, multiplier);
    if !settings.leveling_enabled {
        if points > 0 {
            app.db.give_points(g, u, points).await?;
            metrics::counter!("pinchora_points_granted_total").increment(points as u64);
        }
        return Ok(());
    }
    let amount = rand::random_range(MESSAGE_XP);
    let grant = XpGrant {
        server_xp: scale(amount, multiplier),
        global_xp: amount,
        points,
    };
    let gain = app.db.add_xp(g, u, grant, week_start(app.now())).await?;
    metrics::counter!("pinchora_xp_granted_total").increment(grant.server_xp as u64);
    metrics::counter!("pinchora_points_granted_total").increment(points as u64);
    app.quest(u, pinchora_game::quests::QuestKind::Chat).await;

    let (before, after) = (level_of(gain.old_xp), level_of(gain.new_xp));
    if after <= before {
        return Ok(());
    }
    leveling::level_up(
        &app,
        &settings,
        LevelUp {
            guild: g,
            user: u,
            before,
            after,
            author: Some(&msg.author),
            nick: msg.member.as_ref().and_then(|m| m.nick.as_deref()),
            roles: &held,
            here: msg.channel_id,
        },
    )
    .await
}

/// Gives the join role, logs the join, and posts a welcome card when the server has them turned on.
pub async fn on_member_add(app: Arc<App>, event: MemberAdd) -> anyhow::Result<()> {
    let MemberAdd { guild_id, member } = event;
    let settings = app.guild_settings(guild_id.get()).await?;
    // The role and the log come first: neither depends on the welcome card going out.
    if let Some(role) = settings.join_role_id.filter(|_| !member.user.bot) {
        roles::give_join_role(&app, guild_id.get(), member.user.id.get(), role as u64).await;
    }
    server_log::note_names(&app, &settings, guild_id.get(), &member.user, member.nick.as_deref()).await;
    server_log::member_joined(&app, &settings, &member.user).await;
    if member.user.bot {
        return Ok(());
    }
    let Some(channel) = settings.welcome_channel_id.filter(|_| settings.welcome_enabled) else {
        return Ok(());
    };
    let guild = app.http.guild(guild_id).with_counts(true).await?.model().await?;
    let count = guild.approximate_member_count;
    let name = display_name(&member.user, member.nick.as_deref());
    let avatar = app.avatar(&member.user, WelcomeCard::AVATAR_SIZE, Some(44)).await;
    let server_name = guild.name.clone();
    let title = settings.welcome_title.clone().unwrap_or_else(|| "Welcome".to_owned());
    let theme = Theme::preset(settings.welcome_theme.as_deref().unwrap_or("reef"))
        .with_accent(settings.welcome_accent.map(|c| c as u32));
    let png = app
        .render(move |r| {
            r.welcome_card(&WelcomeCard {
                display_name: &name,
                avatar_png: &avatar,
                server_name: &server_name,
                member_number: count,
                title: &title,
                theme,
            })
        })
        .await?;
    let text = settings
        .welcome_message
        .as_deref()
        .unwrap_or(DEFAULT_WELCOME)
        .replace("{user}", &mention(member.user.id))
        .replace("{server}", &guild.name)
        .replace("{count}", &count.map(|c| c.to_string()).unwrap_or_default());
    let mentions = AllowedMentions {
        users: vec![member.user.id],
        ..Default::default()
    };
    app.http
        .create_message(Id::new(channel as u64))
        .content(&text)
        .allowed_mentions(Some(&mentions))
        .attachments(&[Attachment::from_bytes("welcome.png".to_owned(), png, 1)])
        .await?;
    Ok(())
}

/// Says goodbye when the server has it turned on, and logs the leave.
pub async fn on_member_remove(app: Arc<App>, event: MemberRemove) -> anyhow::Result<()> {
    let MemberRemove { guild_id, user } = event;
    let settings = app.guild_settings(guild_id.get()).await?;
    app.log_state.forget(guild_id.get(), user.id.get()).await;
    server_log::member_left(&app, &settings, &user).await;
    let Some(channel) = settings.goodbye_channel_id.filter(|_| !user.bot) else {
        return Ok(());
    };
    let template = settings.goodbye_message.as_deref().unwrap_or(DEFAULT_GOODBYE);
    // Only ask Discord for the server when the message uses it.
    let (server, members) = if template.contains("{server}") || template.contains("{members}") {
        let guild = app.http.guild(guild_id).with_counts(true).await?.model().await?;
        let members = guild.approximate_member_count.map(|c| c.to_string());
        (guild.name, members.unwrap_or_default())
    } else {
        Default::default()
    };
    // Not a mention: the member is gone, and a name reads better than a bare id.
    let text = fill(
        template,
        &[
            ("user", &display_name(&user, None)),
            ("server", &server),
            ("members", &members),
        ],
    );
    let mentions = AllowedMentions::default();
    app.http
        .create_message(Id::new(channel as u64))
        .content(&text)
        .allowed_mentions(Some(&mentions))
        .await?;
    Ok(())
}

/// Logs nickname and username changes.
pub async fn on_member_update(app: Arc<App>, event: MemberUpdate) -> anyhow::Result<()> {
    let settings = app.guild_settings(event.guild_id.get()).await?;
    server_log::names_updated(
        &app,
        &settings,
        event.guild_id.get(),
        &event.user,
        event.nick.as_deref(),
    )
    .await;
    Ok(())
}

/// Logs a ban.
pub async fn on_ban_add(app: Arc<App>, event: BanAdd) -> anyhow::Result<()> {
    let settings = app.guild_settings(event.guild_id.get()).await?;
    server_log::ban_changed(&app, &settings, &event.user, true).await;
    Ok(())
}

/// Logs an unban.
pub async fn on_ban_remove(app: Arc<App>, event: BanRemove) -> anyhow::Result<()> {
    let settings = app.guild_settings(event.guild_id.get()).await?;
    server_log::ban_changed(&app, &settings, &event.user, false).await;
    Ok(())
}

/// Logs voice joins, leaves and moves, and keeps track of who sits in voice for voice XP.
pub async fn on_voice_state_update(app: Arc<App>, event: VoiceStateUpdate) -> anyhow::Result<()> {
    let state = event.0;
    let Some(guild) = state.guild_id else {
        return Ok(());
    };
    let settings = app.guild_settings(guild.get()).await?;
    server_log::voice_updated(&app, &settings, guild.get(), &state).await;
    voice_xp::track(&app, guild.get(), &state).await;
    Ok(())
}
