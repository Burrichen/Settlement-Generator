//! The framework's testing and sharing tools: table and flow simulations,
//! one-off previews with every roll explained, generation traces in the
//! Roll details view, import and export with reference checks and ID
//! collisions, and safe deletion of flows.

use serde_json::json;
use settlement_core::content::Table;
use settlement_core::db::Database;
use settlement_core::dice::RandomFaces;
use settlement_core::engine::Selections;
use settlement_core::framework::Flow;
use settlement_core::ids::{FlowId, ResultId, TableId, TagId};
use settlement_core::service::{AppService, ExportScope};
use settlement_core::session::ViewOptions;
use settlement_core::transfer::{Collision, ImportAction, PackItem, PackKind, read_pack};

fn service() -> AppService {
    AppService::new(Database::open_in_memory().unwrap())
}

fn table(service: &AppService, id: &str) -> Table {
    service.table_detail(&TableId::new(id)).unwrap().table
}

// ----- simulation -------------------------------------------------------------------

#[test]
fn a_table_simulation_compares_actual_with_expected_chances() {
    let service = service();
    let crime = table(&service, "table.crime");
    let sim = service.simulate_table(&crime, 10_000, &[]).unwrap();
    assert_eq!(sim.runs, 10_000);
    let expected: f64 = sim.results.iter().filter_map(|r| r.expected).sum();
    assert!((expected - 100.0).abs() < 1e-6, "expected chances add up");
    for r in &sim.results {
        let e = r.expected.unwrap();
        assert!(
            (r.actual - e).abs() < 3.0,
            "{}: {} vs {e}",
            r.name,
            r.actual
        );
    }
    assert!(sim.errors.is_empty(), "{:?}", sim.errors);
}

#[test]
fn a_tag_modifier_shows_in_the_simulation() {
    let mut service = service();
    let mut tag = AppService::new_tag();
    tag.name = "Watchful".into();
    tag.effects = serde_json::from_value(json!([
        { "kind": "minus", "table": "table.crime", "amount": 3 }
    ]))
    .unwrap();
    let id = tag.id.clone();
    service.save_tag(tag).unwrap();
    let crime = table(&service, "table.crime");
    let plain = service.simulate_table(&crime, 1000, &[]).unwrap();
    let watched = service.simulate_table(&crime, 1000, &[id]).unwrap();
    assert!(plain.modifiers.is_empty());
    assert_eq!(watched.modifiers[0].count, 1000);
    assert!(
        watched.modifiers[0].label.contains("Watchful"),
        "{:?}",
        watched.modifiers
    );
    // Taking 3 from the roll moves expected chances to the low end.
    let first = |s: &settlement_core::simulate::TableSimulation| s.results[0].expected.unwrap();
    assert!(first(&watched) > first(&plain));
}

fn branching_flow() -> Flow {
    serde_json::from_value(json!({
        "id": "user.flow.test", "name": "Test", "steps": [
            { "id": "crime", "kind": "roll", "table": "table.crime" },
            { "id": "pick", "kind": "branch", "cases": [
                { "when": { "result_is": { "step": "crime", "results": ["low"] } },
                  "steps": [{ "id": "stability", "kind": "roll", "table": "table.stability" }] },
                { "when": { "not": { "all": [] } }, "steps": [] } ] }
        ]
    }))
    .unwrap()
}

#[test]
fn a_flow_simulation_counts_outcomes_and_reports_impossible_branches() {
    let service = service();
    let crime = table(&service, "table.crime");
    assert!(crime.result(&ResultId::new("low")).is_some());
    let sim = service.simulate_flow(&branching_flow(), 500).unwrap();
    assert_eq!(sim.runs, 500);
    let crime_stat = sim.steps.iter().find(|s| s.step == "crime").unwrap();
    assert_eq!(crime_stat.reached, 500);
    let total: u32 = crime_stat.outcomes.iter().map(|o| o.count).sum();
    assert_eq!(total, 500);
    let branch = &sim.branches[0];
    assert_eq!(branch.cases.iter().map(|c| c.count).sum::<u32>(), 500);
    // "Not always" never holds: the runs show it was never taken.
    assert!(
        sim.impossible
            .iter()
            .any(|m| m.contains("Case 2") && m.contains("never taken")),
        "{:?}",
        sim.impossible
    );
}

#[test]
fn the_pipeline_simulates_as_whole_generations() {
    let service = service();
    let content = service.content().unwrap();
    let start = content
        .flow(content.start.as_ref().unwrap())
        .unwrap()
        .clone();
    let sim = service.simulate_flow(&start, 40).unwrap();
    assert!(sim.steps.iter().any(|s| s.label == "Setting Profile"));
    assert!(sim.note.contains("whole test generations"));
}

