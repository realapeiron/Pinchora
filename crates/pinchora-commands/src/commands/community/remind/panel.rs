use super::*;

// /Text reminders

pub(super) async fn remind_in(cx: &Cx, user: u64, duration: &str, text: &str) -> anyhow::Result<()> {
    let lang = cx.lang();
    let after = match parse_duration(duration) {
        Ok(after) => after,
        Err(why) => return cx.reply_ephemeral(t!(lang, why)).await,
    };
    let body = strip_mentions(text);
    if body.is_empty() {
        return cx.reply_ephemeral(t!(lang, "remind.write-text")).await;
    }
    let due = cx.app.now() + after;
    match cx.app.db.add_text_reminder_in(user, &body, lang.code(), due).await? {
        None => {
            cx.reply_ephemeral(t!(lang, "remind.too-many", max = MAX_TEXT_REMINDERS))
                .await
        }
        Some(id) => {
            let embed = EmbedBuilder::new()
                .color(BRAND)
                .title(t!(lang, "remind.set-title"))
                .description(t!(lang, "remind.set-text", when = relative(due), body = body))
                .footer(EmbedFooterBuilder::new(t!(lang, "remind.set-footer", id = id)))
                .build();
            cx.reply(InteractionResponseData {
                embeds: Some(vec![embed]),
                flags: Some(MessageFlags::EPHEMERAL),
                allowed_mentions: Some(AllowedMentions::default()),
                ..Default::default()
            })
            .await
        }
    }
}

/// "2h30m", "45 min", "1d 12h", "2sa30dk": a time from now. Units are `w`, `d`, `h`, `m` and `s` (and
/// their longer spellings), or in Turkish `hf`, `g`, `sa`, `dk` and `sn`; a bare number is refused
/// because it could mean anything. An error is the catalog key of what to tell the player.
pub(super) fn parse_duration(text: &str) -> Result<chrono::Duration, &'static str> {
    const BAD: &str = "remind.bad-time";
    const TOO_LONG: &str = "remind.time-too-long";
    let mut total = chrono::Duration::zero();
    let mut chars = text.trim().chars().peekable();
    let mut any = false;
    while chars.peek().is_some() {
        while chars.next_if(|c| c.is_whitespace() || *c == ',').is_some() {}
        let mut digits = String::new();
        while let Some(c) = chars.next_if(char::is_ascii_digit) {
            digits.push(c);
        }
        while !digits.is_empty() && chars.next_if(|c| c.is_whitespace()).is_some() {}
        let mut unit = String::new();
        while let Some(c) = chars.next_if(|c| c.is_alphabetic()) {
            unit.extend(c.to_lowercase());
        }
        if digits.is_empty() && unit.is_empty() && chars.peek().is_none() {
            break;
        }
        let amount: i64 = digits.parse().map_err(|_| BAD)?;
        let step = match unit.as_str() {
            "w" | "wk" | "wks" | "week" | "weeks" | "hf" | "hafta" => chrono::Duration::try_weeks(amount),
            "d" | "day" | "days" | "g" | "gün" | "gun" => chrono::Duration::try_days(amount),
            "h" | "hr" | "hrs" | "hour" | "hours" | "sa" | "saat" => chrono::Duration::try_hours(amount),
            "m" | "min" | "mins" | "minute" | "minutes" | "dk" | "dakika" => chrono::Duration::try_minutes(amount),
            "s" | "sec" | "secs" | "second" | "seconds" | "sn" | "saniye" => chrono::Duration::try_seconds(amount),
            _ => return Err(BAD),
        }
        .ok_or(TOO_LONG)?;
        total = total.checked_add(&step).ok_or(TOO_LONG)?;
        any = true;
        if total > MAX_AFTER {
            return Err("remind.time-year");
        }
    }
    if !any {
        return Err(BAD);
    }
    if total < MIN_AFTER {
        return Err("remind.time-too-soon");
    }
    Ok(total)
}

