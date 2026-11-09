use super::*;

/// Where a module's settings are shown together. Roles, Safety, Community and Feeds start empty: the
/// dashboard shows a group once a module belongs to it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Group {
    Server,
    Players,
    Home,
    Games,
    Roles,
    Safety,
    Community,
    Feeds,
}

impl Group {
    pub const ALL: [Self; 8] = [
        Self::Server,
        Self::Players,
        Self::Home,
        Self::Games,
        Self::Roles,
        Self::Safety,
        Self::Community,
        Self::Feeds,
    ];

    pub const fn key(self) -> &'static str {
        match self {
            Self::Server => "server",
            Self::Players => "players",
            Self::Home => "home",
            Self::Games => "games",
            Self::Roles => "roles",
            Self::Safety => "safety",
            Self::Community => "community",
            Self::Feeds => "feeds",
        }
    }

    /// [`Group::label`] in `lang`.
    pub fn name(self, lang: Lang) -> String {
        t!(lang, &format!("module-group.{}", self.key()))
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Server => "Server tools",
            Self::Players => "Players",
            Self::Home => "Home, pets and fishing",
            Self::Games => "Games",
            Self::Roles => "Roles",
            Self::Safety => "Safety",
            Self::Community => "Community",
            Self::Feeds => "Feeds",
        }
    }
}

/// Whether a module can be limited to channels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    /// Commands, buttons or chat in a channel: the module can be switched off in some of them.
    Channels,
    /// Something that happens to the server as a whole (a join, a log line): on or off, nothing more.
    Server,
}

/// A part of Pinchora that can be switched off.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Module {
    Leveling,
    Points,
    Welcome,
    Goodbye,
    Log,
    Moderation,
    RolePanels,
    Store,
    Suggestions,
    Polls,
    Giveaways,
    CustomCommands,
    Profile,
    Economy,
    Shop,
    Quests,
    Social,
    House,
    Plaza,
    Pets,
    Fishing,
    Duels,
    BoardGames,
    Puzzles,
    Party,
    PartyGames,
    Collection,
    Gambling,
    TextCommands,
    AutoRoles,
    ColorRoles,
    Stats,
    Tickets,
    AutoMod,
    Feeds,
    TempVoice,
    Info,
    Starboard,
    Invites,
    Verification,
    AntiRaid,
    AntiNuke,
    Birthdays,
    Achievements,
    Utility,
    Events,
    CommandControl,
    Qotd,
    Counting,
    Forms,
    Modmail,
    Roblox,
    Branding,
    GameTrackers,
    Workflows,
}

impl Module {
    /// Every module, in the order the dashboard lists them within a group.
    pub const ALL: [Self; 55] = [
        Self::Leveling,
        Self::Points,
        Self::Welcome,
        Self::Goodbye,
        Self::Log,
        Self::Moderation,
        Self::RolePanels,
        Self::Store,
        Self::Suggestions,
        Self::Polls,
        Self::Giveaways,
        Self::CustomCommands,
        Self::Profile,
        Self::Economy,
        Self::Shop,
        Self::Quests,
        Self::Social,
        Self::House,
        Self::Plaza,
        Self::Pets,
        Self::Fishing,
        Self::Duels,
        Self::BoardGames,
        Self::Puzzles,
        Self::Party,
        Self::PartyGames,
        Self::Collection,
        Self::Gambling,
        Self::TextCommands,
        Self::AutoRoles,
        Self::ColorRoles,
        Self::Stats,
        Self::Tickets,
        Self::AutoMod,
        Self::Feeds,
        Self::TempVoice,
        Self::Info,
        Self::Starboard,
        Self::Invites,
        Self::Verification,
        Self::AntiRaid,
        Self::AntiNuke,
        Self::Birthdays,
        Self::Achievements,
        Self::Utility,
        Self::Events,
        Self::CommandControl,
        Self::Qotd,
        Self::Counting,
        Self::Forms,
        Self::Modmail,
        Self::Roblox,
        Self::Branding,
        Self::GameTrackers,
        Self::Workflows,
    ];

