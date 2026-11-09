use crate::{Result, source};
use std::{collections::BTreeMap, path::Path};
use syn::{UseTree, visit::Visit};

const PREFIX: &str = "crates/pinchora-services/src/";

pub fn run(root: &Path, metadata: &serde_json::Value) -> Result {
    let files = source::rust(root)?;
    let mut modules = BTreeMap::<String, Vec<String>>::new();
    for (path, file) in &files {
        if let Some(domain) = path.strip_prefix(PREFIX).and_then(|p| p.strip_suffix("/mod.rs"))
            && !domain.contains('/')
        {
            modules.insert(
                domain.into(),
                file.items
                    .iter()
                    .filter_map(|item| match item {
                        syn::Item::Mod(module) => Some(module.ident.to_string()),
                        _ => None,
                    })
                    .collect(),
            );
        }
    }
    let mut errors = Vec::new();
    for (path, file) in &files {
        let inside = path.starts_with(PREFIX);
        let domain = path.strip_prefix(PREFIX).and_then(|p| p.split_once('/')).map(|p| p.0);
        let mut checker = Boundary {
            path,
            inside,
            domain,
            modules: &modules,
            aliases: dependency_aliases(root, path, metadata),
            errors: &mut errors,
        };
        checker.visit_file(file);
        // Resolve aliases before visiting expressions and imports, including aliases declared later.
        checker.visit_file(file);
    }
    errors.sort();
    errors.dedup();
    if errors.is_empty() {
        println!("service boundaries: public domain surfaces only");
        Ok(())
    } else {
        Err(format!("service boundaries:\n{}", errors.join("\n")).into())
    }
}

fn dependency_aliases(root: &Path, path: &str, metadata: &serde_json::Value) -> BTreeMap<String, Vec<String>> {
    let mut aliases = BTreeMap::new();
    for package in metadata["packages"].as_array().into_iter().flatten() {
        let Some(manifest) = package["manifest_path"].as_str() else {
            continue;
        };
        let Some(directory) = Path::new(manifest).parent() else {
            continue;
        };
        if !root.join(path).starts_with(directory) {
            continue;
        }
        for dependency in package["dependencies"].as_array().into_iter().flatten() {
            if dependency["name"] == "pinchora-services"
                && let Some(alias) = dependency["rename"].as_str()
            {
                aliases.insert(alias.replace('-', "_"), vec!["pinchora_services".into()]);
            }
        }
    }
    aliases
}

struct Boundary<'a> {
    path: &'a str,
    inside: bool,
    domain: Option<&'a str>,
    modules: &'a BTreeMap<String, Vec<String>>,
    aliases: BTreeMap<String, Vec<String>>,
    errors: &'a mut Vec<String>,
}

impl Boundary<'_> {
    fn macro_items(&mut self, tokens: proc_macro2::TokenStream) {
        if let Ok(file) = syn::parse2::<syn::File>(tokens.clone()) {
            self.visit_file(&file);
        } else {
            for token in tokens {
                if let proc_macro2::TokenTree::Group(group) = token {
                    self.macro_items(group.stream());
                }
            }
        }
    }

    fn check(&mut self, parts: &[String]) {
        let mut parts = parts.to_vec();
        for _ in 0..self.aliases.len() {
            let Some(prefix) = parts.first().and_then(|p| self.aliases.get(p)) else {
                break;
            };
            let expanded: Vec<_> = prefix.iter().cloned().chain(parts.iter().skip(1).cloned()).collect();
            if expanded == parts {
                break;
            }
            parts = expanded;
        }
        if self.inside && (parts.first().is_some_and(|p| p == "super") || parts.iter().any(|p| p == "*")) {
            self.errors.push(format!(
                "{}: no upward or glob imports: {}",
                self.path,
                parts.join("::")
            ));
        }
        let service = parts
            .first()
            .is_some_and(|p| p == "pinchora_services" || (self.inside && p == "crate"));
        if !service {
            return;
        }
        if parts.iter().any(|p| p == "*") {
            self.errors
                .push(format!("{}: no service glob imports: {}", self.path, parts.join("::")));
        }
        if let (Some(domain), Some(child)) = (parts.get(1), parts.get(2))
            && self.domain != Some(domain.as_str())
            && self
                .modules
                .get(domain)
                .is_some_and(|children| children.contains(child))
        {
            self.errors.push(format!(
                "{}: use {domain}::mod.rs public surface: {}",
                self.path,
                parts.join("::")
            ));
        }
    }

    fn imports(&mut self, tree: &UseTree, prefix: Vec<String>) {
        match tree {
            UseTree::Path(path) => {
                let mut prefix = prefix;
                prefix.push(path.ident.to_string());
                self.imports(&path.tree, prefix);
            }
            UseTree::Group(group) => {
                for item in &group.items {
                    self.imports(item, prefix.clone());
                }
            }
            UseTree::Glob(_) => {
                let mut parts = prefix;
                parts.push("*".into());
                self.check(&parts);
            }
            UseTree::Name(name) => {
                let mut parts = prefix;
                let alias = if name.ident == "self" {
                    parts.last().cloned().unwrap_or_default()
                } else {
                    parts.push(name.ident.to_string());
                    name.ident.to_string()
                };
                self.check(&parts);
                self.aliases.insert(alias, parts);
            }
            UseTree::Rename(rename) => {
                let mut parts = prefix;
                if rename.ident != "self" {
                    parts.push(rename.ident.to_string());
                }
                self.check(&parts);
                self.aliases.insert(rename.rename.to_string(), parts);
            }
        }
    }
}

