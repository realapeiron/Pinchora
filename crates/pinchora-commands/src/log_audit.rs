//! What Discord's audit log reports, as it happens (`GUILD_AUDIT_LOG_ENTRY_CREATE`, which needs the
//! bot to have View Audit Log): the server log's entries for roles, channels, threads, permissions,
//! the server's settings, invites, emojis and stickers, kicks and timeouts, each with who did it, and
//! the staff action log's lines for what staff did by hand.

use std::sync::Arc;

use pinchora_core::logging::{LogEvent, Subject};
use pinchora_db::NewStaffAction;
use twilight_model::{
    gateway::payload::incoming::GuildAuditLogEntryCreate,
    guild::{
        Permissions,
        audit_log::{AuditLogChange, AuditLogEntry, AuditLogEventType},
    },
};
use twilight_util::builder::embed::EmbedFieldBuilder;

use crate::{
    App,
    server_log::{Attribution, clip, code, entry, post, route},
    util::{BRAND, DANGER, SUCCESS, WARN, display_name, mention, relative},
};

/// The most change lines one entry shows.
const MAX_LINES: usize = 12;

/// The staff log's name for an audit log action, or `None` for actions that are no staff's doing
/// (or too noisy to keep, like pins).
pub(crate) fn action_key(kind: AuditLogEventType) -> Option<&'static str> {
    use AuditLogEventType as A;
    Some(match kind {
        A::GuildUpdate => "server_update",
        A::ChannelCreate => "channel_create",
        A::ChannelUpdate => "channel_update",
        A::ChannelDelete => "channel_delete",
        A::ChannelOverwriteCreate | A::ChannelOverwriteUpdate | A::ChannelOverwriteDelete => "channel_permissions",
        A::MemberKick => "kick",
        A::MemberPrune => "prune",
        A::MemberBanAdd => "ban",
        A::MemberBanRemove => "unban",
        A::MemberUpdate => "member_update",
        A::MemberRoleUpdate => "member_roles",
        A::MemberMove => "voice_move",
        A::MemberDisconnect => "voice_disconnect",
        A::BotAdd => "bot_add",
        A::RoleCreate => "role_create",
        A::RoleUpdate => "role_update",
        A::RoleDelete => "role_delete",
        A::InviteCreate => "invite_create",
        A::InviteDelete => "invite_delete",
        A::WebhookCreate => "webhook_create",
        A::WebhookUpdate => "webhook_update",
        A::WebhookDelete => "webhook_delete",
        A::EmojiCreate | A::EmojiUpdate | A::EmojiDelete => "emojis",
        A::StickerCreate | A::StickerUpdate | A::StickerDelete => "stickers",
        A::MessageDelete => "message_delete",
        A::MessageBulkDelete => "message_bulk_delete",
        A::ThreadCreate => "thread_create",
        A::ThreadUpdate => "thread_update",
        A::ThreadDelete => "thread_delete",
        _ => return None,
    })
}

/// The server log event an audit log action is, if the log reports it from here.
pub(crate) fn log_event(entry: &AuditLogEntry) -> Option<LogEvent> {
    use AuditLogEventType as A;
    Some(match entry.action_type {
        A::GuildUpdate => LogEvent::ServerUpdate,
        A::ChannelCreate => LogEvent::ChannelCreate,
        A::ChannelUpdate => LogEvent::ChannelUpdate,
        A::ChannelDelete => LogEvent::ChannelDelete,
        A::ChannelOverwriteCreate | A::ChannelOverwriteUpdate | A::ChannelOverwriteDelete => {
            LogEvent::ChannelPermissions
        }
        A::ThreadCreate => LogEvent::ThreadCreate,
        A::ThreadUpdate => LogEvent::ThreadUpdate,
        A::ThreadDelete => LogEvent::ThreadDelete,
        A::RoleCreate => LogEvent::RoleCreate,
        A::RoleUpdate => LogEvent::RoleUpdate,
        A::RoleDelete => LogEvent::RoleDelete,
        A::MemberRoleUpdate => LogEvent::MemberRoles,
        A::MemberKick => LogEvent::Kick,
        // A member update is a timeout here; nicknames come from the member events, mutes from voice.
        A::MemberUpdate
            if entry
                .changes
                .iter()
                .any(|c| matches!(c, AuditLogChange::CommunicationDisabledUntil { .. })) =>
        {
            LogEvent::Timeout
        }
        A::InviteCreate => LogEvent::InviteCreate,
        A::InviteDelete => LogEvent::InviteDelete,
        A::EmojiCreate | A::EmojiUpdate | A::EmojiDelete => LogEvent::Emojis,
        A::StickerCreate | A::StickerUpdate | A::StickerDelete => LogEvent::Stickers,
        _ => return None,
    })
}

