//! Critter Drop and Message in a Bottle: things that wash up in a channel for whoever clicks first.
//!
//! A critter drop shows three cards on one picture, a numbered button each: the first click takes a
//! card, nobody takes two. A bottle has one button and one opener. Both live in `collection_drops`, so
//! they survive restarts, and the ticker closes them when their time is up.

mod message;
use message::*;
mod claim;
pub(crate) use claim::*;
mod bottle;
pub use bottle::*;
#[cfg(test)]
mod depth_tests;


use std::{str::Split, sync::Arc, time::Duration};

use chrono::{Datelike, Duration as Span, Timelike};
use pinchora_core::infra::i18n::Lang;
use pinchora_core::t;
use pinchora_db::{BottleOpen, BottlePrize, DROP_BOTTLE, DROP_CRITTERS, DropRow, Grab, NewDrop, VoteCard};
use pinchora_game::Rarity;
use pinchora_game::collection;
use pinchora_game::collection::BOTTLE_SECS;
use pinchora_game::collection::Collection;
use pinchora_game::collection::DROP_SECS;
use pinchora_game::collection::Entry;
use pinchora_game::collection::MANUAL_CHANNEL_COOLDOWN_SECS;
use pinchora_game::collection::MANUAL_DROP_COOLDOWN_SECS;
use pinchora_game::collection::Prize;
use pinchora_game::lang as letters;
use pinchora_game::play::Game;
use pinchora_render::{BottleCard, DropCard, DropCritter, HaulCard, Theme};
use twilight_interactions::command::{CommandModel, CreateCommand};
use twilight_model::{
    channel::message::{
        AllowedMentions, Component, Embed,
        component::{ButtonStyle, Label, TextInput, TextInputStyle},
    },
    http::{attachment::Attachment, interaction::InteractionResponseData},
    id::Id,
};
use twilight_util::builder::embed::{EmbedBuilder, ImageSource};

use crate::App;
use crate::Cx;
use crate::commands::games::collection::award;
use crate::commands::games::collection::daily;
use crate::commands::games::collection::dice;
use crate::commands::games::collection::haul_tile;
use crate::commands::games::collection::money;
use crate::commands::games::collection::picture;
use crate::commands::games::collection::settings::settings;
use crate::commands::games::games::channel_of;
use crate::commands::games::games::edit_message;
use crate::commands::games::games::quiet;
use crate::commands::games::games::record;
use crate::community::server_events::button;
use crate::community::server_events::is_permanent;
use crate::community::server_events::row as button_row;
use crate::infra::util::BRAND;
use crate::infra::util::SUCCESS;
use crate::infra::util::WARN;
use crate::infra::util::mention;

#[derive(CommandModel, CreateCommand)]
#[command(
    name = "critterdrop",
    desc = "Drop three critters in this channel for everyone to grab",
    contexts = "guild"
)]
pub struct CritterDrop;

impl CritterDrop {
    pub async fn run(self, cx: &Cx) -> anyhow::Result<()> {
        let app = &cx.app;
        let (user, channel) = (cx.author()?.id.get(), channel_of(cx));
        let guild = cx.guild_id().unwrap_or(0);
        let lang = cx.lang();
        // What `/config critters` says holds for a hand-called drop too: switched off, or kept to one channel.
        let setting = settings(app, guild).await?;
        if !setting.enabled {
            return cx.reply_ephemeral(t!(lang, "drop.off")).await;
        }
        if let Some(only) = setting.channel_id.filter(|set| *set as u64 != channel) {
            return cx.reply_ephemeral(t!(lang, "drop.only-channel", channel = only)).await;
        }
        if app.db.drop_waiting_in(channel, app.now()).await? {
            return cx.reply_ephemeral(t!(lang, "drop.waiting")).await;
        }
        // The channel's cooldown first, whoever called last; the caller's own on top of it. A caller who
        // is still waiting must not cost the channel its turn, so the first is given back.
        let channel_wait = format!("critterdrop:channel:{channel}");
        if let Some(left) = app
            .cache
            .try_cooldown(&channel_wait, Duration::from_secs(MANUAL_CHANNEL_COOLDOWN_SECS))
            .await?
        {
            return cx
                .reply_ephemeral(t!(lang, "drop.channel-cooldown", minutes = left.as_secs() / 60 + 1))
                .await;
        }
        if let Some(left) = app
            .cache
            .try_cooldown(
                &format!("critterdrop:{user}"),
                Duration::from_secs(MANUAL_DROP_COOLDOWN_SECS),
            )
            .await?
        {
            app.cache.release_cooldown(&channel_wait).await?;
            return cx
                .reply_ephemeral(t!(lang, "drop.user-cooldown", minutes = left.as_secs() / 60 + 1))
                .await;
        }
        let row = create_critter_drop(app, guild, channel, Some(user), false).await?;
        // The drop is the channel's, so it speaks the server's language from its first post to its close.
        let (embed, png, buttons) = critter_message(app, app.guild_lang(guild).await, &row).await?;
        let row = prepare_ping(app, &row).await?;
        cx.reply(InteractionResponseData {
            embeds: Some(vec![embed]),
            attachments: Some(vec![Attachment::from_bytes("drop.png".to_owned(), png, 1)]),
            components: Some(buttons),
            content: row.protected_user_id.map(mention),
            allowed_mentions: ping_wish(&row),
            ..Default::default()
        })
        .await?;
        track_message(cx, row.id);
        Ok(())
    }
}

