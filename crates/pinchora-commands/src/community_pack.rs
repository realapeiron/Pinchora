//! Gateway hooks and daily delivery for the community pack.
use crate::App;
use chrono::Timelike;
use pinchora_core::{community::CountResult, i18n::Lang, modules::Module, t};
use std::{sync::Arc, time::Duration};
use twilight_http::request::channel::reaction::RequestReactionType;
use twilight_model::{
    channel::Message,
    channel::message::{
        AllowedMentions, Component,
        component::{ActionRow, SelectMenu, SelectMenuOption, SelectMenuType},
    },
    id::Id,
};

pub(crate) async fn on_guild_message(app: &Arc<App>, msg: &Message) {
    let Some(guild) = msg.guild_id.map(|id| id.get()) else {
        return;
    };
    let channel = msg.channel_id.get();
    if app.message_content && app.module_allowed(guild, Some(channel), Module::Counting).await {
        match app.db.count_message(channel, msg.author.id.get(), &msg.content).await {
            Ok(Some((result, _, _))) if result != CountResult::Ignored => {
                let name = if result == CountResult::Correct { "✅" } else { "❌" };
                let _ = app
                    .http
                    .create_reaction(msg.channel_id, msg.id, &RequestReactionType::Unicode { name })
                    .await;
                if result != CountResult::Correct
                    && let Ok(Some(counting)) = app.db.counting_channel(channel).await
                    && let Some(role) = counting.fail_role_id
                {
                    let _ = crate::commands::roles_pack::give_safe_role(
                        app,
                        guild,
                        msg.author.id.get(),
                        role as u64,
                        "Counting failed",
                    )
                    .await;
                }
            }
            Err(error) => tracing::warn!(%error,guild,"counting failed"),
            _ => {}
        }
    }
    if !app.module_allowed(guild, Some(channel), Module::Modmail).await {
        return;
    }
    let Ok(Some(thread)) = app.db.modmail_thread(channel).await else {
        return;
    };
    let Ok(Some(settings)) = app.db.modmail_settings(guild).await else {
        return;
    };
    let staff_role = msg
        .member
        .as_ref()
        .is_some_and(|m| m.roles.iter().any(|r| settings.staff_roles.contains(&(r.get() as i64))));
    let owner = app
        .guild_facts(guild)
        .await
        .is_some_and(|facts| facts.owner == msg.author.id.get());
    let manager = if staff_role || owner {
        true
    } else if let Ok(response) = app.http.roles(Id::new(guild)).await {
        response.models().await.is_ok_and(|roles| {
            msg.member.as_ref().is_some_and(|member| {
                roles.iter().any(|role| {
                    (role.id.get() == guild || member.roles.contains(&role.id))
                        && role.permissions.intersects(
                            twilight_model::guild::Permissions::ADMINISTRATOR
                                | twilight_model::guild::Permissions::MANAGE_GUILD,
                        )
                })
            })
        })
    } else {
        false
    };
    if !settings.enabled || !(staff_role || manager) {
        return;
    }
    let body = relay_body(msg);
    if body.is_empty() {
        return;
    }
    let text = if settings.anonymous_staff {
        body
    } else {
        format!("{}: {body}", msg.author.name)
    };
    match app.http.create_private_channel(Id::new(thread.user_id as u64)).await {
        Ok(response) => {
            if let Ok(dm) = response.model().await {
                let mentions = AllowedMentions::default();
                if let Err(error) = app
                    .http
                    .create_message(dm.id)
                    .content(&text)
                    .allowed_mentions(Some(&mentions))
                    .await
                {
                    tracing::warn!(%error,guild,"modmail staff reply failed");
                }
            }
        }
        Err(error) => tracing::warn!(%error,guild,"modmail DM open failed"),
    }
}

