//! Message hooks and the lease-backed sender for sticky and scheduled messages.

use std::{
    collections::HashMap,
    sync::{Arc, LazyLock, Mutex},
    time::Duration,
};

use moka::future::Cache;

use chrono::{DateTime, Utc};
use pinchora_core::{
    i18n::Lang,
    modules::{Access, Module},
    t,
    utility::{Schedule, next_run},
};
use pinchora_db::ScheduledRow;
use twilight_model::{
    channel::{
        Message,
        message::{AllowedMentions, MentionType},
    },
    guild::Permissions,
    id::Id,
};

use crate::{App, custom::scrub_value, text::access::channel_permissions};

/// Share the bot's mention and bilingual profanity filters; AFK text is always plain text.
pub(crate) fn afk_plain(text: &str) -> Option<String> {
    use pinchora_game::party::filter::is_profane_for;
    let safe = |value: &str| !is_profane_for(value, false, Lang::En) && !is_profane_for(value, false, Lang::Tr);
    if !safe(text) {
        return None;
    }
    let plain = crate::util::strip_mentions(text);
    let plain: String = plain
        .chars()
        .filter(|c| {
            !c.is_control()
                && !matches!(c, '@' | '*' | '_' | '~' | '|' | '`' | '>' | '<' | '\\' | '#' | '[' | ']' | '(' | ')'
            | '\u{200b}'..='\u{200f}' | '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}')
        })
        .collect();
    // Normalisation can reveal a link hidden with formatting/invisible characters.
    let plain = plain
        .split_whitespace()
        .filter(|word| {
            let lower = word.to_ascii_lowercase();
            !lower.contains("://") && !lower.trim_matches(|c: char| !c.is_alphanumeric()).contains('.')
        })
        .collect::<Vec<_>>()
        .join(" ");
    let plain: String = plain.trim().chars().take(100).collect();
    (!plain.is_empty() && safe(&plain)).then_some(plain)
}

static AFK_PRESENT: LazyLock<Cache<(u64, u64, i64), bool>> = LazyLock::new(|| {
    Cache::builder()
        .max_capacity(100_000)
        .time_to_live(Duration::from_secs(5))
        .build()
});
static STICKY_PRESENT: LazyLock<Cache<u64, bool>> = LazyLock::new(|| {
    Cache::builder()
        .max_capacity(50_000)
        .time_to_live(Duration::from_secs(5))
        .build()
});
type ReplyGate = Arc<tokio::sync::Mutex<Option<std::time::Instant>>>;
static AFK_REPLY: LazyLock<Cache<u64, ReplyGate>> = LazyLock::new(|| {
    Cache::builder()
        .max_capacity(50_000)
        .time_to_live(Duration::from_secs(300))
        .build()
});

async fn notice_allowed(app: &App, channel: u64) -> anyhow::Result<bool> {
    let gate = AFK_REPLY
        .get_with(channel, async { Arc::new(tokio::sync::Mutex::new(None)) })
        .await;
    let mut next = gate.lock().await;
    if next.is_some_and(|at| at > std::time::Instant::now()) {
        return Ok(false);
    }
    let claimed = app.db.claim_afk_reply(channel).await?;
    *next = Some(std::time::Instant::now() + Duration::from_secs(if claimed { 300 } else { 5 }));
    Ok(claimed)
}

static COUNTS: LazyLock<Mutex<HashMap<(u64, u64), i32>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

pub(crate) async fn forget_afk(app: &App, user: u64) -> anyhow::Result<()> {
    // A generation shared by pods invalidates negative entries after /afk, including global AFK.
    app.cache
        .incr(&format!("utility:afk:{user}"), Duration::from_secs(60))
        .await?;
    Ok(())
}

async fn afk_present(app: &App, guild: u64, user: u64) -> anyhow::Result<bool> {
    let load = async { Ok::<_, anyhow::Error>(!app.db.afk_for(guild, &[user as i64]).await?.is_empty()) };
    let Ok(generation) = app.cache.counter(&format!("utility:afk:{user}")).await else {
        // Never trust a negative cache while cross-pod invalidation is unavailable.
        return load.await;
    };
    AFK_PRESENT
        .try_get_with((guild, user, generation), load)
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))
}

async fn allowed(app: &App, guild: u64, channel: u64) -> bool {
    matches!(
        app.module_access(guild, Some(channel), Module::Utility).await,
        Ok(Access::Allowed)
    )
}

