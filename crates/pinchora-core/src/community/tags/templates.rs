use super::*;

// /Placeholders

/// One placeholder the editor offers as a chip.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Placeholder {
    pub token: &'static str,
    pub hint: &'static str,
}

const fn ph(token: &'static str, hint: &'static str) -> Placeholder {
    Placeholder { token, hint }
}

const COMMON: [Placeholder; 8] = [
    ph("{user}", "Mentions the member"),
    ph("{user.name}", "Their name, without a mention"),
    ph("{user.id}", "Their Discord id"),
    ph("{user.avatar}", "A link to their avatar"),
    ph("{server}", "The server's name"),
    ph("{server.members}", "How many members the server has"),
    ph("{channel}", "The channel it happens in"),
    ph("{random:a|b|c}", "One of the choices, at random"),
];

const POINTS: [Placeholder; 2] = [
    ph("{points}", "The member's points after a give points action"),
    ph("{points.name}", "What the server calls its points"),
];

/// The placeholders a custom command fills.
pub fn command_placeholders() -> Vec<Placeholder> {
    let mut all = vec![
        ph("{args}", "Everything typed after the command"),
        ph("{1}", "The first word typed (up to {5})"),
        ph("{1:friend}", "The first word, or `friend` when none"),
    ];
    all.extend(COMMON);
    all.extend(POINTS);
    all
}

/// The placeholders an auto-responder of this kind fills.
pub fn responder_placeholders(kind: TriggerKind) -> Vec<Placeholder> {
    let mut all: Vec<Placeholder> = COMMON.to_vec();
    match kind {
        TriggerKind::MemberJoin => all.push(ph("{count}", "Which member number this is")),
        TriggerKind::LevelUp => {
            all.push(ph("{level}", "The level they reached"));
            all.push(ph("{role}", "The level role they earned, if any"));
        }
        TriggerKind::RoleAdded | TriggerKind::RoleRemoved => all.push(ph("{role}", "The role")),
        TriggerKind::MemberLeave | TriggerKind::Keyword => {}
    }
    all.extend(POINTS);
    all
}

// /Filling

/// The values a text is filled with: named placeholders and the words typed after a command.
#[derive(Debug, Clone, Default)]
pub struct Vars {
    named: Vec<(String, String)>,
    words: Vec<String>,
}

impl Vars {
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets `{key}`.
    pub fn set(&mut self, key: &str, value: impl Into<String>) -> &mut Self {
        let value = value.into();
        match self.named.iter_mut().find(|(k, _)| k == key) {
            Some((_, v)) => *v = value,
            None => self.named.push((key.to_owned(), value)),
        }
        self
    }

    /// Whether `{key}` has a value.
    pub fn has(&self, key: &str) -> bool {
        self.named.iter().any(|(k, _)| k == key)
    }

    /// Sets `{args}` and the words `{1}` to `{5}` from what was typed.
    pub fn args(&mut self, text: &str) -> &mut Self {
        self.words = text.split_whitespace().take(ARG_WORDS).map(str::to_owned).collect();
        self.set("args", text)
    }

    fn get(&self, key: &str) -> Option<&str> {
        self.named.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str())
    }
}

/// Fills the placeholders of `template`. `{key}` is a named value, `{1}` to `{5}` the words typed
/// (`{1:default}` falls back when there is none), `{random:a|b|c}` one of its choices; `pick(n)`
/// chooses an index below `n`. Anything else, and anything unclosed, stays as written, and a value that
/// itself looks like a placeholder is not filled again.
pub fn render(template: &str, vars: &Vars, pick: &mut dyn FnMut(usize) -> usize) -> String {
    let mut out = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        match after.find(['{', '}']) {
            Some(end) if after.as_bytes()[end] == b'}' => {
                let key = &after[..end];
                match expand(key, vars, pick) {
                    Some(value) => out.push_str(&value),
                    None => {
                        out.push('{');
                        out.push_str(key);
                        out.push('}');
                    }
                }
                rest = &after[end + 1..];
            }
            _ => {
                out.push('{');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

fn expand(key: &str, vars: &Vars, pick: &mut dyn FnMut(usize) -> usize) -> Option<String> {
    if let Some(value) = vars.get(key) {
        return Some(value.to_owned());
    }
    let (name, tail) = match key.split_once(':') {
        Some((name, tail)) => (name, Some(tail)),
        None => (key, None),
    };
    if let Ok(n) = name.parse::<usize>()
        && (1..=ARG_WORDS).contains(&n)
        && name.bytes().all(|b| b.is_ascii_digit())
    {
        let word = vars.words.get(n - 1).map(String::as_str);
        return Some(word.or(tail).unwrap_or_default().to_owned());
    }
    if name == "random"
        && let Some(tail) = tail
    {
        let choices: Vec<&str> = tail.split('|').take(RANDOM_MAX).collect();
        let at = pick(choices.len()).min(choices.len() - 1);
        return Some(choices[at].to_owned());
    }
    None
}

/// `text` cut to `max` characters, with an ellipsis when something went.
pub fn truncate(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_owned();
    }
    let mut cut: String = text.chars().take(max.saturating_sub(1)).collect();
    cut.push('…');
    cut
}
