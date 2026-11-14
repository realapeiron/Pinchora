use super::*;

/// The first message in a ticket: who it is for, what the panel says to them, their form answers, and the
/// controls. It pings the opener and the panel's support roles, nobody else.
pub(super) async fn welcome(
    cx: &Cx,
    ticket: &Ticket,
    panel: &TicketPanel,
    topic: &Topic,
    answers: &[Answer],
    lang: pinchora_core::infra::i18n::Lang,
) {
    let Some(place) = ticket.place() else { return };
    let opener = ticket.opener_id as u64;
    let server = server_name(&cx.app, ticket.guild_id as u64).await;
    let template = if panel.welcome_text.trim().is_empty() {
        t!(lang, "tickets.welcome.default")
    } else {
        strip_mentions(&panel.welcome_text)
    };
    let mut text = fill_welcome(
        &template,
        &format!("<@{opener}>"),
        &scrub_value(&server),
        ticket.number,
        &scrub_value(&topic.label),
    );
    if let Ok(ops) = cx.app.db.ticket_operation_settings(ticket.guild_id as u64).await
        && outside_support_hours(&ops, cx.app.now())
    {
        text.push_str("\n\n");
        text.push_str(&t!(
            lang,
            "tickets.v2.out-of-hours",
            message = scrub_value(&ops.out_of_hours_message)
        ));
    }
    let embed = welcome_embed(ticket, &text, answers, lang);
    let roles: Vec<u64> = panel.support_roles.iter().map(|r| *r as u64).collect();
    let mut content = format!("<@{opener}>");
    for role in &roles {
        content.push_str(&format!(" <@&{role}>"));
    }
    let embeds = [cx.app.brand_embed(ticket.guild_id as u64, embed).await];
    let components = controls(ticket.id, false, lang);
    let mentions = pinging(&[opener], &roles);
    let sent = cx
        .app
        .http
        .create_message(Id::new(place))
        .content(&content)
        .embeds(&embeds)
        .components(&components)
        .allowed_mentions(Some(&mentions))
        .await;
    if let Err(error) = sent {
        tracing::warn!(ticket = ticket.id, %error, "ticket welcome not posted");
    }
    for (index, batch) in answers.iter().skip(5).collect::<Vec<_>>().chunks(5).enumerate() {
        let mut embed = EmbedBuilder::new()
            .color(BRAND)
            .title(t!(lang, "tickets.v2.more-answers", page = index + 2));
        for answer in batch {
            embed = embed.field(EmbedFieldBuilder::new(
                single_line(&answer.question).chars().take(256).collect::<String>(),
                scrub_value(&answer.answer).chars().take(900).collect::<String>(),
            ));
        }
        let embeds = [cx.app.brand_embed(ticket.guild_id as u64, embed.build()).await];
        if let Err(error) = cx.app.http.create_message(Id::new(place)).embeds(&embeds).await {
            tracing::warn!(ticket = ticket.id, %error, "additional ticket form answers not posted");
        }
    }
}

pub(super) fn outside_support_hours(
    settings: &pinchora_db::TicketOperationSettings,
    now: chrono::DateTime<Utc>,
) -> bool {
    if settings.support_hours.is_empty() {
        return false;
    }
    let Ok(tz) = settings.support_timezone.parse::<Tz>() else {
        return false;
    };
    let local = now.with_timezone(&tz);
    let day = local.weekday().number_from_monday() as u8;
    let minute = (local.hour() * 60 + local.minute()) as u16;
    !settings
        .support_hours
        .iter()
        .any(|window| window.day == day && window.start <= minute && minute < window.end)
}

pub(crate) fn welcome_embed(
    ticket: &Ticket,
    text: &str,
    answers: &[Answer],
    lang: pinchora_core::infra::i18n::Lang,
) -> Embed {
    let mut embed = EmbedBuilder::new()
        .color(BRAND)
        .title(t!(
            lang,
            "tickets.welcome.title",
            number = number(ticket.number),
            topic = single_line(&ticket.topic)
        ))
        .description(text.chars().take(4_000).collect::<String>())
        .footer(EmbedFooterBuilder::new(t!(lang, "tickets.welcome.footer")));
    for answer in answers.iter().take(5) {
        let name: String = single_line(&answer.question).chars().take(256).collect();
        let value: String = scrub_value(&answer.answer).chars().take(1_024).collect();
        embed = embed.field(EmbedFieldBuilder::new(name, value));
    }
    embed.build()
}

/// A log entry: a coloured embed with a title and a line or two.
pub(crate) fn log_embed(color: u32, title: String, text: String) -> Embed {
    EmbedBuilder::new().color(color).title(title).description(text).build()
}
