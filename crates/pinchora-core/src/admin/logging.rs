//! The detailed server log: which events it knows, the category each sits in, where each goes and
//! who it leaves alone. Pure: the bot, `/config log-events` and the dashboard all decide with these.
//!
//! The first four switches of the log (`log_members`, `log_names`, `log_bans`, `log_voice` in
//! `guilds`) stay what they were: each covers its group of events. Every event can also be switched
//! off on its own, and an event is logged when its group (if it has one) is on and it is not switched
//! off itself.

use std::collections::HashMap;

use crate::admin::settings::FieldError;

/// How many channels, roles or members an ignore list may hold.
pub const MAX_IGNORED: usize = 50;

/// Where an event is posted: each category can have a channel of its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum LogCategory {
    Members,
    Messages,
    Roles,
    Channels,
    Voice,
    Moderation,
    Server,
    Invites,
    Emojis,
}

impl LogCategory {
    pub const ALL: [Self; 9] = [
        Self::Members,
        Self::Messages,
        Self::Roles,
        Self::Channels,
        Self::Voice,
        Self::Moderation,
        Self::Server,
        Self::Invites,
        Self::Emojis,
    ];

    pub const fn key(self) -> &'static str {
        match self {
            Self::Members => "members",
            Self::Messages => "messages",
            Self::Roles => "roles",
            Self::Channels => "channels",
            Self::Voice => "voice",
            Self::Moderation => "moderation",
            Self::Server => "server",
            Self::Invites => "invites",
            Self::Emojis => "emojis",
        }
    }

    pub fn parse(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|c| c.key() == key)
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Members => "Members",
            Self::Messages => "Messages",
            Self::Roles => "Roles",
            Self::Channels => "Channels and threads",
            Self::Voice => "Voice",
            Self::Moderation => "Moderation",
            Self::Server => "Server",
            Self::Invites => "Invites",
            Self::Emojis => "Emojis and stickers",
        }
    }

    pub fn events(self) -> impl Iterator<Item = LogEvent> {
        LogEvent::ALL.into_iter().filter(move |e| e.category() == self)
    }
}

/// One of the older switches, each covering a group of events.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Group {
    Members,
    Names,
    Bans,
    Voice,
}

impl Group {
    pub const ALL: [Self; 4] = [Self::Members, Self::Names, Self::Bans, Self::Voice];

    /// The `guilds` column of the switch.
    pub const fn column(self) -> &'static str {
        match self {
            Self::Members => "log_members",
            Self::Names => "log_names",
            Self::Bans => "log_bans",
            Self::Voice => "log_voice",
        }
    }

    pub fn events(self) -> impl Iterator<Item = LogEvent> {
        LogEvent::ALL.into_iter().filter(move |e| e.group() == Some(self))
    }
}

/// Something the log can report.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum LogEvent {
    Report,
    Appeal,
    Restriction,
    RoleRule,
    AutoPurge,

    MemberJoin,
    MemberLeave,
    Nickname,
    Username,
    Avatar,
    MessageEdit,
    MessageDelete,
    MessageBulkDelete,
    RoleCreate,
    RoleDelete,
    RoleUpdate,
    MemberRoles,
    ChannelCreate,
    ChannelDelete,
    ChannelUpdate,
    ChannelPermissions,
    ThreadCreate,
    ThreadDelete,
    ThreadUpdate,
    VoiceJoin,
    VoiceLeave,
    VoiceMove,
    VoiceModeration,
    VoiceSelf,
    Ban,
    Unban,
    Kick,
    Timeout,
    ServerUpdate,
    InviteCreate,
    InviteDelete,
    Emojis,
    Stickers,
}

