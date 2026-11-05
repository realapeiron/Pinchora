use std::sync::Arc;

use twilight_model::{
    application::interaction::Interaction,
    channel::message::{
        Component, Embed, MessageFlags,
        component::{Label, TextInput, TextInputStyle},
    },
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

    /// Opens a modal with one single-line text box, filled with `value` when there is one. The
    /// answer comes back as a modal submit with `custom_id`, and the text under `text`.
    #[allow(deprecated)] // `TextInput::label` moved to the label around it, but the field must be set
    pub async fn text_modal(
        &self,
        custom_id: &str,
        title: &str,
        label: &str,
        placeholder: &str,
        value: Option<&str>,
        max_length: u16,
    ) -> anyhow::Result<()> {
        let input = TextInput {
            custom_id: "text".to_owned(),
            id: None,
            label: None,
            max_length: Some(max_length),
            min_length: Some(1),
            placeholder: Some(placeholder.to_owned()),
            required: Some(true),
            style: TextInputStyle::Short,
            value: value.map(str::to_owned),
        };
        let components = vec![Component::Label(Label {
            component: Box::new(Component::TextInput(input)),
            description: None,
            id: None,
            label: label.to_owned(),
        })];
        self.responder
            .modal(InteractionResponseData {
                custom_id: Some(custom_id.to_owned()),
                title: Some(title.to_owned()),
                components: Some(components),
                ..Default::default()
            })
            .await
    }

    pub async fn reply_png(&self, filename: &str, png: Vec<u8>) -> anyhow::Result<()> {
        self.reply(InteractionResponseData {
            attachments: Some(vec![Attachment::from_bytes(filename.to_owned(), png, 1)]),
            ..Default::default()
        })
        .await
    }
}
