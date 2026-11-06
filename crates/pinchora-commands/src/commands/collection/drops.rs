//! Critter Drop and Message in a Bottle: things that wash up in a channel for whoever clicks first.
//!
//! A critter drop shows three cards on one picture, a numbered button each: the first click takes a
//! card, nobody takes two. A bottle has one button and one opener. Both live in `collection_drops`, so
//! they survive restarts, and the ticker closes them when their time is up.

use std::{str::Split, sync::Arc, time::Duration};

use chrono::{Datelike, Duration as Span, Timelike};
use pinchora_core::i18n::Lang;
use pinchora_db::{BottleOpen, BottlePrize, DROP_BOTTLE, DROP_CRITTERS, DropRow, Grab, NewDrop};
use pinchora_game::{
    Rarity,
    collection::{
        self, BOTTLE_SECS, Collection, DROP_SECS, Entry, MANUAL_CHANNEL_COOLDOWN_SECS, MANUAL_DROP_COOLDOWN_SECS, Prize,
    },
    play::Game,
};
use pinchora_render::{BottleCard, DropCard, DropCritter, HaulCard, Theme};
use twilight_interactions::command::{CommandModel, CreateCommand};
use twilight_model::{
    channel::message::{Component, Embed, component::ButtonStyle},
    http::{attachment::Attachment, interaction::InteractionResponseData},
    id::Id,
};
use twilight_util::builder::embed::{EmbedBuilder, ImageSource};

use super::{award, daily, dice, haul_tile, money, picture, settings::settings};
use crate::{
    App, Cx,
    commands::games::{channel_of, edit_message, quiet, record},
    server_events::{button, is_permanent, row as button_row},
    util::{BRAND, SUCCESS, WARN, mention},
};

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
        // What `/config critters` says holds for a hand-called drop too: switched off, or kept to one channel.
        let setting = settings(app, guild).await?;
        if !setting.enabled {
            return cx
                .reply_ephemeral(
                    "Critter drops are switched off in this server. A manager can turn them on with \
                     `/config critters enabled:true`.",
                )
                .await;
        }
        if let Some(only) = setting.channel_id.filter(|set| *set as u64 != channel) {
            return cx
                .reply_ephemeral(format!("This server keeps critter drops to <#{only}>. Call one there!"))
                .await;
        }
        if app.db.drop_waiting_in(channel, app.now()).await? {
            return cx
                .reply_ephemeral("Something is still waiting in this channel. Let somebody grab it first!")
                .await;
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
                .reply_ephemeral(format!(
                    "This channel had a drop a little while ago. The next one can be called in {} minutes.",
                    left.as_secs() / 60 + 1
                ))
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
                .reply_ephemeral(format!(
                    "You called a drop a little while ago. The next one is ready in {} minutes.",
                    left.as_secs() / 60 + 1
                ))
                .await;
        }
        let row = create_critter_drop(app, guild, channel, Some(user)).await?;
        let (embed, png, buttons) = critter_message(app, &row).await?;
        cx.reply(InteractionResponseData {
            embeds: Some(vec![embed]),
            attachments: Some(vec![Attachment::from_bytes("drop.png".to_owned(), png, 1)]),
            components: Some(buttons),
            allowed_mentions: quiet(),
            ..Default::default()
        })
        .await?;
        track_message(cx, row.id);
        Ok(())
    }
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
    row.cards.iter().filter_map(|id| Collection::get().entry(id)).collect()
}

/// Opens a critter drop in the database: three different critters, open for [`DROP_SECS`].
async fn create_critter_drop(app: &App, guild: u64, channel: u64, host: Option<u64>) -> anyhow::Result<DropRow> {
    let now = app.now();
    let ids: Vec<String> = collection::drop_cards(now.hour(), now.month(), &mut dice())
        .iter()
        .map(|e| e.id.clone())
        .collect();
    app.db
        .create_drop(NewDrop {
            kind: DROP_CRITTERS,
            guild_id: guild,
            channel_id: channel,
            host,
            cards: &ids,
            expires_at: app.now() + Span::seconds(DROP_SECS),
        })
        .await
}

fn sparkles(rarity: Rarity) -> u8 {
    match rarity {
        Rarity::Common | Rarity::Uncommon => 0,
        Rarity::Rare => 1,
        Rarity::Epic => 2,
        Rarity::Legendary => 3,
    }
}

