//! Reading a text command (`fish cast reef`, `duel connect4 @Sam bet:50`) against the registered
//! slash command tree, into the [`CommandData`] Discord would send for the same slash command. Pure:
//! the ids it finds are looked up afterwards.

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
pub(crate) const ALIASES: &[(&str, &str)] = &[
    ("bal", "balance"),
    ("lb", "leaderboard"),
    ("top", "leaderboard"),
    ("h", "help"),
    ("inv", "inventory"),
];

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

/// One word of the message; a quoted one is never read as `name:value`.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Token {
    text: String,
    quoted: bool,
}

fn is_quote(c: char) -> bool {
    matches!(c, '"' | '“' | '”')
}

/// Splits on whitespace; quotes keep spaces in (`"two words"`, `text:"two words"`). An unclosed
/// quote runs to the end.
fn tokens(text: &str) -> Vec<Token> {
    let mut out = Vec::new();
    let mut current = String::new();
    let (mut quoted, mut inside, mut any) = (false, false, false);
    for c in text.chars() {
        if is_quote(c) {
            // Only a word that starts quoted is kept from being read as `name:value`.
            quoted |= !any;
            inside = !inside;
            any = true;
        } else if c.is_whitespace() && !inside {
            if any {
                out.push(Token {
                    text: std::mem::take(&mut current),
                    quoted,
                });
            }
            (quoted, any) = (false, false);
        } else {
            current.push(c);
            any = true;
        }
    }
    if any {
        out.push(Token { text: current, quoted });
    }
    out
}

fn is_branch(option: &CommandOption) -> bool {
    matches!(
        option.kind,
        CommandOptionType::SubCommand | CommandOptionType::SubCommandGroup
    )
}

/// A member's word as a usage line quotes it: without backticks, so it stays inside its code span
/// (no links or formatting of theirs in our reply), and short, so the reply fits in a message.
fn quoted(word: &str) -> String {
    let mut out: String = word.chars().filter(|&c| c != '`').take(40).collect();
    if word.chars().filter(|&c| c != '`').count() > 40 {
        out.push('…');
    }
    out
}

/// `<spot>` for a required option, `[spot]` for an optional one.
fn placeholder(option: &CommandOption) -> String {
    if option.required == Some(true) {
        format!("<{}>", option.name)
    } else {
        format!("[{}]", option.name)
    }
}

/// How to call `path` (`fish cast`) with `options` below it: `p!fish cast [spot]`, or
/// `p!fish <cast|shop|…>` while a subcommand is still missing.
pub(crate) fn usage(prefix: &str, path: &str, options: &[CommandOption], lang: Lang) -> String {
    let rest = if options.iter().any(is_branch) {
        let names: Vec<&str> = options.iter().map(|o| o.name.as_str()).collect();
        format!(" <{}>", names.join("|"))
    } else {
        options.iter().map(|o| format!(" {}", placeholder(o))).collect()
    };
    t!(lang, "text.usage.line", usage = format!("{prefix}{path}{rest}"))
}

/// Whether the first word of `text` names one of the commands in `tree` (in any case, or by an alias).
pub(crate) fn is_command(tree: &[Command], text: &str) -> bool {
    if matches!(
        text.trim().to_ascii_lowercase().as_str(),
        "botinfo" | "status" | "stats bot"
    ) {
        return tree.iter().any(|command| command.name == "info");
    }
    tokens(text)
        .first()
        .is_some_and(|first| command(tree, &first.text).is_some())
}

/// The command `word` names: its name in any case, or an alias.
fn command<'a>(tree: &'a [Command], word: &str) -> Option<&'a Command> {
    let word = word.to_lowercase();
    let name = ALIASES
        .iter()
        .find(|(alias, _)| *alias == word)
        .map_or(word.as_str(), |(_, name)| name);
    tree.iter().find(|c| c.name == name)
}

/// The id in `<@123>`, `<@!123>`, `<@&123>` or `<#123>` (whichever `starts` allows), or a bare id
/// long enough to be one, so a number like `50` is never read as somebody.
fn snowflake(text: &str, starts: &[&str]) -> Option<u64> {
    let digits = match text.strip_prefix('<').and_then(|t| t.strip_suffix('>')) {
        Some(inner) => starts.iter().find_map(|s| inner.strip_prefix(s))?,
        None if (15..=20).contains(&text.len()) => text,
        None => return None,
    };
    digits.parse::<u64>().ok().filter(|&id| id > 0)
}

