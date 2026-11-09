//! Which parts of Pinchora a server uses, and where: the modules a manager can switch off for the
//! whole server or limit to some channels. Pure: the bot, `/setup server` and the dashboard all
//! decide with these rules.

use std::collections::HashMap;

use crate::admin::settings::FieldError;
use crate::infra::i18n::Lang;
use crate::t;

/// How many channels one module may name.
pub const MAX_CHANNELS: usize = 50;

/// Commands that stay available whatever a server switches off: the way back in, the basics, and
/// what is the player's own business rather than the server's (their privacy and data, and the
/// reminders they asked for).
pub const NEVER_GATED: [&str; 8] = [
    // registrations:never_gated:start
    "config",
    "help",
    "language",
    "ping",
    "privacy",
    "commandcontrol",
    "backup",
    "remind", // registrations:never_gated:end
];

/// The custom-id prefixes of the buttons and menus the [`NEVER_GATED`] commands send: they have no
/// module either.
pub const NEVER_GATED_PREFIXES: [&str; 5] = [
    // registrations:never_gated_prefixes:start
    "hp", "pr", "kc", "cf", "rm", // registrations:never_gated_prefixes:end
];

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
        if module == Module::CommandControl {
            return ModuleRule::default();
        }
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
        )
        .keyed("whole-server", [("module", module.key().to_owned())]));
    }
    let mut clean: Vec<u64> = Vec::with_capacity(channels.len());
    for &id in channels {
        if id == 0 {
            return Err(FieldError::new("channels", "That isn't a Discord id.").keyed("not-an-id", []));
        }
        if !clean.contains(&id) {
            clean.push(id);
        }
    }
    if clean.is_empty() {
        return Err(
            FieldError::new("channels", "Pick at least one channel, or choose all channels.")
                .keyed("pick-channels", []),
        );
    }
    if clean.len() > MAX_CHANNELS {
        return Err(FieldError::new(
            "channels",
            format!("A module can name at most {MAX_CHANNELS} channels."),
        )
        .keyed("too-many-channels", [("max", MAX_CHANNELS.to_string())]));
    }
    Ok(clean)
}

/// What `/setup server modules-channels` does with one channel: starts a list in `mode` (or adds to the
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
        return Err(
            FieldError::new("channel", "Pick a channel to add, or choose all channels.").keyed("pick-a-channel", []),
        );
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
    summary_in(Lang::En, rule)
}

/// [`summary`] in `lang`.
pub fn summary_in(lang: Lang, rule: &ModuleRule) -> String {
    if !rule.enabled {
        return t!(lang, "module-summary.off");
    }
    let n = rule.channels.len();
    match rule.mode {
        ChannelMode::All => t!(lang, "module-summary.on"),
        ChannelMode::Only => t!(lang, "module-summary.only", count = n),
        ChannelMode::Except => t!(lang, "module-summary.except", count = n),
    }
}

