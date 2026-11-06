//! Announcing a level-up, wherever it came from (chat or voice): the reward roles first, then the
//! message the server asked for, in the place it asked for.

use std::sync::Arc;

use pinchora_core::{leveling::LevelUpMode, modules::Module};
use pinchora_db::GuildSettings;
use pinchora_render::{LevelUpCard, Theme, placeholder_avatar};
use twilight_model::{
    channel::message::AllowedMentions,
    http::attachment::Attachment,
    id::{Id, marker::ChannelMarker},
    user::User,
};

use crate::{
    App,
    auto::{self, LevelUpEvent},
    commands::customize,
    events::DEFAULT_LEVELUP,
    roles,
    util::{display_name, fill, mention},
};

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
    let template = settings.levelup_message.as_deref().unwrap_or(DEFAULT_LEVELUP);
    let server = if template.contains("{server}") {
        match app.http.guild(Id::new(guild.max(1))).await {
            Ok(response) => response
                .model()
                .await
                .map_or_else(|_| "the server".to_owned(), |g| g.name),
            Err(_) => "the server".to_owned(),
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
    let text = fill(
        template,
        &[
            ("user", &mention(user)),
            ("level", &after.to_string()),
            ("server", &server),
            ("role", &role),
        ],
    );
    let mentions = AllowedMentions {
        users: vec![Id::new(user.max(1))],
        ..Default::default()
    };
    let name = author.map_or_else(|| "Someone".to_owned(), |u| display_name(u, nick));
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
                theme: Theme::reef().with_accent(accent.map(|c| c as u32)),
            })
        })
        .await?;
    let sent = app
        .http
        .create_message(channel)
        .content(&text)
        .allowed_mentions(Some(&mentions))
        .attachments(&[Attachment::from_bytes("levelup.png".to_owned(), png, 1)])
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
    // The new level may have just earned a badge: say so right under the level-up.
    customize::announce_in(app, channel, user).await;
    Ok(())
}

/// The direct message channel with `user`, or `None` when Discord will not open one.
pub(crate) async fn private_channel(app: &App, user: u64) -> Option<Id<ChannelMarker>> {
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
