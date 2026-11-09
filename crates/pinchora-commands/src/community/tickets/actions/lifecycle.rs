use super::*;

/// The transcript as a file only the member sees: the kept one of a closed ticket, else read now.
pub(super) async fn send_transcript(cx: &Cx, ticket: &Ticket) -> anyhow::Result<()> {
    let lang = cx.lang();
    cx.responder.defer(true).await?;
    let server = server_name(&cx.app, ticket.guild_id as u64).await;
    let html = match cx.app.db.transcript(ticket.id).await? {
        Some(stored) if !ticket.is_open() => {
            let names = names(&cx.app, ticket, None).await;
            let page = Page {
                server: &server,
                opener: names.0.as_deref(),
                closed_by: names.1.as_deref(),
            };
            transcript::stored_html(ticket, &page, &stored, lang)
        }
        _ => {
            let captured = match transcript::capture(&cx.app, ticket).await {
                Ok(captured) => captured,
                Err(error) => {
                    tracing::debug!(ticket = ticket.id, %error, "ticket history not read");
                    return tell(cx, t!(lang, "tickets.discord.history")).await;
                }
            };
            let names = names(&cx.app, ticket, Some(&captured)).await;
            let page = Page {
                server: &server,
                opener: names.0.as_deref(),
                closed_by: names.1.as_deref(),
            };
            transcript::page_html(
                ticket,
                &page,
                &captured.lines,
                &captured.authors(),
                &captured.mention_names(),
                captured.with_content,
                lang,
            )
        }
    };
    cx.reply(InteractionResponseData {
        content: Some(t!(lang, "tickets.transcript-here", number = number(ticket.number))),
        attachments: Some(vec![Attachment::from_bytes(transcript::file_name(ticket), html, 1)]),
        flags: Some(twilight_model::channel::message::MessageFlags::EPHEMERAL),
        allowed_mentions: quiet(),
        ..Default::default()
    })
    .await
}

/// The opener's and the closer's names for a transcript: from the messages when they wrote, else from Discord.
async fn names(app: &App, ticket: &Ticket, captured: Option<&Captured>) -> (Option<String>, Option<String>) {
    let lookup = |id: Option<i64>| async move {
        let id = id? as u64;
        if let Some((_, name, _)) = captured.and_then(|c| c.authors.iter().find(|(user, _, _)| *user == id)) {
            return Some(name.clone());
        }
        app.user(id)
            .await
            .map(|u| u.global_name.clone().unwrap_or_else(|| u.name.clone()))
    };
    (lookup(Some(ticket.opener_id)).await, lookup(ticket.closed_by).await)
}

/// Closes `ticket` once, whoever asked (a member, staff, the ticker): keeps the transcript, tells the
/// ticket, takes the opener's access away (or archives the thread), moves the channel to the closed
/// category, logs it with the transcript and sends the opener theirs with the rating buttons. `None` when
/// somebody else closed it first.
pub(crate) async fn close(
    app: &Arc<App>,
    ticket: Ticket,
    panel: Option<TicketPanel>,
    by: Option<(u64, String)>,
    reason: Option<String>,
    idle_before: Option<chrono::DateTime<chrono::Utc>>,
) -> anyhow::Result<Option<Ticket>> {
    let Some(closed) = app
        .db
        .close_ticket(
            ticket.id,
            by.as_ref().map(|(id, _)| *id),
            reason.as_deref(),
            app.now(),
            idle_before,
        )
        .await?
    else {
        return Ok(None);
    };
    finish_close(app, closed, panel, by, reason).await
}

