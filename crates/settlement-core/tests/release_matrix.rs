//! v0.1.0 release matrix: behaviour that only shows across restarts, across
//! the two interfaces sharing one database file, and when editing saved
//! settlements. Everything goes through `AppService`, exactly as the GUI and
//! the terminal interface do.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};

use settlement_core::content::{EffectStatus, FaceRange, TableResult};
use settlement_core::db::Database;
use settlement_core::dice::RandomFaces;
use settlement_core::engine::Selections;
use settlement_core::ids::{ResultId, TableId, TagId};
use settlement_core::service::AppService;
use settlement_core::settings::{GenerationMethod, GenerationPresentation, Settings, Theme};
use settlement_core::settlements::SettlementId;

struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Self {
        static NEXT: AtomicU32 = AtomicU32::new(0);
        let dir = std::env::temp_dir().join(format!(
            "settlement-matrix-test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&dir);
        TempDir(dir)
    }

    fn db(&self) -> PathBuf {
        self.0.join("data.sqlite3")
    }

    /// A fresh "application start" on the same data.
    fn open(&self) -> AppService {
        AppService::new(Database::open(&self.db()).unwrap())
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn generate(s: &mut AppService, name: &str) -> SettlementId {
    let mut active = s.start_generation().unwrap();
    active
        .session
        .choose(0, ResultId::new("kurovia"), &mut RandomFaces::seeded(5))
        .unwrap();
    active
        .session
        .run_automatic(&Selections::new(), &mut RandomFaces::seeded(6))
        .unwrap();
    s.save_new(&active, name).unwrap().id
}

#[test]
fn every_setting_survives_a_restart() {
    let tmp = TempDir::new();
    let mut s = tmp.open();
    let defaults = s.settings().unwrap();
    let mut next = defaults;
    next.generation_method = GenerationMethod::Manual;
    let manual = s.change_settings(&defaults, &next).unwrap();
    let mut next = manual;
    next.generation_presentation = GenerationPresentation::Incremental;
    next.god_mode = true;
    next.tag_view = true;
    next.theme = Theme::OldTimeyDark;
    let expected = s.change_settings(&manual, &next).unwrap();
    drop(s);

    assert_eq!(tmp.open().settings().unwrap(), expected);
    assert_eq!(
        expected,
        Settings {
            generation_method: GenerationMethod::Manual,
            generation_presentation: GenerationPresentation::Incremental,
            god_mode: true,
            tag_view: true,
            theme: Theme::OldTimeyDark,
            bare_minimum_buildings: false,
        }
    );
}

#[test]
fn every_kind_of_content_edit_survives_a_restart() {
    let tmp = TempDir::new();
    let mut s = tmp.open();

    // Rename a built-in table and tag; plan (edit) a tag effect.
    let mut table = s.table_detail(&TableId::new("table.age")).unwrap().table;
    table.name = "Age (house rules)".into();
    s.save_table(table).unwrap();
    let mut tag = s.tag_detail(&TagId::new("world.kurovia")).unwrap().tag;
    assert!(!tag.effects.is_empty(), "fixture tag has effects");
    tag.name = "Kurovia (ours)".into();
    tag.effects[0].status = EffectStatus::Planned;
    s.save_tag(tag).unwrap();

    // A custom tag, and a custom table whose dice mapping and results change.
    let mut custom_tag = AppService::new_tag();
    custom_tag.name = "Riverine".into();
    let custom_tag_id = custom_tag.id.clone();
    s.save_tag(custom_tag).unwrap();
    let mut custom = AppService::new_table();
    custom.name = "Weather".into();
    custom.results[0].range = Some(FaceRange::new(1, 3));
    let added = |id: &str, name: &str, range: FaceRange| TableResult {
        id: ResultId::new(id),
        name: name.into(),
        description: String::new(),
        range: Some(range),
        tags: Vec::new(),
        eligibility: None,
        enabled: true,
        generator: None,
        value: None,
        unique: false,
        fallback: None,
        label_variants: Vec::new(),
    };
    custom
        .results
        .push(added("user.result.rain", "Rain", FaceRange::new(4, 6)));
    let custom_id = custom.id.clone();
    s.save_table(custom).unwrap();
    // Edit the mapping, delete a result, add another that emits the tag.
    let mut custom = s.table_detail(&custom_id).unwrap().table;
    custom
        .results
        .retain(|r| r.id.as_str() != "user.result.rain");
    custom.results[0].range = Some(FaceRange::new(1, 2));
    let mut storm = added("user.result.storm", "Storm", FaceRange::new(3, 6));
    storm.tags.push(custom_tag_id.clone());
    custom.results.push(storm);
    s.save_table(custom).unwrap();
    drop(s);

    let s = tmp.open();
    let content = s.content().unwrap();
    assert_eq!(
        content.table(&TableId::new("table.age")).unwrap().name,
        "Age (house rules)"
    );
    let kurovia = content.tag(&TagId::new("world.kurovia")).unwrap();
    assert_eq!(kurovia.name, "Kurovia (ours)");
    assert_eq!(kurovia.effects[0].status, EffectStatus::Planned);
    assert_eq!(content.tag(&custom_tag_id).unwrap().name, "Riverine");
    let weather = content.table(&custom_id).unwrap();
    let mapping: Vec<_> = weather
        .results
        .iter()
        .map(|r| (r.name.as_str(), r.range.unwrap().to_string()))
        .collect();
    assert_eq!(
        mapping,
        [("New result", "1-2".into()), ("Storm", "3-6".into())]
    );
    assert_eq!(weather.results[1].tags, [custom_tag_id]);
}

#[test]
fn resetting_one_item_or_everything_survives_a_restart() {
    let tmp = TempDir::new();
    let mut s = tmp.open();
    let age = TableId::new("table.age");
    let original = s.table_detail(&age).unwrap().table.name;
    let mut table = s.table_detail(&age).unwrap().table;
    table.name = "Renamed".into();
    s.save_table(table).unwrap();
    let mut tag = s.tag_detail(&TagId::new("age.old")).unwrap().tag;
    tag.name = "Venerable".into();
    s.save_tag(tag).unwrap();

    s.reset_table(&age).unwrap();
    drop(s);
    let mut s = tmp.open();
    assert_eq!(s.table_detail(&age).unwrap().table.name, original);
    assert_eq!(
        s.tag_detail(&TagId::new("age.old")).unwrap().tag.name,
        "Venerable",
        "resetting one item leaves the others"
    );

    s.reset_all().unwrap();
    drop(s);
    let s = tmp.open();
    assert_ne!(
        s.tag_detail(&TagId::new("age.old")).unwrap().tag.name,
        "Venerable"
    );
}

#[test]
fn both_interfaces_share_one_database_file() {
    let tmp = TempDir::new();
    // The GUI and the terminal interface each hold their own service.
    let mut gui = tmp.open();
    let mut tui = tmp.open();

    let id = generate(&mut gui, "Riverside");
    let mut tag = gui.tag_detail(&TagId::new("age.old")).unwrap().tag;
    tag.name = "Venerable".into();
    gui.save_tag(tag).unwrap();

    assert_eq!(tui.saved_list().unwrap()[0].id, id);
    assert_eq!(
        tui.tag_detail(&TagId::new("age.old")).unwrap().tag.name,
        "Venerable"
    );
    tui.saved_set_favourite(&id, true).unwrap();
    assert!(gui.saved_delete(&id).is_err(), "favourite set elsewhere");

    // Each interface changes a different setting from its own (stale) copy:
    // neither change reverts the other.
    let gui_view = gui.settings().unwrap();
    let tui_view = tui.settings().unwrap();
    let mut gui_next = gui_view;
    gui_next.theme = Theme::OldTimeyDark;
    gui.change_settings(&gui_view, &gui_next).unwrap();
    let mut tui_next = tui_view;
    tui_next.tag_view = true;
    let stored = tui.change_settings(&tui_view, &tui_next).unwrap();
    assert_eq!(stored.theme, Theme::OldTimeyDark);
    assert!(stored.tag_view);
}

#[test]
fn edits_keep_the_generated_with_version_unless_results_change() {
    let tmp = TempDir::new();
    let mut s = tmp.open();
    let id = generate(&mut s, "Riverside");
    // Pretend an older release generated it.
    rusqlite::Connection::open(tmp.db())
        .unwrap()
        .execute(
            "UPDATE settlements SET app_version = '0.0.9' WHERE id = ?1",
            [id.as_str()],
        )
        .unwrap();

    // Reopened unchanged: nothing to save.
    let edit = s.start_edit(&id).unwrap();
    assert!(!s.edit_has_changes(&edit, "  Riverside ").unwrap());
    assert!(s.save_edit(&edit, "Riverside").is_err());

    // A rename alone keeps the original version.
    assert!(s.edit_has_changes(&edit, "Riverside Ford").unwrap());
    let renamed = s.save_edit(&edit, "Riverside Ford").unwrap();
    assert_eq!(renamed.summary.name, "Riverside Ford");
    assert_eq!(renamed.summary.app_version, "0.0.9");

    // Changing a result regenerates the snapshot with this version.
    let mut edit = s.start_edit(&id).unwrap();
    let region = &edit.session.snapshot(Default::default()).unwrap().stages[1];
    let other = region
        .options
        .iter()
        .find(|o| o.legal && Some(&o.result.id) != region.result.as_ref().map(|r| &r.id))
        .expect("another region")
        .result
        .id
        .clone();
    edit.session
        .choose(1, other, &mut RandomFaces::seeded(9))
        .unwrap();
    edit.session
        .roll_remaining(&mut RandomFaces::seeded(10))
        .unwrap();
    if !edit.session.is_complete() {
        edit.session
            .run_automatic(&Selections::new(), &mut RandomFaces::seeded(11))
            .unwrap();
    }
    assert!(edit.session.is_complete());
    let revised = s.save_edit(&edit, "Riverside Ford").unwrap();
    assert_eq!(revised.summary.app_version, settlement_core::VERSION);
}
