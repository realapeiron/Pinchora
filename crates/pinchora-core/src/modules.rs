//! Which parts of Pinchora a server uses, and where: the modules a manager can switch off for the
//! whole server or limit to some channels. Pure: the bot, `/config modules` and the dashboard all
//! decide with these rules.

use std::collections::HashMap;

use crate::{i18n::Lang, settings::FieldError, t};

/// How many channels one module may name.
pub const MAX_CHANNELS: usize = 50;

/// Commands that stay available whatever a server switches off: the way back in, the basics, and
/// what is the player's own business rather than the server's (their privacy and data, and the
/// reminders they asked for).
pub const NEVER_GATED: [&str; 6] = ["config", "help", "language", "ping", "privacy", "remind"];

/// The custom-id prefixes of the buttons and menus the [`NEVER_GATED`] commands send: they have no
/// module either.
pub const NEVER_GATED_PREFIXES: [&str; 3] = ["hp", "pr", "rm"];

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
    Info,
}

impl Module {
    /// Every module, in the order the dashboard lists them within a group.
    pub const ALL: [Self; 29] = [
        Self::Leveling,
        Self::Points,
        Self::Welcome,
        Self::Goodbye,
        Self::Log,
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
        Self::Info,
    ];

    /// The key stored in the database and used in the API.
    pub const fn key(self) -> &'static str {
        match self {
            Self::Leveling => "leveling",
            Self::Points => "points",
            Self::Welcome => "welcome",
            Self::Goodbye => "goodbye",
            Self::Log => "log",
            Self::RolePanels => "role_panels",
            Self::Store => "store",
            Self::Suggestions => "suggestions",
            Self::Polls => "polls",
            Self::Giveaways => "giveaways",
            Self::CustomCommands => "custom_commands",
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
            Self::Info => "info",
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
            Self::RolePanels => "Role panels",
            Self::Store => "Points store",
            Self::Suggestions => "Suggestions",
            Self::Polls => "Polls",
            Self::Giveaways => "Giveaways",
            Self::CustomCommands => "Custom commands",
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
            Self::Info => "Info commands",
        }
    }

    /// [`Module::label`] in `lang`.
    pub fn name(self, lang: Lang) -> String {
        t!(lang, &format!("module.{}", self.key()))
    }

    /// One line on what the module covers, for the dashboard and `/config modules show`.
    pub const fn blurb(self) -> &'static str {
        match self {
            Self::Leveling => "Chat and voice XP, levels and level-up messages.",
            Self::Points => "Server points earned by chatting.",
            Self::Welcome => "The welcome card for new members.",
            Self::Goodbye => "The message when a member leaves.",
            Self::Log => "Joins, name changes, bans and voice in the log channel.",
            Self::RolePanels => "/roles and the role buttons.",
            Self::Store => "/store and /points: spending and giving server points.",
            Self::Suggestions => "/suggest and its voting buttons.",
            Self::Polls => "/poll and its voting buttons.",
            Self::Giveaways => "/giveaway and its entry button.",
            Self::CustomCommands => {
                "/tag and /customcommand, and the auto-responders (joins, level-ups, roles, keywords)."
            }
            Self::Profile => "/profile, /rank, /leaderboard, /bio, /customize, /character, /wrapped and game stats.",
            Self::Economy => "/daily, /weekly, /monthly, /balance, /give and /rep.",
            Self::Shop => "/shop, /inventory, /gift and player stalls.",
            Self::Quests => "/quests.",
            Self::Social => "/partner, /friends, /hug, /pat, /wave and /highfive.",
            Self::House => "/house: rooms, decorating, visits, guestbooks and the room contest.",
            Self::Plaza => "/plaza: the server's shared plaza.",
            Self::Pets => "/pet: adopting, caring and trips.",
            Self::Fishing => "/fish: casting, gear, selling and the fish book.",
            Self::Duels => "/duel tic-tac-toe, connect four and rock paper scissors.",
            Self::BoardGames => "/duel seafleet, chess and checkers, and /play picross and 2048.",
            Self::Puzzles => "/play hangman, wordle and trivia.",
            Self::Party => "Crab derby, share or grab, treasure heist, shell game and storm surges.",
            Self::PartyGames => {
                "/party: hot pearl, acronym tide, postcard captions, wordle race, would you rather and truth or dare."
            }
            Self::Collection => "/dex, /beachcomb, /crabpot, /critterdrop and drops that wash up in chat.",
            Self::Gambling => "Games that stake shells on chance: the shell game and staked derbies.",
            Self::TextCommands => {
                "Commands typed as messages, with the prefix or an @mention. Slash commands are not affected."
            }
            Self::Info => "Server, member, role, channel, emoji and invite information.",
        }
    }

    pub const fn group(self) -> Group {
        match self {
            Self::Leveling
            | Self::Points
            | Self::Welcome
            | Self::Goodbye
            | Self::Log
            | Self::RolePanels
            | Self::Store
            | Self::Suggestions
            | Self::Polls
            | Self::Giveaways
            | Self::CustomCommands
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
            Self::Info => Group::Server,
        }
    }

    pub const fn scope(self) -> Scope {
        match self {
            Self::Welcome | Self::Goodbye | Self::Log | Self::Gambling => Scope::Server,
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
            "daily" | "weekly" | "monthly" | "balance" | "give" | "rep" => Self::Economy,
            "house" => Self::House,
            "shop" | "inventory" | "gift" | "stall" => Self::Shop,
            "pet" => Self::Pets,
            "fish" => Self::Fishing,
            "quests" => Self::Quests,
            "plaza" => Self::Plaza,
            "points" | "store" => Self::Store,
            "roles" => Self::RolePanels,
            "partner" | "friends" | "hug" | "pat" | "wave" | "highfive" => Self::Social,
            "giveaway" => Self::Giveaways,
            "tag" | "customcommand" => Self::CustomCommands,
            "suggest" | "suggestion" => Self::Suggestions,
            "poll" => Self::Polls,
            "info" => Self::Info,
            "xp" => Self::Leveling,
            "dex" | "beachcomb" | "crabpot" | "critterdrop" => Self::Collection,
            "surge" => Self::Party,
            "party" => Self::PartyGames,
            "duel" => match sub {
                Some("share") => Self::Party,
                Some("tictactoe" | "connect4" | "rps") => Self::Duels,
                Some("seafleet" | "checkers" | "chess") => Self::BoardGames,
                _ => return None,
            },
            "play" => match sub {
                Some("hangman" | "wordle" | "trivia") => Self::Puzzles,
                Some("picross" | "2048") => Self::BoardGames,
                Some("derby" | "heist" | "shells") => Self::Party,
                Some("stats") => Self::Profile,
                _ => return None,
            },
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
            "bs" | "ck" | "cc" | "pz" | "tz" => Self::BoardGames,
            "hm" | "wo" | "tv" => Self::Puzzles,
            "dy" | "sh" | "sz" | "ss" | "hs" => Self::Party,
            "hl" | "ac" | "cn" | "wx" | "wy" | "td" => Self::PartyGames,
            "cd" | "mb" | "bc" | "kp" => Self::Collection,
            "st" => Self::Store,
            "rr" => Self::RolePanels,
            "pt" | "fd" | "hg" => Self::Social,
            "gw" => Self::Giveaways,
            "tg" | "cm" => Self::CustomCommands,
            "sg" => Self::Suggestions,
            "pl" => Self::Polls,
            "nf" => Self::Info,
            "xi" => Self::Leveling,
            _ => return None,
        })
    }
}

