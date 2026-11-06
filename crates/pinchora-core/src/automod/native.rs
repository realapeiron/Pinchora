//! Discord mirrors only detect and alert. Pinchora executes the ordered chain after safety checks.
use super::Rule;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NativeRule {
    pub name: String,
    pub event_type: u8,
    pub trigger_type: u8,
    pub trigger_metadata: Value,
    pub actions: Vec<Value>,
    pub enabled: bool,
    pub exempt_roles: Vec<String>,
    pub exempt_channels: Vec<String>,
}
/// Discord cannot express only-channels. Those rules stay in the Pinchora engine.
pub fn native_rule(rule: &Rule, alert: &str, enabled: bool) -> Option<NativeRule> {
    if !rule.native || !rule.only_channels.is_empty() || !super::valid_id(alert) {
        return None;
    }
    let p = &rule.params;
    let (kind, metadata) = match rule.kind.as_str() {
        "spam" => (3, json!({})),
        "mentions" => (5, json!({"mention_total_limit": p.threshold.saturating_sub(1).min(50)})),
        "presets" if !p.presets.is_empty() => (4, json!({"presets": p.presets})),
        // Discord's keyword and regex engine cannot reproduce de-disguise, URL host parsing,
        // or Markdown code exclusion. Keep these kinds local to avoid false punishments.
        "bad_words" | "invites" | "links" => return None,
        _ => return None,
    };
    Some(NativeRule {
        name: format!("Pinchora · {}", rule.name),
        event_type: 1,
        trigger_type: kind,
        trigger_metadata: metadata,
        actions: vec![json!({"type": 2, "metadata": {"channel_id": alert}})],
        enabled: enabled && rule.enabled,
        exempt_roles: rule.exempt_roles.clone(),
        exempt_channels: rule.exempt_channels.clone(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn payloads_never_bypass_ordered_actions_or_dry_run() {
        let mut c = super::super::profile("standard", chrono::Utc::now()).unwrap();
        for r in &c.rules {
            if let Some(p) = native_rule(r, "1", true) {
                assert_eq!(p.actions[0]["type"], 2);
                assert!(p.exempt_roles.len() <= 20);
                for re in p.trigger_metadata["regex_patterns"].as_array().into_iter().flatten() {
                    let s = re.as_str().unwrap();
                    assert!(s.len() <= 260);
                    super::super::compile(s).unwrap();
                }
            }
        }
        c.rules[0].only_channels.push("2".into());
        assert!(native_rule(&c.rules[0], "1", true).is_none());
    }
    #[test]
    fn keyword_mirrors_never_disagree_with_local_matcher() {
        let mut c = super::super::profile("strict", chrono::Utc::now()).unwrap();
        for kind in ["links", "invites", "bad_words"] {
            let rule = c.rules.iter_mut().find(|r| r.kind == kind).unwrap();
            rule.native = true;
            assert!(native_rule(rule, "1", true).is_none(), "{kind}");
        }
    }
}