impl LogEvent {
    pub const ALL: [Self; 38] = [
        Self::Report,
        Self::Appeal,
        Self::Restriction,
        Self::RoleRule,
        Self::AutoPurge,
        Self::MemberJoin,
        Self::MemberLeave,
        Self::Nickname,
        Self::Username,
        Self::Avatar,
        Self::MessageEdit,
        Self::MessageDelete,
        Self::MessageBulkDelete,
        Self::RoleCreate,
        Self::RoleDelete,
        Self::RoleUpdate,
        Self::MemberRoles,
        Self::ChannelCreate,
        Self::ChannelDelete,
        Self::ChannelUpdate,
        Self::ChannelPermissions,
        Self::ThreadCreate,
        Self::ThreadDelete,
        Self::ThreadUpdate,
        Self::VoiceJoin,
        Self::VoiceLeave,
        Self::VoiceMove,
        Self::VoiceModeration,
        Self::VoiceSelf,
        Self::Ban,
        Self::Unban,
        Self::Kick,
        Self::Timeout,
        Self::ServerUpdate,
        Self::InviteCreate,
        Self::InviteDelete,
        Self::Emojis,
        Self::Stickers,
    ];

    /// The key stored in the database and used in the API.
    pub const fn key(self) -> &'static str {
        match self {
            Self::MemberJoin => "member_join",
            Self::Report => "report",
            Self::Appeal => "appeal",
            Self::Restriction => "restriction",
            Self::RoleRule => "role_rule",
            Self::AutoPurge => "autopurge",

            Self::MemberLeave => "member_leave",
            Self::Nickname => "nickname",
            Self::Username => "username",
            Self::Avatar => "avatar",
            Self::MessageEdit => "message_edit",
            Self::MessageDelete => "message_delete",
            Self::MessageBulkDelete => "message_bulk_delete",
            Self::RoleCreate => "role_create",
            Self::RoleDelete => "role_delete",
            Self::RoleUpdate => "role_update",
            Self::MemberRoles => "member_roles",
            Self::ChannelCreate => "channel_create",
            Self::ChannelDelete => "channel_delete",
            Self::ChannelUpdate => "channel_update",
            Self::ChannelPermissions => "channel_permissions",
            Self::ThreadCreate => "thread_create",
            Self::ThreadDelete => "thread_delete",
            Self::ThreadUpdate => "thread_update",
            Self::VoiceJoin => "voice_join",
            Self::VoiceLeave => "voice_leave",
            Self::VoiceMove => "voice_move",
            Self::VoiceModeration => "voice_moderation",
            Self::VoiceSelf => "voice_self",
            Self::Ban => "ban",
            Self::Unban => "unban",
            Self::Kick => "kick",
            Self::Timeout => "timeout",
            Self::ServerUpdate => "server_update",
            Self::InviteCreate => "invite_create",
            Self::InviteDelete => "invite_delete",
            Self::Emojis => "emojis",
            Self::Stickers => "stickers",
        }
    }

    pub fn parse(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|e| e.key() == key)
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::MemberJoin => "Member joined",
            Self::Report => "Report",
            Self::Appeal => "Appeal",
            Self::Restriction => "Restriction",
            Self::RoleRule => "RoleRule",
            Self::AutoPurge => "AutoPurge",

            Self::MemberLeave => "Member left",
            Self::Nickname => "Nickname changed",
            Self::Username => "Username or display name changed",
            Self::Avatar => "Avatar changed",
            Self::MessageEdit => "Message edited",
            Self::MessageDelete => "Message deleted",
            Self::MessageBulkDelete => "Messages bulk deleted",
            Self::RoleCreate => "Role created",
            Self::RoleDelete => "Role deleted",
            Self::RoleUpdate => "Role changed",
            Self::MemberRoles => "Member roles changed",
            Self::ChannelCreate => "Channel created",
            Self::ChannelDelete => "Channel deleted",
            Self::ChannelUpdate => "Channel changed",
            Self::ChannelPermissions => "Channel permissions changed",
            Self::ThreadCreate => "Thread created",
            Self::ThreadDelete => "Thread deleted",
            Self::ThreadUpdate => "Thread changed",
            Self::VoiceJoin => "Joined voice",
            Self::VoiceLeave => "Left voice",
            Self::VoiceMove => "Moved voice",
            Self::VoiceModeration => "Server mute or deafen",
            Self::VoiceSelf => "Self mute or deafen",
            Self::Ban => "Member banned",
            Self::Unban => "Member unbanned",
            Self::Kick => "Member kicked",
            Self::Timeout => "Timeout given or removed",
            Self::ServerUpdate => "Server settings changed",
            Self::InviteCreate => "Invite created",
            Self::InviteDelete => "Invite deleted",
            Self::Emojis => "Emoji added, renamed or removed",
            Self::Stickers => "Sticker added, changed or removed",
        }
    }

    pub const fn category(self) -> LogCategory {
        match self {
            Self::MemberJoin | Self::MemberLeave | Self::Nickname | Self::Username | Self::Avatar => {
                LogCategory::Members
            }
            Self::MessageEdit | Self::MessageDelete | Self::MessageBulkDelete => LogCategory::Messages,
            Self::RoleCreate | Self::RoleDelete | Self::RoleUpdate | Self::MemberRoles => LogCategory::Roles,
            Self::ChannelCreate
            | Self::ChannelDelete
            | Self::ChannelUpdate
            | Self::ChannelPermissions
            | Self::ThreadCreate
            | Self::ThreadDelete
            | Self::ThreadUpdate => LogCategory::Channels,
            Self::VoiceJoin | Self::VoiceLeave | Self::VoiceMove | Self::VoiceModeration | Self::VoiceSelf => {
                LogCategory::Voice
            }
            Self::Ban
            | Self::Unban
            | Self::Kick
            | Self::Timeout
            | Self::Report
            | Self::Appeal
            | Self::Restriction
            | Self::RoleRule
            | Self::AutoPurge => LogCategory::Moderation,
            Self::ServerUpdate => LogCategory::Server,
            Self::InviteCreate | Self::InviteDelete => LogCategory::Invites,
            Self::Emojis | Self::Stickers => LogCategory::Emojis,
        }
    }

    /// The older switch that covers it, if any.
    pub const fn group(self) -> Option<Group> {
        match self {
            Self::MemberJoin | Self::MemberLeave => Some(Group::Members),
            Self::Nickname | Self::Username => Some(Group::Names),
            Self::Ban | Self::Unban => Some(Group::Bans),
            Self::VoiceJoin | Self::VoiceLeave | Self::VoiceMove => Some(Group::Voice),
            _ => None,
        }
    }

    /// Whether Pinchora learns of it from Discord's audit log, which needs the View Audit Log permission.
    pub const fn needs_audit_log(self) -> bool {
        matches!(
            self.category(),
            LogCategory::Roles
                | LogCategory::Channels
                | LogCategory::Server
                | LogCategory::Invites
                | LogCategory::Emojis
        ) || matches!(self, Self::Kick | Self::Timeout)
    }
}

