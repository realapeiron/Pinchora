use std::{sync::Arc, time::Duration};

use pinchora_core::leveling::{MESSAGE_XP, MESSAGE_XP_COOLDOWN_SECS, level_of};
use pinchora_render::{LevelUpCard, Theme, WelcomeCard};
use twilight_model::{
    channel::message::{AllowedMentions, MessageType},
    gateway::payload::incoming::{MemberAdd, MessageCreate},
    http::attachment::Attachment,
    id::Id,
};

use crate::{
    App, roles,
    util::{display_name, mention},
};

pub const DEFAULT_LEVELUP: &str = "🎉 {user} reached **level {level}**!";
pub const DEFAULT_WELCOME: &str = "Welcome to **{server}**, {user}!";

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
    let points = if settings.points_enabled {
        rand::random_range(i64::from(settings.points_min)..=i64::from(settings.points_max))
    } else {
        0
    };
    if !settings.leveling_enabled {
        if points > 0 {
            app.db.give_points(g, u, points).await?;
            metrics::counter!("pinchora_points_granted_total").increment(points as u64);
        }
        return Ok(());
    }
    let amount = rand::random_range(MESSAGE_XP);
    let gain = app.db.add_message_xp(g, u, amount, points).await?;
    metrics::counter!("pinchora_xp_granted_total").increment(amount as u64);
    metrics::counter!("pinchora_points_granted_total").increment(points as u64);
    app.quest(u, pinchora_game::quests::QuestKind::Chat).await;

    let (before, after) = (level_of(gain.old_xp), level_of(gain.new_xp));
    if after <= before {
        return Ok(());
    }
    // Roles first: they must not depend on the announcement going out.
    let held: Vec<u64> = msg
        .member
        .as_ref()
        .map(|m| m.roles.iter().map(|r| r.get()).collect())
        .unwrap_or_default();
    roles::level_up_roles(&app, g, u, after, &held, settings.level_roles_stack).await;
    let channel = settings
        .levelup_channel_id
        .map_or(msg.channel_id, |id| Id::new(id as u64));
    let text = settings
        .levelup_message
        .as_deref()
        .unwrap_or(DEFAULT_LEVELUP)
        .replace("{user}", &mention(u))
        .replace("{level}", &after.to_string());
    let mentions = AllowedMentions {
        users: vec![msg.author.id],
        ..Default::default()
    };
    let name = display_name(&msg.author, msg.member.as_ref().and_then(|m| m.nick.as_deref()));
    let avatar = app.avatar(&msg.author, LevelUpCard::AVATAR_SIZE, Some(34)).await;
    let accent = app.db.user(u).await.ok().and_then(|r| r.accent_color);
    let png = app
        .render(move |r| {
            r.level_up_card(&LevelUpCard {
                display_name: &name,
                avatar_png: &avatar,
                from: before,
                to: after,
                theme: Theme::reef().with_accent(accent.map(|c| c as u32)),
            })
        })
        .await?;
    app.http
        .create_message(channel)
        .content(&text)
        .allowed_mentions(Some(&mentions))
        .attachments(&[Attachment::from_bytes("levelup.png".to_owned(), png, 1)])
        .await?;
    Ok(())
}

/// Posts a welcome card when the server has them turned on.
pub async fn on_member_add(app: Arc<App>, event: MemberAdd) -> anyhow::Result<()> {
    let MemberAdd { guild_id, member } = event;
    if member.user.bot {
        return Ok(());
    }
    let settings = app.guild_settings(guild_id.get()).await?;
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