/// Where in a server a module works.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ChannelMode {
    /// Everywhere.
    #[default]
    All,
    /// Only in the listed channels (and what sits under them).
    Only,
    /// Everywhere but the listed channels (and what sits under them).
    Except,
}

impl ChannelMode {
    pub const ALL: [Self; 3] = [Self::All, Self::Only, Self::Except];

    pub const fn key(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::Only => "only",
            Self::Except => "except",
        }
    }

    pub fn parse(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|m| m.key() == key)
    }
}

/// What a module does in a server. A module nobody configured is on everywhere.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModuleRule {
    pub enabled: bool,
    pub mode: ChannelMode,
    /// The channels (or categories) `mode` talks about; empty for [`ChannelMode::All`].
    pub channels: Vec<u64>,
}

impl Default for ModuleRule {
    fn default() -> Self {
        Self {
            enabled: true,
            mode: ChannelMode::All,
            channels: Vec::new(),
        }
    }
}

/// Whether a module may act at one place.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Access {
    Allowed,
    /// Switched off for the whole server.
    ServerOff,
    /// Switched off in this channel.
    ChannelOff,
}

impl ModuleRule {
    /// Whether deciding needs to know what a channel sits under.
    pub fn needs_lineage(&self) -> bool {
        self.enabled && self.mode != ChannelMode::All
    }

