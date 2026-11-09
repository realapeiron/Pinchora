use super::*;

pub(super) async fn save_qotd_settings(cx: &Cx, guild: u64, input: QotdSettings) -> anyhow::Result<()> {
    if input
        .timezone
        .as_ref()
        .is_some_and(|zone| zone.parse::<chrono_tz::Tz>().is_err())
    {
        return cx.reply_ephemeral(t!(cx.lang(), "birthdays.invalid-zone")).await;
    }
    sqlx::query("INSERT INTO qotd_settings(guild_id,channel_id,ping_role_id,hour,timezone,thread_per_question) VALUES($1,$2,$3,COALESCE($4,12),COALESCE($5,'UTC'),COALESCE($6,false)) ON CONFLICT(guild_id) DO UPDATE SET channel_id=COALESCE($2,qotd_settings.channel_id),ping_role_id=COALESCE($3,qotd_settings.ping_role_id),hour=COALESCE($4,qotd_settings.hour),timezone=COALESCE($5,qotd_settings.timezone),thread_per_question=COALESCE($6,qotd_settings.thread_per_question)")
        .bind(guild as i64).bind(input.channel.map(|c|c.id.get() as i64))
        .bind(input.role.map(|r|r.id.get() as i64)).bind(input.hour.map(|hour| hour as i16))
        .bind(input.timezone).bind(input.thread).execute(cx.app.db.pool()).await?;
    cx.app.db.seed_qotd(guild).await?;
    cx.reply_ephemeral(t!(cx.lang(), "community-pack.qotd-settings")).await
}

