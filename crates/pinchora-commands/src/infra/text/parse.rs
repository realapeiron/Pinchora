//! Reading a text command (`fish cast reef`, `duel connect4 @Sam bet:50`) against the registered
//! slash command tree, into the [`CommandData`] Discord would send for the same slash command. Pure:
//! the ids it finds are looked up afterwards.

mod values;
use values::*;
pub(crate) use values::{is_command, usage};
#[cfg(test)]
mod tests;

use std::num::IntErrorKind;

use pinchora_core::infra::i18n::Lang;
use pinchora_core::t;
use twilight_model::{
    application::{
        command::{Command, CommandOption, CommandOptionChoiceValue, CommandOptionType, CommandOptionValue as Bound},
        interaction::application_command::{CommandData, CommandDataOption, CommandOptionValue},
    },
    id::{
        Id,
        marker::{ChannelMarker, RoleMarker, UserMarker},
    },
};

/// The largest whole number Discord lets an integer option carry, either way (2^53: what a double holds
/// exactly). A typed number beyond it is brought back to it, so no handler is handed a value that
/// no slash command could send.
pub(crate) const MAX_INTEGER: i64 = 1 << 53;

/// Short names for the commands typed most, as `(alias, command)`.
pub(crate) use super::aliases::ALIASES;

/// A text command read against the tree.
#[derive(Debug)]
pub(crate) struct Parsed<'a> {
    /// `fish cast`: the command and the subcommand levels it went through.
    pub path: String,
    /// The command's definition, for its permissions.
    pub command: &'a Command,
    /// As Discord sends it, without `resolved`.
    pub data: CommandData,
    /// The ids the options name, which must be looked up before the command runs.
    pub users: Vec<Id<UserMarker>>,
    pub roles: Vec<Id<RoleMarker>>,
    /// Each channel with the option that named it, for the channel types it accepts.
    pub channels: Vec<(Id<ChannelMarker>, &'a CommandOption)>,
}

/// Why a message is not a command to run.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Refusal {
    /// Not one of ours (another bot's, perhaps): nothing is said.
    Unknown,
    /// One of ours, called wrongly: what is wrong, and how to call it.
    Usage(String),
}

/// Fills the options of a subcommand (or a command without any) from the words left.
fn leaf<'a>(
    options: &'a [CommandOption],
    words: &[Token],
    fail: &dyn Fn(String) -> Refusal,
    lang: Lang,
) -> Result<Vec<(&'a CommandOption, CommandOptionValue)>, Refusal> {
    let mut given: Vec<Option<String>> = vec![None; options.len()];
    // The option the last positional word went to, which may take more words.
    let mut last: Option<usize> = None;
    // `name:` with its value in the next word.
    let mut waiting: Option<usize> = None;
    let mut next = 0;
    for word in words {
        if let Some(at) = waiting.take() {
            given[at] = Some(word.text.clone());
            continue;
        }
        if !word.quoted
            && let Some((key, rest)) = word.text.split_once(':')
            && let Some(at) = options.iter().position(|o| o.name.eq_ignore_ascii_case(key))
        {
            if given[at].is_some() {
                return Err(fail(t!(lang, "text.usage.twice", option = options[at].name)));
            }
            given[at] = Some(rest.to_owned());
            if rest.is_empty() {
                waiting = Some(at);
            }
            // `text:two words` without quotes: free text goes on here too.
            last = Some(at);
            continue;
        }
        while next < options.len() && given[next].is_some() {
            next += 1;
        }
        let fits_next = next < options.len() && value(&options[next], &word.text, lang).is_ok();
        match last {
            // Free text keeps going until a word fits the next option.
            Some(at) if is_free_text(&options[at]) && !fits_next => {
                let text = given[at].get_or_insert_with(String::new);
                text.push(' ');
                text.push_str(&word.text);
            }
            _ if next < options.len() => {
                given[next] = Some(word.text.clone());
                last = Some(next);
                next += 1;
            }
            _ => return Err(fail(t!(lang, "text.usage.too-many", word = quoted(&word.text)))),
        }
    }
    let mut out = Vec::new();
    for (option, text) in options.iter().zip(given) {
        match text {
            Some(text) => out.push((option, value(option, &text, lang).map_err(fail)?)),
            None if option.required == Some(true) => {
                return Err(fail(t!(lang, "text.usage.missing", option = option.name)));
            }
            None => {}
        }
    }
    Ok(out)
}

/// Reads `text` (the message after the prefix) as one of the commands in `tree`. `prefix` is only
/// for the usage line, and `lang` for what is said when the call is wrong.
pub(crate) fn parse<'a>(tree: &'a [Command], prefix: &str, text: &str, lang: Lang) -> Result<Parsed<'a>, Refusal> {
    if matches!(
        text.trim().to_ascii_lowercase().as_str(),
        "botinfo" | "status" | "stats bot"
    ) {
        return parse(tree, prefix, "info bot", lang);
    }
    let words = tokens(text);
    let Some(first) = words.first() else {
        return Err(Refusal::Unknown);
    };
    let Some(command) = command(tree, &first.text) else {
        return Err(Refusal::Unknown);
    };
    let mut path = command.name.clone();
    let mut options: &[CommandOption] = &command.options;
    // The subcommand levels, outermost first.
    let mut levels: Vec<(String, CommandOptionType)> = Vec::new();
    let mut at = 1;
    while options.iter().any(is_branch) {
        let wanted = words.get(at).map(|w| w.text.to_lowercase());
        let Some(branch) = options.iter().find(|o| Some(&o.name) == wanted.as_ref()) else {
            let problem = match words.get(at) {
                Some(word) => t!(
                    lang,
                    "text.usage.no-subcommand",
                    command = path,
                    word = quoted(&word.text)
                ),
                None => t!(lang, "text.usage.needs-subcommand", command = path),
            };
            return Err(Refusal::Usage(format!(
                "{problem}\n{}",
                usage(prefix, &path, options, lang)
            )));
        };
        path.push(' ');
        path.push_str(&branch.name);
        levels.push((branch.name.clone(), branch.kind));
        options = branch.options.as_deref().unwrap_or_default();
        at += 1;
    }
    let fail = |problem: String| Refusal::Usage(format!("{problem}\n{}", usage(prefix, &path, options, lang)));
    let filled = leaf(options, &words[at..], &fail, lang)?;
    let mut users = Vec::new();
    let mut roles = Vec::new();
    let mut channels = Vec::new();
    for (option, value) in &filled {
        match value {
            CommandOptionValue::User(id) => users.push(*id),
            CommandOptionValue::Role(id) => roles.push(*id),
            CommandOptionValue::Channel(id) => channels.push((*id, *option)),
            _ => {}
        }
    }
    let mut data_options: Vec<CommandDataOption> = filled
        .into_iter()
        .map(|(option, value)| CommandDataOption {
            name: option.name.clone(),
            value,
        })
        .collect();
    for (name, kind) in levels.into_iter().rev() {
        let value = if kind == CommandOptionType::SubCommandGroup {
            CommandOptionValue::SubCommandGroup(data_options)
        } else {
            CommandOptionValue::SubCommand(data_options)
        };
        data_options = vec![CommandDataOption { name, value }];
    }
    let data = CommandData {
        guild_id: None,
        id: command.id.unwrap_or(Id::new(1)),
        kind: command.kind,
        name: command.name.clone(),
        options: data_options,
        resolved: None,
        target_id: None,
    };
    Ok(Parsed {
        path,
        command,
        data,
        users,
        roles,
        channels,
    })
}
