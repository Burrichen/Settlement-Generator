//! Repository-wide guard: the application version must only be defined in the
//! workspace `Cargo.toml`. See docs/VERSIONING.md.

use std::fs;
use std::path::{Path, PathBuf};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repository root")
}

fn read(rel: &str) -> String {
    fs::read_to_string(repo_root().join(rel)).unwrap_or_else(|e| panic!("reading {rel}: {e}"))
}

#[test]
fn workspace_defines_the_core_version() {
    let manifest = read("Cargo.toml");
    let expected = format!("version = \"{}\"", settlement_core::VERSION);
    assert!(
        manifest.contains(&expected),
        "root Cargo.toml must define {expected} under [workspace.package]"
    );
}

#[test]
fn member_crates_inherit_workspace_version() {
    for manifest in [
        "crates/settlement-core/Cargo.toml",
        "crates/settlement-cli/Cargo.toml",
        "src-tauri/Cargo.toml",
    ] {
        let text = read(manifest);
        assert!(
            text.contains("version.workspace = true"),
            "{manifest} must use `version.workspace = true`"
        );
    }
}

#[test]
fn tauri_config_has_no_hardcoded_version() {
    let conf: serde_json::Value = serde_json::from_str(&read("src-tauri/tauri.conf.json")).unwrap();
    assert!(
        conf.get("version").is_none(),
        "tauri.conf.json must not set `version`; Tauri falls back to the Cargo version"
    );
}

#[test]
fn package_json_has_no_hardcoded_version() {
    let pkg: serde_json::Value = serde_json::from_str(&read("package.json")).unwrap();
    assert!(
        pkg.get("version").is_none(),
        "package.json must not set `version`; the frontend reads it from the core"
    );
}
