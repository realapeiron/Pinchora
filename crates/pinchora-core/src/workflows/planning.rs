use super::*;
use chrono::Timelike;
pub fn condition_passes(c: &Condition, index: usize, event: &Event, now: DateTime<Utc>) -> bool {
    match c {
        Condition::HasRole { role_id } => event.roles.contains(role_id),
        Condition::LacksRole { role_id } => !event.roles.contains(role_id),
        Condition::InChannel { channel_id } => *channel_id == event.channel_id,
        Condition::MinLevel { level } => event.level >= *level,
        Condition::AccountAge { days } => event.account_age_days >= *days,
        Condition::Chance { percent } => event.roll % 100 < *percent,
        Condition::Cooldown { .. } => !event.cooldowns.contains(&index),
        Condition::VarEquals { scope, key, value } => event.variable(*scope, key) == Some(value),
        Condition::VarGreater { scope, key, value } => event
            .variable(*scope, key)
            .and_then(|v| v.parse::<i64>().ok())
            .is_some_and(|v| v > *value),
        Condition::TimeWindow { start, end, tz } => tz.parse::<chrono_tz::Tz>().is_ok_and(|tz| {
            let n = now.with_timezone(&tz);
            let m = n.hour() * 60 + n.minute();
            if start <= end {
                m >= *start && m < *end
            } else {
                m >= *start || m < *end
            }
        }),
    }
}
/// The same deterministic placeholder values for tests and live messages.
pub fn variables(event: &Event) -> tags::Vars {
    let mut vars = tags::Vars::new();
    vars.set("user.id", &event.user_id)
        .set("user", format!("<@{}>", event.user_id))
        .set("channel", format!("<#{}>", event.channel_id))
        .set("level", event.level.to_string())
        .set("event", &event.value);
    for (key, value) in &event.variables {
        vars.set(&format!("var.{}", key.replace(':', ".")), value);
    }
    vars
}
pub fn render(text: &str, event: &Event) -> String {
    tags::truncate(
        &tags::render(text, &variables(event), &mut |n| event.roll as usize % n.max(1)),
        2000,
    )
}
fn preview_value(value: &mut serde_json::Value, event: &Event) {
    match value {
        serde_json::Value::String(s) => *s = render(s, event),
        serde_json::Value::Object(o) => {
            for v in o.values_mut() {
                preview_value(v, event)
            }
        }
        serde_json::Value::Array(a) => {
            for v in a {
                preview_value(v, event)
            }
        }
        _ => {}
    }
}
pub fn plan(def: &Definition, event: &Event, now: DateTime<Utc>) -> Plan {
    plan_with_preview(def, event, now, preview_action)
}

pub fn preview_action(action: &StepAction, event: &Event) -> serde_json::Value {
    let mut detail = serde_json::to_value(action).unwrap_or_default();
    preview_value(&mut detail, event);
    detail
}

pub fn plan_with_preview(
    def: &Definition,
    event: &Event,
    now: DateTime<Utc>,
    preview: impl Fn(&StepAction, &Event) -> serde_json::Value,
) -> Plan {
    let mut result = Plan {
        outcome: "ready".into(),
        steps: Vec::new(),
        actions: Vec::new(),
    };
    if def.validate().is_err() || event.depth != 0 {
        result.outcome = "refused".into();
        return result;
    }
    let matched = def.trigger.matches(event);
    result.steps.push(Playback {
        phase: "trigger".into(),
        index: 0,
        status: if matched { "passed" } else { "failed" }.into(),
        detail: serde_json::json!({"kind":def.trigger.key()}),
    });
    let mut passed = matched;
    for (index, c) in def.conditions.iter().enumerate() {
        let ok = condition_passes(c, index, event, now);
        passed &= ok;
        result.steps.push(Playback {
            phase: "condition".into(),
            index,
            status: if ok { "passed" } else { "failed" }.into(),
            detail: serde_json::to_value(c).unwrap_or_default(),
        });
    }
    for a in &def.actions {
        if let StepAction::RandomBranch { branches } = a {
            result
                .actions
                .extend(branches[event.roll as usize % branches.len()].clone());
        } else {
            result.actions.push(a.clone());
        }
    }
    let mut simulated = event.clone();
    for (index, a) in result.actions.iter().enumerate() {
        let detail = preview(a, &simulated);
        let status = if !passed || result.outcome == "failed" {
            "skipped"
        } else if simulate_variable(a, &mut simulated).is_err() {
            result.outcome = "failed".into();
            "failed"
        } else {
            "would_run"
        };

        result.steps.push(Playback {
            phase: "action".into(),
            index,
            status: status.into(),
            detail,
        });
    }
    if !passed {
        result.outcome = "filtered".into();
        result.actions.clear();
    }
    if result.outcome == "failed" {
        result.actions.clear();
        for step in &mut result.steps {
            if step.status == "would_run" {
                step.status = "skipped".into();
            }
        }
    }
    result
}

fn simulate_variable(action: &StepAction, event: &mut Event) -> Result<(), &'static str> {
    let (scope, key, value, increment) = match action {
        StepAction::SetVar { scope, key, value } => (*scope, key, render(value, event), None),
        StepAction::IncrVar { scope, key, amount } => (*scope, key, String::new(), Some(*amount)),
        _ => return Ok(()),
    };
    if scope == Scope::Member && !id(&event.user_id) {
        return Err("member");
    }
    let keys: std::collections::BTreeSet<_> = event
        .variables
        .keys()
        .filter_map(|k| k.split_once(':').map(|(_, k)| k))
        .collect();
    if !keys.contains(key.as_str()) && keys.len() >= MAX_VARIABLES {
        return Err("variable_limit");
    }
    let value = variable_value(event.variable(scope, key), &value, increment)?;
    event.variables.insert(format!("{}:{key}", scope.key()), value);
    Ok(())
}

/// Shared by the planner and the serialized database update.
pub fn variable_value(old: Option<&str>, value: &str, increment: Option<i64>) -> Result<String, &'static str> {
    if value.chars().count() > 200 {
        return Err("variable");
    }
    match increment {
        Some(by) if by.unsigned_abs() <= 1_000_000_000 => old
            .unwrap_or("0")
            .parse::<i64>()
            .ok()
            .and_then(|v| v.checked_add(by))
            .map(|v| v.to_string())
            .ok_or("variable_overflow"),
        Some(_) => Err("variable"),
        None => Ok(value.to_owned()),
    }
}