#[test]
fn roll_once_explains_every_roll() {
    let service = service();
    let preview = service.preview_flow(&branching_flow()).unwrap();
    assert!(preview.error.is_none(), "{:?}", preview.error);
    assert!(!preview.steps.is_empty());
    let crime = &preview.traces[0];
    assert!(
        crime.lines.iter().any(|l| l.starts_with("Base roll: ")),
        "{crime:?}"
    );
    assert!(
        crime.lines.last().unwrap().starts_with("Tags given")
            || crime.lines.iter().any(|l| l.starts_with("Result: "))
    );
    let roll = service
        .preview_roll(table(&service, "table.crime"))
        .unwrap();
    assert!(
        roll.traces[0]
            .lines
            .iter()
            .any(|l| l.starts_with("Result: "))
    );
}

// ----- generation trace ------------------------------------------------------------------

#[test]
fn roll_details_explain_each_stage_of_a_generation() {
    let mut service = service();
    let mut settings = service.settings().unwrap();
    settings.roll_details = true;
    service.save_settings(&settings).unwrap();
    let mut active = service.start_generation().unwrap();
    assert!(active.view.debug);
    active
        .session
        .choose(0, ResultId::new("kurovia"), &mut RandomFaces::seeded(1))
        .unwrap();
    let mut selections = Selections::new();
    selections.insert("table.size".into(), "town".into());
    active
        .session
        .run_automatic(&selections, &mut RandomFaces::seeded(2))
        .unwrap();
    let views = active.session.views(active.view).unwrap();
    let crime = views
        .iter()
        .find(|v| v.stage.id.as_str() == "table.crime")
        .unwrap();
    let trace = &crime.trace.as_ref().expect("a trace")[0];
    assert!(trace.lines[0].starts_with("Base roll: "), "{trace:?}");
    assert!(trace.lines.iter().any(|l| l.starts_with("Result: ")));
    // Without Roll details there is none.
    let plain = active
        .session
        .views(ViewOptions {
            show_tags: false,
            debug: false,
        })
        .unwrap();
    assert!(plain.iter().all(|v| v.trace.is_none()));
}

// ----- import and export -----------------------------------------------------------------

#[test]
fn a_table_exports_with_what_it_needs_and_imports_under_new_ids() {
    let mut source = service();
    let mut omen = AppService::new_tag();
    omen.name = "Omen".into();
    let omen_id = omen.id.clone();
    source.save_tag(omen).unwrap();
    let mut economy = table(&source, "table.economy");
    economy.id = TableId::new("user.table.economy");
    economy.results[0].tags.push(omen_id.clone());
    source.save_table(economy).unwrap();
    let pack = source
        .export_items(
            &[PackItem {
                kind: PackKind::Table,
                id: "user.table.economy".into(),
            }],
            true,
            "Economy",
        )
        .unwrap();
    assert_eq!(pack.tables.len(), 1);
    // The custom tag comes along; unchanged defaults are named by ID only.
    assert_eq!(pack.tags.len(), 1);
    assert_eq!(pack.tags[0].id, omen_id);
    let text = serde_json::to_string(&pack).unwrap();

    // Importing into the same content: every ID is taken, so Rename gives
    // new ones, and references inside the pack follow.
    let mut target = source;
    let pack = read_pack(&text).unwrap();
    let plan = target.plan_import(&pack, Collision::Rename).unwrap();
    assert!(plan.can_import, "{:?} {:?}", plan.issues, plan.error);
    assert!(
        plan.items
            .iter()
            .all(|i| matches!(i.action, ImportAction::AddAs { .. }))
    );
    target.import_pack(&pack, Collision::Rename).unwrap();
    let overview = target.overview().unwrap();
    let copy = overview
        .tables
        .iter()
        .find(|t| {
            t.name == "Economy"
                && t.id.as_str() != "user.table.economy"
                && t.id.as_str().starts_with("user.")
        })
        .expect("the imported table");
    let imported = target.table_detail(&copy.id).unwrap().table;
    let tags = &imported.results[0].tags;
    assert!(
        !tags.contains(&omen_id),
        "the reference follows the renamed tag"
    );
    let renamed = tags
        .iter()
        .find(|t| t.as_str().starts_with("user.tag."))
        .unwrap();
    assert_eq!(target.tag_detail(renamed).unwrap().tag.name, "Omen");
    target.content().expect("content stays valid");
}

#[test]
fn replacing_and_skipping_existing_items() {
    let mut source = service();
    let mut crime = table(&source, "table.crime");
    crime.name = "Crime (shared)".into();
    source.save_table(crime).unwrap();
    let pack = source.export_everything(ExportScope::Customised).unwrap();
    assert_eq!(pack.tables.len(), 1);

    let mut target = service();
    let skip = target.plan_import(&pack, Collision::Skip).unwrap();
    assert!(matches!(skip.items[0].action, ImportAction::Skip { .. }));
    target.import_pack(&pack, Collision::Replace).unwrap();
    assert_eq!(table(&target, "table.crime").name, "Crime (shared)");
    let origin = target
        .table_detail(&TableId::new("table.crime"))
        .unwrap()
        .origin;
    assert_eq!(origin, Some(settlement_core::layers::Origin::Modified));
}

