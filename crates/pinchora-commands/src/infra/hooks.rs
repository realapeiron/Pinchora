//! Where feature packages listen to what happens in a server.
//!
//! `events.rs` calls these six entry points at fixed places; a package adds ONE call line to the hook
//! that fits and keeps everything else in its own module, so packages never meet in `events.rs`.
//! Order matters and follows the comments: the first line of a hook runs first. A hook never fails
//! its caller: a package logs its own errors, so one broken feature cannot keep the next one (or the
//! member's XP, or their welcome) from running. Hooks run inline; anything slow belongs in
//! `app.spawn(..)` inside the package.

use std::sync::Arc;

use pinchora_db::GuildSettings;
use twilight_model::{
    channel::Message,
    gateway::payload::incoming::{
        MemberAdd, MemberRemove, MemberUpdate, ReactionAdd, ReactionRemove, ReactionRemoveAll, ReactionRemoveEmoji,
    },
};

use crate::App;

/// What the hooks decided about a message in a server.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Verdict {
    /// Carry on: the message counts for XP and may call a text command.
    Continue,
    /// A package dealt with the message (deleted it, say): it earns no XP and calls no text command.
    Blocked,
}

/// What the hooks decided about a member who just joined.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum JoinVerdict {
    /// Carry on: the join role, the welcome and the join auto-responders run.
    Continue,
    /// Held back until they pass a gate: the join is still logged, but no join role, welcome card or
    /// join auto-responder runs. The package that held them lets them in when they are cleared.
    Held,
}

/// A reaction added to or removed from a message in a server. The bot's own reactions arrive too: a
/// package that reacts to messages itself must skip its own user.
#[derive(Debug, Clone)]
pub enum ReactionEvent {
    Add(Box<ReactionAdd>),
    Remove(Box<ReactionRemove>),
    /// Every reaction was cleared from a message.
    RemoveAll(ReactionRemoveAll),
    /// Every reaction of one emoji was cleared from a message.
    RemoveEmoji(ReactionRemoveEmoji),
}

/// A message from a human in a server (bots, webhooks and system messages are filtered out), judged
/// before XP and before a text command reads it.
pub(crate) async fn after_message(app: &Arc<App>, msg: &Message, _settings: &GuildSettings) -> Verdict {
    // automod goes first: a message it deletes is `Verdict::Blocked`, so it earns nothing.
    if crate::moderation::automod::after_message(app, msg, _settings).await == Verdict::Blocked {
        return Verdict::Blocked;
    }
    if crate::community::tickets::on_guild_message(app, msg).await == Verdict::Blocked {
        return Verdict::Blocked;
    }
    // vote channels react only after moderation accepts the message.
    crate::community::starboard::on_message(app, msg).await;
    // stats counts the message.
    crate::admin::stats::count_message(app, msg).await;
    crate::commands::progression::achievements::on_message(
        app,
        msg.guild_id.expect("guild message").get(),
        msg.author.id.get(),
    )
    .await;
    // AFK and sticky messages come after .
    crate::admin::afk_sticky_sender::after_message(app, msg).await;
    crate::community::engagement::on_guild_message(app, msg).await;
    crate::commands::community::tools::on_message(app, msg).await;
    crate::admin::workflows::triggers::message(app, msg);
    // AI characters answer mentions.
    Verdict::Continue
}

