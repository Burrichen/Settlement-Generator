//! Simulated upgrade paths: a database written by this release, holding every
//! kind of user data, is opened by a "future" build with an extra schema
//! migration. User data must survive a successful upgrade untouched, and a
//! failed upgrade must leave it exactly as it was, with a backup kept.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

use serde_json::{Value, json};

use super::*;
use crate::backup::{BackupKind, KEEP_PER_KIND, list_backups};
use crate::dice::RandomFaces;
use crate::engine::Selections;
use crate::ids::{ResultId, TableId, TagId};
use crate::maps::{MapGenerationOptions, MapStatus};
use crate::service::AppService;
use crate::settings::{GenerationMethod, GenerationPresentation, Theme};

/// The schema this release creates.
const CURRENT: u32 = MIGRATIONS.len() as u32;
/// The schema of the simulated next release.
const NEXT: u32 = CURRENT + 1;

/// A realistic future migration: rebuilds `settlements` with a new column
/// (the way SQLite schema changes are usually made), recreating its index
/// and favourite-protection trigger, and adds a column to `settings`.
const SIMULATED_V2: Migration = Migration {
    version: NEXT,
    name: "simulated_rebuild",
    sql: "
        CREATE TABLE settlements_new (
            id              TEXT PRIMARY KEY NOT NULL,
            name            TEXT NOT NULL CHECK (length(trim(name)) > 0),
            created_at      INTEGER NOT NULL,
            updated_at      INTEGER NOT NULL,
            app_version     TEXT NOT NULL,
            favourite       INTEGER NOT NULL DEFAULT 0 CHECK (favourite IN (0, 1)),
            snapshot_format INTEGER NOT NULL,
            snapshot        TEXT NOT NULL,
            notes           TEXT NOT NULL DEFAULT ''
        ) STRICT;
        INSERT INTO settlements_new
            (id, name, created_at, updated_at, app_version, favourite, snapshot_format, snapshot)
            SELECT id, name, created_at, updated_at, app_version, favourite, snapshot_format, snapshot
            FROM settlements;
        DROP TABLE settlements;
        ALTER TABLE settlements_new RENAME TO settlements;
        CREATE INDEX settlements_by_updated ON settlements (updated_at DESC);
        CREATE TRIGGER settlements_keep_favourites
        BEFORE DELETE ON settlements
        WHEN OLD.favourite = 1
        BEGIN
            SELECT RAISE(ABORT, 'favourite settlements cannot be deleted');
        END;
        ALTER TABLE settings ADD COLUMN updated_at INTEGER;
    ",
};

/// A future migration that fails part-way, after already dropping a table.
const BROKEN_V2: Migration = Migration {
    version: NEXT,
    name: "broken",
    sql: "
        CREATE TABLE settlements_new (id TEXT PRIMARY KEY NOT NULL) STRICT;
        DROP TABLE settlements;
        INSERT INTO no_such_table VALUES (1);
    ",
};

fn this_release() -> Vec<Migration> {
    MIGRATIONS
        .iter()
        .map(|m| Migration {
            version: m.version,
            name: m.name,
            sql: m.sql,
        })
        .collect()
}

fn future_release(next: Migration) -> Vec<Migration> {
    let mut all = this_release();
    all.push(next);
    all
}

struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Self {
        static NEXT: AtomicU32 = AtomicU32::new(0);
        let dir = std::env::temp_dir().join(format!(
            "settlement-upgrade-test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&dir);
        TempDir(dir)
    }

    fn db(&self) -> PathBuf {
        self.0.join("SettlementGenerator").join("data.sqlite3")
    }

    fn backups(&self) -> PathBuf {
        self.0.join("SettlementGenerator").join("backups")
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn generate(s: &mut AppService, name: &str, size: &str, seed: u64) -> SettlementId {
    let mut active = s.start_generation().unwrap();
    active
        .session
        .choose(0, ResultId::new("kurovia"), &mut RandomFaces::seeded(seed))
        .unwrap();
    let mut sizes = Selections::new();
    sizes.insert("table.size".into(), size.into());
    active
        .session
        .run_automatic(&sizes, &mut RandomFaces::seeded(seed + 1))
        .unwrap();
    s.save_new(&active, name).unwrap().id
}

/// Writes every kind of user data through the real application service:
/// non-default settings, a table override, a tag override, a custom table, a
/// custom tag, and saved settlements including a favourite. Returns the
/// favourite's ID.
fn populate(path: &Path) -> SettlementId {
    let mut s = AppService::new(Database::open_with(path, &this_release()).unwrap());

    let mut settings = s.settings().unwrap();
    settings.generation_method = GenerationMethod::Manual;
    s.save_settings(&settings).unwrap();
    settings.generation_presentation = GenerationPresentation::OneTable;
    settings.god_mode = true;
    settings.tag_view = true;
    settings.theme = Theme::OldTimeyDark;
    s.save_settings(&settings).unwrap();

    let mut table = s.table_detail(&TableId::new("table.age")).unwrap().table;
    table.name = "Age (house rules)".into();
    s.save_table(table).unwrap();
    let mut tag = s.tag_detail(&TagId::new("age.old")).unwrap().tag;
    tag.name = "Venerable".into();
    s.save_tag(tag).unwrap();
    let mut custom_table = AppService::new_table();
    custom_table.name = "My custom table".into();
    s.save_table(custom_table).unwrap();
    let mut custom_tag = AppService::new_tag();
    custom_tag.name = "My custom tag".into();
    s.save_tag(custom_tag).unwrap();

    let favourite = generate(&mut s, "Riverside", "town", 1);
    s.saved_set_favourite(&favourite, true).unwrap();
    s.maps_generate(&favourite, &MapGenerationOptions::default())
        .unwrap();
    generate(&mut s, "Hillfort", "village", 7);
    favourite
}

/// Everything the user owns, in comparable form.
fn user_data(db: &Database) -> Value {
    let settlements: Vec<Value> = db
        .list_settlements()
        .unwrap()
        .iter()
        .map(|s| serde_json::to_value(db.get_settlement(&s.id).unwrap()).unwrap())
        .collect();
    json!({
        "settings": serde_json::to_value(db.load_settings().unwrap()).unwrap(),
        "overrides": serde_json::to_value(db.load_overrides().unwrap()).unwrap(),
        "settlements": settlements,
    })
}

/// Every saved map set, in comparable form.
fn map_data(db: &Database) -> Value {
    db.list_settlements()
        .unwrap()
        .iter()
        .filter_map(|s| db.load_map_set(&s.id).unwrap())
        .map(|set| serde_json::to_value(set).unwrap())
        .collect()
}

#[test]
fn successful_upgrade_preserves_all_user_data_and_keeps_a_backup() {
    let tmp = TempDir::new();
    let favourite = populate(&tmp.db());
    let old = Database::open_with(&tmp.db(), &this_release()).unwrap();
    let before = user_data(&old);
    let maps_before = map_data(&old);
    drop(old);
    assert_eq!(
        maps_before.as_array().unwrap().len(),
        1,
        "the fixture has maps"
    );

    // Sanity: the fixture really contains each kind of data.
    let overrides = &before["overrides"].to_string();
    for expected in [
        "Age (house rules)",
        "Venerable",
        "My custom table",
        "My custom tag",
    ] {
        assert!(overrides.contains(expected), "fixture lacks {expected}");
    }
    assert_eq!(before["settlements"].as_array().unwrap().len(), 2);
    assert_eq!(before["settings"]["theme"], "old_timey_dark");

    let mut upgraded = Database::open_with(&tmp.db(), &future_release(SIMULATED_V2)).unwrap();
    assert_eq!(upgraded.schema_version().unwrap(), NEXT);
    assert_eq!(user_data(&upgraded), before);
    assert_eq!(map_data(&upgraded), maps_before);

    // The rebuilt table still protects favourites.
    assert!(matches!(
        upgraded.delete_settlement(&favourite),
        Err(DbError::FavouriteCannotBeDeleted(_))
    ));

    // A verified pre-migration backup holds the data at the old schema.
    let backups = list_backups(&tmp.backups(), BackupKind::PRE_MIGRATION_PREFIX);
    assert_eq!(backups.len(), 1);
    let name = backups[0]
        .file_name()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    let expected = format!("-schema{CURRENT}-to-schema{NEXT}.sqlite3");
    assert!(name.ends_with(&expected), "{name}");
    let restored = Database::open_with(&backups[0], &this_release()).unwrap();
    assert_eq!(restored.schema_version().unwrap(), CURRENT);
    assert_eq!(user_data(&restored), before);
}

#[test]
fn failed_upgrade_leaves_data_untouched_and_surfaces_the_backup() {
    let tmp = TempDir::new();
    populate(&tmp.db());
    let before = user_data(&Database::open_with(&tmp.db(), &this_release()).unwrap());

    let error = match Database::open_with(&tmp.db(), &future_release(BROKEN_V2)) {
        Err(e) => e,
        Ok(_) => panic!("the broken migration must fail"),
    };
    let DbError::UpgradeFailed { source, backup } = &error else {
        panic!("expected UpgradeFailed, got {error}");
    };
    assert!(matches!(**source, DbError::Migration { version: NEXT, .. }));
    assert!(backup.exists(), "backup is kept after a failure");
    let message = error.to_string();
    assert!(message.contains("Nothing was changed"), "{message}");
    assert!(message.contains(&backup.display().to_string()), "{message}");

    // The previous release opens the untouched data as before.
    let previous = Database::open_with(&tmp.db(), &this_release()).unwrap();
    assert_eq!(previous.schema_version().unwrap(), CURRENT);
    assert_eq!(user_data(&previous), before);
    drop(previous);

    // A fixed release then upgrades normally; the failed attempt's backup
    // is still there alongside the new one.
    let upgraded = Database::open_with(&tmp.db(), &future_release(SIMULATED_V2)).unwrap();
    assert_eq!(user_data(&upgraded), before);
    assert!(backup.exists());
    assert_eq!(
        list_backups(&tmp.backups(), BackupKind::PRE_MIGRATION_PREFIX).len(),
        2
    );
}

#[test]
fn migration_is_refused_when_no_backup_can_be_made() {
    let tmp = TempDir::new();
    populate(&tmp.db());
    let before = user_data(&Database::open_with(&tmp.db(), &this_release()).unwrap());
    // A file where the backup folder should be makes backing up impossible.
    std::fs::write(tmp.backups(), b"not a folder").unwrap();

    let error = Database::open_with(&tmp.db(), &future_release(SIMULATED_V2))
        .err()
        .expect("must refuse to migrate without a backup");
    assert!(matches!(error, DbError::Backup { .. }), "{error}");
    let previous = Database::open_with(&tmp.db(), &this_release()).unwrap();
    assert_eq!(previous.schema_version().unwrap(), CURRENT);
    assert_eq!(user_data(&previous), before);
}

#[test]
fn up_to_date_and_new_databases_need_no_backup() {
    let tmp = TempDir::new();
    Database::open(&tmp.db()).unwrap(); // new: nothing to protect
    Database::open(&tmp.db()).unwrap(); // already current
    assert!(!tmp.backups().exists());
}

#[test]
fn database_from_a_newer_release_is_refused_without_changes() {
    let tmp = TempDir::new();
    populate(&tmp.db());
    let before = user_data(&Database::open_with(&tmp.db(), &this_release()).unwrap());
    drop(Database::open_with(&tmp.db(), &future_release(SIMULATED_V2)).unwrap());

    // Downgrading: this release refuses the newer schema and touches nothing.
    let error = Database::open_with(&tmp.db(), &this_release())
        .err()
        .expect("newer schema is refused");
    assert!(matches!(
        error,
        DbError::NewerSchema {
            found: NEXT,
            supported: CURRENT
        }
    ));
    let current = Database::open_with(&tmp.db(), &future_release(SIMULATED_V2)).unwrap();
    assert_eq!(user_data(&current), before);
}

#[test]
fn pre_update_backup_is_complete_and_old_ones_are_pruned() {
    let tmp = TempDir::new();
    populate(&tmp.db());
    let before = user_data(&Database::open(&tmp.db()).unwrap());
    let mut service = AppService::new(Database::open(&tmp.db()).unwrap());

    let mut last = PathBuf::new();
    for i in 0..KEEP_PER_KIND + 2 {
        last = service
            .prepare_for_update(&format!("0.1.{}", i + 1))
            .unwrap();
    }
    assert!(last.starts_with(tmp.backups()));
    let name = last.file_name().unwrap().to_string_lossy().into_owned();
    assert!(
        name.starts_with(BackupKind::PRE_UPDATE_PREFIX)
            && name.contains(&format!("-v{VERSION}-to-v0.1.{}", KEEP_PER_KIND + 2)),
        "{name}"
    );
    let kept = list_backups(&tmp.backups(), BackupKind::PRE_UPDATE_PREFIX);
    assert_eq!(kept.len(), KEEP_PER_KIND);
    assert_eq!(kept.last(), Some(&last));

    // The backup alone (without the live database's log) holds everything.
    let restored = Database::open_with(&last, &this_release()).unwrap();
    assert_eq!(user_data(&restored), before);
}

#[test]
fn in_memory_databases_cannot_be_backed_up() {
    let db = Database::open_in_memory().unwrap();
    assert!(matches!(
        db.backup(&BackupKind::PreUpdate {
            from: "0.1.0".into(),
            to: "0.1.1".into()
        }),
        Err(DbError::NoBackupLocation)
    ));
}

#[test]
fn two_processes_upgrading_at_once_both_succeed() {
    // The GUI and the terminal interface may start together after an update.
    let tmp = TempDir::new();
    populate(&tmp.db());
    let before = user_data(&Database::open_with(&tmp.db(), &this_release()).unwrap());

    let path = tmp.db();
    let handles: Vec<_> = (0..2)
        .map(|_| {
            let path = path.clone();
            std::thread::spawn(move || {
                Database::open_with(&path, &future_release(SIMULATED_V2)).map(|db| user_data(&db))
            })
        })
        .collect();
    for handle in handles {
        let data = handle.join().unwrap().expect("each opener succeeds");
        assert_eq!(data, before);
    }
    let db = Database::open_with(&tmp.db(), &future_release(SIMULATED_V2)).unwrap();
    assert_eq!(db.schema_version().unwrap(), NEXT);
}

#[test]
fn a_database_that_cannot_be_opened_can_still_be_backed_up_for_an_update() {
    let tmp = TempDir::new();
    populate(&tmp.db());
    drop(Database::open_with(&tmp.db(), &future_release(SIMULATED_V2)).unwrap());
    // This release cannot open the newer database...
    assert!(Database::open_with(&tmp.db(), &this_release()).is_err());

    // ...but can back up the file before installing an update that can.
    let kind = BackupKind::PreUpdate {
        from: "0.1.0".into(),
        to: "0.2.0".into(),
    };
    let backup = Database::backup_file(&tmp.db(), &kind)
        .unwrap()
        .expect("a backup");
    let restored = Database::open_with(&backup, &future_release(SIMULATED_V2)).unwrap();
    assert_eq!(restored.schema_version().unwrap(), NEXT);

    // No database yet: nothing to protect.
    let empty = TempDir::new();
    assert!(Database::backup_file(&empty.db(), &kind).unwrap().is_none());
}

/// The real upgrade every v0.1.5 user makes: schema 1 gains map storage.
#[test]
fn a_database_from_before_maps_gains_map_storage_without_changing_user_data() {
    let tmp = TempDir::new();
    let before_maps = &this_release()[..1];
    {
        let mut s = AppService::new(Database::open_with(&tmp.db(), before_maps).unwrap());
        generate(&mut s, "Riverside", "town", 1);
        generate(&mut s, "Highmarket", "city", 3);
    }
    let before = user_data(&Database::open_with(&tmp.db(), before_maps).unwrap());

    let db = Database::open(&tmp.db()).unwrap();
    assert_eq!(db.schema_version().unwrap(), CURRENT);
    assert_eq!(user_data(&db), before);
    let backups = list_backups(&tmp.backups(), BackupKind::PRE_MIGRATION_PREFIX);
    assert_eq!(backups.len(), 1);
    let name = backups[0]
        .file_name()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    assert!(
        name.ends_with(&format!("-schema1-to-schema{CURRENT}.sqlite3")),
        "{name}"
    );

    // Settlements saved before maps existed have none, and can now be mapped.
    let mut s = AppService::new(db);
    for summary in s.saved_list().unwrap() {
        let view = s.maps_get(&summary.id).unwrap();
        assert_eq!(view.status, MapStatus::NotGenerated);
        let view = s
            .maps_generate(&summary.id, &MapGenerationOptions::default())
            .unwrap();
        assert_eq!(view.status, MapStatus::Current);
        let set = view.maps.unwrap();
        assert!(!set.maps.is_empty());
        // And their buildings can be opened, which stores their details.
        let map = &set.maps[0];
        let building = &map.geometry.buildings[0].id;
        s.maps_building(&summary.id, &map.id, building).unwrap();
        assert_eq!(s.maps_get(&summary.id).unwrap().buildings.len(), 1);
    }
}

/// A development database that already has map storage (schema 2) gains
/// building details, and keeps its maps exactly.
#[test]
fn a_database_with_maps_gains_building_details_and_keeps_its_maps() {
    let tmp = TempDir::new();
    let with_maps = &this_release()[..2];
    let id = {
        let mut s = AppService::new(Database::open_with(&tmp.db(), with_maps).unwrap());
        let id = generate(&mut s, "Riverside", "town", 1);
        // Saved as a schema-2 build saved maps: the map set alone.
        let mut db = Database::open_with(&tmp.db(), with_maps).unwrap();
        let settlement = db.get_settlement(&id).unwrap();
        let set = crate::maps::generate_map_set(
            &crate::maps::BuiltinEngine,
            &settlement,
            None,
            &MapGenerationOptions::default(),
        )
        .unwrap();
        db.save_map_set(&set).unwrap();
        id
    };
    let before = Database::open_with(&tmp.db(), with_maps)
        .unwrap()
        .load_map_set(&id)
        .unwrap()
        .unwrap();

    let mut s = AppService::new(Database::open(&tmp.db()).unwrap());
    let view = s.maps_get(&id).unwrap();
    assert_eq!(
        view.maps.as_ref(),
        Some(&before),
        "maps are read back exactly"
    );
    assert!(
        view.buildings.is_empty(),
        "no building is generated by upgrading"
    );
    let map = &before.maps[0];
    let opened = s
        .maps_building(&id, &map.id, &map.geometry.buildings[0].id)
        .unwrap();
    assert_eq!(opened.version, 1);
}

/// A database with building details (schema 3) gains the Notable Location
/// directory: its settlements' own locations are listed, unplaced, and can
/// be placed.
#[test]
fn a_database_with_building_details_gains_the_location_directory() {
    let tmp = TempDir::new();
    let before_directory = &this_release()[..3];
    let id = {
        let mut s = AppService::new(Database::open_with(&tmp.db(), before_directory).unwrap());
        let id = generate(&mut s, "Riverside", "town", 1);
        let mut db = Database::open_with(&tmp.db(), before_directory).unwrap();
        let settlement = db.get_settlement(&id).unwrap();
        let set = crate::maps::generate_map_set(
            &crate::maps::BuiltinEngine,
            &settlement,
            None,
            &MapGenerationOptions::default(),
        )
        .unwrap();
        db.save_map_set(&set).unwrap();
        id
    };

    let mut s = AppService::new(Database::open(&tmp.db()).unwrap());
    let view = s.maps_get(&id).unwrap();
    assert!(!view.locations.is_empty(), "the settlement's own locations");
    assert!(view.locations.iter().all(|l| l.placement.is_none()));
    let map = &view.maps.as_ref().unwrap().maps[0];
    let target = crate::maps::PlaceOn {
        map_id: map.id.clone(),
        building_id: map.geometry.buildings[0].id.clone(),
    };
    let placed = s
        .maps_location_assign(&id, &view.locations[0].id, &target, Default::default())
        .unwrap();
    assert_eq!(placed.locations[0].marker, Some(1));
}

/// A database with the location directory (schema 4) gains district names,
/// points of interest and saved views, keeping its placements and numbers.
#[test]
fn a_database_with_the_directory_gains_districts_points_and_views() {
    let tmp = TempDir::new();
    let before = &this_release()[..4];
    let (id, placed) = {
        let mut s = AppService::new(Database::open_with(&tmp.db(), before).unwrap());
        let id = generate(&mut s, "Riverside", "town", 1);
        let mut db = Database::open_with(&tmp.db(), before).unwrap();
        let settlement = db.get_settlement(&id).unwrap();
        let set = crate::maps::generate_map_set(
            &crate::maps::BuiltinEngine,
            &settlement,
            None,
            &MapGenerationOptions::default(),
        )
        .unwrap();
        db.save_map_set(&set).unwrap();
        let map = &set.maps[0];
        let location = settlement.snapshot.notable_locations().unwrap().locations[0]
            .id
            .clone();
        // Stored as a schema-4 build stored a placement: one location row
        // and the sequences.
        let record = serde_json::json!({
            "id": location,
            "origin": "settlement",
            "version": 1,
            "marker": 1,
            "assignment": {
                "map_id": map.id,
                "building_id": map.geometry.buildings[0].id,
                "assigned_at": "2026-10-24T12:00:00Z"
            }
        });
        db.conn
            .execute(
                "INSERT INTO settlement_locations
                     (settlement_id, location_id, map_format, building_id, document)
                 VALUES (?1, ?2, 1, ?3, ?4)",
                rusqlite::params![
                    id.as_str(),
                    location,
                    map.geometry.buildings[0].id.as_str(),
                    record.to_string()
                ],
            )
            .unwrap();
        db.conn
            .execute(
                "INSERT INTO settlement_location_sequences VALUES (?1, 2, 1)",
                [id.as_str()],
            )
            .unwrap();
        (id, location)
    };

    let mut s = AppService::new(Database::open(&tmp.db()).unwrap());
    let view = s.maps_get(&id).unwrap();
    let entry = view.locations.iter().find(|l| l.id == placed).unwrap();
    assert_eq!(entry.marker, Some(1));
    assert!(view.pois.is_empty() && view.views.is_empty());
    let map = &view.maps.as_ref().unwrap().maps[0];
    let added = s
        .maps_poi_add(
            &id,
            &map.id,
            map.geometry.buildings[1].centroid,
            "Well".into(),
            String::new(),
        )
        .unwrap();
    assert_eq!(added.pois[0].number, 2, "continues the existing sequence");
}
