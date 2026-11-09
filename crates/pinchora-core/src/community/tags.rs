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

// /Embeds

/// A Discord embed, as far as a server can write one: text, a colour and two pictures.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EmbedSpec {
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    /// Makes the title a link.
    #[serde(default)]
    pub url: Option<String>,
    /// `#rrggbb`.
    #[serde(default)]
    pub color: Option<String>,
    #[serde(default)]
    pub author: Option<String>,
    #[serde(default)]
    pub footer: Option<String>,
    #[serde(default)]
    pub image: Option<String>,
    #[serde(default)]
    pub thumbnail: Option<String>,
    #[serde(default)]
    pub fields: Vec<EmbedFieldSpec>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EmbedFieldSpec {
    pub name: String,
    pub value: String,
    #[serde(default)]
    pub inline: bool,
}

impl EmbedSpec {
    /// Every character the embed shows, counted the way Discord counts its 6,000.
    pub fn total_chars(&self) -> usize {
        let text = |t: &Option<String>| t.as_deref().map_or(0, |t| t.chars().count());
        text(&self.title)
            + text(&self.description)
            + text(&self.author)
            + text(&self.footer)
            + self
                .fields
                .iter()
                .map(|f| f.name.chars().count() + f.value.chars().count())
                .sum::<usize>()
    }

    /// Whether there is anything to show.
    pub fn is_blank(&self) -> bool {
        let blank = |t: &Option<String>| t.as_deref().is_none_or(|t| t.trim().is_empty());
        blank(&self.title)
            && blank(&self.description)
            && blank(&self.image)
            && blank(&self.thumbnail)
            && blank(&self.footer)
            && blank(&self.author)
            && self.fields.is_empty()
    }

    /// The embed's colour as a number.
    pub fn color_value(&self) -> Option<u32> {
        self.color.as_deref().and_then(parse_hex_color)
    }

    /// Every text of the embed, for the message pipeline to fill.
    pub fn texts_mut(&mut self) -> Vec<&mut String> {
        let mut texts: Vec<&mut String> = Vec::new();
        texts.extend(
            [
                &mut self.title,
                &mut self.description,
                &mut self.author,
                &mut self.footer,
            ]
            .into_iter()
            .flatten(),
        );
        for field in &mut self.fields {
            texts.push(&mut field.name);
            texts.push(&mut field.value);
        }
        texts
    }
}

fn trimmed(text: &mut Option<String>) {
    if let Some(t) = text {
        let t2 = t.trim();
        *text = if t2.is_empty() { None } else { Some(t2.to_owned()) };
    }
}

fn check_len(errors: &mut Vec<FieldError>, text: &Option<String>, max: usize, what: &str) {
    if let Some(t) = text
        && t.chars().count() > max
    {
        errors.push(FieldError::new(
            "embed",
            format!("The embed's {what} is at most {max} characters."),
        ));
    }
}

fn check_url(errors: &mut Vec<FieldError>, url: &Option<String>, what: &str) {
    if let Some(u) = url
        && !(u.starts_with("https://")
            && u.len() > "https://".len()
            && u.len() <= URL_MAX
            && !u.contains(char::is_whitespace))
    {
        errors.push(FieldError::new(
            "embed",
            format!("The embed's {what} must be a web address starting with https:// (at most {URL_MAX} characters)."),
        ));
    }
}