async fn prepare_ping(app: &App, row: &DropRow) -> anyhow::Result<DropRow> {
    let present = if let Some(user) = row.protected_user_id {
        match app
            .http
            .guild_member(Id::new(row.guild_id as u64), Id::new(user as u64))
            .await
        {
            Ok(response) => response.model().await.is_ok_and(|member| !member.user.bot),
            Err(_) => false,
        }
    } else {
        false
    };
    app.db.prepare_collection_ping(row.id, present).await
}

fn ping_wish(row: &DropRow) -> Option<AllowedMentions> {
    Some(AllowedMentions {
        users: row
            .protected_user_id
            .map(|id| vec![Id::new(id as u64)])
            .unwrap_or_default(),
        ..Default::default()
    })
}

/// Learns which message the command just posted and keeps it on the drop, so the ticker can edit it when
/// the drop ends. Best effort: without it the buttons are only retired when somebody clicks.
fn track_message(cx: &Cx, drop: i64) {
    let app = cx.app.clone();
    let responder = cx.responder.clone();
    // The drop stays keyed to the channel it was called in, which `/critterdrop` checks for a waiting one.
    let channel = channel_of(cx);
    cx.app.spawn(async move {
        // The answer to the command may still be on its way to Discord.
        tokio::time::sleep(Duration::from_millis(1_500)).await;
        let message = responder.original().await;
        match message {
            Ok((_, message)) => {
                if let Err(error) = app.db.set_drop_message(drop, channel, message).await {
                    tracing::debug!(%error, drop, "drop message not remembered");
                }
            }
            Err(error) => tracing::debug!(%error, drop, "could not look up the drop message"),
        }
    });
}

// /Critter drop

/// The critters of a drop in slot order.
fn cards(row: &DropRow) -> Vec<&'static Entry> {
    row.cards
        .iter()
        .filter_map(|id| Collection::get().entry(id.strip_prefix("g:").unwrap_or(id)))
        .collect()
}

/// Opens a critter drop in the database: three different critters, open for [`DROP_SECS`].
async fn create_critter_drop(
    app: &App,
    guild: u64,
    channel: u64,
    host: Option<u64>,
    variety: bool,
) -> anyhow::Result<DropRow> {
    let now = app.now();
    let ids: Vec<String> = collection::drop_cards(now.hour(), now.month(), &mut dice())
        .iter()
        .map(|e| {
            if rand::random::<f64>() < collection::GLIMMER_SHARE {
                format!("g:{}", e.id)
            } else {
                e.id.clone()
            }
        })
        .collect();
    let row = app
        .db
        .create_drop(NewDrop {
            kind: DROP_CRITTERS,
            guild_id: guild,
            channel_id: channel,
            host,
            cards: &ids,
            expires_at: app.now() + Span::seconds(DROP_SECS),
        })
        .await?;
    let roll = rand::random::<f64>();
    let (mode, word) = if !variety {
        ("standard", None)
    } else if roll < 0.15 {
        ("hidden", None)
    } else if roll < 0.30 {
        ("word", Some(format!("{:08x}", rand::random::<u32>())))
    } else if roll < 0.45 {
        ("vote", None)
    } else {
        ("standard", None)
    };
    app.db.set_drop_mode(row.id, mode, word.as_deref()).await
}

