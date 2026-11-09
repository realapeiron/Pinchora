//! Follow key arguments into local bindings and small key helpers; ignore runtime suffix values.
use crate::source;
use std::collections::{BTreeMap, BTreeSet};
use syn::{
    Expr,
    visit::{self, Visit},
};
#[derive(Default)]
pub struct Keys {
    pub patterns: BTreeSet<String>,
    functions: BTreeMap<String, syn::Block>,
    locals: BTreeMap<String, Expr>,
    cache_file: bool,
}
impl Keys {
    pub fn collect(files: &[(String, syn::File)]) -> BTreeSet<String> {
        let mut keys = Self::default();
        for (path, file) in files {
            if !path.contains("/src/") {
                continue;
            }
            keys.functions.clear();
            struct Helpers<'a>(&'a mut BTreeMap<String, syn::Block>);
            impl<'a> Visit<'a> for Helpers<'_> {
                fn visit_item_fn(&mut self, f: &'a syn::ItemFn) {
                    self.0.insert(f.sig.ident.to_string(), *f.block.clone());
                }
                fn visit_impl_item_fn(&mut self, f: &'a syn::ImplItemFn) {
                    self.0.insert(f.sig.ident.to_string(), f.block.clone());
                }
            }
            Helpers(&mut keys.functions).visit_file(file);
            keys.cache_file = path.starts_with("crates/pinchora-cache/");
            keys.visit_file(file);
        }
        keys.patterns
    }
    fn resolve(&mut self, expr: &Expr, depth: usize) {
        if depth > 6 {
            return;
        }
        for s in source::strings(expr) {
            if !s.is_empty() && s.len() < 180 && !s.contains(char::is_whitespace) && !s.contains("://") {
                self.patterns.insert(s);
            }
        }
        match expr {
            Expr::Reference(r) => self.resolve(&r.expr, depth + 1),
            Expr::Path(p) => {
                if let Some(value) = self.locals.get(&source::tokens(p)).cloned() {
                    self.resolve(&value, depth + 1);
                }
            }
            Expr::Call(c) => {
                let name = source::tokens(&c.func).split("::").last().unwrap().trim().to_owned();
                if let Some(block) = self.functions.get(&name) {
                    for s in source::strings(block) {
                        if s.contains(':') && !s.contains(char::is_whitespace) && s.len() < 180 {
                            self.patterns.insert(s);
                        }
                    }
                }
            }
            _ => {}
        }
    }
}
impl<'a> Visit<'a> for Keys {
    fn visit_item_mod(&mut self, node: &'a syn::ItemMod) {
        if node.ident != "tests" {
            visit::visit_item_mod(self, node);
        }
    }
    fn visit_item_fn(&mut self, f: &'a syn::ItemFn) {
        self.locals.clear();
        if f.sig.ident.to_string().ends_with("_key") || f.sig.ident == "key" {
            for s in source::strings(&f.block) {
                if s.contains(':') && !s.contains(char::is_whitespace) && s.len() < 180 {
                    self.patterns.insert(s);
                }
            }
        }
        visit::visit_item_fn(self, f);
    }
    fn visit_impl_item_fn(&mut self, f: &'a syn::ImplItemFn) {
        self.locals.clear();
        visit::visit_impl_item_fn(self, f);
    }
    fn visit_local(&mut self, node: &'a syn::Local) {
        if let (syn::Pat::Ident(id), Some(init)) = (&node.pat, &node.init) {
            self.locals.insert(id.ident.to_string(), *init.expr.clone());
        }
        visit::visit_local(self, node);
    }
    fn visit_expr_method_call(&mut self, node: &'a syn::ExprMethodCall) {
        let receiver = source::tokens(&node.receiver);
        let name = node.method.to_string();
        if (receiver.contains("cache") || name.starts_with("reef_"))
            && [
                "get",
                "put",
                "put_if_present",
                "compare_put",
                "take",
                "claim_observed_value",
                "extend",
                "forget",
                "incr",
                "incr_by",
                "counter",
                "once",
                "try_cooldown",
                "release_cooldown",
                "try_reserve",
                "release_reservation",
                "map_set",
                "map_replace",
                "map_merge",
                "map_remove",
                "map_all",
                "map_len",
                "set_add",
                "set_remove",
                "set_members",
                "reef_key",
                "reef_get",
                "reef_publish",
                "reef_subscribe",
                "reef_acquire",
                "reef_renew",
                "reef_release",
            ]
            .contains(&name.as_str())
            && let Some(key) = node.args.first()
        {
            self.resolve(key, 0);
        }
        visit::visit_expr_method_call(self, node);
    }
    fn visit_macro(&mut self, node: &'a syn::Macro) {
        if self.cache_file {
            for s in source::strings(&node.tokens) {
                if s.contains(':') && !s.contains(char::is_whitespace) && s.len() < 180 {
                    self.patterns.insert(s);
                }
            }
        }
        visit::visit_macro(self, node);
    }
    fn visit_item_const(&mut self, node: &'a syn::ItemConst) {
        if self.cache_file && node.ident == "PREFIX" {
            self.patterns.extend(source::strings(&node.expr));
        }
    }
}
