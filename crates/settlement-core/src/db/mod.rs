//! SQLite-backed persistent application data: settings, user content
//! overrides and saved settlements.
//!
//! The database lives in the per-user data directory (see
//! [`crate::storage`]), never beside the application binaries. Schema changes
//! are applied by numbered migrations, each in its own transaction, and the
//! applied version is tracked in `PRAGMA user_version`.

use std::path::{Path, PathBuf};
use std::time::Duration;

use rusqlite::{Connection, OptionalExtension, Transaction, TransactionBehavior, params};
use thiserror::Error;

use crate::VERSION;
use crate::backup::{self, BackupKind};
use crate::framework::SettingValues;
use crate::layers::{OVERRIDES_FORMAT_VERSION, UserOverrides};

/// The `settings` key holding the values of content-defined settings.
const CONTENT_SETTINGS_KEY: &str = "content_settings";
/// Prefix of the `settings` keys holding editor drafts.
const DRAFT_PREFIX: &str = "draft.";
use crate::settings::{Settings, SettingsError};
use crate::settlements::{
    MAX_NAME_LENGTH, SNAPSHOT_FORMAT_VERSION, Settlement, SettlementId, SettlementSnapshot,
    SettlementSummary, from_unix_millis, now, to_unix_millis,
};

pub(crate) struct Migration {
    pub version: u32,
    pub name: &'static str,
    pub sql: &'static str,
}

/// All migrations, in order. Never edit a released migration; add a new one.
pub(crate) const MIGRATIONS: &[Migration] = &[Migration {
    version: 1,
    name: "initial",
    sql: include_str!("migrations/0001_initial.sql"),
}];

/// Schema version this build creates and understands.
pub fn latest_schema_version() -> u32 {
    MIGRATIONS.last().map_or(0, |m| m.version)
}

const FAVOURITE_TRIGGER_MESSAGE: &str = "favourite settlements cannot be deleted";

#[derive(Debug, Error)]
pub enum DbError {
    #[error("could not create data directory {path}: {source}")]
    CreateDir {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error(transparent)]
    Sqlite(#[from] rusqlite::Error),
    #[error(
        "the database was created by a newer version of the application (schema {found}, this version supports {supported})"
    )]
    NewerSchema { found: u32, supported: u32 },
    #[error("database migration {version} ({name}) failed and was rolled back: {source}")]
    Migration {
        version: u32,
        name: &'static str,
        #[source]
        source: rusqlite::Error,
    },
    #[error("stored {what} is not valid: {source}")]
    Corrupt {
        what: String,
        #[source]
        source: serde_json::Error,
    },
    #[error("stored {what} uses format {found}, but this version reads up to {supported}")]
    NewerFormat {
        what: String,
        found: u32,
        supported: u32,
    },
    #[error("settlement `{0}` does not exist")]
    SettlementNotFound(SettlementId),
    #[error("settlement `{0}` is a favourite; unfavourite it before deleting it")]
    FavouriteCannotBeDeleted(SettlementId),
    #[error("a settlement name cannot be empty")]
    EmptyName,
    #[error("a settlement name can be at most {MAX_NAME_LENGTH} characters")]
    NameTooLong,
    #[error(transparent)]
    Settings(#[from] SettingsError),
    #[error("could not back up the database into {dir}: {reason}")]
    Backup { dir: PathBuf, reason: String },
    #[error("this database has no file location, so it cannot be backed up")]
    NoBackupLocation,
    #[error(
        "Your saved data could not be upgraded for this version: {source}. \
         Nothing was changed: your data is exactly as it was before the update, \
         and a backup made before the upgrade is kept at {backup}. \
         Reinstalling the previous version will open your data as before. \
         Please keep the backup and report this problem."
    )]
    UpgradeFailed {
        #[source]
        source: Box<DbError>,
        backup: PathBuf,
    },
}

pub type DbResult<T> = Result<T, DbError>;

