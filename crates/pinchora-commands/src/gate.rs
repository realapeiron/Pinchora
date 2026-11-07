//! Switched-off modules: the one check every slash command, button, select and form passes before it
//! runs. A refusal is private and names no one; `/config`, `/help` and `/ping` are never refused.

use pinchora_core::modules::{Access, Module, refusal};
use twilight_model::application::interaction::application_command::{CommandData, CommandOptionValue};

use crate::Cx;

/// The subcommand or subcommand group a command was called through, one level down.
fn subcommand(data: &CommandData) -> Option<&str> {
    data.options
        .first()
        .filter(|o| {
            matches!(
                o.value,
                CommandOptionValue::SubCommand(_) | CommandOptionValue::SubCommandGroup(_)
            )
        })
        .map(|o| o.name.as_str())
}

/// Whether the command's module is off here; if so, the invoker has been told and the command must
/// not run.
pub(crate) async fn refuses_command(cx: &Cx, data: &CommandData) -> anyhow::Result<bool> {
    let refused = refuses(cx, Module::of_command(&data.name, subcommand(data))).await?
        || crate::command_control::refuses(cx, &crate::command_control::path(data), false).await?;
    if !refused {
        crate::stats::count_command(cx).await;
    }
    Ok(refused)
}

/// [`refuses_command`] for a button, select or form, by the prefix of its custom id.
pub(crate) async fn refuses_component(cx: &Cx, custom_id: &str) -> anyhow::Result<bool> {
    if refuses(cx, custom_id.split(':').next().and_then(Module::of_component_prefix)).await? {
        return Ok(true);
    }
    match crate::command_control::component_owner(cx, custom_id).await? {
        Some(path) => crate::command_control::refuses(cx, path, true).await,
        None => Ok(false),
    }
}

async fn refuses(cx: &Cx, module: Option<Module>) -> anyhow::Result<bool> {
    let (Some(module), Some(guild)) = (module, cx.guild_id()) else {
        return Ok(false);
    };
    if matches!(module, Module::Verification | Module::AntiRaid) && crate::verification::active_gate(cx, module).await?
    {
        return Ok(false);
    }
    let channel = cx.interaction.channel.as_ref().map(|c| c.id.get());
    match cx.app.module_access(guild, channel, module).await? {
        Access::Allowed => Ok(false),
        access => {
            cx.reply_ephemeral(refusal(module, access, cx.can_manage_guild(), cx.lang()))
                .await?;
            metrics::counter!("pinchora_module_refusals_total", "module" => module.key()).increment(1);
            Ok(true)
        }
    }
}

#[cfg(test)]
mod tests {
    use pinchora_core::modules::{NEVER_GATED, NEVER_GATED_PREFIXES};
    use twilight_model::application::command::{Command, CommandOption, CommandOptionType};

    use super::*;

    /// Every `(command, subcommand)` a member can run, the subcommand being the first level below the command.
    fn runnable() -> Vec<(String, Option<String>)> {
        let mut found = Vec::new();
        for Command { name, options, .. } in crate::command_definitions() {
            let subs: Vec<&CommandOption> = options
                .iter()
                .filter(|o| {
                    matches!(
                        o.kind,
                        CommandOptionType::SubCommand | CommandOptionType::SubCommandGroup
                    )
                })
                .collect();
            if subs.is_empty() {
                found.push((name.clone(), None));
            }
            for sub in subs {
                found.push((name.clone(), Some(sub.name.clone())));
            }
        }
        found
    }

    /// A command added without a module would stay on whatever a server switches off, so this fails
    /// until it gets one (in `Module::of_command`) or is added to the never-gated list on purpose.
    #[test]
    fn every_command_has_a_module() {
        for (name, sub) in runnable() {
            if NEVER_GATED.contains(&name.as_str()) {
                assert_eq!(
                    Module::of_command(&name, sub.as_deref()),
                    None,
                    "/{name} is never gated"
                );
                continue;
            }
            assert!(
                Module::of_command(&name, sub.as_deref()).is_some(),
                "/{name} {} has no module: add it to Module::of_command",
                sub.as_deref().unwrap_or("")
            );
        }
        let names: Vec<String> = crate::command_definitions().into_iter().map(|c| c.name).collect();
        for never in NEVER_GATED {
            assert!(names.iter().any(|n| n == never), "/{never} is not a command any more");
        }
    }

    /// The two-letter prefixes a router matches, found in its source: a button added without a module
    /// would stay clickable after its module is switched off.
    fn prefixes(source: &str, from: &str, to: &str) -> Vec<String> {
        let start = source.find(from).expect("the router is in components.rs");
        let end = start + source[start..].find(to).expect("the router has an end");
        let mut found = Vec::new();
        for piece in source[start..end].split('"').skip(1).step_by(2) {
            if piece.len() == 2 && piece.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit()) {
                found.push(piece.to_owned());
            }
        }
        found.sort();
        found.dedup();
        found
    }

    #[test]
    fn every_button_and_form_has_a_module() {
        let source = include_str!("components.rs");
        let buttons = prefixes(source, "pub(crate) async fn dispatch(", "/// Routes a submitted modal");
        let forms = prefixes(
            source,
            "pub(crate) async fn dispatch_modal(",
            "/// Every text input of a submitted modal",
        );
        assert!(
            buttons.len() > 40 && forms.len() >= 3,
            "the scan found {buttons:?} / {forms:?}"
        );
        for prefix in buttons.iter().chain(&forms) {
            if NEVER_GATED_PREFIXES.contains(&prefix.as_str()) {
                assert_eq!(Module::of_component_prefix(prefix), None, "`{prefix}:` is never gated");
                continue;
            }
            assert!(
                Module::of_component_prefix(prefix).is_some(),
                "`{prefix}:` has no module: add it to Module::of_component_prefix"
            );
        }
    }

    /// The module a button belongs to must be the module of the command that sends it, or a server
    /// could keep a part alive through the other door. These are the ones that are easy to get wrong.
    #[test]
    fn buttons_follow_their_command() {
        for (prefix, command, sub) in [
            ("sh", "duel", "share"),
            ("tt", "duel", "tictactoe"),
            ("c4", "duel", "connect4"),
            ("rp", "duel", "rps"),
            ("bs", "duel", "seafleet"),
            ("ck", "duel", "checkers"),
            ("cc", "duel", "chess"),
            ("pz", "play", "picross"),
            ("tz", "play", "2048"),
            ("hl", "party", "hotpearl"),
            ("ac", "party", "acronym"),
            ("cn", "party", "postcard"),
            ("wx", "party", "wordrace"),
            ("wy", "party", "wouldyourrather"),
            ("td", "party", "truthordare"),
            ("hm", "play", "hangman"),
            ("dy", "play", "derby"),
            ("hs", "play", "heist"),
            ("sz", "play", "shells"),
            ("ss", "surge", ""),
            ("pe", "plaza", "edit"),
            ("pd", "plaza", "donate"),
            ("st", "store", ""),
            ("rr", "roles", "post"),
            ("lb", "leaderboard", ""),
            ("gb", "house", "guestbook"),
            ("rc", "house", "contest"),
            ("cd", "critterdrop", ""),
            ("bc", "dex", ""),
            ("kp", "crabpot", ""),
            ("mb", "critterdrop", ""),
            ("tg", "customcommand", "list"),
            ("cm", "customcommand", "create"),
        ] {
            let sub = Some(sub).filter(|s| !s.is_empty());
            assert_eq!(
                Module::of_component_prefix(prefix),
                Module::of_command(command, sub),
                "`{prefix}:` and /{command} disagree"
            );
        }
    }
}
