//! Application updates through the Tauri updater and GitHub Releases.
//!
//! Every update is signed: the updater verifies the downloaded installer
//! against the public key in `tauri.conf.json` (and the version recorded in
//! the signature) before anything is installed. Nothing installs without the
//! user accepting it, and user data is flushed and backed up first. These
//! commands are the only route to the updater; the webview gets no direct
//! updater permissions.

use std::sync::Mutex;

use semver::Version;
use serde::Serialize;
use settlement_core::service::AppService;
use tauri::ipc::Channel;
use tauri::{AppHandle, State};
use tauri_plugin_updater::{Update, UpdaterExt};

use crate::AppState;

/// An available update, as shown to the user.
#[derive(Serialize, Clone, Debug)]
pub struct UpdateOffer {
    pub current_version: String,
    pub version: String,
    pub notes: Option<String>,
}

/// Progress reported while an accepted update is installed.
#[derive(Serialize, Clone, Debug)]
#[serde(tag = "stage", rename_all = "snake_case")]
pub enum InstallProgress {
    Downloading { downloaded: u64, total: Option<u64> },
    BackingUp,
    Installing,
}

/// The update found by the last check, waiting for the user's answer.
#[derive(Default)]
pub struct PendingUpdate(Mutex<Option<Update>>);

/// Only stable releases newer than the running version are offered.
pub fn is_offered(current: &Version, available: &Version) -> bool {
    available.pre.is_empty() && available > current
}

/// Checks GitHub Releases for a newer stable version. Development builds
/// never check, so a debug build can never replace itself.
#[tauri::command]
pub async fn update_check(
    app: AppHandle,
    pending: State<'_, PendingUpdate>,
) -> Result<Option<UpdateOffer>, String> {
    if cfg!(debug_assertions) {
        return Ok(None);
    }
    let update = app
        .updater_builder()
        .version_comparator(|current, release| is_offered(&current, &release.version))
        .build()
        .map_err(|e| e.to_string())?
        .check()
        .await
        .map_err(|e| e.to_string())?;
    let offer = update.as_ref().map(|u| UpdateOffer {
        current_version: u.current_version.clone(),
        version: u.version.clone(),
        notes: u.body.clone().filter(|b| !b.trim().is_empty()),
    });
    *pending.0.lock().map_err(|_| "update lock poisoned")? = update;
    Ok(offer)
}

/// Installs the update the user accepted: downloads and verifies it, flushes
/// and backs up user data, closes the database, then runs the installer
/// (which closes and restarts the application). Any failure before the
/// installer starts leaves the application running with its data.
#[tauri::command]
pub async fn update_install(
    app: AppHandle,
    state: State<'_, AppState>,
    pending: State<'_, PendingUpdate>,
    on_progress: Channel<InstallProgress>,
) -> Result<(), String> {
    let update = pending
        .0
        .lock()
        .map_err(|_| "update lock poisoned")?
        .take()
        .ok_or("No update is waiting to be installed.")?;

    // Download and verify the signature before touching user data.
    let mut downloaded = 0u64;
    let bytes = update
        .download(
            |chunk, total| {
                downloaded += chunk as u64;
                let _ = on_progress.send(InstallProgress::Downloading { downloaded, total });
            },
            || {},
        )
        .await
        .map_err(|e| format!("The update could not be downloaded or verified: {e}"))?;

    let _ = on_progress.send(InstallProgress::BackingUp);
    {
        // Same lock order as the other commands: generation, then service.
        let mut generation = state
            .generation
            .lock()
            .map_err(|_| "generation lock poisoned")?;
        let mut service = state.service.lock().map_err(|_| "service lock poisoned")?;
        let backup = match service.as_mut() {
            Ok(s) => s.prepare_for_update(&update.version).map(|_| ()),
            // The database could not be opened (for example data from a newer
            // version): back up the file itself, so an update that may fix
            // the problem can still be installed safely.
            Err(_) => AppService::prepare_unopened_for_update(&update.version).map(|_| ()),
        };
        if let Err(e) = backup {
            // Keep the offer so the user can try again.
            if let Ok(mut slot) = pending.0.lock() {
                *slot = Some(update);
            }
            return Err(format!(
                "Your data could not be backed up, so the update was not installed: {e}"
            ));
        }
        // Closing the database checkpoints its log, so the file on disk is
        // complete and nothing is mid-write when the installer runs.
        *generation = None;
        *service = Err("Settlement Generator is installing an update.".into());
    }

    let _ = on_progress.send(InstallProgress::Installing);
    // On Windows this starts the installer and exits the application.
    if let Err(e) = update.install(bytes) {
        if let Ok(mut service) = state.service.lock() {
            *service = AppService::open_default();
        }
        return Err(format!("The update could not be installed: {e}"));
    }
    app.restart()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(text: &str) -> Version {
        Version::parse(text).unwrap()
    }

    #[test]
    fn only_newer_stable_versions_are_offered() {
        let current = v("0.1.0");
        assert!(is_offered(&current, &v("0.1.1")));
        assert!(is_offered(&current, &v("1.0.0")));
        assert!(!is_offered(&current, &v("0.1.0")), "same version");
        assert!(!is_offered(&current, &v("0.0.9")), "downgrade");
        assert!(!is_offered(&current, &v("0.2.0-beta.1")), "pre-release");
    }

    #[test]
    fn the_updater_compares_against_the_canonical_version() {
        // Tauri reads the app version from this crate's (workspace) version,
        // which is also the version every interface displays.
        assert_eq!(env!("CARGO_PKG_VERSION"), settlement_core::VERSION);
    }
}