/// "Send messages" for `SEND_MESSAGES`.
fn permission_name(raw: &str) -> String {
    let lower = raw.replace('_', " ").to_lowercase();
    let mut chars = lower.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => lower,
    }
}

/// The permissions that `new` has and `old` lacks, and the other way round, in words.
pub(crate) fn permission_diff(old: Permissions, new: Permissions) -> (Vec<String>, Vec<String>) {
    let names = |p: Permissions| p.iter_names().map(|(n, _)| permission_name(n)).collect::<Vec<_>>();
    (names(new - old), names(old - new))
}

fn shown<T: ToString>(value: &Option<T>) -> String {
    value
        .as_ref()
        .map_or_else(|| "*none*".to_owned(), |v| code(&clip(&v.to_string(), 200)))
}

fn yes_no(value: &Option<bool>) -> &'static str {
    match value {
        Some(true) => "yes",
        Some(false) => "no",
        None => "*none*",
    }
}

fn channel(value: &Option<twilight_model::id::Id<twilight_model::id::marker::ChannelMarker>>) -> String {
    value.map_or_else(|| "*none*".to_owned(), |c| format!("<#{c}>"))
}

/// One change in words: `**Name**: `old` → `new``. `None` for a change not worth a line (a channel
/// moving position) or one Pinchora does not read.
pub(crate) fn change_line(change: &AuditLogChange) -> Option<String> {
    use AuditLogChange as C;
    let pair = |what: &str, old: String, new: String| Some(format!("**{what}**: {old} → {new}"));
    match change {
        C::Name { old, new } => pair("Name", shown(old), shown(new)),
        C::Topic { old, new } => pair("Topic", shown(old), shown(new)),
        C::Description { old, new } => pair("Description", shown(old), shown(new)),
        C::Nsfw { old, new } => pair("Age-restricted", yes_no(old).into(), yes_no(new).into()),
        C::RateLimitPerUser { old, new } => pair("Slowmode (seconds)", shown(old), shown(new)),
        C::Bitrate { old, new } => pair("Bitrate", shown(old), shown(new)),
        C::UserLimit { old, new } => pair("User limit", shown(old), shown(new)),
        C::Color { old, new } => pair(
            "Colour",
            old.map_or("*none*".into(), |c| format!("`#{c:06x}`")),
            new.map_or("*none*".into(), |c| format!("`#{c:06x}`")),
        ),
        C::Hoist { old, new } => pair("Shown separately", yes_no(old).into(), yes_no(new).into()),
        C::Mentionable { old, new } => pair("Mentionable", yes_no(old).into(), yes_no(new).into()),
        C::Archived { old, new } => pair("Archived", yes_no(old).into(), yes_no(new).into()),
        C::Locked { old, new } => pair("Locked", yes_no(old).into(), yes_no(new).into()),
        C::Code { new, old } => Some(format!("**Code**: {}", shown(&new.clone().or(old.clone())))),
        C::ChannelId { new, old } => Some(format!("**Channel**: {}", channel(&new.or(*old)))),
        C::MaxUses { new, .. } => Some(format!(
            "**Uses**: {}",
            new.filter(|n| *n > 0).map_or("no limit".to_owned(), |n| n.to_string())
        )),
        C::MaxAge { new, .. } => Some(format!(
            "**Expires**: {}",
            new.filter(|n| *n > 0)
                .map_or("never".to_owned(), |n| format!("after {n} seconds"))
        )),
        C::Temporary { new, .. } => Some(format!("**Temporary membership**: {}", yes_no(new))),
        C::IconHash { .. } => Some("**Icon** changed".to_owned()),
        C::VanityUrlCode { old, new } => pair("Vanity link", shown(old), shown(new)),
        C::AfkChannelId { old, new } => pair("AFK channel", channel(old), channel(new)),
        C::SystemChannelId { old, new } => pair("System channel", channel(old), channel(new)),
        C::RulesChannelId { old, new } => pair("Rules channel", channel(old), channel(new)),
        C::Nick { old, new } => pair("Nickname", shown(old), shown(new)),
        C::CommunicationDisabledUntil { new, .. } => Some(match new {
            Some(until) => match chrono::DateTime::from_timestamp(until.as_secs(), 0) {
                Some(at) => format!("**Timed out** until {}", relative(at)),
                None => "**Timed out**".to_owned(),
            },
            None => "**Timeout** removed".to_owned(),
        }),
        C::RoleAdded { new, .. } => Some(format!(
            "**Given**: {}",
            new.iter()
                .map(|r| format!("<@&{}>", r.id))
                .collect::<Vec<_>>()
                .join(" ")
        )),
        C::RoleRemoved { new, .. } => Some(format!(
            "**Taken**: {}",
            new.iter()
                .map(|r| format!("<@&{}>", r.id))
                .collect::<Vec<_>>()
                .join(" ")
        )),
        C::Permissions { old, new } | C::Allow { old, new } | C::Deny { old, new } => {
            let what = match change {
                C::Allow { .. } => "Allowed",
                C::Deny { .. } => "Denied",
                _ => "Permissions",
            };
            let (added, removed) = permission_diff(
                old.unwrap_or_else(Permissions::empty),
                new.unwrap_or_else(Permissions::empty),
            );
            let mut parts = Vec::new();
            if !added.is_empty() {
                parts.push(format!("+ {}", added.join(", ")));
            }
            if !removed.is_empty() {
                parts.push(format!("− {}", removed.join(", ")));
            }
            (!parts.is_empty()).then(|| format!("**{what}**: {}", parts.join(" · ")))
        }
        C::Position { .. } => None,
        _ => None,
    }
}

