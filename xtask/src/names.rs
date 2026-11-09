use crate::{Result, source};
use regex::Regex;
use std::{fs, path::Path};
use syn::visit::{self, Visit};

fn forbidden(name: &str) -> bool {
    Regex::new(r"(?i)(_v[0-9]+|_parity|_pack|wave|(?:^|[_-])(?:d[0-9]+|a[0-9]+|w[0-9]+)(?:[_-]|\.|$))")
        .unwrap()
        .is_match(name)
}
struct Modules(Vec<String>, bool);
impl<'a> Visit<'a> for Modules {
    fn visit_item_fn(&mut self, node: &'a syn::ItemFn) {
        if forbidden(&node.sig.ident.to_string()) && (self.1 || source::tokens(&node.sig.output).contains("Router")) {
            self.0.push(format!("route handler {}", node.sig.ident));
        }
        visit::visit_item_fn(self, node);
    }
    fn visit_macro(&mut self, node: &'a syn::Macro) {
        self.macro_modules(node.tokens.clone());
    }
    fn visit_item_mod(&mut self, node: &'a syn::ItemMod) {
        if forbidden(&node.ident.to_string()) {
            self.0.push(format!("module {}", node.ident));
        }
        visit::visit_item_mod(self, node);
    }
}

impl Modules {
    fn macro_modules(&mut self, tokens: proc_macro2::TokenStream) {
        if let Ok(file) = syn::parse2::<syn::File>(tokens.clone()) {
            self.visit_file(&file);
        } else {
            for token in tokens {
                if let proc_macro2::TokenTree::Group(group) = token {
                    self.macro_modules(group.stream());
                }
            }
        }
    }
}

pub fn run(root: &Path) -> Result {
    let mut errors = vec![];
    // Match comments and strings together, so comment markers inside wire strings are ignored.
    let raw = (1..=8)
        .rev()
        .map(|n| {
            let hashes = "#".repeat(n);
            format!("r{hashes}\".*?\"{hashes}")
        })
        .collect::<Vec<_>>()
        .join("|");
    let lexer = Regex::new(&format!(
        r##"(?s){raw}|"(?:\\.|[^"\\])*"|'(?:\\.|[^'\\\n])'|//[^\n]*|/\*.*?\*/"##
    ))?;
    let plans = Regex::new(r"\bD[0-9]+(?:[.-][0-9]+)?\b|(?i:\b(?:wave|faz|phase|plan)\s*[-:]?\s*[A-Z]?[0-9]+\b)")?;
    for path in source::files(root)? {
        let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("");
        if !matches!(ext, "rs" | "ts" | "tsx" | "js" | "css") {
            continue;
        }
        if path.components().any(|p| forbidden(&p.as_os_str().to_string_lossy())) {
            errors.push(format!("{}: forbidden file/module name", path.display()));
        }
        let text = fs::read_to_string(root.join(&path))?;
        if ext == "rs" {
            let route_source =
                path.starts_with("crates/pinchora-web/src/routes") || path.starts_with("crates/pinchora-web/src/reef");
            let mut modules = Modules(vec![], route_source);
            modules.visit_file(&syn::parse_file(&text)?);
            for name in modules.0 {
                errors.push(format!("{}: {name}", path.display()));
            }
        }
        for m in lexer
            .find_iter(&text)
            .filter(|m| m.as_str().starts_with("//") || m.as_str().starts_with("/*"))
        {
            if let Some(code) = plans.find(m.as_str()) {
                errors.push(format!(
                    "{}:{}: plan code in comment: {}",
                    path.display(),
                    text[..m.start()].lines().count() + 1,
                    code.as_str()
                ));
            }
        }
    }
    if !errors.is_empty() {
        return Err(format!("names:\n{}", errors.join("\n")).into());
    }
    println!("names: clean (wire strings excluded)");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn temporary_suffixes_and_plan_file_names_are_rejected() {
        for name in [
            "handler_v2.rs",
            "thing_parity.rs",
            "extra_pack",
            "wave7_tools",
            "d12_routes.rs",
        ] {
            assert!(forbidden(name), "{name}");
        }
        for name in ["routes.rs", "privacy.rs", "workflows.rs"] {
            assert!(!forbidden(name));
        }
    }
}