/// An embed made tidy and checked; `Ok(None)` when it holds nothing at all.
pub fn clean_embed(embed: Option<EmbedSpec>) -> Result<Option<EmbedSpec>, Vec<FieldError>> {
    let Some(mut e) = embed else { return Ok(None) };
    for text in [
        &mut e.title,
        &mut e.description,
        &mut e.author,
        &mut e.footer,
        &mut e.url,
        &mut e.color,
    ] {
        trimmed(text);
    }
    trimmed(&mut e.image);
    trimmed(&mut e.thumbnail);
    let mut errors = Vec::new();
    check_len(&mut errors, &e.title, EMBED_TITLE_MAX, "title");
    check_len(&mut errors, &e.description, EMBED_DESCRIPTION_MAX, "description");
    check_len(&mut errors, &e.author, EMBED_AUTHOR_MAX, "author");
    check_len(&mut errors, &e.footer, EMBED_FOOTER_MAX, "footer");
    check_url(&mut errors, &e.url, "link");
    check_url(&mut errors, &e.image, "picture");
    check_url(&mut errors, &e.thumbnail, "thumbnail");
    if e.url.is_some() && e.title.is_none() {
        errors.push(FieldError::new("embed", "A link needs a title to sit on."));
    }
    if let Some(color) = &e.color {
        match parse_hex_color(color) {
            Some(value) => e.color = Some(format!("#{value:06x}")),
            None => errors.push(FieldError::new("embed", "The embed's colour looks like `#ff6b4a`.")),
        }
    }
    if e.fields.len() > EMBED_FIELDS_MAX {
        errors.push(FieldError::new(
            "embed",
            format!("An embed has at most {EMBED_FIELDS_MAX} fields."),
        ));
    }
    for (n, field) in e.fields.iter_mut().enumerate() {
        field.name = field.name.trim().to_owned();
        field.value = field.value.trim().to_owned();
        if field.name.is_empty() || field.value.is_empty() {
            errors.push(FieldError::new(
                "embed",
                format!("Field {} needs a name and a value.", n + 1),
            ));
        }
        if field.name.chars().count() > EMBED_FIELD_NAME_MAX || field.value.chars().count() > EMBED_FIELD_VALUE_MAX {
            errors.push(FieldError::new(
                "embed",
                format!(
                    "Field {} is too long: a name is at most {EMBED_FIELD_NAME_MAX} characters and a value {EMBED_FIELD_VALUE_MAX}.",
                    n + 1
                ),
            ));
        }
    }
    if e.total_chars() > EMBED_TOTAL_MAX {
        errors.push(FieldError::new(
            "embed",
            format!(
                "An embed holds at most {EMBED_TOTAL_MAX} characters in all; this one has {}.",
                e.total_chars()
            ),
        ));
    }
    if errors.is_empty() && e.is_blank() {
        errors.push(FieldError::new(
            "embed",
            "An embed needs a title, a description, a field or a picture. Remove it to send plain text.",
        ));
    }
    if errors.is_empty() { Ok(Some(e)) } else { Err(errors) }
}

// /Actions

/// Something a custom command or auto-responder does besides answering.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Action {
    AddRole {
        #[serde(with = "ids")]
        role_id: u64,
    },
    RemoveRole {
        #[serde(with = "ids")]
        role_id: u64,
    },
    ToggleRole {
        #[serde(with = "ids")]
        role_id: u64,
    },
    /// Gives (positive) or takes (negative) server points. Never shells: those are a global currency.
    GivePoints { amount: i64 },
    /// A direct message to the member.
    Dm { text: String },
    /// Reacts to the message that triggered a keyword responder.
    React { emoji: String },
}

impl Action {
    pub const fn key(&self) -> &'static str {
        match self {
            Self::AddRole { .. } => "add_role",
            Self::RemoveRole { .. } => "remove_role",
            Self::ToggleRole { .. } => "toggle_role",
            Self::GivePoints { .. } => "give_points",
            Self::Dm { .. } => "dm",
            Self::React { .. } => "react",
        }
    }

    /// The role the action hands out or takes, when it is a role action.
    pub const fn role(&self) -> Option<u64> {
        match self {
            Self::AddRole { role_id } | Self::RemoveRole { role_id } | Self::ToggleRole { role_id } => Some(*role_id),
            _ => None,
        }
    }
}

