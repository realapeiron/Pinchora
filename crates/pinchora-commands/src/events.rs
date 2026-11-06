use std::{sync::Arc, time::Duration};

use pinchora_core::{
    leveling::{MESSAGE_XP, MESSAGE_XP_COOLDOWN_SECS, level_of, scale, week_start},
    modules::Module,
};
use pinchora_db::XpGrant;
use pinchora_render::{Theme, WelcomeCard};
use twilight_model::{
    channel::message::{AllowedMentions, MessageType},
    gateway::payload::incoming::{
        BanAdd, BanRemove, GuildCreate, GuildDelete, MemberAdd, MemberRemove, MemberUpdate, MessageCreate,
        VoiceStateUpdate,
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

pub use pinchora_core::settings::{DEFAULT_GOODBYE, DEFAULT_LEVELUP, DEFAULT_WELCOME};

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
    // With message content on, the server log keeps the message a day to show an edit or delete.
    server_log::keep_message(&app, g, &msg).await;
    // Chat that makes critters and bottles wash up counts here, before the XP cooldown: it never reads
    // the message, and a failure never keeps a member from their XP.
    let here = Some(msg.channel_id.get());
    if app.module_allowed(g, here, Module::Collection).await
        && let Err(error) = collection::spawn::on_chat(&app, g, u, msg.channel_id.get()).await
    {
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
        g,
        &msg.author,
        msg.member.as_ref().and_then(|m| m.nick.as_deref()),
    )
    .await;
    // Leveling and points can also be switched off in some channels (`/config modules`).
    let leveling_on = settings.leveling_enabled && app.module_allowed(g, here, Module::Leveling).await;
    let points_on = settings.points_enabled && app.module_allowed(g, here, Module::Points).await;
    let points = if points_on {
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
    let rules = app.xp_rules(g).await?;
    let lineage = app.xp_lineage(&rules, msg.channel_id.get()).await;
    let multiplier = rules.multiplier_in(&lineage, &held);
    let points = scale(points, multiplier);
    if !leveling_on {
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
    server_log::note_names(&app, guild_id.get(), &member.user, member.nick.as_deref()).await;
    let roles: Vec<u64> = member.roles.iter().map(|r| r.get()).collect();
    server_log::member_joined(&app, guild_id.get(), &member.user, &roles).await;
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
    server_log::member_left(&app, guild_id.get(), &user).await;
    if !app.module_allowed(guild_id.get(), None, Module::Goodbye).await {
        return Ok(());
    }
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

/// Logs nickname, username and avatar changes.
pub async fn on_member_update(app: Arc<App>, event: MemberUpdate) -> anyhow::Result<()> {
    let roles: Vec<u64> = event.roles.iter().map(|r| r.get()).collect();
    server_log::names_updated(&app, event.guild_id.get(), &event.user, event.nick.as_deref(), &roles).await;
    Ok(())
}

/// Logs a ban.
pub async fn on_ban_add(app: Arc<App>, event: BanAdd) -> anyhow::Result<()> {
    server_log::ban_changed(&app, event.guild_id.get(), &event.user, true).await;
    Ok(())
}

/// Logs an unban.
pub async fn on_ban_remove(app: Arc<App>, event: BanRemove) -> anyhow::Result<()> {
    server_log::ban_changed(&app, event.guild_id.get(), &event.user, false).await;
    Ok(())
}

/// Logs voice joins, leaves and moves, and keeps track of who sits in voice for voice XP.
pub async fn on_voice_state_update(app: Arc<App>, event: VoiceStateUpdate) -> anyhow::Result<()> {
    let state = event.0;
    let Some(guild) = state.guild_id else {
        return Ok(());
    };
    let settings = app.guild_settings(guild.get()).await?;
    server_log::voice_updated(&app, guild.get(), &state).await;
    voice_xp::track(&app, guild.get(), &state, &settings).await;
    Ok(())
}

/// Rebuilds who is in voice from the server's voice states, which Discord sends once per session in
/// GUILD_CREATE: a leave or join missed while the bot was away is put right here.
pub async fn on_guild_create(app: Arc<App>, event: GuildCreate) -> anyhow::Result<()> {
    match voice_snapshot(&app, event) {
        Some(snapshot) => voice_xp::rebuild(&app, snapshot).await,
        None => Ok(()),
    }
}

/// What the rebuild of a GUILD_CREATE needs, taken at once and without awaiting anything: the guild itself
/// (its members, roles and emojis, big on a big server) is dropped here, so a rebuild waiting for its slot or
/// for the database holds only the snapshot. `None` for a server that is unavailable.
pub fn voice_snapshot(app: &App, event: GuildCreate) -> Option<voice_xp::VoiceSnapshot> {
    let GuildCreate::Available(guild) = event else {
        return None;
    };
    Some(voice_xp::VoiceSnapshot::of(&guild, app.now().timestamp()))
}

/// Forgets who was in voice in a server the bot lost, or that went down: it starts over from the
/// GUILD_CREATE that follows when it comes back.
pub async fn on_guild_delete(app: Arc<App>, event: GuildDelete) -> anyhow::Result<()> {
    voice_xp::forget(&app, event.id.get()).await
}
