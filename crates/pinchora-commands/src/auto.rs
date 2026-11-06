//! Auto-responders at run time: a message the server wrote, sent when a member joins or leaves, levels up,
//! gets or loses a role, or (with the Message Content intent) says a keyword. Nothing here ever fails the
//! event it hangs off: a responder that cannot answer is logged and skipped.

use std::{sync::Arc, time::Duration};

use pinchora_core::{
    i18n::Lang,
    modules::Module,
    tags::{Destination, TriggerKind, run_cooldown},
};
use pinchora_db::ResponderRow;
use twilight_model::{
    channel::{Message, message::AllowedMentions},
    gateway::payload::incoming::MemberUpdate,
    guild::Member,
    id::{
        Id,
        marker::{ChannelMarker, MessageMarker},
    },
    user::User,
};

use crate::{
    App,
    custom::{
        Actor, Scene, Spec, all_texts, dm_allowed, own_change_key, pick, render_message, run_discord_actions,
        run_points, scene_vars, to_discord,
    },
    leveling::private_channel,
};

/// What happened, and to whom.
struct Moment<'a> {
    guild: u64,
    user: &'a User,
    nick: Option<&'a str>,
    /// The roles the member holds, for a toggle.
    held: &'a [u64],
    /// The channel it happened in: the one a message was sent in, or where a member levelled up.
    here: Option<u64>,
    /// The message that set a keyword responder off.
    message: Option<Id<MessageMarker>>,
    /// Placeholders of this kind of event, already worded.
    extra: Vec<(&'static str, String)>,
}

/// The responders of `kind` the server has switched on.
fn watching(set: &pinchora_db::TagSet, kind: TriggerKind) -> Vec<&ResponderRow> {
    set.responders
        .iter()
        .filter(|r| r.def.enabled && r.def.trigger_kind == kind)
        .collect()
}

/// A member joined (a bot never sets one off).
pub(crate) async fn member_joined(app: &Arc<App>, guild: u64, member: &Member) {
    if member.user.bot {
        return;
    }
    let Ok(set) = app.tag_set(guild).await else { return };
    let roles: Vec<u64> = member.roles.iter().map(|r| r.get()).collect();
    if set
        .responders
        .iter()
        .any(|r| r.def.enabled && r.def.watched_role().is_some())
    {
        app.custom.seed_roles(guild, member.user.id.get(), roles.clone()).await;
    }
    let found = watching(&set, TriggerKind::MemberJoin);
    if found.is_empty() {
        return;
    }
    let moment = Moment {
        guild,
        user: &member.user,
        nick: member.nick.as_deref(),
        held: &roles,
        here: None,
        message: None,
        extra: Vec::new(),
    };
    for row in found {
        fire(app, row, &moment).await;
    }
}

/// A member left.
pub(crate) async fn member_left(app: &Arc<App>, guild: u64, user: &User) {
    if user.bot {
        return;
    }
    let Ok(set) = app.tag_set(guild).await else { return };
    let found = watching(&set, TriggerKind::MemberLeave);
    if found.is_empty() {
        return;
    }
    let moment = Moment {
        guild,
        user,
        nick: None,
        held: &[],
        here: None,
        message: None,
        extra: Vec::new(),
    };
    for row in found {
        fire(app, row, &moment).await;
    }
}

/// A member reached `level`; `role` is the level role they just earned, if any.
pub(crate) struct LevelUpEvent {
    pub guild: u64,
    pub user: User,
    pub nick: Option<String>,
    pub held: Vec<u64>,
    pub level: u32,
    pub role: Option<u64>,
    pub here: u64,
}

pub(crate) async fn leveled_up(app: &Arc<App>, up: LevelUpEvent) {
    if up.user.bot {
        return;
    }
    let Ok(set) = app.tag_set(up.guild).await else { return };
    let found: Vec<&ResponderRow> = watching(&set, TriggerKind::LevelUp)
        .into_iter()
        .filter(|r| up.level >= r.def.min_level())
        .collect();
    if found.is_empty() {
        return;
    }
    let moment = Moment {
        guild: up.guild,
        user: &up.user,
        nick: up.nick.as_deref(),
        held: &up.held,
        here: Some(up.here),
        message: None,
        extra: vec![
            ("level", up.level.to_string()),
            ("role", up.role.map(|r| format!("<@&{r}>")).unwrap_or_default()),
        ],
    };
    for row in found {
        fire(app, row, &moment).await;
    }
}

