//! The shared application service used by the GUI and the terminal.

use settlement_core::db::Database;
use settlement_core::dice::RandomFaces;
use settlement_core::engine::Selections;
use settlement_core::ids::{ResultId, TableId, TagId};
use settlement_core::service::AppService;
use settlement_core::settings::GenerationMethod;

fn service() -> AppService {
    AppService::new(Database::open_in_memory().unwrap())
}

#[test]
fn generation_round_trip_through_the_service() {
    let mut s = service();
    let mut active = s.start_generation().unwrap();
    assert!(active.editing.is_none());
    assert!(
        s.save_new(&active, "Too early").is_err(),
        "incomplete is refused"
    );

    active
        .session
        .choose(0, ResultId::new("kurovia"), &mut RandomFaces::seeded(1))
        .unwrap();
    let mut sizes = Selections::new();
    sizes.insert("table.size".into(), "town".into());
    active
        .session
        .run_automatic(&sizes, &mut RandomFaces::seeded(2))
        .unwrap();
    let saved = s.save_new(&active, "Riverside").unwrap();
    assert_eq!(s.saved_list().unwrap()[0].id, saved.id);

    // Editing reopens the snapshot; saving revises it.
    let edit = s.start_edit(&saved.id).unwrap();
    assert!(edit.session.is_complete());
    let revised = s.save_edit(&edit, "Riverside II").unwrap();
    assert_eq!(revised.summary.name, "Riverside II");
    assert_eq!(revised.stages.len(), 8);
}

#[test]
fn favourites_are_protected_through_the_service() {
    let mut s = service();
    let mut active = s.start_generation().unwrap();
    active
        .session
        .choose(0, ResultId::new("kurovia"), &mut RandomFaces::seeded(1))
        .unwrap();
    let mut sizes = Selections::new();
    sizes.insert("table.size".into(), "village".into());
    active
        .session
        .run_automatic(&sizes, &mut RandomFaces::seeded(3))
        .unwrap();
    let saved = s.save_new(&active, "Keep").unwrap();
    s.saved_set_favourite(&saved.id, true).unwrap();
    assert!(s.saved_delete(&saved.id).is_err());
    s.saved_set_favourite(&saved.id, false).unwrap();
    s.saved_delete(&saved.id).unwrap();
}

#[test]
fn content_edits_are_checked_and_persisted() {
    let mut s = service();
    let mut table = s.table_detail(&TableId::new("table.age")).unwrap().table;
    table.dice = Some("1d20".into());
    let check = s.check_table(table.clone()).unwrap();
    assert!(!check.is_acceptable());
    assert!(s.save_table(table).is_err());

    let mut tag = s.tag_detail(&TagId::new("age.old")).unwrap().tag;
    tag.name = "Venerable".into();
    s.save_tag(tag).unwrap();
    let overview = s.overview().unwrap();
    let old = overview
        .tags
        .iter()
        .find(|t| t.id.as_str() == "age.old")
        .unwrap();
    assert_eq!(old.name, "Venerable");
    s.reset_all().unwrap();
    assert_eq!(
        s.tag_detail(&TagId::new("age.old")).unwrap().tag.name,
        "Old"
    );
}

#[test]
fn settings_go_through_the_core_rules() {
    let mut s = service();
    let mut settings = s.settings().unwrap();
    settings.generation_method = GenerationMethod::Manual;
    let stored = s.save_settings(&settings).unwrap();
    assert_eq!(stored.generation_method, GenerationMethod::Manual);
}
