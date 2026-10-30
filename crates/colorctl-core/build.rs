//! Captures the compiler version for the About screen.
//!
//! `option_env!("RUSTC_VERSION")` in `about.rs` falls back to "unknown" when
//! this script has not run, so a plain `cargo build` still works.

use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");

    let version = Command::new(std::env::var("RUSTC").unwrap_or_else(|_| "rustc".into()))
        .arg("--version")
        .output()
        .ok()
        .and_then(|out| String::from_utf8(out.stdout).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "rustc (unknown)".to_string());

    println!("cargo:rustc-env=RUSTC_VERSION={version}");
}