/// Whether the entry is only channels or roles being reordered, which Discord reports one entry per
/// moved item: not worth a line in the log.
fn only_moves(entry: &AuditLogEntry) -> bool {
    !entry.changes.is_empty()
        && entry
            .changes
            .iter()
            .all(|c| matches!(c, AuditLogChange::Position { .. }))
}

/// The thing an entry is about, as Discord markup.
fn target_text(entry: &AuditLogEntry, event: LogEvent) -> String {
    let Some(target) = entry.target_id.map(|t| t.get()) else {
        return "the server".to_owned();
    };
    let named = entry.changes.iter().find_map(|c| match c {
        AuditLogChange::Name { new, old } => new.clone().or(old.clone()),
        _ => None,
    });
    let deleted = matches!(
        event,
        LogEvent::ChannelDelete | LogEvent::ThreadDelete | LogEvent::RoleDelete
    ) || matches!(
        entry.action_type,
        AuditLogEventType::EmojiDelete | AuditLogEventType::StickerDelete
    );
    match event {
        LogEvent::ServerUpdate => "the server".to_owned(),
        LogEvent::MemberRoles | LogEvent::Kick | LogEvent::Timeout => mention(target),
        LogEvent::RoleCreate | LogEvent::RoleUpdate if !deleted => format!("<@&{target}>"),
        LogEvent::ChannelCreate
        | LogEvent::ChannelUpdate
        | LogEvent::ChannelPermissions
        | LogEvent::ThreadCreate
        | LogEvent::ThreadUpdate
            if !deleted =>
        {
            format!("<#{target}>")
        }
        _ => named.map_or_else(|| format!("`{target}`"), |n| code(&n)),
    }
}

/// Whose permissions an overwrite entry is about.
fn overwrite_text(entry: &AuditLogEntry) -> Option<String> {
    let options = entry.options.as_ref()?;
    let id = options.id?;
    Some(match options.kind.as_deref() {
        Some("1") => format!("for {}", mention(id)),
        _ => match &options.role_name {
            Some(name) if name == "@everyone" => "for @everyone".to_owned(),
            _ => format!("for <@&{id}>"),
        },
    })
}

fn color_of(kind: AuditLogEventType) -> u32 {
    use AuditLogEventType as A;
    match kind {
        A::ChannelCreate
        | A::RoleCreate
        | A::InviteCreate
        | A::EmojiCreate
        | A::StickerCreate
        | A::ThreadCreate
        | A::ChannelOverwriteCreate => SUCCESS,
        A::ChannelDelete
        | A::RoleDelete
        | A::InviteDelete
        | A::EmojiDelete
        | A::StickerDelete
        | A::ThreadDelete
        | A::ChannelOverwriteDelete
        | A::MemberKick => DANGER,
        A::MemberUpdate => WARN,
        _ => BRAND,
    }
}