pub(crate) async fn on_dm(app: &Arc<App>, msg: &Message) {
    let user = msg.author.id.get();
    let lang = app.user_lang_choice(user).await.unwrap_or_default();
    let body = relay_body(msg);
    let text = body.trim();
    if text.is_empty() {
        return;
    }
    if !matches!(
        app.cache
            .try_cooldown(&format!("community:dm:{user}"), Duration::from_secs(5))
            .await,
        Ok(None)
    ) {
        return;
    }
    let candidates = match app.db.modmail_enabled_guilds().await {
        Ok(guilds) => guilds,
        Err(error) => {
            tracing::warn!(%error,"modmail guild lookup failed");
            return;
        }
    };
    let mut shared = Vec::new();
    for guild in candidates {
        if guild <= 0 {
            continue;
        }
        let guild = guild as u64;
        if !app.module_allowed(guild, None, Module::Modmail).await
            || !matches!(app.db.modmail_blocked(guild, user).await, Ok(false))
        {
            continue;
        }
        if app.http.guild_member(Id::new(guild), msg.author.id).await.is_ok() {
            shared.push(guild);
        }
    }
    match pinchora_core::community::mail_route(&shared) {
        pinchora_core::community::MailRoute::None => {
            let _ = app
                .http
                .create_message(msg.channel_id)
                .content(&t!(lang, "community-pack.modmail-no-server"))
                .await;
        }
        pinchora_core::community::MailRoute::One(guild) => {
            if let Err(error) = route_dm(app, guild, user, text).await {
                tracing::warn!(%error,guild,"modmail route failed");
            }
        }
        pinchora_core::community::MailRoute::Choose => {
            if let Err(error)=sqlx::query("INSERT INTO modmail_pending(user_id,body) VALUES($1,$2) ON CONFLICT(user_id) DO UPDATE SET body=$2,created_at=now()")
                .bind(user as i64).bind(text).execute(app.db.pool()).await {
                tracing::warn!(%error,"modmail pending save failed");return
            }
            let mut options = Vec::new();
            for guild in shared.into_iter().take(25) {
                let name = match app.http.guild(Id::new(guild)).await {
                    Ok(response) => response.model().await.ok(),
                    Err(_) => None,
                };
                options.push(SelectMenuOption {
                    default: false,
                    description: None,
                    emoji: None,
                    label: name.map_or_else(|| guild.to_string(), |g| g.name),
                    value: guild.to_string(),
                });
            }
            let component = Component::ActionRow(ActionRow {
                id: None,
                components: vec![Component::SelectMenu(SelectMenu {
                    channel_types: None,
                    custom_id: "mml:g".into(),
                    default_values: None,
                    disabled: false,
                    id: None,
                    kind: SelectMenuType::Text,
                    max_values: Some(1),
                    min_values: Some(1),
                    options: Some(options),
                    placeholder: Some(t!(lang, "community-pack.modmail-choose")),
                    required: None,
                })],
            });
            let _ = app
                .http
                .create_message(msg.channel_id)
                .content(&t!(lang, "community-pack.modmail-choose"))
                .components(&[component])
                .await;
        }
    }
}
fn relay_body(msg: &Message) -> String {
    let mut body = msg.content.trim().chars().take(1500).collect::<String>();
    for attachment in msg.attachments.iter().take(3) {
        if !body.is_empty() {
            body.push('\n')
        }
        body.push_str(&attachment.url);
    }
    body.chars().take(1800).collect()
}

