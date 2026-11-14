use super::*;

pub(super) async fn card(cx: &Cx, title: String, subtitle: String, rows: Vec<InfoRow>) -> anyhow::Result<()> {
    card_with_image(cx, title, subtitle, rows, None).await
}
pub(super) async fn card_with_image(
    cx: &Cx,
    title: String,
    subtitle: String,
    rows: Vec<InfoRow>,
    image: Option<Vec<u8>>,
) -> anyhow::Result<()> {
    send_card(cx, title, subtitle, rows, image, Vec::new(), false).await
}

pub(super) async fn send_card(
    cx: &Cx,
    title: String,
    subtitle: String,
    rows: Vec<InfoRow>,
    image: Option<Vec<u8>>,
    components: Vec<Component>,
    update: bool,
) -> anyhow::Result<()> {
    let lang = cx.lang();
    let png = cx
        .app
        .render(move |r| {
            r.svg_to_png(
                &InfoCard {
                    lang,
                    title,
                    subtitle,
                    rows,
                    image,
                    theme: Theme::reef(),
                }
                .to_svg(),
            )
        })
        .await?;
    let data = Cx::card(EmbedBuilder::new().color(BRAND), "game.png", png, components, false);
    if update {
        cx.update(data).await
    } else {
        cx.reply(data).await
    }
}

pub(super) fn tabs(cx: &Cx, token: &str, selected: &str) -> Vec<Component> {
    let button = |tab: &str, label: String| {
        Component::Button(Button {
            custom_id: Some(format!("gt:{token}:{tab}")),
            disabled: tab == selected,
            emoji: None,
            id: None,
            label: Some(label),
            sku_id: None,
            style: if tab == selected {
                ButtonStyle::Primary
            } else {
                ButtonStyle::Secondary
            },
            url: None,
        })
    };
    vec![Component::ActionRow(ActionRow {
        id: None,
        components: vec![
            button("rank", t!(cx.lang(), "trackers.rank-tab")),
            button("mastery", t!(cx.lang(), "trackers.mastery-tab")),
        ],
    })]
}

pub(crate) async fn on_component<'a>(cx: &Cx, mut parts: impl Iterator<Item = &'a str>) -> anyhow::Result<()> {
    let (Some(token), Some(tab), None) = (parts.next(), parts.next(), parts.next()) else {
        return cx.reply_ephemeral(t!(cx.lang(), "trackers.card-expired")).await;
    };
    if token.len() != 32 || !token.bytes().all(|b| b.is_ascii_hexdigit()) || !matches!(tab, "rank" | "mastery") {
        return cx.reply_ephemeral(t!(cx.lang(), "trackers.card-expired")).await;
    }
    let Some(data) = cx.app.db.tracker_cached(&format!("riot:card:{token}"), 300).await? else {
        return cx.reply_ephemeral(t!(cx.lang(), "trackers.card-expired")).await;
    };
    let Some(title) = data["title"].as_str() else {
        return cx.reply_ephemeral(t!(cx.lang(), "trackers.card-expired")).await;
    };
    let rows = data[tab]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|value| {
            Some(row(
                value["label"].as_str()?.to_owned(),
                value["value"].as_str()?.to_owned(),
            ))
        })
        .collect();
    let image = serde_json::from_value::<Option<Vec<u8>>>(data["image"].clone())
        .ok()
        .flatten();
    send_card(
        cx,
        title.to_owned(),
        t!(cx.lang(), "trackers.league"),
        rows,
        image,
        tabs(cx, token, tab),
        true,
    )
    .await
}

