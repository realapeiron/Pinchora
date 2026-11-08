//! Durable birthday claims, bounded announcement batches and safe, expiring roles.

use chrono::Datelike;
use pinchora_core::settings::{self, RoleFacts};
use pinchora_core::{birthdays, modules::Module, t};
use pinchora_render::BirthdayCard;
use std::{sync::Arc, time::Duration};
use twilight_model::{channel::message::AllowedMentions, http::attachment::Attachment, id::Id};

use crate::{App, Cx, commands::games::quiet};

pub async fn tick(app: &Arc<App>) -> anyhow::Result<usize> {
    let Some(_dispatch) = app.db.birthday_dispatch_lock().await? else {
        return Ok(0);
    };
    let now = app.now();
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
        // The entire local day is eligible: restarting after midnight must catch up.
        if birthdays::birthday_this_year(item.month as u32, item.day as u32, local.year()) != Some(local) {
            continue;
        }
        // `members` keeps historical XP after a leave. Only current Discord members receive gifts.
        let _member = app.db.lock_role_member(guild, user).await?;
        let Ok(member) = app.http.guild_member(Id::new(guild), Id::new(user)).await else {
            continue;
        };
        let Ok(member) = member.model().await else {
            continue;
        };
        let until = if let Some(role) = item.role_id
            && !member.roles.contains(&Id::new(role as u64))
            && role_safe(app, guild, role as u64).await
        {
            Some((role as u64, now + chrono::Duration::hours(24)))
        } else {
            None
        };
        app.db
            .claim_birthday(guild, user, local.year() as i16, item.shells_gift, item.badge, until)
            .await?;
    }

    let mut pending = Vec::new();
    for row in app.db.birthday_deliveries().await? {
        let item = &row.birthday;
        let (guild, user) = (item.guild_id as u64, item.user_id as u64);
        if !app.module_allowed(guild, None, Module::Birthdays).await {
            continue;
        }
        if app.http.guild_member(Id::new(guild), Id::new(user)).await.is_err() {
            continue;
        }
        if !row.role_done {
            let _member = app.db.lock_role_member(guild, user).await?;
            let guard = app.db.lock_birthday_batch(&[row.delivery_id]).await?;
            if guard.ids.is_empty() {
                continue;
            }
            if let (Some(role), Some(until)) = (item.role_id, row.role_until)
                && until > app.now()
                && role_safe(app, guild, role as u64).await
                && let Err(error) = app
                    .http
                    .add_guild_member_role(Id::new(guild), Id::new(user), Id::new(role as u64))
                    .await
            {
                tracing::warn!(guild,user,role,%error,"birthday role grant failed");
                continue;
            }
            app.db.finish_birthday_role(row.delivery_id).await?;
        }
        let lang = app.guild_lang(guild).await;
        let local_year = birthdays::timezone(&item.tz).map_or(now.year(), |tz| birthdays::local_date(now, tz).year());
        let age = birthdays::visible_age(item.year.map(i32::from), item.show_age, item.allow_age, local_year);
        let template = if item.message == birthdays::DEFAULT_MESSAGE {
            t!(lang, "birthdays.default-message")
        } else {
            item.message.clone()
        };
        let message = birthdays::format_message(&template, user, age);
        pending.push((row, message, age));
    }
    let mut posted = 0;
    // Persist membership of each bounded batch before sending, so retries use the same nonce.
    while !pending.is_empty() {
        let first = pending.remove(0);
        let guild = first.0.birthday.guild_id;
        let channel = first.0.birthday.channel_id;
        let existing = first.0.batch_id;
        let batch_id = existing.unwrap_or(first.0.delivery_id);
        let mut batch = vec![first];
        let mut length = batch[0].1.encode_utf16().count();
        let mut i = 0;
        while i < pending.len() && batch.len() < 25 {
            let candidate = &pending[i];
            let size = candidate.1.encode_utf16().count() + 1;
            if candidate.0.birthday.guild_id == guild
                && candidate.0.birthday.channel_id == channel
                && candidate.0.batch_id == existing
                && (existing.is_some() || length + size <= 1900)
            {
                length += size;
                batch.push(pending.remove(i));
            } else {
                i += 1;
            }
        }
        let ids: Vec<_> = batch.iter().map(|(row, _, _)| row.delivery_id).collect();
        app.db.birthday_batch(&ids, batch_id).await?;
        let Some(channel) = channel else {
            continue;
        };
        let guard = app.db.lock_birthday_batch(&ids).await?;
        batch.retain(|(row, _, _)| guard.ids.contains(&row.delivery_id));
        if batch.is_empty() {
            continue;
        }
        let lang = app.guild_lang(guild as u64).await;
        let name = if batch.len() == 1 {
            app.user(batch[0].0.birthday.user_id as u64).await.map_or_else(
                || t!(lang, "birthdays.member"),
                |u| u.global_name.clone().unwrap_or_else(|| u.name.clone()),
            )
        } else {
            t!(
                lang,
                "birthdays.members",
                count = pinchora_core::server_stats::grouped_in(lang, batch.len() as i64)
            )
        };
        let card = BirthdayCard {
            lang,
            name,
            age: if batch.len() == 1 { batch[0].2 } else { None },
            badge: batch[0].0.birthday.badge,
        };
        let png = app.render(move |r| r.svg_to_png(&card.to_svg())).await?;
        let files = [Attachment::from_bytes("birthday.png".into(), png, 1)];
        let mentions = AllowedMentions {
            users: batch
                .iter()
                .map(|(row, _, _)| Id::new(row.birthday.user_id as u64))
                .collect(),
            ..AllowedMentions::default()
        };
        let budget = (1900 - batch.len()) / batch.len();
        let content = if batch
            .iter()
            .map(|(_, text, _)| text.encode_utf16().count() + 1)
            .sum::<usize>()
            > 1900
        {
            batch
                .iter()
                .map(|(_, text, _)| birthdays::truncate_message(text, budget))
                .collect::<Vec<_>>()
                .join("\n")
        } else {
            batch
                .iter()
                .map(|(_, text, _)| text.as_str())
                .collect::<Vec<_>>()
                .join("\n")
        };
        let payload = serde_json::to_vec(&serde_json::json!({
            "content": content,
            "allowed_mentions": mentions,
            "nonce": format!("bd{batch_id}"), "enforce_nonce": true,
        }))?;
        match app
            .http
            .create_message(Id::new(channel as u64))
            .attachments(&files)
            .payload_json(&payload)
            .await
        {
            Ok(_) => {
                app.db.finish_birthday_batch(&guard.ids).await?;
                for (row, _, _) in &batch {
                    crate::workflows::triggers::simple(
                        app,
                        guild as u64,
                        row.birthday.user_id as u64,
                        "birthday",
                        Some(channel as u64),
                    );
                }
                posted += 1;
            }
            Err(error) => tracing::warn!(guild,channel,%error,"birthday batch will be retried"),
        }
    }
    Ok(posted)
}

/// Keep the dispatcher stopped through data deletion, so no grant can race timer erasure.
pub(crate) async fn before_delete(app: &App, user: u64) -> anyhow::Result<impl Send> {
    let guard = loop {
        if let Some(guard) = app.db.birthday_dispatch_lock().await? {
            break guard;
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    };
    for (guild, role) in app.db.birthday_cleanup_roles(user).await? {
        let _member = app.db.lock_role_member(guild as u64, user).await?;
        if let Err(error) = app
            .http
            .remove_guild_member_role(Id::new(guild as u64), Id::new(user), Id::new(role as u64))
            .await
            && !crate::roles::role_is_gone(&error)
            && !crate::roles::member_is_gone(&error)
        {
            return Err(error.into());
        }
    }
    Ok(guard)
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
