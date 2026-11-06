//! The control panel of a room: an embed that says how the room stands and the buttons that change it.
//!
//! One panel message lives in the room's own chat (a voice channel has a text chat) or in the private thread
//! paired with it. Every button's custom id is `vc:<action>` with nothing else in it: the room is the one the
//! panel sits in, or else the one the member clicking is in.

use pinchora_core::{i18n::Lang, t};
use pinchora_db::TempRoom;
use twilight_model::{
    channel::message::{Component, Embed, component::ButtonStyle},
    id::Id,
};
use twilight_util::builder::embed::{EmbedBuilder, EmbedFooterBuilder};

use crate::{
    App,
    commands::{games::quiet, tempvoice::lang_of},
    server_events::{button, row},
    util::BRAND,
};

/// The panel buttons' actions, as the part of the custom id after `vc:`.
pub(crate) const LOCK: &str = "lk";
pub(crate) const UNLOCK: &str = "ul";
pub(crate) const LIMIT: &str = "lm";
pub(crate) const RENAME: &str = "rn";
pub(crate) const KICK: &str = "kk";
pub(crate) const TRANSFER: &str = "tr";
pub(crate) const HIDE: &str = "hd";
pub(crate) const CLAIM: &str = "cl";

fn id(action: &str) -> String {
    format!("vc:{action}")
}

/// The panel for `room`, in `lang`.
pub(crate) fn message(lang: Lang, room: &TempRoom) -> (Embed, Vec<Component>) {
    let owner = match room.owner_id {
        Some(owner) => t!(lang, "voice.panel.owner", owner = format!("<@{owner}>")),
        None => t!(lang, "voice.panel.no-owner"),
    };
    let lock = if room.locked {
        t!(lang, "voice.panel.locked")
    } else {
        t!(lang, "voice.panel.open")
    };
    let hidden = if room.hidden {
        t!(lang, "voice.panel.hidden")
    } else {
        t!(lang, "voice.panel.visible")
    };
    let limit = if room.user_limit > 0 {
        t!(lang, "voice.panel.limit", count = room.user_limit)
    } else {
        t!(lang, "voice.panel.no-limit")
    };
    let embed = EmbedBuilder::new()
        .color(BRAND)
        .title(t!(lang, "voice.panel.title"))
        .description(format!("{owner}\n{lock} · {hidden} · {limit}"))
        .footer(EmbedFooterBuilder::new(t!(lang, "voice.panel.footer")))
        .build();
    let buttons = |list: &[(&str, &str, &str, ButtonStyle)]| {
        row(list
            .iter()
            .map(|(action, key, emoji, style)| button(id(action), &t!(lang, key), Some(emoji), *style, false))
            .collect())
    };
    let components = vec![
        buttons(&[
            (LOCK, "voice.btn.lock", "🔒", ButtonStyle::Secondary),
            (UNLOCK, "voice.btn.unlock", "🔓", ButtonStyle::Secondary),
            (LIMIT, "voice.btn.limit", "👥", ButtonStyle::Secondary),
            (RENAME, "voice.btn.rename", "✏️", ButtonStyle::Secondary),
        ]),
        buttons(&[
            (KICK, "voice.btn.kick", "🚪", ButtonStyle::Danger),
            (TRANSFER, "voice.btn.transfer", "👑", ButtonStyle::Secondary),
            (HIDE, "voice.btn.hide", "👁️", ButtonStyle::Secondary),
            (CLAIM, "voice.btn.claim", "🙋", ButtonStyle::Primary),
        ]),
    ];
    (embed, components)
}

/// Posts the panel of a new room where it belongs (`panel_channel`) and returns the message's id.
pub(crate) async fn post(app: &App, lang: Lang, room: &TempRoom, panel_channel: u64) -> Option<u64> {
    let (embed, components) = message(lang, room);
    let sent = app
        .http
        .create_message(Id::new(panel_channel.max(1)))
        .embeds(&[embed])
        .components(&components)
        .allowed_mentions(quiet().as_ref())
        .await;
    match sent {
        Ok(response) => response.model().await.ok().map(|m| m.id.get()),
        Err(error) => {
            tracing::debug!(%error, channel = panel_channel, "temp voice: the panel was not posted");
            None
        }
    }
}

/// Makes the panel message say how the room stands now. A message somebody deleted is left alone.
pub(crate) async fn refresh(app: &App, room: &TempRoom) {
    let (Some(channel), Some(posted)) = (room.panel_channel_id, room.panel_message_id) else {
        return;
    };
    let lang = lang_of(app, room.guild_id as u64).await;
    let (embed, components) = message(lang, room);
    let edited = app
        .http
        .update_message(Id::new(channel as u64), Id::new(posted as u64))
        .embeds(Some(&[embed]))
        .components(Some(&components))
        .await;
    if let Err(error) = edited {
        tracing::debug!(%error, channel, "temp voice: the panel was not refreshed");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn room() -> TempRoom {
        TempRoom {
            channel_id: 10,
            guild_id: 1,
            source_id: 2,
            owner_id: Some(33),
            created_at: chrono::Utc::now(),
            locked: false,
            hidden: false,
            user_limit: 0,
            last_rename_at: None,
            text_channel_id: None,
            panel_channel_id: None,
            panel_message_id: None,
        }
    }

    #[test]
    fn the_panel_has_eight_buttons_in_two_rows_and_says_how_the_room_stands() {
        let (embed, rows) = message(Lang::En, &room());
        let description = embed.description.unwrap();
        assert!(
            description.contains("<@33>") && description.contains("Open"),
            "{description}"
        );
        let ids: Vec<String> = rows
            .iter()
            .flat_map(|r| match r {
                Component::ActionRow(r) => r.components.clone(),
                _ => Vec::new(),
            })
            .filter_map(|c| match c {
                Component::Button(b) => b.custom_id,
                _ => None,
            })
            .collect();
        assert_eq!(
            ids,
            ["vc:lk", "vc:ul", "vc:lm", "vc:rn", "vc:kk", "vc:tr", "vc:hd", "vc:cl"]
        );
        assert!(ids.iter().all(|i| i.len() <= 100));
        let mut locked = room();
        locked.locked = true;
        locked.hidden = true;
        locked.user_limit = 4;
        locked.owner_id = None;
        let (embed, _) = message(Lang::En, &locked);
        let description = embed.description.unwrap();
        assert!(description.contains("Locked") && description.contains("Hidden") && description.contains('4'));
        assert!(
            description.contains("Claim"),
            "an ownerless room points at Claim: {description}"
        );
    }
}
