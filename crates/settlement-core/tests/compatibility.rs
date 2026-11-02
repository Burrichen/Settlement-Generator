//! Saves from released versions keep working: a database written by v0.1.1
//! itself (tests/fixtures/v0.1.1.sqlite3) opens with this build, its
//! settlements read back unchanged, and they can be mapped later without
//! regenerating anything.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};

use settlement_core::db::{Database, latest_schema_version};
use settlement_core::maps::{MapGenerationOptions, MapLayout, MapStatus};
use settlement_core::service::AppService;

/// A private copy of the fixture (opening it migrates it).
struct Copy(PathBuf);

impl Copy {
    fn new() -> Self {
        static NEXT: AtomicU32 = AtomicU32::new(0);
        let dir = std::env::temp_dir().join(format!(
            "settlement-compat-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let fixture =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/v0.1.1.sqlite3");
        let path = dir.join("settlement-generator.sqlite3");
        std::fs::copy(fixture, &path).unwrap();
        Copy(path)
    }
}

impl Drop for Copy {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(self.0.parent().unwrap());
    }
}

#[test]
fn a_v0_1_1_database_opens_and_keeps_every_settlement() {
    let copy = Copy::new();
    let db = Database::open(&copy.0).unwrap();
    assert_eq!(db.schema_version().unwrap(), latest_schema_version());
    let s = AppService::new(db);
    let mut names: Vec<String> = s
        .saved_list()
        .unwrap()
        .into_iter()
        .map(|x| x.name)
        .collect();
    names.sort();
    assert_eq!(
        names,
        ["Little Hollow", "Riverside", "Ærøskøbing – Ōshū 城"]
    );
    for summary in s.saved_list().unwrap() {
        assert_eq!(summary.app_version, "0.1.1");
        let view = s.saved_get(&summary.id).unwrap();
        assert!(!view.stages.is_empty(), "{} reads back", summary.name);
        // No maps were stored, and none appear by themselves.
        let maps = s.maps_get(&summary.id).unwrap();
        assert_eq!(maps.status, MapStatus::NotGenerated);
        assert!(maps.maps.is_none() && maps.damaged.is_empty());
    }
    // The upgrade kept a verified backup of the old database.
    let backups = std::fs::read_dir(copy.0.parent().unwrap().join("backups")).unwrap();
    assert!(backups.count() >= 1);
}

#[test]
fn v0_1_1_settlements_can_be_mapped_later_without_regenerating_them() {
    let copy = Copy::new();
    let mut s = AppService::new(Database::open(&copy.0).unwrap());
    for summary in s.saved_list().unwrap() {
        let before = s.saved_get(&summary.id).unwrap();
        let view = s.maps_get(&summary.id).unwrap();
        match summary.name.as_str() {
            // A v0.1.1 City predates districts: it is not given invented
            // ones, and says what is missing.
            "Ærøskøbing – Ōshū 城" => {
                assert_eq!(view.layout, Some(MapLayout::PerDistrict));
                let problem = view.problem.expect("explains why it cannot be mapped yet");
                assert!(problem.contains("no districts"), "{problem}");
                assert!(
                    s.maps_generate(&summary.id, &MapGenerationOptions::default())
                        .is_err()
                );
            }
            _ => {
                assert_eq!(view.layout, Some(MapLayout::Single));
                let drawn = s
                    .maps_generate(&summary.id, &MapGenerationOptions::default())
                    .unwrap();
                assert_eq!(drawn.status, MapStatus::Current);
                // Nothing in the old save is needed beyond what it has: it has
                // no Notable Locations stage, so the directory is empty.
                assert!(drawn.locations.is_empty());
                let map = &drawn.maps.as_ref().unwrap().maps[0];
                let details = s
                    .maps_building(&summary.id, &map.id, &map.geometry.buildings[0].id)
                    .unwrap();
                assert_eq!(details.version, 1);
            }
        }
        assert_eq!(s.saved_get(&summary.id).unwrap(), before, "never changed");
    }
}
