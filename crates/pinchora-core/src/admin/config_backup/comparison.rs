use super::*;

pub fn validate_shape(backup: &Backup, lang: Lang) -> Result<(), String> {
    let invalid = || t!(lang, "config_backup.invalid");
    if backup.version != VERSION
        || backup.source_guild.parse::<u64>().ok().filter(|id| *id > 0).is_none()
        || serde_json::to_vec(backup).map_err(|_| invalid())?.len() > MAX_BYTES
    {
        return Err(invalid());
    }
    if backup.sections.len() != SECTIONS.len() {
        return Err(t!(lang, "config_backup.incomplete"));
    }
    for section in SECTIONS {
        let rows = backup.sections.get(section.table).ok_or_else(invalid)?;
        if rows.len() > 1000
            || (section.key.is_empty() && rows.len() > 1)
            || (section.table == "guilds" && rows.len() != 1)
        {
            return Err(invalid());
        }
        let mut keys = std::collections::BTreeSet::new();
        let mut serial_ids = std::collections::BTreeSet::new();
        let mut colours = std::collections::BTreeSet::new();
        for row in rows {
            let obj = row.as_object().ok_or_else(invalid)?;
            if obj
                .keys()
                .any(|key| !(section.serial && key == "id") && !section.columns.split(',').any(|c| c == key))
                || section.columns.split(',').any(|c| !obj.contains_key(c))
            {
                return Err(invalid());
            }
            if section.serial
                && row
                    .get("id")
                    .and_then(Value::as_str)
                    .and_then(|id| id.parse::<i64>().ok())
                    .is_none_or(|id| id <= 0)
            {
                return Err(invalid());
            }
            if !keys.insert(row_key(section, row)) {
                return Err(invalid());
            }
            if section.table == "color_roles" && !colours.insert(row["hex"].to_string()) {
                return Err(invalid());
            }
            if section.serial && !serial_ids.insert(row["id"].to_string()) {
                return Err(invalid());
            }
        }
    }
    Ok(())
}
pub fn diff(before: &Backup, after: &Backup, policy: Policy) -> Vec<Difference> {
    SECTIONS
        .iter()
        .map(|section| {
            let old: BTreeMap<_, _> = before
                .sections
                .get(section.table)
                .into_iter()
                .flatten()
                .map(|r| (row_key(section, r), r))
                .collect();
            let new: BTreeMap<_, _> = after
                .sections
                .get(section.table)
                .into_iter()
                .flatten()
                .map(|r| (row_key(section, r), r))
                .collect();
            let normalized = |value: &Value| {
                let mut v = value.clone();
                if section.serial {
                    v.as_object_mut().unwrap().remove("id");
                }
                v
            };
            Difference {
                section: section.table.into(),
                added: new.keys().filter(|k| !old.contains_key(*k)).count(),
                changed: new
                    .iter()
                    .filter(|(k, v)| old.get(*k).is_some_and(|o| normalized(o) != normalized(v)))
                    .count(),
                removed: if policy == Policy::Replace {
                    old.keys().filter(|k| !new.contains_key(*k)).count()
                } else {
                    0
                },
            }
        })
        .collect()
}