/// A member joined, before their join role and welcome.
pub(crate) async fn on_member_join(app: &Arc<App>, event: &MemberAdd) -> JoinVerdict {
    crate::commands::community::tools::on_join(
        app,
        event.guild_id.get(),
        event.user.id.get(),
        event
            .joined_at
            .and_then(|t| chrono::DateTime::from_timestamp_micros(t.as_micros()))
            .unwrap_or(app.now()),
    )
    .await;
    crate::progression::leveling_policy::on_member_join(app, event).await;
    if crate::moderation::policy::member_join(app, event.guild_id.get(), event.user.id.get()).await == JoinVerdict::Held
    {
        return JoinVerdict::Held;
    }
    let verdict = crate::moderation::verification::on_member_join(app, event).await;
    // stats counts every join, held or not.
    crate::admin::stats::count_member(app, event.guild_id.get(), true).await;
    // attribution must observe every join, including members held by the gate.
    crate::commands::community::invites::on_member_join(app, event).await;
    // join gate and captcha goes first. Roblox queues every join, including one it held.
    let roblox_held = crate::integrations::roblox::on_join(
        app,
        event.guild_id.get(),
        event.member.user.id.get(),
        verdict == JoinVerdict::Held,
    )
    .await;
    if verdict == JoinVerdict::Held || roblox_held {
        return JoinVerdict::Held;
    }
    // auto roles (they honour Held).
    crate::commands::roles::automation::on_member_join(app, event).await;
    crate::admin::workflows::triggers::joined(app, event).await;
    JoinVerdict::Continue
}

/// A member left or was removed, before the goodbye.
pub(crate) async fn on_member_leave(app: &Arc<App>, event: &MemberRemove) {
    crate::commands::community::tools::on_leave(app, event.guild_id.get(), event.user.id.get()).await;
    crate::progression::leveling_policy::on_member_leave(app, event).await;
    crate::moderation::verification::on_member_leave(app, event).await;
    if let Err(error) = app
        .db
        .roblox_forget_member(event.guild_id.get(), event.user.id.get())
        .await
    {
        tracing::warn!(%error,"Roblox member cleanup failed");
    }
    // sticky roles remember what they held.
    crate::commands::roles::automation::on_member_leave(app, event).await;
    // invite tracker takes the invite back.
    crate::commands::community::invites::on_member_leave(app, event).await;
    crate::admin::workflows::triggers::left(app, event);
    // stats counts the leave.
    crate::admin::stats::count_member(app, event.guild_id.get(), false).await;
}

/// A reaction changed on a message in a server.
pub(crate) async fn on_reaction(app: &Arc<App>, event: &ReactionEvent) {
    crate::progression::leveling_policy::on_reaction(app, event);
    // reaction roles.
    crate::commands::roles::automation::on_reaction(app, event).await;
    // starboard counts stars.
    crate::community::starboard::on_reaction(app, event).await;
    crate::admin::workflows::triggers::reaction(app, event);
}

/// A message sent to the bot in a direct message (from a human; DM text needs no privileged intent).
pub(crate) async fn on_dm(_app: &Arc<App>, _msg: &Message) {
    crate::community::engagement::on_dm(_app, _msg).await;
}

/// A member gained a level, after their level roles were given. `role` is the role they earned, if any.
pub(crate) async fn on_level_up(_app: &Arc<App>, _guild: u64, _user: u64, _level: u32, _role: Option<u64>) {
    // achievements (the level kind).
    crate::commands::progression::achievements::on_level_up(_app, _guild, _user).await;
    crate::admin::workflows::triggers::simple(_app, _guild, _user, "level_up", None);
}

/// A member's roles or name changed, before the server log hears of it. Not one of the first six: sticky roles
/// need to see every change to a member's roles, because Discord's leave event does not say what they held.
pub(crate) async fn on_member_update(app: &Arc<App>, event: &MemberUpdate) {
    crate::admin::workflows::triggers::roles(app, event).await;
    let card_app = app.clone();
    let card_event = event.clone();
    app.spawn(async move { crate::admin::card_images::on_member_update(&card_app, &card_event).await });
    crate::moderation::policy::member_event(app, event.guild_id.get(), event.user.id.get()).await;
    crate::moderation::verification::on_member_update(app, event).await;
    // sticky roles keep what the member holds now.
    crate::commands::roles::automation::on_member_update(app, event).await;
}

pub(crate) async fn on_workflow_voice(app: &Arc<App>, state: &twilight_model::voice::VoiceState) {
    crate::admin::workflows::triggers::voice(app, state).await;
}