fn count_message(guild: u64, channel: u64) {
    let mut counts = COUNTS.lock().expect("utility counts");
    // Bound memory even during a DB outage; losing counts only delays a sticky.
    if counts.len() < 50_000 || counts.contains_key(&(guild, channel)) {
        let count = counts.entry((guild, channel)).or_default();
        *count = (*count + 1).min(1000);
    }
}

async fn post_channel(app: &App, guild: u64, channel: u64) -> anyhow::Result<twilight_model::channel::Channel> {
    match app.http.channel(Id::new(channel)).await {
        Ok(response) => {
            let target: twilight_model::channel::Channel = response.model().await?;
            if let Some(owner) = target.guild_id {
                anyhow::ensure!(owner.get() == guild, "channel belongs to another server");
                return Ok(target);
            }
            // Some responses omit guild_id: verify membership against the guild list.
        }
        Err(error) if matches!(crate::roles::error_code(&error), Some(10003 | 50001 | 50013)) => {
            return Err(error.into());
        }
        Err(_) => {}
    }
    app.http
        .guild_channels(Id::new(guild))
        .await?
        .models()
        .await?
        .into_iter()
        .find(|item| item.id.get() == channel)
        .ok_or_else(|| anyhow::anyhow!("channel not found"))
}

pub(crate) async fn after_message(app: &Arc<App>, msg: &Message) {
    let Some(guild) = msg.guild_id.map(|id| id.get()) else {
        return;
    };
    if msg.author.bot || msg.webhook_id.is_some() || !allowed(app, guild, msg.channel_id.get()).await {
        return;
    }
    let app = Arc::clone(app);
    let message = msg.clone();
    app.clone().spawn(async move {
        if let Err(error) = handle_message(&app, guild, &message).await {
            tracing::warn!(%error,"utility message hook failed");
        }
    });
}

async fn handle_message(app: &App, guild: u64, msg: &Message) -> anyhow::Result<()> {
    let channel = msg.channel_id.get();
    if !allowed(app, guild, channel).await {
        return Ok(());
    }
    let author = msg.author.id.get();
    let sent = DateTime::<Utc>::from_timestamp_micros(msg.timestamp.as_micros()).unwrap_or_else(Utc::now);
    let cleared = if afk_present(app, guild, author).await? {
        let rows = app.db.clear_afk(guild, author, sent).await?;
        forget_afk(app, author).await?;
        rows
    } else {
        Vec::new()
    };
    if !cleared.is_empty() {
        for _row in cleared.iter().filter(|r| r.nick_prefixed && r.guild_id != 0) {
            if let Ok(member) = app.http.guild_member(Id::new(guild), Id::new(author)).await
                && let Ok(member) = member.model().await
                && let Some(nick) = member.nick.and_then(|n| n.strip_prefix("[AFK] ").map(str::to_owned))
            {
                let _ = app
                    .http
                    .update_guild_member(Id::new(guild), Id::new(author))
                    .nick(Some(&nick))
                    .await;
            }
        }
        let lang = app.guild_lang(guild).await;
        let none = AllowedMentions::default();
        if notice_allowed(app, channel).await?
            && let Ok(response) = app
                .http
                .create_message(Id::new(channel))
                .content(&t!(lang, "utility.afk-back"))
                .allowed_mentions(Some(&none))
                .await
            && let Ok(copy) = response.model().await
        {
            app.delete_later(Id::new(channel), copy.id, Duration::from_secs(10));
        }
    }
    let candidates: Vec<i64> = msg
        .mentions
        .iter()
        .map(|u| u.id.get() as i64)
        .filter(|u| *u != author as i64)
        .collect();
    let mut mentioned = Vec::new();
    for user in candidates.into_iter().take(20) {
        if afk_present(app, guild, user as u64).await? {
            mentioned.push(user);
        }
    }
    if !mentioned.is_empty() && notice_allowed(app, channel).await? {
        let away = app.db.afk_for(guild, &mentioned).await?;
        if !away.is_empty() {
            let lang = app.guild_lang(guild).await;
            let names = away
                .iter()
                .map(|r| {
                    scrub_value(
                        &afk_plain(
                            &msg.mentions
                                .iter()
                                .find(|m| m.id.get() as i64 == r.user_id)
                                .map_or_else(|| r.user_id.to_string(), |m| m.name.clone()),
                        )
                        .unwrap_or_else(|| t!(lang, "utility.afk-member")),
                    )
                })
                .collect::<Vec<_>>()
                .join(", ");
            let reasons = away
                .iter()
                .map(|r| scrub_value(&afk_plain(&r.message).unwrap_or_else(|| t!(lang, "utility.afk-default"))))
                .collect::<Vec<_>>()
                .join(" · ");
            let text = t!(
                lang,
                "utility.afk-mention",
                names = names,
                verb = t!(
                    lang,
                    if away.len() == 1 {
                        "utility.afk-is"
                    } else {
                        "utility.afk-are"
                    }
                ),
                reasons = reasons
            );
            let none = AllowedMentions::default();
            let _ = app
                .http
                .create_message(Id::new(channel))
                .content(&text.chars().take(1900).collect::<String>())
                .allowed_mentions(Some(&none))
                .await;
        }
    }
    if STICKY_PRESENT
        .try_get_with(channel, app.db.utility_sticky_enabled(channel))
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?
    {
        count_message(guild, channel);
    }
    Ok(())
}

