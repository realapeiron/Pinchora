use super::*;

/// Discord references, including nested action and command policies, for editor-side existence checks.
pub fn references(backup: &Backup) -> Vec<(bool, String)> {
    fn ids(v: &Value, role: bool, out: &mut Vec<(bool, String)>) {
        match v {
            Value::Array(a) => {
                for v in a {
                    ids(v, role, out)
                }
            }
            Value::String(s) => out.push((role, s.clone())),
            Value::Number(n) => out.push((role, n.to_string())),
            _ => {}
        }
    }
    fn walk(v: &Value, out: &mut Vec<(bool, String)>) {
        match v {
            Value::Object(map) => {
                for (key, v) in map {
                    if key == "role_weights" {
                        if let Some(map) = v.as_object() {
                            out.extend(map.keys().map(|id| (true, id.clone())));
                        }
                    } else if key == "target_id" {
                        ids(v, map.get("kind").and_then(Value::as_str) == Some("role"), out);
                    } else if key == "trigger_value"
                        && map
                            .get("trigger_kind")
                            .and_then(Value::as_str)
                            .is_some_and(|s| s.starts_with("role_"))
                    {
                        ids(v, true, out);
                    } else if let Some(role) = reference_kind(key) {
                        ids(v, role, out);
                    } else {
                        walk(v, out);
                    }
                }
            }
            Value::Array(a) => {
                for v in a {
                    walk(v, out)
                }
            }
            _ => {}
        }
    }
    let mut out = Vec::new();
    for rows in backup.sections.values() {
        for row in rows {
            walk(row, &mut out);
        }
    }
    out.sort();
    out.dedup();
    out
}
