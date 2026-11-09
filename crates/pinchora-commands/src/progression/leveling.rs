//! Announcing a level-up, wherever it came from (chat or voice): the reward roles first, then the
//! message the server asked for, in the place it asked for.

use std::sync::Arc;

use pinchora_core::admin::card_design::CardKind;
use pinchora_core::admin::modules::Module;
use pinchora_core::admin::settings::MessageKind;
use pinchora_core::progression::leveling::LevelUpMode;
use pinchora_core::t;
use pinchora_db::GuildSettings;
use pinchora_render::{LevelUpCard, Theme, placeholder_avatar};
use twilight_model::{
    channel::message::{AllowedMentions, MentionType},
    http::attachment::Attachment,
    id::{Id, marker::ChannelMarker},
    user::User,
};

use crate::App;
use crate::admin::auto;
use crate::admin::auto::LevelUpEvent;
use crate::commands::social::customize;
use crate::community::events::default_message;
use crate::community::server_events::dm_lang;
use crate::infra::util::display_name;
use crate::infra::util::fill;
use crate::infra::util::mention;
use crate::roles::roles;

/// A member who just gained a level.
pub(crate) struct LevelUp<'a> {
    pub guild: u64,
    pub user: u64,
    pub before: u32,
    pub after: u32,
    /// Their name and avatar as Discord has them; `None` when it cannot be asked.
    pub author: Option<&'a User>,
    pub nick: Option<&'a str>,
    /// The roles they hold.
    pub roles: &'a [u64],
    /// Where they were chatting, or the voice channel they sit in.
    pub here: Id<ChannelMarker>,
}

