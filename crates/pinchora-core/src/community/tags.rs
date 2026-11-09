//! Custom commands (`/tag`) and auto-responders: what a server can write, the limits, the sentences that say
//! why a draft is refused, and the placeholder language the text is filled with. Pure: the slash commands,
//! the bot's event paths and the dashboard all decide with these rules.

use std::ops::RangeInclusive;

use serde::{Deserialize, Serialize};

use crate::admin::settings::FieldError;
use crate::admin::settings::LEVEL_RANGE;
use crate::admin::settings::check_points_emoji;
use crate::admin::settings::parse_hex_color;
use crate::infra::i18n::Lang;
use crate::t;

// /Limits

/// Custom commands one server may have.
pub const MAX_COMMANDS: usize = 50;
/// Auto-responders one server may have.
pub const MAX_RESPONDERS: usize = 25;
/// Variants of one reply; one is picked at random.
pub const MAX_RESPONSES: usize = 5;
/// One reply, in characters (Discord's message limit).
pub const RESPONSE_MAX: usize = 2000;
pub const NAME_MAX: usize = 32;
pub const DESCRIPTION_MAX: usize = 100;
pub const RESPONDER_NAME_MAX: usize = 40;
pub const KEYWORD_MAX: usize = 100;
pub const MAX_ACTIONS: usize = 3;
/// Roles or channels in one allow or deny list.
pub const MAX_LIST: usize = 25;
pub const COMMAND_COOLDOWN_MAX: i64 = 3_600;
pub const RESPONDER_COOLDOWN_MAX: i64 = 86_400;
/// The shortest cooldown, per member, of a command or responder that does something besides answering:
/// actions must not be a points tap or a role toggle to hammer.
pub const ACTION_COOLDOWN_MIN: i64 = 30;
/// Server points custom actions may give one member in one server per day (UTC); taking is not capped.
pub const ACTION_POINTS_DAILY_MAX: i64 = 1_000;
/// Direct messages custom actions and auto-responders may send one member of one server per hour.
pub const ACTION_DMS_PER_HOUR: i64 = 5;
/// Server points one action may give (positive) or take (negative).
pub const POINTS_ACTION: RangeInclusive<i64> = -1_000..=1_000;
/// What a member types after the command name, in characters.
pub const ARGS_MAX: usize = 200;
/// Words `{1}` to `{5}` can reach.
pub const ARG_WORDS: usize = 5;
/// Choices one `{random:a|b|c}` may have.
pub const RANDOM_MAX: usize = 25;

pub const EMBED_TITLE_MAX: usize = 256;
pub const EMBED_DESCRIPTION_MAX: usize = 4_096;
pub const EMBED_AUTHOR_MAX: usize = 256;
pub const EMBED_FOOTER_MAX: usize = 2_048;
pub const EMBED_FIELD_NAME_MAX: usize = 256;
pub const EMBED_FIELD_VALUE_MAX: usize = 1_024;
pub const EMBED_FIELDS_MAX: usize = 25;
/// Every character of an embed together.
pub const EMBED_TOTAL_MAX: usize = 6_000;
pub const URL_MAX: usize = 512;

/// What a name that is one of the bot's own commands is told.
pub const RESERVED_NOTE: &str = "That name belongs to one of Pinchora's own commands.";

// /Ids

/// Discord ids travel as strings in JSON (a number would lose digits in a browser) and are read back
/// from either.
mod ids {
    use serde::{Deserialize, Deserializer, Serializer};

    #[derive(Deserialize)]
    #[serde(untagged)]
    pub enum Flex {
        Text(String),
        Number(u64),
    }

    impl Flex {
        pub fn id<E: serde::de::Error>(self) -> Result<u64, E> {
            match self {
                Self::Number(n) => Ok(n),
                Self::Text(s) => s.trim().parse().map_err(|_| E::custom("Discord ids are numbers")),
            }
        }
    }

    pub fn serialize<S: Serializer>(value: &u64, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(value)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<u64, D::Error> {
        Flex::deserialize(d)?.id()
    }
}

mod id_opt {
    use serde::{Deserialize, Deserializer, Serializer};

    use crate::community::tags::ids::Flex;

