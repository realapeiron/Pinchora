//! Public event cards, reminders, Discord Scheduled Events and occurrence ticker.

use std::{sync::Arc, time::Duration as StdDuration};

use pinchora_core::modules::{Access, Module};
use pinchora_core::{i18n::Lang, t};
use pinchora_db::EventRow;
use pinchora_render::thousands_in;
use twilight_model::{
    channel::message::{
        AllowedMentions, Component,
        component::{ActionRow, Button, ButtonStyle},
    },
    guild::scheduled_event::PrivacyLevel,
    id::Id,
    util::Timestamp,
};
use twilight_util::builder::embed::{EmbedBuilder, EmbedFieldBuilder};

use crate::{App, util::BRAND};

fn card(
    row: &EventRow,
    lang: Lang,
    counts: &[(String, i64)],
) -> (twilight_model::channel::message::Embed, Vec<Component>) {
    let mut embed = EmbedBuilder::new()
        .color(BRAND)
        .title(format!("{} · #{}", row.title, row.id))
        .description(format!(
            "{}\n\n<t:{}:F> · <t:{}:R>\n{} <t:{}:t>",
            row.description,
            row.starts_at.timestamp(),
            row.starts_at.timestamp(),
            t!(lang, "events.ends"),
            row.ends_at.timestamp()
        ));
    if !row.location.is_empty() {
        embed = embed.field(EmbedFieldBuilder::new(t!(lang, "events.location"), &row.location).build());
    }
    if let Some(role) = row.required_role_id {
        embed = embed.field(EmbedFieldBuilder::new(t!(lang, "events.required"), format!("<@&{role}>")).build());
    }
    let options = row.options();
    let mut buttons = Vec::new();
    for (idx, o) in options.iter().enumerate() {
        let count = counts.iter().find(|(key, _)| key == &o.key).map_or(0, |(_, n)| *n);
        let name = if idx < 3 {
            t!(lang, &format!("events.option.{}", o.key))
        } else {
            o.label.clone()
        };
        let count = thousands_in(lang, count);
        let label = if o.capacity > 0 {
            format!("{name} {count}/{}", thousands_in(lang, o.capacity.into()))
        } else {
            format!("{name} {count}")
        };
        buttons.push(Component::Button(Button {
            id: None,
            custom_id: Some(format!("ev:r:{}:{}", row.id, o.key)),
            disabled: row.status != "upcoming" || row.ends_at <= chrono::Utc::now(),
            emoji: None,
            label: Some(label),
            style: if idx == 0 {
                ButtonStyle::Success
            } else {
                ButtonStyle::Secondary
            },
            url: None,
            sku_id: None,
        }));
    }
    (
        embed.build(),
        vec![Component::ActionRow(ActionRow {
            id: None,
            components: buttons,
        })],
    )
}

async fn counts(app: &App, id: i64) -> anyhow::Result<Vec<(String, i64)>> {
    app.db.event_counts(id).await
}

/// Posts the RSVP card, once per occurrence.
pub async fn publish(app: &App, row: &EventRow) -> anyhow::Result<()> {
    let Some(channel) = row.channel_id else { return Ok(()) };
    let lang = app.guild_lang(row.guild_id as u64).await;
    let (embed, components) = card(row, lang, &[]);
    let embed = app.brand_embed(row.guild_id as u64, embed).await;
    let message = app
        .http
        .create_message(Id::new(channel as u64))
        .embeds(&[embed])
        .components(&components)
        .allowed_mentions(Some(&AllowedMentions::default()))
        .await?
        .model()
        .await?;
    app.db
        .set_event_message(row.id, channel as u64, message.id.get())
        .await?;
    Ok(())
}

pub async fn refresh_card(app: &App, row: &EventRow) {
    let (Some(channel), Some(message)) = (row.channel_id, row.message_id) else {
        return;
    };
    let result = async {
        let lang = app.guild_lang(row.guild_id as u64).await;
        let (embed, components) = card(row, lang, &counts(app, row.id).await?);
        let embed = app.brand_embed(row.guild_id as u64, embed).await;
        app.http
            .update_message(Id::new(channel as u64), Id::new(message as u64))
            .embeds(Some(&[embed]))
            .components(Some(&components))
            .allowed_mentions(Some(&AllowedMentions::default()))
            .await?;
        anyhow::Ok(())
    }
    .await;
    if let Err(error) = result {
        tracing::warn!(%error,id=row.id,"event card update failed");
    }
}

pub(crate) async fn schedule_all(app: &App, row: &EventRow) -> anyhow::Result<()> {
    app.db.reconcile_event_reminders(row.id).await
}

/// Checks the RSVP and privacy choice again at delivery time.
pub(crate) async fn reminder_plan(
    app: &App,
    reminder: &pinchora_db::ReminderRow,
    minutes: i32,
    lang: Lang,
) -> anyhow::Result<Option<(String, String)>> {
    let Some(row) = app.db.event_by_id(reminder.ref_id).await? else {
        return Ok(None);
    };
    if row.status != "upcoming"
        || row.starts_at <= app.now()
        || app
            .module_access(row.guild_id as u64, row.channel_id.map(|id| id as u64), Module::Events)
            .await?
            != Access::Allowed
    {
        return Ok(None);
    }
    if !app.db.claim_event_delivery(reminder, minutes).await? {
        return Ok(None);
    }
    Ok(Some((
        t!(lang, "events.reminder-title", title = row.title),
        t!(
            lang,
            "events.reminder-text",
            minutes = thousands_in(lang, minutes.into()),
            when = format!("<t:{}:F>", row.starts_at.timestamp())
        ),
    )))
}

