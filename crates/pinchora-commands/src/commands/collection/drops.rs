//! Critter Drop and Message in a Bottle: things that wash up in a channel for whoever clicks first.
//!
//! A critter drop shows three cards on one picture, a numbered button each: the first click takes a
//! card, nobody takes two. A bottle has one button and one opener. Both live in `collection_drops`, so
//! they survive restarts, and the ticker closes them when their time is up.

use std::{str::Split, sync::Arc, time::Duration};

use chrono::{Datelike, Duration as Span, Timelike};
use pinchora_core::{i18n::Lang, t};
use pinchora_db::{BottleOpen, BottlePrize, DROP_BOTTLE, DROP_CRITTERS, DropRow, Grab, NewDrop};
use pinchora_game::{
    Rarity,
    collection::{
        self, BOTTLE_SECS, Collection, DROP_SECS, Entry, MANUAL_CHANNEL_COOLDOWN_SECS, MANUAL_DROP_COOLDOWN_SECS, Prize,
    },
    lang as letters,
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
        let row = create_critter_drop(app, guild, channel, Some(user)).await?;
        // The drop is the channel's, so it speaks the server's language from its first post to its close.
        let (embed, png, buttons) = critter_message(app, app.guild_lang(guild).await, &row).await?;
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
    let token = cx.interaction.token.clone();
    // The drop stays keyed to the channel it was called in, which `/critterdrop` checks for a waiting one.
    let channel = channel_of(cx);
    cx.app.spawn(async move {
        // The answer to the command may still be on its way to Discord.
        tokio::time::sleep(Duration::from_millis(1_500)).await;
        let message = async { anyhow::Ok(app.interaction().response(&token).await?.model().await?) }.await;
        match message {
            Ok(message) => {
                if let Err(error) = app.db.set_drop_message(drop, channel, message.id.get()).await {
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
async fn critter_message(app: &App, lang: Lang, row: &DropRow) -> anyhow::Result<(Embed, Vec<u8>, Vec<Component>)> {
    let card = DropCard {
        title: Game::CritterDrop.name(lang),
        subtitle: t!(lang, "drop.card.subtitle"),
        cards: cards(row)
            .into_iter()
            .map(|e| DropCritter {
                art: picture(app, e, DropCard::ART_BOX),
                name: e.name_in(lang),
                rarity: e.rarity.label_in(lang).to_owned(),
                frame: e.rarity.color(),
                habitat: e.place_in(lang).to_owned(),
                sparkles: sparkles(e.rarity),
            })
            .collect(),
        footer: t!(lang, "drop.card.footer", secs = DROP_SECS),
        theme: Theme::reef(),
    };
    let png = app.render(move |r| r.drop_card(&card)).await?;
    Ok((critter_embed(row, false, lang), png, critter_buttons(row, false)))
}

/// The text of a drop: what each card is and who has it. `over`: its time is up.
fn critter_embed(row: &DropRow, over: bool, lang: Lang) -> Embed {
    let lines: Vec<String> = cards(row)
        .iter()
        .zip(&row.takers)
        .enumerate()
        .map(|(i, (entry, taker))| {
            let who = match (*taker, over) {
                (0, false) => t!(lang, "drop.free"),
                (0, true) => t!(lang, "drop.drifted"),
                (taker, _) => t!(lang, "drop.taken-by", user = mention(taker)),
            };
            t!(
                lang,
                "drop.line",
                number = i + 1,
                name = entry.name_in(lang),
                rarity = letters::lower(lang, entry.rarity.label_in(lang)),
                who = who
            )
        })
        .collect();
    let all_gone = row.takers.iter().all(|t| *t != 0);
    let (title, intro, color) = if over && all_gone {
        (t!(lang, "drop.title-over"), t!(lang, "drop.all-home-over"), SUCCESS)
    } else if over {
        (t!(lang, "drop.title-over"), t!(lang, "drop.tide-took"), WARN)
    } else if all_gone {
        (t!(lang, "drop.title-gone"), t!(lang, "drop.all-home"), SUCCESS)
    } else {
        (
            t!(lang, "drop.title-open"),
            t!(
                lang,
                "drop.intro",
                when = format!("<t:{}:R>", row.expires_at.timestamp())
            ),
            BRAND,
        )
    };
    let called = row.host_id.map_or(String::new(), |host| {
        format!("\n{}", t!(lang, "drop.called-by", user = mention(host)))
    });
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
    let lang = app.guild_lang(guild).await;
    let row = create_critter_drop(app, guild, channel, None).await?;
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
        return cx.reply_ephemeral(t!(cx.lang(), "collect.button-expired")).await;
    };
    let (app, user, lang) = (&cx.app, cx.author()?.id.get(), cx.lang());
    let Some(row) = app.db.drop_row(id).await?.filter(|r| r.kind == DROP_CRITTERS) else {
        return cx.reply_ephemeral(t!(lang, "drop.gone")).await;
    };
    let Some(entry) = row.cards.get(slot).and_then(|c| Collection::get().entry(c)) else {
        return cx.reply_ephemeral(t!(lang, "drop.gone")).await;
    };
    let outcome = app
        .db
        .grab_card(id, slot, user, app.now(), award(entry), daily(app, Game::CritterDrop))
        .await?;
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
    let card = HaulCard {
        title: Game::CritterDrop.name(lang),
        subtitle: t!(lang, "drop.card-number", number = slot + 1),
        tiles: vec![haul_tile(app, lang, entry, Some(&granted), HaulCard::SINGLE_ART_BOX)],
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

// /Bottle

/// Posts a chat-driven bottle in `channel`; false when Discord would not take it.
pub async fn post_bottle(app: &Arc<App>, guild: u64, channel: u64) -> anyhow::Result<bool> {
    let lang = app.guild_lang(guild).await;
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
        title: Game::Bottle.name(lang),
        subtitle: t!(lang, "bottle.subtitle"),
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
        bottle_embed(&row, None, lang),
        "bottle.png",
        png,
        bottle_buttons(&row, false, lang),
    )
    .await
}

/// What an opened bottle held, for the embed.
struct Opened {
    by: u64,
    text: String,
    found: String,
}

fn bottle_embed(row: &DropRow, opened: Option<&Opened>, lang: Lang) -> Embed {
    let mut embed = EmbedBuilder::new();
    embed = match opened {
        Some(opened) => embed
            .color(SUCCESS)
            .title(t!(lang, "bottle.title-opened"))
            .description(t!(
                lang,
                "bottle.opened",
                text = opened.text,
                user = mention(opened.by),
                found = opened.found
            )),
        None if row.is_open() => embed.color(BRAND).title(t!(lang, "bottle.title-open")).description(t!(
            lang,
            "bottle.open",
            when = format!("<t:{}:R>", row.expires_at.timestamp())
        )),
        None => embed
            .color(WARN)
            .title(t!(lang, "bottle.title-gone"))
            .description(t!(lang, "bottle.gone-text")),
    };
    if let Ok(source) = ImageSource::attachment("bottle.png") {
        embed = embed.image(source);
    }
    embed.build()
}

fn bottle_buttons(row: &DropRow, over: bool, lang: Lang) -> Vec<Component> {
    vec![button_row(vec![button(
        format!("mb:o:{}", row.id),
        &match (over, row.takers.first().is_some_and(|t| *t != 0)) {
            (false, _) => t!(lang, "bottle.button-open"),
            (true, true) => t!(lang, "bottle.button-opened"),
            (true, false) => t!(lang, "bottle.button-gone"),
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
        return cx.reply_ephemeral(t!(cx.lang(), "collect.button-expired")).await;
    };
    let (app, user, lang) = (&cx.app, cx.author()?.id.get(), cx.lang());
    let Some(row) = app.db.drop_row(id).await?.filter(|r| r.kind == DROP_BOTTLE) else {
        return cx.reply_ephemeral(t!(lang, "bottle.drifted")).await;
    };
    // The opened bottle replaces the channel's post: it is told in the server's language.
    let table = app.guild_lang(row.guild_id as u64).await;
    let text = row
        .cards
        .first()
        .and_then(|id| Collection::get().bottle_text(id, table))
        .map_or_else(|| t!(table, "bottle.faded"), str::to_owned);
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
        BottleOpen::Gone => return cx.reply_ephemeral(t!(lang, "bottle.drifted")).await,
        // A second click of the one who opened it (both sent before the button went grey).
        BottleOpen::Lost { by } if by == user => {
            return cx.reply_ephemeral(t!(lang, "bottle.already")).await;
        }
        BottleOpen::Lost { by } => {
            return cx
                .reply_ephemeral(t!(lang, "bottle.too-slow", user = mention(by)))
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
    let lang = table;
    let found = if !paid {
        t!(lang, "bottle.found-limit")
    } else {
        match (prize, &granted) {
            (Prize::Shells(_), _) => t!(lang, "bottle.found-shells", shells = shells),
            (Prize::Treasure(_), _) => t!(lang, "bottle.found-treasure", shells = shells),
            (Prize::Fragments(_), _) => t!(lang, "bottle.found-fragments", fragments = fragments),
            (Prize::Find(entry), Some(granted)) => {
                let pay = money(lang, granted);
                t!(
                    lang,
                    "bottle.found-find",
                    name = entry.name_in(lang),
                    rarity = letters::lower(lang, entry.rarity.label_in(lang)),
                    what = if granted.new {
                        t!(lang, "bottle.found-new")
                    } else {
                        t!(lang, "bottle.found-spare")
                    },
                    pay = if pay.is_empty() {
                        String::new()
                    } else {
                        format!(" · {pay}")
                    }
                )
            }
            (Prize::Find(entry), None) => t!(lang, "bottle.found-plain", name = entry.name_in(lang)),
        }
    };
    let opened = Opened { by: user, text, found };
    cx.update(InteractionResponseData {
        embeds: Some(vec![bottle_embed(&drop, Some(&opened), lang)]),
        components: Some(bottle_buttons(&drop, true, lang)),
        allowed_mentions: quiet(),
        ..Default::default()
    })
    .await
}

// /Closing

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