/// The actions made tidy and checked. `keyword` says whether a message can be reacted to, `member`
/// whether there is still a member to act on.
pub fn clean_actions(actions: Vec<Action>, keyword: bool, member: bool) -> Result<Vec<Action>, Vec<FieldError>> {
    let mut errors = Vec::new();
    if actions.len() > MAX_ACTIONS {
        errors.push(FieldError::new(
            "actions",
            format!("A command does at most {MAX_ACTIONS} things besides answering."),
        ));
    }
    let mut clean = Vec::with_capacity(actions.len());
    for (n, action) in actions.into_iter().enumerate() {
        let number = n + 1;
        if !member {
            errors.push(FieldError::new(
                "actions",
                format!("Action {number}: the member is gone by then, so there is nothing to do to them."),
            ));
            continue;
        }
        match action {
            Action::AddRole { role_id } | Action::RemoveRole { role_id } | Action::ToggleRole { role_id }
                if role_id == 0 =>
            {
                errors.push(FieldError::new("actions", format!("Action {number} needs a role.")));
            }
            Action::GivePoints { amount } if amount == 0 || !POINTS_ACTION.contains(&amount) => {
                errors.push(FieldError::new(
                    "actions",
                    format!(
                        "Action {number}: points go from {} to {}, and not 0.",
                        POINTS_ACTION.start(),
                        POINTS_ACTION.end()
                    ),
                ));
            }
            Action::Dm { text } => {
                let text = text.trim().to_owned();
                if text.is_empty() || text.chars().count() > RESPONSE_MAX {
                    errors.push(FieldError::new(
                        "actions",
                        format!("Action {number}: a direct message is 1 to {RESPONSE_MAX} characters."),
                    ));
                } else {
                    clean.push(Action::Dm { text });
                }
            }
            Action::React { .. } if !keyword => {
                errors.push(FieldError::new(
                    "actions",
                    format!("Action {number}: only a keyword responder has a message to react to."),
                ));
            }
            Action::React { emoji } => match check_points_emoji(&emoji) {
                Ok(emoji) => clean.push(Action::React { emoji }),
                Err(_) => errors.push(FieldError::new(
                    "actions",
                    format!(
                        "Action {number}: react with one emoji, a Unicode one like 🎉 or a custom one like <:name:123456789012345678>."
                    ),
                )),
            },
            other => clean.push(other),
        }
    }
    if errors.is_empty() { Ok(clean) } else { Err(errors) }
}

// /Texts

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

// /Export

/// What an export file holds: the commands (or responders) of one server, to be imported into another.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Export<T> {
    pub version: u32,
    pub items: Vec<T>,
}

impl<T> Export<T> {
    pub const VERSION: u32 = 1;

    pub fn new(items: Vec<T>) -> Self {
        Self {
            version: Self::VERSION,
            items,
        }
    }
}

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

#[cfg(test)]
mod tests {
    use serde_json::json;

    use crate::community::tags::*;

    fn command() -> CommandDef {
        CommandDef {
            name: "Rules".into(),
            description: "  The   rules ".into(),
            responses: vec!["Be kind.".into()],
            embed: None,
            reply_mode: ReplyMode::Public,
            actions: Vec::new(),
            allow_roles: Vec::new(),
            deny_roles: Vec::new(),
            allow_channels: Vec::new(),
            deny_channels: Vec::new(),
            cooldown_secs: 0,
            enabled: true,
            allow_mentions: false,
        }
    }

    fn responder(kind: TriggerKind) -> ResponderDef {
        ResponderDef {
            name: "Greeter".into(),
            trigger_kind: kind,
            trigger_value: None,
            match_kind: None,
            responses: vec!["Hi {user}".into()],
            embed: None,
            destination: Destination::Channel,
            channel_id: Some(5),
            actions: Vec::new(),
            chance: 100,
            cooldown_secs: 0,
            allow_channels: Vec::new(),
            deny_channels: Vec::new(),
            enabled: true,
            allow_mentions: false,
        }
    }

    fn reserved() -> Vec<String> {
        vec!["help".into(), "tag".into()]
    }