/// The server log entry for an audit log entry.
pub(crate) fn log_embed(entry_: &AuditLogEntry, event: LogEvent) -> twilight_model::channel::message::Embed {
    let actor = entry_.user_id.map(|u| u.get());
    let mut text = target_text(entry_, event);
    if let Some(whose) = overwrite_text(entry_) {
        text.push(' ');
        text.push_str(&whose);
    }
    if let Some(actor) = actor {
        text.push_str(&format!(" · by {}", mention(actor)));
    }
    let mut lines: Vec<String> = entry_.changes.iter().filter_map(change_line).collect();
    let more = lines.len().saturating_sub(MAX_LINES);
    lines.truncate(MAX_LINES);
    if more > 0 {
        lines.push(format!("and {more} more"));
    }
    if !lines.is_empty() {
        text.push_str("\n\n");
        text.push_str(&lines.join("\n"));
    }
    let id = entry_.target_id.map_or(0, |t| t.get());
    let mut embed = entry(
        event.label(),
        color_of(entry_.action_type),
        id,
        None,
        clip(&text, 4_000),
    );
    if let Some(reason) = &entry_.reason {
        embed = embed.field(EmbedFieldBuilder::new("Reason", clip(reason, 1_000)));
    }
    embed.build()
}

/// The staff log line's details: what changed, on one line.
fn details(entry: &AuditLogEntry) -> Option<String> {
    let lines: Vec<String> = entry.changes.iter().filter_map(change_line).collect();
    (!lines.is_empty()).then(|| clip(&lines.join(" · ").replace("**", ""), 500))
}

