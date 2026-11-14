use super::*;

/// The giveaway numbered `id` in this server, or a reply saying there is none (and `None`).
async fn find(cx: &Cx, guild: u64, id: i64) -> anyhow::Result<Option<GiveawayRow>> {
    let found = cx.app.db.giveaway(id).await?.filter(|g| g.guild_id as u64 == guild);
    if found.is_none() {
        tell(cx, t!(cx.lang(), "giveaway.none", id = id)).await?;
    }
    Ok(found)
}

pub(super) async fn end_now(cx: &Cx, guild: u64, id: i64) -> anyhow::Result<()> {
    let Some(giveaway) = find(cx, guild, id).await? else {
        return Ok(());
    };
    let Some(ended) = end(&cx.app, giveaway.id, true).await? else {
        return tell(cx, t!(cx.lang(), "giveaway.already-ended", id = id)).await;
    };
    let text = if ended.winners.is_empty() {
        t!(cx.lang(), "giveaway.ended-none", id = id)
    } else {
        t!(
            cx.lang(),
            "giveaway.ended-with",
            id = id,
            winners = mentions(&ended.giveaway.winner_ids)
        )
    };
    tell(cx, text).await
}

pub(super) async fn reroll(cx: &Cx, guild: u64, c: GiveawayReroll) -> anyhow::Result<()> {
    let count = c.winners.unwrap_or(1) as usize;
    let outcome = cx
        .app
        .db
        .reroll_giveaway_weighted(guild, c.id, count, draw_weighted)
        .await?;
    let (giveaway, picked) = match outcome {
        Reroll::Done(giveaway, picked) => (*giveaway, picked),
        Reroll::Missing => return tell(cx, t!(cx.lang(), "giveaway.none", id = c.id)).await,
        Reroll::StillRunning => {
            return tell(cx, t!(cx.lang(), "giveaway.still-running", id = c.id)).await;
        }
        Reroll::NoOneLeft => {
            return tell(cx, t!(cx.lang(), "giveaway.no-one-left", id = c.id)).await;
        }
    };
    let winners = mentions(&picked.iter().map(|w| *w as i64).collect::<Vec<_>>());
    // A reroll only picks people: the shells of a shells prize were paid when it ended.
    // The announcement answers nobody in particular: it speaks the server's language.
    let server_lang = cx.app.guild_lang(guild).await;
    let text = t!(
        server_lang,
        "giveaway.reroll-announce",
        count = picked.len(),
        prize = giveaway.prize,
        winners = winners,
        note = if giveaway.shells > 0 {
            t!(server_lang, "giveaway.reroll-no-shells")
        } else {
            String::new()
        }
    );
    let channel = Id::new((giveaway.channel_id as u64).max(1));
    let no_pings = quiet();
    let mut post = cx
        .app
        .http
        .create_message(channel)
        .content(&text)
        .allowed_mentions(no_pings.as_ref());
    if let Some(message) = giveaway.message_id {
        post = post.reply(Id::new((message as u64).max(1))).fail_if_not_exists(false);
    }
    let told = match post.await {
        Ok(_) => t!(cx.lang(), "giveaway.announced", channel = format!("<#{channel}>")),
        Err(error) => {
            tracing::warn!(guild, giveaway = giveaway.id, %error, "could not announce a reroll");
            t!(
                cx.lang(),
                "giveaway.announce-failed",
                channel = format!("<#{channel}>"),
                reason = post_refusal(cx.lang(), channel, &error)
            )
        }
    };
    tell(
        cx,
        t!(
            cx.lang(),
            "giveaway.rerolled",
            id = giveaway.id,
            winners = winners,
            told = told
        ),
    )
    .await
}

pub(super) async fn list(cx: &Cx, guild: u64) -> anyhow::Result<()> {
    let running = cx.app.db.running_giveaways(guild).await?;
    if running.is_empty() {
        return tell(cx, t!(cx.lang(), "giveaway.list-empty")).await;
    }
    let lang = cx.lang();
    let lines: Vec<String> = running
        .iter()
        .map(|g| {
            let prize: String = g.prize.chars().take(40).collect();
            let link = g.message_id.map_or_else(String::new, |m| {
                t!(
                    lang,
                    "giveaway.jump",
                    link = message_link(guild, g.channel_id as u64, m as u64)
                )
            });
            t!(
                lang,
                "giveaway.list-row",
                id = g.id,
                prize = prize,
                entries = t!(lang, "giveaway.entries-count", count = g.entries),
                ends = relative(g.ends_at),
                link = link
            )
        })
        .collect();
    let embed = EmbedBuilder::new()
        .color(BRAND)
        .title(t!(lang, "giveaway.list-title"))
        .description(lines.join("\n"))
        .build();
    cx.reply(InteractionResponseData {
        embeds: Some(vec![embed]),
        flags: Some(MessageFlags::EPHEMERAL),
        allowed_mentions: quiet(),
        ..Default::default()
    })
    .await
}