#[test]
fn an_import_that_would_break_references_changes_nothing() {
    let mut target = service();
    let mut pack = target.export_everything(ExportScope::Customised).unwrap();
    pack.flows.push(
        serde_json::from_value(
            json!({ "id": "user.flow.broken", "name": "Broken", "steps": [
            { "id": "roll", "kind": "roll", "table": "table.missing" } ] }),
        )
        .unwrap(),
    );
    let plan = target.plan_import(&pack, Collision::Rename).unwrap();
    assert!(!plan.can_import);
    assert!(
        plan.issues
            .iter()
            .any(|i| i.message.contains("table.missing"))
    );
    let before = target.framework_items().unwrap();
    assert!(target.import_pack(&pack, Collision::Rename).is_err());
    assert_eq!(target.framework_items().unwrap(), before);
}

#[test]
fn only_content_files_of_a_known_version_are_read() {
    assert!(
        read_pack("{}")
            .unwrap_err()
            .contains("not a Settlement Generator content file")
    );
    let newer = json!({ "format": "settlement-generator-content", "version": 99 }).to_string();
    assert!(read_pack(&newer).unwrap_err().contains("newer version"));
    assert!(
        read_pack("not json")
            .unwrap_err()
            .contains("not valid JSON")
    );
}

#[test]
fn packs_are_written_to_and_read_from_files() {
    let service = service();
    let dir = std::env::temp_dir().join(format!("sg-export-{}", std::process::id()));
    let pack = service.export_everything(ExportScope::Everything).unwrap();
    let path = AppService::write_pack(&pack, "All: content?", Some(dir.clone())).unwrap();
    assert_eq!(path.file_name().unwrap(), "All- content-.json");
    let back = AppService::read_pack_file(&path).unwrap();
    assert_eq!(back, pack);
    let _ = std::fs::remove_dir_all(dir);
}

// ----- dependency safety ---------------------------------------------------------------

#[test]
fn a_flow_another_flow_runs_cannot_be_deleted() {
    let mut service = service();
    let mut inner: Flow = serde_json::from_value(json!({
        "id": "user.flow.inner", "name": "Inner", "steps": [] }))
    .unwrap();
    inner.steps.clear();
    service.save_flow(inner).unwrap();
    let outer: Flow = serde_json::from_value(json!({
        "id": "user.flow.outer", "name": "Outer", "steps": [
            { "id": "run", "kind": "run_flow", "flow": "user.flow.inner" } ] }))
    .unwrap();
    service.save_flow(outer).unwrap();
    let usages = service
        .flow_usages(&FlowId::new("user.flow.inner"))
        .unwrap();
    assert_eq!(usages.len(), 1);
    assert_eq!(usages[0].flow.as_ref().unwrap().as_str(), "user.flow.outer");
    let err = service
        .delete_flow(&FlowId::new("user.flow.inner"))
        .unwrap_err();
    assert!(err.contains("still used"), "{err}");
    service
        .delete_flow(&FlowId::new("user.flow.outer"))
        .unwrap();
    service
        .delete_flow(&FlowId::new("user.flow.inner"))
        .unwrap();
    // The start flow is the pipeline: never deletable.
    let start = service.content().unwrap().start.clone().unwrap();
    assert!(service.delete_flow(&start).is_err());
    let _ = TagId::new("unused");
}

#[test]
fn explaining_and_testing_never_change_what_is_generated() {
    // The same seeds give the same settlement with Roll details on or off,
    // and after simulations have run.
    let generate = |details: bool| {
        let mut service = service();
        let mut settings = service.settings().unwrap();
        settings.roll_details = details;
        service.save_settings(&settings).unwrap();
        if details {
            let crime = table(&service, "table.crime");
            service.simulate_table(&crime, 500, &[]).unwrap();
            service.preview_flow(&branching_flow()).unwrap();
        }
        let mut active = service.start_generation().unwrap();
        active
            .session
            .choose(0, ResultId::new("kurovia"), &mut RandomFaces::seeded(11))
            .unwrap();
        let mut selections = Selections::new();
        selections.insert("table.size".into(), "city".into());
        active
            .session
            .run_automatic(&selections, &mut RandomFaces::seeded(12))
            .unwrap();
        let _ = active.session.views(active.view).unwrap();
        serde_json::to_string(&active.session.to_snapshot()).unwrap()
    };
    let plain = generate(false);
    let explained = generate(true);
    // Settlements differ only by random NPC IDs; compare with them numbered.
    let canonical = |s: &str| {
        let mut out = String::new();
        let mut ids: Vec<String> = Vec::new();
        let mut rest = s;
        while let Some(at) = rest.find("npc-") {
            out.push_str(&rest[..at]);
            let id = rest[at..(at + 36).min(rest.len())].to_owned();
            let n = ids.iter().position(|i| *i == id).unwrap_or_else(|| {
                ids.push(id.clone());
                ids.len() - 1
            });
            out.push_str(&format!("npc#{n}"));
            rest = &rest[(at + 36).min(rest.len())..];
        }
        out.push_str(rest);
        out
    };
    assert_eq!(canonical(&plain), canonical(&explained));
}
