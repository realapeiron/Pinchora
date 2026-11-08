//! Command-tree transition. Definitions and payloads share this manifest; business handlers keep
//! their original typed models. Legacy definitions remain internal for rollout and text compatibility.
use std::{collections::BTreeSet, sync::LazyLock};

use pinchora_core::{command_control::protected as recovery_path, modules::Module};
use serde::Deserialize;
use twilight_model::{
    application::{
        command::{Command, CommandOption, CommandOptionType},
        interaction::application_command::{CommandData, CommandDataOption, CommandOptionValue},
    },
    guild::Permissions,
};

pub const COMPLETED_SLICE: u8 = 20;
pub const PROJECTED_ROOTS: usize = 80;
pub const FUTURE_ROOT_RESERVE: usize = 3;

pub(crate) const COMMUNITY_ADMIN_PATHS: &[&str] = &[
    "qotd settings",
    "qotd queue",
    "counting setup",
    "counting reset",
    "form create",
    "form post",
    "form close",
    "modmail settings",
];

/// Administration roots share the original setup permission and privacy boundary.
pub fn is_admin_path(path: &str) -> bool {
    COMMUNITY_ADMIN_PATHS.contains(&path)
        || matches!(
            path.split(' ').next(),
            Some("setup" | "safety" | "community" | "integrations" | "progression")
        )
}

/// The applied 0066 paths remain valid for stored policies, backups and typed shortcuts.
const ADMIN_GROUP_MOVES: &[(&str, &str)] = &[
    ("setup moderation", "safety moderation"),
    ("setup automod", "safety automod"),
    ("setup verification", "safety verification"),
    ("setup antinuke", "safety antinuke"),
    ("setup tickets", "community tickets"),
    ("setup starboard", "community starboard"),
    ("setup events", "community events"),
    ("setup birthdays", "community birthdays"),
    ("setup utility", "community utility"),
    ("setup community", "community server"),
    ("setup feeds", "integrations feeds"),
    ("setup stats", "integrations stats"),
    ("setup invites", "integrations invites"),
    ("setup leveling", "progression leveling"),
    ("setup economy", "progression economy"),
    ("setup achievements", "progression achievements"),
];

pub(crate) const PREVIOUS_ACTIONS: &[(&str, &str)] = &[
    ("mod ban", "ban"),
    ("mod kick", "kick"),
    ("mod warn", "warn"),
    ("mod timeout", "timeout"),
    ("mod purge", "purge"),
];

pub(crate) fn previous_path(route: &Route) -> String {
    previous_admin_path(&route.canonical)
}

fn previous_admin_path(path: &str) -> String {
    for (old, new) in PREVIOUS_ACTIONS.iter().chain(ADMIN_GROUP_MOVES) {
        if let Some(suffix) = path.strip_prefix(new)
            && (suffix.is_empty() || suffix.starts_with(' '))
        {
            return format!("{old}{suffix}");
        }
    }
    path.to_owned()
}

#[derive(Debug, Deserialize)]
pub struct Route {
    /// Stable operation identity and the path understood by the existing business handler.
    pub legacy: String,
    pub canonical: String,
    pub slice: u8,
}

/// Metadata shared by both invocation paths. Handler-specific checks remain in the business model.
#[derive(Debug)]
pub struct ActionMetadata<'a> {
    pub operation: &'a str,
    pub module: Option<Module>,
    pub setup_permissions: Permissions,
    pub protected: bool,
    pub response: crate::Deferral,
    pub help_area: Option<usize>,
}

impl Route {
    pub fn metadata(&self) -> ActionMetadata<'_> {
        ActionMetadata {
            operation: &self.legacy,
            module: self.module(),
            setup_permissions: if self.active() && is_admin_path(&self.canonical) {
                Permissions::MANAGE_GUILD
            } else {
                Permissions::empty()
            },
            protected: protected(&self.legacy),
            response: crate::Deferral::for_path(&self.legacy),
            help_area: crate::commands::help::area_of(&self.legacy),
        }
    }

    pub fn active(&self) -> bool {
        self.slice > 0 && self.slice <= COMPLETED_SLICE
    }

    pub fn module(&self) -> Option<Module> {
        let mut parts = self.legacy.split(' ');
        Module::of_command(parts.next().unwrap_or_default(), parts.next())
    }
}

