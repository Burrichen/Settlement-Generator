//! Where persistent user data lives.
//!
//! All user data lives in the per-user OS data directory (on Windows,
//! `%APPDATA%\SettlementGenerator`), never next to the installed binaries, so
//! application updates cannot touch it. Both interfaces use these paths, so
//! they always share the same database.

use std::path::PathBuf;

use crate::db::{Database, DbResult};

/// Folder name under the OS data directory.
pub const APP_DIR_NAME: &str = "SettlementGenerator";
/// SQLite database holding settings, content customisations and saved
/// settlements.
pub const DATABASE_FILE: &str = "settlement-generator.sqlite3";
/// Environment variable naming another default export folder.
pub const EXPORT_DIR_VAR: &str = "SETTLEMENT_GENERATOR_EXPORT_DIR";

/// The per-user data directory, or `None` if the OS does not provide one.
pub fn user_data_dir() -> Option<PathBuf> {
    dirs::data_dir().map(|d| d.join(APP_DIR_NAME))
}

/// Where exports go unless another folder is chosen: a "Settlement
/// Generator" folder in the user's Documents (or in the data directory if
/// the OS has no Documents folder). Never beside the binaries.
/// `SETTLEMENT_GENERATOR_EXPORT_DIR` overrides it (for scripts and tests).
pub fn default_export_dir() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os(EXPORT_DIR_VAR).filter(|d| !d.is_empty()) {
        return Some(PathBuf::from(dir));
    }
    dirs::document_dir()
        .map(|d| d.join("Settlement Generator"))
        .or_else(|| user_data_dir().map(|d| d.join("exports")))
}

pub fn default_database_path() -> Option<PathBuf> {
    user_data_dir().map(|d| d.join(DATABASE_FILE))
}

/// Opens the shared application database in the user data directory.
pub fn open_default_database() -> DbResult<Database> {
    let path = default_database_path().ok_or_else(|| crate::db::DbError::CreateDir {
        path: PathBuf::from(APP_DIR_NAME),
        source: std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "the operating system did not provide a user data directory",
        ),
    })?;
    Database::open(&path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn database_lives_in_an_app_specific_user_directory() {
        let path = default_database_path().expect("OS provides a data directory");
        assert!(path.ends_with(format!("{APP_DIR_NAME}/{DATABASE_FILE}")));
        let exe_dir = std::env::current_exe().unwrap();
        assert!(!path.starts_with(exe_dir.parent().unwrap()));
    }

    #[test]
    fn backups_live_beside_the_database_in_the_user_data_directory() {
        let dir = crate::backup::backup_dir_for(&default_database_path().unwrap());
        assert!(dir.starts_with(user_data_dir().unwrap()));
        let exe_dir = std::env::current_exe().unwrap();
        assert!(!dir.starts_with(exe_dir.parent().unwrap()));
    }
}