fn bound_number(bound: Option<&Bound>) -> Option<f64> {
    match bound {
        Some(Bound::Integer(n)) => Some(*n as f64),
        Some(Bound::Number(n)) => Some(*n),
        None => None,
    }
}

/// Whether `n` is within the bounds an option has; if not, says what they are.
fn in_bounds(option: &CommandOption, n: f64, lang: Lang) -> Result<(), String> {
    let show = |n: f64| {
        if n.fract() == 0.0 {
            format!("{}", n as i64)
        } else {
            format!("{n}")
        }
    };
    let (low, high) = (
        bound_number(option.min_value.as_ref()),
        bound_number(option.max_value.as_ref()),
    );
    if low.is_none_or(|low| n >= low) && high.is_none_or(|high| n <= high) {
        return Ok(());
    }
    let name = &option.name;
    match (low, high) {
        (Some(low), Some(high)) => Err(t!(
            lang,
            "text.usage.bounds-both",
            option = name,
            low = show(low),
            high = show(high)
        )),
        (Some(low), None) => Err(t!(lang, "text.usage.bounds-low", option = name, low = show(low))),
        (None, Some(high)) => Err(t!(lang, "text.usage.bounds-high", option = name, high = show(high))),
        (None, None) => Ok(()),
    }
}

/// `kelp_forest` for `Kelp-Forest` or `kelp forest`: how choices are compared.
fn fold(text: &str) -> String {
    text.to_lowercase().replace(['-', ' '], "_")
}

/// The choice `text` picks: by its value or its name in any case, or by the start of one value.
fn choice(option: &CommandOption, text: &str, lang: Lang) -> Result<Option<CommandOptionValue>, String> {
    let Some(choices) = option.choices.as_ref().filter(|c| !c.is_empty()) else {
        return Ok(None);
    };
    let wanted = fold(text);
    let value = |v: &CommandOptionChoiceValue| match v {
        CommandOptionChoiceValue::String(s) => (fold(s), CommandOptionValue::String(s.clone())),
        CommandOptionChoiceValue::Integer(n) => (n.to_string(), CommandOptionValue::Integer(*n)),
        CommandOptionChoiceValue::Number(n) => (n.to_string(), CommandOptionValue::Number(*n)),
    };
    if let Some(c) = choices
        .iter()
        .find(|c| value(&c.value).0 == wanted || fold(&c.name) == wanted)
    {
        return Ok(Some(value(&c.value).1));
    }
    let starting: Vec<_> = choices
        .iter()
        .filter(|c| !wanted.is_empty() && value(&c.value).0.starts_with(&wanted))
        .collect();
    if let [only] = starting[..] {
        return Ok(Some(value(&only.value).1));
    }
    let keys: Vec<String> = choices.iter().map(|c| value(&c.value).0).collect();
    Err(t!(
        lang,
        "text.usage.choice",
        option = option.name,
        choices = keys.join(", ")
    ))
}

