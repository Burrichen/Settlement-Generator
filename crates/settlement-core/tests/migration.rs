//! Migration from the released app. `fixtures/released-0.1` holds a
//! database written by the released version (main, before the framework):
//! customised settings (Bare Minimum Buildings on, Tag View, a theme), a
//! customised table and tag, a custom tag, and three saved settlements with
//! Authority Figures, roles and Notable Locations, one a favourite.
//! `expected.json` is what that version showed for each saved settlement.
//!
//! This build must open it and keep every part of it: overrides, settings,
//! saved settlements, favourites, and their exact records.

use std::path::PathBuf;

use serde_json::Value;
use settlement_core::db::Database;
use settlement_core::dice::RandomFaces;
use settlement_core::engine::Selections;
use settlement_core::framework::{BARE_MINIMUM_BUILDINGS, SettingValue};
use settlement_core::ids::{ResultId, TableId, TagId};
use settlement_core::service::AppService;
use settlement_core::settlements::SettlementId;

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/released-0.1")
        .join(name)
}

/// The released database, copied so the test never changes the fixture.
fn open_released() -> (AppService, Value, tempdir::Dir) {
    let dir = tempdir::Dir::new();
    let path = dir.path().join("settlements.sqlite3");
    std::fs::copy(fixture("settlements.sqlite3"), &path).unwrap();
    let expected: Value =
        serde_json::from_str(&std::fs::read_to_string(fixture("expected.json")).unwrap()).unwrap();
    (
        AppService::new(Database::open(&path).unwrap()),
        expected,
        dir,
    )
}

/// A temporary directory removed when dropped (no extra dependency).
mod tempdir {
    use std::path::{Path, PathBuf};

    pub struct Dir(PathBuf);

    impl Dir {
        pub fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "sg-migration-{}-{}",
                std::process::id(),
                rand_suffix()
            ));
            std::fs::create_dir_all(&path).unwrap();
            Dir(path)
        }

        pub fn path(&self) -> &Path {
            &self.0
        }
    }

    fn rand_suffix() -> u128 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    }

    impl Drop for Dir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
}

#[test]
fn customised_content_is_kept_and_still_valid() {
    let (service, expected, _dir) = open_released();
    // The whole effective content is valid under the framework.
    let content = service.content().expect("released overrides load cleanly");
    assert_eq!(
        content.table(&TableId::new("table.crime")).unwrap().name,
        "Crime (customised)"
    );
    assert_eq!(
        content
            .tag(&TagId::new("world.kurovia"))
            .unwrap()
            .description,
        "A customised description."
    );
    let custom = expected["custom_tag"].as_str().unwrap();
    assert_eq!(content.tag(&TagId::new(custom)).unwrap().name, "Haunted");
    // The pipeline is a flow now, with the built-in Authority and Notable
    // Locations stages.
    let start = content.flow(content.start.as_ref().unwrap()).unwrap();
    let handlers: Vec<&str> = start
        .steps
        .iter()
        .filter_map(|s| match &s.action {
            settlement_core::framework::StepAction::Builtin { handler, .. } => {
                Some(handler.as_str())
            }
            _ => None,
        })
        .collect();
    assert_eq!(
        handlers,
        ["authority_roles", "authority_figures", "notable_locations"]
    );
}

#[test]
fn settings_are_kept_and_bare_minimum_buildings_becomes_a_content_setting() {
    let (service, expected, _dir) = open_released();
    let settings = serde_json::to_value(service.settings().unwrap()).unwrap();
    for (key, value) in expected["settings"].as_object().unwrap() {
        assert_eq!(&settings[key], value, "setting {key}");
    }
    let bmb = service
        .content_settings()
        .unwrap()
        .into_iter()
        .find(|v| v.setting.id.as_str() == BARE_MINIMUM_BUILDINGS)
        .unwrap();
    assert_eq!(bmb.value, SettingValue::Toggle(true));
}

#[test]
fn saved_settlements_and_favourites_are_unchanged() {
    let (service, expected, _dir) = open_released();
    let saved = expected["saved"].as_array().unwrap();
    let list = service.saved_list().unwrap();
    assert_eq!(list.len(), saved.len());
    for view in saved {
        let id = SettlementId::from(view["id"].as_str().unwrap());
        let now = serde_json::to_value(service.saved_get(&id).unwrap()).unwrap();
        // Every stage, result, die, tag, figure and location as it was.
        assert_eq!(&now, view, "saved settlement {}", view["name"]);
    }
    let favourites: Vec<&str> = list
        .iter()
        .filter(|s| s.favourite)
        .map(|s| s.name.as_str())
        .collect();
    assert_eq!(favourites, ["Legacy city"]);
}

#[test]
fn saved_settlements_reopen_for_editing_without_changes() {
    let (service, expected, _dir) = open_released();
    for view in expected["saved"].as_array().unwrap() {
        let id = SettlementId::from(view["id"].as_str().unwrap());
        let name = view["name"].as_str().unwrap();
        let active = service.start_edit(&id).unwrap();
        assert!(active.session.is_complete(), "{name} reopens complete");
        assert!(
            !service.edit_has_changes(&active, name).unwrap(),
            "{name} reopens unchanged"
        );
    }
}

#[test]
fn new_generations_still_work_with_the_migrated_data() {
    let (mut service, _, _dir) = open_released();
    let mut active = service.start_generation().unwrap();
    // Bare Minimum Buildings was on in the released settings.
    assert!(active.session.options().bare_minimum_buildings);
    active
        .session
        .choose(0, ResultId::new("kurovia"), &mut RandomFaces::seeded(5))
        .unwrap();
    let mut selections = Selections::new();
    selections.insert("table.size".into(), "town".into());
    active
        .session
        .run_automatic(&selections, &mut RandomFaces::seeded(6))
        .unwrap();
    let summary = service.save_new(&active, "After migration").unwrap();
    let saved = service.saved_get(&summary.id).unwrap();
    let json = serde_json::to_string(&saved).unwrap();
    assert!(
        json.contains("bareMinimumBuildings"),
        "the guaranteed Tavern"
    );
    assert_eq!(service.saved_list().unwrap().len(), 4);
}
