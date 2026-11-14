use super::*;

// /Posting

/// Where a panel was posted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PostedPanel {
    pub channel: u64,
    pub message: u64,
    /// The message that was already there was edited in place.
    pub updated: bool,
    /// The channel the panel was in before, when it moved.
    pub moved_from: Option<u64>,
    /// Emojis Discord would not take as reactions.
    pub unusable: Vec<String>,
}

/// Why a panel was not posted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PostRefusal {
    /// The panel has no roles yet.
    NoRoles,
    /// A reactions panel needs an emoji on every role; this one has none (the label).
    MissingEmoji(String),
    /// Two roles of a reactions panel share an emoji (the second label).
    SameEmoji(String),
    /// The bot may not post there.
    NoAccess,
    UnknownChannel,
    /// Discord refused the message (an emoji on a button it does not know, say).
    Refused,
    /// Discord could not be reached, or the database failed.
    Unreachable,
}

fn refused_by(error: &Error) -> PostRefusal {
    match roles::error_code(error) {
        Some(MISSING_ACCESS | MISSING_PERMISSIONS) => PostRefusal::NoAccess,
        Some(UNKNOWN_CHANNEL) => PostRefusal::UnknownChannel,
        Some(_) => PostRefusal::Refused,
        None => PostRefusal::Unreachable,
    }
}

/// Posts the panel in `channel`, or edits its message when it already sits there; a message somebody
/// deleted is posted afresh. A reactions panel gets its emojis put under the message.
pub async fn post_panel(app: &App, guild: u64, panel: &PanelRow, channel: u64) -> Result<PostedPanel, PostRefusal> {
    let entries = app.db.panel_roles(panel.id).await.map_err(|error| {
        tracing::warn!(guild, panel = panel.id, %error, "could not read a role panel");
        PostRefusal::Unreachable
    })?;
    if entries.is_empty() {
        return Err(PostRefusal::NoRoles);
    }
    let style = style_of(panel);
    let mut wanted: Vec<(String, u64, EmojiReactionType)> = Vec::new();
    if style == PanelStyle::Reactions {
        for entry in &entries {
            let Some(emoji) = entry.emoji.as_deref().and_then(parse_emoji) else {
                return Err(PostRefusal::MissingEmoji(entry.label.clone()));
            };
            let key = emoji_key(&emoji);
            if wanted.iter().any(|(k, ..)| *k == key) {
                return Err(PostRefusal::SameEmoji(entry.label.clone()));
            }
            wanted.push((key, entry.role_id as u64, emoji));
        }
    }
    let lang = app.guild_lang(guild).await;
    let (embed, components) = panel_message(panel, &entries, lang);
    let embed = app.brand_embed(guild, embed).await;
    let channel_id = Id::new(channel.max(1));
    let before = panel.channel_id.zip(panel.message_id);

    // Same channel: edit the message in place. A message someone deleted is posted afresh.
    let mut posted: Option<(u64, bool)> = None;
    if let Some((_, message)) = before.filter(|(posted_in, _)| *posted_in as u64 == channel) {
        match app
            .http
            .update_message(channel_id, Id::new(message as u64))
            .embeds(Some(std::slice::from_ref(&embed)))
            .components(Some(&components))
            .await
        {
            Ok(_) => posted = Some((message as u64, true)),
            Err(error) if roles::error_code(&error) == Some(UNKNOWN_MESSAGE) => {}
            Err(error) => {
                tracing::warn!(guild, panel = panel.id, %error, "could not update a role panel");
                return Err(refused_by(&error));
            }
        }
    }
    let (message, updated) = match posted {
        Some(found) => found,
        None => {
            let made = app
                .http
                .create_message(channel_id)
                .embeds(&[embed])
                .components(&components)
                .await;
            let message = match made {
                Ok(response) => response.model().await.map_err(|error| {
                    tracing::warn!(guild, panel = panel.id, %error, "could not read the posted role panel");
                    PostRefusal::Unreachable
                })?,
                Err(error) => {
                    tracing::warn!(guild, panel = panel.id, %error, "could not post a role panel");
                    return Err(refused_by(&error));
                }
            };
            app.db
                .set_panel_message(panel.id, channel, message.id.get())
                .await
                .map_err(|error| {
                    tracing::warn!(guild, panel = panel.id, %error, "could not remember a role panel's message");
                    PostRefusal::Unreachable
                })?;
            (message.id.get(), false)
        }
    };
    let moved_from = before
        .filter(|(posted_in, _)| *posted_in as u64 != channel)
        .map(|(posted_in, _)| posted_in as u64);
    let unusable = sync_reactions(app, guild, panel, channel_id, message, updated, &wanted).await;
    crate::commands::roles::automation::reactions::forget(message).await;
    Ok(PostedPanel {
        channel,
        message,
        updated,
        moved_from,
        unusable,
    })
}

/// Puts the panel's emojis under its message and remembers which gives what; takes away the emojis of
/// roles that are no longer on the panel, and all of them when the panel is not a reactions panel any more.
async fn sync_reactions(
    app: &App,
    guild: u64,
    panel: &PanelRow,
    channel: Id<ChannelMarker>,
    message: u64,
    updated: bool,
    wanted: &[(String, u64, EmojiReactionType)],
) -> Vec<String> {
    let message_id = Id::new(message.max(1));
    let before = match app.db.panel_reactions(panel.id).await {
        Ok(rows) => rows,
        Err(error) => {
            tracing::warn!(guild, panel = panel.id, %error, "could not read a panel's reactions");
            Vec::new()
        }
    };
    if updated {
        if wanted.is_empty() && !before.is_empty() {
            if let Err(error) = app.http.delete_all_reactions(channel, message_id).await {
                tracing::warn!(guild, panel = panel.id, %error, "could not clear a panel's reactions");
            }
        } else {
            for (key, _) in before.iter().filter(|(k, _)| !wanted.iter().any(|(w, ..)| w == k)) {
                let Some(emoji) = KeyEmoji::parse(key) else { continue };
                if let Err(error) = app
                    .http
                    .delete_all_reaction(channel, message_id, &emoji.request())
                    .await
                {
                    tracing::warn!(guild, panel = panel.id, %error, "could not clear an old reaction");
                }
            }
        }
    }
    let mut unusable = Vec::new();
    for (key, _, emoji) in wanted {
        if let Err(error) = app
            .http
            .create_reaction(channel, message_id, &request_type(emoji))
            .await
        {
            tracing::warn!(guild, panel = panel.id, %error, "could not add a panel's reaction");
            unusable.push(key.clone());
        }
    }
    let saved: Vec<(String, u64)> = wanted.iter().map(|(k, r, _)| (k.clone(), *r)).collect();
    if let Err(error) = app.db.replace_panel_reactions(panel.id, &saved).await {
        tracing::warn!(guild, panel = panel.id, %error, "could not remember a panel's reactions");
    }
    unusable
}