/// What a typed value means for `option`, or why it can't.
fn value(option: &CommandOption, text: &str, lang: Lang) -> Result<CommandOptionValue, String> {
    let name = &option.name;
    if let Some(picked) = choice(option, text, lang)? {
        return Ok(picked);
    }
    match option.kind {
        CommandOptionType::String => {
            let length = text.chars().count();
            let min = option.min_length.map_or(0, usize::from);
            let max = option.max_length.map_or(6_000, usize::from);
            if length < min.max(1) || length > max {
                return Err(t!(
                    lang,
                    "text.usage.length",
                    option = name,
                    min = min.max(1),
                    max = max
                ));
            }
            Ok(CommandOptionValue::String(text.to_owned()))
        }
        CommandOptionType::Integer => {
            let digits: String = text.chars().filter(|c| !matches!(c, ',' | '_')).collect();
            let n = match digits.parse::<i64>() {
                Ok(n) => n,
                // Too big to be a number at all is still a big number, which `clamp` brings in range.
                Err(error) if matches!(error.kind(), IntErrorKind::PosOverflow) => MAX_INTEGER,
                Err(error) if matches!(error.kind(), IntErrorKind::NegOverflow) => -MAX_INTEGER,
                Err(_) => return Err(t!(lang, "text.usage.whole-number", option = name)),
            }
            .clamp(-MAX_INTEGER, MAX_INTEGER);
            in_bounds(option, n as f64, lang)?;
            Ok(CommandOptionValue::Integer(n))
        }
        CommandOptionType::Number => {
            let n: f64 = text
                .replace(',', ".")
                .parse()
                .ok()
                .filter(|n: &f64| n.is_finite())
                .ok_or_else(|| t!(lang, "text.usage.number", option = name))?;
            in_bounds(option, n, lang)?;
            Ok(CommandOptionValue::Number(n))
        }
        CommandOptionType::Boolean => match text.to_lowercase().as_str() {
            "true" | "yes" | "y" | "on" | "1" | "evet" | "e" => Ok(CommandOptionValue::Boolean(true)),
            "false" | "no" | "n" | "off" | "0" | "hayır" | "hayir" | "h" => Ok(CommandOptionValue::Boolean(false)),
            _ => Err(t!(lang, "text.usage.yes-no", option = name)),
        },
        CommandOptionType::User => snowflake(text, &["@!", "@"])
            .map(|id| CommandOptionValue::User(Id::new(id)))
            .ok_or_else(|| t!(lang, "text.usage.member", option = name)),
        CommandOptionType::Role => snowflake(text, &["@&"])
            .map(|id| CommandOptionValue::Role(Id::new(id)))
            .ok_or_else(|| t!(lang, "text.usage.role", option = name)),
        CommandOptionType::Channel => snowflake(text, &["#"])
            .map(|id| CommandOptionValue::Channel(Id::new(id)))
            .ok_or_else(|| t!(lang, "text.usage.channel", option = name)),
        _ => Err(t!(lang, "text.usage.slash-only-option", option = name)),
    }
}