async fn finish_close(
    app: &Arc<App>,
    closed: Ticket,
    panel: Option<TicketPanel>,
    by: Option<(u64, String)>,
    reason: Option<String>,
) -> anyhow::Result<Option<Ticket>> {
    let guild = closed.guild_id as u64;
    crate::admin::workflows::triggers::simple(app, guild, closed.opener_id as u64, "ticket_closed", closed.place());
    let lang = guild_lang(app, guild).await;
    let settings = app.db.ticket_settings(guild).await?;
    let captured = capture_closed_transcript(app, &closed).await;
    let who = by
        .as_ref()
        .map_or_else(|| t!(lang, "tickets.notice.nobody"), |(id, _)| mention(*id));
    let mut text = t!(lang, "tickets.notice.closed", user = who);
    if let Some(reason) = &reason {
        text.push('\n');
        text.push_str(&t!(lang, "tickets.notice.reason", reason = scrub_value(reason)));
    }
    notice(
        app,
        closed.guild_id as u64,
        place_of(&closed),
        DANGER,
        text.clone(),
        closed_controls(closed.id, lang),
    )
    .await;
    shut(app, &closed, panel.as_ref(), &settings).await;

    let server = server_name(app, guild).await;
    let names = names(app, &closed, Some(&captured)).await;
    let page = Page {
        server: &server,
        opener: names.0.as_deref(),
        closed_by: names.1.as_deref(),
    };
    let html = transcript::page_html(
        &closed,
        &page,
        &captured.lines,
        &captured.authors(),
        &captured.mention_names(),
        captured.with_content,
        lang,
    );
    let file = transcript::file_name(&closed);
    let mut summary = t!(
        lang,
        "tickets.log.closed",
        opener = mention(closed.opener_id as u64),
        user = who,
        count = captured.lines.len()
    );
    if let Some(reason) = &reason {
        summary.push('\n');
        summary.push_str(&t!(lang, "tickets.notice.reason", reason = scrub_value(reason)));
    }
    log(
        app,
        closed.guild_id as u64,
        log_channel(&settings, panel.as_ref()),
        log_embed(
            DANGER,
            t!(lang, "tickets.log.closed-title", number = number(closed.number)),
            summary,
        ),
        Some((file.clone(), html.clone())),
    )
    .await;
    tell_opener(app, &closed, &settings, &server, (file, html)).await;
    let board = app.clone();
    tokio::spawn(async move {
        let _ = crate::community::tickets::refresh_status_board(&board, guild).await;
    });
    Ok(Some(closed))
}

/// Takes the closed ticket away from its members: their overwrites go and the channel moves to the closed
/// category; a thread loses its opener and is archived and locked.
async fn shut(app: &App, ticket: &Ticket, _panel: Option<&TicketPanel>, settings: &TicketSettings) {
    let Some(place) = place_of(ticket) else { return };
    let bot = app.application_id.get();
    if ticket.thread_id.is_some() {
        let mut members = vec![ticket.opener_id as u64];
        let mut after: Option<Id<UserMarker>> = None;
        loop {
            let request = app.http.thread_members(place).limit(100);
            let response = match after {
                Some(after) => request.after(after).await,
                None => request.await,
            };
            let list = match response {
                Ok(response) => match response.models().await {
                    Ok(list) => list,
                    Err(error) => {
                        tracing::debug!(ticket = ticket.id, %error, "thread members not read");
                        break;
                    }
                },
                Err(error) => {
                    tracing::debug!(ticket = ticket.id, %error, "thread members not read");
                    break;
                }
            };
            let full = list.len() == 100;
            let last = list.last().and_then(|member| member.user_id);
            for member in list {
                if let Some(id) = member.user_id.map(Id::get)
                    && id != bot
                    && !members.contains(&id)
                {
                    members.push(id);
                }
            }
            if !full || last.is_none() || last == after {
                break;
            }
            after = last;
        }
        for member in members {
            if let Err(error) = app
                .http
                .remove_thread_member(place, Id::<UserMarker>::new(member))
                .await
            {
                tracing::debug!(ticket = ticket.id, member, %error, "ticket member not taken out of the thread");
            }
        }
        if let Err(error) = app.http.update_thread(place).archived(true).locked(true).await {
            tracing::debug!(ticket = ticket.id, %error, "ticket thread not archived");
        }
        return;
    }
    let mut members = vec![ticket.opener_id as u64];
    if let Ok(response) = app.http.channel(place).await
        && let Ok(channel) = response.model().await
    {
        for overwrite in channel.permission_overwrites.unwrap_or_default() {
            let id = overwrite.id.get();
            if overwrite.kind == twilight_model::channel::permission_overwrite::PermissionOverwriteType::Member
                && id != bot
                && !members.contains(&id)
            {
                members.push(id);
            }
        }
    }
    for member in members {
        if let Err(error) = app.http.delete_channel_permission(place).member(Id::new(member)).await {
            tracing::debug!(ticket = ticket.id, member, %error, "ticket access not taken back");
        }
    }
    if let Some(category) = settings.closed_category_id
        && let Err(error) = app
            .http
            .update_channel(place)
            .parent_id(Some(Id::new(category as u64)))
            .await
    {
        tracing::debug!(ticket = ticket.id, %error, "closed ticket not moved");
    }
}