/// A connection to the application database.
pub struct Database {
    conn: Connection,
    /// The database file; `None` for in-memory databases.
    path: Option<PathBuf>,
}

impl Database {
    /// Opens (creating if needed) the database at `path` and migrates it.
    ///
    /// If an existing database needs migrating, a verified backup is written
    /// to the `backups` folder beside it first; if that backup cannot be
    /// made, nothing is migrated. A failed migration leaves the database
    /// unchanged and reports [`DbError::UpgradeFailed`] with the backup.
    pub fn open(path: &Path) -> DbResult<Self> {
        Self::open_with(path, MIGRATIONS)
    }

    pub(crate) fn open_with(path: &Path, migrations: &[Migration]) -> DbResult<Self> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|source| DbError::CreateDir {
                path: dir.to_owned(),
                source,
            })?;
        }
        let mut conn = Connection::open(path)?;
        // WAL lets the GUI and terminal interface use the database at once.
        conn.pragma_update(None, "journal_mode", "WAL")?;
        Self::configure(&conn)?;

        let current = user_version(&conn)?;
        check_not_newer(current, migrations)?;
        let latest = migrations.last().map_or(0, |m| m.version);
        // A brand-new database (version 0) has no user data to protect.
        let backup = if current > 0 && current < latest {
            let kind = BackupKind::PreMigration {
                from: current,
                to: latest,
            };
            let dir = backup::backup_dir_for(path);
            let written = backup::write_backup(&conn, &dir, &kind)
                .map_err(|reason| DbError::Backup { dir, reason })?;
            Some(written)
        } else {
            None
        };
        if let Err(error) = migrate(&mut conn, migrations) {
            return Err(match backup {
                Some(backup) => DbError::UpgradeFailed {
                    source: Box::new(error),
                    backup,
                },
                None => error,
            });
        }
        if let Some(backup) = &backup
            && let Some(dir) = backup.parent()
        {
            backup::prune(dir, BackupKind::PRE_MIGRATION_PREFIX);
        }
        Ok(Database {
            conn,
            path: Some(path.to_owned()),
        })
    }

    /// Backs up the database file at `path` without opening it as an
    /// application database: read-only, no migrations. Used when the
    /// database could not be opened (for example after a failed upgrade) but
    /// an update is about to be installed. Returns `None` when there is no
    /// database file to protect.
    pub fn backup_file(path: &Path, kind: &BackupKind) -> DbResult<Option<PathBuf>> {
        if !path.exists() {
            return Ok(None);
        }
        let conn = Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        let dir = backup::backup_dir_for(path);
        let written =
            backup::write_backup(&conn, &dir, kind).map_err(|reason| DbError::Backup {
                dir: dir.clone(),
                reason,
            })?;
        backup::prune(&dir, kind.prefix());
        Ok(Some(written))
    }

    /// A private in-memory database, for tests and previews.
    pub fn open_in_memory() -> DbResult<Self> {
        let mut conn = Connection::open_in_memory()?;
        Self::configure(&conn)?;
        migrate(&mut conn, MIGRATIONS)?;
        Ok(Database { conn, path: None })
    }

    fn configure(conn: &Connection) -> DbResult<()> {
        conn.busy_timeout(Duration::from_secs(5))?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        Ok(())
    }

    pub fn schema_version(&self) -> DbResult<u32> {
        Ok(user_version(&self.conn)?)
    }

    /// The database file, or `None` for an in-memory database.
    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    /// Writes everything committed so far into the main database file
    /// (checkpointing the write-ahead log). Every change is already durable
    /// once committed; this makes the file on disk complete on its own.
    pub fn flush(&self) -> DbResult<()> {
        // Returns (busy, log frames, checkpointed frames). Another open
        // connection (such as the terminal interface) can keep part of the
        // log busy; the data is still safe in the log.
        self.conn
            .query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |_| Ok(()))?;
        Ok(())
    }

    /// Writes a verified backup into the `backups` folder beside the
    /// database, keeps only the newest few of the same kind, and returns the
    /// backup's path.
    pub fn backup(&self, kind: &BackupKind) -> DbResult<PathBuf> {
        let path = self.path.as_deref().ok_or(DbError::NoBackupLocation)?;
        let dir = backup::backup_dir_for(path);
        let written =
            backup::write_backup(&self.conn, &dir, kind).map_err(|reason| DbError::Backup {
                dir: dir.clone(),
                reason,
            })?;
        backup::prune(&dir, kind.prefix());
        Ok(written)
    }

    // ----- settings ---------------------------------------------------------

    /// Loads settings. Missing or unreadable values fall back to their
    /// defaults individually, so one bad value never loses the others.
    pub fn load_settings(&self) -> DbResult<Settings> {
        let mut stmt = self.conn.prepare("SELECT key, value FROM settings")?;
        let mut map = serde_json::Map::new();
        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;
        for row in rows {
            let (key, value) = row?;
            if let Ok(value) = serde_json::from_str(&value) {
                map.insert(key, value);
            }
        }
        let defaults = serde_json::to_value(Settings::default()).expect("settings serialise");
        let mut merged = serde_json::Map::new();
        for (key, default) in defaults.as_object().expect("settings are an object") {
            let candidate = map.get(key).cloned().unwrap_or_else(|| default.clone());
            // Validate each value on its own against the defaults.
            let mut probe = defaults.clone();
            probe[key] = candidate.clone();
            let value = if serde_json::from_value::<Settings>(probe).is_ok() {
                candidate
            } else {
                default.clone()
            };
            merged.insert(key.clone(), value);
        }
        Ok(serde_json::from_value(serde_json::Value::Object(merged)).expect("validated settings"))
    }

    /// Saves all settings atomically. Changing a disabled setting (the
    /// presentation while the method is Automatic) is refused.
    pub fn save_settings(&mut self, settings: &Settings) -> DbResult<()> {
        settings.check_change_from(&self.load_settings()?)?;
        let value = serde_json::to_value(settings).expect("settings serialise");
        let tx = self.conn.transaction()?;
        for (key, value) in value.as_object().expect("settings are an object") {
            tx.execute(
                "INSERT INTO settings (key, value) VALUES (?1, ?2)
                 ON CONFLICT (key) DO UPDATE SET value = excluded.value",
                params![key, value.to_string()],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    /// Loads the values chosen for content-defined generation settings
    /// (stored under their own key). Unreadable data reads as none chosen,
    /// so every setting takes its default.
    pub fn load_content_settings(&self) -> DbResult<SettingValues> {
        let value: Option<String> = self
            .conn
            .query_row(
                "SELECT value FROM settings WHERE key = ?1",
                params![CONTENT_SETTINGS_KEY],
                |row| row.get(0),
            )
            .optional()?;
        Ok(value
            .and_then(|v| serde_json::from_str(&v).ok())
            .unwrap_or_default())
    }

    /// Saves the values chosen for content-defined generation settings.
    pub fn save_content_settings(&mut self, values: &SettingValues) -> DbResult<()> {
        let json = serde_json::to_string(values).expect("setting values serialise");
        self.conn.execute(
            "INSERT INTO settings (key, value) VALUES (?1, ?2)
             ON CONFLICT (key) DO UPDATE SET value = excluded.value",
            params![CONTENT_SETTINGS_KEY, json],
        )?;
        Ok(())
    }

    /// An editor draft saved under `key` (e.g. `table.<id>`), if any. Drafts
    /// are kept beside settings, under their own keys, until saved or
    /// discarded.
    pub fn load_draft(&self, key: &str) -> DbResult<Option<serde_json::Value>> {
        let value: Option<String> = self
            .conn
            .query_row(
                "SELECT value FROM settings WHERE key = ?1",
                params![format!("{DRAFT_PREFIX}{key}")],
                |row| row.get(0),
            )
            .optional()?;
        Ok(value.and_then(|v| serde_json::from_str(&v).ok()))
    }

    pub fn save_draft(&mut self, key: &str, value: &serde_json::Value) -> DbResult<()> {
        self.conn.execute(
            "INSERT INTO settings (key, value) VALUES (?1, ?2)
             ON CONFLICT (key) DO UPDATE SET value = excluded.value",
            params![format!("{DRAFT_PREFIX}{key}"), value.to_string()],
        )?;
        Ok(())
    }

    pub fn delete_draft(&mut self, key: &str) -> DbResult<()> {
        self.conn.execute(
            "DELETE FROM settings WHERE key = ?1",
            params![format!("{DRAFT_PREFIX}{key}")],
        )?;
        Ok(())
    }

    /// Loads, changes and saves settings in one step.
    pub fn update_settings(&mut self, change: impl FnOnce(&mut Settings)) -> DbResult<Settings> {
        let mut settings = self.load_settings()?;
        change(&mut settings);
        self.save_settings(&settings)?;
        Ok(settings)
    }

    // ----- content overrides ------------------------------------------------

    /// Loads the user's content overrides; none stored means no
    /// customisations.
    pub fn load_overrides(&self) -> DbResult<UserOverrides> {
        let document: Option<String> = self
            .conn
            .query_row(
                "SELECT document FROM content_overrides WHERE id = 1",
                [],
                |row| row.get(0),
            )
            .optional()?;
        let Some(document) = document else {
            return Ok(UserOverrides::default());
        };
        let overrides: UserOverrides =
            serde_json::from_str(&document).map_err(|source| DbError::Corrupt {
                what: "content customisations".into(),
                source,
            })?;
        if overrides.format_version > OVERRIDES_FORMAT_VERSION {
            return Err(DbError::NewerFormat {
                what: "content customisations".into(),
                found: overrides.format_version,
                supported: OVERRIDES_FORMAT_VERSION,
            });
        }
        Ok(overrides)
    }

    pub fn save_overrides(&mut self, overrides: &UserOverrides) -> DbResult<()> {
        let document = serde_json::to_string(overrides).expect("overrides serialise");
        self.conn.execute(
            "INSERT INTO content_overrides (id, document, updated_at) VALUES (1, ?1, ?2)
             ON CONFLICT (id) DO UPDATE SET document = excluded.document,
                                            updated_at = excluded.updated_at",
            params![document, to_unix_millis(now())],
        )?;
        Ok(())
    }

    // ----- settlements ------------------------------------------------------

    /// Saves a newly generated settlement under a user-entered name.
    pub fn create_settlement(
        &mut self,
        name: &str,
        snapshot: SettlementSnapshot,
    ) -> DbResult<Settlement> {
        let name = clean_name(name)?;
        let at = now();
        let summary = SettlementSummary {
            id: SettlementId::new_random(),
            name,
            created_at: at,
            updated_at: at,
            app_version: VERSION.to_owned(),
            favourite: false,
        };
        self.conn.execute(
            "INSERT INTO settlements
                 (id, name, created_at, updated_at, app_version, favourite, snapshot_format, snapshot)
             VALUES (?1, ?2, ?3, ?4, ?5, 0, ?6, ?7)",
            params![
                summary.id.as_str(),
                summary.name,
                to_unix_millis(at),
                to_unix_millis(at),
                summary.app_version,
                SNAPSHOT_FORMAT_VERSION,
                serde_json::to_string(&snapshot).expect("snapshot serialises"),
            ],
        )?;
        Ok(Settlement { summary, snapshot })
    }

    /// All saved settlements, most recently updated first.
    pub fn list_settlements(&self) -> DbResult<Vec<SettlementSummary>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, created_at, updated_at, app_version, favourite
             FROM settlements ORDER BY updated_at DESC, id",
        )?;
        let rows = stmt.query_map([], summary_from_row)?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    pub fn get_settlement(&self, id: &SettlementId) -> DbResult<Settlement> {
        let row = self
            .conn
            .query_row(
                "SELECT id, name, created_at, updated_at, app_version, favourite,
                        snapshot_format, snapshot
                 FROM settlements WHERE id = ?1",
                [id.as_str()],
                |row| {
                    Ok((
                        summary_from_row(row)?,
                        row.get::<_, u32>(6)?,
                        row.get::<_, String>(7)?,
                    ))
                },
            )
            .optional()?;
        let Some((summary, format, json)) = row else {
            return Err(DbError::SettlementNotFound(id.clone()));
        };
        if format > SNAPSHOT_FORMAT_VERSION {
            return Err(DbError::NewerFormat {
                what: format!("settlement `{id}`"),
                found: format,
                supported: SNAPSHOT_FORMAT_VERSION,
            });
        }
        let snapshot = serde_json::from_str(&json).map_err(|source| DbError::Corrupt {
            what: format!("settlement `{id}`"),
            source,
        })?;
        Ok(Settlement { summary, snapshot })
    }

    pub fn rename_settlement(&mut self, id: &SettlementId, name: &str) -> DbResult<()> {
        let name = clean_name(name)?;
        self.update_one(
            id,
            "UPDATE settlements SET name = ?2, updated_at = ?3 WHERE id = ?1",
            params![id.as_str(), name, to_unix_millis(now())],
        )
    }

    pub fn set_favourite(&mut self, id: &SettlementId, favourite: bool) -> DbResult<()> {
        self.update_one(
            id,
            "UPDATE settlements SET favourite = ?2 WHERE id = ?1",
            params![id.as_str(), favourite],
        )
    }

    /// Explicitly replaces a settlement's snapshot after a deliberate edit or
    /// reroll. Records the current application version as the generator.
    pub fn replace_snapshot(
        &mut self,
        id: &SettlementId,
        snapshot: &SettlementSnapshot,
    ) -> DbResult<()> {
        self.update_one(
            id,
            "UPDATE settlements
             SET snapshot = ?2, snapshot_format = ?3, app_version = ?4, updated_at = ?5
             WHERE id = ?1",
            params![
                id.as_str(),
                serde_json::to_string(snapshot).expect("snapshot serialises"),
                SNAPSHOT_FORMAT_VERSION,
                VERSION,
                to_unix_millis(now()),
            ],
        )
    }

    /// Saves a deliberate edit: the new name and revised snapshot together,
    /// in one transaction. Returns the updated settlement.
    pub fn update_settlement(
        &mut self,
        id: &SettlementId,
        name: &str,
        snapshot: &SettlementSnapshot,
    ) -> DbResult<Settlement> {
        let name = clean_name(name)?;
        let tx = self.conn.transaction()?;
        let changed = tx.execute(
            "UPDATE settlements
             SET name = ?2,
                 -- A rename alone keeps the version that generated the snapshot.
                 app_version = CASE WHEN snapshot = ?3 THEN app_version ELSE ?5 END,
                 snapshot = ?3, snapshot_format = ?4, updated_at = ?6
             WHERE id = ?1",
            params![
                id.as_str(),
                name,
                serde_json::to_string(snapshot).expect("snapshot serialises"),
                SNAPSHOT_FORMAT_VERSION,
                VERSION,
                to_unix_millis(now()),
            ],
        )?;
        if changed == 0 {
            return Err(DbError::SettlementNotFound(id.clone()));
        }
        tx.commit()?;
        self.get_settlement(id)
    }

    /// Deletes a settlement. Favourites cannot be deleted: unfavourite first.
    pub fn delete_settlement(&mut self, id: &SettlementId) -> DbResult<()> {
        let tx = self.conn.transaction()?;
        let favourite: Option<bool> = tx
            .query_row(
                "SELECT favourite FROM settlements WHERE id = ?1",
                [id.as_str()],
                |row| row.get(0),
            )
            .optional()?;
        match favourite {
            None => return Err(DbError::SettlementNotFound(id.clone())),
            Some(true) => return Err(DbError::FavouriteCannotBeDeleted(id.clone())),
            Some(false) => {}
        }
        tx.execute("DELETE FROM settlements WHERE id = ?1", [id.as_str()])
            .map_err(|e| favourite_violation(e, id))?;
        tx.commit()?;
        Ok(())
    }

    fn update_one(
        &mut self,
        id: &SettlementId,
        sql: &str,
        params: &[&dyn rusqlite::ToSql],
    ) -> DbResult<()> {
        match self.conn.execute(sql, params)? {
            0 => Err(DbError::SettlementNotFound(id.clone())),
            _ => Ok(()),
        }
    }

    /// Raw connection for tests inside the crate.
    #[cfg(test)]
    pub(crate) fn connection(&self) -> &Connection {
        &self.conn
    }
}

fn clean_name(name: &str) -> DbResult<String> {
    let name = name.trim();
    if name.is_empty() {
        Err(DbError::EmptyName)
    } else if name.chars().count() > MAX_NAME_LENGTH {
        Err(DbError::NameTooLong)
    } else {
        Ok(name.to_owned())
    }
}

fn summary_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<SettlementSummary> {
    Ok(SettlementSummary {
        id: SettlementId::from(row.get::<_, String>(0)?.as_str()),
        name: row.get(1)?,
        created_at: from_unix_millis(row.get(2)?),
        updated_at: from_unix_millis(row.get(3)?),
        app_version: row.get(4)?,
        favourite: row.get(5)?,
    })
}

fn favourite_violation(error: rusqlite::Error, id: &SettlementId) -> DbError {
    if error.to_string().contains(FAVOURITE_TRIGGER_MESSAGE) {
        DbError::FavouriteCannotBeDeleted(id.clone())
    } else {
        DbError::Sqlite(error)
    }
}

fn user_version(conn: &Connection) -> rusqlite::Result<u32> {
    conn.pragma_query_value(None, "user_version", |row| row.get(0))
}

/// Refuses a database whose schema is newer than `migrations` know about.
fn check_not_newer(current: u32, migrations: &[Migration]) -> DbResult<()> {
    let latest = migrations.last().map_or(0, |m| m.version);
    if current > latest {
        return Err(DbError::NewerSchema {
            found: current,
            supported: latest,
        });
    }
    Ok(())
}

/// Applies every migration newer than the database's `user_version`, all in
/// one transaction together with the version bumps, so an upgrade either
/// completes or leaves the database exactly as it was. Foreign keys are
/// checked before committing. Returns the final version.
pub(crate) fn migrate(conn: &mut Connection, migrations: &[Migration]) -> DbResult<u32> {
    let current = user_version(conn)?;
    check_not_newer(current, migrations)?;
    if migrations.iter().all(|m| m.version <= current) {
        return Ok(current);
    }
    // Table rebuilds need enforcement off; it cannot change inside a
    // transaction, so violations are checked explicitly before committing.
    let enforced: bool = conn.pragma_query_value(None, "foreign_keys", |row| row.get(0))?;
    conn.pragma_update(None, "foreign_keys", "OFF")?;
    let outcome = apply_pending(conn, migrations);
    conn.pragma_update(None, "foreign_keys", enforced)?;
    outcome?;
    Ok(user_version(conn)?)
}

fn apply_pending(conn: &mut Connection, migrations: &[Migration]) -> DbResult<()> {
    // IMMEDIATE takes the write lock up front, and the version is re-read
    // under it: if the GUI and the terminal interface open an old database
    // together, one migrates and the other then finds nothing left to do.
    let tx: Transaction<'_> = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let current = user_version(&tx)?;
    check_not_newer(current, migrations)?;
    let pending: Vec<&Migration> = migrations.iter().filter(|m| m.version > current).collect();
    if pending.is_empty() {
        return Ok(());
    }
    for migration in &pending {
        let failed = |source| DbError::Migration {
            version: migration.version,
            name: migration.name,
            source,
        };
        tx.execute_batch(migration.sql).map_err(failed)?;
        tx.pragma_update(None, "user_version", migration.version)
            .map_err(failed)?;
    }
    let violations: i64 =
        tx.query_row("SELECT count(*) FROM pragma_foreign_key_check", [], |row| {
            row.get(0)
        })?;
    if violations > 0 {
        let last = pending.last().expect("pending is not empty");
        return Err(DbError::Migration {
            version: last.version,
            name: last.name,
            source: rusqlite::Error::SqliteFailure(
                rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_CONSTRAINT_FOREIGNKEY),
                Some(format!(
                    "{violations} foreign key violation(s) after migrating"
                )),
            ),
        });
    }
    // Dropping `tx` without committing rolls everything back.
    tx.commit()?;
    Ok(())
}

