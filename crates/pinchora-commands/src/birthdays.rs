//! Minute birthday delivery. The database claim happens before Discord, so pods never duplicate it.

use chrono::Datelike;
use pinchora_core::settings::{self, RoleFacts};
use pinchora_core::{birthdays, modules::Module, t};
use pinchora_render::BirthdayCard;
use std::{sync::Arc, time::Duration};
use twilight_model::{channel::message::AllowedMentions, http::attachment::Attachment, id::Id};

use crate::{App, Cx, commands::games::quiet};

pub async fn tick(app: &Arc<App>) -> anyhow::Result<usize> {
    let now = app.now();
    let mut posted = 0;
    for item in app.db.birthday_candidates(now).await? {
        let guild = item.guild_id as u64;
        let user = item.user_id as u64;
        if !app.module_allowed(guild, None, Module::Birthdays).await {
            continue;
        }
        let Some(tz) = birthdays::timezone(&item.tz) else {
            continue;
        };
        let local = birthdays::local_date(now, tz);
        if !birthdays::local_midnight_window(now, tz)
            || birthdays::birthday_this_year(item.month as u32, item.day as u32, local.year()) != Some(local)
        {
            continue;
        }
        let until = item.role_id.map(|_| now + chrono::Duration::hours(24));
        let Some(paid) = app
            .db
            .claim_birthday(guild, user, local.year() as i16, item.shells_gift, item.badge, until)
            .await?
        else {
            continue;
        };
        if let Some(role) = item.role_id {
            if !role_safe(app, guild, role as u64).await {
                tracing::warn!(guild, user, role, "birthday role is no longer safe to grant");
            } else if let Err(error) = app.db.schedule_removal(guild, user, role as u64, until.unwrap()).await {
                tracing::warn!(guild,user,role,%error,"birthday role timer failed");
            } else if let Err(error) = app
                .http
                .add_guild_member_role(Id::new(guild), Id::new(user), Id::new(role as u64))
                .await
            {
                tracing::warn!(guild,user,role,%error,"birthday role grant failed");
            }
        }
        let lang = app.guild_lang(guild).await;
        let age = birthdays::visible_age(item.year.map(i32::from), item.show_age, item.allow_age, local.year());
        let template = if item.message == birthdays::DEFAULT_MESSAGE {
            t!(lang, "birthdays.default-message")
        } else {
            item.message.clone()
        };
        let message = birthdays::format_message(&template, user, age);
        let name = app.user(user).await.map_or_else(
            || t!(lang, "birthdays.member"),
            |u| u.global_name.clone().unwrap_or_else(|| u.name.clone()),
        );
        let card = BirthdayCard {
            lang,
            name,
            age,
            badge: item.badge,
        };
        let png = app.render(move |r| r.svg_to_png(&card.to_svg())).await?;
        let file = [Attachment::from_bytes("birthday.png".into(), png, 1)];
        let mentions = AllowedMentions {
            users: vec![Id::new(user)],
            ..AllowedMentions::default()
        };
        if let Some(channel) = item.channel_id {
            match app
                .http
                .create_message(Id::new(channel as u64))
                .content(&message)
                .attachments(&file)
                .allowed_mentions(Some(&mentions))
                .await
            {
                Ok(_) => posted += 1,
                Err(error) => tracing::warn!(guild,user,channel,%error,"birthday announcement failed"),
            }
        }
        if paid > 0 {
            tracing::info!(user, paid, "birthday gift paid");
        }
    }
    Ok(posted)
}

pub fn spawn_ticker(app: Arc<App>) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(60));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            interval.tick().await;
            if let Err(error) = tick(&app).await {
                tracing::warn!(%error,"birthday tick failed");
            }
        }
    });
}

pub(crate) async fn preview(cx: &Cx, guild: u64) -> anyhow::Result<()> {
    let s = cx.app.db.birthday_settings(guild).await?;
    let user = cx.author()?.id.get();
    let b = cx.app.db.birthday(user).await?;
    let age =
        b.and_then(|b| birthdays::visible_age(b.year.map(i32::from), b.show_age, s.allow_age, cx.app.now().year()));
    let lang = cx.lang();
    let name = cx.author_display_name();
    let png = cx
        .app
        .render(move |r| {
            r.svg_to_png(
                &BirthdayCard {
                    lang,
                    name,
                    age,
                    badge: s.badge,
                }
                .to_svg(),
            )
        })
        .await?;
    let mut response = Cx::card(
        twilight_util::builder::embed::EmbedBuilder::new().color(crate::util::BRAND),
        "birthday.png",
        png,
        Vec::new(),
        true,
    );
    response.content = Some(t!(cx.lang(), "birthdays.preview"));
    response.allowed_mentions = quiet();
    cx.reply(response).await
}

/// Check live role permissions and hierarchy; a manager can edit the role after setup.
async fn role_safe(app: &App, guild: u64, role: u64) -> bool {
    let Ok(roles) = app.http.roles(Id::new(guild)).await else {
        return false;
    };
    let Ok(roles) = roles.models().await else {
        return false;
    };
    let Ok(me) = app
        .http
        .guild_member(Id::new(guild), Id::new(app.application_id.get()))
        .await
    else {
        return false;
    };
    let Ok(me) = me.model().await else {
        return false;
    };
    let bot_top = roles
        .iter()
        .filter(|r| me.roles.contains(&r.id))
        .map(|r| r.position)
        .max();
    let Some(found) = roles.iter().find(|r| r.id.get() == role) else {
        return false;
    };
    let facts = RoleFacts {
        everyone: role == guild,
        managed: found.managed,
        position: found.position,
        permissions: found.permissions.bits(),
    };
    settings::role_refusal(facts, bot_top, &format!("<@&{role}>")).is_none()
        && settings::run_time_refusal(found.permissions.bits(), &format!("<@&{role}>")).is_none()
}
