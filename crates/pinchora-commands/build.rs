use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=../../.git/HEAD");
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
        output("git", &["rev-parse", "--short", "HEAD"])
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