/// Gives the level roles, then announces the level-up the way the server's mode says. Roles never
/// depend on the announcement going out.
pub(crate) async fn level_up(app: &Arc<App>, settings: &GuildSettings, up: LevelUp<'_>) -> anyhow::Result<()> {
    let LevelUp {
        guild,
        user,
        before,
        after,
        author,
        nick,
        roles: held,
        here,
    } = up;
    let earned = roles::level_up_roles(app, guild, user, after, held, settings.level_roles_stack).await;
    crate::infra::hooks::on_level_up(app, guild, user, after, earned).await;
    // Auto-responders for a level-up answer whatever the level-up mode says, and never hold it up.
    if let Some(author) = author {
        let event = LevelUpEvent {
            guild,
            user: author.clone(),
            nick: nick.map(str::to_owned),
            held: held.to_vec(),
            level: after,
            role: earned,
            here: here.get(),
        };
        let task_app = app.clone();
        app.spawn(async move { auto::leveled_up(&task_app, event).await });
    }
    let mode = LevelUpMode::parse(&settings.levelup_mode);
    let channel = match mode {
        LevelUpMode::Off => return Ok(()),
        LevelUpMode::Current => here,
        // The leveling module can be kept to some channels, and the bot does not post where it is off:
        // a level-up channel its rule leaves out gets nothing, and the level-up is told where it was earned.
        LevelUpMode::Channel => match settings.levelup_channel_id {
            Some(id) if app.module_allowed(guild, Some(id as u64), Module::Leveling).await => Id::new(id as u64),
            _ => here,
        },
        LevelUpMode::Dm => match private_channel(app, user).await {
            Some(channel) => channel,
            None => return Ok(()),
        },
    };
    // A level-up in a channel answers nobody in particular and speaks the server's language; one by direct
    // message speaks the member's.
    let lang = if mode == LevelUpMode::Dm {
        dm_lang(app, user, guild).await
    } else {
        app.guild_lang(guild).await
    };
    let template = settings
        .levelup_message
        .clone()
        .unwrap_or_else(|| default_message(lang, MessageKind::LevelUp));
    let template = template.as_str();
    let server = if template.contains("{server}") {
        match app.http.guild(Id::new(guild.max(1))).await {
            Ok(response) => response
                .model()
                .await
                .map_or_else(|_| t!(lang, "events.the-server"), |g| g.name),
            Err(_) => t!(lang, "events.the-server"),
        }
    } else {
        String::new()
    };
    let role = match earned {
        // A mention does not render in a direct message, so the name goes there.
        Some(role) if mode == LevelUpMode::Dm => app
            .guild_roles(guild)
            .await
            .and_then(|names| names.get(&role).cloned())
            .unwrap_or_else(|| format!("<@&{role}>")),
        Some(role) => format!("<@&{role}>"),
        None => String::new(),
    };
    let mut text = fill(
        template,
        &[
            ("user", &mention(user)),
            ("level", &after.to_string()),
            ("server", &server),
            ("role", &role),
        ],
    );
    let mut mentions = AllowedMentions {
        users: vec![Id::new(user.max(1))],
        ..Default::default()
    };
    let card_server = if server.is_empty() {
        match app.http.guild(Id::new(guild.max(1))).await {
            Ok(response) => response
                .model()
                .await
                .map_or_else(|_| t!(lang, "events.the-server"), |g| g.name),
            Err(_) => t!(lang, "events.the-server"),
        }
    } else {
        server.clone()
    };
    let booster = match app.http.guild_member(Id::new(guild.max(1)), Id::new(user.max(1))).await {
        Ok(response) => response
            .model()
            .await
            .is_ok_and(|member| member.premium_since.is_some()),
        Err(_) => false,
    };
    if let Some((chosen, mass)) = crate::social::message_studio::event_text(
        app,
        guild,
        CardKind::LevelUp,
        crate::social::message_studio::EventMember {
            user_id: user,
            user: &mention(user),
            server: &card_server,
            level: Some(after),
            booster,
            returning: app.db.card_was_returning(guild, user).await?,
            ordinal: None,
        },
    )
    .await?
    {
        text = chosen;
        if mass {
            mentions.parse = vec![MentionType::Everyone, MentionType::Roles];
        }
    }
    let studio = crate::admin::card_images::server_card(app, settings, guild, CardKind::LevelUp).await?;
    if studio.custom || studio.delivery != CardKind::LevelUp.default_delivery() {
        let subject = crate::admin::card_images::CardSubject {
            user: Id::new(user.max(1)),
            avatar: author.and_then(|u| u.avatar),
            name: author.map_or_else(|| t!(lang, "interact.someone"), |u| display_name(u, nick)),
            username: author.map_or_else(|| user.to_string(), |u| u.name.clone()),
            server: card_server,
            members: None,
        };
        let image = if studio.delivery.has_image() {
            Some((
                "levelup.png",
                crate::admin::card_images::card_png(app, guild, &studio.design, &subject).await?,
                studio.design.alt_text.as_str(),
            ))
        } else {
            None
        };
        crate::admin::card_images::deliver(app, channel, studio.delivery, &text, image, &mentions).await?;
        customize::announce_in(app, channel, user, lang).await;
        return Ok(());
    }
    let name = author.map_or_else(|| t!(lang, "interact.someone"), |u| display_name(u, nick));
    let avatar = match author {
        Some(u) => app.avatar(u, LevelUpCard::AVATAR_SIZE, Some(34)).await,
        None => Arc::new(placeholder_avatar(user, LevelUpCard::AVATAR_SIZE)),
    };
    let accent = app.db.user(user).await.ok().and_then(|r| r.accent_color);
    let png = app
        .render(move |r| {
            r.level_up_card(&LevelUpCard {
                display_name: &name,
                avatar_png: &avatar,
                from: before,
                to: after,
                lang,
                theme: Theme::reef().with_accent(accent.map(|c| c as u32)),
            })
        })
        .await?;
    let mut attachment = Attachment::from_bytes("levelup.png".to_owned(), png, 1);
    if !studio.design.alt_text.is_empty() {
        attachment.description(studio.design.alt_text);
    }
    let sent = app
        .http
        .create_message(channel)
        .content(&text)
        .allowed_mentions(Some(&mentions))
        .attachments(&[attachment])
        .await;
    match sent {
        Ok(_) => {}
        // Closed direct messages are the member's choice, not a failure.
        Err(error) if mode == LevelUpMode::Dm => {
            tracing::debug!(guild, user, %error, "level-up direct message not delivered");
            return Ok(());
        }
        Err(error) => return Err(error.into()),
    }
    // The new level may have just earned a badge: say so right under the level-up, in its language.
    customize::announce_in(app, channel, user, lang).await;
    Ok(())
}

/// The direct message channel with `user`, or `None` when they switched direct messages from servers off
/// (`/privacy`) or Discord will not open one. Every message a server sets up to send by DM comes through here.
pub(crate) async fn private_channel(app: &App, user: u64) -> Option<Id<ChannelMarker>> {
    match app.db.dm_opt_out(user).await {
        Ok(false) => {}
        Ok(true) => return None,
        Err(error) => {
            tracing::warn!(user, %error, "could not read the direct message choice");
            return None;
        }
    }
    open_private_channel(app, user).await
}

/// The direct message channel with `user`, whatever they chose about servers' messages: for what the
/// player asked for themselves (`/remind`), which is not a server's to switch off. `None` when Discord
/// will not open one.
pub async fn open_private_channel(app: &App, user: u64) -> Option<Id<ChannelMarker>> {
    let opened = async {
        anyhow::Ok(
            app.http
                .create_private_channel(Id::new(user.max(1)))
                .await?
                .model()
                .await?
                .id,
        )
    };
    opened
        .await
        .map_err(|error| tracing::debug!(user, %error, "could not open a direct message"))
        .ok()
}
