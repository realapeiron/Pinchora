use super::*;

/// Feed sources may carry private RSS credentials even though no token column is exported.
/// Keep the subscription recoverable, but require fresh credentials before it can run again.
pub fn redact_feed_credentials(backup: &mut Backup) {
    for row in backup.sections.get_mut("feeds").into_iter().flatten() {
        let Some(source) = row["source"].as_str().and_then(redact_url_credentials) else {
            continue;
        };
        row["source"] = Value::String(source);
        row["enabled"] = Value::Bool(false);
    }
}

/// A sanitized URL when it contains credentials; shared by backup export and audit redaction.
pub fn redact_url_credentials(source: &str) -> Option<String> {
    let Ok(mut url) = url::Url::parse(source) else {
        return None;
    };
    let mut sensitive = !url.username().is_empty() || url.password().is_some();
    let query: Vec<(String, String)> = url
        .query_pairs()
        .map(|(key, value)| {
            let name = key.to_ascii_lowercase().replace('-', "_");
            let secret = name.contains("token")
                || name.contains("secret")
                || name.contains("password")
                || matches!(
                    name.as_str(),
                    "key" | "api_key" | "apikey" | "auth" | "authorization" | "signature" | "sig"
                );
            sensitive |= secret;
            (
                key.into_owned(),
                if secret { "REDACTED".into() } else { value.into_owned() },
            )
        })
        .collect();
    if sensitive {
        let _ = url.set_username("");
        let _ = url.set_password(None);
        if url.query().is_some() {
            url.query_pairs_mut().clear().extend_pairs(query);
        }
        Some(url.into())
    } else {
        None
    }
}

pub fn row_key(section: &Section, row: &Value) -> String {
    section
        .key
        .split(',')
        .filter(|k| !k.is_empty())
        .map(|k| {
            let value = &row[k];
            if section.table == "role_panels" {
                value.as_str().unwrap_or_default().to_lowercase()
            } else {
                value.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\u{0}")
}
/// Align document-local ids with matching destination rows before merging or comparing children.
/// Records keyed only by id match within the same guild; another guild gets new records.
/// SQL still allocates actual ids for new rows; unused document ids cannot alias another parent.
pub fn align_serial_ids(before: &Backup, incoming: &mut Backup, same_guild: bool) {
    let mut mapped = BTreeMap::new();
    for section in SECTIONS.iter().filter(|s| s.serial) {
        let existing = &before.sections[section.table];
        let rows = incoming.sections.get_mut(section.table).unwrap();
        let mut used: std::collections::BTreeSet<String> = existing
            .iter()
            .chain(rows.iter())
            .map(|r| r["id"].to_string())
            .collect();
        let mut next = i64::MAX;
        for row in rows {
            let key = row_key(section, row);
            let id = if let Some(old) = existing
                .iter()
                .find(|r| (section.key != "id" || same_guild) && row_key(section, r) == key)
            {
                old["id"].clone()
            } else {
                while used.contains(&Value::String(next.to_string()).to_string()) {
                    next -= 1;
                }
                let id = Value::String(next.to_string());
                used.insert(id.to_string());
                next -= 1;
                id
            };
            mapped.insert((section.table, row["id"].to_string()), id.clone());
            row["id"] = id;
        }
    }
    for rows in incoming.sections.values_mut() {
        for row in rows {
            for (field, parent) in [("panel_id", "role_panels"), ("form_id", "ticket_forms")] {
                if let Some(old) = row.get(field)
                    && let Some(id) = mapped.get(&(parent, old.to_string()))
                {
                    row[field] = id.clone();
                }
            }
        }
    }
}