pub static MANIFEST: LazyLock<Vec<Route>> =
    LazyLock::new(|| serde_json::from_str(include_str!("manifest.json")).expect("checked command route manifest"));

pub fn route(path: &str) -> Option<&'static Route> {
    MANIFEST
        .iter()
        .find(|r| r.legacy == path || (r.active() && (r.canonical == path || previous_path(r) == path)))
}

pub fn legacy(path: &str) -> &str {
    route(path).map_or(path, |r| r.legacy.as_str())
}

pub fn canonical(path: &str) -> &str {
    if let Some(route) = route(path).filter(|r| r.active()) {
        return &route.canonical;
    }
    if let Some((_, new)) = ADMIN_GROUP_MOVES.iter().find(|(old, _)| *old == path) {
        return new;
    }
    // Old component owners can name a whole family (am -> automod, an -> antinuke).
    // Only map parents whose complete family has moved; voice and color still have member leaves.
    let descendants: Vec<_> = MANIFEST
        .iter()
        .filter(|r| r.legacy.strip_prefix(path).is_some_and(|s| s.starts_with(' ')))
        .collect();
    if descendants.is_empty() || descendants.iter().any(|r| !r.active()) {
        return path;
    }
    let first = &descendants[0].canonical;
    let mut end = first.len();
    while !descendants.iter().all(|r| {
        r.canonical == first[..end]
            || r.canonical
                .strip_prefix(&first[..end])
                .is_some_and(|s| s.starts_with(' '))
    }) {
        let Some(index) = first[..end].rfind(' ') else {
            return path;
        };
        end = index;
    }
    let parent = &first[..end];
    if MANIFEST.iter().any(|r| r.canonical == parent) {
        return path;
    }
    // A flattened family can share a group with unrelated actions. Never broaden an old rule.
    if MANIFEST.iter().any(|r| {
        (r.canonical == parent || r.canonical.strip_prefix(parent).is_some_and(|s| s.starts_with(' ')))
            && !(r.legacy == path || r.legacy.strip_prefix(path).is_some_and(|s| s.starts_with(' ')))
    }) {
        return path;
    }
    parent
}

/// Recovery protection follows the original action, never the entire `/setup` root.
pub(crate) fn protected(path: &str) -> bool {
    recovery_path(path) || recovery_path(legacy(path))
}

/// Both trees share additive policy rules and cooldowns during the transition. Component callers
/// use this mapping too, but do not inherit the setup slash command's Manage Server guard.
pub(crate) fn matching<'a>(
    config: &'a pinchora_core::command_control::Config,
    path: &str,
) -> Vec<(&'a String, &'a pinchora_core::command_control::Rule)> {
    let old = legacy(path);
    let new = canonical(path);
    let previous = previous_admin_path(new);
    let mut result = Vec::new();
    let matches = |key: &str, p: &str| p == key || p.strip_prefix(key).is_some_and(|s| s.starts_with(' '));
    for (key, rule) in &config.rules {
        if [old, new, previous.as_str()].into_iter().any(|p| matches(key, p)) {
            result.push((key, rule));
            result.extend(rule.constraints.iter());
        } else if route(path).is_none() {
            // Old generic component owners name a family, not a leaf. Its migrated parent
            // constraint is still authoritative; unrelated leaf constraints are not inherited.
            for (source, constraint) in &rule.constraints {
                if [old, new, previous.as_str()].into_iter().any(|p| matches(source, p))
                    && !result.contains(&(source, constraint))
                {
                    result.push((source, constraint));
                }
            }
        }
    }
    result
}

