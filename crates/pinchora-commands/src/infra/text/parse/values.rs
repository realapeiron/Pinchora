use super::*;

/// One word of the message; a quoted one is never read as `name:value`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Token {
    pub(super) text: String,
    pub(super) quoted: bool,
}

fn is_quote(c: char) -> bool {
    matches!(c, '"' | '“' | '”')
}

/// Splits on whitespace; quotes keep spaces in (`"two words"`, `text:"two words"`). An unclosed
/// quote runs to the end.
pub(super) fn tokens(text: &str) -> Vec<Token> {
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

pub(super) fn is_branch(option: &CommandOption) -> bool {
    matches!(
        option.kind,
        CommandOptionType::SubCommand | CommandOptionType::SubCommandGroup
    )
}

/// A member's word as a usage line quotes it: without backticks, so it stays inside its code span
/// (no links or formatting of theirs in our reply), and short, so the reply fits in a message.
pub(super) fn quoted(word: &str) -> String {
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
pub(super) fn command<'a>(tree: &'a [Command], word: &str) -> Option<&'a Command> {
    let word = word.to_lowercase();
    let name = ALIASES
        .iter()
        .find(|(alias, _)| *alias == word)
        .map_or(word.as_str(), |(_, name)| name);
    tree.iter().find(|c| c.name == name)
}

/// The id in `<@123>`, `<@!123>`, `<@&123>` or `<#123>` (whichever `starts` allows), or a bare id
/// long enough to be one, so a number like `50` is never read as somebody.
pub(super) fn snowflake(text: &str, starts: &[&str]) -> Option<u64> {
    let digits = match text.strip_prefix('<').and_then(|t| t.strip_suffix('>')) {
        Some(inner) => starts.iter().find_map(|s| inner.strip_prefix(s))?,
        None if (15..=20).contains(&text.len()) => text,
        None => return None,
    };
    digits.parse::<u64>().ok().filter(|&id| id > 0)
}

pub(super) fn bound_number(bound: Option<&Bound>) -> Option<f64> {
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
pub(super) fn value(option: &CommandOption, text: &str, lang: Lang) -> Result<CommandOptionValue, String> {
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
pub(super) fn is_free_text(option: &CommandOption) -> bool {
    option.kind == CommandOptionType::String && option.choices.as_ref().is_none_or(Vec::is_empty)
}