/// A free-text option, which takes the words after it until one fits the next option.
fn is_free_text(option: &CommandOption) -> bool {
    option.kind == CommandOptionType::String && option.choices.as_ref().is_none_or(Vec::is_empty)
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

#[cfg(test)]
mod tests {
    use twilight_interactions::command::{CommandInputData, CommandModel};

    use crate::infra::text::parse::*;

    fn tree() -> Vec<Command> {
        crate::command_definitions()
            .into_iter()
            .filter(|c| c.kind == twilight_model::application::command::CommandType::ChatInput)
            .collect()
    }

    fn ok<'a>(tree: &'a [Command], text: &str) -> Parsed<'a> {
        parse(tree, "p!", text, Lang::En).unwrap_or_else(|refusal| panic!("{text:?}: {refusal:?}"))
    }

    fn usage_of(tree: &[Command], text: &str) -> String {
        match parse(tree, "p!", text, Lang::En) {
            Err(Refusal::Usage(text)) => text,
            other => panic!("{text:?} gave {other:?}"),
        }
    }

    /// The options of the innermost level: the subcommand's, or the command's.
    fn leaf_options(data: &CommandData) -> &[CommandDataOption] {
        let mut options = data.options.as_slice();
        while let Some(CommandDataOption {
            value: CommandOptionValue::SubCommand(inner) | CommandOptionValue::SubCommandGroup(inner),
            ..
        }) = options.first()
        {
            options = inner;
        }
        options
    }

    #[test]
    fn quotes_keep_words_together() {
        let words: Vec<String> = tokens(r#"say "two words" text:"a b"  “curly one” x"#)
            .into_iter()
            .map(|t| t.text)
            .collect();
        assert_eq!(words, ["say", "two words", "text:a b", "curly one", "x"]);
        assert!(tokens(r#""a:b""#)[0].quoted);
        assert_eq!(tokens(r#"open "never closed"#)[1].text, "never closed");
        assert!(tokens("   ").is_empty());
        assert_eq!(tokens(r#""""#)[0].text, "", "an empty quote is an empty word");
    }

    #[test]
    fn mentions_and_ids_become_snowflakes() {
        assert_eq!(
            snowflake("<@123456789012345678>", &["@!", "@"]),
            Some(123456789012345678)
        );
        assert_eq!(
            snowflake("<@!123456789012345678>", &["@!", "@"]),
            Some(123456789012345678)
        );
        assert_eq!(snowflake("123456789012345678", &["@"]), Some(123456789012345678));
        assert_eq!(snowflake("<@&123456789012345678>", &["@&"]), Some(123456789012345678));
        assert_eq!(snowflake("<#123456789012345678>", &["#"]), Some(123456789012345678));
        assert_eq!(
            snowflake("<@&123456789012345678>", &["@!", "@"]),
            None,
            "a role is no user"
        );
        assert_eq!(snowflake("<#123456789012345678>", &["@"]), None);
        assert_eq!(snowflake("12", &["@"]), None, "too short for a snowflake");
        assert_eq!(snowflake("<@7>", &["@"]), Some(7), "a mention needs no length");
        assert_eq!(snowflake("@everyone", &["@"]), None);
    }

    #[test]
    fn names_are_any_case_and_aliases_work() {
        let tree = tree();
        assert_eq!(ok(&tree, "PROFILE").path, "profile");
        assert_eq!(ok(&tree, "Fish CAST").path, "fish cast");
        assert_eq!(ok(&tree, "bal").path, "balance");
        assert_eq!(ok(&tree, "lb").path, "leaderboard");
        assert_eq!(ok(&tree, "h").path, "help");
        for (alias, name) in ALIASES {
            assert!(
                tree.iter().any(|c| c.name == *name),
                "{alias} → /{name} is not a command"
            );
            assert!(!tree.iter().any(|c| c.name == *alias), "{alias} hides /{alias}");
        }
        assert_eq!(
            parse(&tree, "p!", "notacommand", Lang::En).unwrap_err(),
            Refusal::Unknown
        );
        assert_eq!(parse(&tree, "p!", "", Lang::En).unwrap_err(), Refusal::Unknown);
    }

    #[test]
    fn a_choice_is_picked_by_value_or_name() {
        let tree = tree();
        let parsed = ok(&tree, "fish cast REEF");
        let options = leaf_options(&parsed.data);
        assert_eq!(options.len(), 1);
        assert!(matches!(&options[0].value, CommandOptionValue::String(s) if s == "reef"));
        let kelp = ok(&tree, "fish cast kelp");
        assert!(matches!(&leaf_options(&kelp.data)[0].value, CommandOptionValue::String(s) if s == "kelp_forest"));
        let deep = ok(&tree, "fish cast \"Deep Sea\"");
        assert!(matches!(&leaf_options(&deep.data)[0].value, CommandOptionValue::String(s) if s == "deep_sea"));
        let wrong = usage_of(&tree, "fish cast moon");
        assert!(wrong.starts_with("`spot` is one of: "), "{wrong}");
        assert!(wrong.ends_with("Usage: `p!fish cast [spot]`"), "{wrong}");
    }

    #[test]
    fn a_wrong_call_explains_itself() {
        let tree = tree();
        let missing = usage_of(&tree, "duel connect4");
        assert_eq!(missing, "`user` is missing.\nUsage: `p!duel connect4 <user> [bet]`");
        let sub = usage_of(&tree, "fish");
        assert!(
            sub.starts_with("`fish` needs one more word.\nUsage: `p!fish <cast|"),
            "{sub}"
        );
        assert!(usage_of(&tree, "fish swim").starts_with("`fish` has no `swim`."));
        let low = usage_of(&tree, "duel connect4 <@123456789012345678> 5");
        assert!(low.starts_with("`bet` goes from 10 to 500."), "{low}");
        let word = usage_of(&tree, "duel connect4 <@123456789012345678> lots");
        assert!(word.starts_with("`bet` is a whole number."), "{word}");
        assert!(usage_of(&tree, "duel connect4 sam").starts_with("`user` is a member"));
        assert!(usage_of(&tree, "profile <@123456789012345678> extra").contains("one word too many"));
        // A member's word can't close the code span it is quoted in, nor make the reply too long.
        let sneaky = usage_of(&tree, "fish x`[nitro](https://e.x)`");
        assert!(sneaky.starts_with("`fish` has no `x[nitro](https://e.x)`."), "{sneaky}");
        assert!(usage_of(&tree, &format!("fish {}", "a".repeat(5_000))).chars().count() < 200);
        let twice = usage_of(
            &tree,
            "duel connect4 user:<@123456789012345678> user:<@223456789012345678>",
        );
        assert!(twice.starts_with("`user` is given twice."), "{twice}");
    }

    #[test]
    fn named_values_and_mentions_are_read() {
        let tree = tree();
        let parsed = ok(&tree, "duel connect4 bet:50 <@!123456789012345678>");
        assert_eq!(parsed.users, [Id::new(123456789012345678)]);
        let options = leaf_options(&parsed.data);
        assert!(
            options
                .iter()
                .any(|o| o.name == "bet" && o.value == CommandOptionValue::Integer(50))
        );
        assert!(
            options
                .iter()
                .any(|o| o.name == "user" && o.value == CommandOptionValue::User(Id::new(123456789012345678)))
        );
        // A colon that names no option is just part of the word.
        let parsed = ok(&tree, "suggest text:\"open at 10:30\"");
        assert!(matches!(&leaf_options(&parsed.data)[0].value, CommandOptionValue::String(s) if s == "open at 10:30"));
        // Free text given by name goes on without quotes too.
        let parsed = ok(&tree, "suggest text:let us party");
        assert!(matches!(&leaf_options(&parsed.data)[0].value, CommandOptionValue::String(s) if s == "let us party"));
    }

    #[test]
    fn free_text_takes_the_words_after_it() {
        let tree = tree();
        let parsed = ok(&tree, "suggest let us have a pool party");
        match &leaf_options(&parsed.data)[0].value {
            CommandOptionValue::String(text) => assert_eq!(text, "let us have a pool party"),
            other => panic!("{other:?}"),
        }
    }

    /// A value that fits the option, as a member would type it.
    fn sample(option: &CommandOption) -> String {
        if let Some(choice) = option.choices.as_ref().and_then(|c| c.first()) {
            return match &choice.value {
                CommandOptionChoiceValue::String(s) => s.clone(),
                CommandOptionChoiceValue::Integer(n) => n.to_string(),
                CommandOptionChoiceValue::Number(n) => n.to_string(),
            };
        }
        let low = bound_number(option.min_value.as_ref());
        match option.kind {
            CommandOptionType::String => {
                let length = option.min_length.map_or(4, usize::from).max(4);
                let length = option.max_length.map_or(length, |max| length.min(usize::from(max)));
                if length >= 3 {
                    format!("\"{} y\"", "x".repeat(length - 2))
                } else {
                    "x".repeat(length)
                }
            }
            CommandOptionType::Integer => (low.unwrap_or(1.0) as i64).to_string(),
            CommandOptionType::Number => low.unwrap_or(1.0).to_string(),
            CommandOptionType::Boolean => "yes".to_owned(),
            CommandOptionType::User => "<@123456789012345678>".to_owned(),
            CommandOptionType::Role => "<@&223456789012345678>".to_owned(),
            CommandOptionType::Channel => "<#323456789012345678>".to_owned(),
            other => panic!("no sample for {other:?}"),
        }
    }

    /// Every leaf the tree registers: its path and its options.
    fn leaves(tree: &[Command]) -> Vec<(String, Vec<CommandOption>)> {
        fn walk(path: &str, options: &[CommandOption], out: &mut Vec<(String, Vec<CommandOption>)>) {
            if !options.iter().any(is_branch) {
                out.push((path.to_owned(), options.to_vec()));
                return;
            }
            for option in options {
                walk(
                    &format!("{path} {}", option.name),
                    option.options.as_deref().unwrap_or_default(),
                    out,
                );
            }
        }
        let mut out = Vec::new();
        for command in tree {
            walk(&command.name, &command.options, &mut out);
        }
        out
    }

    /// Every registered command parses its own usage: every option positionally, only the required
    /// ones, and every option by name; and what comes out is what the handler's model reads.
    #[test]
    fn every_command_parses_its_own_usage() {
        let tree = tree();
        let leaves = leaves(&tree);
        assert!(leaves.len() > 100, "found only {} commands", leaves.len());
        for (path, options) in leaves {
            if options
                .iter()
                .any(|o| matches!(o.kind, CommandOptionType::Attachment | CommandOptionType::Mentionable))
            {
                continue;
            }
            let all: Vec<String> = options.iter().map(sample).collect();
            let required: Vec<String> = options
                .iter()
                .filter(|o| o.required == Some(true))
                .map(sample)
                .collect();
            let named: Vec<String> = options
                .iter()
                .rev()
                .map(|o| format!("{}:{}", o.name, sample(o)))
                .collect();
            for (form, words, expected) in [
                ("positional", &all, options.len()),
                ("required", &required, required.len()),
                ("named", &named, options.len()),
            ] {
                let text = format!("{path} {}", words.join(" "));
                let parsed = ok(&tree, &text);
                assert_eq!(parsed.path, path, "{form}: {text}");
                let given = leaf_options(&parsed.data);
                assert_eq!(given.len(), expected, "{form}: {text} gave {given:?}");
            }
            let shown = usage("p!", &path, &options, Lang::En);
            assert!(shown.starts_with(&format!("Usage: `p!{path}")), "{shown}");
        }
    }

    #[test]
    fn whole_numbers_stop_at_what_discord_allows() {
        let option: CommandOption = serde_json::from_value(serde_json::json!({
            "type": 4,
            "name": "n",
            "description": "a number",
        }))
        .expect("option JSON");
        let read = |text: &str| value(&option, text, Lang::En);
        assert_eq!(read("50"), Ok(CommandOptionValue::Integer(50)));
        assert_eq!(read("-1,000"), Ok(CommandOptionValue::Integer(-1000)));
        assert_eq!(read("9007199254740992"), Ok(CommandOptionValue::Integer(MAX_INTEGER)));
        for huge in ["9007199254740993", "9223372036854775807", "99999999999999999999999999"] {
            assert_eq!(read(huge), Ok(CommandOptionValue::Integer(MAX_INTEGER)), "{huge}");
        }
        for huge in [
            "-9007199254740993",
            "-9223372036854775808",
            "-99999999999999999999999999",
        ] {
            assert_eq!(read(huge), Ok(CommandOptionValue::Integer(-MAX_INTEGER)), "{huge}");
        }
        assert!(read("12x").is_err() && read("").is_err() && read("-").is_err());
        // The option's own bounds still apply to the clamped number.
        let bounded: CommandOption = serde_json::from_value(serde_json::json!({
            "type": 4,
            "name": "bet",
            "description": "a bet",
            "min_value": 1,
            "max_value": 1000,
        }))
        .expect("option JSON");
        assert_eq!(value(&bounded, "1000", Lang::En), Ok(CommandOptionValue::Integer(1000)));
        assert_eq!(
            value(&bounded, "99999999999999999999", Lang::En),
            Err("`bet` goes from 1 to 1000.".to_owned())
        );
    }

    /// A wrong call is explained in the language it is parsed in, and `evet`/`hayır` answer a switch.
    #[test]
    fn a_wrong_call_explains_itself_in_turkish() {
        let tree = tree();
        let tr = |text: &str| match parse(&tree, "p!", text, Lang::Tr) {
            Err(Refusal::Usage(text)) => text,
            other => panic!("{text:?} gave {other:?}"),
        };
        assert_eq!(
            tr("duel connect4"),
            "`user` eksik.\nKullanım: `p!duel connect4 <user> [bet]`"
        );
        assert!(tr("fish swim").starts_with("`fish` komutunda `swim` diye bir şey yok."));
        assert!(tr("duel connect4 <@123456789012345678> 5").starts_with("`bet` 10 ile 500 arasında olmalı."));
        assert!(tr("fish cast moon").starts_with("`spot` şunlardan biri olmalı: "));
        let option: CommandOption = serde_json::from_value(serde_json::json!({
            "type": 5,
            "name": "enabled",
            "description": "a switch",
        }))
        .expect("option JSON");
        assert_eq!(value(&option, "Evet", Lang::Tr), Ok(CommandOptionValue::Boolean(true)));
        assert_eq!(
            value(&option, "HAYIR", Lang::Tr),
            Ok(CommandOptionValue::Boolean(false))
        );
        assert_eq!(
            value(&option, "belki", Lang::Tr),
            Err("`enabled` evet ya da hayır olmalı.".to_owned())
        );
    }

    /// What the parser builds is what twilight's derive reads for the handler, mentions included.
    #[test]
    fn the_handlers_model_reads_the_parsed_data() {
        let tree = tree();
        let parsed = ok(&tree, "fish cast shore");
        let input = CommandInputData::from(parsed.data);
        assert!(crate::commands::games::fish::FishCommand::from_interaction(input).is_ok());
    }
}
