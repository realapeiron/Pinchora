use super::*;

fn patterns(registration: &Registration, section: &str) -> Result<Vec<String>> {
    let mut result = vec![];
    for tokens in registration
        .sections
        .get(section)
        .into_iter()
        .flat_map(|entries| entries.values())
    {
        let arm: syn::Arm = syn::parse2(tokens.clone())?;
        result.extend(source::strings(&arm.pat));
    }
    Ok(result)
}

fn unique(values: &[String], label: &str) -> Result {
    let mut seen = BTreeSet::new();
    for value in values {
        if !seen.insert(value) {
            return Err(format!("duplicate {label}: {value}").into());
        }
    }
    Ok(())
}

pub(super) fn check(registration: &Registration) -> Result {
    super::overlap::check(registration)?;
    for section in ["components", "modals", "owners", "command_gates", "component_gates"] {
        unique(&patterns(registration, section)?, section)?;
    }
    for section in ["private", "opens_form", "opens_modal", "slash_only"] {
        let entries = source::strings(&registration.tokens(section));
        unique(&entries, section)?;
        for (i, entry) in entries.iter().enumerate() {
            let separator = if section == "opens_modal" { ':' } else { ' ' };
            for other in &entries[i + 1..] {
                if covers(entry, other, separator) || covers(other, entry, separator) {
                    return Err(format!("ambiguous {section}: {entry} / {other}").into());
                }
            }
        }
    }
    let private = source::strings(&registration.tokens("private"));
    for form in source::strings(&registration.tokens("opens_form")) {
        if private.iter().any(|p| covers(p, &form, ' ') || covers(&form, p, ' ')) {
            return Err(format!("form {form} also defers privately").into());
        }
    }
    let routes = output::routes(registration)?;
    unique(&routes.iter().map(|r| r.0.clone()).collect::<Vec<_>>(), "legacy route")?;
    unique(
        &routes.iter().map(|r| r.1.clone()).collect::<Vec<_>>(),
        "canonical route",
    )?;
    let aliases = registration
        .sections
        .get("aliases")
        .ok_or("aliases are required")?
        .values()
        .map(|entry| source::strings(entry)[0].to_lowercase())
        .collect::<Vec<_>>();
    unique(&aliases, "text alias")?;
    Ok(())
}

fn covers(prefix: &str, path: &str, separator: char) -> bool {
    path == prefix
        || path
            .strip_prefix(prefix)
            .is_some_and(|suffix| suffix.starts_with(separator))
}

pub(super) fn components(root: &Path, registration: &Registration) -> Result {
    let snapshot: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(root.join("contracts/components.json"))?)?;
    let owners = patterns(registration, "owners")?;
    let gates = patterns(registration, "component_gates")?;
    let never = source::strings(&registration.tokens("never_gated_prefixes"));
    for (section, kind) in [("components", "dispatch"), ("modals", "dispatch_modal")] {
        for prefix in patterns(registration, section)? {
            let legacy = &snapshot["prefixes"][format!("{kind}:{prefix}")];
            if !owners.contains(&prefix) && (legacy.is_null() || !legacy["owner"].is_null()) {
                return Err(format!("component {prefix} requires an owner").into());
            }
            if !gates.contains(&prefix) && !never.contains(&prefix) {
                return Err(format!("component {prefix} requires a module or explicit never-gated rule").into());
            }
        }
    }
    Ok(())
}

pub(super) fn coverage(root: &Path, registration: &Registration) -> Result {
    components(root, registration)?;
    let areas = source::strings(&registration.tokens("help_areas"));
    let mut covers = vec![];
    for area in &areas {
        for path in source::strings(&registration.tokens(&format!("help_{area}"))) {
            covers.push((path, area));
        }
    }
    unique(
        &covers.iter().map(|(path, _)| path.clone()).collect::<Vec<_>>(),
        "help prefix",
    )?;
    for (legacy, _, _) in output::routes(registration)? {
        if !covers.iter().any(|(prefix, _)| self::covers(prefix, &legacy, ' ')) {
            return Err(format!("command {legacy} requires a help area").into());
        }
    }
    Ok(())
}
