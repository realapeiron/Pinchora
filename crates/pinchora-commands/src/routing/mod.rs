//! Command-tree transition. Definitions and payloads share this manifest; business handlers keep
//! their original typed models. Legacy roots stay registered until the final release slice.
use std::{collections::BTreeSet, sync::LazyLock};

use pinchora_core::{command_control::PROTECTED, modules::Module};
use serde::Deserialize;
use twilight_model::{
    application::{
        command::{Command, CommandOption, CommandOptionType},
        interaction::application_command::{CommandData, CommandDataOption, CommandOptionValue},
    },
    guild::Permissions,
};

pub const COMPLETED_SLICE: u8 = 6;
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
            setup_permissions: if self.active() {
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
    &first[..end]
}

/// Recovery protection follows the original action, never the entire `/setup` root.
pub(crate) fn protected(path: &str) -> bool {
    PROTECTED.contains(&legacy(path).split(' ').next().unwrap_or_default())
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

/// Rebuild the exact old nesting without changing resolved objects, focused options or values.
pub(crate) fn adapt(mut data: CommandData) -> anyhow::Result<CommandData> {
    if data.name != "setup" {
        return Ok(data);
    }
    let path = crate::command_control::path(&data);
    let route = route(&path)
        .filter(|r| r.active() && r.canonical == path)
        .ok_or_else(|| anyhow::anyhow!("unknown setup action `{path}`"))?;
    anyhow::ensure!(data.options.len() == 1, "expected one setup group");
    let CommandOptionValue::SubCommandGroup(mut group) = data.options.remove(0).value else {
        anyhow::bail!("expected setup group");
    };
    anyhow::ensure!(group.len() == 1, "expected one setup action");
    let CommandOptionValue::SubCommand(mut options) = group.remove(0).value else {
        anyhow::bail!("expected setup action");
    };
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

fn source<'a>(tree: &'a [Command], path: &str) -> &'a CommandOption {
    let mut parts = path.split(' ');
    let root = tree
        .iter()
        .find(|c| Some(c.name.as_str()) == parts.clone().next())
        .expect("manifest root");
    parts.next();
    let mut options = &root.options[..];
    let mut found = None;
    for part in parts {
        let option = options.iter().find(|o| o.name == part).expect("manifest leaf");
        found = Some(option);
        options = option.options.as_deref().unwrap_or_default();
    }
    found.expect("setup sources are subcommands")
}

/// Clone typed definitions, including limits, choices, autocomplete and localized descriptions.
pub(crate) fn setup(tree: &[Command]) -> Command {
    let mut command = tree
        .iter()
        .find(|c| c.name == "config")
        .expect("config definition")
        .clone();
    command.name = "setup".into();
    command.description = "Configure server settings and administration tools".into();
    command.default_member_permissions = Some(Permissions::MANAGE_GUILD);
    command.options.clear();
    for (name, description) in [
        ("server", "Server settings"),
        ("welcome", "Welcome and goodbye settings"),
        ("moderation", "Moderation settings"),
        ("leveling", "Levels and XP settings"),
        ("economy", "Server economy settings"),
        ("roles", "Role settings"),
        ("tickets", "Ticket settings"),
        ("utility", "Utility settings"),
        ("automod", "AutoMod settings"),
        ("verification", "Verification settings"),
        ("antinuke", "Anti-nuke settings"),
    ] {
        let mut group: CommandOption = serde_json::from_value(serde_json::json!({
            "type": 2, "name": name, "description": description, "options": []
        }))
        .expect("group definition");
        let mut children = Vec::new();
        for route in MANIFEST
            .iter()
            .filter(|r| r.active() && r.canonical.split(' ').nth(1) == Some(name))
        {
            let mut leaf = source(tree, &route.legacy).clone();
            leaf.name = route.canonical.split(' ').nth(2).expect("setup action").into();
            children.push(leaf);
        }
        group.options = Some(children);
        command.options.push(group);
    }
    command
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