async fn live_permissions(app: &App, guild: u64, channel: u64, user: u64) -> anyhow::Result<Permissions> {
    let roles = app.http.roles(Id::new(guild)).await?.models().await?;
    let member = app
        .http
        .guild_member(Id::new(guild), Id::new(user))
        .await?
        .model()
        .await?;
    let held: Vec<u64> = member.roles.iter().map(|r| r.get()).collect();
    let base = roles
        .iter()
        .filter(|r| r.id.get() == guild || held.contains(&r.id.get()))
        .fold(Permissions::empty(), |p, r| p | r.permissions);
    let target = post_channel(app, guild, channel).await?;
    Ok(channel_permissions(
        base,
        guild,
        user,
        &held,
        target.permission_overwrites.as_deref().unwrap_or(&[]),
    ))
}

pub async fn bot_permissions(app: &App, guild: u64, channel: u64) -> anyhow::Result<Permissions> {
    live_permissions(app, guild, channel, app.application_id.get()).await
}

pub async fn creator_may_ping(app: &App, guild: u64, channel: u64, user: u64) -> bool {
    let result = async {
        if app.http.guild(Id::new(guild)).await?.model().await?.owner_id.get() == user {
            return anyhow::Ok(true);
        }
        let perms = live_permissions(app, guild, channel, user).await?;
        anyhow::Ok(perms.contains(Permissions::MENTION_EVERYONE))
    }
    .await;
    result.unwrap_or(false)
}

async fn post(
    app: &App,
    guild: u64,
    channel: u64,
    content: &str,
    embed: Option<&serde_json::Value>,
    ping: bool,
) -> anyhow::Result<u64> {
    let permissions = bot_permissions(app, guild, channel).await?;
    anyhow::ensure!(
        permissions.contains(Permissions::SEND_MESSAGES | Permissions::VIEW_CHANNEL),
        "missing Send Messages"
    );
    let broad = ping && permissions.contains(Permissions::MENTION_EVERYONE);
    let mentions = AllowedMentions {
        parse: if broad {
            vec![MentionType::Everyone, MentionType::Roles]
        } else {
            Vec::new()
        },
        ..Default::default()
    };
    let text = if broad {
        content.to_owned()
    } else {
        content
            .replace("@everyone", "@\u{200B}everyone")
            .replace("@here", "@\u{200B}here")
    };
    let embeds = embed
        .and_then(|v| serde_json::from_value::<pinchora_core::tags::EmbedSpec>(v.clone()).ok())
        .map(|e| vec![crate::custom::embed_of(&e)]);
    let mut request = app
        .http
        .create_message(Id::new(channel))
        .content(&text)
        .allowed_mentions(Some(&mentions));
    if let Some(ref embeds) = embeds {
        request = request.embeds(embeds);
    }
    Ok(request.await?.model().await?.id.get())
}

pub async fn send_scheduled(app: &App, row: &ScheduledRow) -> anyhow::Result<u64> {
    let guild = row.guild_id as u64;
    let channel = row.channel_id as u64;
    anyhow::ensure!(allowed(app, guild, channel).await, "module disabled");
    // Privacy deletion may have anonymised the creator since this worker claimed the row.
    let current = app.db.schedule(guild, row.id).await?;
    let creator = current.as_ref().and_then(|r| r.created_by);
    let ping = row.ping_allowed
        && if let Some(actor) = creator {
            creator_may_ping(app, guild, channel, actor as u64).await
        } else {
            false
        };
    post(app, guild, channel, &row.content, row.embed.as_ref(), ping).await
}

