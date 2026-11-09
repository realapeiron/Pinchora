use super::*;

/// What a member may do in a locked ticket: read, not write.
fn read_only() -> PermissionOverwrite {
    PermissionOverwrite {
        allow: Some(Permissions::VIEW_CHANNEL | Permissions::READ_MESSAGE_HISTORY),
        deny: Some(Permissions::SEND_MESSAGES | Permissions::ATTACH_FILES | Permissions::ADD_REACTIONS),
        id: Id::new(1),
        kind: PermissionOverwriteType::Member,
    }
}

pub(super) fn member_overwrite(user: u64) -> PermissionOverwrite {
    PermissionOverwrite {
        allow: Some(member_allow()),
        deny: None,
        id: Id::new(user),
        kind: PermissionOverwriteType::Member,
    }
}

pub(super) async fn lock(cx: &Cx, ticket: &Ticket, locking: bool) -> anyhow::Result<()> {
    let lang = cx.lang();
    let Some(place) = place_of(ticket) else {
        return tell(cx, t!(lang, "tickets.gone")).await;
    };
    if let Some(lacking) = missing(cx, Need::Manage(place.get())) {
        return tell(cx, lacking).await;
    }
    let done = if ticket.thread_id.is_some() {
        cx.app
            .http
            .update_thread(place)
            .locked(locking)
            .await
            .map(|_| ())
            .map_err(anyhow::Error::from)
    } else {
        lock_channel_members(cx, ticket, place, locking).await
    };
    if let Err(error) = done {
        let message = error.downcast_ref::<twilight_http::Error>().map_or_else(
            || t!(lang, "tickets.discord.refused"),
            |error| discord_refusal(error, lang, Need::Manage(place.get())),
        );
        return tell(cx, message).await;
    }
    cx.app.db.set_ticket_locked(ticket.id, locking).await?;
    let server = guild_lang(&cx.app, ticket.guild_id as u64).await;
    let (user, _) = actor(cx)?;
    let key = if locking {
        "tickets.notice.locked"
    } else {
        "tickets.notice.unlocked"
    };
    notice(
        &cx.app,
        ticket.guild_id as u64,
        Some(place),
        WARN,
        t!(server, key, user = mention(user)),
        Vec::new(),
    )
    .await;
    let key = if locking {
        "tickets.locked-you"
    } else {
        "tickets.unlocked-you"
    };
    tell(cx, t!(lang, key)).await
}

/// A role's staff allowance beats the @everyone write deny; explicit member allowances do not.
async fn lock_channel_members(cx: &Cx, ticket: &Ticket, place: Id<ChannelMarker>, locking: bool) -> anyhow::Result<()> {
    let channel = cx.app.http.channel(place).await?.model().await?;
    let everyone = PermissionOverwrite {
        allow: None,
        deny: Some(if locking {
            Permissions::VIEW_CHANNEL
                | Permissions::SEND_MESSAGES
                | Permissions::ATTACH_FILES
                | Permissions::ADD_REACTIONS
        } else {
            Permissions::VIEW_CHANNEL
        }),
        id: Id::new(ticket.guild_id as u64),
        kind: PermissionOverwriteType::Role,
    };
    cx.app.http.update_channel_permission(place, &everyone).await?;
    let bot = cx.app.application_id.get();
    let mut members = vec![ticket.opener_id as u64];
    let support = match ticket.panel_id {
        Some(id) => cx
            .app
            .db
            .ticket_panel(id)
            .await?
            .map(|p| p.support_roles)
            .unwrap_or_default(),
        None => Vec::new(),
    };
    let guild = Id::new(ticket.guild_id as u64);
    let staff_roles: Vec<u64> = match cx.app.http.roles(guild).await {
        Ok(response) => response
            .models()
            .await?
            .into_iter()
            .filter(|role| {
                support.contains(&(role.id.get() as i64))
                    || crate::community::tickets::STAFF_PERMISSIONS
                        .iter()
                        .any(|needed| role.permissions.contains(*needed))
            })
            .map(|role| role.id.get())
            .collect(),
        Err(_) => support.into_iter().map(|id| id as u64).collect(),
    };
    if locking {
        for role in &staff_roles {
            let overwrite = PermissionOverwrite {
                allow: Some(member_allow()),
                deny: None,
                id: Id::new(*role),
                kind: PermissionOverwriteType::Role,
            };
            cx.app.http.update_channel_permission(place, &overwrite).await?;
        }
    }
    for overwrite in channel.permission_overwrites.unwrap_or_default() {
        let id = overwrite.id.get();
        if overwrite.kind == twilight_model::channel::permission_overwrite::PermissionOverwriteType::Member
            && id != bot
            && !members.contains(&id)
        {
            members.push(id);
        }
    }
    for member in members {
        if locking
            && let Ok(response) = cx.app.http.guild_member(guild, Id::new(member)).await
            && let Ok(found) = response.model().await
            && found.roles.iter().any(|role| staff_roles.contains(&role.get()))
        {
            continue;
        }
        let overwrite = if locking {
            PermissionOverwrite {
                id: Id::new(member),
                ..read_only()
            }
        } else {
            member_overwrite(member)
        };
        cx.app.http.update_channel_permission(place, &overwrite).await?;
    }
    Ok(())
}

