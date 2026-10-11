//! Backups of the user database, taken before anything could put user data
//! at risk: before an accepted application update is installed, and before
//! schema migrations run on an existing database.
//!
//! A backup is a complete, self-contained SQLite file written with
//! `VACUUM INTO` (consistent even while the database is in WAL mode), then
//! re-opened and integrity-checked. Backups live in a `backups` folder beside
//! the database in the per-user data directory, never in the application's
//! install directory. Only the newest few of each kind are kept.

use std::path::{Path, PathBuf};

use rusqlite::{Connection, OpenFlags};
use time::OffsetDateTime;

/// Folder, beside the database, that holds backups.
pub const BACKUP_DIR: &str = "backups";
/// How many backups of each kind are kept; older ones are removed after a
/// new backup of that kind succeeds.
pub const KEEP_PER_KIND: usize = 5;
const EXTENSION: &str = "sqlite3";

/// Why a backup was taken. Part of the file name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BackupKind {
    /// Before installing an application update from `from` to `to`.
    PreUpdate { from: String, to: String },
    /// Before migrating the database schema from `from` to `to`.
    PreMigration { from: u32, to: u32 },
}

impl BackupKind {
    pub const PRE_UPDATE_PREFIX: &str = "pre-update-";
    pub const PRE_MIGRATION_PREFIX: &str = "pre-migration-";

    /// File-name prefix shared by every backup of this kind.
    pub fn prefix(&self) -> &'static str {
        match self {
            BackupKind::PreUpdate { .. } => Self::PRE_UPDATE_PREFIX,
            BackupKind::PreMigration { .. } => Self::PRE_MIGRATION_PREFIX,
        }
    }

    fn detail(&self) -> String {
        match self {
            BackupKind::PreUpdate { from, to } => {
                format!("v{}-to-v{}", file_safe(from), file_safe(to))
            }
            BackupKind::PreMigration { from, to } => format!("schema{from}-to-schema{to}"),
        }
    }
}

/// The backup folder for the database at `database`.
pub fn backup_dir_for(database: &Path) -> PathBuf {
    database
        .parent()
        .map_or_else(|| PathBuf::from(BACKUP_DIR), |d| d.join(BACKUP_DIR))
}

/// Keeps version text (which may come from a remote release) safe to use in a
/// file name: anything but ASCII letters, digits, `.` and `-` becomes `_`.
fn file_safe(text: &str) -> String {
    let safe: String = text
        .chars()
        .take(40)
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '.' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect();
    if safe.is_empty() {
        "unknown".into()
    } else {
        safe
    }
}

/// UTC timestamp that sorts chronologically as text, e.g.
/// `20261011T142233123Z`.
fn stamp(t: OffsetDateTime) -> String {
    format!(
        "{:04}{:02}{:02}T{:02}{:02}{:02}{:03}Z",
        t.year(),
        u8::from(t.month()),
        t.day(),
        t.hour(),
        t.minute(),
        t.second(),
        t.millisecond()
    )
}

/// Writes a verified backup of `conn`'s database into `dir` and returns its
/// path. Nothing is written over an existing file.
pub(crate) fn write_backup(
    conn: &Connection,
    dir: &Path,
    kind: &BackupKind,
) -> Result<PathBuf, String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("could not create {}: {e}", dir.display()))?;
    let base = format!(
        "{}{}-{}",
        kind.prefix(),
        stamp(OffsetDateTime::now_utc()),
        kind.detail()
    );
    // Reserve a unique name by creating it exclusively (VACUUM INTO accepts
    // an empty file), so two processes backing up in the same millisecond
    // never write into the same file.
    let mut n = 1;
    let path = loop {
        let candidate = if n == 1 {
            dir.join(format!("{base}.{EXTENSION}"))
        } else {
            dir.join(format!("{base}-{n}.{EXTENSION}"))
        };
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&candidate)
        {
            Ok(_) => break candidate,
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists && n < 100 => n += 1,
            Err(e) => return Err(format!("could not create {}: {e}", candidate.display())),
        }
    };
    let target = path
        .to_str()
        .ok_or_else(|| format!("{} is not a valid UTF-8 path", path.display()))?;
    // Another process (the other interface) may commit a migration while the
    // copy is made; the copy is then consistent at one version or the other.
    let before = user_version(conn).map_err(|e| e.to_string())?;
    if let Err(e) = conn.execute("VACUUM INTO ?1", [target]) {
        let _ = std::fs::remove_file(&path);
        return Err(e.to_string());
    }
    let after = user_version(conn).map_err(|e| e.to_string())?;
    if let Err(e) = verify(&path, &[before, after]) {
        let _ = std::fs::remove_file(&path);
        return Err(format!("the backup failed verification: {e}"));
    }
    Ok(path)
}

fn user_version(conn: &Connection) -> rusqlite::Result<u32> {
    conn.pragma_query_value(None, "user_version", |row| row.get(0))
}

/// Re-opens a backup read-only and checks it is intact and at one of the
/// expected schema versions.
fn verify(path: &Path, expected: &[u32]) -> Result<(), String> {
    let backup = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|e| e.to_string())?;
    let check: String = backup
        .query_row("PRAGMA quick_check", [], |row| row.get(0))
        .map_err(|e| e.to_string())?;
    if check != "ok" {
        return Err(check);
    }
    let found = user_version(&backup).map_err(|e| e.to_string())?;
    if !expected.contains(&found) {
        return Err(format!("schema version {found}, expected {expected:?}"));
    }
    Ok(())
}

/// Backups in `dir` of the kind with `prefix`, oldest first.
pub fn list_backups(dir: &Path, prefix: &str) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut found: Vec<PathBuf> = entries
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            p.extension().is_some_and(|e| e == EXTENSION)
                && p.file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.starts_with(prefix))
        })
        .collect();
    // The timestamp follows the prefix, so names sort chronologically.
    found.sort();
    found
}

/// Removes all but the newest [`KEEP_PER_KIND`] backups with `prefix`.
/// Failures are ignored: a leftover backup is harmless.
pub(crate) fn prune(dir: &Path, prefix: &str) {
    let backups = list_backups(dir, prefix);
    let excess = backups.len().saturating_sub(KEEP_PER_KIND);
    for old in &backups[..excess] {
        let _ = std::fs::remove_file(old);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remote_version_text_cannot_escape_the_backup_folder() {
        assert_eq!(file_safe("0.2.0"), "0.2.0");
        assert_eq!(file_safe("..\\..\\evil/x"), ".._.._evil_x");
        assert_eq!(file_safe(""), "unknown");
    }

    #[test]
    fn stamps_sort_chronologically() {
        let a = OffsetDateTime::from_unix_timestamp(1_700_000_000).unwrap();
        let b = OffsetDateTime::from_unix_timestamp(1_800_000_000).unwrap();
        assert!(stamp(a) < stamp(b));
        assert_eq!(stamp(a).len(), "20231114T221320000Z".len());
    }
}
