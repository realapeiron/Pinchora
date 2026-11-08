//! Optional XP from reactions and leave reset. Hook failures never hold gateway events.

use std::{sync::Arc, time::Duration};

use pinchora_core::{
    leveling::{scale, week_start},
    modules::Module,
};
use twilight_model::gateway::payload::incoming::MemberRemove;

use crate::{
    App,
    hooks::ReactionEvent,
    leveling::{self, LevelUp},
};
use pinchora_core::t;
use twilight_model::id::Id;

pub async fn post_weekly_highlights(app: &App) -> anyhow::Result<usize> {
    let week = week_start(app.now()) - chrono::Duration::weeks(1);
    let mut posted = 0;
    for (guild, channel) in app.db.due_leveling_highlights(week).await? {
        if !app.db.claim_leveling_highlight(guild, week).await? {
            continue;
        }
        let result = async {
            let rows = app.db.public_level_board(guild, Some(week), 5).await?;
            let lang = app.guild_lang(guild).await;
            let mut content = t!(lang, "xp.weekly-highlights", week = week);
            if rows.is_empty() {
                content.push_str(&t!(lang, "xp.weekly-empty"));
            }
            for (index, row) in rows.iter().enumerate() {
                content.push_str(&format!("\n{}. <@{}> — {} XP", index + 1, row.user_id, row.value));
            }
            let payload = serde_json::json!({"content": content, "allowed_mentions": {"parse": []}});
            app.http
                .create_message(Id::new(channel))
                .payload_json(&serde_json::to_vec(&payload)?)
                .await?;
            anyhow::Ok(())
        }
        .await;
        match result {
            Ok(()) => posted += 1,
            Err(error) => {
                tracing::warn!(%error, guild, channel, "weekly highlights will retry");
                app.db.retry_leveling_highlight(guild, week).await?;
            }
        }
    }
    Ok(posted)
}

pub fn spawn_ticker(app: Arc<App>) {
    tokio::spawn(async move {
        let mut wake = tokio::time::interval(Duration::from_secs(600));
        loop {
            wake.tick().await;
            if let Err(error) = post_weekly_highlights(&app).await {
                tracing::warn!(%error, "leveling highlights tick failed");
            }
        }
    });
}

pub(crate) fn on_reaction(app: &Arc<App>, event: &ReactionEvent) {
    if let ReactionEvent::Add(added) = event {
        let app = app.clone();
        let added = added.clone();
        app.clone().spawn(async move {
            if let Err(error) = reaction(&app, &added).await {
                tracing::warn!(%error, "reaction XP failed");
            }
        });
    }
}

async fn reaction(
    app: &Arc<App>,
    added: &twilight_model::gateway::payload::incoming::ReactionAdd,
) -> anyhow::Result<()> {
    let Some(guild) = added.guild_id.map(|id| id.get()) else {
        return Ok(());
    };
    let user = added.user_id.get();
    if user == app.application_id.get() || added.member.as_ref().is_some_and(|m| m.user.bot) {
        return Ok(());
    }
    let settings = app.guild_settings(guild).await?;
    let parity = app.leveling_parity(guild).await?;
    if !settings.leveling_enabled
        || !parity.reactions
        || !app
            .module_allowed(guild, Some(added.channel_id.get()), Module::Leveling)
            .await
    {
        return Ok(());
    }
    let rules = app.xp_rules(guild).await?;
    let lineage = app.xp_lineage(&rules, added.channel_id.get()).await;
    let roles: Vec<u64> = added
        .member
        .as_ref()
        .map(|m| m.roles.iter().map(|r| r.get()).collect())
        .unwrap_or_default();
    let xp = scale(parity.reaction_xp, rules.multiplier_in(&lineage, &roles));
    if xp == 0 {
        return Ok(());
    }
    // A member can earn from each message once per day, regardless of emoji or remove/re-add.
    let key = format!("xp:reaction:{guild}:{user}:{}", added.message_id.get());
    if app
        .cache
        .try_cooldown(&key, Duration::from_secs(86_400))
        .await?
        .is_some()
    {
        return Ok(());
    }
    if app
        .cache
        .try_cooldown(&format!("xp:reaction:minute:{guild}:{user}"), Duration::from_secs(60))
        .await?
        .is_some()
    {
        return Ok(());
    }
    let gain = app.db.add_event_xp(guild, user, xp, week_start(app.now())).await?;
    let before = parity.level_of(gain.old_xp);
    let after = parity.level_of(gain.new_xp);
    if after > before {
        leveling::level_up(
            app,
            &settings,
            LevelUp {
                guild,
                user,
                before,
                after,
                author: added.member.as_ref().map(|m| &m.user),
                nick: added.member.as_ref().and_then(|m| m.nick.as_deref()),
                roles: &roles,
                here: added.channel_id,
            },
        )
        .await?;
    }
    Ok(())
}

pub(crate) fn on_member_leave(app: &Arc<App>, event: &MemberRemove) {
    if event.user.bot {
        return;
    }
    let (guild, user) = (event.guild_id.get(), event.user.id.get());
    let app = app.clone();
    app.clone().spawn(async move {
        let result = async {
            if app.leveling_parity(guild).await?.reset_on_leave {
                app.db.reset_departed_level(guild, user).await?;
            }
            anyhow::Ok(())
        }
        .await;
        if let Err(error) = result {
            tracing::warn!(%error, guild, user, "leave XP reset failed");
        }
    });
}

/// Only a first cast on one poll or suggestion earns XP. Switching or withdrawing never pays.
pub(crate) async fn award_vote(
    app: &Arc<App>,
    guild: u64,
    user: u64,
    channel: u64,
    roles: &[u64],
    kind: &str,
    id: i64,
) {
    let result = async {
        let settings = app.guild_settings(guild).await?;
        let parity = app.leveling_parity(guild).await?;
        if !settings.leveling_enabled
            || !parity.votes
            || !app.module_allowed(guild, Some(channel), Module::Leveling).await
        {
            return anyhow::Ok(());
        }
        let key = format!("xp:vote:{guild}:{user}:{kind}:{id}");
        if app
            .cache
            .try_cooldown(&key, Duration::from_secs(365 * 86_400))
            .await?
            .is_some()
        {
            return Ok(());
        }
        let rules = app.xp_rules(guild).await?;
        let lineage = app.xp_lineage(&rules, channel).await;
        let amount = scale(parity.vote_xp, rules.multiplier_in(&lineage, roles));
        if amount == 0 {
            return Ok(());
        }
        let gain = app.db.add_event_xp(guild, user, amount, week_start(app.now())).await?;
        let before = parity.level_of(gain.old_xp);
        let after = parity.level_of(gain.new_xp);
        if after > before {
            leveling::level_up(
                app,
                &settings,
                LevelUp {
                    guild,
                    user,
                    before,
                    after,
                    author: None,
                    nick: None,
                    roles,
                    here: Id::new(channel),
                },
            )
            .await?;
        }
        Ok(())
    }
    .await;
    if let Err(error) = result {
        tracing::warn!(%error, guild, user, "vote XP failed");
    }
}