pub fn spawn_ticker(app: Arc<App>) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(5));
        loop {
            interval.tick().await;
            if let Err(error) = tick(&app).await {
                tracing::warn!(%error,"utility ticker failed");
            }
        }
    });
}

pub async fn tick(app: &App) -> anyhow::Result<()> {
    let counts = std::mem::take(&mut *COUNTS.lock().expect("utility counts"));
    for ((guild, channel), count) in counts {
        if allowed(app, guild, channel).await {
            app.db.count_sticky_messages(channel, count).await?;
        }
    }
    for (channel, copy) in app.db.sticky_cleanups(30).await? {
        match app
            .http
            .delete_message(Id::new(channel as u64), Id::new(copy as u64))
            .await
        {
            Ok(_) => app.db.finish_sticky_cleanup(channel as u64, copy as u64).await?,
            Err(error) if matches!(crate::roles::error_code(&error), Some(10008 | 10003)) => {
                app.db.finish_sticky_cleanup(channel as u64, copy as u64).await?
            }
            Err(error) => tracing::warn!(%error,channel,copy,"sticky cleanup will retry"),
        }
    }
    for row in app.db.claim_stickies(20).await? {
        let channel = row.channel_id as u64;
        if !allowed(app, row.guild_id as u64, channel).await {
            app.db.release_sticky(&row).await?;
            continue;
        }
        if !app.db.begin_sticky(&row).await? {
            continue;
        }
        let result = async {
            let permissions = bot_permissions(app, row.guild_id as u64, channel).await?;
            anyhow::ensure!(
                permissions.contains(Permissions::MANAGE_MESSAGES),
                "missing Manage Messages"
            );
            let copy = post(
                app,
                row.guild_id as u64,
                channel,
                &row.content,
                row.embed.as_ref(),
                false,
            )
            .await?;
            app.db.finish_sticky(&row, copy).await?;
            anyhow::Ok(())
        }
        .await;
        if let Err(error) = result {
            tracing::warn!(%error,channel,"sticky post failed");
            app.db.pause_sticky(&row, failure_reason(&error)).await?;
        }
    }
    for row in app.db.claim_schedules(20).await? {
        if !allowed(app, row.guild_id as u64, row.channel_id as u64).await {
            app.db.release_schedule(&row).await?;
            continue;
        }
        let Ok(rule) = serde_json::from_value::<Schedule>(row.schedule.clone()) else {
            app.db.pause_schedule(&row, "invalid_schedule").await?;
            continue;
        };
        if !app.db.begin_schedule(&row).await? {
            continue;
        }
        match send_scheduled(app, &row).await {
            Ok(_) => {
                let next = next_run(&rule, &row.tz, Utc::now());
                app.db.finish_schedule(&row, next).await?;
            }
            Err(error) => {
                tracing::warn!(%error,id=row.id,"scheduled message failed");
                app.db.pause_schedule(&row, failure_reason(&error)).await?;
            }
        }
    }
    Ok(())
}

pub(crate) fn failure_reason(error: &anyhow::Error) -> &'static str {
    if let Some(http) = error.downcast_ref::<twilight_http::Error>() {
        match crate::roles::error_code(http) {
            Some(10003) => return "channel_deleted",
            Some(50001 | 50013) => return "permissions_lost",
            _ => {}
        }
    }
    if error.to_string().starts_with("missing ") {
        "permissions_lost"
    } else if error.to_string() == "channel not found" {
        "channel_deleted"
    } else {
        "delivery_uncertain"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn afk_filters_untrusted_reasons_and_names() {
        for text in [
            "shithead",
            "f*ck",
            "nazi",
            "https://evil.test",
            "https:\u{200b}//host",
            "https:**//host",
            "example.com",
            "<@&123>",
            "\u{202e}",
        ] {
            assert!(afk_plain(text).is_none(), "{text}");
        }
        let value = afk_plain("**Lunch** <@123> @eve**ryone https://evil.test").unwrap();
        assert!(!value.contains(['*', '@', '<']));
        assert!(!value.contains("evil.test"));
        assert!(!scrub_value(&afk_plain("[hello](x)").unwrap()).starts_with("[hello]"));
    }

    #[test]
    fn storm_counts_are_bounded_and_batched() {
        for _ in 0..10_000 {
            count_message(u64::MAX, u64::MAX);
        }
        assert_eq!(COUNTS.lock().unwrap().remove(&(u64::MAX, u64::MAX)), Some(1000));
    }
}