/// Longest command-token match. The argument suffix is copied byte-for-byte, including quotes.
/// Server aliases expand once before this function; shipped mappings never consume alias slots.
pub(crate) fn resolve_text(text: &str) -> (String, Option<&'static Route>) {
    let text = text.trim_start();
    let mut best = None;
    let mut best_len = 0;
    for route in MANIFEST.iter().filter(|r| r.active()) {
        for source in [&route.legacy, &previous_path(route)] {
            let mut rest = text;
            let mut matched = true;
            let parts: Vec<_> = source.split(' ').collect();
            for (index, part) in parts.iter().enumerate() {
                let end = rest.find(char::is_whitespace).unwrap_or(rest.len());
                if !rest[..end].eq_ignore_ascii_case(part) {
                    matched = false;
                    break;
                }
                rest = &rest[end..];
                if index + 1 < parts.len() {
                    rest = rest.trim_start();
                }
            }
            if matched && source.len() > best_len && source != &route.canonical {
                best = Some((route, rest));
                best_len = source.len();
            }
        }
    }
    match best {
        Some((route, suffix)) => (format!("{}{suffix}", route.canonical), Some(route)),
        None => (text.into(), None),
    }
}

/// Rebuild the exact old nesting without changing resolved objects, focused options or values.
pub(crate) fn adapt(mut data: CommandData) -> anyhow::Result<CommandData> {
    let path = crate::command_control::path(&data);
    let Some(route) = route(&path).filter(|r| r.active()) else {
        anyhow::ensure!(
            !(is_admin_path(&data.name) || matches!(data.name.as_str(), "mod" | "rewards")),
            "unknown command action `{path}`"
        );
        return Ok(data);
    };
    if path == route.legacy {
        return Ok(data);
    }
    let mut options = data.options;
    for _ in path.split(' ').skip(1) {
        anyhow::ensure!(options.len() == 1, "expected one command action");
        options = match options.remove(0).value {
            CommandOptionValue::SubCommand(children) | CommandOptionValue::SubCommandGroup(children) => children,
            _ => anyhow::bail!("expected command action"),
        };
    }
    let parts: Vec<_> = route.legacy.split(' ').collect();
    for (index, name) in parts.iter().enumerate().skip(1).rev() {
        let value = if index == parts.len() - 1 {
            CommandOptionValue::SubCommand(options)
        } else {
            CommandOptionValue::SubCommandGroup(options)
        };
        options = vec![CommandDataOption {
            name: (*name).into(),
            value,
        }];
    }
    data.name = parts[0].into();
    data.options = options;
    Ok(data)
}

fn source(tree: &[Command], path: &str) -> CommandOption {
    let mut parts = path.split(' ');
    let name = parts.next().expect("root name");
    let root = tree.iter().find(|c| c.name == name).expect("manifest root");
    let mut leaf: CommandOption = serde_json::from_value(serde_json::json!({
        "type": 1, "name": root.name, "description": root.description,
        "description_localizations": root.description_localizations, "options": root.options
    }))
    .expect("root as subcommand");
    for part in parts {
        leaf = leaf
            .options
            .as_ref()
            .unwrap()
            .iter()
            .find(|o| o.name == part)
            .expect("manifest leaf")
            .clone();
    }
    leaf
}

