use crate::{Result, source};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};
use syn::visit::{self, Visit};

#[derive(Default)]
struct Rules {
    function: String,
    arms: BTreeMap<String, Vec<Value>>,
    blocks: BTreeMap<String, String>,
    constants: BTreeMap<String, Vec<String>>,
}
impl<'a> Visit<'a> for Rules {
    fn visit_item_fn(&mut self, node: &'a syn::ItemFn) {
        self.function = node.sig.ident.to_string();
        self.blocks.insert(self.function.clone(), source::tokens(&node.block));
        visit::visit_item_fn(self, node);
    }
    fn visit_impl_item_fn(&mut self, node: &'a syn::ImplItemFn) {
        self.function = node.sig.ident.to_string();
        self.blocks.insert(self.function.clone(), source::tokens(&node.block));
        visit::visit_impl_item_fn(self, node);
    }
    fn visit_arm(&mut self, arm: &'a syn::Arm) {
        self.arms.entry(self.function.clone()).or_default().push(json!({
            "prefixes": source::strings(&arm.pat), "result": source::tokens(&arm.body),
            "guard": arm.guard.as_ref().map(|(_, e)| source::tokens(e))
        }));
        visit::visit_arm(self, arm);
    }
    fn visit_item_const(&mut self, node: &'a syn::ItemConst) {
        self.constants
            .insert(node.ident.to_string(), source::strings(&node.expr));
    }
    fn visit_item_mod(&mut self, node: &'a syn::ItemMod) {
        if node.ident != "tests" {
            visit::visit_item_mod(self, node);
        }
    }
}
fn rules(root: &Path, path: &str) -> Result<Rules> {
    let mut rules = Rules::default();
    rules.visit_file(&syn::parse_file(&std::fs::read_to_string(root.join(path))?)?);
    Ok(rules)
}
fn mapping(rules: &Rules, function: &str) -> BTreeMap<String, String> {
    let mut map = BTreeMap::new();
    for arm in &rules.arms[function] {
        for prefix in arm["prefixes"].as_array().unwrap() {
            map.insert(
                prefix.as_str().unwrap().to_owned(),
                arm["result"].as_str().unwrap().to_owned(),
            );
        }
    }
    map
}

fn components(root: &Path, update: bool) -> Result {
    let dispatch = rules(root, "crates/pinchora-commands/src/infra/components.rs")?;
    let owner = rules(root, "crates/pinchora-commands/src/admin/command_control.rs")?;
    let gate = rules(root, "crates/pinchora-core/src/admin/modules.rs")?;
    let defer = rules(root, "crates/pinchora-commands/src/infra/defer.rs")?;
    let owners = mapping(&owner, "owner");
    let gates = mapping(&gate, "of_component_prefix");
    let mut prefixes = BTreeMap::new();
    for kind in ["dispatch", "dispatch_modal"] {
        for arm in &dispatch.arms[kind] {
            for prefix in arm["prefixes"].as_array().unwrap() {
                let p = prefix.as_str().unwrap();
                let modes: Vec<_> = defer.constants["OPENS_MODAL"]
                    .iter()
                    .filter(|s| s.split(':').next() == Some(p))
                    .collect();
                prefixes.insert(
                    format!("{kind}:{p}"),
                    json!({
                        "module_gate": gates.get(p), "owner": owners.get(p),
                        "handler": arm["result"], "guard": arm["guard"],
                        "default_defer": if p == "wf" && kind == "dispatch" { "Private" } else { "Public" },
                        "never_defer_custom_id_prefixes": if kind == "dispatch" { modes } else { vec![] }
                    }),
                );
            }
        }
    }
    // Retain conditional ownership, module overrides and timer rules, including data-dependent cases.
    let gates = rules(root, "crates/pinchora-commands/src/infra/gate.rs")?;
    source::snapshot(
        root,
        "components",
        &json!({"prefixes": prefixes,
        "owner_rules": owner.arms["component_owner"], "owner_overrides": owner.blocks["component_owner"],
        "module_rules": gates.blocks["refuses_component"], "defer_rules": defer.blocks["of"]}),
        update,
    )
}

#[derive(Default)]
struct Identifiers {
    redis: BTreeSet<String>,
    ledger: BTreeSet<String>,
    cache_file: bool,
}
impl<'a> Visit<'a> for Identifiers {
    fn visit_item_mod(&mut self, node: &'a syn::ItemMod) {
        if node.ident != "tests" {
            visit::visit_item_mod(self, node);
        }
    }
    fn visit_macro(&mut self, node: &'a syn::Macro) {
        if self.cache_file {
            for s in source::strings(&node.tokens) {
                if s.contains(':') && !s.contains(char::is_whitespace) && s.len() < 180 {
                    self.redis.insert(s);
                }
            }
        }
        visit::visit_macro(self, node);
    }
    fn visit_expr_method_call(&mut self, node: &'a syn::ExprMethodCall) {
        if [
            "spend_shells",
            "earn_shells",
            "buy_cosmetic",
            "grant_capped",
            "capped_game_win",
        ]
        .contains(&node.method.to_string().as_str())
            && let Some(last) = node.args.last()
        {
            self.ledger.extend(source::strings(last));
        }
        visit::visit_expr_method_call(self, node);
    }
    fn visit_field_value(&mut self, node: &'a syn::FieldValue) {
        if source::tokens(&node.member) == "reason" && matches!(node.expr, syn::Expr::Lit(_)) {
            self.ledger.extend(source::strings(&node.expr));
        }
        visit::visit_field_value(self, node);
    }
    fn visit_expr_call(&mut self, node: &'a syn::ExprCall) {
        let name = source::tokens(&node.func);
        let leaf = name.split("::").last().unwrap().trim();
        if [
            "ledger",
            "pay",
            "movement",
            "money",
            "money_record",
            "inventory_credit",
            "take_offer",
            "give_offer",
            "earn",
            "credit",
            "grant",
            "grant_capped",
            "game_credit",
            "capped_win",
            "capped_game_win",
            "spend_shells",
            "earn_shells",
            "buy_cosmetic",
        ]
        .contains(&leaf)
            && let Some(last) = node.args.last()
        {
            self.ledger.extend(source::strings(last));
        }
        visit::visit_expr_call(self, node);
    }
}
pub fn run(root: &Path, update: bool) -> Result {
    components(root, update)?;
    let mut visitor = Identifiers::default();
    let files = source::rust(root)?;
    for (path, ast) in &files {
        if path.contains("/src/")
            && (path.starts_with("crates/pinchora-cache/")
                || path.starts_with("crates/pinchora-db/")
                || path.starts_with("crates/pinchora-commands/"))
        {
            visitor.cache_file = path.starts_with("crates/pinchora-cache/");
            visitor.visit_file(ast);
        }
    }
    source::snapshot(
        root,
        "redis-key-patterns",
        &json!(super::keys::Keys::collect(&files)),
        update,
    )?;
    source::snapshot(root, "ledger-reasons", &json!(visitor.ledger), update)
}