pub(super) async fn review_qotd_queue(cx: &Cx, guild: u64) -> anyhow::Result<()> {
    let queue = cx.app.db.qotd_queue(guild).await?;
    let items = queue
        .iter()
        .take(5)
        .map(|q| {
            format!(
                "#{} {} {}",
                q.id,
                if q.approved { "✓" } else { "?" },
                if cx.lang() == pinchora_core::infra::i18n::Lang::Tr {
                    q.text_tr.as_deref().unwrap_or(&q.text)
                } else {
                    &q.text
                }
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let pending = queue.iter().filter(|q| !q.approved).take(5);
    let rows = pending
        .map(|q| {
            Component::ActionRow(ActionRow {
                id: None,
                components: vec![
                    Component::Button(Button {
                        custom_id: Some(format!("qd:ok:{}", q.id)),
                        disabled: false,
                        emoji: None,
                        id: None,
                        label: Some(format!("✓ #{}", q.id)),
                        sku_id: None,
                        style: ButtonStyle::Success,
                        url: None,
                    }),
                    Component::Button(Button {
                        custom_id: Some(format!("qd:no:{}", q.id)),
                        disabled: false,
                        emoji: None,
                        id: None,
                        label: Some(format!("✕ #{}", q.id)),
                        sku_id: None,
                        style: ButtonStyle::Danger,
                        url: None,
                    }),
                ],
            })
        })
        .collect();
    cx.reply(InteractionResponseData {
        content: Some(t!(cx.lang(), "community-pack.qotd-queue", items = items)),
        components: Some(rows),
        flags: Some(MessageFlags::EPHEMERAL),
        ..Default::default()
    })
    .await
}

pub(super) async fn setup_counting(cx: &Cx, guild: u64, input: CountingSetup) -> anyhow::Result<()> {
    if let Some(ref role) = input.fail_role
        && let Some(reason) = crate::roles::roles::unassignable(cx, guild, role).await
    {
        return cx.reply_ephemeral(reason).await;
    }
    cx.app
        .db
        .save_counting(
            guild,
            input.channel.id.get(),
            input.reset_on_fail.unwrap_or(true),
            input.allow_math.unwrap_or(false),
            input.fail_role.map(|r| r.id.get()),
        )
        .await?;
    cx.reply_ephemeral(t!(
        cx.lang(),
        "community-pack.count-set",
        channel = input.channel.id.get()
    ))
    .await
}

pub(super) async fn reset_counting(cx: &Cx, guild: u64) -> anyhow::Result<()> {
    let changed = cx.app.db.reset_counting(guild).await?;
    cx.reply_ephemeral(t!(
        cx.lang(),
        if changed {
            "community-pack.count-reset"
        } else {
            "community-pack.count-unset"
        }
    ))
    .await
}

pub(super) async fn create_form(cx: &Cx, guild: u64, input: FormCreate) -> anyhow::Result<()> {
    let questions: Vec<_> = input
        .questions
        .split('|')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect();
    if questions.is_empty() || questions.len() > 5 || questions.iter().any(|s| s.chars().count() > 45) {
        return cx.reply_ephemeral(t!(cx.lang(), "community-pack.form-invalid")).await;
    }
    if let Some(ref role) = input.accept_role
        && let Some(reason) = crate::roles::roles::unassignable(cx, guild, role).await
    {
        return cx.reply_ephemeral(reason).await;
    }
    let Some(id) = cx
        .app
        .db
        .save_form(&pinchora_db::FormRow {
            id: 0,
            guild_id: guild as i64,
            name: input.name,
            questions: serde_json::json!(questions),
            submit_channel_id: Some(input.review_channel.id.get() as i64),
            review_roles: vec![],
            accept_role_id: input.accept_role.map(|r| r.id.get() as i64),
            dm_result: true,
            open: true,
            message_id: None,
        })
        .await?
    else {
        return cx.reply_ephemeral(t!(cx.lang(), "community-pack.form-invalid")).await;
    };
    cx.reply_ephemeral(t!(cx.lang(), "community-pack.form-created", id = id))
        .await
}

pub(super) async fn post_form(cx: &Cx, guild: u64, input: FormPost) -> anyhow::Result<()> {
    let Some(form) = cx.app.db.form(guild, input.id).await?.filter(|f| f.open) else {
        return cx.reply_ephemeral(t!(cx.lang(), "community-pack.form-missing")).await;
    };
    let channel = cx.interaction.channel.as_ref().map(|c| c.id);
    let Some(channel) = channel else {
        return cx.reply_ephemeral(t!(cx.lang(), "community-pack.form-missing")).await;
    };
    let row = Component::ActionRow(ActionRow {
        id: None,
        components: vec![Component::Button(Button {
            custom_id: Some(format!("fm:o:{}", form.id)),
            disabled: false,
            emoji: None,
            id: None,
            label: Some(form.name.clone()),
            sku_id: None,
            style: ButtonStyle::Primary,
            url: None,
        })],
    });
    let response = cx
        .app
        .http
        .create_message(channel)
        .content(&form.name)
        .components(&[row])
        .await?;
    let message = response.model().await?;
    sqlx::query("UPDATE forms SET message_id=$2 WHERE id=$1")
        .bind(form.id)
        .bind(message.id.get() as i64)
        .execute(cx.app.db.pool())
        .await?;
    cx.reply_ephemeral(t!(cx.lang(), "community-pack.form-posted")).await
}

pub(super) async fn close_form(cx: &Cx, guild: u64, input: FormClose) -> anyhow::Result<()> {
    sqlx::query("UPDATE forms SET open=FALSE WHERE guild_id=$1 AND id=$2")
        .bind(guild as i64)
        .bind(input.id)
        .execute(cx.app.db.pool())
        .await?;
    cx.reply_ephemeral(t!(cx.lang(), "community-pack.form-closed")).await
}

pub(super) async fn save_modmail_settings(cx: &Cx, guild: u64, input: ModmailSettingsInput) -> anyhow::Result<()> {
    sqlx::query("INSERT INTO modmail_settings(guild_id,enabled,forum_channel_id,greeting,anonymous_staff,staff_roles,close_after_hours,log_channel_id) VALUES($1,COALESCE($2,false),$3,COALESCE($4,''),COALESCE($5,false),COALESCE($6,'{}'::bigint[]),COALESCE($7,72),$8) ON CONFLICT(guild_id) DO UPDATE SET enabled=COALESCE($2,modmail_settings.enabled),forum_channel_id=COALESCE($3,modmail_settings.forum_channel_id),greeting=COALESCE($4,modmail_settings.greeting),anonymous_staff=COALESCE($5,modmail_settings.anonymous_staff),staff_roles=COALESCE($6,modmail_settings.staff_roles),close_after_hours=COALESCE($7,modmail_settings.close_after_hours),log_channel_id=COALESCE($8,modmail_settings.log_channel_id)")
        .bind(guild as i64).bind(input.enabled)
        .bind(input.forum.map(|c|c.id.get() as i64)).bind(input.greeting)
        .bind(input.anonymous_staff)
        .bind(input.staff_role.map(|r|vec![r.id.get() as i64]))
        .bind(input.close_after_hours.map(|h|h as i32))
        .bind(input.log_channel.map(|c|c.id.get() as i64))
        .execute(cx.app.db.pool()).await?;
    cx.reply_ephemeral(t!(cx.lang(), "community-pack.modmail-settings"))
        .await
}
