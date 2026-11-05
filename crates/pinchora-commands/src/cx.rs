use std::sync::Arc;

use twilight_model::{
    application::interaction::Interaction,
    channel::message::{Component, Embed, MessageFlags},
    guild::Permissions,
    http::{attachment::Attachment, interaction::InteractionResponseData},
    user::User,
};

use twilight_util::builder::embed::{EmbedBuilder, ImageSource};

use crate::{App, Responder};

/// Per-interaction context handed to every command.
pub struct Cx {
    pub app: Arc<App>,
    /// The interaction, with `data` already taken out for parsing.
    pub interaction: Interaction,
    pub responder: Arc<Responder>,
}

impl Cx {
    pub fn author(&self) -> anyhow::Result<&User> {
        self.interaction
            .author()
            .ok_or_else(|| anyhow::anyhow!("interaction without author"))
    }

    pub fn guild_id(&self) -> Option<u64> {
        self.interaction.guild_id.map(|id| id.get())
    }

    /// Whether the invoker has Manage Server (or Administrator) here.
    pub fn can_manage_guild(&self) -> bool {
        self.interaction
            .member
            .as_ref()
            .and_then(|m| m.permissions)
            .is_some_and(|p| p.contains(Permissions::MANAGE_GUILD) || p.contains(Permissions::ADMINISTRATOR))
    }

    /// Server nickname, then global display name, then username.
    pub fn author_display_name(&self) -> String {
        let nick = self.interaction.member.as_ref().and_then(|m| m.nick.clone());
        let user = self.interaction.author();
        nick.or_else(|| user.and_then(|u| u.global_name.clone()))
            .or_else(|| user.map(|u| u.name.clone()))
            .unwrap_or_default()
    }

    pub async fn reply(&self, data: InteractionResponseData) -> anyhow::Result<()> {
        self.responder.respond(data, false).await
    }

    pub async fn update(&self, data: InteractionResponseData) -> anyhow::Result<()> {
        self.responder.respond(data, true).await
    }

    /// Opens a modal; it has to be the first answer.
    pub async fn modal(&self, data: InteractionResponseData) -> anyhow::Result<()> {
        self.responder.modal(data).await
    }

    /// The channel and id of the message a component was clicked on.
    pub fn clicked_message(&self) -> Option<(u64, u64)> {
        self.interaction
            .message
            .as_ref()
            .map(|m| (m.channel_id.get(), m.id.get()))
    }

    pub async fn reply_text(&self, content: impl Into<String>) -> anyhow::Result<()> {
        self.reply(InteractionResponseData {
            content: Some(content.into()),
            ..Default::default()
        })
        .await
    }

    pub async fn reply_ephemeral(&self, content: impl Into<String>) -> anyhow::Result<()> {
        self.reply(InteractionResponseData {
            content: Some(content.into()),
            flags: Some(MessageFlags::EPHEMERAL),
            ..Default::default()
        })
        .await
    }

    pub async fn reply_embed(&self, embed: Embed) -> anyhow::Result<()> {
        self.reply(InteractionResponseData {
            embeds: Some(vec![embed]),
            ..Default::default()
        })
        .await
    }

    pub async fn reply_embed_components(&self, embed: Embed, components: Vec<Component>) -> anyhow::Result<()> {
        self.reply(InteractionResponseData {
            embeds: Some(vec![embed]),
            components: Some(components),
            ..Default::default()
        })
        .await
    }

    /// An embed showing `png` as its image, plus optional components.
    pub fn card(
        embed: EmbedBuilder,
        filename: &str,
        png: Vec<u8>,
        components: Vec<Component>,
        ephemeral: bool,
    ) -> InteractionResponseData {
        let embed = match ImageSource::attachment(filename) {
            Ok(source) => embed.image(source),
            Err(_) => embed,
        };
        InteractionResponseData {
            embeds: Some(vec![embed.build()]),
            attachments: Some(vec![Attachment::from_bytes(filename.to_owned(), png, 1)]),
            components: Some(components),
            flags: ephemeral.then_some(MessageFlags::EPHEMERAL),
            ..Default::default()
        }
    }

    pub async fn reply_png(&self, filename: &str, png: Vec<u8>) -> anyhow::Result<()> {
        self.reply(InteractionResponseData {
            attachments: Some(vec![Attachment::from_bytes(filename.to_owned(), png, 1)]),
            ..Default::default()
        })
        .await
    }
}
