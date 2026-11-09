use crate::App;
use pinchora_core::admin::modules::Module;
use twilight_model::{
    channel::{ChannelType, message::AllowedMentions},
    id::Id,
};
/// The existing ticket store and permission model, with no pings or recursive ticket event.
pub(super) async fn open_ticket(app: &App, guild: u64, user: u64, panel_id: i64, topic: usize) -> anyhow::Result<()> {
    use pinchora_db::{NewTicket, TicketOpen};
    anyhow::ensure!(
        user > 0 && app.guild_modules(guild).await?.rule(Module::Tickets).enabled,
        "tickets_disabled"
    );
    let panel = app
        .db
        .ticket_panel(panel_id)
        .await?
        .filter(|p| p.guild_id == guild as i64 && p.enabled)
        .ok_or_else(|| anyhow::anyhow!("panel"))?;
    anyhow::ensure!(panel.questions.is_empty(), "ticket_form_required");
    let topic = panel.topics.get(topic).ok_or_else(|| anyhow::anyhow!("topic"))?;
    let settings = app.db.ticket_settings(guild).await?;
    let ops = app.db.ticket_panel_operations(panel.id).await?;
    let member = app
        .http
        .guild_member(Id::new(guild), Id::new(user))
        .await?
        .model()
        .await?;
    if let Some(refusal) = crate::community::tickets::eligibility_refusal(
        &ops,
        user,
        member.roles.iter().map(|role| role.get()),
        member.joined_at.map(|joined| joined.as_secs()),
        app.now(),
        app.guild_lang(guild).await,
    ) {
        anyhow::bail!(refusal);
    }
    // The insertion transaction enforces the configured cooldown and all opening limits
    // across workflows and member interactions, including concurrent opens on other pods.
    let count = if settings.mode() == pinchora_core::community::tickets::Mode::Auto {
        app.http.guild_channels(Id::new(guild)).await?.models().await?.len()
    } else {
        0
    };
    let new = NewTicket {
        guild_id: guild,
        panel_id: Some(panel.id),
        opener: user,
        topic: &topic.label,
        answers: &[],
        max_open: settings.max_open_per_user,
        at: app.now(),
    };
    let TicketOpen::Opened(ticket) = app.db.open_ticket(&new).await? else {
        anyhow::bail!("ticket_limit")
    };
    let made = create_place(app, guild, user, &panel, &settings, &ticket, count).await;
    let channel = match made {
        Ok(channel) => channel,
        Err(error) => {
            if app.db.ticket(ticket.id).await?.is_some_and(|t| t.place().is_none()) {
                app.db.abort_ticket(ticket.id).await?;
            }
            return Err(error);
        }
    };
    let lang = app.guild_lang(guild).await;
    let template = if panel.welcome_text.trim().is_empty() {
        pinchora_core::t!(lang, "tickets.welcome.default")
    } else {
        panel.welcome_text.clone()
    };
    let text = pinchora_core::community::tickets::fill_welcome(
        &template,
        &format!("<@{user}>"),
        &crate::community::tickets::workflow_server_name(app, guild).await,
        ticket.number,
        &topic.label,
    );
    let embed = app
        .brand_embed(
            guild,
            crate::community::tickets::workflow_welcome_embed(&ticket, &text, &[], lang),
        )
        .await;
    app.http
        .create_message(channel)
        .embeds(&[embed])
        .components(&crate::community::tickets::workflow_controls(ticket.id, false, lang))
        .allowed_mentions(Some(&AllowedMentions::default()))
        .await?;
    Ok(())
}

async fn create_place(
    app: &App,
    guild: u64,
    user: u64,
    panel: &pinchora_db::TicketPanel,
    settings: &pinchora_db::TicketSettings,
    ticket: &pinchora_db::Ticket,
    count: usize,
) -> anyhow::Result<twilight_model::id::Id<twilight_model::id::marker::ChannelMarker>> {
    let name = format!("ticket-{}", ticket.number);
    if settings.mode().threads(count) {
        let parent = Id::new(panel.channel_id as u64);
        anyhow::ensure!(
            app.http.channel(parent).await?.model().await?.guild_id.map(Id::get) == Some(guild),
            "foreign_channel"
        );
        let channel = app
            .http
            .create_thread(parent, &name, ChannelType::PrivateThread)
            .auto_archive_duration(twilight_model::channel::thread::AutoArchiveDuration::Week)
            .invitable(false)
            .await?
            .model()
            .await?;
        // Keep the place even if adding the member fails, so staff can recover it.
        app.db
            .set_ticket_place(ticket.id, parent.get(), Some(channel.id.get()))
            .await?;
        app.http.add_thread_member(channel.id, Id::new(user)).await?;
        Ok::<_, anyhow::Error>(channel.id)
    } else {
        let overwrites =
            crate::community::tickets::workflow_overwrites(guild, user, app.application_id.get(), &panel.support_roles);
        let mut request = app
            .http
            .create_guild_channel(Id::new(guild), &name)
            .kind(ChannelType::GuildText)
            .permission_overwrites(&overwrites);
        if let Some(category) = panel.open_category_id.or(settings.open_category_id) {
            request = request.parent_id(Id::new(category as u64));
        }
        let channel = request.await?.model().await?;
        app.db.set_ticket_place(ticket.id, channel.id.get(), None).await?;
        Ok(channel.id)
    }
}