    /// Whether `channels` names `id`.
    pub fn names(&self, id: u64) -> bool {
        self.channels.contains(&id)
    }

    /// The verdict at a place described by `lineage`: the channel followed by what it sits under
    /// (its category, or a thread's channel then that channel's category). An empty lineage is
    /// somewhere unknown, which only [`ChannelMode::Except`] lets through.
    pub fn access(&self, lineage: &[u64]) -> Access {
        if !self.enabled {
            return Access::ServerOff;
        }
        let listed = lineage.iter().any(|id| self.names(*id));
        match self.mode {
            ChannelMode::All => Access::Allowed,
            ChannelMode::Only if listed => Access::Allowed,
            ChannelMode::Except if !listed => Access::Allowed,
            ChannelMode::Only | ChannelMode::Except => Access::ChannelOff,
        }
    }
}

/// The rules a server set, by module.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GuildModules {
    rules: HashMap<Module, ModuleRule>,
}

impl GuildModules {
    pub fn new(rules: impl IntoIterator<Item = (Module, ModuleRule)>) -> Self {
        Self {
            rules: rules.into_iter().collect(),
        }
    }

    /// What `module` does here. Where a module's switch lives in a `guilds` column the row's own
    /// `enabled` means nothing, so it reads as on.
    pub fn rule(&self, module: Module) -> ModuleRule {
        let mut rule = self.rules.get(&module).cloned().unwrap_or_default();
        if module.setting().is_some() {
            rule.enabled = true;
        }
        rule
    }

    /// Every module's rule, in [`Module::ALL`] order.
    pub fn rules(&self) -> impl Iterator<Item = (Module, ModuleRule)> + '_ {
        Module::ALL.into_iter().map(|m| (m, self.rule(m)))
    }
}

/// The channels of a rule, checked: `mode` and `channels` agree, the list is short, has no
/// repeats, and only modules that can be limited to channels are. Returns the list to store
/// (empty for [`ChannelMode::All`]).
pub fn check_channels(module: Module, mode: ChannelMode, channels: &[u64]) -> Result<Vec<u64>, FieldError> {
    if mode == ChannelMode::All {
        return Ok(Vec::new());
    }
    if module.scope() == Scope::Server {
        return Err(FieldError::new(
            "channel_mode",
            format!(
                "{} works for the whole server, so it can't be limited to channels.",
                module.label()
            ),
        ));
    }
    let mut clean: Vec<u64> = Vec::with_capacity(channels.len());
    for &id in channels {
        if id == 0 {
            return Err(FieldError::new("channels", "That isn't a Discord id."));
        }
        if !clean.contains(&id) {
            clean.push(id);
        }
    }
    if clean.is_empty() {
        return Err(FieldError::new(
            "channels",
            "Pick at least one channel, or choose all channels.",
        ));
    }
    if clean.len() > MAX_CHANNELS {
        return Err(FieldError::new(
            "channels",
            format!("A module can name at most {MAX_CHANNELS} channels."),
        ));
    }
    Ok(clean)
}