    fn fields(errors: Vec<FieldError>) -> Vec<&'static str> {
        errors.into_iter().map(|e| e.field).collect()
    }

    fn pick_first(_: usize) -> usize {
        0
    }

    #[test]
    fn a_command_is_made_tidy() {
        let clean = command().clean(&reserved()).expect("valid");
        assert_eq!(
            (clean.name.as_str(), clean.description.as_str()),
            ("rules", "The rules")
        );
    }

    #[test]
    fn names_follow_the_rule_and_skip_the_bots_own_commands() {
        for name in ["a", "rules", "0day", "my-cmd_2", &"x".repeat(32), "/Slash"] {
            assert!(check_name(name, &reserved()).is_ok(), "{name}");
        }
        for name in ["", "-a", "_a", "has space", "dot.dot", "ünï", &"x".repeat(33)] {
            assert!(check_name(name, &reserved()).is_err(), "{name:?}");
        }
        assert_eq!(check_name("HELP", &reserved()).unwrap_err().message, RESERVED_NOTE);
    }

    #[test]
    fn replies_are_counted_and_an_embed_can_stand_in_for_text() {
        let mut c = command();
        c.responses = vec![];
        assert_eq!(fields(c.clean(&reserved()).unwrap_err()), ["responses"]);
        let mut c = command();
        c.responses = vec!["x".into(); 6];
        assert_eq!(fields(c.clean(&reserved()).unwrap_err()), ["responses"]);
        let mut c = command();
        c.responses = vec!["é".repeat(RESPONSE_MAX + 1)];
        assert!(c.clean(&reserved()).unwrap_err()[0].message.contains("2001"));
        let mut c = command();
        c.responses = vec!["é".repeat(RESPONSE_MAX)];
        assert!(c.clean(&reserved()).is_ok(), "characters, not bytes");
        let mut c = command();
        c.responses = vec!["  ".into()];
        assert_eq!(fields(c.clean(&reserved()).unwrap_err()), ["responses"]);
        let mut c = command();
        c.responses = vec![String::new()];
        c.embed = Some(EmbedSpec {
            title: Some("Rules".into()),
            ..Default::default()
        });
        assert!(c.clean(&reserved()).is_ok(), "the embed carries the answer");
    }

    #[test]
    fn embeds_are_limited_like_discord_limits_them() {
        let ok = |e: EmbedSpec| clean_embed(Some(e));
        let titled = |t: &str| EmbedSpec {
            title: Some(t.into()),
            ..Default::default()
        };
        assert!(ok(titled("Hello")).unwrap().is_some());
        assert_eq!(clean_embed(None).unwrap(), None);
        assert!(ok(titled(&"x".repeat(257))).is_err());
        assert!(ok(EmbedSpec::default()).is_err(), "an empty embed is refused");
        assert!(
            ok(EmbedSpec {
                title: Some("  ".into()),
                ..Default::default()
            })
            .is_err()
        );
        let mut e = titled("T");
        e.color = Some("ff6b4a".into());
        assert_eq!(ok(e).unwrap().unwrap().color.as_deref(), Some("#ff6b4a"));
        let mut e = titled("T");
        e.color = Some("red".into());
        assert!(ok(e).is_err());
        let mut e = titled("T");
        e.image = Some("http://example.com/a.png".into());
        assert!(ok(e).is_err(), "https only");
        let mut e = titled("T");
        e.image = Some("https://example.com/a.png".into());
        assert!(ok(e).is_ok());
        let mut e = titled("T");
        e.url = Some("https://example.com".into());
        assert!(ok(e).is_ok());
        let no_title = EmbedSpec {
            description: Some("d".into()),
            url: Some("https://example.com".into()),
            ..Default::default()
        };
        assert!(ok(no_title).is_err(), "a link needs a title");
        let field = |n: &str, v: &str| EmbedFieldSpec {
            name: n.into(),
            value: v.into(),
            inline: false,
        };
        let mut e = titled("T");
        e.fields = (0..26).map(|_| field("a", "b")).collect();
        assert!(ok(e).is_err(), "26 fields");
        let mut e = titled("T");
        e.fields = vec![field("a", "")];
        assert!(ok(e).is_err(), "an empty field value");
        let mut e = EmbedSpec {
            description: Some("d".repeat(4_096)),
            footer: Some("f".repeat(2_048)),
            ..Default::default()
        };
        let error = &ok(e.clone()).unwrap_err()[0].message;
        assert!(
            error.contains("6000") && error.contains("6144"),
            "over the total: {error}"
        );
        e.footer = Some("f".repeat(1_000));
        assert!(ok(e).is_ok(), "under the total");
    }

    #[test]
    fn actions_are_limited_and_checked() {
        let add = |id| Action::AddRole { role_id: id };
        assert!(clean_actions(vec![add(1), add(2), add(3)], false, true).is_ok());
        assert!(clean_actions(vec![add(1), add(2), add(3), add(4)], false, true).is_err());
        assert!(clean_actions(vec![add(0)], false, true).is_err());
        for amount in [0, 1_001, -1_001] {
            assert!(
                clean_actions(vec![Action::GivePoints { amount }], false, true).is_err(),
                "{amount}"
            );
        }
        for amount in [1, -1, 1_000, -1_000] {
            assert!(
                clean_actions(vec![Action::GivePoints { amount }], false, true).is_ok(),
                "{amount}"
            );
        }
        let dm = Action::Dm { text: "  hi ".into() };
        assert_eq!(
            clean_actions(vec![dm], false, true).unwrap(),
            [Action::Dm { text: "hi".into() }]
        );
        assert!(clean_actions(vec![Action::Dm { text: " ".into() }], false, true).is_err());
        let react = || Action::React { emoji: "🎉".into() };
        assert!(clean_actions(vec![react()], true, true).is_ok());
        assert!(
            clean_actions(vec![react()], false, true).is_err(),
            "only a keyword has a message"
        );
        assert!(
            clean_actions(
                vec![Action::React {
                    emoji: "two words".into()
                }],
                true,
                true
            )
            .is_err()
        );
        assert!(clean_actions(vec![add(1)], true, false).is_err(), "the member is gone");
        assert!(clean_actions(vec![], true, false).unwrap().is_empty());
    }

    #[test]
    fn actions_travel_as_json_with_string_ids() {
        let action = Action::ToggleRole {
            role_id: 123_456_789_012_345_678,
        };
        let value = serde_json::to_value(&action).unwrap();
        assert_eq!(value, json!({ "kind": "toggle_role", "role_id": "123456789012345678" }));
        assert_eq!(serde_json::from_value::<Action>(value).unwrap(), action);
        assert_eq!(
            serde_json::from_value::<Action>(json!({ "kind": "add_role", "role_id": 5 })).unwrap(),
            Action::AddRole { role_id: 5 }
        );
        assert!(serde_json::from_value::<Action>(json!({ "kind": "add_role", "role_id": "x" })).is_err());
        assert!(serde_json::from_value::<Action>(json!({ "kind": "give_points", "amount": 5, "extra": 1 })).is_err());
        assert!(serde_json::from_value::<Action>(json!({ "kind": "format_disk" })).is_err());
    }

    #[test]
    fn a_command_reads_from_json_with_defaults() {
        let def: CommandDef = serde_json::from_value(json!({
            "name": "hi", "responses": ["hello"], "allow_roles": ["5", 6], "deny_channels": ["7"]
        }))
        .unwrap();
        assert_eq!(def.allow_roles, [5, 6]);
        assert_eq!(def.deny_channels, [7]);
        assert!(def.enabled && !def.allow_mentions);
        assert_eq!(def.reply_mode, ReplyMode::Public);
        let back = serde_json::to_value(&def).unwrap();
        assert_eq!(back["allow_roles"], json!(["5", "6"]));
        assert!(
            serde_json::from_value::<CommandDef>(json!({ "name": "x", "responses": ["y"], "surprise": 1 })).is_err()
        );
    }

    #[test]
    fn lists_and_cooldowns_are_bounded() {
        let mut c = command();
        c.allow_roles = (1..=26).collect();
        assert_eq!(fields(c.clean(&reserved()).unwrap_err()), ["allow_roles"]);
        let mut c = command();
        c.deny_roles = vec![3, 3, 4];
        assert_eq!(c.clean(&reserved()).unwrap().deny_roles, [3, 4], "repeats go");
        let mut c = command();
        c.cooldown_secs = 3_601;
        assert_eq!(fields(c.clean(&reserved()).unwrap_err()), ["cooldown_secs"]);
        let mut c = command();
        c.cooldown_secs = -1;
        assert_eq!(fields(c.clean(&reserved()).unwrap_err()), ["cooldown_secs"]);
        let mut c = command();
        c.cooldown_secs = 3_600;
        assert!(c.clean(&reserved()).is_ok());
    }

    #[test]
    fn actions_need_a_cooldown_of_their_own() {
        let mut c = command();
        c.actions = vec![Action::GivePoints { amount: 1_000 }];
        for seconds in [0, ACTION_COOLDOWN_MIN - 1] {
            c.cooldown_secs = seconds;
            let errors = c.clone().clean(&reserved()).unwrap_err();
            assert_eq!(fields(errors.clone()), ["cooldown_secs"], "{seconds}");
            assert!(
                errors[0].message.contains("at least 30 seconds"),
                "{}",
                errors[0].message
            );
        }
        c.cooldown_secs = ACTION_COOLDOWN_MIN;
        assert!(c.clean(&reserved()).is_ok());
        let mut r = responder(TriggerKind::LevelUp);
        r.actions = vec![Action::Dm { text: "hi".into() }];
        assert_eq!(fields(r.clone().clean().unwrap_err()), ["cooldown_secs"]);
        r.cooldown_secs = 60;
        assert!(r.clean().is_ok());
        // Without actions any cooldown is fine; a stored row without one still runs with the floor.
        assert!(command().clean(&reserved()).is_ok());
        assert_eq!(run_cooldown(0, &[]), 0);
        assert_eq!(
            run_cooldown(0, &[Action::GivePoints { amount: 5 }]),
            ACTION_COOLDOWN_MIN
        );
        assert_eq!(run_cooldown(90, &[Action::GivePoints { amount: 5 }]), 90);
    }

    #[test]
    fn who_may_run_a_command_and_where() {
        let mut c = command();
        assert_eq!(c.allows(&[], &[1]), Ok(()));
        c.allow_roles = vec![10];
        assert!(c.allows(&[], &[1]).is_err());
        assert!(c.allows(&[10, 11], &[1]).is_ok());
        c.deny_roles = vec![11];
        assert!(c.allows(&[10, 11], &[1]).is_err(), "a deny wins");
        c.allow_channels = vec![2];
        assert!(c.allows(&[10], &[1]).is_err(), "not in the allowed channel");
        assert!(c.allows(&[10], &[1, 2]).is_ok(), "its category counts");
        c.deny_channels = vec![1];
        assert!(c.allows(&[10], &[1, 2]).is_err());
    }

    #[test]
    fn a_responder_needs_what_its_trigger_needs() {
        let mut r = responder(TriggerKind::Keyword);
        assert_eq!(fields(r.clone().clean().unwrap_err()), ["trigger_value"]);
        r.trigger_value = Some("  Hello  ".into());
        let clean = r.clone().clean().unwrap();
        assert_eq!(clean.trigger_value.as_deref(), Some("Hello"));
        assert_eq!(clean.match_kind, Some(MatchKind::Contains), "contains unless said");

        let mut r = responder(TriggerKind::RoleAdded);
        assert_eq!(fields(r.clone().clean().unwrap_err()), ["trigger_value"]);
        r.trigger_value = Some("0".into());
        assert!(r.clone().clean().is_err());
        r.trigger_value = Some("42".into());
        assert_eq!(r.clean().unwrap().watched_role(), Some(42));

        let mut r = responder(TriggerKind::LevelUp);
        assert_eq!(r.clone().clean().unwrap().min_level(), 1);
        r.trigger_value = Some("10".into());
        assert_eq!(r.clone().clean().unwrap().min_level(), 10);
        r.trigger_value = Some("0".into());
        assert_eq!(fields(r.clone().clean().unwrap_err()), ["trigger_value"]);
        r.trigger_value = Some("many".into());
        assert!(r.clean().is_err());

        let mut r = responder(TriggerKind::MemberJoin);
        r.trigger_value = Some("ignored".into());
        r.match_kind = Some(MatchKind::Exact);
        let clean = r.clean().unwrap();
        assert_eq!((clean.trigger_value, clean.match_kind), (None, None));
    }

    #[test]
    fn a_responder_answers_somewhere_that_exists() {
        let mut r = responder(TriggerKind::MemberJoin);
        r.destination = Destination::Current;
        assert_eq!(fields(r.clone().clean().unwrap_err()), ["destination"]);
        r.destination = Destination::Dm;
        assert!(r.clone().clean().unwrap().channel_id.is_none());
        r.destination = Destination::Channel;
        r.channel_id = None;
        assert_eq!(fields(r.clone().clean().unwrap_err()), ["channel_id"]);

        let mut leave = responder(TriggerKind::MemberLeave);
        leave.destination = Destination::Dm;
        assert_eq!(fields(leave.clone().clean().unwrap_err()), ["destination"]);
        leave.destination = Destination::Channel;
        leave.actions = vec![Action::GivePoints { amount: 5 }];
        assert_eq!(
            fields(leave.clean().unwrap_err()),
            ["actions"],
            "nobody left to give to"
        );

        let mut talk = responder(TriggerKind::Keyword);
        talk.trigger_value = Some("hi".into());
        talk.destination = Destination::Current;
        talk.actions = vec![Action::React { emoji: "👋".into() }];
        talk.cooldown_secs = ACTION_COOLDOWN_MIN;
        assert!(talk.clean().unwrap().channel_id.is_none());
    }

    #[test]
    fn chance_and_cooldown_have_ranges() {
        for chance in [0, 101, -5] {
            let mut r = responder(TriggerKind::MemberJoin);
            r.chance = chance;
            assert_eq!(fields(r.clean().unwrap_err()), ["chance"], "{chance}");
        }
        let mut r = responder(TriggerKind::MemberJoin);
        r.chance = 1;
        r.cooldown_secs = 86_400;
        assert!(r.clone().clean().is_ok());
        r.cooldown_secs = 86_401;
        assert_eq!(fields(r.clean().unwrap_err()), ["cooldown_secs"]);
    }

    #[test]
    fn keywords_match_by_kind_without_case() {
        let hears = |kind: MatchKind, text: &str| kind.matches("Hello", text);
        assert!(hears(MatchKind::Contains, "oh HELLO there"));
        assert!(!hears(MatchKind::Contains, "help"));
        assert!(hears(MatchKind::Exact, "  hello "));
        assert!(!hears(MatchKind::Exact, "hello there"));
        assert!(hears(MatchKind::StartsWith, "Hello world"));
        assert!(!hears(MatchKind::StartsWith, "oh hello"));
        assert!(hears(MatchKind::EndsWith, "oh hello"));
        assert!(!hears(MatchKind::Contains, ""));
        assert!(
            !MatchKind::Contains.matches("  ", "anything"),
            "an empty keyword hears nothing"
        );
        let mut r = responder(TriggerKind::Keyword);
        r.trigger_value = Some("gm".into());
        r.match_kind = Some(MatchKind::Exact);
        assert!(r.hears("GM") && !r.hears("gm all"));
        assert!(!responder(TriggerKind::MemberJoin).hears("gm"));
    }

    #[test]
    fn responder_channels_follow_allow_and_deny() {
        let mut r = responder(TriggerKind::Keyword);
        assert!(r.allows_channel(&[1]));
        r.allow_channels = vec![2];
        assert!(!r.allows_channel(&[1]));
        assert!(r.allows_channel(&[1, 2]));
        r.deny_channels = vec![1];
        assert!(!r.allows_channel(&[1, 2]));
    }

    fn filled(template: &str, vars: &Vars) -> String {
        render(template, vars, &mut pick_first)
    }

    #[test]
    fn named_placeholders_are_filled_once() {
        let mut v = Vars::new();
        v.set("user", "<@1>").set("server", "Reef");
        assert_eq!(
            filled("Hi {user}, welcome to {server}! {user}", &v),
            "Hi <@1>, welcome to Reef! <@1>"
        );
        assert_eq!(filled("{level} {", &v), "{level} {", "unknown and unclosed stay");
        assert_eq!(filled("{{user}}", &v), "{<@1>}");
        v.set("user", "{server}");
        assert_eq!(
            filled("{user} in {server}", &v),
            "{server} in Reef",
            "a value is not filled again"
        );
        assert_eq!(filled("", &v), "");
        assert_eq!(filled("👋 {server} — ok", &v), "👋 Reef — ok");
    }

    #[test]
    fn words_come_from_what_was_typed() {
        let mut v = Vars::new();
        v.args("red  green blue yellow orange purple");
        assert_eq!(filled("{args}", &v), "red  green blue yellow orange purple");
        assert_eq!(filled("{1} {2} {5}", &v), "red green orange");
        assert_eq!(filled("{6}", &v), "{6}", "only five words");
        assert_eq!(filled("{0}", &v), "{0}");
        let mut v = Vars::new();
        v.args("");
        assert_eq!(filled("[{1}] [{1:friend}]", &v), "[] [friend]");
        v.args("Sam");
        assert_eq!(filled("Hi {1:friend}!", &v), "Hi Sam!");
        assert_eq!(filled("{01}", &v), "Sam", "leading zeros count as the number");
        assert_eq!(filled("{+1}", &v), "{+1}");
    }

    #[test]
    fn random_picks_by_the_index_it_is_given() {
        let v = Vars::new();
        let mut last = |n: usize| n - 1;
        assert_eq!(render("{random:a|b|c}", &v, &mut last), "c");
        assert_eq!(render("{random:only}", &v, &mut last), "only");
        assert_eq!(render("{random:}", &v, &mut last), "");
        let mut high = |_: usize| 99;
        assert_eq!(
            render("{random:a|b}", &v, &mut high),
            "b",
            "an index past the end is held back"
        );
        assert_eq!(render("{random}", &v, &mut last), "{random}");
        assert_eq!(render("x {random:a|b} y {random:c|d}", &v, &mut last), "x b y d");
        let many = format!(
            "{{random:{}}}",
            (0..40).map(|n| n.to_string()).collect::<Vec<_>>().join("|")
        );
        assert_eq!(render(&many, &v, &mut last), "24", "the first 25 choices count");
    }

    #[test]
    fn the_catalogue_names_what_the_editor_offers() {
        let all = command_placeholders();
        assert!(all.iter().any(|p| p.token == "{args}") && all.iter().any(|p| p.token == "{1:friend}"));
        for kind in TriggerKind::ALL {
            let tokens: Vec<&str> = responder_placeholders(kind).iter().map(|p| p.token).collect();
            assert!(tokens.contains(&"{user}") && tokens.contains(&"{points}"), "{kind:?}");
            assert!(!tokens.contains(&"{args}"), "{kind:?}");
        }
        let tokens = |kind| responder_placeholders(kind).iter().map(|p| p.token).collect::<Vec<_>>();
        assert!(tokens(TriggerKind::LevelUp).contains(&"{level}"));
        assert!(tokens(TriggerKind::MemberJoin).contains(&"{count}"));
        assert!(tokens(TriggerKind::RoleAdded).contains(&"{role}"));
        assert!(!tokens(TriggerKind::Keyword).contains(&"{level}"));
    }

    #[test]
    fn long_text_is_cut_with_an_ellipsis() {
        assert_eq!(truncate("short", 10), "short");
        assert_eq!(truncate("abcdef", 4), "abc…");
        assert_eq!(truncate("éééé", 4), "éééé");
        assert_eq!(truncate("", 0), "");
    }

    #[test]
    fn keys_round_trip() {
        for kind in TriggerKind::ALL {
            assert_eq!(TriggerKind::parse(kind.key()), Some(kind));
            assert_eq!(serde_json::to_value(kind).unwrap(), json!(kind.key()));
        }
        for kind in MatchKind::ALL {
            assert_eq!(MatchKind::parse(kind.key()), Some(kind));
            assert_eq!(serde_json::to_value(kind).unwrap(), json!(kind.key()));
        }
        for d in [Destination::Current, Destination::Channel, Destination::Dm] {
            assert_eq!(Destination::parse(d.key()), Some(d));
            assert_eq!(serde_json::to_value(d).unwrap(), json!(d.key()));
        }
        assert_eq!(ReplyMode::parse("ephemeral"), Some(ReplyMode::Ephemeral));
        assert_eq!(Action::GivePoints { amount: 1 }.key(), "give_points");
    }

    #[test]
    fn an_export_wraps_its_items_with_a_version() {
        let export = Export::new(vec![command()]);
        let value = serde_json::to_value(&export).unwrap();
        assert_eq!(value["version"], 1);
        let back: Export<CommandDef> = serde_json::from_value(value).unwrap();
        assert_eq!(back, export);
    }
}