    /// The key stored in the database and used in the API.
    pub const fn key(self) -> &'static str {
        match self {
            Self::Leveling => "leveling",
            Self::Points => "points",
            Self::Welcome => "welcome",
            Self::Goodbye => "goodbye",
            Self::Log => "log",
            Self::Moderation => "moderation",
            Self::RolePanels => "role_panels",
            Self::Store => "store",
            Self::Suggestions => "suggestions",
            Self::Polls => "polls",
            Self::Giveaways => "giveaways",
            Self::CustomCommands => "custom_commands",
            Self::Workflows => "workflows",
            Self::Profile => "profile",
            Self::Economy => "economy",
            Self::Shop => "shop",
            Self::Quests => "quests",
            Self::Social => "social",
            Self::House => "house",
            Self::Plaza => "plaza",
            Self::Pets => "pets",
            Self::Fishing => "fishing",
            Self::Duels => "duels",
            Self::BoardGames => "board_games",
            Self::Puzzles => "puzzles",
            Self::Party => "party",
            Self::PartyGames => "party_games",
            Self::Collection => "collection",
            Self::Gambling => "gambling",
            Self::TextCommands => "text_commands",
            Self::AutoRoles => "autoroles",
            Self::ColorRoles => "color_roles",
            Self::Stats => "stats",
            Self::Tickets => "tickets",
            Self::AutoMod => "automod",
            Self::Feeds => "feeds",
            Self::TempVoice => "temp_voice",
            Self::Info => "info",
            Self::Starboard => "starboard",
            Self::Invites => "invites",
            Self::Verification => "verification",
            Self::AntiRaid => "anti_raid",
            Self::AntiNuke => "anti_nuke",
            Self::Birthdays => "birthdays",
            Self::Achievements => "achievements",
            Self::Utility => "utility",
            Self::Events => "events",
            Self::CommandControl => "command_control",
            Self::Qotd => "qotd",
            Self::Counting => "counting",
            Self::Forms => "forms",
            Self::Modmail => "modmail",
            Self::Roblox => "roblox",
            Self::Branding => "branding",
            Self::GameTrackers => "game_trackers",
        }
    }

    pub fn parse(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|m| m.key() == key)
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Leveling => "Leveling",
            Self::Points => "Chat points",
            Self::Welcome => "Welcome cards",
            Self::Goodbye => "Goodbye messages",
            Self::Log => "Server log",
            Self::Moderation => "Moderation",
            Self::RolePanels => "Role panels",
            Self::Store => "Points store",
            Self::Suggestions => "Suggestions",
            Self::Polls => "Polls",
            Self::Giveaways => "Giveaways",
            Self::CustomCommands => "Custom commands",
            Self::Workflows => "Workflows",
            Self::Profile => "Profiles and ranks",
            Self::Economy => "Shell economy",
            Self::Shop => "Shops and gifts",
            Self::Quests => "Daily quests",
            Self::Social => "Friends and hugs",
            Self::House => "Houses",
            Self::Plaza => "Plaza",
            Self::Pets => "Pets",
            Self::Fishing => "Fishing",
            Self::Duels => "Duels",
            Self::BoardGames => "Board and puzzle games",
            Self::Puzzles => "Word and trivia games",
            Self::Party => "Party games",
            Self::PartyGames => "Word party games",
            Self::Collection => "Critter drops and beachcombing",
            Self::Gambling => "Gambling-style games",
            Self::TextCommands => "Text commands",
            Self::AutoRoles => "Auto roles",
            Self::ColorRoles => "Colour roles",
            Self::Stats => "Stats",
            Self::Tickets => "Tickets",
            Self::AutoMod => "AutoMod",
            Self::Feeds => "Social feeds",
            Self::TempVoice => "Temp voice",
            Self::Info => "Info commands",
            Self::Starboard => "Starboard",
            Self::Invites => "Invites",
            Self::Verification => "Verification",
            Self::AntiRaid => "Raid protection",
            Self::AntiNuke => "Anti-nuke",
            Self::Birthdays => "Birthdays",
            Self::Achievements => "Achievements",
            Self::Utility => "Messages",
            Self::Events => "Events",
            Self::CommandControl => "Command control",
            Self::Qotd => "Question of the day",
            Self::Counting => "Counting",
            Self::Forms => "Application forms",
            Self::Modmail => "Modmail",
            Self::Roblox => "Roblox",
            Self::Branding => "Branding",
            Self::GameTrackers => "Game trackers",
        }
    }

    /// [`Module::label`] in `lang`.
    pub fn name(self, lang: Lang) -> String {
        t!(lang, &format!("module.{}", self.key()))
    }

    pub const fn group(self) -> Group {
        match self {
            Self::Leveling
            | Self::Points
            | Self::Welcome
            | Self::Goodbye
            | Self::Log
            | Self::Store
            | Self::Suggestions
            | Self::Polls
            | Self::Giveaways
            | Self::CustomCommands
            | Self::Workflows
            | Self::TextCommands => Group::Server,
            Self::Profile | Self::Economy | Self::Shop | Self::Quests | Self::Social => Group::Players,
            Self::House | Self::Plaza | Self::Pets | Self::Fishing => Group::Home,
            Self::Duels
            | Self::BoardGames
            | Self::Puzzles
            | Self::Party
            | Self::PartyGames
            | Self::Collection
            | Self::Gambling => Group::Games,
            Self::Moderation => Group::Safety,
            Self::RolePanels | Self::AutoRoles | Self::ColorRoles => Group::Roles,
            Self::Stats | Self::Invites => Group::Server,
            Self::Tickets => Group::Community,
            Self::AutoMod => Group::Safety,
            Self::Feeds => Group::Feeds,
            Self::GameTrackers => Group::Feeds,
            Self::TempVoice => Group::Community,
            Self::Info => Group::Server,
            Self::Starboard => Group::Community,
            Self::Verification => Group::Safety,
            Self::AntiRaid => Group::Safety,
            Self::AntiNuke => Group::Safety,
            Self::Birthdays => Group::Community,
            Self::Achievements => Group::Players,
            Self::Utility => Group::Server,
            Self::Events => Group::Community,
            Self::CommandControl => Group::Server,
            Self::Qotd | Self::Counting | Self::Forms | Self::Modmail => Group::Community,
            Self::Roblox => Group::Feeds,
            Self::Branding => Group::Server,
        }
    }

    pub const fn scope(self) -> Scope {
        match self {
            Self::Welcome | Self::Goodbye | Self::Log | Self::Moderation | Self::Gambling | Self::AutoRoles => {
                Scope::Server
            }
            Self::AutoMod => Scope::Server,
            Self::Workflows => Scope::Server,
            Self::AntiNuke => Scope::Server,
            Self::Feeds => Scope::Server,
            Self::GameTrackers => Scope::Server,
            Self::TempVoice => Scope::Server,
            Self::Invites => Scope::Server,
            Self::Verification => Scope::Server,
            Self::AntiRaid => Scope::Server,
            Self::Birthdays => Scope::Server,
            Self::Achievements => Scope::Server,
            Self::CommandControl => Scope::Server,
            Self::Modmail => Scope::Server,
            Self::Roblox => Scope::Server,
            _ => Scope::Channels,
        }
    }

    /// The `guilds` column that already holds this module's on/off switch, when it has one: the
    /// switch lives there and nowhere else, so there is never a second one for the same thing.
    pub const fn setting(self) -> Option<&'static str> {
        match self {
            Self::Leveling => Some("leveling_enabled"),
            Self::Points => Some("points_enabled"),
            Self::Welcome => Some("welcome_enabled"),
            Self::Gambling => Some("gambling_enabled"),
            _ => None,
        }
    }

    /// The module a slash command belongs to: `name` is the command, `sub` its first subcommand or
    /// subcommand group. `None` for the commands in [`NEVER_GATED`] and for anything unknown.
    pub fn of_command(name: &str, sub: Option<&str>) -> Option<Self> {
        Some(match name {
            "profile" | "rank" | "leaderboard" | "bio" | "customize" | "character" | "wrapped" => Self::Profile,
            "economyv2" | "daily" | "weekly" | "monthly" | "balance" | "give" | "rep" => Self::Economy,
            "house" => Self::House,
            "shop" | "inventory" | "gift" | "stall" => Self::Shop,
            "pet" => Self::Pets,
            "fish" => Self::Fishing,
            "quests" => Self::Quests,
            "plaza" => Self::Plaza,
            "points" | "store" => Self::Store,
            "roles" => Self::RolePanels,
            "modtools" | "Report" | "ban" | "unban" | "kick" | "timeout" | "untimeout" | "warn" | "warnings"
            | "unwarn" | "purge" | "slowmode" | "lock" | "unlock" | "nick" | "role" | "case" | "reason" => {
                Self::Moderation
            }
            "autoroles" => Self::AutoRoles,
            "color" => Self::ColorRoles,
            "partner" | "friends" | "hug" | "pat" | "wave" | "highfive" => Self::Social,
            "giveaway" => Self::Giveaways,
            "tag" | "customcommand" => Self::CustomCommands,
            "workflow" => Self::Workflows,
            "suggest" | "suggestion" => Self::Suggestions,
            "poll" => Self::Polls,
            "feeds" => Self::Feeds,
            "game" | "trackers" => Self::GameTrackers,
            "info" => Self::Info,
            "xp" => Self::Leveling,
            "dex" | "beachcomb" | "crabpot" | "critterdrop" => Self::Collection,
            "surge" => Self::Party,
            "party" => Self::PartyGames,
            "stats" | "counters" => Self::Stats,
            "ticket" | "tickets" | "Open ticket" => Self::Tickets,
            "starboard" => Self::Starboard,
            "invites" | "inviteadmin" => Self::Invites,
            "afk" | "sticky" | "schedule" | "highlight" | "checklist" | "autopost" => Self::Utility,
            "anniversary" => Self::Birthdays,
            "confess" => Self::Modmail,
            "duel" => match sub {
                Some("share") => Self::Party,
                Some("tictactoe" | "connect4" | "rps") => Self::Duels,
                Some("seafleet" | "checkers" | "chess" | "othello") => Self::BoardGames,
                _ => return None,
            },
            "play" => match sub {
                Some(
                    "hangman" | "wordle" | "trivia" | "memory" | "minesweeper" | "higherlower" | "anagram" | "typing"
                    | "guessnumber" | "kelpmaze" | "gemtide",
                ) => Self::Puzzles,
                Some("picross" | "2048" | "reefmind") => Self::BoardGames,
                Some("derby" | "heist" | "shells") => Self::Party,
                Some("stats") => Self::Profile,
                _ => return None,
            },
            "automod" => Self::AutoMod,
            "antinuke" => Self::AntiNuke,
            "voice" => Self::TempVoice,
            "verify" | "verification" => Self::Verification,
            "raid" => Self::AntiRaid,
            "birthday" | "birthdays" => Self::Birthdays,
            "achievements" | "achievementadmin" => Self::Achievements,
            "event" => Self::Events,
            "community" => match sub {
                Some("qotd") => Self::Qotd,
                Some("counting") => Self::Counting,
                Some("forms") => Self::Forms,
                Some("modmail") => Self::Modmail,
                _ => return None,
            },
            "qotd" => Self::Qotd,
            "counting" => Self::Counting,
            "form" => Self::Forms,
            "modmail" => Self::Modmail,
            "roblox" | "robloxadmin" => Self::Roblox,
            "branding" => Self::Branding,
            _ => return None,
        })
    }

    /// The module a button, select or modal belongs to, by the `prefix` of its custom id.
    pub fn of_component_prefix(prefix: &str) -> Option<Self> {
        Some(match prefix {
            "lb" | "ch" | "ca" | "cz" | "ti" | "cs" | "sw" => Self::Profile,
            "he" | "hv" | "hu" | "dw" | "df" | "dn" | "gb" | "rc" => Self::House,
            "pe" | "pd" => Self::Plaza,
            "mk" | "cb" | "cp" | "iv" | "sl" | "su" | "sb" | "gf" => Self::Shop,
            "pc" | "pa" | "ps" | "pn" | "py" => Self::Pets,
            "fr" | "fx" | "fz" => Self::Fishing,
            "qc" => Self::Quests,
            "tt" | "c4" | "rp" => Self::Duels,
            "bs" | "ck" | "cc" | "of" | "pz" | "tz" => Self::BoardGames,
            "d9c" => Self::BoardGames,
            "d9s" => Self::Puzzles,
            "d9p" => Self::PartyGames,
            "hm" | "wo" | "tv" | "mm" | "mi" | "yl" | "ag" | "yt" | "gn" => Self::Puzzles,
            "dy" | "sh" | "sz" | "ss" | "hs" => Self::Party,
            "hl" | "ac" | "cn" | "wx" | "wy" | "td" | "ni" => Self::PartyGames,
            "cd" | "mb" | "bc" | "kp" => Self::Collection,
            "st" => Self::Store,
            "rr" | "rb" | "rs" => Self::RolePanels,
            "cr" => Self::ColorRoles,
            "pt" | "fd" | "hg" => Self::Social,
            "gw" => Self::Giveaways,
            "tg" | "cm" | "ms" => Self::CustomCommands,
            "wf" | "wk" => Self::Workflows,
            "sg" => Self::Suggestions,
            "pl" => Self::Polls,
            "sc" => Self::Stats,
            "tk" | "tp" | "tf" | "tr" => Self::Tickets,
            "am" | "ax" => Self::AutoMod,
            "vf" | "cq" => Self::Verification,
            "rd" => Self::AntiRaid,
            "an" | "vp" => Self::AntiNuke,
            "ah" => Self::Achievements,
            "sn" => Self::Feeds,
            "vc" | "vo" => Self::TempVoice,
            "nf" => Self::Info,
            "xi" => Self::Leveling,
            "sr" => Self::Starboard,
            "in" | "tl" => Self::Invites,
            "bd" => Self::Birthdays,
            "af" | "sk" | "sm" => Self::Utility,
            "ev" => Self::Events,
            "qd" => Self::Qotd,
            "ct" => Self::Counting,
            "fm" => Self::Forms,
            "mml" | "cv" => Self::Modmail,
            "rx" | "rk" => Self::Roblox,
            "wb" => Self::Branding,
            "gt" | "mc" => Self::GameTrackers,
            _ => return None,
        })
    }
}
