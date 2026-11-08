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
    if !valid_template(&kit.content) || kit.variants.iter().any(|text| !valid_template(text)) {
        errors.push(FieldError::new(
            "content",
            "Use balanced, non-nested if/else conditions in at most 2,000 characters.",
        ));
    }
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
        if [&embed.url, &embed.image, &embed.thumbnail]
            .into_iter()
            .flatten()
            .any(|url| !valid_web_url(url))
        {
            errors.push(FieldError::new("embeds", "Use a valid HTTPS URL without credentials."));
        }
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
    if component_rows(&kit.components).len() > 5 {
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
                if url
                    .as_ref()
                    .is_some_and(|u| u.len() > 512 || u.contains(char::is_whitespace) || !valid_web_url(u))
                    || response
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

/// Pack adjacent buttons into rows of five; each select occupies its own row.
pub fn component_rows(components: &[KitComponent]) -> Vec<std::ops::Range<usize>> {
    let mut rows = Vec::new();
    let mut start = 0;
    while start < components.len() {
        let mut end = start + 1;
        if matches!(components[start], KitComponent::Button { .. }) {
            while end < components.len() && end - start < 5 && matches!(components[end], KitComponent::Button { .. }) {
                end += 1;
            }
        }
        rows.push(start..end);
        start = end;
    }
    rows
}

// Embed URLs are passed to Discord, never fetched by this server.
pub fn valid_web_url(raw: &str) -> bool {
    raw.len() <= 512
        && !raw.contains(char::is_whitespace)
        && url::Url::parse(raw).is_ok_and(|url| {
            url.scheme() == "https" && url.host().is_some() && url.username().is_empty() && url.password().is_none()
        })
}

/// Deliberately one level: no recursion, evaluation, or unbounded template input.
fn valid_template(text: &str) -> bool {
    if text.chars().count() > 2000 {
        return false;
    }
    let mut rest = text;
    let mut open = false;
    let mut otherwise = false;
    while let Some(at) = rest.find("{{") {
        rest = &rest[at + 2..];
        let Some(end) = rest.find("}}") else {
            return false;
        };
        let token = &rest[..end];
        if let Some(condition) = token.strip_prefix("if ") {
            if open
                || !(matches!(condition, "booster" | "returning")
                    || condition
                        .strip_prefix("account_age_days>=")
                        .is_some_and(|s| s.parse::<u32>().is_ok())
                    || condition
                        .strip_prefix("ordinal>=")
                        .is_some_and(|s| s.parse::<u64>().is_ok()))
            {
                return false;
            }
            open = true;
            otherwise = false;
        } else if token == "else" {
            if !open || otherwise {
                return false;
            }
            otherwise = true;
        } else if token == "end" {
            if !open {
                return false;
            }
            open = false;
        } else {
            return false;
        }
        rest = &rest[end + 2..];
    }
    !open
}

/// A small, bounded conditional language for member data. Unknown conditions stay literal.
pub fn conditional(text: &str, booster: bool, returning: bool, account_age_days: u32, ordinal: Option<u64>) -> String {
    if !valid_template(text) {
        return text.chars().take(2000).collect();
    }
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
    let template = conditional(raw, data.booster, data.returning, data.account_age_days, data.ordinal);
    let level = data.level.map_or_else(String::new, |n| n.to_string());
    let ordinal = data.ordinal.map_or_else(String::new, |n| n.to_string());
    let mut rest = template.as_str();
    let mut output = String::new();
    let mut remaining = 2000;
    // Scan the template once. Replacement values are data, never more template source.
    while !rest.is_empty() && remaining > 0 {
        let replacement = [
            ("{user}", data.user),
            ("{server}", data.server),
            ("{level}", level.as_str()),
            ("{ordinal}", ordinal.as_str()),
        ]
        .into_iter()
        .find(|(token, _)| rest.starts_with(token));
        if let Some((token, value)) = replacement {
            // Event handlers provide a generated user mention. It cannot grant a mass ping,
            // and allowed_mentions still controls whether that individual user is notified.
            let user_mention = value
                .strip_prefix("<@")
                .and_then(|s| s.strip_suffix('>'))
                .is_some_and(|s| s.strip_prefix('!').unwrap_or(s).parse::<u64>().is_ok_and(|id| id > 0));
            for c in value.chars() {
                if remaining == 0 {
                    break;
                }
                output.push(c);
                remaining -= 1;
                // Untrusted names cannot turn an approved template into a mass ping.
                if c == '@' && remaining > 0 && !user_mention {
                    output.push('\u{200b}');
                    remaining -= 1;
                }
            }
            rest = &rest[token.len()..];
        } else {
            let c = rest.chars().next().unwrap();
            output.push(c);
            remaining -= 1;
            rest = &rest[c.len_utf8()..];
        }
    }
    output
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
    #[test]
    fn template_replacements_are_literal_bounded_and_cannot_inject_pings() {
        let mut sample = kit();
        sample.content = "{user} {server} {level}".into();
        let data = MemberData {
            user: "{server}@everyone",
            server: "{level}",
            level: Some(9),
            booster: false,
            returning: false,
            account_age_days: 0,
            ordinal: None,
        };
        assert_eq!(render_content(&sample, 0, &data), "{server}@\u{200b}everyone {level} 9");
        let mention = MemberData { user: "<@42>", ..data };
        assert_eq!(render_content(&sample, 0, &mention), "<@42> {level} 9");
        let role = MemberData { user: "<@&42>", ..data };
        assert_eq!(render_content(&sample, 0, &role), "<@\u{200b}&42> {level} 9");
        sample.content = "{user}".repeat(333);
        let enormous = "{server}".repeat(100_000);
        let data = MemberData {
            user: &enormous,
            ..data
        };
        assert_eq!(render_content(&sample, 0, &data).chars().count(), 2000);
    }

    #[test]
    fn rejects_nested_malformed_and_unknown_conditions() {
        for text in [
            "{{if booster}}{{if returning}}x{{end}}{{end}}",
            "{{else}}x",
            "{{end}}",
            "{{if booster}}x",
            "{{if booster}}x{{else}}y{{else}}z{{end}}",
            "{{if exec(1)}}x{{end}}",
        ] {
            let mut sample = kit();
            sample.content = text.into();
            assert!(validate(sample).is_err(), "{text}");
            assert_eq!(conditional(text, true, true, 0, None), text);
        }
        assert_eq!(conditional(&"x".repeat(100_000), false, false, 0, None).len(), 2000);
    }

    #[test]
    fn button_rows_and_select_limits_are_enforced() {
        let button = KitComponent::Button {
            label: "Open".into(),
            url: None,
            response: Some("OK".into()),
        };
        let mut sample = kit();
        sample.components = vec![button.clone(); 25];
        assert!(validate(sample.clone()).is_ok());
        assert_eq!(
            component_rows(&sample.components),
            vec![0..5, 5..10, 10..15, 15..20, 20..25]
        );
        sample.components.push(button.clone());
        assert!(validate(sample).is_err());
        let mut sample = kit();
        let options: Vec<_> = (0..25)
            .map(|n| KitOption {
                label: n.to_string(),
                value: n.to_string(),
                response: "OK".into(),
            })
            .collect();
        sample.components = vec![
            KitComponent::Select {
                placeholder: "Pick".into(),
                options: options.clone()
            };
            5
        ];
        assert!(validate(sample.clone()).is_ok());
        sample.components.push(button);
        assert!(validate(sample).is_err());
        let mut options = options;
        options.push(KitOption {
            label: "26".into(),
            value: "26".into(),
            response: "OK".into(),
        });
        let mut sample = kit();
        sample.components = vec![KitComponent::Select {
            placeholder: "Pick".into(),
            options,
        }];
        assert!(validate(sample).is_err());
    }

    #[test]
    fn embed_urls_require_a_real_https_host_without_credentials() {
        for url in [
            "javascript:alert(1)",
            "file:///etc/passwd",
            "https://#fragment",
            "https://user:secret@example.com",
            "https://[broken",
        ] {
            let mut sample = kit();
            sample.embeds = vec![EmbedSpec {
                image: Some(url.into()),
                ..Default::default()
            }];
            assert!(validate(sample).is_err(), "{url}");
        }
        assert!(valid_web_url("https://example.com/image.png"));
    }

    #[test]
    fn all_discord_embed_and_content_boundaries_are_enforced() {
        use crate::tags::EmbedFieldSpec;
        let mut sample = kit();
        sample.content = "x".repeat(2000);
        assert!(validate(sample.clone()).is_ok());
        sample.content.push('x');
        assert!(validate(sample).is_err());
        for (field, max) in [("title", 256), ("description", 4096), ("author", 256), ("footer", 2048)] {
            let mut value = serde_json::json!({"description":"OK"});
            value[field] = serde_json::json!("x".repeat(max));
            let mut sample = kit();
            sample.embeds = vec![serde_json::from_value(value.clone()).unwrap()];
            assert!(validate(sample.clone()).is_ok());
            value[field] = serde_json::json!("x".repeat(max + 1));
            sample.embeds = vec![serde_json::from_value(value).unwrap()];
            assert!(validate(sample).is_err(), "{field}");
        }
        let field = EmbedFieldSpec {
            name: "x".repeat(256),
            value: "x".repeat(1024),
            inline: false,
        };
        let mut sample = kit();
        sample.embeds = vec![EmbedSpec {
            fields: vec![field],
            ..Default::default()
        }];
        assert!(validate(sample.clone()).is_ok());
        sample.embeds[0].fields[0].name.push('x');
        assert!(validate(sample.clone()).is_err());
        sample.embeds[0].fields[0].name = "x".into();
        sample.embeds[0].fields[0].value.push('x');
        assert!(validate(sample.clone()).is_err());
        sample.embeds[0].fields = vec![
            EmbedFieldSpec {
                name: "n".into(),
                value: "v".into(),
                inline: false
            };
            26
        ];
        assert!(validate(sample).is_err());
        let mut sample = kit();
        sample.embeds = vec![
            EmbedSpec {
                description: Some("x".repeat(600)),
                ..Default::default()
            };
            10
        ];
        assert!(validate(sample.clone()).is_ok());
        sample.embeds[0].description.as_mut().unwrap().push('x');
        assert!(validate(sample.clone()).is_err());
        sample.embeds = vec![
            EmbedSpec {
                description: Some("x".into()),
                ..Default::default()
            };
            11
        ];
        assert!(validate(sample).is_err());
    }
}