/// The message of an open critter drop: its embed, the picture of the three cards, and the buttons.
async fn critter_message(app: &App, row: &DropRow) -> anyhow::Result<(Embed, Vec<u8>, Vec<Component>)> {
    let card = DropCard {
        title: "Critter drop".to_owned(),
        subtitle: "Grab one!".to_owned(),
        cards: cards(row)
            .into_iter()
            .map(|e| DropCritter {
                art: picture(app, e, DropCard::ART_BOX),
                name: e.name.clone(),
                rarity: e.rarity.label().to_owned(),
                frame: e.rarity.color(),
                habitat: e.place().to_owned(),
                sparkles: sparkles(e.rarity),
            })
            .collect(),
        footer: format!("Press 1, 2 or 3 · one card each · gone in {DROP_SECS} seconds"),
        theme: Theme::reef(),
    };
    let png = app.render(move |r| r.drop_card(&card)).await?;
    Ok((critter_embed(row, false), png, critter_buttons(row, false)))
}

/// The text of a drop: what each card is and who has it. `over`: its time is up.
fn critter_embed(row: &DropRow, over: bool) -> Embed {
    let lines: Vec<String> = cards(row)
        .iter()
        .zip(&row.takers)
        .enumerate()
        .map(|(i, (entry, taker))| {
            let who = match (*taker, over) {
                (0, false) => "free".to_owned(),
                (0, true) => "drifted away".to_owned(),
                (taker, _) => format!("taken by {}", mention(taker)),
            };
            format!(
                "**{}** · {} *({})* — {who}",
                i + 1,
                entry.name,
                entry.rarity.label().to_lowercase()
            )
        })
        .collect();
    let all_gone = row.takers.iter().all(|t| *t != 0);
    let (title, intro, color) = if over && all_gone {
        ("🦀 Critter drop over", "Every card found a home.".to_owned(), SUCCESS)
    } else if over {
        (
            "🦀 Critter drop over",
            "The tide took what nobody grabbed.".to_owned(),
            WARN,
        )
    } else if all_gone {
        ("🦀 Critter drop", "Every card has a new home.".to_owned(), SUCCESS)
    } else {
        (
            "🦀 Critters washed up!",
            format!(
                "Press a number to take that critter: one card each, first click wins. Gone <t:{}:R>.",
                row.expires_at.timestamp()
            ),
            BRAND,
        )
    };
    let called = row
        .host_id
        .map_or(String::new(), |host| format!("\nCalled by {}.", mention(host)));
    let mut embed = EmbedBuilder::new()
        .color(color)
        .title(title)
        .description(format!("{intro}{called}\n\n{}", lines.join("\n")));
    if let Ok(source) = ImageSource::attachment("drop.png") {
        embed = embed.image(source);
    }
    embed.build()
}

fn critter_buttons(row: &DropRow, over: bool) -> Vec<Component> {
    let buttons = row
        .takers
        .iter()
        .enumerate()
        .map(|(slot, taker)| {
            let free = *taker == 0 && !over;
            button(
                format!("cd:g:{}:{slot}", row.id),
                &(slot + 1).to_string(),
                None,
                if free {
                    ButtonStyle::Primary
                } else {
                    ButtonStyle::Secondary
                },
                !free,
            )
        })
        .collect();
    vec![button_row(buttons)]
}