/// The opener's direct message after a close: the transcript when the server sends it, the stars when it
/// asks for a rating. Members who switched off direct messages from servers get nothing.
async fn tell_opener(app: &App, ticket: &Ticket, settings: &TicketSettings, server: &str, file: (String, Vec<u8>)) {
    if !settings.dm_transcript && !settings.rating_enabled {
        return;
    }
    let opener = ticket.opener_id as u64;
    let Some(channel) = crate::progression::leveling::private_channel(app, opener).await else {
        return;
    };
    let lang = match app.user_lang_choice(opener).await {
        Some(lang) => lang,
        None => app.guild_lang(ticket.guild_id as u64).await,
    };
    let mut text = t!(
        lang,
        "tickets.dm.closed",
        number = number(ticket.number),
        server = scrub_value(server)
    );
    if settings.rating_enabled {
        text.push('\n');
        text.push_str(&t!(lang, "tickets.dm.rate"));
    }
    let components: Vec<Component> = if settings.rating_enabled {
        vec![rating_row(ticket.id)]
    } else {
        Vec::new()
    };
    let sent = app
        .http
        .create_message(channel)
        .content(&text)
        .components(&components)
        .allowed_mentions(quiet().as_ref())
        .await;
    if let Err(error) = sent {
        tracing::debug!(ticket = ticket.id, %error, "ticket direct message not delivered");
    }
    if settings.dm_transcript {
        let files = [Attachment::from_bytes(file.0, file.1, 1)];
        if let Err(error) = app.http.create_message(channel).attachments(&files).await {
            tracing::debug!(ticket = ticket.id, %error, "ticket transcript direct message not delivered");
        }
    }
}

/// Opens a closed ticket again: the opener sees it again (the thread is unarchived), it moves back to the
/// open category, and the ticket says so with its controls.
pub(super) async fn reopen(cx: &Cx, ticket: &Ticket, panel: Option<&TicketPanel>, force: bool) -> anyhow::Result<bool> {
    let app = &cx.app;
    let reopened = if force {
        app.db
            .force_reopen_ticket(ticket.id, app.now(), cx.author()?.id.get())
            .await?
    } else {
        app.db.reopen_ticket(ticket.id, app.now()).await?
    };
    let Some(opened) = reopened else {
        refuse(cx, Refusal::NotClosed).await?;
        return Ok(false);
    };
    let guild = ticket.guild_id as u64;
    crate::admin::workflows::triggers::simple(app, guild, opened.opener_id as u64, "ticket_opened", opened.place());
    let settings = app.db.ticket_settings(guild).await?;
    if let Some(place) = place_of(&opened) {
        let opener = Id::<UserMarker>::new(opened.opener_id as u64);
        if opened.thread_id.is_some() {
            if let Err(error) = app.http.update_thread(place).archived(false).locked(false).await {
                tracing::debug!(ticket = opened.id, %error, "ticket thread not unarchived");
            }
            if let Err(error) = app.http.add_thread_member(place, opener).await {
                tracing::debug!(ticket = opened.id, %error, "opener not added back");
            }
        } else {
            if let Err(error) = app
                .http
                .update_channel_permission(place, &member_overwrite(opener.get()))
                .await
            {
                tracing::debug!(ticket = opened.id, %error, "opener's access not given back");
            }
            let open_category = panel.and_then(|p| p.open_category_id).or(settings.open_category_id);
            if settings.closed_category_id.is_some()
                && let Some(category) = open_category
                && let Err(error) = app
                    .http
                    .update_channel(place)
                    .parent_id(Some(Id::new(category as u64)))
                    .await
            {
                tracing::debug!(ticket = opened.id, %error, "reopened ticket not moved back");
            }
        }
    }
    let lang = guild_lang(app, guild).await;
    let (user, _) = actor(cx)?;
    notice(
        app,
        guild,
        place_of(&opened),
        SUCCESS,
        t!(
            lang,
            "tickets.notice.reopened",
            user = mention(user),
            opener = mention(opened.opener_id as u64)
        ),
        controls(opened.id, false, lang),
    )
    .await;
    log(
        app,
        guild,
        log_channel(&settings, panel),
        log_embed(
            SUCCESS,
            t!(lang, "tickets.log.reopened-title", number = number(opened.number)),
            t!(lang, "tickets.log.reopened", user = mention(user)),
        ),
        None,
    )
    .await;
    Ok(true)
}

/// Deletes a closed ticket's channel or thread for good; the ticket and its transcript stay.
pub(super) async fn delete(cx: &Cx, ticket: &Ticket) -> anyhow::Result<()> {
    let lang = cx.lang();
    let Some(place) = place_of(ticket) else {
        return tell(cx, t!(lang, "tickets.gone")).await;
    };
    cx.update(InteractionResponseData {
        content: Some(t!(lang, "tickets.deleting")),
        components: Some(Vec::new()),
        allowed_mentions: quiet(),
        ..Default::default()
    })
    .await?;
    if let Err(error) = cx.app.http.delete_channel(place).await
        && crate::roles::roles::error_code(&error) != Some(UNKNOWN_CHANNEL)
    {
        tell(cx, discord_refusal(&error, lang, Need::Manage(place.get()))).await?;
    }
    Ok(())
}