/// What `/config modules channels` does with one channel: starts a list in `mode` (or adds to the
/// one already in that mode), or takes `channel` off it. Returns the mode and list to check and
/// store: removing the last channel goes back to all channels.
pub fn edit_channels(
    now: &ModuleRule,
    mode: ChannelMode,
    channel: Option<u64>,
    remove: bool,
) -> Result<(ChannelMode, Vec<u64>), FieldError> {
    if mode == ChannelMode::All {
        return Ok((ChannelMode::All, Vec::new()));
    }
    let Some(channel) = channel else {
        return Err(FieldError::new(
            "channel",
            "Pick a channel to add, or choose all channels.",
        ));
    };
    // Taking a channel off a list the module does not use changes nothing (rather than wiping the one it does).
    if remove && now.mode != mode {
        return Ok((now.mode, now.channels.clone()));
    }
    // A list in another mode means something else: switching the mode starts over.
    let mut list = if now.mode == mode {
        now.channels.clone()
    } else {
        Vec::new()
    };
    if remove {
        list.retain(|id| *id != channel);
    } else if !list.contains(&channel) {
        list.push(channel);
    }
    if list.is_empty() {
        return Ok((ChannelMode::All, Vec::new()));
    }
    Ok((mode, list))
}

/// The sentence that says a module is off, for the person who tried it. `manager` adds the way back.
pub fn refusal(module: Module, access: Access, manager: bool, lang: Lang) -> String {
    let mut text = match access {
        Access::ServerOff | Access::Allowed => t!(lang, "gate.server-off"),
        Access::ChannelOff => t!(lang, "gate.channel-off"),
    };
    if manager {
        text.push_str(&t!(lang, "gate.how-to-enable", module = module.name(lang)));
    }
    text
}