#[cfg(test)]
mod upgrade_tests;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrations_are_numbered_contiguously_from_one() {
        for (i, m) in MIGRATIONS.iter().enumerate() {
            assert_eq!(m.version as usize, i + 1, "migration {}", m.name);
        }
    }

    #[test]
    fn fresh_database_is_fully_migrated() {
        let db = Database::open_in_memory().unwrap();
        assert_eq!(db.schema_version().unwrap(), latest_schema_version());
    }

    #[test]
    fn migrating_twice_is_a_no_op() {
        let mut conn = Connection::open_in_memory().unwrap();
        assert_eq!(migrate(&mut conn, MIGRATIONS).unwrap(), 1);
        assert_eq!(migrate(&mut conn, MIGRATIONS).unwrap(), 1);
    }

    #[test]
    fn failed_migration_rolls_back_completely() {
        const BROKEN: &[Migration] = &[
            Migration {
                version: 1,
                name: "ok",
                sql: "CREATE TABLE a (x INTEGER);",
            },
            Migration {
                version: 2,
                name: "broken",
                sql: "CREATE TABLE b (x INTEGER); INSERT INTO nowhere VALUES (1);",
            },
        ];
        let mut conn = Connection::open_in_memory().unwrap();
        let err = migrate(&mut conn, BROKEN).unwrap_err();
        assert!(matches!(err, DbError::Migration { version: 2, .. }));
        // Pending migrations share one transaction: nothing was applied.
        assert_eq!(user_version(&conn).unwrap(), 0);
        let a_exists: bool = conn
            .query_row(
                "SELECT EXISTS (SELECT 1 FROM sqlite_master WHERE name = 'a')",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(!a_exists);
        let b_exists: bool = conn
            .query_row(
                "SELECT EXISTS (SELECT 1 FROM sqlite_master WHERE name = 'b')",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(!b_exists);
    }

    #[test]
    fn database_from_a_newer_version_is_refused() {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.pragma_update(None, "user_version", 99).unwrap();
        assert!(matches!(
            migrate(&mut conn, MIGRATIONS),
            Err(DbError::NewerSchema { found: 99, .. })
        ));
    }

    #[test]
    fn trigger_blocks_deleting_favourites_even_with_raw_sql() {
        let mut db = Database::open_in_memory().unwrap();
        let s = db
            .create_settlement("Kept", SettlementSnapshot::default())
            .unwrap();
        db.set_favourite(&s.summary.id, true).unwrap();
        let err = db
            .connection()
            .execute(
                "DELETE FROM settlements WHERE id = ?1",
                [s.summary.id.as_str()],
            )
            .unwrap_err();
        assert!(matches!(
            favourite_violation(err, &s.summary.id),
            DbError::FavouriteCannotBeDeleted(_)
        ));
    }
}