fn sparkles(rarity: Rarity) -> u8 {
    match rarity {
        Rarity::Common | Rarity::Uncommon => 0,
        Rarity::Rare => 1,
        Rarity::Epic => 2,
        Rarity::Legendary => 3,
    }
}

/// Posts a chat-driven critter drop in `channel`. False when Discord would not take it (the bot cannot
/// post there), which is no reason to try again at once.
pub async fn post_critter_drop(app: &Arc<App>, guild: u64, channel: u64) -> anyhow::Result<bool> {
    post_critter_drop_mode(app, guild, channel, false).await
}

pub(crate) async fn post_variety_drop(app: &Arc<App>, guild: u64, channel: u64) -> anyhow::Result<bool> {
    post_critter_drop_mode(app, guild, channel, true).await
}

async fn post_critter_drop_mode(app: &Arc<App>, guild: u64, channel: u64, variety: bool) -> anyhow::Result<bool> {
    let lang = app.guild_lang(guild).await;
    let row = create_critter_drop(app, guild, channel, None, variety).await?;
    let (embed, png, buttons) = match critter_message(app, lang, &row).await {
        Ok(message) => message,
        Err(error) => {
            app.db.abort_drop(row.id).await?;
            return Err(error);
        }
    };
    post(app, &row, embed, "drop.png", png, buttons).await
}

/// Posts the message of a drop that was just created and remembers where it went; takes the drop
/// back when Discord refuses.
async fn post(
    app: &Arc<App>,
    row: &DropRow,
    embed: Embed,
    filename: &str,
    png: Vec<u8>,
    buttons: Vec<Component>,
) -> anyhow::Result<bool> {
    let prepared;
    let row = if row.kind == DROP_CRITTERS {
        prepared = prepare_ping(app, row).await?;
        &prepared
    } else {
        row
    };
    let channel = row.channel_id as u64;
    let embeds = [app.brand_embed(row.guild_id as u64, embed).await];
    let attachments = [Attachment::from_bytes(filename.to_owned(), png, 1)];
    let allowed = if row.kind == DROP_CRITTERS {
        ping_wish(row)
    } else {
        quiet()
    };
    let ping = row.protected_user_id.map(mention).unwrap_or_default();
    let posted = app
        .http
        .create_message(Id::new(channel.max(1)))
        .content(&ping)
        .embeds(&embeds)
        .attachments(&attachments)
        .components(&buttons)
        .allowed_mentions(allowed.as_ref())
        .await;
    match posted {
        Ok(response) => {
            let message = response.model().await?;
            app.db.set_drop_message(row.id, channel, message.id.get()).await?;
            Ok(true)
        }
        Err(error) => {
            app.db.abort_drop(row.id).await?;
            if is_permanent(&error) {
                tracing::debug!(%error, channel, "could not post a drop");
                Ok(false)
            } else {
                Err(error.into())
            }
        }
    }
}

