use super::*;

/// `cd:g:<drop>:<slot>`: a click on a numbered card.
pub(crate) async fn on_grab(cx: &Cx, mut parts: Split<'_, char>) -> anyhow::Result<()> {
    let (Some(action @ ("g" | "w" | "v")), Some(id), Some(slot)) = (
        parts.next(),
        parts.next().and_then(|v| v.parse::<i64>().ok()),
        parts.next().and_then(|v| v.parse::<usize>().ok()),
    ) else {
        return cx.reply_ephemeral(t!(cx.lang(), "collect.button-expired")).await;
    };
    if cx
        .app
        .db
        .drop_row(id)
        .await?
        .is_none_or(|r| Some(r.guild_id as u64) != cx.guild_id() || r.channel_id as u64 != channel_of(cx))
    {
        return cx.reply_ephemeral(t!(cx.lang(), "drop.gone")).await;
    }
    if action == "v" {
        if cx
            .app
            .cache
            .try_cooldown(
                &format!("crittervote:{}", cx.author()?.id.get()),
                Duration::from_secs(2),
            )
            .await?
            .is_some()
        {
            return cx.reply_ephemeral(t!(cx.lang(), "collection-depth.claim-slow")).await;
        }
        return match cx
            .app
            .db
            .vote_card(id, slot, cx.author()?.id.get(), cx.app.now())
            .await?
        {
            VoteCard::Cast => {
                cx.reply_ephemeral(t!(cx.lang(), "collection-depth.vote-cast", number = slot + 1))
                    .await
            }
            VoteCard::Gone => cx.reply_ephemeral(t!(cx.lang(), "collection-depth.vote-closed")).await,
            VoteCard::Full => cx.reply_ephemeral(t!(cx.lang(), "collection-depth.vote-full")).await,
        };
    }
    if action == "w" {
        let row = cx.app.db.drop_row(id).await?;
        if row
            .as_ref()
            .is_none_or(|r| !r.is_open() || r.expires_at <= cx.app.now())
        {
            return cx.reply_ephemeral(t!(cx.lang(), "drop.gone")).await;
        }
        #[allow(deprecated)]
        let field = Component::Label(Label {
            component: Box::new(Component::TextInput(TextInput {
                custom_id: "answer".to_owned(),
                id: None,
                label: None,
                max_length: Some(20),
                min_length: Some(1),
                placeholder: None,
                required: Some(true),
                style: TextInputStyle::Short,
                value: None,
            })),
            description: None,
            id: None,
            label: t!(cx.lang(), "collection-depth.word-label"),
        });
        return cx
            .modal(
                format!("cd:m:{id}:{slot}"),
                &t!(cx.lang(), "collection-depth.word-title"),
                vec![field],
            )
            .await;
    }
    claim(cx, id, slot, None).await
}

pub(crate) async fn on_word(cx: &Cx, mut parts: Split<'_, char>, fields: &[(String, String)]) -> anyhow::Result<()> {
    let (Some("m"), Some(id), Some(slot)) = (
        parts.next(),
        parts.next().and_then(|v| v.parse::<i64>().ok()),
        parts.next().and_then(|v| v.parse::<usize>().ok()),
    ) else {
        return cx.reply_ephemeral(t!(cx.lang(), "collect.button-expired")).await;
    };
    if cx
        .app
        .cache
        .try_cooldown(
            &format!("critterword:{}", cx.author()?.id.get()),
            Duration::from_secs(3),
        )
        .await?
        .is_some()
    {
        return cx.reply_ephemeral(t!(cx.lang(), "collection-depth.claim-slow")).await;
    }
    let answer = fields
        .iter()
        .find(|(key, _)| key == "answer")
        .map(|(_, value)| value.trim())
        .unwrap_or("");
    claim(cx, id, slot, Some(answer)).await
}

