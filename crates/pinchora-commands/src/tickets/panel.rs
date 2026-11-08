//! A panel's message: its embed and its buttons or menu, posted in its channel or edited in place.

use std::sync::Arc;

use pinchora_core::{
    i18n::Lang,
    t,
    tickets::{PanelStyle, Tone},
};
use pinchora_db::TicketPanel;
use twilight_model::{
    channel::message::{
        Component, Embed, EmojiReactionType,
        component::{ActionRow, Button, ButtonStyle, SelectMenu, SelectMenuOption, SelectMenuType},
    },
    id::Id,
};
use twilight_util::builder::embed::{EmbedBuilder, EmbedFooterBuilder};

use crate::{
    App,
    commands::games::quiet,
    roles,
    util::{single_line, strip_mentions},
};

use super::{Need, discord_refusal};

const UNKNOWN_MESSAGE: u64 = 10008;

/// How posting a panel went.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PanelPost {
    /// It is up in `channel`; `updated` when an earlier post was edited in place.
    Posted { channel: u64, message: u64, updated: bool },
    /// Discord would not take it; the sentence says why (a missing permission, a deleted channel).
    Refused(String),
    /// No such panel.
    Gone,
}

pub(crate) fn open_button_id(panel: i64, topic: usize) -> String {
    format!("tk:o:{panel}:{topic}")
}

pub(crate) fn menu_id(panel: i64) -> String {
    format!("tk:s:{panel}")
}

fn emoji(text: Option<&str>) -> Option<EmojiReactionType> {
    text.filter(|e| pinchora_core::tickets::is_emoji(e))
        .map(|name| EmojiReactionType::Unicode {
            name: name.trim().to_owned(),
        })
}

fn style(tone: Tone) -> ButtonStyle {
    match tone {
        Tone::Primary => ButtonStyle::Primary,
        Tone::Secondary => ButtonStyle::Secondary,
        Tone::Success => ButtonStyle::Success,
        Tone::Danger => ButtonStyle::Danger,
    }
}

/// The panel's embed: its title, its text, its colour. A manager's text keeps its markdown but pings nobody.
pub(crate) fn panel_embed(panel: &TicketPanel, lang: Lang) -> Embed {
    let mut embed = EmbedBuilder::new()
        .title(single_line(&strip_mentions(&panel.title)))
        .color(panel.color as u32 & 0xff_ffff)
        .footer(EmbedFooterBuilder::new(t!(lang, "tickets.panel.footer")));
    let description = strip_mentions(&panel.description);
    if !description.is_empty() {
        embed = embed.description(description);
    }
    embed.build()
}

/// One row of buttons, or one menu, opening a ticket on each topic.
pub(crate) fn panel_components(panel: &TicketPanel, lang: Lang) -> Vec<Component> {
    let disabled = !panel.enabled;
    let inner = match panel.style {
        PanelStyle::Buttons => panel
            .topics
            .iter()
            .take(pinchora_core::tickets::MAX_BUTTONS)
            .enumerate()
            .map(|(n, topic)| {
                Component::Button(Button {
                    custom_id: Some(open_button_id(panel.id, n)),
                    disabled,
                    emoji: emoji(topic.emoji.as_deref()),
                    id: None,
                    label: Some(single_line(&topic.label)),
                    sku_id: None,
                    style: style(topic.tone),
                    url: None,
                })
            })
            .collect(),
        PanelStyle::Select => {
            let options = panel
                .topics
                .iter()
                .take(pinchora_core::tickets::MAX_OPTIONS)
                .enumerate()
                .map(|(n, topic)| SelectMenuOption {
                    default: false,
                    description: topic.description.as_deref().map(single_line).filter(|d| !d.is_empty()),
                    emoji: emoji(topic.emoji.as_deref()),
                    label: single_line(&topic.label),
                    value: n.to_string(),
                })
                .collect();
            vec![Component::SelectMenu(SelectMenu {
                channel_types: None,
                custom_id: menu_id(panel.id),
                default_values: None,
                disabled,
                id: None,
                kind: SelectMenuType::Text,
                max_values: Some(1),
                min_values: Some(1),
                options: Some(options),
                placeholder: Some(t!(lang, "tickets.panel.pick")),
                required: None,
            })]
        }
    };
    if inner.is_empty() {
        return Vec::new();
    }
    vec![Component::ActionRow(ActionRow {
        id: None,
        components: inner,
    })]
}

/// Posts panel `id` in its channel, or edits the message it was posted as before (posted elsewhere or
/// deleted since, it is posted anew). The dashboard's **Post** and `/tickets panel post` both come here;
/// a refusal is told in `lang`, the panel itself speaks the server's language.
pub async fn post_panel(app: &Arc<App>, id: i64, lang: Lang) -> anyhow::Result<PanelPost> {
    let Some(panel) = app.db.ticket_panel(id).await? else {
        return Ok(PanelPost::Gone);
    };
    let shown = app.guild_lang(panel.guild_id as u64).await;
    let embeds = [app.brand_embed(panel.guild_id as u64, panel_embed(&panel, shown)).await];
    let components = panel_components(&panel, shown);
    let channel = Id::new(panel.channel_id as u64);
    if let Some(message) = panel.message_id {
        let edited = app
            .http
            .update_message(channel, Id::new(message as u64))
            .embeds(Some(&embeds))
            .components(Some(&components))
            .await;
        match edited {
            Ok(_) => {
                return Ok(PanelPost::Posted {
                    channel: channel.get(),
                    message: message as u64,
                    updated: true,
                });
            }
            Err(error) if roles::error_code(&error) == Some(UNKNOWN_MESSAGE) => {}
            Err(error) => {
                return Ok(PanelPost::Refused(discord_refusal(
                    &error,
                    lang,
                    Need::Post(channel.get()),
                )));
            }
        }
    }
    let posted = app
        .http
        .create_message(channel)
        .embeds(&embeds)
        .components(&components)
        .allowed_mentions(quiet().as_ref())
        .await;
    let message = match posted {
        Ok(response) => response.model().await?,
        Err(error) => {
            tracing::debug!(panel = id, %error, "ticket panel not posted");
            return Ok(PanelPost::Refused(discord_refusal(
                &error,
                lang,
                Need::Post(channel.get()),
            )));
        }
    };
    app.db
        .set_ticket_panel_message(id, channel.get(), Some(message.id.get()))
        .await?;
    Ok(PanelPost::Posted {
        channel: channel.get(),
        message: message.id.get(),
        updated: false,
    })
}