/// One audit log entry: logged where the server log wants it, noted for the ban's own entry, and put
/// in the staff log when a member did it by hand.
pub async fn on_audit_log_entry(app: Arc<App>, event: GuildAuditLogEntryCreate) -> anyhow::Result<()> {
    let entry = event.0;
    let Some(guild) = entry.guild_id.map(|g| g.get()) else {
        return Ok(());
    };
    let me = app.application_id.get();
    let actor = entry.user_id.map(|u| u.get());
    let target = entry.target_id.map(|t| t.get());
    let kind = entry.action_type;
    if let (Some(actor), Some(target)) = (actor, target)
        && matches!(
            kind,
            AuditLogEventType::MemberBanAdd | AuditLogEventType::MemberBanRemove
        )
    {
        let by = Attribution {
            actor,
            reason: entry.reason.clone(),
        };
        app.log_state
            .note_ban(guild, target, kind == AuditLogEventType::MemberBanAdd, by)
            .await;
    }
    let actor_user = match actor {
        Some(actor) if actor != me => app.user(actor).await,
        _ => None,
    };
    // Staff: what a member did to the server or to somebody else. Pinchora's own actions are in the
    // log already, with the moderator who asked for them.
    if let (Some(actor), Some(action)) = (actor, action_key(kind))
        && actor != me
        && !(kind == AuditLogEventType::MemberUpdate && target == Some(actor))
    {
        let actor_name = actor_user
            .as_deref()
            .map_or_else(|| format!("User {actor}"), |u| display_name(u, None));
        let target_name = match kind {
            AuditLogEventType::MemberKick
            | AuditLogEventType::MemberBanAdd
            | AuditLogEventType::MemberBanRemove
            | AuditLogEventType::MemberUpdate
            | AuditLogEventType::MemberRoleUpdate => match target {
                Some(t) => app.user(t).await.map(|u| display_name(&u, None)),
                None => None,
            },
            _ => entry.changes.iter().find_map(|c| match c {
                AuditLogChange::Name { new, old } => new.clone().or(old.clone()),
                _ => None,
            }),
        };
        let details = details(&entry);
        if let Err(error) = app
            .db
            .record_staff_action(&NewStaffAction {
                guild,
                actor,
                actor_name: &actor_name,
                source: "discord",
                action,
                target,
                target_name: target_name.as_deref(),
                case_number: None,
                reason: entry.reason.as_deref(),
                details: details.as_deref(),
            })
            .await
        {
            tracing::warn!(%error, guild, "staff action not recorded");
        }
    }
    let Some(event) = log_event(&entry) else {
        return Ok(());
    };
    // Bans, kicks and timeouts done through Pinchora have their case in the mod-log.
    if actor == Some(me) && matches!(event, LogEvent::Kick | LogEvent::Timeout) {
        return Ok(());
    }
    if only_moves(&entry) {
        return Ok(());
    }
    let member_event = matches!(event, LogEvent::MemberRoles | LogEvent::Kick | LogEvent::Timeout);
    let subject = Subject {
        user: if member_event { target } else { actor },
        bot: !member_event && actor_user.as_ref().is_some_and(|u| u.bot),
        ..Subject::default()
    };
    let place = match event.category() {
        pinchora_core::logging::LogCategory::Channels => target,
        pinchora_core::logging::LogCategory::Invites => entry.changes.iter().find_map(|c| match c {
            AuditLogChange::ChannelId { new, old } => new.or(*old).map(|c| c.get()),
            _ => None,
        }),
        _ => None,
    };
    if let Some(to) = route(&app, guild, event, subject, place).await {
        post(&app, to, log_embed(&entry, event)).await;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn audit(value: serde_json::Value) -> AuditLogEntry {
        serde_json::from_value(value).expect("audit log entry JSON")
    }

    #[test]
    fn permissions_read_as_words() {
        let (added, removed) = permission_diff(
            Permissions::SEND_MESSAGES | Permissions::VIEW_CHANNEL,
            Permissions::VIEW_CHANNEL | Permissions::MANAGE_MESSAGES,
        );
        assert_eq!(added, ["Manage messages"]);
        assert_eq!(removed, ["Send messages"]);
    }

    #[test]
    fn a_channel_rename_by_a_moderator_names_both() {
        let entry = audit(json!({
            "action_type": 11,
            "changes": [
                { "key": "name", "old_value": "general", "new_value": "lobby" },
                { "key": "rate_limit_per_user", "old_value": 0, "new_value": 30 },
            ],
            "guild_id": "1",
            "id": "100",
            "target_id": "55",
            "user_id": "7",
        }));
        assert_eq!(log_event(&entry), Some(LogEvent::ChannelUpdate));
        let embed = log_embed(&entry, LogEvent::ChannelUpdate);
        let text = embed.description.unwrap();
        assert!(text.starts_with("<#55> · by <@7>"), "{text}");
        assert!(text.contains("**Name**: `general` → `lobby`"), "{text}");
        assert!(text.contains("**Slowmode (seconds)**: `0` → `30`"), "{text}");
        assert_eq!(embed.title.as_deref(), Some("Channel changed"));
    }

    #[test]
    fn member_updates_are_timeouts_only_when_the_timeout_changed() {
        let timeout = audit(json!({
            "action_type": 24,
            "changes": [{ "key": "communication_disabled_until", "new_value": "2030-01-01T00:00:00.000000+00:00" }],
            "guild_id": "1", "id": "101", "target_id": "8", "user_id": "7", "reason": "spam",
        }));
        assert_eq!(log_event(&timeout), Some(LogEvent::Timeout));
        let embed = log_embed(&timeout, LogEvent::Timeout);
        assert!(embed.description.as_deref().unwrap().contains("**Timed out** until"));
        assert_eq!(embed.fields[0].value, "spam");
        let nick = audit(json!({
            "action_type": 24,
            "changes": [{ "key": "nick", "old_value": "a", "new_value": "b" }],
            "guild_id": "1", "id": "102", "target_id": "8", "user_id": "8",
        }));
        assert_eq!(log_event(&nick), None, "nicknames come from the member events");
        assert_eq!(action_key(nick.action_type), Some("member_update"));
    }

    #[test]
    fn role_changes_and_overwrites_say_who_and_what() {
        let roles = audit(json!({
            "action_type": 25,
            "changes": [{ "key": "$add", "new_value": [{ "id": "40", "name": "Helper" }] }],
            "guild_id": "1", "id": "103", "target_id": "8", "user_id": "7",
        }));
        let text = log_embed(&roles, LogEvent::MemberRoles).description.unwrap();
        assert!(
            text.starts_with("<@8> · by <@7>") && text.contains("**Given**: <@&40>"),
            "{text}"
        );
        let overwrite = audit(json!({
            "action_type": 14,
            "changes": [{ "key": "deny", "old_value": "0", "new_value": "2048" }],
            "options": { "id": "1", "type": "0", "role_name": "@everyone" },
            "guild_id": "1", "id": "104", "target_id": "55", "user_id": "7",
        }));
        let text = log_embed(&overwrite, LogEvent::ChannelPermissions).description.unwrap();
        assert!(text.starts_with("<#55> for @everyone · by <@7>"), "{text}");
        assert!(text.contains("**Denied**: + Send messages"), "{text}");
    }

    #[test]
    fn deleted_things_are_named_not_mentioned_and_reorders_are_skipped() {
        let deleted = audit(json!({
            "action_type": 12,
            "changes": [{ "key": "name", "old_value": "old-news" }],
            "guild_id": "1", "id": "105", "target_id": "56", "user_id": "7",
        }));
        assert!(
            log_embed(&deleted, LogEvent::ChannelDelete)
                .description
                .unwrap()
                .starts_with("`old-news`")
        );
        let moved = audit(json!({
            "action_type": 11,
            "changes": [{ "key": "position", "old_value": 1, "new_value": 2 }],
            "guild_id": "1", "id": "106", "target_id": "56", "user_id": "7",
        }));
        assert!(only_moves(&moved));
    }
}
