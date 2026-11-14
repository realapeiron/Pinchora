use super::*;

/// Entry counts shown on the message change at most this often per giveaway.
const REFRESH_EVERY: Duration = Duration::from_secs(5);
/// Giveaways whose entry count changed inside that window, waiting for the ticker to show it.
const CHANGED: &str = "gw:changed";
fn refresh_key(id: i64) -> String {
    format!("gw-refresh:{id}")
}

/// Shows a giveaway's new entry count on its message, unless one was shown in the last five seconds;
/// then the ticker shows the latest count once that window is over.
pub(super) async fn show_count(app: &Arc<App>, id: i64) {
    match app.cache.try_cooldown(&refresh_key(id), REFRESH_EVERY).await {
        Ok(None) => refresh_message(app, id).await,
        Ok(Some(_)) => {
            if let Err(error) = app.cache.set_add(CHANGED, &id.to_string()).await {
                tracing::debug!(%error, giveaway = id, "entry count not queued");
            }
        }
        Err(error) => tracing::debug!(%error, giveaway = id, "entry count not shown"),
    }
}

/// Edits the giveaway message to the entry count the database has now.
pub(crate) async fn refresh_message(app: &App, id: i64) {
    let giveaway = match app.db.giveaway(id).await {
        Ok(Some(g)) if g.is_running() => g,
        Ok(_) => return,
        Err(error) => return tracing::debug!(%error, giveaway = id, "giveaway not loaded"),
    };
    if let Some(message) = giveaway.message_id {
        let lang = app.guild_lang(giveaway.guild_id as u64).await;
        let refreshed = edit_message(
            app,
            giveaway.channel_id as u64,
            message as u64,
            None,
            Some(&[giveaway_embed(lang, &giveaway)]),
            Some(&giveaway_components(lang, &giveaway)),
        )
        .await;
        if refreshed {
            let _ = sqlx::query("UPDATE giveaways SET start_announced=TRUE WHERE id=$1 AND status='running'")
                .bind(id)
                .execute(app.db.pool())
                .await;
        }
    }
}

/// The ticker's part in entry counts: shows the ones that changed inside a window, as soon as their
/// window is over.
pub(crate) async fn refresh_changed(app: &Arc<App>) {
    let Ok(changed) = app.cache.set_members(CHANGED).await else {
        return;
    };
    for raw in changed {
        let Ok(id) = raw.parse::<i64>() else {
            let _ = app.cache.set_remove(CHANGED, &raw).await;
            continue;
        };
        match app.cache.try_cooldown(&refresh_key(id), REFRESH_EVERY).await {
            Ok(None) => {
                let _ = app.cache.set_remove(CHANGED, &raw).await;
                refresh_message(app, id).await;
            }
            // Still inside the window: the next tick.
            Ok(Some(_)) => {}
            Err(error) => tracing::debug!(%error, giveaway = id, "entry count not shown"),
        }
    }
}

/// Ends giveaway `id` (when it is due, or at once with `early`) and tells the channel. `None` when it
/// was not running, because somebody else ended it first.
pub(crate) async fn end(app: &Arc<App>, id: i64, early: bool) -> anyhow::Result<Option<EndedGiveaway>> {
    let Some(ended) = app
        .db
        .end_giveaway_weighted(id, app.now(), early, draw_weighted)
        .await?
    else {
        return Ok(None);
    };
    metrics::counter!("pinchora_giveaways_ended_total").increment(1);
    if let Err(error) = announce(app, &ended.giveaway).await {
        // The winners are paid and saved; the ticker tries the announcement again.
        tracing::warn!(%error, giveaway = id, "giveaway ended but not announced");
    }
    Ok(Some(ended))
}

/// Shows the result on the giveaway's message and posts it in the channel, once, whichever pod gets
/// there first. A refusal that will never change (the channel is gone) is given up on.
async fn announce(app: &Arc<App>, g: &GiveawayRow) -> anyhow::Result<()> {
    if !app.db.claim_giveaway_announcement(g.id, app.now()).await? {
        return Ok(());
    }
    let channel = Id::new((g.channel_id as u64).max(1));
    let lang = app.guild_lang(g.guild_id as u64).await;
    if let Some(message) = g.message_id {
        edit_message(
            app,
            g.channel_id as u64,
            message as u64,
            None,
            Some(&[giveaway_embed(lang, g)]),
            Some(&giveaway_components(lang, g)),
        )
        .await;
    }
    let winners = mentions(&g.winner_ids);
    let unpaid = g.shells * (i64::from(g.winners) - g.winner_ids.len() as i64).max(0);
    let mut text = if g.winner_ids.is_empty() {
        t!(lang, "giveaway.nobody", prize = g.prize)
    } else {
        t!(lang, "giveaway.congrats", winners = winners, prize = g.prize)
    };
    if g.shells > 0 && !g.winner_ids.is_empty() {
        text.push_str(&t!(lang, "giveaway.paid", amount = thousands_in(lang, g.shells)));
    }
    if unpaid > 0 {
        text.push_str(&t!(
            lang,
            "giveaway.unclaimed",
            amount = thousands_in(lang, unpaid),
            host = format!("<@{}>", g.host_id)
        ));
    }
    let no_pings = quiet();
    let mut post = app
        .http
        .create_message(channel)
        .content(&text)
        .allowed_mentions(no_pings.as_ref());
    if let Some(message) = g.message_id {
        post = post.reply(Id::new((message as u64).max(1))).fail_if_not_exists(false);
    }
    match post.await {
        Ok(_) => {}
        Err(error) if is_permanent(&error) => {
            tracing::warn!(giveaway = g.id, %error, "giveaway result could not be posted; giving up");
        }
        Err(error) => return Err(error.into()),
    }
    app.db.mark_giveaway_announced(g.id).await
}

/// Tells the channels of giveaways that ended without being announced (the pod that ended them
/// died, or Discord was unreachable), once their claim has lapsed.
pub(crate) async fn announce_pending(app: &Arc<App>) {
    let pending = match app.db.unannounced_giveaways(20).await {
        Ok(pending) => pending,
        Err(error) => return tracing::warn!(%error, "unannounced giveaways not loaded"),
    };
    for giveaway in pending {
        if let Err(error) = announce(app, &giveaway).await {
            tracing::warn!(%error, giveaway = giveaway.id, "giveaway result not announced");
        }
    }
}