    pub fn serialize<S: Serializer>(value: &Option<u64>, s: S) -> Result<S::Ok, S::Error> {
        match value {
            Some(id) => s.collect_str(id),
            None => s.serialize_none(),
        }
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Option<u64>, D::Error> {
        Option::<Flex>::deserialize(d)?.map(Flex::id).transpose()
    }
}

mod id_list {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    use crate::community::tags::ids::Flex;

    pub fn serialize<S: Serializer>(value: &[u64], s: S) -> Result<S::Ok, S::Error> {
        value.iter().map(u64::to_string).collect::<Vec<_>>().serialize(s)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<u64>, D::Error> {
        Vec::<Flex>::deserialize(d)?.into_iter().map(Flex::id).collect()
    }
}

fn yes() -> bool {
    true
}

fn chance_always() -> i64 {
    100
}

// /Choices

/// Who sees a custom command's answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReplyMode {
    #[default]
    Public,
    Ephemeral,
}

impl ReplyMode {
    pub const fn key(self) -> &'static str {
        match self {
            Self::Public => "public",
            Self::Ephemeral => "ephemeral",
        }
    }

    pub fn parse(key: &str) -> Option<Self> {
        match key {
            "public" => Some(Self::Public),
            "ephemeral" => Some(Self::Ephemeral),
            _ => None,
        }
    }
}

/// What makes an auto-responder fire.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TriggerKind {
    MemberJoin,
    MemberLeave,
    LevelUp,
    RoleAdded,
    RoleRemoved,
    Keyword,
}

impl TriggerKind {
    pub const ALL: [Self; 6] = [
        Self::MemberJoin,
        Self::MemberLeave,
        Self::LevelUp,
        Self::RoleAdded,
        Self::RoleRemoved,
        Self::Keyword,
    ];

    pub const fn key(self) -> &'static str {
        match self {
            Self::MemberJoin => "member_join",
            Self::MemberLeave => "member_leave",
            Self::LevelUp => "level_up",
            Self::RoleAdded => "role_added",
            Self::RoleRemoved => "role_removed",
            Self::Keyword => "keyword",
        }
    }

    pub fn parse(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|k| k.key() == key)
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::MemberJoin => "A member joins",
            Self::MemberLeave => "A member leaves",
            Self::LevelUp => "A member levels up",
            Self::RoleAdded => "A member gets a role",
            Self::RoleRemoved => "A member loses a role",
            Self::Keyword => "A message has a keyword",
        }
    }

    /// Whether the trigger needs the privileged Message Content intent.
    pub const fn needs_message_content(self) -> bool {
        matches!(self, Self::Keyword)
    }
}

/// How a keyword is looked for in a message (never case-sensitive).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MatchKind {
    Contains,
    Exact,
    StartsWith,
    EndsWith,
}

impl MatchKind {
    pub const ALL: [Self; 4] = [Self::Contains, Self::Exact, Self::StartsWith, Self::EndsWith];

    pub const fn key(self) -> &'static str {
        match self {
            Self::Contains => "contains",
            Self::Exact => "exact",
            Self::StartsWith => "starts_with",
            Self::EndsWith => "ends_with",
        }
    }

    pub fn parse(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|k| k.key() == key)
    }

    /// Whether `message` matches `keyword` this way, ignoring case and the spaces around the message.
    pub fn matches(self, keyword: &str, message: &str) -> bool {
        let (keyword, message) = (keyword.trim().to_lowercase(), message.trim().to_lowercase());
        if keyword.is_empty() {
            return false;
        }
        match self {
            Self::Contains => message.contains(&keyword),
            Self::Exact => message == keyword,
            Self::StartsWith => message.starts_with(&keyword),
            Self::EndsWith => message.ends_with(&keyword),
        }
    }
}

/// Where an auto-responder answers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Destination {
    /// Where it happened: the channel of the message, or where the member levelled up.
    #[default]
    Current,
    /// One channel the server picked.
    Channel,
    /// A direct message to the member.
    Dm,
}

