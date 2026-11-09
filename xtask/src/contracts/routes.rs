use crate::{Result, source};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};
use syn::{Expr, Item, Stmt};
type Routes = BTreeSet<String>;
struct Catalog {
    functions: BTreeMap<String, syn::ItemFn>,
}

pub fn collect(root: &Path) -> Result<Routes> {
    let mut functions = BTreeMap::new();
    load(&root.join("crates/pinchora-web/src/lib.rs"), "crate", &mut functions)?;
    Catalog { functions }.function("crate::routes::router")
}
fn load(path: &Path, module: &str, functions: &mut BTreeMap<String, syn::ItemFn>) -> Result {
    let ast = syn::parse_file(&std::fs::read_to_string(path)?)?;
    for item in ast.items {
        match item {
            Item::Fn(f) => {
                functions.insert(format!("{module}::{}", f.sig.ident), f);
            }
            Item::Mod(m) if m.content.is_none() && m.ident != "tests" => {
                let explicit = m.attrs.iter().find_map(|a| {
                    if !a.path().is_ident("path") {
                        return None;
                    }
                    if let syn::Meta::NameValue(v) = &a.meta {
                        literal(&v.value).ok()
                    } else {
                        None
                    }
                });
                let parent = path.parent().unwrap();
                let child = if let Some(p) = explicit {
                    parent.join(p)
                } else {
                    let base = if matches!(path.file_stem().unwrap().to_str(), Some("lib" | "mod")) {
                        parent.to_path_buf()
                    } else {
                        parent.join(path.file_stem().unwrap())
                    };
                    let simple = base.join(format!("{}.rs", m.ident));
                    if simple.exists() {
                        simple
                    } else {
                        base.join(m.ident.to_string()).join("mod.rs")
                    }
                };
                load(&child, &format!("{module}::{}", m.ident), functions)?;
            }
            _ => {}
        }
    }
    Ok(())
}
impl Catalog {
    fn function(&self, name: &str) -> Result<Routes> {
        let function = self
            .functions
            .get(name)
            .ok_or_else(|| format!("unresolved router {name}"))?;
        let mut locals = BTreeMap::new();
        let mut result = Routes::new();
        for stmt in &function.block.stmts {
            match stmt {
                Stmt::Local(local) => {
                    if let (syn::Pat::Ident(id), Some(init)) = (&local.pat, &local.init) {
                        locals.insert(id.ident.to_string(), &*init.expr);
                    }
                }
                Stmt::Expr(expr, None) => result.extend(self.expr(expr, &locals)?),
                Stmt::Item(Item::Use(_)) => {}
                _ => return Err(format!("unsupported statement in router {name}; extend the collector").into()),
            }
        }
        Ok(result)
    }
    fn expr(&self, expr: &Expr, locals: &BTreeMap<String, &Expr>) -> Result<Routes> {
        let mut result = Routes::new();
        match expr {
            Expr::MethodCall(call) => {
                result.extend(self.expr(&call.receiver, locals)?);
                let method = call.method.to_string();
                if method == "route" {
                    let path = literal(&call.args[0])?;
                    for method in methods(&call.args[1])? {
                        result.insert(format!("{method} {path}"));
                    }
                } else if method == "nest" || method == "merge" {
                    let (prefix, child) = if method == "nest" {
                        (literal(&call.args[0])?, &call.args[1])
                    } else {
                        (String::new(), &call.args[0])
                    };
                    for route in self.expr(child, locals)? {
                        let (method, path) = route.split_once(' ').unwrap();
                        result.insert(format!("{method} {prefix}{path}"));
                    }
                } else if !matches!(
                    method.as_str(),
                    "route_layer" | "layer" | "with_state" | "fallback" | "fallback_service"
                ) {
                    return Err(format!("unsupported router method {method}").into());
                }
            }
            Expr::Call(call) => {
                let name = source::tokens(&call.func).replace(' ', "");
                if name.ends_with("Router::new") {
                    return Ok(result);
                }
                let name = if name.starts_with("reef::") {
                    format!("crate::{name}")
                } else {
                    name
                };
                result.extend(self.function(&name)?);
            }
            Expr::Path(p) => {
                let name = source::tokens(p);
                let expr = locals
                    .get(&name)
                    .ok_or_else(|| format!("unresolved router binding {name}"))?;
                result.extend(self.expr(expr, locals)?);
            }
            _ => return Err(format!("unsupported router expression {}", source::tokens(expr)).into()),
        }
        Ok(result)
    }
}
fn literal(expr: &Expr) -> Result<String> {
    if let Expr::Lit(syn::ExprLit {
        lit: syn::Lit::Str(s), ..
    }) = expr
    {
        Ok(s.value())
    } else {
        Err("route path must be a literal; extend the collector".into())
    }
}
fn methods(expr: &Expr) -> Result<Routes> {
    let (method, rest) = match expr {
        Expr::Call(call) => (
            source::tokens(&call.func).split("::").last().unwrap().trim().to_owned(),
            None,
        ),
        Expr::MethodCall(call) => (call.method.to_string(), Some(&*call.receiver)),
        _ => return Err("unsupported route methods".into()),
    };
    let mut result = if let Some(rest) = rest {
        methods(rest)?
    } else {
        Routes::new()
    };
    if [
        "get", "post", "put", "patch", "delete", "head", "options", "trace", "any",
    ]
    .contains(&method.as_str())
    {
        result.insert(method.to_uppercase());
        // Axum's get also installs HEAD; pin this outward-facing method explicitly.
        if method == "get" {
            result.insert("HEAD".into());
        }
    } else if !["layer", "route_layer"].contains(&method.as_str()) {
        return Err(format!("unsupported method {method}").into());
    }
    Ok(result)
}
