use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=../../.git/HEAD");
    println!("cargo:rerun-if-env-changed=GIT_SHA");
    let output = |program: &str, args: &[&str]| -> String {
        Command::new(program)
            .args(args)
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
            .unwrap_or_else(|| "unknown".into())
    };
    println!(
        "cargo:rustc-env=PINCHORA_BUILD_COMMIT={}",
        std::env::var("GIT_SHA")
            .ok()
            .filter(|sha| !sha.trim().is_empty())
            .unwrap_or_else(|| output("git", &["rev-parse", "--short", "HEAD"]))
    );
    println!(
        "cargo:rustc-env=PINCHORA_BUILD_DATE={}",
        output("date", &["-u", "+%Y-%m-%d"])
    );
    println!(
        "cargo:rustc-env=PINCHORA_RUST_VERSION={}",
        output("rustc", &["--version"])
    );
}
