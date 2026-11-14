use super::*;

/// The ticket row and its place, or the sentence that says why there is none.
pub(super) async fn create(
    cx: &Cx,
    guild: u64,
    panel: &TicketPanel,
    settings: &TicketSettings,
    topic: &Topic,
    answers: &[Answer],
) -> anyhow::Result<Result<Ticket, String>> {
    let app = &cx.app;
    let lang = cx.lang();
    let opener = cx.author()?.id.get();
    let thread = settings.mode().threads(channel_count(cx, guild, settings).await);
    let need = if thread {
        Need::Thread(panel.channel_id as u64)
    } else {
        Need::Channel
    };
    if let Some(lacking) = missing(cx, need) {
        return Ok(Err(lacking));
    }
    let new = NewTicket {
        guild_id: guild,
        panel_id: Some(panel.id),
        opener,
        topic: &topic.label,
        answers,
        max_open: settings.max_open_per_user,
        at: app.now(),
    };
    let mut opened = app.db.open_ticket(&new).await?;
    if matches!(opened, TicketOpen::TooMany { .. }) {
        let mut recovered = false;
        for stale in app.db.open_tickets_for_user(guild, opener).await? {
            let Some(place) = stale.place() else { continue };
            if let Err(error) = app.http.channel(Id::new(place)).await
                && crate::roles::roles::error_code(&error) == Some(crate::community::tickets::UNKNOWN_CHANNEL)
            {
                crate::community::tickets::on_place_deleted(app, guild, place).await?;
                recovered = true;
            }
        }
        if recovered {
            opened = app.db.open_ticket(&new).await?;
        }
    }
    let ticket = match opened {
        TicketOpen::Opened(ticket) => *ticket,
        TicketOpen::Cooldown { seconds } => return Ok(Err(t!(lang, "tickets.wait", secs = seconds))),
        TicketOpen::TooMany { open } => {
            let ops = app.db.ticket_panel_operations(panel.id).await?;
            return Ok(Err(if ops.overflow_message.is_empty() {
                t!(lang, "tickets.too-many", count = open)
            } else {
                ops.overflow_message
            }));
        }
    };
    let user_name = cx.author()?.name.clone();
    let mut name = channel_name(&settings.naming, ticket.number, &user_name, &topic.label);
    if name.is_empty() {
        name = channel_name(DEFAULT_NAMING, ticket.number, "", "");
    }
    let made = if thread {
        make_thread(cx, panel, &name).await
    } else {
        make_channel(cx, guild, panel, settings, &name, &ticket).await
    };
    match made {
        Ok((channel, thread)) => {
            app.db.set_ticket_place(ticket.id, channel, thread).await?;
            Ok(Ok(Ticket {
                channel_id: Some(channel as i64),
                thread_id: thread.map(|t| t as i64),
                ..ticket
            }))
        }
        Err(refusal) => {
            app.db.abort_ticket(ticket.id).await?;
            Ok(Err(refusal))
        }
    }
}

/// How many channels the server has, when `auto` mode has to know; 0 when it does not or Discord will
/// not say (channels it is, then).
async fn channel_count(cx: &Cx, guild: u64, settings: &TicketSettings) -> usize {
    if settings.mode() != pinchora_core::community::tickets::Mode::Auto {
        return 0;
    }
    match cx.app.http.guild_channels(Id::new(guild)).await {
        Ok(response) => response.models().await.map(|c| c.len()).unwrap_or(0),
        Err(error) => {
            tracing::debug!(guild, %error, "channel count not read");
            0
        }
    }
}

/// Who sees a ticket channel: not everyone, the opener, the panel's support roles and the bot.
pub(crate) fn overwrites(guild: u64, opener: u64, bot: u64, support: &[i64]) -> Vec<PermissionOverwrite> {
    let mut list = vec![
        PermissionOverwrite {
            allow: Permissions::empty(),
            deny: Permissions::VIEW_CHANNEL,
            id: Id::new(guild),
            kind: PermissionOverwriteType::Role,
        },
        PermissionOverwrite {
            allow: member_allow(),
            deny: Permissions::empty(),
            id: Id::new(opener),
            kind: PermissionOverwriteType::Member,
        },
        PermissionOverwrite {
            allow: member_allow(),
            deny: Permissions::empty(),
            id: Id::new(bot),
            kind: PermissionOverwriteType::Member,
        },
    ];
    for role in support {
        list.push(PermissionOverwrite {
            allow: member_allow(),
            deny: Permissions::empty(),
            id: Id::new(*role as u64),
            kind: PermissionOverwriteType::Role,
        });
    }
    list
}

async fn make_channel(
    cx: &Cx,
    guild: u64,
    panel: &TicketPanel,
    settings: &TicketSettings,
    name: &str,
    ticket: &Ticket,
) -> Result<(u64, Option<u64>), String> {
    let lang = cx.lang();
    let overwrites = overwrites(
        guild,
        ticket.opener_id as u64,
        cx.app.application_id.get(),
        &panel.support_roles,
    );
    let topic = t!(
        lang,
        "tickets.channel-topic",
        number = number(ticket.number),
        topic = &ticket.topic
    );
    let mut request = cx
        .app
        .http
        .create_guild_channel(Id::new(guild), name)
        .kind(ChannelType::GuildText)
        .permission_overwrites(&overwrites)
        .topic(&topic);
    if let Some(category) = panel.open_category_id.or(settings.open_category_id) {
        request = request.parent_id(Id::new(category as u64));
    }
    match request.await {
        Ok(response) => match response.model().await {
            Ok(channel) => Ok((channel.id.get(), None)),
            Err(error) => Err({
                tracing::warn!(guild, %error, "ticket channel answer unreadable");
                t!(lang, "tickets.discord.refused")
            }),
        },
        Err(error) => Err(discord_refusal(&error, lang, Need::Channel)),
    }
}

async fn make_thread(cx: &Cx, panel: &TicketPanel, name: &str) -> Result<(u64, Option<u64>), String> {
    let lang = cx.lang();
    let parent: Id<ChannelMarker> = Id::new(panel.channel_id as u64);
    let made = cx
        .app
        .http
        .create_thread(parent, name, ChannelType::PrivateThread)
        .auto_archive_duration(AutoArchiveDuration::Week)
        .invitable(false)
        .await;
    let thread = match made {
        Ok(response) => match response.model().await {
            Ok(thread) => thread.id,
            Err(_) => return Err(t!(lang, "tickets.discord.refused")),
        },
        Err(error) => return Err(discord_refusal(&error, lang, Need::Thread(parent.get()))),
    };
    let opener = cx.interaction.author_id().unwrap_or(Id::new(1));
    if let Err(error) = cx.app.http.add_thread_member(thread, opener).await {
        tracing::warn!(thread = thread.get(), %error, "ticket opener not added to the thread");
    }
    Ok((parent.get(), Some(thread.get())))
}
