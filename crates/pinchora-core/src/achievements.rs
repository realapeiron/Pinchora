//! Kinds and limits shared by the bot and dashboard.

use crate::modules::Module;

pub const MAX_ACHIEVEMENTS: usize = 25;
pub const ICONS: [&str; 4] = ["star", "shell", "crab", "heart"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Messages,
    VoiceMinutes,
    Level,
    DaysActive,
    Invites,
    StarsReceived,
    TicketsClosed,
    Reputation,
    GamesWon,
    FishCaught,
    Custom,
}

impl Kind {
    pub const ALL: [Self; 11] = [
        Self::Messages,
        Self::VoiceMinutes,
        Self::Level,
        Self::DaysActive,
        Self::Invites,
        Self::StarsReceived,
        Self::TicketsClosed,
        Self::Reputation,
        Self::GamesWon,
        Self::FishCaught,
        Self::Custom,
    ];

    pub const fn key(self) -> &'static str {
        match self {
            Self::Messages => "messages",
            Self::VoiceMinutes => "voice_minutes",
            Self::Level => "level",
            Self::DaysActive => "days_active",
            Self::Invites => "invites",
            Self::StarsReceived => "stars_received",
            Self::TicketsClosed => "tickets_closed",
            Self::Reputation => "reputation",
            Self::GamesWon => "games_won",
            Self::FishCaught => "fish_caught",
            Self::Custom => "custom",
        }
    }

    pub fn parse(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.key() == key)
    }

    pub const fn module(self) -> Option<Module> {
        match self {
            Self::Messages | Self::VoiceMinutes | Self::DaysActive => Some(Module::Stats),
            Self::Level => Some(Module::Leveling),
            Self::Invites => Some(Module::Invites),
            Self::StarsReceived => Some(Module::Starboard),
            Self::TicketsClosed => Some(Module::Tickets),
            Self::Reputation => Some(Module::Economy),
            Self::GamesWon => Some(Module::Profile),
            Self::FishCaught => Some(Module::Fishing),
            Self::Custom => None,
        }
    }
}

pub fn valid_name(value: &str) -> bool {
    !value.trim().is_empty() && value.chars().count() <= 40
}

pub fn valid_description(value: &str) -> bool {
    value.chars().count() <= 100
}

pub fn valid_icon(value: &str) -> bool {
    ICONS.contains(&value)
        || value.strip_prefix("b_").is_some_and(|id| {
            !id.is_empty()
                && id.len() <= 38
                && id
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
        })
}
