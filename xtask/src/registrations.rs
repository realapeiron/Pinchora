//! Deterministic compatibility output for the fixed domain registration list.
use crate::{Result, source};
use proc_macro2::TokenStream;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
};
use syn::{
    Ident, braced,
    parse::{Parse, ParseStream},
};

mod ordering;
mod output;
mod overlap;
mod parsing;
#[cfg(test)]
mod tests;
mod validation;

#[derive(Default, Clone)]
struct Registration {
    sections: BTreeMap<String, BTreeMap<usize, TokenStream>>,
    sources: BTreeMap<String, BTreeMap<usize, String>>,
    fallbacks: BTreeMap<String, BTreeSet<usize>>,
}

impl Registration {
    fn merge(&mut self, other: Self) -> Result {
        for (section, entries) in other.sections {
            let offset = self.sections.get(&section).map_or(0, BTreeMap::len);
            if let Some(sources) = other.sources.get(&section) {
                self.sources
                    .entry(section.clone())
                    .or_default()
                    .extend(sources.iter().map(|(index, value)| (offset + index, value.clone())));
            }
            if let Some(fallbacks) = other.fallbacks.get(&section) {
                self.fallbacks
                    .entry(section.clone())
                    .or_default()
                    .extend(fallbacks.iter().map(|index| offset + index));
            }
            self.sections
                .entry(section)
                .or_default()
                .extend(entries.into_iter().map(|(index, value)| (offset + index, value)));
        }
        Ok(())
    }

    fn raw(&self, section: &str) -> String {
        self.sections
            .get(section)
            .into_iter()
            .flat_map(|entries| entries.iter())
            .map(|(order, tokens)| {
                self.sources
                    .get(section)
                    .and_then(|entries| entries.get(order))
                    .cloned()
                    .unwrap_or_else(|| tokens.to_string())
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn tokens(&self, section: &str) -> TokenStream {
        self.sections
            .get(section)
            .into_iter()
            .flat_map(|entries| entries.values().cloned())
            .collect()
    }
}

fn domain_names(root: &Path) -> Result<Vec<String>> {
    let file = syn::parse_file(&fs::read_to_string(
        root.join("crates/pinchora-commands/src/commands/mod.rs"),
    )?)?;
    let registration = file
        .items
        .iter()
        .find_map(|item| match item {
            syn::Item::Macro(m) if m.mac.path.segments.last().is_some_and(|p| p.ident == "domains") => Some(&m.mac),
            _ => None,
        })
        .ok_or("missing domain composition root")?;
    let names = registration.parse_body_with(syn::punctuated::Punctuated::<Ident, syn::Token![,]>::parse_terminated)?;
    Ok(names.iter().map(ToString::to_string).collect())
}

fn read_registration(path: &Path) -> Result<Registration> {
    let file = syn::parse_file(&fs::read_to_string(path)?)?;
    let mut result = Registration::default();
    for item in &file.items {
        if let syn::Item::Macro(item) = item {
            if item.mac.path.is_ident("compatibility") {
                result.merge(item.mac.parse_body()?)?;
            } else if item.mac.path.is_ident("commands") {
                parsing::commands(&mut result, &item.mac)?;
            }
        }
    }
    parsing::data(&mut result, &path.with_extension("json"))?;
    Ok(result)
}

fn collect(root: &Path) -> Result<Registration> {
    let names = domain_names(root)?;
    let mut found = BTreeSet::new();
    let folder = root.join("crates/pinchora-commands/src/commands");
    for entry in fs::read_dir(&folder)? {
        let path = entry?.path();
        if path.join("registration.rs").is_file() {
            found.insert(path.file_name().unwrap().to_string_lossy().into_owned());
        }
    }
    if names.len() != found.len() || names.iter().cloned().collect::<BTreeSet<_>>() != found {
        return Err("domain list differs from discovered registration files".into());
    }
    let mut result = Registration::default();
    for name in names {
        if !folder.join(&name).join("registration.json").is_file() {
            return Err(format!("{name}: missing registration data").into());
        }
        let registration = read_registration(&folder.join(&name).join("registration.rs"))?;
        validation::components(root, &registration).map_err(|error| format!("{name}: {error}"))?;
        result.merge(registration)?;
    }
    validation::check(&result)?;
    validation::coverage(root, &result)?;
    ordering::apply(root, &mut result)?;
    Ok(result)
}

pub fn run(root: &Path, check: bool) -> Result {
    let registration = collect(root)?;
    let mut consumed = BTreeSet::new();
    let mut outputs = Vec::new();
    for path in source::files(root)?
        .into_iter()
        .filter(|p| p.extension().is_some_and(|e| e == "rs"))
    {
        let original = fs::read_to_string(root.join(&path))?;
        if !original.contains("// registrations:") {
            continue;
        }
        let generated = output::replace(&original, &registration, &mut consumed)?;
        outputs.push((path, output::format(root, &generated)?));
    }
    outputs.extend(output::data(&registration)?);
    outputs.extend(ordering::outputs(root, &registration)?);
    consumed.insert("commands".to_owned());
    outputs.push(output::fixture(root)?);
    outputs.push(output::documentation(root, &registration)?);
    consumed.extend(["routes".to_owned(), "legacy_leaves".to_owned()]);
    if let Some(unused) = registration.sections.keys().find(|key| !consumed.contains(*key)) {
        return Err(format!("registration section {unused} has no consumer").into());
    }
    let mut stale = Vec::new();
    for (path, generated) in outputs {
        if fs::read_to_string(root.join(&path)).ok().as_deref() != Some(&generated) {
            if check {
                stale.push(path.display().to_string());
            } else {
                fs::write(root.join(&path), generated)?;
            }
        }
    }
    if !stale.is_empty() {
        return Err(format!(
            "stale registration output: {}; run cargo xtask registrations",
            stale.join(", ")
        )
        .into());
    }
    println!("domain registrations: {} sections checked", registration.sections.len());
    Ok(())
}