/// The older switches, as `guilds` holds them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Groups {
    pub members: bool,
    pub names: bool,
    pub bans: bool,
    pub voice: bool,
}

impl Default for Groups {
    fn default() -> Self {
        Self {
            members: true,
            names: true,
            bans: true,
            voice: true,
        }
    }
}

impl Groups {
    pub fn get(&self, group: Group) -> bool {
        match group {
            Group::Members => self.members,
            Group::Names => self.names,
            Group::Bans => self.bans,
            Group::Voice => self.voice,
        }
    }

    pub fn set(&mut self, group: Group, on: bool) {
        match group {
            Group::Members => self.members = on,
            Group::Names => self.names = on,
            Group::Bans => self.bans = on,
            Group::Voice => self.voice = on,
        }
    }
}

/// Everything that decides whether and where an event is logged.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogConfig {
    /// The server log's channel (`guilds.log_channel_id`): every category without its own goes here.
    pub default_channel: Option<u64>,
    pub groups: Groups,
    pub channels: HashMap<LogCategory, u64>,
    pub disabled: Vec<LogEvent>,
    pub ignored_channels: Vec<u64>,
    pub ignored_roles: Vec<u64>,
    pub ignored_users: Vec<u64>,
    pub ignore_bots: bool,
}

impl Default for LogConfig {
    fn default() -> Self {
        Self {
            default_channel: None,
            groups: Groups::default(),
            channels: HashMap::new(),
            disabled: vec![LogEvent::VoiceSelf],
            ignored_channels: Vec::new(),
            ignored_roles: Vec::new(),
            ignored_users: Vec::new(),
            ignore_bots: false,
        }
    }
}

