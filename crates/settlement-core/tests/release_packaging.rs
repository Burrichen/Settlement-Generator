//! Repository-wide guards for the Windows release packaging. See
//! docs/RELEASING.md.

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;

const CLI_BINARY: &str = "settlement-generator-cli";

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repository root")
}

fn read(rel: &str) -> String {
    fs::read_to_string(repo_root().join(rel)).unwrap_or_else(|e| panic!("reading {rel}: {e}"))
}

fn json(rel: &str) -> Value {
    serde_json::from_str(&read(rel)).unwrap()
}

#[test]
fn the_cli_binary_has_its_release_name() {
    let manifest = read("crates/settlement-cli/Cargo.toml");
    assert!(
        manifest.contains(&format!("[[bin]]\nname = \"{CLI_BINARY}\"")),
        "the CLI binary must be named {CLI_BINARY}"
    );
}

#[test]
fn the_release_installer_bundles_the_cli_beside_the_gui() {
    let release = json("src-tauri/tauri.release.conf.json");
    let bins = release["bundle"]["externalBin"]
        .as_array()
        .expect("externalBin");
    assert_eq!(bins, &[Value::from(format!("binaries/{CLI_BINARY}"))]);

    // The base config stays free of the sidecar so everyday builds and tests
    // do not need a prebuilt CLI.
    let base = json("src-tauri/tauri.conf.json");
    assert!(base["bundle"].get("externalBin").is_none());
    assert_ne!(
        base["mainBinaryName"].as_str(),
        Some(CLI_BINARY),
        "GUI and CLI are separate executables"
    );
}

#[test]
fn the_installer_is_a_per_user_nsis_installer() {
    let base = json("src-tauri/tauri.conf.json");
    assert_eq!(base["bundle"]["targets"], serde_json::json!(["nsis"]));
    assert_eq!(
        base["bundle"]["windows"]["nsis"]["installMode"],
        "currentUser"
    );
}

#[test]
fn release_scripts_are_wired_up() {
    let package = json("package.json");
    for script in [
        "release:build",
        "release:check",
        "release:manifest",
        "version:bump",
    ] {
        assert!(
            package["scripts"][script].is_string(),
            "package.json is missing the {script} script"
        );
    }
    let gitignore = read(".gitignore");
    assert!(
        gitignore.contains("/src-tauri/binaries"),
        "the staged sidecar must not be committed"
    );
}

#[test]
fn the_gui_name_limit_matches_the_core() {
    let source = read("app/src/components/generation.tsx");
    let expected = format!(
        "export const MAX_NAME_LENGTH = {};",
        settlement_core::settlements::MAX_NAME_LENGTH
    );
    assert!(
        source.contains(&expected),
        "generation.tsx must declare {expected}"
    );
}
