//! Repository-wide guards for safe updates: the updater configuration must
//! stay secure, and no private signing key may ever be committed. See
//! docs/RELEASING.md.

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repository root")
}

fn tauri_config() -> Value {
    let text = fs::read_to_string(repo_root().join("src-tauri/tauri.conf.json")).unwrap();
    serde_json::from_str(&text).unwrap()
}

#[test]
fn updater_requires_signed_updates_over_https_from_github_releases() {
    let conf = tauri_config();
    let updater = &conf["plugins"]["updater"];
    assert!(
        updater["pubkey"].as_str().is_some_and(|k| !k.is_empty()),
        "the updater must have a public key to verify signatures"
    );
    assert_eq!(
        updater["requireSignedVersion"], true,
        "the signed version must match the announced version (blocks downgrades)"
    );
    for flag in [
        "dangerousInsecureTransportProtocol",
        "dangerousAcceptInvalidCerts",
        "dangerousAcceptInvalidHostnames",
        "allowDowngrades",
    ] {
        assert!(
            updater.get(flag).is_none() || updater[flag] == false,
            "`{flag}` weakens updater security and must not be enabled"
        );
    }
    let endpoints = updater["endpoints"].as_array().expect("endpoints");
    assert!(!endpoints.is_empty());
    for endpoint in endpoints {
        let url = endpoint.as_str().unwrap();
        assert!(
            url.starts_with("https://github.com/")
                && url.ends_with("/releases/latest/download/latest.json"),
            "{url} must be the latest stable GitHub Release's latest.json"
        );
    }
    assert_eq!(
        conf["bundle"]["createUpdaterArtifacts"], true,
        "release builds must produce signed updater artifacts"
    );
}

#[test]
fn the_webview_cannot_drive_the_updater_directly() {
    // Updates go through the app's own commands, which back up user data
    // first; the frontend is given no updater plugin permissions.
    let text = fs::read_to_string(repo_root().join("src-tauri/capabilities/default.json")).unwrap();
    assert!(
        !text.contains("updater:"),
        "capabilities must not grant updater:* permissions"
    );
}

/// Directories that are generated or third-party, not repository content.
const SKIPPED: &[&str] = &["target", "node_modules", "dist", ".git", "gen"];

fn walk(dir: &Path, files: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).unwrap().flatten() {
        let path = entry.path();
        let name = entry.file_name();
        if path.is_dir() {
            if !SKIPPED.iter().any(|s| name == *s) {
                walk(&path, files);
            }
        } else {
            files.push(path);
        }
    }
}

#[test]
fn no_private_signing_key_is_in_the_repository() {
    // Built by concatenation so this file does not match itself. Tauri
    // private keys are minisign secret keys, stored base64-encoded.
    let markers = [
        ["rsign encrypted", " secret key"].concat(),
        ["minisign encrypted", " secret key"].concat(),
        [
            "dW50cnVzdGVkIGNvbW1lbnQ6IHJzaWduIGVuY3J5cHRl",
            "ZCBzZWNyZXQga2V5",
        ]
        .concat(),
    ];
    let mut files = Vec::new();
    walk(&repo_root(), &mut files);
    assert!(files.len() > 20, "the scan found the repository files");
    for file in files {
        let name = file.file_name().unwrap().to_string_lossy();
        assert!(
            !name.ends_with(".key"),
            "{} looks like a private key file",
            file.display()
        );
        let Ok(text) = fs::read_to_string(&file) else {
            continue; // binary
        };
        for marker in &markers {
            assert!(
                !text.contains(marker.as_str()),
                "{} contains a private signing key",
                file.display()
            );
        }
    }
}