pub(super) async fn add(cx: &Cx, ticket: &Ticket, user: u64) -> anyhow::Result<()> {
    let lang = cx.lang();
    let Some(place) = place_of(ticket) else {
        return tell(cx, t!(lang, "tickets.gone")).await;
    };
    let done = if ticket.thread_id.is_some() {
        cx.app.http.add_thread_member(place, Id::new(user)).await.map(|_| ())
    } else {
        cx.app
            .http
            .update_channel_permission(place, &member_overwrite(user))
            .await
            .map(|_| ())
    };
    if let Err(error) = done {
        return tell(cx, discord_refusal(&error, lang, Need::Manage(place.get()))).await;
    }
    let server = guild_lang(&cx.app, ticket.guild_id as u64).await;
    let (by, _) = actor(cx)?;
    notice(
        &cx.app,
        ticket.guild_id as u64,
        Some(place),
        SUCCESS,
        t!(server, "tickets.notice.added", user = mention(user), by = mention(by)),
        Vec::new(),
    )
    .await;
    tell(cx, t!(lang, "tickets.added-you", user = mention(user))).await
}

pub(super) async fn remove(cx: &Cx, ticket: &Ticket, user: u64) -> anyhow::Result<()> {
    let lang = cx.lang();
    if user == ticket.opener_id as u64 {
        return tell(cx, t!(lang, "tickets.remove-opener")).await;
    }
    if user == cx.app.application_id.get() {
        return tell(cx, t!(lang, "tickets.remove-bot")).await;
    }
    let Some(place) = place_of(ticket) else {
        return tell(cx, t!(lang, "tickets.gone")).await;
    };
    let done = if ticket.thread_id.is_some() {
        cx.app.http.remove_thread_member(place, Id::new(user)).await.map(|_| ())
    } else {
        cx.app
            .http
            .delete_channel_permission(place)
            .member(Id::new(user))
            .await
            .map(|_| ())
    };
    if let Err(error) = done {
        return tell(cx, discord_refusal(&error, lang, Need::Manage(place.get()))).await;
    }
    let server = guild_lang(&cx.app, ticket.guild_id as u64).await;
    let (by, _) = actor(cx)?;
    notice(
        &cx.app,
        ticket.guild_id as u64,
        Some(place),
        WARN,
        t!(server, "tickets.notice.removed", user = mention(user), by = mention(by)),
        Vec::new(),
    )
    .await;
    tell(cx, t!(lang, "tickets.removed-you", user = mention(user))).await
}

pub(super) async fn rename(cx: &Cx, ticket: &Ticket, wanted: &str) -> anyhow::Result<()> {
    let lang = cx.lang();
    let Some(place) = place_of(ticket) else {
        return tell(cx, t!(lang, "tickets.gone")).await;
    };
    let opener = cx
        .app
        .user(ticket.opener_id as u64)
        .await
        .map(|u| u.name.clone())
        .unwrap_or_default();
    let name = channel_name(wanted, ticket.number, &opener, &ticket.topic);
    if name.is_empty() {
        return tell(cx, t!(lang, "tickets.rename-empty")).await;
    }
    let done = if ticket.thread_id.is_some() {
        cx.app.http.update_thread(place).name(&name).await.map(|_| ())
    } else {
        cx.app.http.update_channel(place).name(&name).await.map(|_| ())
    };
    if let Err(error) = done {
        return tell(cx, discord_refusal(&error, lang, Need::Manage(place.get()))).await;
    }
    tell(cx, t!(lang, "tickets.renamed", name = name)).await
}

pub(super) async fn move_to(cx: &Cx, ticket: &Ticket, category: u64) -> anyhow::Result<()> {
    let lang = cx.lang();
    if ticket.thread_id.is_some() {
        return tell(cx, t!(lang, "tickets.move-thread")).await;
    }
    let Some(place) = place_of(ticket) else {
        return tell(cx, t!(lang, "tickets.gone")).await;
    };
    let done = cx
        .app
        .http
        .update_channel(place)
        .parent_id(Some(Id::new(category)))
        .await;
    if let Err(error) = done {
        return tell(cx, discord_refusal(&error, lang, Need::Manage(place.get()))).await;
    }
    tell(cx, t!(lang, "tickets.moved", category = format!("<#{category}>"))).await
}