/// Who and where an event is about, for the ignore lists. `places` is the channel followed by what it
/// sits under (a thread's channel, a channel's category), as far as it is known.
#[derive(Debug, Clone, Copy, Default)]
pub struct Subject<'a> {
    pub user: Option<u64>,
    pub bot: bool,
    pub roles: &'a [u64],
    pub places: &'a [u64],
}

impl LogConfig {
    /// Whether the event is switched on (its group, then the event itself).
    pub fn enabled(&self, event: LogEvent) -> bool {
        event.group().is_none_or(|g| self.groups.get(g)) && !self.disabled.contains(&event)
    }

    /// The channel of a category: its own, else the server log's.
    pub fn category_channel(&self, category: LogCategory) -> Option<u64> {
        self.channels.get(&category).copied().or(self.default_channel)
    }

    /// Where the event goes, or `None` when it is not logged.
    pub fn channel(&self, event: LogEvent) -> Option<u64> {
        self.category_channel(event.category()).filter(|_| self.enabled(event))
    }

    /// Whether the ignore lists leave this alone.
    pub fn ignores(&self, subject: &Subject<'_>) -> bool {
        (self.ignore_bots && subject.bot)
            || subject.user.is_some_and(|u| self.ignored_users.contains(&u))
            || subject.roles.iter().any(|r| self.ignored_roles.contains(r))
            || subject.places.iter().any(|c| self.ignored_channels.contains(c))
    }

    /// Whether anything is logged at all.
    pub fn any(&self) -> bool {
        LogEvent::ALL.into_iter().any(|e| self.channel(e).is_some())
    }

    /// Switches one event on or off. Switching on an event whose older switch is off turns that switch
    /// on and its other events off, so only the one asked for starts.
    pub fn toggle(&mut self, event: LogEvent, on: bool) {
        if on {
            self.disabled.retain(|e| *e != event);
            if let Some(group) = event.group()
                && !self.groups.get(group)
            {
                self.groups.set(group, true);
                for other in group.events().filter(|e| *e != event) {
                    if !self.disabled.contains(&other) {
                        self.disabled.push(other);
                    }
                }
            }
        } else if !self.disabled.contains(&event) {
            self.disabled.push(event);
        }
        self.disabled.sort();
    }

    /// Switches a whole category on or off.
    pub fn toggle_category(&mut self, category: LogCategory, on: bool) {
        for event in category.events() {
            if on {
                self.disabled.retain(|e| *e != event);
                if let Some(group) = event.group() {
                    self.groups.set(group, true);
                }
            } else if !self.disabled.contains(&event) {
                self.disabled.push(event);
            }
        }
        self.disabled.sort();
    }

    /// One of the older switches, as `/config log-events` sets it: on also brings back the events of
    /// its group that were switched off one by one.
    pub fn set_group(&mut self, group: Group, on: bool) {
        self.groups.set(group, on);
        if on {
            self.disabled.retain(|e| e.group() != Some(group));
        }
    }
}

/// An ignore list, checked: no zero ids, no repeats, not too long.
pub fn check_ignored(field: &'static str, ids: &[u64]) -> Result<Vec<u64>, FieldError> {
    let mut clean: Vec<u64> = Vec::with_capacity(ids.len());
    for &id in ids {
        if id == 0 {
            return Err(FieldError::new(field, "That isn't a Discord id."));
        }
        if !clean.contains(&id) {
            clean.push(id);
        }
    }
    if clean.len() > MAX_IGNORED {
        return Err(FieldError::new(
            field,
            format!("An ignore list holds at most {MAX_IGNORED} entries."),
        ));
    }
    Ok(clean)
}

/// The event keys of a list, for storage.
pub fn keys(events: &[LogEvent]) -> Vec<String> {
    events.iter().map(|e| e.key().to_owned()).collect()
}

/// Events from stored keys; unknown keys (an event since removed) are dropped.
pub fn parse_events(keys: &[String]) -> Vec<LogEvent> {
    let mut events: Vec<LogEvent> = keys.iter().filter_map(|k| LogEvent::parse(k)).collect();
    events.sort();
    events.dedup();
    events
}

#[cfg(test)]
mod tests {
    use crate::admin::logging::*;

