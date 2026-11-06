//! Embeds the dashboard's built SPA (`web/dist`, made by `npm run build`) into the binary.
//!
//! Without Node there is no build: a placeholder page is written to `web/dist` so `cargo build` and
//! `cargo test` work anyway. `PINCHORA_WEB__STATIC_DIR` serves a folder from disk instead.

use std::{
    env, fs,
    path::{Path, PathBuf},
};

const PLACEHOLDER: &str = r#"<!doctype html>
<html lang="en">
<head><meta charset="utf-8"><title>Pinchora dashboard</title></head>
<body><h1>Pinchora dashboard</h1>
<p>This binary was built without the web app. Run <code>npm ci &amp;&amp; npm run build</code> in <code>web/</code>, then build again.</p>
</body>
</html>
"#;

fn main() {
    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("manifest dir"));
    let dist = manifest.join("../../web/dist");
    if !dist.join("index.html").exists() {
        fs::create_dir_all(&dist).expect("create web/dist");
        fs::write(dist.join("index.html"), PLACEHOLDER).expect("write the placeholder page");
    }
    println!("cargo:rerun-if-changed={}", dist.display());

    let mut files = Vec::new();
    collect(&dist, &dist, &mut files);
    files.sort();
    let mut out = String::from(
        "/// The built SPA: path inside `web/dist` and the file's bytes.\npub static SPA: &[(&str, &[u8])] = &[\n",
    );
    for (name, path) in files {
        out.push_str(&format!("    ({name:?}, include_bytes!({path:?})),\n"));
    }
    out.push_str("];\n");
    let target = PathBuf::from(env::var("OUT_DIR").expect("out dir")).join("spa.rs");
    fs::write(target, out).expect("write spa.rs");
}

fn collect(root: &Path, dir: &Path, out: &mut Vec<(String, String)>) {
    let Ok(entries) = fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect(root, &path, out);
        } else if let Ok(relative) = path.strip_prefix(root) {
            let name = relative
                .components()
                .map(|c| c.as_os_str().to_string_lossy())
                .collect::<Vec<_>>()
                .join("/");
            let absolute = path.canonicalize().unwrap_or(path.clone());
            out.push((name, absolute.to_string_lossy().into_owned()));
        }
    }
}