pub async fn sync_discord(app: &App, row: &EventRow, on: bool) -> anyhow::Result<()> {
    let guild = Id::new(row.guild_id as u64);
    if !on {
        if let Some(id) = row.discord_event_id {
            check_manage_events(app, row.guild_id as u64).await?;
            app.http.delete_guild_scheduled_event(guild, Id::new(id as u64)).await?;
            app.db.set_discord_event(row.id, None).await?;
        }
        return Ok(());
    }
    check_manage_events(app, row.guild_id as u64).await?;
    let start = Timestamp::from_secs(row.starts_at.timestamp())?;
    let end = Timestamp::from_secs(row.ends_at.timestamp())?;
    let location = if row.location.is_empty() {
        "Pinchora"
    } else {
        &row.location
    };
    if let Some(id) = row.discord_event_id {
        app.http
            .update_guild_scheduled_event(guild, Id::new(id as u64))
            .name(&row.title)
            .description((!row.description.is_empty()).then_some(row.description.as_str()))
            .location(Some(location))
            .scheduled_start_time(&start)
            .scheduled_end_time(Some(&end))
            .await?;
    } else {
        let request = app
            .http
            .create_guild_scheduled_event(guild, PrivacyLevel::GuildOnly)
            .external(&row.title, location, &start, &end);
        let request = if row.description.is_empty() {
            request
        } else {
            request.description(&row.description)
        };
        let created = request.await?.model().await?;
        app.db.set_discord_event(row.id, Some(created.id.get())).await?;
    }
    Ok(())
}

/// Checks the bot's guild permission before a dashboard write requests a mirror.
pub async fn check_manage_events(app: &App, guild_id: u64) -> anyhow::Result<()> {
    let guild = Id::new(guild_id);
    let me = app
        .http
        .guild_member(guild, Id::new(app.application_id.get()))
        .await?
        .model()
        .await?;
    let roles = app.role_table(guild_id).await?;
    let permissions = me
        .roles
        .iter()
        .map(|r| r.get())
        .chain([guild_id])
        .filter_map(|id| roles.get(&id))
        .fold(twilight_model::guild::Permissions::empty(), |p, r| p | r.permissions);
    anyhow::ensure!(
        permissions.contains(twilight_model::guild::Permissions::MANAGE_EVENTS)
            || permissions.contains(twilight_model::guild::Permissions::ADMINISTRATOR),
        "Manage Events"
    );
    Ok(())
}

pub async fn tick(app: &Arc<App>) -> anyhow::Result<usize> {
    let new = app.db.advance_events(app.now()).await?;
    for row in &new {
        if app
            .module_access(row.guild_id as u64, row.channel_id.map(|id| id as u64), Module::Events)
            .await?
            != Access::Allowed
        {
            continue;
        }
        if let Err(error) = publish(app, row).await {
            tracing::warn!(%error,id=row.id,"recurring event post failed");
        }
        if let Some(parent) = row.parent_id
            && app
                .db
                .event_by_id(parent)
                .await?
                .is_some_and(|old| old.discord_event_id.is_some())
            && let Err(error) = sync_discord(app, row, true).await
        {
            tracing::warn!(%error, id=row.id, "recurring scheduled event sync failed");
        }
    }
    Ok(new.len())
}
pub fn spawn_ticker(app: Arc<App>) {
    tokio::spawn(async move {
        let mut wake = tokio::time::interval(StdDuration::from_secs(30));
        wake.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            wake.tick().await;
            if let Err(error) = tick(&app).await {
                tracing::warn!(%error,"event ticker failed");
            }
        }
    });
}

/// Discord may complete or cancel a mirrored event outside Pinchora.
pub async fn on_scheduled_event_update(
    app: Arc<App>,
    event: Box<twilight_model::gateway::payload::incoming::GuildScheduledEventUpdate>,
) {
    use twilight_model::guild::scheduled_event::Status;
    let status = match event.0.status {
        Status::Cancelled => "cancelled",
        Status::Completed => "ended",
        _ => return,
    };
    match app
        .db
        .update_scheduled_event_status(event.0.guild_id.get(), event.0.id.get(), status)
        .await
    {
        Ok(Some(row)) => refresh_card(&app, &row).await,
        Ok(None) => {}
        Err(error) => tracing::warn!(%error,"scheduled event status sync failed"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rsvp_cards_localize_labels_and_large_counts() {
        let start = chrono::Utc::now() + chrono::Duration::hours(1);
        let mut options = pinchora_core::events::default_options();
        options[0].capacity = 2500;
        let row: EventRow = serde_json::from_value(serde_json::json!({
            "id": 1, "guild_id": 1, "title": "Buluşma", "description": "",
            "starts_at": start, "ends_at": start + chrono::Duration::hours(1),
            "tz": "Europe/Istanbul", "location": "", "options": options,
            "reminders": [15, 60], "status": "upcoming"
        }))
        .unwrap();
        for (lang, expected) in [(Lang::Tr, "Katılıyorum 1.234/2.500"), (Lang::En, "Going 1,234/2,500")] {
            let (_, components) = card(&row, lang, &[("going".into(), 1234)]);
            let Component::ActionRow(actions) = &components[0] else {
                panic!("action row")
            };
            let Component::Button(button) = &actions.components[0] else {
                panic!("button")
            };
            assert_eq!(button.label.as_deref(), Some(expected));
        }
    }
}