/// Posts a chat-driven critter drop in `channel`. False when Discord would not take it (the bot cannot
/// post there), which is no reason to try again at once.
pub async fn post_critter_drop(app: &Arc<App>, guild: u64, channel: u64) -> anyhow::Result<bool> {
    let row = create_critter_drop(app, guild, channel, None).await?;
    let (embed, png, buttons) = match critter_message(app, &row).await {
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
    let channel = row.channel_id as u64;
    let embeds = [embed];
    let attachments = [Attachment::from_bytes(filename.to_owned(), png, 1)];
    let no_pings = quiet();
    let posted = app
        .http
        .create_message(Id::new(channel.max(1)))
        .embeds(&embeds)
        .attachments(&attachments)
        .components(&buttons)
        .allowed_mentions(no_pings.as_ref())
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

/// `cd:g:<drop>:<slot>`: a click on a numbered card.
pub(crate) async fn on_grab(cx: &Cx, mut parts: Split<'_, char>) -> anyhow::Result<()> {
    let (Some("g"), Some(id), Some(slot)) = (
        parts.next(),
        parts.next().and_then(|v| v.parse::<i64>().ok()),
        parts.next().and_then(|v| v.parse::<usize>().ok()),
    ) else {
        return cx.reply_ephemeral("This button has expired.").await;
    };
    let (app, user) = (&cx.app, cx.author()?.id.get());
    let Some(row) = app.db.drop_row(id).await?.filter(|r| r.kind == DROP_CRITTERS) else {
        return cx.reply_ephemeral("That drop has already drifted away.").await;
    };
    let Some(entry) = row.cards.get(slot).and_then(|c| Collection::get().entry(c)) else {
        return cx.reply_ephemeral("That drop has already drifted away.").await;
    };
    let outcome = app
        .db
        .grab_card(id, slot, user, app.now(), award(entry), daily(app, Game::CritterDrop))
        .await?;
    let (granted, drop, paid) = match outcome {
        Grab::Gone => return cx.reply_ephemeral("That drop has already drifted away.").await,
        Grab::Have { slot } => {
            return cx
                .reply_ephemeral(format!(
                    "You already took card **{}** from this drop. Leave some for the others!",
                    slot + 1
                ))
                .await;
        }
        Grab::Lost { by } => {
            return cx
                .reply_ephemeral(format!("Too slow! {} grabbed **{}** first.", mention(by), entry.name))
                .await;
        }
        Grab::Taken { granted, drop, paid } => (granted, drop, paid),
    };
    record(app, Game::CritterDrop, user, true).await;
    // The card first, for everyone who is watching; the details are only for the one who grabbed it.
    cx.update(InteractionResponseData {
        embeds: Some(vec![critter_embed(&drop, false)]),
        components: Some(critter_buttons(&drop, false)),
        allowed_mentions: quiet(),
        ..Default::default()
    })
    .await?;
    let footer = match (paid, granted.new) {
        (false, _) => "Daily limit reached: critters still fill your dex, shells wait for tomorrow",
        (true, true) => "A new critter for your Tide Pool Dex",
        (true, false) => "A spare: recycled into shells and fragments",
    }
    .to_owned();
    let card = HaulCard {
        title: "Critter drop".to_owned(),
        subtitle: format!("Card {}", slot + 1),
        tiles: vec![haul_tile(app, entry, Some(&granted), HaulCard::SINGLE_ART_BOX)],
        footer,
        theme: Theme::reef(),
    };
    let png = app.render(move |r| r.haul_card(&card)).await?;
    let pay = money(&granted);
    let summary = format!(
        "You took **{}** ({}) · {}{}",
        entry.name,
        entry.rarity.label().to_lowercase(),
        if granted.new {
            "new for your dex!".to_owned()
        } else {
            "a spare, recycled".to_owned()
        },
        if pay.is_empty() {
            String::new()
        } else {
            format!(" · {pay}")
        }
    );
    let embed = EmbedBuilder::new()
        .color(if granted.new { SUCCESS } else { BRAND })
        .description(summary);
    cx.reply(Cx::card(embed, "critter.png", png, Vec::new(), true)).await
}

// /Bottle

/// Posts a chat-driven bottle in `channel`; false when Discord would not take it.
pub async fn post_bottle(app: &Arc<App>, guild: u64, channel: u64) -> anyhow::Result<bool> {
    let bottles = &Collection::get().bottles;
    anyhow::ensure!(!bottles.is_empty(), "no bottle messages");
    let message = &bottles[(rand::random::<f64>() * bottles.len() as f64) as usize % bottles.len()];
    let row = app
        .db
        .create_drop(NewDrop {
            kind: DROP_BOTTLE,
            guild_id: guild,
            channel_id: channel,
            host: None,
            cards: std::slice::from_ref(&message.id),
            expires_at: app.now() + Span::seconds(BOTTLE_SECS),
        })
        .await?;
    let card = BottleCard {
        title: "Message in a bottle".to_owned(),
        subtitle: "Washed ashore".to_owned(),
        seed: row.id as u32,
        theme: Theme::reef(),
    };
    let png = match app.render(move |r| r.bottle_card(&card)).await {
        Ok(png) => png,
        Err(error) => {
            app.db.abort_drop(row.id).await?;
            return Err(error);
        }
    };
    post(
        app,
        &row,
        bottle_embed(&row, None),
        "bottle.png",
        png,
        bottle_buttons(&row, false),
    )
    .await
}

/// What an opened bottle held, for the embed.
struct Opened {
    by: u64,
    text: String,
    found: String,
}

fn bottle_embed(row: &DropRow, opened: Option<&Opened>) -> Embed {
    let mut embed = EmbedBuilder::new();
    embed = match opened {
        Some(opened) => embed
            .color(SUCCESS)
            .title("🍾 The bottle was opened")
            .description(format!(
                "> {}\n\nOpened by {}. {}",
                opened.text,
                mention(opened.by),
                opened.found
            )),
        None if row.is_open() => embed
            .color(BRAND)
            .title("🍾 A bottle washed ashore!")
            .description(format!(
                "Somebody wrote a message and sent it out to sea. Be the first to open it! It drifts away <t:{}:R>.",
                row.expires_at.timestamp()
            )),
        None => embed
            .color(WARN)
            .title("🍾 The bottle drifted away")
            .description("Nobody opened it, and the tide took it back out to sea."),
    };
    if let Ok(source) = ImageSource::attachment("bottle.png") {
        embed = embed.image(source);
    }
    embed.build()
}

fn bottle_buttons(row: &DropRow, over: bool) -> Vec<Component> {
    vec![button_row(vec![button(
        format!("mb:o:{}", row.id),
        match (over, row.takers.first().is_some_and(|t| *t != 0)) {
            (false, _) => "Open the bottle",
            (true, true) => "Opened",
            (true, false) => "Drifted away",
        },
        Some("🍾"),
        if over {
            ButtonStyle::Secondary
        } else {
            ButtonStyle::Primary
        },
        over,
    )])]
}

/// `mb:o:<drop>`: somebody opens the bottle.
pub(crate) async fn on_open(cx: &Cx, mut parts: Split<'_, char>) -> anyhow::Result<()> {
    let (Some("o"), Some(id)) = (parts.next(), parts.next().and_then(|v| v.parse::<i64>().ok())) else {
        return cx.reply_ephemeral("This button has expired.").await;
    };
    let (app, user) = (&cx.app, cx.author()?.id.get());
    let Some(row) = app.db.drop_row(id).await?.filter(|r| r.kind == DROP_BOTTLE) else {
        return cx.reply_ephemeral("That bottle has drifted off.").await;
    };
    let lang = cx.lang();
    let text = row
        .cards
        .first()
        .and_then(|id| Collection::get().bottle_text(id, lang))
        .unwrap_or(match lang {
            Lang::En => "The note has faded.",
            Lang::Tr => "Not solmuş.",
        })
        .to_owned();
    let prize = collection::open_bottle(&mut dice());
    let held = match prize {
        Prize::Shells(n) => BottlePrize::Shells(n),
        Prize::Fragments(n) => BottlePrize::Fragments(n),
        Prize::Treasure(n) => BottlePrize::Treasure(n),
        Prize::Find(entry) => BottlePrize::Find(award(entry)),
    };
    let outcome = app
        .db
        .open_bottle(id, user, app.now(), held, daily(app, Game::Bottle))
        .await?;
    let (drop, shells, fragments, granted, paid) = match outcome {
        BottleOpen::Gone => return cx.reply_ephemeral("That bottle has drifted off.").await,
        // A second click of the one who opened it (both sent before the button went grey).
        BottleOpen::Lost { by } if by == user => {
            return cx.reply_ephemeral("You already opened this bottle.").await;
        }
        BottleOpen::Lost { by } => {
            return cx
                .reply_ephemeral(format!("Too slow! {} opened it first.", mention(by)))
                .await;
        }
        BottleOpen::Opened {
            drop,
            shells,
            fragments,
            granted,
            paid,
        } => (drop, shells, fragments, granted, paid),
    };
    record(app, Game::Bottle, user, true).await;
    let found = if !paid {
        "The daily limit is reached, so there was only the message today.".to_owned()
    } else {
        match (prize, &granted) {
            (Prize::Shells(_), _) => format!("Tucked inside: **{shells} shells**."),
            (Prize::Treasure(_), _) => format!("A little treasure fell out: **{shells} shells**!"),
            (Prize::Fragments(_), _) => format!("Tucked inside: **{fragments} fragments** of sea glass."),
            (Prize::Find(entry), Some(granted)) => {
                let pay = money(granted);
                format!(
                    "Tucked inside: **{}** ({}){}{}",
                    entry.name,
                    entry.rarity.label().to_lowercase(),
                    if granted.new {
                        ", new for their dex!"
                    } else {
                        ", a spare, recycled"
                    },
                    if pay.is_empty() {
                        String::new()
                    } else {
                        format!(" · {pay}")
                    }
                )
            }
            (Prize::Find(entry), None) => format!("Tucked inside: **{}**.", entry.name),
        }
    };
    let opened = Opened { by: user, text, found };
    cx.update(InteractionResponseData {
        embeds: Some(vec![bottle_embed(&drop, Some(&opened))]),
        components: Some(bottle_buttons(&drop, true)),
        allowed_mentions: quiet(),
        ..Default::default()
    })
    .await
}

// /Closing

/// Tells the channel a drop is over: retires its buttons and says what happened to the cards.
pub(crate) async fn close(app: &Arc<App>, row: &DropRow) {
    let Some(message) = row.message_id else { return };
    let (embed, buttons) = if row.kind == DROP_CRITTERS {
        (critter_embed(row, true), critter_buttons(row, true))
    } else {
        (bottle_embed(row, None), bottle_buttons(row, true))
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