pub(crate) async fn route_dm(app: &Arc<App>, guild: u64, user: u64, body: &str) -> anyhow::Result<bool> {
    if guild == 0 || user == 0 || body.trim().is_empty() {
        return Ok(false);
    }
    if !app.module_allowed(guild, None, Module::Modmail).await {
        return Ok(false);
    }
    if app.db.modmail_blocked(guild, user).await? {
        return Ok(false);
    }
    let Some(settings) = app.db.modmail_settings(guild).await?.filter(|s| s.enabled) else {
        return Ok(false);
    };
    if app.http.guild_member(Id::new(guild), Id::new(user)).await.is_err() {
        return Ok(false);
    }
    if app
        .cache
        .try_cooldown(&format!("community:mail-delivery:{user}"), Duration::from_secs(5))
        .await?
        .is_some()
    {
        return Ok(false);
    }
    let Some(forum) = settings.forum_channel_id else {
        return Ok(false);
    };
    let guild_lang = app.guild_lang(guild).await;
    let member_lang = crate::lang::resolve_text(app, user, guild).await;
    // Hold a cross-pod lock until Discord creation and persistence finish. A cooldown also
    // prevents repeated orphan threads after an ambiguous HTTP/DB failure or process restart.
    let mut lock = app.db.pool().begin().await?;
    let acquired: bool = sqlx::query_scalar("SELECT pg_try_advisory_xact_lock(hashtextextended($1,0))")
        .bind(format!("community:mail:{guild}:{user}"))
        .fetch_one(&mut *lock)
        .await?;
    if !acquired {
        return Ok(false);
    }
    let mut text = body.chars().take(1800).collect::<String>();
    let existing: Option<pinchora_db::ModmailThread> =
        sqlx::query_as("SELECT * FROM modmail_threads WHERE guild_id=$1 AND user_id=$2 AND status='open'")
            .bind(guild as i64)
            .bind(user as i64)
            .fetch_optional(&mut *lock)
            .await?;
    let mut created = false;
    if let Some(thread) = existing {
        let mentions = AllowedMentions::default();
        app.http
            .create_message(Id::new(thread.thread_id as u64))
            .content(&text)
            .allowed_mentions(Some(&mentions))
            .await?;
    } else {
        if app
            .cache
            .try_cooldown(&format!("community:mail-create:{user}"), Duration::from_secs(600))
            .await?
            .is_some()
        {
            return Ok(false);
        }
        let name = t!(guild_lang, "community-pack.modmail-thread-title", user = user);
        text = format!("<@{user}>: {text}");
        let mentions = AllowedMentions::default();
        let response = app
            .http
            .create_forum_thread(Id::new(forum as u64), &name)
            .message()
            .content(&text)
            .allowed_mentions(Some(&mentions))
            .await?;
        let thread = response.model().await?;
        sqlx::query("INSERT INTO modmail_threads(guild_id,user_id,thread_id) VALUES($1,$2,$3)")
            .bind(guild as i64)
            .bind(user as i64)
            .bind(thread.channel.id.get() as i64)
            .execute(&mut *lock)
            .await?;
        created = true;
    }
    lock.commit().await?;
    if created {
        let dm = app.http.create_private_channel(Id::new(user)).await?.model().await?;
        let greeting = if settings.greeting.is_empty() {
            t!(member_lang, "community-pack.modmail-greeting")
        } else {
            settings.greeting
        };
        let _ = app.http.create_message(dm.id).content(&greeting).await;
    }
    Ok(true)
}
pub async fn close_thread(
    app: &Arc<App>,
    guild: u64,
    thread_id: u64,
    actor: u64,
    reason: Option<&str>,
) -> anyhow::Result<bool> {
    let Some(thread) = app
        .db
        .modmail_thread(thread_id)
        .await?
        .filter(|r| r.guild_id == guild as i64)
    else {
        return Ok(false);
    };
    if !app.db.modmail_close(guild, thread_id, actor).await? {
        return Ok(false);
    }
    let lang = crate::lang::resolve_text(app, thread.user_id as u64, guild).await;
    let message = reason.filter(|s| !s.trim().is_empty()).unwrap_or("");
    let dm_text = if message.is_empty() {
        t!(lang, "community-pack.modmail-closed")
    } else {
        t!(
            lang,
            "community-pack.modmail-closed-reason",
            reason = message.chars().take(300).collect::<String>()
        )
    };
    if let Ok(dm) = app.http.create_private_channel(Id::new(thread.user_id as u64)).await
        && let Ok(dm) = dm.model().await
    {
        let _ = app.http.create_message(dm.id).content(&dm_text).await;
    }
    if let Err(error) = app.http.update_thread(Id::new(thread_id)).archived(true).await {
        tracing::warn!(%error,guild,"modmail archive failed");
    }
    if let Some(settings) = app.db.modmail_settings(guild).await?
        && let Some(log) = settings.log_channel_id
    {
        let log_text = t!(
            app.guild_lang(guild).await,
            "community-pack.modmail-log",
            thread = thread.id,
            user = thread.user_id,
            actor = actor
        );
        let _ = app.http.create_message(Id::new(log as u64)).content(&log_text).await;
    }
    Ok(true)
}

