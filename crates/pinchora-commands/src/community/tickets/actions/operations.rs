//! Staff-only ticket operations.
use crate::community::tickets::actions::*;

pub(super) async fn priority(cx: &Cx, ticket: &Ticket, level: &str, escalate: bool) -> anyhow::Result<()> {
    if !cx.app.db.set_ticket_priority(ticket.id, level, escalate).await? {
        return refuse(cx, Refusal::AlreadyClosed).await;
    }
    let label = match level {
        "low" => t!(cx.lang(), "tickets.v2.priority-low"),
        "high" => t!(cx.lang(), "tickets.v2.priority-high"),
        "urgent" => t!(cx.lang(), "tickets.v2.priority-urgent"),
        _ => t!(cx.lang(), "tickets.v2.priority-normal"),
    };
    tell(cx, t!(cx.lang(), "tickets.v2.priority", priority = label)).await
}

pub(super) async fn transfer(
    cx: &Cx,
    ticket: &Ticket,
    panel: Option<&TicketPanel>,
    user: Option<u64>,
    role: Option<u64>,
) -> anyhow::Result<()> {
    if user.is_some() == role.is_some() {
        return tell(cx, t!(cx.lang(), "tickets.v2.transfer-pick")).await;
    }
    if let Some(role) = role
        && !panel.is_some_and(|p| p.support_roles.contains(&(role as i64)))
    {
        return tell(cx, t!(cx.lang(), "tickets.v2.transfer-role")).await;
    }
    if let Some(role) = role {
        let roles = cx
            .app
            .http
            .roles(Id::new(ticket.guild_id as u64))
            .await?
            .models()
            .await?;
        if role == ticket.guild_id as u64 || !roles.iter().any(|found| found.id.get() == role) {
            return tell(cx, t!(cx.lang(), "tickets.v2.transfer-role")).await;
        }
    }
    if let Some(user) = user {
        let guild = Id::new(ticket.guild_id as u64);
        let member = cx.app.http.guild_member(guild, Id::new(user)).await?.model().await?;
        let roles = cx.app.http.roles(guild).await?.models().await?;
        let staff = member.roles.iter().any(|held| {
            roles.iter().any(|role| {
                role.id == *held
                    && (panel.is_some_and(|p| p.support_roles.contains(&(held.get() as i64)))
                        || crate::community::tickets::STAFF_PERMISSIONS
                            .iter()
                            .any(|needed| role.permissions.contains(*needed)))
            })
        });
        let owner = cx.app.http.guild(guild).await?.model().await?.owner_id.get() == user;
        if !staff && !owner {
            return tell(cx, t!(cx.lang(), "tickets.v2.transfer-staff")).await;
        }
    }
    if !cx.app.db.transfer_ticket(ticket.id, user, role).await? {
        return if cx
            .app
            .db
            .ticket(ticket.id)
            .await?
            .is_some_and(|ticket| ticket.is_open())
        {
            tell(cx, t!(cx.lang(), "tickets.v2.transfer-staff-limit")).await
        } else {
            refuse(cx, Refusal::AlreadyClosed).await
        };
    }
    let target = user.map_or_else(|| format!("<@&{}>", role.unwrap_or_default()), mention);
    notice(
        &cx.app,
        ticket.guild_id as u64,
        place_of(ticket),
        SUCCESS,
        t!(cx.lang(), "tickets.v2.transferred", target = target),
        Vec::new(),
    )
    .await;
    tell(cx, t!(cx.lang(), "tickets.v2.transferred", target = target)).await
}

pub(super) async fn request_close(cx: &Cx, ticket: &Ticket) -> anyhow::Result<()> {
    let settings = cx.app.db.ticket_operation_settings(ticket.guild_id as u64).await?;
    if !cx
        .app
        .db
        .request_ticket_close(ticket.id, cx.author()?.id.get(), settings.close_timeout_hours)
        .await?
    {
        return tell(cx, t!(cx.lang(), "tickets.expired")).await;
    }
    let confirm = row(vec![button(
        format!("tp:rc:{}", ticket.id),
        &t!(cx.lang(), "tickets.v2.close-confirm"),
        None,
        ButtonStyle::Danger,
        false,
    )]);
    notice(
        &cx.app,
        ticket.guild_id as u64,
        place_of(ticket),
        WARN,
        t!(
            cx.lang(),
            "tickets.v2.close-requested",
            hours = settings.close_timeout_hours
        ),
        vec![confirm],
    )
    .await;
    tell(
        cx,
        t!(
            cx.lang(),
            "tickets.v2.close-requested",
            hours = settings.close_timeout_hours
        ),
    )
    .await
}

pub(super) async fn note(cx: &Cx, ticket: &Ticket, body: &str) -> anyhow::Result<()> {
    let body: String = body.trim().chars().take(2000).collect();
    if body.is_empty() {
        return tell(cx, t!(cx.lang(), "tickets.expired")).await;
    }
    cx.app
        .db
        .add_ticket_staff_note(ticket.id, cx.author()?.id.get(), &body)
        .await?;
    tell(cx, t!(cx.lang(), "tickets.v2.note-saved")).await
}

pub(super) async fn snippet(cx: &Cx, ticket: &Ticket, name: &str) -> anyhow::Result<()> {
    let Some(saved) = cx
        .app
        .db
        .ticket_snippets(ticket.guild_id as u64)
        .await?
        .into_iter()
        .find(|s| s.name.eq_ignore_ascii_case(name.trim()))
    else {
        return tell(cx, t!(cx.lang(), "tickets.v2.snippet-missing")).await;
    };
    let panel = match ticket.panel_id {
        Some(id) => cx.app.db.ticket_panel(id).await?,
        None => None,
    };
    let claim_mode = match panel.as_ref() {
        Some(p) => cx.app.db.ticket_panel_operations(p.id).await?.claim_mode,
        None => "optional".into(),
    };
    if claim_mode != "optional" && ticket.claimed_by.is_none() {
        return tell(cx, t!(cx.lang(), "tickets.v2.claimed-required")).await;
    }
    if claim_mode == "exclusive" && ticket.claimed_by != Some(cx.author()?.id.get() as i64) {
        return tell(cx, t!(cx.lang(), "tickets.v2.exclusive-claim")).await;
    }
    let Some(place) = place_of(ticket) else {
        return tell(cx, t!(cx.lang(), "tickets.gone")).await;
    };
    let body = pinchora_core::community::tickets::fill_welcome(
        &crate::infra::util::strip_mentions(&saved.body),
        &mention(ticket.opener_id as u64),
        &crate::infra::util::strip_mentions(&server_name(&cx.app, ticket.guild_id as u64).await),
        ticket.number,
        &crate::infra::util::strip_mentions(&ticket.topic),
    );
    let body: String = body.chars().take(2000).collect();
    cx.app
        .http
        .create_message(place)
        .content(&body)
        .allowed_mentions(Some(&quiet().unwrap_or_default()))
        .await?;
    tell(cx, t!(cx.lang(), "tickets.v2.snippet-sent")).await
}
