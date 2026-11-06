//! The settings rules as the browser needs them, written to `web/src/lib/rules.json`: the limits,
//! catalogues and defaults the forms show, and sample checks with the exact answers
//! `pinchora_core::settings` gives. The SPA's instant checks are tested against those samples, so a
//! rule changed here and forgotten there fails `committed_rules_match`.
//!
//! Rewrite after changing a rule: `cargo test -p pinchora-web rules -- --ignored write_rules`.

use pinchora_core::{
    leveling::{MAX_MULTIPLIER, VOICE_XP_RANGE},
    settings::{self, FieldError, MessageKind},
    tags::{self, MatchKind, TriggerKind},
};
use pinchora_db::{MAX_LEVEL_ROLES, MAX_REP_ROLES, MAX_STORE_ITEMS, MAX_XP_RULES};
use pinchora_render::{ACCENT_PRESETS, Theme};
use serde_json::{Value, json};

const PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../web/src/lib/rules.json");

fn answer<T: Into<Value>>(result: Result<T, FieldError>) -> Value {
    match result {
        Ok(value) => json!({ "ok": true, "value": value.into() }),
        Err(error) => json!({ "ok": false, "field": error.field, "message": error.message }),
    }
}

fn cases() -> Vec<Value> {
    let mut cases = Vec::new();
    let mut case = |check: &str, input: Value, result: Value| {
        cases.push(json!({ "check": check, "input": input, "result": result }));
    };
    for input in ["#ff6b4a", "06d6a0", " #FFD166 ", "#fff", "red", "#gg0000"] {
        case("accent", json!(input), answer(settings::check_accent(input)));
    }
    let long = "x".repeat(settings::MESSAGE_MAX + 1);
    let full = "é".repeat(settings::MESSAGE_MAX);
    for kind in MessageKind::ALL {
        for input in ["Hi {user}!", "   ", long.as_str(), full.as_str()] {
            case(
                "message",
                json!({ "kind": kind.key(), "text": input }),
                answer(settings::check_message(kind, input).map(|()| true)),
            );
        }
    }
    for input in ["WELCOME", "  hi there ", "", "THIS IS FAR TOO LONG"] {
        case("title", json!(input), answer(settings::check_welcome_title(input)));
    }
    for input in ["coins", " shells ", "", &"p".repeat(33)] {
        case("points_name", json!(input), answer(settings::check_points_name(input)));
    }
    for input in ["⭐", " 🪙 ", "🇹🇷", "<:coin:123456789012345678>", "coins", "⭐⭐", ""] {
        case(
            "points_emoji",
            json!(input),
            answer(settings::check_points_emoji(input)),
        );
    }
    for (min, max) in [(1, 3), (0, 1000), (5, 4), (-1, 3), (0, 1001)] {
        case(
            "points",
            json!([min, max]),
            answer(settings::check_points(min, max).map(|()| true)),
        );
    }
    for input in [1.5, 0.0, 1.234, 5.004, 5.01, -1.0] {
        case("multiplier", json!(input), answer(settings::check_multiplier(input)));
    }
    for input in [10, 1, 100, 0, 101] {
        case(
            "voice_xp_amount",
            json!(input),
            answer(settings::check_voice_amount(input)),
        );
    }
    for input in [1, 1000, 0, 1001] {
        case("level", json!(input), answer(settings::check_level(input)));
    }
    for input in [1, 100_000, 0] {
        case("reputation", json!(input), answer(settings::check_reputation(input)));
    }
    for input in [1, 1_000_000_000, 0, 1_000_000_001] {
        case("price", json!(input), answer(settings::check_price(input)));
    }
    for input in [1, 1_000_000, 0] {
        case("stock", json!(input), answer(settings::check_stock(input)));
    }
    let reserved = pinchora_commands::reserved_command_names();
    for input in [
        "rules",
        " Rules ",
        "/slash",
        "my-cmd_2",
        "0day",
        "has space",
        "-x",
        "_x",
        "help",
        "tag",
        "",
        "ünï",
        &"x".repeat(33),
    ] {
        case("command_name", json!(input), answer(tags::check_name(input, reserved)));
    }
    cases
}