/// A rule in a few words, for lists: "on", "off", "only in 3 channels", "off in 2 channels".
pub fn summary(rule: &ModuleRule) -> String {
    if !rule.enabled {
        return "off".to_owned();
    }
    let n = rule.channels.len();
    let channels = if n == 1 { "channel" } else { "channels" };
    match rule.mode {
        ChannelMode::All => "on".to_owned(),
        ChannelMode::Only => format!("only in {n} {channels}"),
        ChannelMode::Except => format!("off in {n} {channels}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_are_unique_and_parse_back() {
        for (i, module) in Module::ALL.iter().enumerate() {
            assert_eq!(Module::parse(module.key()), Some(*module));
            assert!(!Module::ALL[i + 1..].iter().any(|m| m.key() == module.key()));
            assert!(!module.label().is_empty() && !module.blurb().is_empty());
        }
        assert_eq!(Module::parse("nope"), None);
        // The four newer groups are filled by the phase-4 packages; until then they are empty and hidden.
        for group in [Group::Server, Group::Players, Group::Home, Group::Games] {
            assert!(Module::ALL.iter().any(|m| m.group() == group), "{group:?} is empty");
        }
        for (i, group) in Group::ALL.iter().enumerate() {
            assert!(!group.label().is_empty());
            assert!(!Group::ALL[i + 1..].iter().any(|g| g.key() == group.key()));
        }
    }

    #[test]
    fn modules_with_a_settings_column_are_server_wide_or_chat_driven() {
        let backed: Vec<_> = Module::ALL
            .into_iter()
            .filter_map(|m| m.setting().map(|c| (m, c)))
            .collect();
        assert_eq!(
            backed,
            [
                (Module::Leveling, "leveling_enabled"),
                (Module::Points, "points_enabled"),
                (Module::Welcome, "welcome_enabled"),
                (Module::Gambling, "gambling_enabled"),
            ]
        );
    }

    #[test]
    fn commands_and_buttons_find_their_module() {
        assert_eq!(Module::of_command("pet", Some("view")), Some(Module::Pets));
        assert_eq!(Module::of_command("duel", Some("share")), Some(Module::Party));
        assert_eq!(Module::of_command("duel", Some("connect4")), Some(Module::Duels));
        assert_eq!(Module::of_command("play", Some("shells")), Some(Module::Party));
        assert_eq!(Module::of_command("play", Some("wordle")), Some(Module::Puzzles));
        assert_eq!(Module::of_command("play", Some("stats")), Some(Module::Profile));
        assert_eq!(Module::of_command("points", Some("give")), Some(Module::Store));
        assert_eq!(Module::of_command("duel", Some("chess")), Some(Module::BoardGames));
        assert_eq!(Module::of_command("duel", Some("seafleet")), Some(Module::BoardGames));
        assert_eq!(Module::of_command("duel", Some("checkers")), Some(Module::BoardGames));
        assert_eq!(Module::of_command("play", Some("picross")), Some(Module::BoardGames));
        assert_eq!(Module::of_command("play", Some("2048")), Some(Module::BoardGames));
        assert_eq!(Module::of_command("party", Some("hotpearl")), Some(Module::PartyGames));
        assert_eq!(Module::of_command("party", Some("stop")), Some(Module::PartyGames));
        assert_eq!(Module::of_command("play", None), None);
        assert_eq!(Module::of_command("weekly", None), Some(Module::Economy));
        assert_eq!(Module::of_command("wrapped", None), Some(Module::Profile));
        for prefix in NEVER_GATED_PREFIXES {
            assert_eq!(Module::of_component_prefix(prefix), None, "{prefix}");
        }
        for never in NEVER_GATED {
            assert_eq!(Module::of_command(never, None), None, "{never}");
            assert_eq!(Module::of_command(never, Some("show")), None, "{never}");
        }
        assert_eq!(Module::of_component_prefix("pc"), Some(Module::Pets));
        assert_eq!(Module::of_component_prefix("sh"), Some(Module::Party));
        for prefix in ["bs", "ck", "cc", "pz", "tz"] {
            assert_eq!(
                Module::of_component_prefix(prefix),
                Some(Module::BoardGames),
                "{prefix}"
            );
        }
        for prefix in ["hl", "ac", "cn", "wx", "wy", "td"] {
            assert_eq!(
                Module::of_component_prefix(prefix),
                Some(Module::PartyGames),
                "{prefix}"
            );
        }
        assert_eq!(Module::of_command("tag", None), Some(Module::CustomCommands));
        assert_eq!(
            Module::of_command("customcommand", Some("create")),
            Some(Module::CustomCommands)
        );
        assert_eq!(Module::of_component_prefix("tg"), Some(Module::CustomCommands));
        assert_eq!(Module::of_component_prefix("cm"), Some(Module::CustomCommands));
        assert_eq!(Module::of_component_prefix("zz"), None);
    }

    #[test]
    fn a_rule_decides_by_channel_and_by_what_it_sits_under() {
        let all = ModuleRule::default();
        assert_eq!(all.access(&[1]), Access::Allowed);
        let off = ModuleRule {
            enabled: false,
            ..Default::default()
        };
        assert_eq!(off.access(&[1]), Access::ServerOff);
        let only = ModuleRule {
            enabled: true,
            mode: ChannelMode::Only,
            channels: vec![10, 20],
        };
        assert_eq!(only.access(&[10]), Access::Allowed);
        assert_eq!(
            only.access(&[99, 20]),
            Access::Allowed,
            "a thread follows its channel's category"
        );
        assert_eq!(only.access(&[99]), Access::ChannelOff);
        assert_eq!(only.access(&[]), Access::ChannelOff, "an unknown place is not allowed");
        let except = ModuleRule {
            mode: ChannelMode::Except,
            ..only
        };
        assert_eq!(except.access(&[10]), Access::ChannelOff);
        assert_eq!(except.access(&[99, 20]), Access::ChannelOff);
        assert_eq!(except.access(&[99]), Access::Allowed);
        assert_eq!(except.access(&[]), Access::Allowed);
        assert!(!all.needs_lineage() && except.needs_lineage());
    }

    #[test]
    fn a_switch_in_a_settings_column_ignores_the_row() {
        let rows = [(
            Module::Leveling,
            ModuleRule {
                enabled: false,
                mode: ChannelMode::Except,
                channels: vec![5],
            },
        )];
        let modules = GuildModules::new(rows);
        let rule = modules.rule(Module::Leveling);
        assert!(rule.enabled, "the column decides, never the row");
        assert_eq!(rule.channels, [5]);
        assert_eq!(modules.rule(Module::Pets), ModuleRule::default());
        assert_eq!(modules.rules().count(), Module::ALL.len());
    }

    #[test]
    fn channel_lists_are_checked() {
        assert_eq!(
            check_channels(Module::Pets, ChannelMode::All, &[1, 2]).unwrap(),
            Vec::<u64>::new()
        );
        assert_eq!(
            check_channels(Module::Pets, ChannelMode::Only, &[3, 4, 3]).unwrap(),
            [3, 4]
        );
        assert_eq!(
            check_channels(Module::Pets, ChannelMode::Only, &[]).unwrap_err().field,
            "channels"
        );
        assert_eq!(
            check_channels(Module::Pets, ChannelMode::Except, &[0])
                .unwrap_err()
                .field,
            "channels"
        );
        let many: Vec<u64> = (1..=MAX_CHANNELS as u64 + 1).collect();
        assert_eq!(
            check_channels(Module::Pets, ChannelMode::Only, &many)
                .unwrap_err()
                .field,
            "channels"
        );
        assert!(check_channels(Module::Pets, ChannelMode::Only, &many[..MAX_CHANNELS]).is_ok());
        let error = check_channels(Module::Welcome, ChannelMode::Only, &[1]).unwrap_err();
        assert_eq!(error.field, "channel_mode");
        assert!(error.message.contains("whole server"), "{}", error.message);
        assert!(check_channels(Module::Welcome, ChannelMode::All, &[]).is_ok());
    }

    #[test]
    fn the_slash_command_edits_one_channel_at_a_time() {
        let now = ModuleRule {
            enabled: true,
            mode: ChannelMode::Only,
            channels: vec![1, 2],
        };
        assert_eq!(
            edit_channels(&now, ChannelMode::Only, Some(3), false).unwrap(),
            (ChannelMode::Only, vec![1, 2, 3])
        );
        assert_eq!(
            edit_channels(&now, ChannelMode::Only, Some(2), false).unwrap(),
            (ChannelMode::Only, vec![1, 2])
        );
        assert_eq!(
            edit_channels(&now, ChannelMode::Only, Some(1), true).unwrap(),
            (ChannelMode::Only, vec![2])
        );
        assert_eq!(
            edit_channels(&now, ChannelMode::Only, Some(7), true).unwrap(),
            (ChannelMode::Only, vec![1, 2])
        );
        // Another mode starts a list of its own; taking a channel off it leaves the one in use alone.
        assert_eq!(
            edit_channels(&now, ChannelMode::Except, Some(9), false).unwrap(),
            (ChannelMode::Except, vec![9])
        );
        assert_eq!(
            edit_channels(&now, ChannelMode::Except, Some(1), true).unwrap(),
            (ChannelMode::Only, vec![1, 2])
        );
        // The last channel off, and all channels, both mean everywhere.
        let last = ModuleRule {
            channels: vec![4],
            ..now.clone()
        };
        assert_eq!(
            edit_channels(&last, ChannelMode::Only, Some(4), true).unwrap(),
            (ChannelMode::All, vec![])
        );
        assert_eq!(
            edit_channels(&now, ChannelMode::All, None, false).unwrap(),
            (ChannelMode::All, vec![])
        );
        assert_eq!(
            edit_channels(&now, ChannelMode::Only, None, false).unwrap_err().field,
            "channel"
        );
    }

    #[test]
    fn the_refusal_and_the_summary_read_well() {
        assert_eq!(
            refusal(Module::Pets, Access::ServerOff, false, Lang::En),
            "This feature is turned off on this server."
        );
        assert_eq!(
            refusal(Module::Pets, Access::ChannelOff, false, Lang::En),
            "This feature is turned off in this channel."
        );
        let manager = refusal(Module::Pets, Access::ServerOff, true, Lang::En);
        assert_eq!(
            refusal(Module::Pets, Access::ChannelOff, true, Lang::Tr),
            "Bu özellik bu kanalda kapalı. Bu, **Evcil hayvanlar** modülü: `/config modules` ile ya da panelden açabilirsin."
        );
        assert!(
            manager.contains("**Pets**") && manager.contains("/config modules"),
            "{manager}"
        );
        for module in Module::ALL {
            assert_eq!(module.name(Lang::En), module.label(), "locales/en/common.toml [module]");
        }
        let rule = |enabled, mode, n: u64| ModuleRule {
            enabled,
            mode,
            channels: (1..=n).collect(),
        };
        assert_eq!(summary(&ModuleRule::default()), "on");
        assert_eq!(summary(&rule(false, ChannelMode::All, 0)), "off");
        assert_eq!(summary(&rule(true, ChannelMode::Only, 1)), "only in 1 channel");
        assert_eq!(summary(&rule(true, ChannelMode::Except, 3)), "off in 3 channels");
    }
}