/// A member's roles changed: tells apart the roles they got and lost from what the bot remembered. The
/// first update the bot sees for a member only teaches it their roles.
pub(crate) async fn roles_changed(app: &Arc<App>, event: &MemberUpdate) {
    if event.user.bot {
        return;
    }
    let (guild, user) = (event.guild_id.get(), event.user.id.get());
    let Ok(set) = app.tag_set(guild).await else { return };
    let watchers: Vec<&ResponderRow> = set
        .responders
        .iter()
        .filter(|r| r.def.enabled && matches!(r.def.trigger_kind, TriggerKind::RoleAdded | TriggerKind::RoleRemoved))
        .collect();
    if watchers.is_empty() {
        return;
    }
    let now: Vec<u64> = event.roles.iter().map(|r| r.get()).collect();
    let Some(before) = app.custom.swap_roles(guild, user, now.clone()).await else {
        return;
    };
    // A change an auto-responder made sets no responder off: a chain stops after one responder.
    let mut ours = Vec::new();
    for role in watchers.iter().filter_map(|r| r.def.watched_role()) {
        if now.contains(&role) != before.contains(&role) && !ours.contains(&role) {
            match app.cache.take(&own_change_key(guild, user, role)).await {
                Ok(Some(_)) => ours.push(role),
                Ok(None) => {}
                Err(error) => tracing::warn!(guild, %error, "could not read a responder's role change"),
            }
        }
    }
    for row in watchers {
        let Some(role) = row.def.watched_role() else { continue };
        let happened = match row.def.trigger_kind {
            TriggerKind::RoleAdded => now.contains(&role) && !before.contains(&role),
            _ => !now.contains(&role) && before.contains(&role),
        };
        if !happened || ours.contains(&role) {
            continue;
        }
        let moment = Moment {
            guild,
            user: &event.user,
            nick: event.nick.as_deref(),
            held: &now,
            here: None,
            message: None,
            extra: vec![("role", format!("<@&{role}>"))],
        };
        fire(app, row, &moment).await;
    }
}

/// A chat message: keywords it holds set responders off, at most one per message. Messages only have
/// text when the bot runs with the Message Content intent. Without it Discord still sends the text of
/// messages that mention the bot (text commands need it), but keyword responders never hear it.
pub(crate) async fn chat(app: &Arc<App>, msg: &Message) {
    let Some(guild) = msg.guild_id else { return };
    let guild_id = guild.get();
    let Ok(set) = app.tag_set(guild_id).await else { return };
    if set.responders.is_empty() {
        return;
    }
    let held: Vec<u64> = msg
        .member
        .as_ref()
        .map(|m| m.roles.iter().map(|r| r.get()).collect())
        .unwrap_or_default();
    if msg.member.is_some()
        && set
            .responders
            .iter()
            .any(|r| r.def.enabled && r.def.watched_role().is_some())
    {
        app.custom.seed_roles(guild_id, msg.author.id.get(), held.clone()).await;
    }
    if msg.content.is_empty() || !app.message_content {
        return;
    }
    let heard: Vec<&ResponderRow> = set
        .responders
        .iter()
        .filter(|r| r.def.enabled && r.def.hears(&msg.content))
        .collect();
    if heard.is_empty()
        || !app
            .module_allowed(guild_id, Some(msg.channel_id.get()), Module::CustomCommands)
            .await
    {
        return;
    }
    let moment = Moment {
        guild: guild_id,
        user: &msg.author,
        nick: msg.member.as_ref().and_then(|m| m.nick.as_deref()),
        held: &held,
        here: Some(msg.channel_id.get()),
        message: Some(msg.id),
        extra: Vec::new(),
    };
    for row in heard {
        if fire(app, row, &moment).await {
            break;
        }
    }
}

/// Answers once for `row`: the chance, the cooldown, where it may answer, its actions and its message.
/// Returns whether it answered.
async fn fire(app: &Arc<App>, row: &ResponderRow, moment: &Moment<'_>) -> bool {
    match try_fire(app, row, moment).await {
        Ok(fired) => fired,
        Err(error) => {
            tracing::warn!(error = ?error, guild = moment.guild, responder = row.id, "an auto-responder failed");
            false
        }
    }
}

