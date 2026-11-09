use crate::Result;
use quote::ToTokens;
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

pub fn files(root: &Path) -> Result<Vec<PathBuf>> {
    let output = Command::new("git")
        .args(["ls-files", "--cached", "--others", "--exclude-standard", "-z"])
        .current_dir(root)
        .output()?;
    if !output.status.success() {
        return Err("git ls-files failed".into());
    }
    let mut paths: Vec<_> = output
        .stdout
        .split(|b| *b == 0)
        .filter(|s| !s.is_empty())
        .map(|s| PathBuf::from(String::from_utf8_lossy(s).as_ref()))
        .filter(|p| root.join(p).is_file())
        .collect();
    paths.sort();
    paths.dedup();
    Ok(paths)
}

pub fn rust(root: &Path) -> Result<Vec<(String, syn::File)>> {
    files(root)?
        .into_iter()
        .filter(|p| p.extension().is_some_and(|s| s == "rs"))
        .map(|p| {
            Ok((
                p.to_string_lossy().into_owned(),
                syn::parse_file(&fs::read_to_string(root.join(p))?)?,
            ))
        })
        .collect()
}

pub fn tokens(node: &impl ToTokens) -> String {
    node.to_token_stream().to_string()
}

pub fn strings(node: &impl ToTokens) -> Vec<String> {
    fn walk(stream: proc_macro2::TokenStream, out: &mut Vec<String>) {
        for token in stream {
            match token {
                proc_macro2::TokenTree::Group(g) => walk(g.stream(), out),
                proc_macro2::TokenTree::Literal(l) => {
                    if let Ok(s) = syn::parse_str::<syn::LitStr>(&l.to_string()) {
                        out.push(s.value());
                    }
                }
                _ => {}
            }
        }
    }
    let mut result = vec![];
    walk(node.to_token_stream(), &mut result);
    result
}

pub fn snapshot(root: &Path, name: &str, value: &serde_json::Value, update: bool) -> Result {
    let path = root.join("contracts").join(format!("{name}.json"));
    let actual = format!("{}\n", serde_json::to_string_pretty(value)?);
    if update {
        fs::write(path, actual)?;
        return Ok(());
    }
    let expected = fs::read_to_string(&path).map_err(|e| format!("{name}: {e}; run contracts --update"))?;
    if expected != actual {
        let expected: serde_json::Value = serde_json::from_str(&expected)?;
        diff(name, "", &expected, value);
        return Err(format!("contract {name} changed; review before contracts --update").into());
    }
    Ok(())
}

fn diff(name: &str, path: &str, old: &serde_json::Value, new: &serde_json::Value) {
    if old == new {
        return;
    }
    if let (Some(a), Some(b)) = (old.as_object(), new.as_object()) {
        let keys: std::collections::BTreeSet<_> = a.keys().chain(b.keys()).collect();
        for key in keys {
            diff(name, &format!("{path}/{key}"), &old[key], &new[key]);
        }
    } else if let (Some(a), Some(b)) = (old.as_array(), new.as_array()) {
        if a.iter().chain(b).all(serde_json::Value::is_string) {
            let a: std::collections::BTreeSet<_> = a.iter().map(|v| v.as_str().unwrap()).collect();
            let b: std::collections::BTreeSet<_> = b.iter().map(|v| v.as_str().unwrap()).collect();
            for removed in a.difference(&b) {
                eprintln!("{name}{path}: - {removed}");
            }
            for added in b.difference(&a) {
                eprintln!("{name}{path}: + {added}");
            }
        } else {
            for index in 0..a.len().max(b.len()) {
                diff(
                    name,
                    &format!("{path}/{index}"),
                    a.get(index).unwrap_or(&serde_json::Value::Null),
                    b.get(index).unwrap_or(&serde_json::Value::Null),
                );
            }
        }
    } else {
        let clip = |v: &serde_json::Value| {
            let s = v.to_string();
            s.chars().take(400).collect::<String>()
        };
        eprintln!("{name}{path}:\n- {}\n+ {}", clip(old), clip(new));
    }
}
