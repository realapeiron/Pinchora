use crate::{Result, source};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
    process::Command,
};
use syn::{
    spanned::Spanned,
    visit::{self, Visit},
};

#[derive(Default)]
struct Functions {
    scope: Vec<String>,
    lengths: BTreeMap<String, usize>,
}
impl Functions {
    fn function(&mut self, name: String, block: &syn::Block) {
        self.scope.push(name);
        let length = block.span().end().line - block.span().start().line + 1;
        if length > 100 {
            self.lengths.insert(self.scope.join("::"), length);
        }
        visit::visit_block(self, block);
        self.scope.pop();
    }
}
impl<'a> Visit<'a> for Functions {
    fn visit_item_mod(&mut self, node: &'a syn::ItemMod) {
        self.scope.push(node.ident.to_string());
        visit::visit_item_mod(self, node);
        self.scope.pop();
    }
    fn visit_item_trait(&mut self, node: &'a syn::ItemTrait) {
        self.scope.push(node.ident.to_string());
        visit::visit_item_trait(self, node);
        self.scope.pop();
    }
    fn visit_item_impl(&mut self, node: &'a syn::ItemImpl) {
        let mut name = source::tokens(&node.self_ty);
        if let Some((_, tr, _)) = &node.trait_ {
            name = format!("{name} as {}", source::tokens(tr));
        }
        self.scope.push(name);
        visit::visit_item_impl(self, node);
        self.scope.pop();
    }
    fn visit_macro(&mut self, node: &'a syn::Macro) {
        // Macro bodies are opaque to syn's normal visitor. Inspect nested token groups too.
        self.macro_functions(node.tokens.clone());
    }
    fn visit_item_fn(&mut self, node: &'a syn::ItemFn) {
        self.function(node.sig.ident.to_string(), &node.block);
    }
    fn visit_impl_item_fn(&mut self, node: &'a syn::ImplItemFn) {
        self.function(node.sig.ident.to_string(), &node.block);
    }
    fn visit_trait_item_fn(&mut self, node: &'a syn::TraitItemFn) {
        if let Some(block) = &node.default {
            self.function(node.sig.ident.to_string(), block);
        }
    }
}

impl Functions {
    fn macro_functions(&mut self, tokens: proc_macro2::TokenStream) {
        if let Ok(file) = syn::parse2::<syn::File>(tokens.clone()) {
            self.visit_file(&file);
            return;
        }
        let tokens: Vec<_> = tokens.into_iter().collect();
        for (index, token) in tokens.iter().enumerate() {
            if matches!(token, proc_macro2::TokenTree::Ident(id) if id == "fn") {
                self.macro_signature(&tokens[index + 1..]);
            }
            if let proc_macro2::TokenTree::Group(group) = token {
                self.macro_functions(group.stream());
            }
        }
    }
    fn macro_signature(&mut self, tokens: &[proc_macro2::TokenTree]) {
        let mut name = String::new();
        for token in tokens {
            match token {
                proc_macro2::TokenTree::Group(group) if group.delimiter() == proc_macro2::Delimiter::Brace => {
                    let length = group.span().end().line - group.span().start().line + 1;
                    if length > 100 {
                        self.lengths
                            .insert(format!("{}::macro::{name}", self.scope.join("::")), length);
                    }
                    return;
                }
                proc_macro2::TokenTree::Group(_) => {}
                proc_macro2::TokenTree::Ident(id) if name.is_empty() => name = id.to_string(),
                _ => {}
            }
        }
    }
}

pub fn committed(root: &Path, path: &str) -> Result<Option<Value>> {
    let base = std::env::var("PINCHORA_RATCHET_BASE").unwrap_or_else(|_| "HEAD".into());
    let revision = Command::new("git")
        .args(["rev-parse", "--verify", &format!("{base}^{{commit}}")])
        .current_dir(root)
        .output()?;
    if !revision.status.success() {
        return Err(format!("ratchet base {base} is unavailable; fetch the comparison revision first").into());
    }
    let output = Command::new("git")
        .args(["show", &format!("{base}:{path}")])
        .current_dir(root)
        .output()?;
    Ok(if output.status.success() {
        Some(serde_json::from_slice(&output.stdout)?)
    } else {
        None
    })
}

pub fn ratchet_set(root: &Path, path: &str, now: &BTreeSet<String>) -> Result {
    if let Some(old) = committed(root, path)? {
        let old: BTreeSet<String> = serde_json::from_value(old)?;
        let added: Vec<_> = now.difference(&old).collect();
        if !added.is_empty() {
            return Err(format!("{path}: exceptions may only shrink: {added:?}").into());
        }
    }
    Ok(())
}