impl Destination {
    pub const fn key(self) -> &'static str {
        match self {
            Self::Current => "current",
            Self::Channel => "channel",
            Self::Dm => "dm",
        }
    }

    pub fn parse(key: &str) -> Option<Self> {
        match key {
            "current" => Some(Self::Current),
            "channel" => Some(Self::Channel),
            "dm" => Some(Self::Dm),
            _ => None,
        }
    }
}

/// The replies, tidy and checked. With an embed, a reply may be empty (the embed carries the answer).
fn clean_responses(responses: Vec<String>, has_embed: bool, errors: &mut Vec<FieldError>) -> Vec<String> {
    let responses: Vec<String> = responses.into_iter().map(|r| r.trim().to_owned()).collect();
    if responses.is_empty() {
        errors.push(FieldError::new("responses", "Write at least one reply.").keyed("tag-no-reply", []));
        return responses;
    }
    if responses.len() > MAX_RESPONSES {
        errors.push(
            FieldError::new(
                "responses",
                format!("A command has at most {MAX_RESPONSES} replies; one is picked at random."),
            )
            .keyed("tag-too-many-replies", [("max", MAX_RESPONSES.to_string())]),
        );
    }
    for (n, response) in responses.iter().enumerate() {
        if response.is_empty() && !has_embed {
            errors.push(
                FieldError::new("responses", format!("Reply {} is empty.", n + 1))
                    .keyed("tag-reply-empty", [("number", (n + 1).to_string())]),
            );
        }
        let length = response.chars().count();
        if length > RESPONSE_MAX {
            errors.push(
                FieldError::new(
                    "responses",
                    format!(
                        "Reply {} is {length} characters; a reply is at most {RESPONSE_MAX}.",
                        n + 1
                    ),
                )
                .keyed(
                    "tag-reply-long",
                    [
                        ("number", (n + 1).to_string()),
                        ("length", length.to_string()),
                        ("max", RESPONSE_MAX.to_string()),
                    ],
                ),
            );
        }
    }
    responses
}

fn clean_list(name: &'static str, ids: Vec<u64>, errors: &mut Vec<FieldError>) -> Vec<u64> {
    let mut clean: Vec<u64> = Vec::with_capacity(ids.len());
    for id in ids {
        if id == 0 {
            errors.push(FieldError::new(name, "That isn't a Discord id."));
        } else if !clean.contains(&id) {
            clean.push(id);
        }
    }
    if clean.len() > MAX_LIST {
        errors.push(FieldError::new(
            name,
            format!("A list holds at most {MAX_LIST} entries."),
        ));
    }
    clean
}

fn clean_cooldown(seconds: i64, max: i64, has_actions: bool, errors: &mut Vec<FieldError>) -> i64 {
    if !(0..=max).contains(&seconds) {
        errors.push(
            FieldError::new("cooldown_secs", format!("A cooldown goes from 0 to {max} seconds."))
                .keyed("tag-cooldown-range", [("max", max.to_string())]),
        );
    } else if has_actions && seconds < ACTION_COOLDOWN_MIN {
        errors.push(FieldError::new(
            "cooldown_secs",
            format!(
                "With actions, a member waits at least {ACTION_COOLDOWN_MIN} seconds between uses: set the cooldown to {ACTION_COOLDOWN_MIN} or more, or remove the actions."
            ),
        )
        .keyed("tag-cooldown-actions", [("min", ACTION_COOLDOWN_MIN.to_string())]));
    }
    seconds.clamp(0, max)
}

/// The cooldown a command or responder runs with: what the server set, and at least
/// [`ACTION_COOLDOWN_MIN`] when it has actions (a row written before that rule still gets it).
pub fn run_cooldown(seconds: i64, actions: &[Action]) -> i64 {
    if actions.is_empty() {
        seconds.max(0)
    } else {
        seconds.max(ACTION_COOLDOWN_MIN)
    }
}

/// Fills `errors` from the embed's and actions' verdicts and returns what survived.
fn absorb<T: Default>(result: Result<T, Vec<FieldError>>, errors: &mut Vec<FieldError>) -> T {
    match result {
        Ok(value) => value,
        Err(mut found) => {
            errors.append(&mut found);
            T::default()
        }
    }
}

// /Custom commands

