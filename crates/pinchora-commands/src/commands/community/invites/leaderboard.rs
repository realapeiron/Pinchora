use super::*;

pub(super) async fn bonus(cx: &Cx, guild: u64, user: u64, amount: i64, remove: bool) -> anyhow::Result<()> {
    if !(1..=1000).contains(&amount) {
        return cx.reply_ephemeral(t!(cx.lang(), "invites.bonus-range")).await;
    }
    let by = if remove { -amount } else { amount };
    let total = cx.app.db.change_invite_bonus(guild, user, by as i32).await?;
    reconcile_invite_rewards(&cx.app, guild, user).await?;
    cx.reply_ephemeral(t!(
        cx.lang(),
        "invites.bonus-saved",
        total = grouped_in(cx.lang(), i64::from(total))
    ))
    .await
}

pub(super) async fn card_png(
    app: &App,
    title: &str,
    score: &InviteScore,
    lang: pinchora_core::infra::i18n::Lang,
) -> anyhow::Result<Vec<u8>> {
    let title = title.to_owned();
    let labels = [
        t!(lang, "invites.card.regular"),
        t!(lang, "invites.card.left"),
        t!(lang, "invites.card.fake"),
        t!(lang, "invites.card.bonus"),
    ];
    let values = [score.regular, score.left, score.fake, i64::from(score.bonus)];
    let total = t!(lang, "invites.card.total");
    let score = score.clone();
    app.render(move |r| {
        let mut svg = String::from("<svg xmlns='http://www.w3.org/2000/svg' width='900' height='420' viewBox='0 0 900 420'><rect width='900' height='420' fill='#101936'/><rect x='16' y='16' width='868' height='388' rx='12' fill='#172a53' stroke='#68aeea' stroke-width='4'/><rect x='38' y='38' width='824' height='72' fill='#233d70'/>");
        use std::fmt::Write;
        let _ = write!(svg, "<text x='58' y='84' font-size='32' font-family='Silkscreen' font-weight='bold' fill='#fff'>{}</text>", escape_svg(&title));
        for (i, (label, value)) in labels.iter().zip(values).enumerate() {
            let x = 40 + i * 210;
            let value = grouped_in(lang, value);
            let _ = write!(svg, "<rect x='{x}' y='134' width='194' height='140' fill='#203a6c' stroke='#426ca3' stroke-width='2'/><text x='{}' y='180' font-family='Silkscreen' font-size='20' fill='#accff2'>{}</text><text x='{}' y='244' font-family='Silkscreen' font-size='44' font-weight='bold' fill='#fff'>{value}</text>", x+16, escape_svg(label), x+16);
        }
        let _ = write!(svg, "<rect x='40' y='298' width='820' height='80' fill='#0d9274'/><text x='64' y='350' font-family='Silkscreen' font-size='26' font-weight='bold' fill='#fff'>{}</text><text x='826' y='350' text-anchor='end' font-family='Silkscreen' font-size='38' font-weight='bold' fill='#fff'>{}</text></svg>", escape_svg(&total), grouped_in(lang, score.total));
        r.svg_to_png(&svg)
    }).await
}

pub(super) async fn top(cx: &Cx, guild: u64, span: Span, page: u64) -> anyhow::Result<()> {
    let since = match span {
        Span::All => None,
        Span::Week => Some(Utc::now() - chrono::Duration::days(7)),
        Span::Month => Some(Utc::now() - chrono::Duration::days(30)),
    };
    let leaders = cx.app.db.invite_leaders(guild, since, 10, (page * 10) as i64).await?;
    let mut lines = Vec::new();
    for (i, row) in leaders.iter().enumerate() {
        lines.push(format!(
            "**{}.** <@{}> · **{}**",
            page * 10 + i as u64 + 1,
            row.user_id,
            grouped_in(cx.lang(), row.total)
        ));
    }
    let description = if lines.is_empty() {
        t!(cx.lang(), "invites.top.empty")
    } else {
        lines.join("\n")
    };
    let embed = EmbedBuilder::new()
        .color(BRAND)
        .title(t!(cx.lang(), "invites.top.title"))
        .description(description)
        .build();
    let buttons = [
        ("prev", page.saturating_sub(1), page == 0),
        ("next", page + 1, leaders.len() < 10),
    ]
    .into_iter()
    .map(|(kind, n, disabled)| {
        Component::Button(Button {
            custom_id: Some(format!("in:{kind}:{n}:{}", span.key())),
            disabled,
            emoji: None,
            id: None,
            label: Some(t!(
                cx.lang(),
                if kind == "next" {
                    "invites.next"
                } else {
                    "invites.previous"
                }
            )),
            style: ButtonStyle::Secondary,
            url: None,
            sku_id: None,
        })
    })
    .collect();
    cx.reply(InteractionResponseData {
        embeds: Some(vec![embed]),
        components: Some(vec![Component::ActionRow(ActionRow {
            id: None,
            components: buttons,
        })]),
        ..Default::default()
    })
    .await
}

impl Span {
    fn key(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::Week => "week",
            Self::Month => "month",
        }
    }
}

