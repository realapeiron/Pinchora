use super::*;

/// The message context command lets the member pick a live panel and topic before the normal
/// FAQ, eligibility and form flow. The source link is carried through those steps.
pub(crate) async fn on_message_context(cx: &Cx, guild: u64, channel: u64, message: u64) -> anyhow::Result<()> {
    if !crate::community::tickets::module_on(&cx.app, guild, Some(channel)).await {
        return tell(cx, t!(cx.lang(), "tickets.v2.context-unavailable")).await;
    }
    let options: Vec<SelectMenuOption> = cx
        .app
        .db
        .ticket_panels(guild)
        .await?
        .into_iter()
        .filter(|panel| panel.enabled && !panel.topics.is_empty())
        .map(|panel| SelectMenuOption {
            default: false,
            description: None,
            emoji: None,
            label: single_line(&panel.title).chars().take(100).collect(),
            value: panel.id.to_string(),
        })
        .collect();
    if options.is_empty() {
        return tell(cx, t!(cx.lang(), "tickets.v2.context-unavailable")).await;
    }
    cx.reply(private(
        t!(cx.lang(), "tickets.v2.context-pick"),
        vec![row(vec![Component::SelectMenu(SelectMenu {
            channel_types: None,
            custom_id: format!("tk:m:{channel}:{message}"),
            default_values: None,
            disabled: false,
            id: None,
            kind: SelectMenuType::Text,
            max_values: Some(1),
            min_values: Some(1),
            options: Some(options),
            placeholder: Some(t!(cx.lang(), "tickets.v2.context-pick")),
            required: Some(true),
        })])],
    ))
    .await
}

pub(super) async fn choose_context_topic(cx: &Cx, panel_id: i64, source: Option<(u64, u64)>) -> anyhow::Result<()> {
    let Some((panel, _)) = usable(cx, panel_id, 0).await? else {
        return Ok(());
    };
    let Some((channel, message)) = source else {
        return tell(cx, t!(cx.lang(), "tickets.expired")).await;
    };
    let options = panel
        .topics
        .iter()
        .enumerate()
        .map(|(index, topic)| SelectMenuOption {
            default: false,
            description: None,
            emoji: None,
            label: single_line(&topic.label).chars().take(100).collect(),
            value: index.to_string(),
        })
        .collect();
    cx.reply(private(
        t!(cx.lang(), "tickets.v2.context-topic"),
        vec![row(vec![Component::SelectMenu(SelectMenu {
            channel_types: None,
            custom_id: format!("tk:q:{panel_id}:{channel}:{message}"),
            default_values: None,
            disabled: false,
            id: None,
            kind: SelectMenuType::Text,
            max_values: Some(1),
            min_values: Some(1),
            options: Some(options),
            placeholder: Some(t!(cx.lang(), "tickets.v2.context-topic")),
            required: Some(true),
        })])],
    ))
    .await
}