pub fn run(root: &Path, update: bool) -> Result {
    let exemptions: BTreeMap<String, String> =
        serde_json::from_slice(&fs::read(root.join("contracts/size-exemptions.json"))?)?;
    if let Some(old) = committed(root, "contracts/size-exemptions.json")? {
        for key in exemptions.keys() {
            if old.get(key).is_none() {
                return Err(format!("new size exemption forbidden: {key}").into());
            }
        }
    }
    let mut files = BTreeMap::new();
    let mut functions = BTreeMap::new();
    let executable = regex::Regex::new(r"\b(function|class)\b")?;
    for path in source::files(root)? {
        let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("");
        if !matches!(ext, "rs" | "ts" | "tsx" | "js" | "css") {
            continue;
        }
        let name = path.to_string_lossy().into_owned();
        if let Some(reason) = exemptions.get(&name) {
            if reason.trim().is_empty() {
                return Err(format!("{name}: missing exemption reason").into());
            }
            let text = fs::read_to_string(root.join(&path))?;
            if text.contains("=>") || executable.is_match(&text) {
                return Err(format!("{name}: executable code in a pure-data exemption").into());
            }
            continue;
        }
        let text = fs::read_to_string(root.join(&path))?;
        let lines = text.lines().count();
        if lines > 800 {
            files.insert(name.clone(), lines);
        }
        if ext == "rs" {
            let mut visitor = Functions::default();
            visitor.visit_file(&syn::parse_file(&text)?);
            for (symbol, length) in visitor.lengths {
                functions.insert(format!("{name}::{symbol}"), length);
            }
        }
    }
    let current = json!({"files": files, "functions": functions});
    let path = "contracts/size-baseline.json";
    if let Some(old) = committed(root, path)? {
        compare(&old, &current)?;
    }
    let baseline: Value = serde_json::from_slice(&fs::read(root.join(path))?)?;
    if let Some(old) = committed(root, path)? {
        compare(&old, &baseline)?;
    }
    if update {
        fs::write(
            root.join(path),
            format!("{}\n", serde_json::to_string_pretty(&current)?),
        )?;
    } else {
        compare(&baseline, &current)?;
    }
    println!(
        "size: {} files, {} functions above thresholds",
        files.len(),
        functions.len()
    );
    Ok(())
}

fn compare(baseline: &Value, current: &Value) -> Result {
    let mut errors = vec![];
    for kind in ["files", "functions"] {
        for (name, length) in current[kind].as_object().unwrap() {
            if length.as_u64().unwrap() > baseline[kind][name].as_u64().unwrap_or(0) {
                errors.push(format!("{name}: {length} lines (baseline {})", baseline[kind][name]));
            }
        }
    }
    if !errors.is_empty() {
        return Err(format!("size ratchet:\n{}", errors.join("\n")).into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn physical_function_spans_ignore_braces_inside_literals() {
        let source = format!(
            "impl Sample {{ fn measure() {{\nlet text = r#\"{{}}\"#;\n{}\n}} }}",
            "// padding\n".repeat(99)
        );
        let ast = syn::parse_file(&source).unwrap();
        let mut visitor = Functions::default();
        visitor.visit_file(&ast);
        assert_eq!(visitor.lengths["Sample::measure"], 103);
    }
}

#[cfg(test)]
mod regression_tests {
    use super::*;
    #[test]
    fn macro_definitions_cannot_hide_long_functions() {
        let code = format!(
            "macro_rules! generate {{ () => {{ fn oversized() {{\n{} }} }}; }}",
            "// padding\n".repeat(101)
        );
        let mut visitor = Functions::default();
        visitor.visit_file(&syn::parse_file(&code).unwrap());
        assert_eq!(visitor.lengths["oversized"], 103);
    }
    #[test]
    fn macro_metavariables_cannot_hide_long_functions() {
        let code = format!(
            "macro_rules! generate {{ ($name:ident) => {{ fn $name() {{\n{} }} }}; }}",
            "// padding\n".repeat(101)
        );
        let mut visitor = Functions::default();
        visitor.visit_file(&syn::parse_file(&code).unwrap());
        assert_eq!(visitor.lengths["::macro::name"], 103);
    }
    #[test]
    fn unused_baseline_entries_cannot_grow() {
        let old = json!({"files": {}, "functions": {}});
        let inflated = json!({"files": {"fiction.rs": 999}, "functions": {}});
        assert!(compare(&old, &inflated).is_err());
    }
}