    #[test]
    fn keys_are_unique_and_every_category_has_events() {
        for (i, event) in LogEvent::ALL.iter().enumerate() {
            assert_eq!(LogEvent::parse(event.key()), Some(*event));
            assert!(!LogEvent::ALL[i + 1..].iter().any(|e| e.key() == event.key()));
        }
        for category in LogCategory::ALL {
            assert_eq!(LogCategory::parse(category.key()), Some(category));
            assert!(category.events().count() > 0, "{category:?}");
        }
        for group in Group::ALL {
            assert!(group.events().count() >= 2, "{group:?}");
        }
    }

    #[test]
    fn an_event_follows_its_group_then_itself() {
        let mut config = LogConfig {
            default_channel: Some(7),
            ..LogConfig::default()
        };
        assert_eq!(config.channel(LogEvent::MemberJoin), Some(7));
        assert_eq!(config.channel(LogEvent::VoiceSelf), None, "self mutes start off");
        config.groups.members = false;
        assert_eq!(config.channel(LogEvent::MemberJoin), None);
        assert_eq!(config.channel(LogEvent::Avatar), Some(7), "avatars have no group");
        config.channels.insert(LogCategory::Members, 9);
        assert_eq!(config.channel(LogEvent::Avatar), Some(9));
        config.default_channel = None;
        assert_eq!(config.channel(LogEvent::MessageEdit), None, "no channel, no log");
        assert!(config.any());
    }

    #[test]
    fn switching_one_event_on_under_a_closed_group_starts_only_that_one() {
        let mut config = LogConfig {
            default_channel: Some(7),
            ..LogConfig::default()
        };
        config.groups.voice = false;
        config.toggle(LogEvent::VoiceMove, true);
        assert!(config.groups.voice);
        assert!(config.enabled(LogEvent::VoiceMove));
        assert!(!config.enabled(LogEvent::VoiceJoin) && !config.enabled(LogEvent::VoiceLeave));
        config.toggle(LogEvent::VoiceMove, false);
        assert!(!config.enabled(LogEvent::VoiceMove));
        config.set_group(Group::Voice, true);
        assert!(config.enabled(LogEvent::VoiceJoin) && config.enabled(LogEvent::VoiceMove));
        assert!(!config.enabled(LogEvent::VoiceSelf), "not in the group");
    }

    #[test]
    fn a_category_switch_covers_every_event_in_it() {
        let mut config = LogConfig::default();
        config.groups.bans = false;
        config.toggle_category(LogCategory::Moderation, true);
        assert!(LogCategory::Moderation.events().all(|e| config.enabled(e)));
        config.toggle_category(LogCategory::Moderation, false);
        assert!(LogCategory::Moderation.events().all(|e| !config.enabled(e)));
        assert!(config.groups.bans, "the group stays, the events are off one by one");
    }

    #[test]
    fn ignore_lists_cover_members_roles_bots_and_places() {
        let config = LogConfig {
            ignored_channels: vec![30],
            ignored_roles: vec![20],
            ignored_users: vec![10],
            ignore_bots: true,
            ..LogConfig::default()
        };
        let none = Subject::default();
        assert!(!config.ignores(&none));
        assert!(config.ignores(&Subject { user: Some(10), ..none }));
        assert!(config.ignores(&Subject { bot: true, ..none }));
        assert!(config.ignores(&Subject {
            roles: &[1, 20],
            ..none
        }));
        assert!(
            config.ignores(&Subject {
                places: &[5, 30],
                ..none
            }),
            "a thread under an ignored channel"
        );
        assert!(!config.ignores(&Subject {
            user: Some(11),
            roles: &[21],
            places: &[31],
            ..none
        }));
    }

    #[test]
    fn ignore_lists_are_checked() {
        assert_eq!(check_ignored("users", &[1, 2, 1]).unwrap(), [1, 2]);
        assert!(check_ignored("users", &[0]).is_err());
        let many: Vec<u64> = (1..=51).collect();
        assert!(check_ignored("users", &many).is_err());
        assert_eq!(
            parse_events(&["kick".into(), "nope".into(), "ban".into()]),
            [LogEvent::Ban, LogEvent::Kick]
        );
    }
}