/// A custom command as a server writes it: what `/tag` runs. Also the shape of the dashboard's JSON and
/// of an export.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CommandDef {
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub responses: Vec<String>,
    #[serde(default)]
    pub embed: Option<EmbedSpec>,
    #[serde(default)]
    pub reply_mode: ReplyMode,
    #[serde(default)]
    pub actions: Vec<Action>,
    #[serde(default, with = "id_list")]
    pub allow_roles: Vec<u64>,
    #[serde(default, with = "id_list")]
    pub deny_roles: Vec<u64>,
    #[serde(default, with = "id_list")]
    pub allow_channels: Vec<u64>,
    #[serde(default, with = "id_list")]
    pub deny_channels: Vec<u64>,
    #[serde(default)]
    pub cooldown_secs: i64,
    #[serde(default = "yes")]
    pub enabled: bool,
    /// Whether the text may mention the users and roles it names (never everyone or here): a Manage
    /// Server choice, off unless the server asks for it.
    #[serde(default)]
    pub allow_mentions: bool,
}

/// A command name tidy: trimmed, lowercase, a leading `/` dropped.
pub fn tidy_name(name: &str) -> String {
    name.trim().trim_start_matches('/').trim().to_lowercase()
}

/// Whether `name` is a valid command name: 1 to 32 of `a-z 0-9 _ -`, starting with a letter or digit.
pub fn is_name(name: &str) -> bool {
    let bytes = name.as_bytes();
    (1..=NAME_MAX).contains(&bytes.len())
        && (bytes[0].is_ascii_lowercase() || bytes[0].is_ascii_digit())
        && bytes
            .iter()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'_' || *b == b'-')
}

/// The name of a command, checked against the bot's own commands (`reserved`, lowercase).
pub fn check_name(name: &str, reserved: &[String]) -> Result<String, FieldError> {
    let name = tidy_name(name);
    if !is_name(&name) {
        return Err(FieldError::new(
            "name",
            format!(
                "A name is 1 to {NAME_MAX} characters: lowercase letters, numbers, `-` and `_`, starting with a letter or number."
            ),
        )
        .keyed("tag-name", [("max", NAME_MAX.to_string())]));
    }
    if reserved.contains(&name) {
        return Err(FieldError::new("name", RESERVED_NOTE).keyed("tag-reserved", []));
    }
    Ok(name)
}

impl CommandDef {
    /// The command made tidy and checked: every problem found, worded for the person who wrote it.
    pub fn clean(mut self, reserved: &[String]) -> Result<Self, Vec<FieldError>> {
        let mut errors = Vec::new();
        match check_name(&self.name, reserved) {
            Ok(name) => self.name = name,
            Err(error) => errors.push(error),
        }
        self.description = self.description.split_whitespace().collect::<Vec<_>>().join(" ");
        if self.description.chars().count() > DESCRIPTION_MAX {
            errors.push(
                FieldError::new(
                    "description",
                    format!("A description is at most {DESCRIPTION_MAX} characters."),
                )
                .keyed("tag-description", [("max", DESCRIPTION_MAX.to_string())]),
            );
        }
        self.embed = absorb(clean_embed(self.embed.take()), &mut errors);
        self.responses = clean_responses(std::mem::take(&mut self.responses), self.embed.is_some(), &mut errors);
        self.actions = absorb(
            clean_actions(std::mem::take(&mut self.actions), false, true),
            &mut errors,
        );
        self.allow_roles = clean_list("allow_roles", std::mem::take(&mut self.allow_roles), &mut errors);
        self.deny_roles = clean_list("deny_roles", std::mem::take(&mut self.deny_roles), &mut errors);
        self.allow_channels = clean_list("allow_channels", std::mem::take(&mut self.allow_channels), &mut errors);
        self.deny_channels = clean_list("deny_channels", std::mem::take(&mut self.deny_channels), &mut errors);
        self.cooldown_secs = clean_cooldown(
            self.cooldown_secs,
            COMMAND_COOLDOWN_MAX,
            !self.actions.is_empty(),
            &mut errors,
        );
        if errors.is_empty() { Ok(self) } else { Err(errors) }
    }

