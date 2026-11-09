//! Stable source fingerprint: cached observations cannot validate changed production/test inputs.
use std::{fs, path::Path, process::Command};
pub fn fingerprint(root: &Path) -> std::io::Result<String> {
    let output = Command::new("git")
        .args(["ls-files", "--cached", "--others", "--exclude-standard", "-z"])
        .current_dir(root)
        .output()?;
    if !output.status.success() {
        return Err(std::io::Error::other("git ls-files failed"));
    }
    let mut files: Vec<_> = output
        .stdout
        .split(|b| *b == 0)
        .filter_map(|p| std::str::from_utf8(p).ok())
        .filter(|p| p.starts_with("crates/") || p.starts_with("content/") || *p == "Cargo.toml" || *p == "Cargo.lock")
        .filter(|p| ["rs", "toml", "json", "lock"].contains(&p.rsplit('.').next().unwrap_or("")))
        .filter(|p| root.join(p).is_file())
        .collect();
    files.sort_unstable();
    files.dedup();
    let mut hash = 0xcbf29ce484222325u64;
    for file in files {
        for byte in file.bytes().chain([0]).chain(fs::read(root.join(file))?).chain([0]) {
            hash = (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3);
        }
    }
    Ok(format!("{hash:016x}"))
}
