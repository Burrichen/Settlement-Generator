//! Settings, content customisations and saved settlements in SQLite.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};

use serde_json::json;
use settlement_core::VERSION;
use settlement_core::content::ContentSet;
use settlement_core::db::{Database, DbError};
use settlement_core::dice::ScriptedFaces;
use settlement_core::engine::Generator;
use settlement_core::layers::{ContentLayers, UserOverrides};
use settlement_core::settings::{GenerationMethod, GenerationPresentation, Settings, Theme};
use settlement_core::settlements::{SettlementId, SettlementSnapshot};

struct TempDb {
    dir: PathBuf,
}

impl TempDb {
    fn new() -> Self {
        static NEXT: AtomicU32 = AtomicU32::new(0);
        let dir = std::env::temp_dir().join(format!(
            "settlement-db-test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        TempDb { dir }
    }

    fn path(&self) -> PathBuf {
        self.dir.join("data").join("test.sqlite3")
    }

    fn open(&self) -> Database {
        Database::open(&self.path()).expect("open database")
    }
}

impl Drop for TempDb {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn content() -> ContentSet {
    serde_json::from_value(json!({
        "tables": [{ "id": "t.econ", "name": "Economy", "dice": "1d12", "results": [
            { "id": "poor", "name": "Poor", "range": [1, 8] },
            { "id": "rich", "name": "Prosperous", "range": [9, 12], "tags": ["g.rich"] }
        ]}],
        "tags": [{ "id": "g.rich", "name": "Rich" }]
    }))
    .unwrap()
}

fn generate(c: &ContentSet, face: u32) -> SettlementSnapshot {
    Generator::new(c)
        .unwrap()
        .run(&["t.econ".into()], &mut ScriptedFaces::new([face]))
        .unwrap()
        .into()
}

// ----- database file ---------------------------------------------------------

#[test]
fn creates_the_directory_and_reopens_existing_data() {
    let tmp = TempDb::new();
    {
        let mut db = tmp.open();
        db.update_settings(|s| s.tag_view = true).unwrap();
    }
    assert!(tmp.path().exists());
    assert!(tmp.open().load_settings().unwrap().tag_view);
}

#[test]
fn two_connections_share_data() {
    // The GUI and the terminal interface may be open at the same time.
    let tmp = TempDb::new();
    let mut gui = tmp.open();
    let tui = tmp.open();
    let saved = gui
        .create_settlement("Shared", generate(&content(), 3))
        .unwrap();
    assert_eq!(tui.get_settlement(&saved.summary.id).unwrap(), saved);
}

// ----- settings --------------------------------------------------------------

#[test]
fn settings_default_until_saved() {
    let db = Database::open_in_memory().unwrap();
    let s = db.load_settings().unwrap();
    assert_eq!(s, Settings::default());
    assert_eq!(s.theme, Theme::ModernDark);
}

#[test]
fn settings_round_trip() {
    let mut db = Database::open_in_memory().unwrap();
    let wanted = Settings {
        generation_method: GenerationMethod::Manual,
        generation_presentation: GenerationPresentation::OneTable,
        god_mode: true,
        tag_view: true,
        theme: Theme::OldTimeyDark,
        bare_minimum_buildings: true,
        roll_details: true,
    };
    db.save_settings(&wanted).unwrap();
    assert_eq!(db.load_settings().unwrap(), wanted);
}

#[test]
fn presentation_survives_switching_to_automatic_and_back() {
    let tmp = TempDb::new();
    let mut db = tmp.open();
    db.update_settings(|s| {
        s.generation_method = GenerationMethod::Manual;
        s.generation_presentation = GenerationPresentation::OneTable;
    })
    .unwrap();
    let auto = db
        .update_settings(|s| s.generation_method = GenerationMethod::Automatic)
        .unwrap();
    assert_eq!(auto.active_presentation(), None);
    assert!(!auto.presentation_applies());

    let reloaded = tmp.open().load_settings().unwrap();
    assert_eq!(
        reloaded.generation_presentation,
        GenerationPresentation::OneTable
    );

    let manual = db
        .update_settings(|s| s.generation_method = GenerationMethod::Manual)
        .unwrap();
    assert_eq!(
        manual.active_presentation(),
        Some(GenerationPresentation::OneTable)
    );
}

#[test]
fn an_unreadable_setting_falls_back_alone() {
    let tmp = TempDb::new();
    {
        let mut db = tmp.open();
        db.update_settings(|s| {
            s.tag_view = true;
            s.theme = Theme::OldTimeyDark;
        })
        .unwrap();
    }
    // Simulate a value written by some future version.
    let conn = rusqlite::Connection::open(tmp.path()).unwrap();
    conn.execute(
        "UPDATE settings SET value = '\"neon\"' WHERE key = 'theme'",
        [],
    )
    .unwrap();
    drop(conn);
    let s = tmp.open().load_settings().unwrap();
    assert_eq!(s.theme, Theme::ModernDark);
    assert!(s.tag_view, "other settings are unaffected");
}

// ----- content overrides -----------------------------------------------------

#[test]
fn content_customisations_persist() {
    let tmp = TempDb::new();
    let mut db = tmp.open();
    assert_eq!(db.load_overrides().unwrap(), UserOverrides::default());

    let mut layers = ContentLayers::new(content(), db.load_overrides().unwrap());
    layers.rename_table(&"t.econ".into(), "Wealth").unwrap();
    db.save_overrides(layers.overrides()).unwrap();

    let reloaded = ContentLayers::new(content(), tmp.open().load_overrides().unwrap());
    assert_eq!(reloaded.merged().content.tables[0].name, "Wealth");
}

// ----- settlements -----------------------------------------------------------

#[test]
fn saving_a_settlement_records_its_metadata() {
    let mut db = Database::open_in_memory().unwrap();
    let saved = db
        .create_settlement("  Riverside  ", generate(&content(), 9))
        .unwrap();
    let s = &saved.summary;
    assert_eq!(s.name, "Riverside");
    assert_eq!(s.app_version, VERSION);
    assert_eq!(s.created_at, s.updated_at);
    assert!(!s.favourite);
    assert!(s.id.as_str().starts_with("stl-"));

    let loaded = db.get_settlement(&s.id).unwrap();
    assert_eq!(loaded, saved);
    let record = loaded.snapshot.log.table_rolls().next().unwrap();
    assert_eq!(record.result.name, "Prosperous");
    assert_eq!(record.accepted().unwrap().roll.dice[0].value, 9);
    assert_eq!(loaded.snapshot.log.tags[0].name, "Rich");
}

#[test]
fn names_are_required() {
    let mut db = Database::open_in_memory().unwrap();
    assert!(matches!(
        db.create_settlement("   ", SettlementSnapshot::default()),
        Err(DbError::EmptyName)
    ));
    let s = db
        .create_settlement("A", SettlementSnapshot::default())
        .unwrap();
    assert!(matches!(
        db.rename_settlement(&s.summary.id, ""),
        Err(DbError::EmptyName)
    ));
}

#[test]
fn snapshots_do_not_change_when_content_changes_later() {
    let tmp = TempDb::new();
    let mut db = tmp.open();
    let saved = db
        .create_settlement("Before", generate(&content(), 10))
        .unwrap();

    // The user renames everything and changes the dice afterwards.
    let mut layers = ContentLayers::new(content(), UserOverrides::default());
    layers.rename_table(&"t.econ".into(), "Renamed").unwrap();
    layers
        .rename_result(&"t.econ".into(), &"rich".into(), "Renamed result")
        .unwrap();
    layers.rename_tag(&"g.rich".into(), "Renamed tag").unwrap();
    layers.set_table_dice(&"t.econ".into(), "2d6").unwrap();
    db.save_overrides(layers.overrides()).unwrap();

    let loaded = tmp.open().get_settlement(&saved.summary.id).unwrap();
    assert_eq!(loaded, saved);
    let record = loaded.snapshot.log.table_rolls().next().unwrap();
    assert_eq!(
        record.to_string(),
        "table: Economy\ndice: 1d12\nroll: 10\nresult: Prosperous"
    );
    assert_eq!(record.tags[0].name, "Rich");
}

#[test]
fn deliberate_reroll_replaces_the_snapshot_explicitly() {
    let mut db = Database::open_in_memory().unwrap();
    let saved = db
        .create_settlement("Town", generate(&content(), 1))
        .unwrap();
    std::thread::sleep(std::time::Duration::from_millis(5));
    let rerolled = generate(&content(), 12);
    db.replace_snapshot(&saved.summary.id, &rerolled).unwrap();

    let loaded = db.get_settlement(&saved.summary.id).unwrap();
    assert_eq!(loaded.snapshot, rerolled);
    assert_eq!(loaded.summary.created_at, saved.summary.created_at);
    assert!(loaded.summary.updated_at > saved.summary.updated_at);
    assert_eq!(loaded.summary.name, "Town");
}

#[test]
fn renaming_updates_the_edit_time() {
    let mut db = Database::open_in_memory().unwrap();
    let saved = db
        .create_settlement("Old", SettlementSnapshot::default())
        .unwrap();
    std::thread::sleep(std::time::Duration::from_millis(5));
    db.rename_settlement(&saved.summary.id, "New").unwrap();
    let loaded = db.get_settlement(&saved.summary.id).unwrap();
    assert_eq!(loaded.summary.name, "New");
    assert!(loaded.summary.updated_at > saved.summary.updated_at);
    assert_eq!(loaded.snapshot, saved.snapshot);
}

#[test]
fn listing_is_most_recently_updated_first() {
    let mut db = Database::open_in_memory().unwrap();
    let a = db
        .create_settlement("A", SettlementSnapshot::default())
        .unwrap();
    std::thread::sleep(std::time::Duration::from_millis(5));
    db.create_settlement("B", SettlementSnapshot::default())
        .unwrap();
    std::thread::sleep(std::time::Duration::from_millis(5));
    db.rename_settlement(&a.summary.id, "A2").unwrap();
    let names: Vec<_> = db
        .list_settlements()
        .unwrap()
        .into_iter()
        .map(|s| s.name)
        .collect();
    assert_eq!(names, ["A2", "B"]);
}

#[test]
fn favourites_cannot_be_deleted_until_unfavourited() {
    let mut db = Database::open_in_memory().unwrap();
    let id = db
        .create_settlement("Keep", SettlementSnapshot::default())
        .unwrap()
        .summary
        .id;
    db.set_favourite(&id, true).unwrap();
    assert!(db.get_settlement(&id).unwrap().summary.favourite);

    let err = db.delete_settlement(&id).unwrap_err();
    assert!(matches!(err, DbError::FavouriteCannotBeDeleted(_)));
    assert_eq!(
        err.to_string(),
        format!("settlement `{id}` is a favourite; unfavourite it before deleting it")
    );
    assert!(db.get_settlement(&id).is_ok(), "still there");

    db.set_favourite(&id, false).unwrap();
    db.delete_settlement(&id).unwrap();
    assert!(matches!(
        db.get_settlement(&id),
        Err(DbError::SettlementNotFound(_))
    ));
}

#[test]
fn missing_settlements_are_reported() {
    let mut db = Database::open_in_memory().unwrap();
    let ghost = SettlementId::from("stl-none");
    assert!(matches!(
        db.delete_settlement(&ghost),
        Err(DbError::SettlementNotFound(_))
    ));
    assert!(matches!(
        db.set_favourite(&ghost, true),
        Err(DbError::SettlementNotFound(_))
    ));
    assert!(matches!(
        db.replace_snapshot(&ghost, &SettlementSnapshot::default()),
        Err(DbError::SettlementNotFound(_))
    ));
}

#[test]
fn a_damaged_or_newer_snapshot_is_an_error_for_that_settlement_only() {
    let tmp = TempDb::new();
    let mut db = tmp.open();
    let damaged = db
        .create_settlement("Damaged", SettlementSnapshot::default())
        .unwrap()
        .summary
        .id;
    let future = db
        .create_settlement("Future", SettlementSnapshot::default())
        .unwrap()
        .summary
        .id;
    let fine = db
        .create_settlement("Fine", SettlementSnapshot::default())
        .unwrap()
        .summary
        .id;

    let conn = rusqlite::Connection::open(tmp.path()).unwrap();
    conn.execute(
        "UPDATE settlements SET snapshot = '{oops' WHERE id = ?1",
        [damaged.as_str()],
    )
    .unwrap();
    conn.execute(
        "UPDATE settlements SET snapshot_format = 99 WHERE id = ?1",
        [future.as_str()],
    )
    .unwrap();
    drop(conn);

    assert!(matches!(
        db.get_settlement(&damaged),
        Err(DbError::Corrupt { .. })
    ));
    assert!(matches!(
        db.get_settlement(&future),
        Err(DbError::NewerFormat { found: 99, .. })
    ));
    assert!(db.get_settlement(&fine).is_ok());
    assert_eq!(db.list_settlements().unwrap().len(), 3);
}

#[test]
fn presentation_cannot_be_changed_while_automatic() {
    use settlement_core::settings::SettingsError;
    let mut db = Database::open_in_memory().unwrap();
    db.update_settings(|s| {
        s.generation_method = GenerationMethod::Manual;
        s.generation_presentation = GenerationPresentation::OneTable;
    })
    .unwrap();
    db.update_settings(|s| s.generation_method = GenerationMethod::Automatic)
        .unwrap();
    let err = db
        .update_settings(|s| s.generation_presentation = GenerationPresentation::Incremental)
        .unwrap_err();
    assert!(matches!(
        err,
        DbError::Settings(SettingsError::PresentationDisabled)
    ));
    // Switching to Automatic and changing presentation at once is refused too.
    let mut db2 = Database::open_in_memory().unwrap();
    db2.update_settings(|s| s.generation_method = GenerationMethod::Manual)
        .unwrap();
    assert!(
        db2.update_settings(|s| {
            s.generation_method = GenerationMethod::Automatic;
            s.generation_presentation = GenerationPresentation::OneTable;
        })
        .is_err()
    );
    assert_eq!(
        db.load_settings().unwrap().generation_presentation,
        GenerationPresentation::OneTable
    );
}

#[test]
fn an_edit_updates_name_and_snapshot_together() {
    let mut db = Database::open_in_memory().unwrap();
    let saved = db
        .create_settlement("Before", generate(&content(), 1))
        .unwrap();
    std::thread::sleep(std::time::Duration::from_millis(5));
    let revised = generate(&content(), 12);
    let updated = db
        .update_settlement(&saved.summary.id, " After ", &revised)
        .unwrap();
    assert_eq!(updated.summary.name, "After");
    assert_eq!(updated.snapshot, revised);
    assert_eq!(updated.summary.created_at, saved.summary.created_at);
    assert!(updated.summary.updated_at > saved.summary.updated_at);
    assert!(matches!(
        db.update_settlement(&saved.summary.id, "", &revised),
        Err(DbError::EmptyName)
    ));
    assert!(matches!(
        db.update_settlement(&"stl-none".into(), "X", &revised),
        Err(DbError::SettlementNotFound(_))
    ));
    // A failed edit changes nothing.
    assert_eq!(db.get_settlement(&saved.summary.id).unwrap(), updated);
}

#[test]
fn overly_long_names_are_refused() {
    let mut db = Database::open_in_memory().unwrap();
    let long = "x".repeat(81);
    assert!(matches!(
        db.create_settlement(&long, SettlementSnapshot::default()),
        Err(DbError::NameTooLong)
    ));
    assert!(
        db.create_settlement(&"x".repeat(80), SettlementSnapshot::default())
            .is_ok()
    );
}