impl Module {
    /// One line on what the module covers, for the dashboard and `/setup server modules-show`.
    pub const fn blurb(self) -> &'static str {
        match self {
            Self::Leveling => "Chat and voice XP, levels and level-up messages.",
            Self::Points => "Server points earned by chatting.",
            Self::Welcome => "The welcome card for new members.",
            Self::Goodbye => "The message when a member leaves.",
            Self::Log => "Members, messages, roles, channels, voice, bans and server changes in the log channels.",
            Self::Moderation => {
                "/ban, /kick, /timeout, /warn, /purge, /mod lock and the other moderation commands, with numbered cases."
            }
            Self::RolePanels => "/setup roles and the role buttons.",
            Self::Store => "/store and /points: spending and giving server points.",
            Self::Suggestions => "/suggest and its voting buttons.",
            Self::Polls => "/poll and its voting buttons.",
            Self::Giveaways => "/giveaway and its entry button.",
            Self::Workflows => "Trigger, conditions and bounded actions",
            Self::CustomCommands => {
                "/tag and /setup commands, and the auto-responders (joins, level-ups, roles, keywords)."
            }
            Self::Profile => {
                "/profile, /rank, /leaderboard, /customize bio, /customize, /character, /info wrapped and game stats."
            }
            Self::Economy => "/daily, /rewards weekly, /rewards monthly, /balance, /give and /rep.",
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
            Self::AutoRoles => "/setup roles: roles given to members who join, and roles kept for those who come back.",
            Self::ColorRoles => "/color: colour roles members pick for their names.",
            Self::Stats => {
                "/stats, /integrations stats, the Stats page and counting messages, voice minutes, joins and leaves."
            }
            Self::Tickets => {
                "/ticket and /community tickets: private support tickets opened from a panel, with transcripts."
            }
            Self::AutoMod => "Automatic message rules and moderation actions.",
            Self::Verification => "Join gate and pixel captcha verification.",
            Self::AntiRaid => "Join detection and reversible raid lockdown.",
            Self::Achievements => "Server achievements and their rewards.",
            Self::Events => "/event: scheduled server events and RSVPs.",
            Self::CommandControl => "Command permissions, aliases and configuration backups.",
            Self::Qotd => "Daily server questions and suggestions.",
            Self::Counting => "A channel for sequential counting.",
            Self::Forms => "Application forms and reviews.",
            Self::Modmail => "Private member messages to server staff.",
            Self::Roblox => "Roblox profile verification, group binds and nicknames.",
            Self::GameTrackers => "/game: League profiles and Minecraft status; patch and watch settings.",
            Self::Feeds => {
                "/integrations feeds: new YouTube videos, RSS and podcast episodes, Reddit and Bluesky posts in a channel."
            }
            Self::TempVoice => "Creator channels that make a voice room for whoever joins, and /voice to run it.",
            Self::Info => "Server, member, role, channel, emoji and invite information.",
            Self::Starboard => "/community starboard: highlight popular posts and set up vote channels.",
            Self::Invites => "/invites: invite counts, leaderboards, rewards and temporary links.",
            Self::AntiNuke => "Protect server structure, staff and VIP members.",
            Self::Birthdays => "Opt-in birthdays, private dates and server celebrations.",
            Self::Utility => "AFK notices, sticky posts and scheduled messages.",
            Self::Branding => "Server bot profile and embed colour.",
        }
    }

    /// [`Module::blurb`] in `lang`.
    pub fn blurb_in(self, lang: Lang) -> String {
        t!(lang, &format!("module-blurb.{}", self.key()))
    }
}

#[cfg(test)]
mod tests {
    use crate::admin::modules::*;

    #[test]
    fn keys_are_unique_and_parse_back() {
        for (i, module) in Module::ALL.iter().enumerate() {
            assert_eq!(Module::parse(module.key()), Some(*module));
            assert!(!Module::ALL[i + 1..].iter().any(|m| m.key() == module.key()));
            assert!(!module.label().is_empty() && !module.blurb().is_empty());
            for lang in [Lang::En, Lang::Tr] {
                for prefix in ["module", "module-blurb"] {
                    let key = format!("{prefix}.{}", module.key());
                    assert!(
                        crate::infra::i18n::exact(lang, &key).is_some(),
                        "{lang:?} is missing {key}"
                    );
                }
            }
        }
        assert_eq!(Module::parse("nope"), None);
        // The four newer groups stay empty and hidden until their features are added.
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
        assert_eq!(Module::of_component_prefix("ms"), Some(Module::CustomCommands));
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
        assert!(error.text(Lang::Tr).contains("Karşılama kartları"));
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
            "Bu özellik bu kanalda kapalı. Bu, **Evcil hayvanlar** modülü: `/setup server` ile ya da panelden açabilirsin."
        );
        assert!(
            manager.contains("**Pets**") && manager.contains("/setup server"),
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

mod catalog;
pub use catalog::{Group, Module, Scope};
