use super::*;

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
pub(super) struct Opened {
    by: u64,
    text: String,
    found: String,
}

pub(super) fn bottle_embed(row: &DropRow, opened: Option<&Opened>, lang: Lang) -> Embed {
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

pub(super) fn bottle_buttons(row: &DropRow, over: bool, lang: Lang) -> Vec<Component> {
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
    let Some(row) = app.db.drop_row(id).await?.filter(|r| {
        r.kind == DROP_BOTTLE && Some(r.guild_id as u64) == cx.guild_id() && r.channel_id as u64 == channel_of(cx)
    }) else {
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