    /// Whether a member with `roles`, in a place described by `lineage` (the channel and what it sits
    /// under), may run the command; otherwise why not.
    pub fn allows_in(&self, lang: Lang, roles: &[u64], lineage: &[u64]) -> Result<(), String> {
        self.allows(roles, lineage).map_err(|why| {
            t!(
                lang,
                match why {
                    "That command can't be used in this channel." => "tag.not-here",
                    "Your roles don't allow that command." => "tag.roles-no",
                    _ => "tag.role-needed",
                }
            )
        })
    }

    /// [`CommandDef::allows_in`] in English.
    pub fn allows(&self, roles: &[u64], lineage: &[u64]) -> Result<(), &'static str> {
        let any = |list: &[u64], held: &[u64]| held.iter().any(|id| list.contains(id));
        if any(&self.deny_channels, lineage) {
            return Err("That command can't be used in this channel.");
        }
        if !self.allow_channels.is_empty() && !any(&self.allow_channels, lineage) {
            return Err("That command can't be used in this channel.");
        }
        if any(&self.deny_roles, roles) {
            return Err("Your roles don't allow that command.");
        }
        if !self.allow_roles.is_empty() && !any(&self.allow_roles, roles) {
            return Err("That command is for members with a particular role.");
        }
        Ok(())
    }
}

// /Auto-responders

/// An auto-responder as a server writes it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResponderDef {
    pub name: String,
    pub trigger_kind: TriggerKind,
    /// A role id for `role_added` and `role_removed`, the keyword for `keyword`, the lowest level for
    /// `level_up` (empty: every level-up).
    #[serde(default)]
    pub trigger_value: Option<String>,
    #[serde(default)]
    pub match_kind: Option<MatchKind>,
    pub responses: Vec<String>,
    #[serde(default)]
    pub embed: Option<EmbedSpec>,
    #[serde(default)]
    pub destination: Destination,
    #[serde(default, with = "id_opt")]
    pub channel_id: Option<u64>,
    #[serde(default)]
    pub actions: Vec<Action>,
    /// Percent of the times it fires that it answers, 1 to 100.
    #[serde(default = "chance_always")]
    pub chance: i64,
    #[serde(default)]
    pub cooldown_secs: i64,
    #[serde(default, with = "id_list")]
    pub allow_channels: Vec<u64>,
    #[serde(default, with = "id_list")]
    pub deny_channels: Vec<u64>,
    #[serde(default = "yes")]
    pub enabled: bool,
    #[serde(default)]
    pub allow_mentions: bool,
}