async fn claim(cx: &Cx, id: i64, slot: usize, answer: Option<&str>) -> anyhow::Result<()> {
    let (app, user, lang) = (&cx.app, cx.author()?.id.get(), cx.lang());
    let Some(row) = app.db.drop_row(id).await?.filter(|r| {
        r.kind == DROP_CRITTERS && Some(r.guild_id as u64) == cx.guild_id() && r.channel_id as u64 == channel_of(cx)
    }) else {
        return cx.reply_ephemeral(t!(lang, "drop.gone")).await;
    };
    let Some(entry) = row
        .cards
        .get(slot)
        .and_then(|c| Collection::get().entry(c.strip_prefix("g:").unwrap_or(c)))
    else {
        return cx.reply_ephemeral(t!(lang, "drop.gone")).await;
    };
    let outcome = if let Some(answer) = answer {
        app.db
            .grab_word_card(
                id,
                slot,
                user,
                app.now(),
                award(entry),
                daily(app, Game::CritterDrop),
                answer,
            )
            .await?
    } else {
        app.db
            .grab_card(id, slot, user, app.now(), award(entry), daily(app, Game::CritterDrop))
            .await?
    };
    let (granted, drop, paid) = match outcome {
        Grab::Gone => return cx.reply_ephemeral(t!(lang, "drop.gone")).await,
        Grab::Have { slot } => {
            return cx.reply_ephemeral(t!(lang, "drop.have", number = slot + 1)).await;
        }
        Grab::Lost { by } => {
            return cx
                .reply_ephemeral(t!(
                    lang,
                    "drop.too-slow",
                    user = mention(by),
                    name = entry.name_in(lang)
                ))
                .await;
        }
        Grab::Protected => return cx.reply_ephemeral(t!(lang, "collection-depth.protected")).await,
        Grab::Limit => return cx.reply_ephemeral(t!(lang, "collection-depth.claim-limit")).await,
        Grab::Voting => return cx.reply_ephemeral(t!(lang, "collection-depth.vote-closed")).await,
        Grab::Challenge => return cx.reply_ephemeral(t!(lang, "collection-depth.wrong-word")).await,
        Grab::Taken { granted, drop, paid } => (granted, drop, paid),
    };
    record(app, Game::CritterDrop, user, true).await;
    // The card first, for everyone who is watching (in the server's language); the details are only for the one
    // who grabbed it.
    let table = app.guild_lang(drop.guild_id as u64).await;
    cx.update(InteractionResponseData {
        embeds: Some(vec![critter_embed(&drop, false, table)]),
        components: Some(critter_buttons(&drop, false)),
        allowed_mentions: quiet(),
        ..Default::default()
    })
    .await?;
    let footer = match (paid, granted.new) {
        (false, _) => t!(lang, "drop.footer-limit"),
        (true, true) => t!(lang, "drop.footer-new"),
        (true, false) => t!(lang, "drop.footer-spare"),
    };
    let mut tile = haul_tile(app, lang, entry, Some(&granted), HaulCard::SINGLE_ART_BOX);
    if row.cards[slot].starts_with("g:") {
        tile.art = crate::commands::games::collection::glimmer_picture(app, entry, HaulCard::SINGLE_ART_BOX);
        tile.frame = 0x9af5ec;
        tile.tag = t!(lang, "collection-depth.glimmer");
    }
    let card = HaulCard {
        title: Game::CritterDrop.name(lang),
        subtitle: t!(lang, "drop.card-number", number = slot + 1),
        tiles: vec![tile],
        footer,
        theme: Theme::reef(),
    };
    let png = app.render(move |r| r.haul_card(&card)).await?;
    let pay = money(lang, &granted);
    let summary = t!(
        lang,
        "drop.took",
        name = entry.name_in(lang),
        rarity = letters::lower(lang, entry.rarity.label_in(lang)),
        what = format!(
            "{}{}",
            if granted.new {
                t!(lang, "drop.took-new")
            } else {
                t!(lang, "drop.took-spare")
            },
            if pay.is_empty() {
                String::new()
            } else {
                format!(" · {pay}")
            }
        )
    );
    let embed = EmbedBuilder::new()
        .color(if granted.new { SUCCESS } else { BRAND })
        .description(summary);
    cx.reply(Cx::card(embed, "critter.png", png, Vec::new(), true)).await
}

// /Closing

pub(crate) async fn reveal_vote(app: &Arc<App>, row: &DropRow) {
    let Some(message) = row.message_id else { return };
    let lang = app.guild_lang(row.guild_id as u64).await;
    edit_message(
        app,
        row.channel_id as u64,
        message as u64,
        None,
        Some(&[critter_embed(row, false, lang)]),
        Some(&critter_buttons(row, false)),
    )
    .await;
}

/// Tells the channel a drop is over: retires its buttons and says what happened to the cards.
pub(crate) async fn close(app: &Arc<App>, row: &DropRow) {
    let Some(message) = row.message_id else { return };
    // Nobody in particular is answered: the server's own language.
    let lang = app.guild_lang(row.guild_id as u64).await;
    let (embed, buttons) = if row.kind == DROP_CRITTERS {
        (critter_embed(row, true, lang), critter_buttons(row, true))
    } else {
        (bottle_embed(row, None, lang), bottle_buttons(row, true, lang))
    };
    edit_message(
        app,
        row.channel_id as u64,
        message as u64,
        None,
        Some(&[embed]),
        Some(&buttons),
    )
    .await;
}