pub async fn tick(app: &Arc<App>) -> anyhow::Result<usize> {
    sqlx::query("DELETE FROM modmail_pending WHERE created_at <= now()-INTERVAL '15 minutes'")
        .execute(app.db.pool())
        .await?;
    let stale:Vec<(i64,i64)>=sqlx::query_as(
        "SELECT t.guild_id,t.thread_id FROM modmail_threads t JOIN modmail_settings s ON s.guild_id=t.guild_id WHERE t.status='open' AND t.opened_at < now() - make_interval(hours=>s.close_after_hours) LIMIT 100")
        .fetch_all(app.db.pool()).await?;
    for (guild, thread) in stale {
        if guild > 0 && thread > 0 {
            let _ = close_thread(app, guild as u64, thread as u64, app.application_id.get(), None).await;
        }
    }
    type QotdTickSettings = (
        i64,
        Option<i64>,
        Option<i64>,
        i16,
        String,
        bool,
        Option<chrono::NaiveDate>,
    );
    let settings:Vec<QotdTickSettings>=sqlx::query_as(
        "SELECT guild_id,channel_id,ping_role_id,hour,timezone,thread_per_question,last_posted_on FROM qotd_settings WHERE channel_id IS NOT NULL")
        .fetch_all(app.db.pool()).await?;
    let mut posted = 0;
    for (guild, channel, role, hour, zone, thread, last_date) in settings {
        if guild <= 0 {
            continue;
        }
        let Ok(tz) = zone.parse::<chrono_tz::Tz>() else {
            continue;
        };
        let now = app.now().with_timezone(&tz);
        if now.hour() < hour as u32 || last_date == Some(now.date_naive()) {
            continue;
        }
        let guild = guild as u64;
        if !app
            .module_allowed(guild, channel.map(|id| id as u64), Module::Qotd)
            .await
        {
            continue;
        }
        let Some(question) = app.db.claim_qotd(guild, now.date_naive()).await? else {
            continue;
        };
        let Some(channel) = channel else { continue };
        let question_text = if app.guild_lang(guild).await == Lang::Tr {
            question.text_tr.unwrap_or(question.text)
        } else {
            question.text
        };
        let content = match role {
            Some(role) => format!("<@&{role}> {question_text}"),
            None => question_text,
        };
        let mentions = AllowedMentions {
            roles: role.map(|id| vec![Id::new(id as u64)]).unwrap_or_default(),
            ..Default::default()
        };
        let sent = app
            .http
            .create_message(Id::new(channel as u64))
            .content(&content)
            .allowed_mentions(Some(&mentions))
            .await;
        match sent {
            Ok(response) => {
                posted += 1;
                if thread && let Ok(message) = response.model().await {
                    let title = content.chars().take(80).collect::<String>();
                    let _ = app
                        .http
                        .create_thread_from_message(Id::new(channel as u64), message.id, &title)
                        .await;
                }
            }
            Err(error) => {
                tracing::warn!(%error,guild,"QOTD post failed");
                // A failed response may follow a successful Discord send. Retain the durable
                // claim: at-most-once delivery is safer than repeating a daily ping after restart.
            }
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
                tracing::warn!(%error,"QOTD tick failed");
            }
        }
    });
}