async fn tick_operations(app: &Arc<App>, now: chrono::DateTime<chrono::Utc>) -> anyhow::Result<usize> {
    app.db.purge_expired_ticket_form_drafts().await?;
    let mut closed = 0;
    for (id, guild, place) in app.db.due_ticket_reminders(now, 25).await? {
        if !module_on(app, guild as u64, Some(place as u64)).await || !app.db.claim_ticket_reminder(id).await? {
            continue;
        }
        let lang = guild_lang(app, guild as u64).await;
        notice(
            app,
            guild as u64,
            Some(Id::new(place as u64)),
            WARN,
            t!(lang, "tickets.v2.idle-reminder"),
            Vec::new(),
        )
        .await;
    }
    for id in app.db.due_ticket_close_requests(now, 25).await? {
        let Some(ticket) = app.db.ticket(id).await? else {
            continue;
        };
        let guild = ticket.guild_id as u64;
        if !module_on(app, guild, ticket.place()).await {
            continue;
        }
        let panel = match ticket.panel_id {
            Some(id) => app.db.ticket_panel(id).await?,
            None => None,
        };
        let lang = guild_lang(app, guild).await;
        let reason = t!(lang, "tickets.v2.close-timeout");
        if let Some(closed_ticket) = app.db.close_requested_ticket(ticket.id, &reason, now).await? {
            finish_close(app, closed_ticket, panel, None, Some(reason)).await?;
            closed += 1;
        }
    }
    for (guild, _, _) in app.db.ticket_status_boards().await? {
        if let Err(error) = crate::community::tickets::refresh_status_board(app, guild as u64).await {
            tracing::warn!(guild, %error, "ticket status board not refreshed");
        }
    }
    Ok(closed)
}

/// One step of the ticker: closes the open tickets nobody wrote in for longer than their server allows.
/// Discord is asked for each one's last message first, since the bot does not count messages itself:
/// a newer one makes it busy again. A ticket whose channel is gone closes too. Returns how many closed.
pub async fn tick(app: &Arc<App>) -> anyhow::Result<usize> {
    let now = app.now();
    let mut closed = tick_operations(app, now).await?;
    for idle in app.db.idle_tickets(now, 25).await? {
        let (guild, place) = (idle.guild_id as u64, idle.place as u64);
        // A ticket skipped here goes to the back of the queue, so 25 that can never close (a switched-off
        // module, a server the bot left) cannot hold up every other server's.
        if !module_on(app, guild, Some(place)).await {
            app.db.touch_ticket(idle.id, now).await?;
            continue;
        }
        let cutoff = idle_cutoff(now, idle.auto_close_hours);
        match app.http.channel(Id::new(place)).await {
            Ok(response) => {
                let channel = response.model().await?;
                if let Some(last) = channel.last_message_id {
                    let at = snowflake::timestamp(last.get());
                    if at > idle.last_activity_at {
                        app.db.touch_ticket(idle.id, at).await?;
                        if at >= cutoff {
                            continue;
                        }
                    }
                }
            }
            Err(error) if crate::roles::roles::error_code(&error) == Some(UNKNOWN_CHANNEL) => {}
            Err(error) => {
                tracing::debug!(ticket = idle.id, %error, "idle ticket's channel not read");
                app.db.touch_ticket(idle.id, now).await?;
                continue;
            }
        }
        let Some(ticket) = app.db.ticket(idle.id).await? else {
            continue;
        };
        let panel = match ticket.panel_id {
            Some(id) => app.db.ticket_panel(id).await?,
            None => None,
        };
        let lang = guild_lang(app, guild).await;
        let reason = t!(lang, "tickets.auto-reason", count = idle.auto_close_hours);
        match close(app, ticket, panel, None, Some(reason), Some(cutoff)).await {
            Ok(Some(_)) => closed += 1,
            Ok(None) => {}
            Err(error) => tracing::warn!(ticket = idle.id, %error, "idle ticket not closed"),
        }
    }
    Ok(closed)
}

/// Runs [`tick`] for the life of the process.
pub fn spawn_ticker(app: Arc<App>) {
    tokio::spawn(async move {
        let mut wake = tokio::time::interval(WAKE);
        wake.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            wake.tick().await;
            if let Err(error) = Box::pin(tick(&app)).await {
                tracing::warn!(%error, "tickets tick failed");
            }
        }
    });
}

async fn capture_closed_transcript(app: &App, closed: &Ticket) -> Captured {
    let captured = match transcript::capture(app, closed).await {
        Ok(captured) => captured,
        Err(error) => {
            tracing::warn!(ticket = closed.id, %error, "ticket history not read, the transcript is empty");
            Captured {
                lines: Vec::new(),
                authors: Vec::new(),
                with_content: app.message_content,
                first_reply: None,
            }
        }
    };
    if let Err(error) = app
        .db
        .save_transcript(
            closed.id,
            &captured.lines,
            &captured.authors,
            captured.with_content,
            captured.first_reply,
        )
        .await
    {
        tracing::warn!(ticket = closed.id, %error, "ticket transcript not saved");
    }
    captured
}