/// Clone typed definitions, including limits, choices, autocomplete and localized descriptions.
/// Only canonical roots are registered; legacy models remain available to dispatch.
pub(crate) fn extend(tree: &mut Vec<Command>) {
    let original = tree.clone();
    // The former F20 community definition is retained only as a legacy handler model.
    tree.retain(|command| command.name != "community");
    // Wrapped remains usable in DMs. The existing info handlers retain their own guild guard.
    tree.iter_mut().find(|c| c.name == "info").unwrap().contexts =
        original.iter().find(|c| c.name == "wrapped").unwrap().contexts.clone();
    for (name, description, permissions) in [
        ("setup", "Server settings and tools", Some(Permissions::MANAGE_GUILD)),
        (
            "safety",
            "Configure moderation and server protection",
            Some(Permissions::MANAGE_GUILD),
        ),
        (
            "community",
            "Configure community tools",
            Some(Permissions::MANAGE_GUILD),
        ),
        (
            "integrations",
            "Configure feeds, counters and invite tracking",
            Some(Permissions::MANAGE_GUILD),
        ),
        (
            "progression",
            "Configure leveling, economy and achievements",
            Some(Permissions::MANAGE_GUILD),
        ),
        ("mod", "Manage moderation actions and records", None),
        ("rewards", "Claim your weekly and monthly rewards", None),
    ] {
        let mut root = original
            .iter()
            .find(|c| c.name == if name == "rewards" { "weekly" } else { "config" })
            .unwrap()
            .clone();
        root.name = name.into();
        root.description = description.into();
        root.default_member_permissions = permissions;
        root.options.clear();
        tree.push(root);
    }
    for route in MANIFEST.iter().filter(|r| r.active() && r.legacy != r.canonical) {
        let parts: Vec<_> = route.canonical.split(' ').collect();
        let root = tree.iter_mut().find(|c| c.name == parts[0]).expect("canonical root");
        let mut leaf = source(&original, &route.legacy);
        leaf.name = parts.last().unwrap().to_string();
        if parts.len() == 2 {
            root.options.push(leaf);
        } else {
            let name = parts[1];
            if !root.options.iter().any(|o| o.name == name) {
                let description = if parts[0] == "mod" {
                    original.iter().find(|c| c.name == name).unwrap().description.clone()
                } else if matches!(parts[0], "community" | "setup") {
                    format!("{name} settings")
                } else {
                    format!("Configure {name} settings")
                };
                root.options.push(
                    serde_json::from_value(serde_json::json!({
                        "type": 2, "name": name, "description": description, "options": []
                    }))
                    .expect("group definition"),
                );
            }
            root.options
                .iter_mut()
                .find(|o| o.name == name)
                .unwrap()
                .options
                .as_mut()
                .unwrap()
                .push(leaf);
        }
    }
}

/// Final release cutover: register exactly the canonical leaves, retaining old models internally.
pub(crate) fn remove_replaced(tree: &mut Vec<Command>) {
    fn keep(path: &str) -> bool {
        MANIFEST
            .iter()
            .any(|r| r.canonical == path || r.canonical.strip_prefix(path).is_some_and(|s| s.starts_with(' ')))
    }
    fn prune(options: &mut Vec<CommandOption>, parent: &str) {
        options.retain_mut(|option| {
            if !matches!(
                option.kind,
                CommandOptionType::SubCommand | CommandOptionType::SubCommandGroup
            ) {
                return true;
            }
            let path = format!("{parent} {}", option.name);
            if !keep(&path) {
                return false;
            }
            if let Some(children) = &mut option.options {
                prune(children, &path);
            }
            true
        });
    }
    tree.retain_mut(|command| {
        if !keep(&command.name) {
            return false;
        }
        prune(&mut command.options, &command.name);
        true
    });
}

/// A completed registration must not capture new guilds as if they had pre-migration overrides.
pub fn has_legacy_paths(tree: &[Command]) -> bool {
    crate::commands::help::leaves(tree, pinchora_core::i18n::Lang::En)
        .iter()
        .any(|leaf| {
            MANIFEST
                .iter()
                .any(|r| r.active() && r.legacy != r.canonical && r.legacy == leaf.path)
        })
}

/// Discord counts the longest localized value (including the default) per field.
/// https://docs.discord.com/developers/interactions/application-commands#slash-commands
pub const COMMAND_SIZE_LIMIT: usize = 8_000;
pub const COMMAND_SIZE_BUDGET: usize = 6_800;
pub const COMMUNITY_SIZE_BUDGET: usize = 6_400;

