//! Deterministic compatibility output for the fixed domain registration list.
use crate::{Result, source};
use proc_macro2::TokenStream;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
};
use syn::{
    Ident, LitInt, braced,
    parse::{Parse, ParseStream},
};

mod output;
#[cfg(test)]
mod tests;
mod validation;

#[derive(Default, Clone)]
struct Registration {
    sections: BTreeMap<String, BTreeMap<usize, TokenStream>>,
    sources: BTreeMap<String, BTreeMap<usize, String>>,
}

impl Parse for Registration {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let mut registration = Self::default();
        while !input.is_empty() {
            let section = input.parse::<Ident>()?.to_string();
            let body;
            braced!(body in input);
            let mut entries = BTreeMap::new();
            let mut sources = BTreeMap::new();
            while !body.is_empty() {
                let order = body.parse::<LitInt>()?.base10_parse()?;
                let entry;
                let brace = braced!(entry in body);
                if let Some(source) = brace.span.join().source_text() {
                    sources.insert(order, source[1..source.len() - 1].trim().to_owned());
                }
                if entries.insert(order, entry.parse()?).is_some() {
                    return Err(body.error("duplicate registration order"));
                }
            }
            registration.sources.insert(section.clone(), sources);
            if registration.sections.insert(section, entries).is_some() {
                return Err(input.error("duplicate registration section"));
            }
        }
        Ok(registration)
    }
}

impl Registration {
    fn merge(&mut self, other: Self) -> Result {
        for (section, sources) in other.sources {
            self.sources.entry(section).or_default().extend(sources);
        }
        for (section, entries) in other.sections {
            let target = self.sections.entry(section.clone()).or_default();
            for (order, tokens) in entries {
                if target.insert(order, tokens).is_some() {
                    return Err(format!("duplicate {section} order {order}").into());
                }
            }
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
        if let syn::Item::Macro(item) = item
            && item.mac.path.is_ident("compatibility")
        {
            result.merge(item.mac.parse_body()?)?;
        }
    }
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
        let registration = read_registration(&folder.join(&name).join("registration.rs"))?;
        validation::components(root, &registration).map_err(|error| format!("{name}: {error}"))?;
        result.merge(registration)?;
    }
    validation::check(&result)?;
    validation::coverage(root, &result)?;
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
    outputs.push(output::fixture(root)?);
    outputs.push(output::documentation(root, &registration)?);
    consumed.extend(["routes".to_owned(), "legacy_leaves".to_owned()]);
    if let Some(unused) = registration.sections.keys().find(|key| !consumed.contains(*key)) {
        return Err(format!("registration section {unused} has no consumer").into());
    }
    let mut stale = Vec::new();
    for (path, generated) in outputs {
        if fs::read_to_string(root.join(&path))? != generated {
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
