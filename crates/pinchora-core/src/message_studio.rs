//! Validated message kits shared by the dashboard and bot.

use serde::{Deserialize, Serialize};

use crate::{
    settings::FieldError,
    tags::{EmbedSpec, clean_embed},
};

pub const KIT_BYTES_MAX: usize = 32 * 1024;
pub const KIT_NAME_MAX: usize = 60;
pub const KIT_COUNT_MAX: i64 = 50;
pub const VARIANTS_MAX: usize = 10;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MessageKit {
    pub name: String,
    pub content: String,
    #[serde(default)]
    pub variants: Vec<String>,
    #[serde(default)]
    pub embeds: Vec<EmbedSpec>,
    #[serde(default)]
    pub components: Vec<KitComponent>,
    #[serde(default)]
    pub allow_mass_mentions: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum KitComponent {
    Button {
        label: String,
        url: Option<String>,
        response: Option<String>,
    },
    Select {
        placeholder: String,
        options: Vec<KitOption>,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KitOption {
    pub label: String,
    pub value: String,
    pub response: String,
}

pub fn validate(mut kit: MessageKit) -> Result<MessageKit, Vec<FieldError>> {
    let mut errors = Vec::new();
    kit.name = kit.name.trim().to_owned();
    if kit.name.is_empty() || kit.name.chars().count() > KIT_NAME_MAX {
        errors.push(FieldError::new("name", "Use 1 to 60 characters for the kit name."));
    }
    if kit.content.chars().count() > 2000
        || kit.variants.len() > VARIANTS_MAX
        || kit
            .variants
            .iter()
            .any(|s| s.trim().is_empty() || s.chars().count() > 2000)
    {
        errors.push(FieldError::new(
            "content",
            "Content and each of at most 10 random variants must fit Discord's 2,000 characters.",
        ));
    }
    if kit.embeds.len() > 10 {
        errors.push(FieldError::new("embeds", "A message has at most 10 embeds."));
    }
    if kit.embeds.iter().map(EmbedSpec::total_chars).sum::<usize>() > 6000 {
        errors.push(FieldError::new(
            "embeds",
            "All embeds together have at most 6,000 characters.",
        ));
    }
    let mut cleaned = Vec::new();
    for embed in kit.embeds {
        match clean_embed(Some(embed)) {
            Ok(Some(value)) => cleaned.push(value),
            Ok(None) => {}
            Err(mut found) => errors.append(&mut found),
        }
    }
    kit.embeds = cleaned;
    if kit.content.trim().is_empty() && kit.variants.is_empty() && kit.embeds.is_empty() {
        errors.push(FieldError::new(
            "content",
            "Add content, a random variant, or an embed.",
        ));
    }
    if kit.components.len() > 5 {
        errors.push(FieldError::new("components", "A message has at most 5 component rows."));
    }
    for component in &kit.components {
        match component {
            KitComponent::Button { label, url, response } => {
                if label.trim().is_empty() || label.chars().count() > 80 || url.is_some() == response.is_some() {
                    errors.push(FieldError::new(
                        "components",
                        "A button needs a label and exactly one HTTPS link or reply.",
                    ));
                }
                if url.as_ref().is_some_and(|u| {
                    u.len() > 512
                        || u.contains(char::is_whitespace)
                        || !url::Url::parse(u).is_ok_and(|parsed| parsed.scheme() == "https" && parsed.host().is_some())
                }) || response
                    .as_ref()
                    .is_some_and(|r| r.trim().is_empty() || r.chars().count() > 2000)
                {
                    errors.push(FieldError::new(
                        "components",
                        "Use an HTTPS link under 512 characters or a reply under 2,000 characters.",
                    ));
                }
            }
            KitComponent::Select { placeholder, options } => {
                if placeholder.chars().count() > 150
                    || options.is_empty()
                    || options.len() > 25
                    || options.iter().any(|o| {
                        o.label.trim().is_empty()
                            || o.label.chars().count() > 100
                            || o.value.is_empty()
                            || o.value.len() > 100
                            || o.value.contains(':')
                            || o.response.trim().is_empty()
                            || o.response.chars().count() > 2000
                    })
                    || options
                        .iter()
                        .map(|o| o.value.as_str())
                        .collect::<std::collections::HashSet<_>>()
                        .len()
                        != options.len()
                {
                    errors.push(FieldError::new(
                        "components",
                        "A select needs 1 to 25 labeled choices with short values and replies.",
                    ));
                }
            }
        }
    }
    if serde_json::to_vec(&kit).is_ok_and(|v| v.len() > KIT_BYTES_MAX) {
        errors.push(FieldError::new("kit", "A saved kit is at most 32 KiB of JSON."));
    }
    if errors.is_empty() { Ok(kit) } else { Err(errors) }
}

/// A small, bounded conditional language for member data. Unknown conditions stay literal.
pub fn conditional(text: &str, booster: bool, returning: bool, account_age_days: u32, ordinal: Option<u64>) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find("{{if ") {
        out.push_str(&rest[..at]);
        let tail = &rest[at + 5..];
        let Some(close) = tail.find("}}") else {
            rest = &rest[at..];
            break;
        };
        let condition = &tail[..close];
        let body = &tail[close + 2..];
        let Some(end) = body.find("{{end}}") else {
            rest = &rest[at..];
            break;
        };
        let truth = match condition {
            "booster" => Some(booster),
            "returning" => Some(returning),
            _ => condition
                .strip_prefix("account_age_days>=")
                .and_then(|n| n.parse::<u32>().ok())
                .map(|n| account_age_days >= n)
                .or_else(|| {
                    condition
                        .strip_prefix("ordinal>=")
                        .and_then(|n| n.parse::<u64>().ok())
                        .map(|n| ordinal.is_some_and(|value| value >= n))
                }),
        };
        if let Some(truth) = truth {
            let branch = &body[..end];
            let (yes, no) = branch.split_once("{{else}}").unwrap_or((branch, ""));
            out.push_str(if truth { yes } else { no });
            rest = &body[end + 7..];
        } else {
            out.push_str(&rest[at..at + 5 + close + 2 + end + 7]);
            rest = &body[end + 7..];
        }
    }
    out.push_str(rest);
    out
}

pub struct MemberData<'a> {
    pub user: &'a str,
    pub server: &'a str,
    pub level: Option<u32>,
    pub booster: bool,
    pub returning: bool,
    pub account_age_days: u32,
    pub ordinal: Option<u64>,
}

pub fn render_content(kit: &MessageKit, variant: usize, data: &MemberData<'_>) -> String {
    let raw = if kit.variants.is_empty() {
        &kit.content
    } else {
        &kit.variants[variant % kit.variants.len()]
    };
    conditional(raw, data.booster, data.returning, data.account_age_days, data.ordinal)
        .replace("{user}", data.user)
        .replace("{server}", data.server)
        .replace("{level}", &data.level.map_or_else(String::new, |n| n.to_string()))
        .replace("{ordinal}", &data.ordinal.map_or_else(String::new, |n| n.to_string()))
        .chars()
        .take(2000)
        .collect()
}

/// Discord snowflake creation time, rounded down to whole days.
pub fn account_age_days(user_id: u64, now_ms: i64) -> u32 {
    let created_ms = ((user_id >> 22) as i64).saturating_add(1_420_070_400_000);
    now_ms
        .saturating_sub(created_ms)
        .max(0)
        .div_euclid(86_400_000)
        .min(u32::MAX as i64) as u32
}

#[cfg(test)]
mod tests {
    use super::*;
    fn kit() -> MessageKit {
        MessageKit {
            name: "Greeting".into(),
            content: "Hello {user}".into(),
            variants: Vec::new(),
            embeds: Vec::new(),
            components: Vec::new(),
            allow_mass_mentions: false,
        }
    }
    #[test]
    fn conditions_choose_one_branch() {
        assert_eq!(
            conditional("Hi {{if booster}}star{{else}}friend{{end}}!", true, false, 1, None),
            "Hi star!"
        );
        assert_eq!(
            conditional("{{if account_age_days>=7}}old{{else}}new{{end}}", false, false, 2, None),
            "new"
        );
        assert_eq!(
            conditional(
                "{{if ordinal>=100}}milestone{{else}}regular{{end}}",
                false,
                false,
                2,
                Some(100)
            ),
            "milestone"
        );
    }
    #[test]
    fn render_uses_member_data_and_variant() {
        let mut kit = kit();
        kit.variants = vec![
            "A".into(),
            "{{if returning}}Again{{else}}First{{end}} {user} in {server} at {level}".into(),
        ];
        let data = MemberData {
            user: "Ada",
            server: "Reef",
            level: Some(3),
            booster: false,
            returning: true,
            account_age_days: 100,
            ordinal: Some(42),
        };
        assert_eq!(render_content(&kit, 1, &data), "Again Ada in Reef at 3");
        assert_eq!(render_content(&kit, 2, &data), "A");
    }
    #[test]
    fn rejects_discord_limit_and_unsafe_components() {
        let mut kit = kit();
        kit.content = "x".repeat(2001);
        kit.components.push(KitComponent::Button {
            label: "click".into(),
            url: Some("http://example.com".into()),
            response: None,
        });
        let errors = validate(kit).unwrap_err();
        assert!(errors.iter().any(|e| e.field == "content"));
        assert!(errors.iter().any(|e| e.field == "components"));
    }
    #[test]
    fn mass_mentions_default_off() {
        assert!(!kit().allow_mass_mentions);
    }
    #[test]
    fn select_values_must_be_unique_and_links_need_a_host() {
        let mut sample = kit();
        sample.components = vec![
            KitComponent::Button {
                label: "Visit".into(),
                url: Some("https://".into()),
                response: None,
            },
            KitComponent::Select {
                placeholder: "Choose".into(),
                options: vec![
                    KitOption {
                        label: "A".into(),
                        value: "same".into(),
                        response: "A".into(),
                    },
                    KitOption {
                        label: "B".into(),
                        value: "same".into(),
                        response: "B".into(),
                    },
                ],
            },
        ];
        assert_eq!(
            validate(sample)
                .unwrap_err()
                .iter()
                .filter(|error| error.field == "components")
                .count(),
            2
        );
    }
}