/// A reminder's text without any mention: users, roles, channels, `@everyone` and `@here` are
/// dropped, and the rest tidied up.
pub(super) fn strip_mentions(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(open) = rest.find('<') {
        out.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        let end = after.find('>');
        let is_mention = end.is_some_and(|end| {
            let inner = &after[..end];
            let digits = inner
                .strip_prefix("@&")
                .or_else(|| inner.strip_prefix("@!"))
                .or_else(|| inner.strip_prefix('@'))
                .or_else(|| inner.strip_prefix('#'));
            digits.is_some_and(|d| !d.is_empty() && d.bytes().all(|b| b.is_ascii_digit()))
        });
        match end {
            Some(end) if is_mention => rest = &after[end + 1..],
            _ => {
                out.push('<');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    // `@everyone` and `@here`, whatever their case, and the zero-width characters people hide them in.
    let plain: String = out
        .chars()
        .filter(|c| !matches!(c, '\u{200b}' | '\u{200c}' | '\u{200d}' | '\u{feff}'))
        .collect();
    let mut cleaned = String::with_capacity(plain.len());
    let mut i = 0;
    while i < plain.len() {
        let rest = &plain.as_bytes()[i..];
        // The words are ASCII, so a match ends on a character boundary.
        let skip = ["@everyone", "@here"]
            .iter()
            .find(|m| rest.len() >= m.len() && rest[..m.len()].eq_ignore_ascii_case(m.as_bytes()))
            .map(|m| m.len());
        match skip {
            Some(n) => i += n,
            None => {
                let c = plain[i..].chars().next().unwrap_or(' ');
                cleaned.push(c);
                i += c.len_utf8();
            }
        }
    }
    let tidy = cleaned.split_whitespace().collect::<Vec<_>>().join(" ");
    tidy.chars().take(MAX_TEXT).collect()
}

// /List

fn chosen(kind: &str, optins: &[String]) -> bool {
    optins.iter().any(|k| k == kind)
}

fn label_of(kind: &str, lang: Lang) -> String {
    match kind {
        "daily" | "weekly" | "monthly" | "pet" | "trip" => t!(lang, &format!("remind.label.{kind}")),
        _ => String::new(),
    }
}

/// The list: which notifications are on, and the reminders still to come, with a menu to cancel one.
pub(super) async fn list_panel(cx: &Cx, user: u64, note: &str) -> anyhow::Result<InteractionResponseData> {
    let lang = cx.lang();
    let optins = cx.app.db.reminder_optins(user).await?;
    let rows = cx.app.db.text_reminders(user).await?;
    let notifications = OPT_IN_KINDS
        .iter()
        .map(|kind| {
            format!(
                "{} {}",
                if chosen(kind, &optins) { "🔔" } else { "🔕" },
                label_of(kind, lang)
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    // An embed field holds 1,024 characters: long notes are cut here, and the menu below has them all.
    let mut reminders = String::new();
    for (i, r) in rows.iter().enumerate() {
        let mut body: String = r.body.chars().take(40).collect();
        if body.len() < r.body.len() {
            body.push('…');
        }
        let line = format!("`#{}` {} · {body}\n", r.id, relative(r.due_at));
        if reminders.chars().count() + line.chars().count() > 980 {
            reminders.push_str(&t!(lang, "remind.list-more", count = rows.len() - i));
            break;
        }
        reminders.push_str(&line);
    }
    let reminders = if rows.is_empty() {
        t!(lang, "remind.list-none")
    } else {
        reminders.trim_end().to_owned()
    };
    let mut embed = EmbedBuilder::new()
        .color(BRAND)
        .title(t!(lang, "remind.list-title"))
        .field(EmbedFieldBuilder::new(
            t!(lang, "remind.list-notifications"),
            notifications,
        ))
        .field(EmbedFieldBuilder::new(t!(lang, "remind.list-reminders"), reminders))
        .footer(EmbedFooterBuilder::new(t!(
            lang,
            "remind.list-footer",
            count = rows.len(),
            max = MAX_TEXT_REMINDERS
        )));
    if !note.is_empty() {
        embed = embed.description(note);
    }
    let mut components = Vec::new();
    if !rows.is_empty() {
        let options = rows
            .iter()
            .map(|r| SelectMenuOption {
                default: false,
                description: Some(r.due_at.format("%Y-%m-%d %H:%M UTC").to_string()),
                emoji: None,
                label: r.body.chars().take(100).collect(),
                value: r.id.to_string(),
            })
            .collect();
        components.push(Component::ActionRow(ActionRow {
            id: None,
            components: vec![Component::SelectMenu(SelectMenu {
                channel_types: None,
                custom_id: "rm:c".to_owned(),
                default_values: None,
                disabled: false,
                id: None,
                kind: SelectMenuType::Text,
                max_values: Some(1),
                min_values: Some(1),
                options: Some(options),
                placeholder: Some(t!(lang, "remind.list-placeholder")),
                required: None,
            })],
        }));
    }
    Ok(InteractionResponseData {
        embeds: Some(vec![embed.build()]),
        components: Some(components),
        flags: Some(MessageFlags::EPHEMERAL),
        allowed_mentions: Some(AllowedMentions::default()),
        ..Default::default()
    })
}