fn tag_rules() -> Value {
    let range = |r: &std::ops::RangeInclusive<i64>| json!([r.start(), r.end()]);
    json!({
        "limits": {
            "max_commands": tags::MAX_COMMANDS,
            "max_responders": tags::MAX_RESPONDERS,
            "max_responses": tags::MAX_RESPONSES,
            "response_max": tags::RESPONSE_MAX,
            "name_max": tags::NAME_MAX,
            "description_max": tags::DESCRIPTION_MAX,
            "responder_name_max": tags::RESPONDER_NAME_MAX,
            "keyword_max": tags::KEYWORD_MAX,
            "max_actions": tags::MAX_ACTIONS,
            "max_list": tags::MAX_LIST,
            "command_cooldown_max": tags::COMMAND_COOLDOWN_MAX,
            "responder_cooldown_max": tags::RESPONDER_COOLDOWN_MAX,
            "action_cooldown_min": tags::ACTION_COOLDOWN_MIN,
            "action_points_daily_max": tags::ACTION_POINTS_DAILY_MAX,
            "action_dms_per_hour": tags::ACTION_DMS_PER_HOUR,
            "points_action": range(&tags::POINTS_ACTION),
            "args_max": tags::ARGS_MAX,
            "embed_title_max": tags::EMBED_TITLE_MAX,
            "embed_description_max": tags::EMBED_DESCRIPTION_MAX,
            "embed_author_max": tags::EMBED_AUTHOR_MAX,
            "embed_footer_max": tags::EMBED_FOOTER_MAX,
            "embed_field_name_max": tags::EMBED_FIELD_NAME_MAX,
            "embed_field_value_max": tags::EMBED_FIELD_VALUE_MAX,
            "embed_fields_max": tags::EMBED_FIELDS_MAX,
            "embed_total_max": tags::EMBED_TOTAL_MAX,
        },
        "reserved": pinchora_commands::reserved_command_names(),
        "triggers": TriggerKind::ALL
            .iter()
            .map(|k| json!({ "key": k.key(), "label": k.label(), "needs_message_content": k.needs_message_content() }))
            .collect::<Vec<_>>(),
        "match_kinds": MatchKind::ALL.iter().map(|k| k.key()).collect::<Vec<_>>(),
        "command_placeholders": tags::command_placeholders(),
        "responder_placeholders": TriggerKind::ALL
            .iter()
            .map(|k| (k.key().to_owned(), json!(tags::responder_placeholders(*k))))
            .collect::<serde_json::Map<_, _>>(),
    })
}

fn rules() -> Value {
    let range = |r: &std::ops::RangeInclusive<i64>| json!([r.start(), r.end()]);
    let messages: serde_json::Map<String, Value> = MessageKind::ALL
        .iter()
        .map(|kind| {
            (
                kind.key().to_owned(),
                json!({
                    "field": kind.field(),
                    "placeholders": kind.placeholders(),
                    "default": kind.default_message(),
                }),
            )
        })
        .collect();
    json!({
        "limits": {
            "message_max": settings::MESSAGE_MAX,
            "welcome_title_max": settings::WELCOME_TITLE_MAX,
            "points_name_max": settings::POINTS_NAME_MAX,
            "points_emoji_max": settings::POINTS_EMOJI_MAX,
            "points": range(&settings::POINTS_RANGE),
            "level": range(&settings::LEVEL_RANGE),
            "reputation": range(&settings::REPUTATION_RANGE),
            "price": range(&settings::PRICE_RANGE),
            "stock": range(&settings::STOCK_RANGE),
            "voice_xp": range(&VOICE_XP_RANGE),
            "multiplier": [0.0, MAX_MULTIPLIER],
            "max_xp_rules": MAX_XP_RULES,
            "max_level_roles": MAX_LEVEL_ROLES,
            "max_rep_roles": MAX_REP_ROLES,
            "max_store_items": MAX_STORE_ITEMS,
        },
        "themes": settings::WELCOME_THEMES
            .iter()
            .map(|(key, name)| {
                let theme = Theme::preset(key);
                json!({
                    "key": key,
                    "name": name,
                    "accent": theme.accent,
                    "top": theme.bg_top,
                    "bottom": theme.bg_bottom,
                    "edge": theme.panel_edge,
                })
            })
            .collect::<Vec<_>>(),
        "accents": ACCENT_PRESETS
            .iter()
            .map(|(name, rgb)| json!({ "name": name, "hex": format!("#{rgb:06x}") }))
            .collect::<Vec<_>>(),
        "levelup_modes": settings::LEVELUP_MODES.map(|m| m.key()),
        "messages": messages,
        "tags": tag_rules(),
        "defaults": {
            "theme": settings::DEFAULT_THEME,
            "welcome_title": settings::DEFAULT_WELCOME_TITLE,
        },
        "cases": cases(),
    })
}

fn rendered() -> String {
    let mut text = serde_json::to_string_pretty(&rules()).expect("rules serialize");
    text.push('\n');
    text
}

#[test]
fn committed_rules_match() {
    let committed = std::fs::read_to_string(PATH).unwrap_or_default();
    assert!(
        committed == rendered(),
        "web/src/lib/rules.json is out of date: run `cargo test -p pinchora-web rules -- --ignored write_rules`"
    );
}

#[test]
#[ignore = "writes web/src/lib/rules.json"]
fn write_rules() {
    std::fs::write(PATH, rendered()).expect("write rules.json");
}