impl ResponderDef {
    /// The responder made tidy and checked.
    pub fn clean(mut self) -> Result<Self, Vec<FieldError>> {
        let mut errors = Vec::new();
        self.name = self.name.split_whitespace().collect::<Vec<_>>().join(" ");
        if self.name.is_empty() || self.name.chars().count() > RESPONDER_NAME_MAX {
            errors.push(FieldError::new(
                "name",
                format!("A responder needs a name of 1 to {RESPONDER_NAME_MAX} characters."),
            ));
        }
        let kind = self.trigger_kind;
        let value = self
            .trigger_value
            .take()
            .map(|v| v.trim().to_owned())
            .filter(|v| !v.is_empty());
        match kind {
            TriggerKind::MemberJoin | TriggerKind::MemberLeave => {
                self.trigger_value = None;
                self.match_kind = None;
            }
            TriggerKind::LevelUp => {
                self.match_kind = None;
                self.trigger_value = match value {
                    None => None,
                    Some(v) => match v.parse::<i64>().ok().filter(|n| LEVEL_RANGE.contains(n)) {
                        Some(n) => Some(n.to_string()),
                        None => {
                            errors.push(FieldError::new(
                                "trigger_value",
                                format!(
                                    "The level goes from {} to {}, or leave it empty for every level-up.",
                                    LEVEL_RANGE.start(),
                                    LEVEL_RANGE.end()
                                ),
                            ));
                            None
                        }
                    },
                };
            }
            TriggerKind::RoleAdded | TriggerKind::RoleRemoved => {
                self.match_kind = None;
                self.trigger_value = match value.and_then(|v| v.parse::<u64>().ok()).filter(|id| *id != 0) {
                    Some(id) => Some(id.to_string()),
                    None => {
                        errors.push(FieldError::new("trigger_value", "Pick the role to watch."));
                        None
                    }
                };
            }
            TriggerKind::Keyword => {
                self.match_kind.get_or_insert(MatchKind::Contains);
                self.trigger_value = match value {
                    Some(v) if v.chars().count() <= KEYWORD_MAX => Some(v),
                    Some(_) => {
                        errors.push(FieldError::new(
                            "trigger_value",
                            format!("A keyword is at most {KEYWORD_MAX} characters."),
                        ));
                        None
                    }
                    None => {
                        errors.push(FieldError::new(
                            "trigger_value",
                            "Write the word or phrase to listen for.",
                        ));
                        None
                    }
                };
            }
        }
        // Where it answers has to exist when it happens.
        match (self.destination, kind) {
            (Destination::Current, TriggerKind::Keyword | TriggerKind::LevelUp) => self.channel_id = None,
            (Destination::Current, _) => errors.push(FieldError::new(
                "destination",
                "Nobody is chatting when that happens: pick a channel or a direct message.",
            )),
            (Destination::Dm, TriggerKind::MemberLeave) => errors.push(FieldError::new(
                "destination",
                "A member who left can't be messaged: pick a channel.",
            )),
            (Destination::Dm, _) => self.channel_id = None,
            (Destination::Channel, _) => {
                if self.channel_id.is_none_or(|id| id == 0) {
                    errors.push(FieldError::new("channel_id", "Pick the channel to answer in."));
                }
            }
        }
        self.embed = absorb(clean_embed(self.embed.take()), &mut errors);
        self.responses = clean_responses(std::mem::take(&mut self.responses), self.embed.is_some(), &mut errors);
        self.actions = absorb(
            clean_actions(
                std::mem::take(&mut self.actions),
                kind == TriggerKind::Keyword,
                kind != TriggerKind::MemberLeave,
            ),
            &mut errors,
        );
        if !(1..=100).contains(&self.chance) {
            errors.push(FieldError::new("chance", "The chance goes from 1 to 100 percent."));
            self.chance = self.chance.clamp(1, 100);
        }
        self.cooldown_secs = clean_cooldown(
            self.cooldown_secs,
            RESPONDER_COOLDOWN_MAX,
            !self.actions.is_empty(),
            &mut errors,
        );
        self.allow_channels = clean_list("allow_channels", std::mem::take(&mut self.allow_channels), &mut errors);
        self.deny_channels = clean_list("deny_channels", std::mem::take(&mut self.deny_channels), &mut errors);
        if errors.is_empty() { Ok(self) } else { Err(errors) }
    }

    /// The role a `role_added` or `role_removed` responder watches.
    pub fn watched_role(&self) -> Option<u64> {
        match self.trigger_kind {
            TriggerKind::RoleAdded | TriggerKind::RoleRemoved => self.trigger_value.as_deref()?.parse().ok(),
            _ => None,
        }
    }

    /// The lowest level a `level_up` responder fires at (1 when it fires for every level-up).
    pub fn min_level(&self) -> u32 {
        self.trigger_value.as_deref().and_then(|v| v.parse().ok()).unwrap_or(1)
    }

    /// Whether a message with this text trips a keyword responder.
    pub fn hears(&self, text: &str) -> bool {
        self.trigger_kind == TriggerKind::Keyword
            && self
                .trigger_value
                .as_deref()
                .zip(self.match_kind)
                .is_some_and(|(keyword, kind)| kind.matches(keyword, text))
    }

    /// Whether it may fire in a place described by `lineage`.
    pub fn allows_channel(&self, lineage: &[u64]) -> bool {
        let any = |list: &[u64]| lineage.iter().any(|id| list.contains(id));
        !any(&self.deny_channels) && (self.allow_channels.is_empty() || any(&self.allow_channels))
    }
}

#[cfg(test)]
mod tests;

mod embeds;
pub use embeds::{EmbedFieldSpec, EmbedSpec, clean_embed};

mod templates;
pub use templates::{Placeholder, Vars, command_placeholders, render, responder_placeholders, truncate};

mod definitions;
pub use definitions::Export;

mod actions;
pub use actions::{Action, clean_actions};
