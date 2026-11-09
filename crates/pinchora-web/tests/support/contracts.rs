//! Value-free observations, one atomic file per process; nextest processes never share a writer.
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
    sync::{LazyLock, Mutex},
};
type Shapes = BTreeMap<String, BTreeSet<String>>;
static OBSERVED: LazyLock<Mutex<Shapes>> = LazyLock::new(|| Mutex::new(BTreeMap::new()));
static ROUTES: LazyLock<Vec<String>> = LazyLock::new(|| {
    let mut routes: Vec<String> = serde_json::from_str(include_str!("../../../../contracts/routes.json")).unwrap();
    // Literal paths win over parameters; catch-alls are last.
    routes.sort_by_key(|r| (r.contains('*'), r.matches('{').count(), std::cmp::Reverse(r.len())));
    routes
});

pub fn template(method: &str, path: &str) -> Option<String> {
    ROUTES.iter().find_map(|route| {
        let (verb, pattern) = route.split_once(' ')?;
        if verb != method && verb != "ANY" {
            return None;
        }
        let actual: Vec<_> = path.trim_end_matches('/').split('/').collect();
        let expected: Vec<_> = pattern.trim_end_matches('/').split('/').collect();
        let matches = expected
            .iter()
            .enumerate()
            .all(|(i, e)| e.starts_with("{*") || actual.get(i).is_some_and(|a| *e == *a || e.starts_with('{')))
            && (actual.len() == expected.len() || pattern.contains("{*"));
        matches.then(|| pattern.to_owned())
    })
}

pub fn record(method: &str, path: &str, status: u16, body: &[u8]) {
    if !path.starts_with("/api/") {
        return;
    }
    let path =
        template(method, path).unwrap_or_else(|| panic!("unregistered API route {method} {path}; update routes"));
    let mut shape = BTreeSet::new();
    match serde_json::from_slice::<Value>(body) {
        Ok(value) => walk("$", &value, &mut shape),
        Err(_) => {
            shape.insert(if body.is_empty() { "$:empty" } else { "$:binary" }.into());
        }
    }
    let mut observations = OBSERVED.lock().unwrap();
    observations
        .entry(format!("{method} {path} {status}"))
        .or_default()
        .extend(shape);
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/contracts/observations");
    std::fs::create_dir_all(&dir).unwrap();
    let pid = std::process::id();
    let temp = dir.join(format!("{pid}.tmp"));
    std::fs::write(&temp, serde_json::to_vec(&*observations).unwrap()).unwrap();
    std::fs::rename(temp, dir.join(format!("{pid}.json"))).unwrap();
}
fn walk(path: &str, value: &Value, out: &mut BTreeSet<String>) {
    let kind = match value {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    };
    out.insert(format!("{path}:{kind}"));
    match value {
        Value::Array(values) => {
            for value in values {
                walk(&format!("{path}[]"), value, out);
            }
        }
        Value::Object(values) => {
            for (key, value) in values {
                // JSON maps keyed by snowflakes contain fixture identities, not schema fields.
                let key = if key.parse::<u64>().is_ok() { "*" } else { key };
                walk(
                    &format!("{path}.{}", key.replace('~', "~0").replace('.', "~1")),
                    value,
                    out,
                );
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn shapes_drop_values_and_array_indices_but_retain_missing_fields() {
        let first = serde_json::json!({"items": [{"id": "fixture", "count": 1}], "ready": true});
        let second =
            serde_json::json!({"items": [{"id": "different", "count": 4}, {"id": "last", "count": 8}], "ready": false});
        let mut a = BTreeSet::new();
        let mut b = BTreeSet::new();
        walk("$", &first, &mut a);
        walk("$", &second, &mut b);
        assert_eq!(a, b);
        let mut removed = first;
        removed["items"][0].as_object_mut().unwrap().remove("count");
        let mut c = BTreeSet::new();
        walk("$", &removed, &mut c);
        assert_ne!(a, c);
        assert!(a.difference(&c).any(|s| s == "$.items[].count:number"));
    }
    #[test]
    fn literal_routes_precede_parameters_and_unknown_paths_use_catchall() {
        assert_eq!(template("GET", "/api/me/guilds"), Some("/api/me/guilds".into()));
        assert_eq!(
            template("GET", "/api/house/rooms/2"),
            Some("/api/house/rooms/{idx}".into())
        );
        assert_eq!(template("GET", "/api/not-a-route"), Some("/api/{*rest}".into()));
    }
}
