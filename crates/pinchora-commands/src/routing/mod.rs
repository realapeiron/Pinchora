//! Command-tree transition. Definitions and payloads share this manifest; business handlers keep
//! their original typed models. Legacy roots stay registered until the final release slice.
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

pub const COMPLETED_SLICE: u8 = 12;
pub const PROJECTED_ROOTS: usize = 62;

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
            setup_permissions: if self.active() && self.canonical.starts_with("setup ") {
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
        .find(|r| r.legacy == path || (r.active() && r.canonical == path))
}

pub fn legacy(path: &str) -> &str {
    route(path).map_or(path, |r| r.legacy.as_str())
}

pub fn canonical(path: &str) -> &str {
    if let Some(route) = route(path).filter(|r| r.active()) {
        return &route.canonical;
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
    config
        .rules
        .iter()
        .filter(|(key, _)| {
            [old, new]
                .iter()
                .any(|p| *p == key.as_str() || p.strip_prefix(key.as_str()).is_some_and(|s| s.starts_with(' ')))
        })
        .collect()
}

/// Longest command-token match. The argument suffix is copied byte-for-byte, including quotes.
/// Server aliases expand once before this function; shipped mappings never consume alias slots.
pub(crate) fn resolve_text(text: &str) -> (String, Option<&'static Route>) {
    let text = text.trim_start();
    let mut best = None;
    for route in MANIFEST.iter().filter(|r| r.active()) {
        let mut rest = text;
        let mut matched = true;
        let parts: Vec<_> = route.legacy.split(' ').collect();
        for (index, part) in parts.iter().enumerate() {
            let end = rest.find(char::is_whitespace).unwrap_or(rest.len());
            if !rest[..end].eq_ignore_ascii_case(part) {
                matched = false;
                break;
            }
            rest = &rest[end..];
            // Keep the separator before arguments; only trim between path tokens.
            if index + 1 < parts.len() {
                rest = rest.trim_start();
            }
        }
        if matched && best.is_none_or(|(r, _): (&Route, &str)| route.legacy.len() > r.legacy.len()) {
            best = Some((route, rest));
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
    let Some(route) = route(&path).filter(|r| r.active() && r.canonical == path) else {
        anyhow::ensure!(
            !matches!(data.name.as_str(), "setup" | "mod" | "rewards"),
            "unknown command action `{path}`"
        );
        return Ok(data);
    };
    let mut options = data.options;
    for _ in route.canonical.split(' ').skip(1) {
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
/// Legacy roots remain registered throughout the transition.
pub(crate) fn extend(tree: &mut Vec<Command>) {
    let original = tree.clone();
    // Wrapped remains usable in DMs. The existing info handlers retain their own guild guard.
    tree.iter_mut().find(|c| c.name == "info").unwrap().contexts =
        original.iter().find(|c| c.name == "wrapped").unwrap().contexts.clone();
    for (name, description, permissions) in [
        (
            "setup",
            "Configure server settings and administration tools",
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
    for route in MANIFEST.iter().filter(|r| r.active()) {
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
        anyhow::ensure!(roots.insert(&root.name), "duplicate root {}", root.name);
        walk(&root.options, 0, &root.name)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests;