pub fn command_size(command: &Command) -> usize {
    size_of_node(&serde_json::to_value(command).expect("serializable command"))
}

fn size_of_node(node: &serde_json::Value) -> usize {
    let text_len = |value: &serde_json::Value| match value {
        serde_json::Value::String(text) => text.chars().count(),
        serde_json::Value::Number(number) => number.to_string().chars().count(),
        _ => 0,
    };
    let mut size = text_len(&node["value"]);
    for field in ["name", "description"] {
        size += node[format!("{field}_localizations")]
            .as_object()
            .into_iter()
            .flat_map(|map| map.values())
            .map(text_len)
            .chain(std::iter::once(text_len(&node[field])))
            .max()
            .unwrap_or_default();
    }
    for field in ["options", "choices"] {
        size += node[field]
            .as_array()
            .into_iter()
            .flatten()
            .map(size_of_node)
            .sum::<usize>();
    }
    size
}

/// Validate the complete registration payload before any Discord HTTP request.
pub fn validate(tree: &[Command]) -> anyhow::Result<()> {
    anyhow::ensure!(
        tree.len() <= 100,
        "{} roots exceeds Discord's 100-root budget",
        tree.len()
    );
    let mut roots = BTreeSet::new();
    fn walk(options: &[CommandOption], depth: usize, path: &str) -> anyhow::Result<()> {
        anyhow::ensure!(options.len() <= 25, "{path}: more than 25 children/options");
        let mut names = BTreeSet::new();
        let has_commands = options.iter().any(|o| {
            matches!(
                o.kind,
                CommandOptionType::SubCommand | CommandOptionType::SubCommandGroup
            )
        });
        for option in options {
            anyhow::ensure!(names.insert(&option.name), "{path}: duplicate option {}", option.name);
            let next = format!("{path} {}", option.name);
            anyhow::ensure!(
                option.choices.as_ref().is_none_or(|c| c.len() <= 25),
                "{next}: more than 25 choices"
            );
            match option.kind {
                CommandOptionType::SubCommandGroup => {
                    anyhow::ensure!(depth == 0, "{next}: groups must be directly under a root");
                    let children = option.options.as_deref().unwrap_or_default();
                    anyhow::ensure!(
                        !children.is_empty() && children.iter().all(|c| c.kind == CommandOptionType::SubCommand),
                        "{next}: groups must contain subcommands"
                    );
                }
                CommandOptionType::SubCommand => {
                    anyhow::ensure!(depth <= 1, "{next}: subcommands cannot nest");
                    anyhow::ensure!(
                        option.options.iter().flatten().all(|c| !matches!(
                            c.kind,
                            CommandOptionType::SubCommand | CommandOptionType::SubCommandGroup
                        )),
                        "{next}: subcommands contain only arguments"
                    );
                }
                _ => {
                    anyhow::ensure!(!has_commands, "{path}: arguments mixed with subcommands");
                    anyhow::ensure!(
                        option.options.as_ref().is_none_or(Vec::is_empty),
                        "{next}: argument has children"
                    );
                }
            }
            walk(option.options.as_deref().unwrap_or_default(), depth + 1, &next)?;
        }
        Ok(())
    }
    for root in tree {
        let size = command_size(root);
        let budget = if root.name == "community" {
            COMMUNITY_SIZE_BUDGET
        } else {
            COMMAND_SIZE_BUDGET
        };
        anyhow::ensure!(
            size <= budget,
            "/{}: {size} characters exceeds {budget} (Discord limit {COMMAND_SIZE_LIMIT})",
            root.name
        );
        anyhow::ensure!(roots.insert(&root.name), "duplicate root {}", root.name);
        walk(&root.options, 0, &root.name)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests;

mod policy;
pub use policy::{normalize_backup, normalize_control};

mod permissions;
pub(crate) use permissions::{refuses_saved_permissions, saved_permissions_denied};