impl<'ast> Visit<'ast> for Boundary<'_> {
    fn visit_item_extern_crate(&mut self, item: &'ast syn::ItemExternCrate) {
        if let Some((_, alias)) = &item.rename {
            self.aliases.insert(alias.to_string(), vec![item.ident.to_string()]);
        }
    }

    fn visit_macro(&mut self, item: &'ast syn::Macro) {
        self.macro_items(item.tokens.clone());
    }

    fn visit_item_use(&mut self, item: &'ast syn::ItemUse) {
        self.imports(&item.tree, Vec::new());
    }
    fn visit_path(&mut self, path: &'ast syn::Path) {
        self.check(&path.segments.iter().map(|s| s.ident.to_string()).collect::<Vec<_>>());
        syn::visit::visit_path(self, path);
    }
    fn visit_item_mod(&mut self, module: &'ast syn::ItemMod) {
        if self.inside && self.domain.is_some() && matches!(module.vis, syn::Visibility::Public(_)) {
            self.errors.push(format!(
                "{}: re-export public items through mod.rs; child modules must stay private",
                self.path
            ));
        }
        syn::visit::visit_item_mod(self, module);
    }
}

#[cfg(test)]
mod tests {
    use crate::boundary::Boundary;
    use std::collections::BTreeMap;
    use syn::visit::Visit;

    fn rejected(source: &str, inside: bool) -> bool {
        let modules = BTreeMap::from([("economy".into(), vec!["ledger".into()])]);
        let mut errors = Vec::new();
        let mut checker = Boundary {
            path: "probe.rs",
            inside,
            domain: Some("infra"),
            modules: &modules,
            aliases: BTreeMap::new(),
            errors: &mut errors,
        };
        let file = syn::parse_file(source).unwrap();
        checker.visit_file(&file);
        checker.visit_file(&file);
        !errors.is_empty()
    }

    #[test]
    fn rejects_private_paths_aliases_globs_and_upward_imports() {
        assert!(rejected("use pinchora_services::economy::ledger::Entry;", false));
        assert!(rejected(
            "use pinchora_services::economy as eco; fn f() { eco::ledger::run(); }",
            false
        ));
        assert!(rejected("use pinchora_services::economy::*;", false));
        assert!(rejected("use super::Clock;", true));
        assert!(rejected(
            "extern crate pinchora_services as svc; use svc::economy::ledger::Entry;",
            false
        ));
        assert!(rejected(
            "macro_rules! access { () => { use pinchora_services::economy::*; }; }",
            false
        ));
        assert!(rejected("use crate::economy::ledger::Entry;", true));
        assert!(rejected("pub mod ledger;", true));
        assert!(!rejected(
            "use pinchora_services::economy::Entry; fn f() { Entry::new(); }",
            false
        ));
        assert!(!rejected("use crate::infra::Clock;", true));
    }
}