async fn try_fire(app: &Arc<App>, row: &ResponderRow, m: &Moment<'_>) -> anyhow::Result<bool> {
    let def = &row.def;
    let guild = m.guild;
    let user = m.user.id.get();
    // Where it answers must be somewhere the server's module rules leave open.
    let target: Option<Id<ChannelMarker>> = match def.destination {
        Destination::Current => m.here,
        Destination::Channel => def.channel_id,
        Destination::Dm => None,
    }
    .map(|id| Id::new(id.max(1)));
    match (def.destination, target) {
        (Destination::Dm, _) => {
            if !app.module_enabled(guild, Module::CustomCommands).await {
                return Ok(false);
            }
        }
        (_, Some(channel)) => {
            if !app
                .module_allowed(guild, Some(channel.get()), Module::CustomCommands)
                .await
            {
                return Ok(false);
            }
        }
        (_, None) => return Ok(false),
    }
    if let Some(here) = m.here
        && !(def.allow_channels.is_empty() && def.deny_channels.is_empty())
    {
        let lineage = app
            .named_lineage(here, &[&def.allow_channels, &def.deny_channels])
            .await;
        if !def.allows_channel(&lineage) {
            return Ok(false);
        }
    }
    if def.chance < 100 && rand::random_range(1..=100) > def.chance {
        return Ok(false);
    }
    let cooldown = run_cooldown(def.cooldown_secs, &def.actions);
    if cooldown > 0 {
        let key = format!("auto:{guild}:{}:{user}", row.id);
        let ttl = Duration::from_secs(cooldown as u64);
        if app.cache.try_cooldown(&key, ttl).await?.is_some() {
            return Ok(false);
        }
    }

    let points = run_points(app, Lang::En, guild, user, &def.actions).await;
    if let Some(note) = &points.capped {
        tracing::debug!(
            guild,
            responder = row.id,
            note,
            "an auto-responder hit the daily points cap"
        );
    }
    let mut scene = Scene::of(guild, m.here.or(def.channel_id), m.user, m.nick);
    scene.extra.clone_from(&m.extra);
    let spec = Spec {
        responses: &def.responses,
        embed: def.embed.as_ref(),
        allow_mentions: def.allow_mentions,
    };
    let vars = scene_vars(app, &scene, &all_texts(&spec, &def.actions), points.balance).await;
    let rendered = render_message(&spec, &vars, &mut pick);
    let (content, embed, mentions) = to_discord(&rendered, guild, user);
    if !(content.is_empty() && embed.is_none()) {
        let embeds: Vec<_> = embed.into_iter().collect();
        match (def.destination, target) {
            (Destination::Dm, _) => {
                if dm_allowed(app, guild, user).await
                    && let Some(channel) = private_channel(app, user).await
                {
                    // A direct message pings nobody else.
                    let none = AllowedMentions::default();
                    send(app, channel, &content, &embeds, &none, None, true).await;
                }
            }
            (destination, Some(channel)) => {
                let reply = (destination == Destination::Current).then_some(m.message).flatten();
                send(app, channel, &content, &embeds, &mentions, reply, false).await;
            }
            (_, None) => {}
        }
    }
    let actor = Actor {
        guild,
        user,
        held: m.held,
        message: m.here.zip(m.message).map(|(c, msg)| (Id::new(c.max(1)), msg)),
        lang: Lang::En,
        responder: true,
    };
    // Nobody ran this, so there is nobody to tell about an action that did not work.
    let notes = run_discord_actions(app, &actor, &def.actions, &vars).await;
    for note in notes {
        tracing::debug!(guild, responder = row.id, note, "an auto-responder action did not work");
    }
    metrics::counter!("pinchora_auto_responders_total", "trigger" => def.trigger_kind.key()).increment(1);
    Ok(true)
}

/// Sends a message; a refusal (no access, a closed direct message) is the server's or the member's doing,
/// so it is logged and left.
async fn send(
    app: &App,
    channel: Id<ChannelMarker>,
    content: &str,
    embeds: &[twilight_model::channel::message::Embed],
    mentions: &AllowedMentions,
    reply: Option<Id<MessageMarker>>,
    quiet: bool,
) {
    let mut request = app.http.create_message(channel).allowed_mentions(Some(mentions));
    if !content.is_empty() {
        request = request.content(content);
    }
    if !embeds.is_empty() {
        request = request.embeds(embeds);
    }
    if let Some(message) = reply {
        request = request.reply(message).fail_if_not_exists(false);
    }
    if let Err(error) = request.await {
        if quiet {
            tracing::debug!(%error, "an auto-responder direct message was not delivered");
        } else {
            tracing::warn!(%error, channel = channel.get(), "an auto-responder could not post");
        }
    }
}
