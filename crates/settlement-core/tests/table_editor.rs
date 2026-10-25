//! Support for the no-code table editor: applying Dice Assistant layouts,
//! secondary tables, result order, usages, drafts, preview rolls and copies.

use serde_json::json;
use settlement_core::content::{ContentSet, FaceRange, Table};
use settlement_core::db::Database;
use settlement_core::dice::ScriptedFaces;
use settlement_core::dice_assist::{FitKind, advise};
use settlement_core::engine::{AttemptOutcome, GenerationEvent, Generator};
use settlement_core::ids::{ResultId, TableId};
use settlement_core::layers::{ContentLayers, UserOverrides};
use settlement_core::service::AppService;
use settlement_core::table_editor::{ROLL_AGAIN_NAME, apply_fit, is_roll_again, table_usages};
use settlement_core::validation::{IssueKind, Severity, validate};

fn table(results: usize) -> Table {
    serde_json::from_value(json!({
        "id": "user.table.t", "name": "T", "dice": "1d6",
        "results": (0..results)
            .map(|i| json!({ "id": format!("r{i}"), "name": format!("R{i}") }))
            .collect::<Vec<_>>()
    }))
    .unwrap()
}

fn ids(t: &Table) -> Vec<ResultId> {
    t.results.iter().map(|r| r.id.clone()).collect()
}

fn service() -> AppService {
    AppService::new(Database::open_in_memory().unwrap())
}

// ----- applying layouts ----------------------------------------------------------

#[test]
fn an_exact_layout_sets_the_dice_and_every_range() {
    let t = table(5);
    let fit = advise(&[1; 5]).suggestions.remove(0);
    let out = apply_fit(&t, &ids(&t), &fit, "user.result.again".into()).unwrap();
    assert_eq!(out.dice.as_deref(), Some("1d10"));
    let ranges: Vec<_> = out.results.iter().map(|r| r.range.unwrap()).collect();
    assert_eq!(
        ranges,
        [(1, 2), (3, 4), (5, 6), (7, 8), (9, 10)].map(|(a, b)| FaceRange::new(a, b))
    );
    assert!(!out.results.iter().any(is_roll_again));
}

#[test]
fn a_roll_again_layout_adds_a_disabled_entry_that_is_rerolled() {
    let t = table(7);
    let fit = advise(&[1; 7]).suggestions.remove(0);
    assert_eq!(fit.kind, FitKind::RollAgain);
    let out = apply_fit(&t, &ids(&t), &fit, "user.result.again".into()).unwrap();
    let again = out.results.iter().find(|r| is_roll_again(r)).unwrap();
    assert_eq!(again.name, ROLL_AGAIN_NAME);
    assert_eq!(again.range, Some(FaceRange::single(8)));
    assert!(!again.enabled);
    // Physical dice: an 8 is rolled again, then a 3 stands.
    let content: ContentSet = serde_json::from_value(json!({ "tables": [out.clone()] })).unwrap();
    assert!(!validate(&content).has_errors(), "{}", validate(&content));
    let log = Generator::new(&content)
        .unwrap()
        .run(
            std::slice::from_ref(&out.id),
            &mut ScriptedFaces::new([8, 3]),
        )
        .unwrap();
    let record = log.record_for(&out.id).unwrap();
    assert_eq!(record.attempts[0].outcome, AttemptOutcome::Disabled);
    assert_eq!(record.result.name, "R2");
    // Applying an exact layout afterwards removes the entry again.
    let fit = advise(&[1; 6]).suggestions.remove(0);
    let mut six = out.clone();
    six.results.retain(|r| r.id.as_str() != "r6");
    let entries: Vec<ResultId> = six
        .results
        .iter()
        .filter(|r| !is_roll_again(r))
        .map(|r| r.id.clone())
        .collect();
    let back = apply_fit(&six, &entries, &fit, "user.result.other".into()).unwrap();
    assert!(!back.results.iter().any(is_roll_again));
    assert_eq!(back.dice.as_deref(), Some("1d6"));
}

#[test]
fn a_layout_must_match_the_entries() {
    let t = table(3);
    let fit = advise(&[1; 5]).suggestions.remove(0);
    assert!(apply_fit(&t, &ids(&t), &fit, "user.result.x".into()).is_err());
}

// ----- secondary tables ----------------------------------------------------------

fn secondary() -> ContentSet {
    serde_json::from_value(json!({
        "tables": [
            { "id": "table.a", "name": "A", "dice": "1d4", "results": [
                { "id": "hazard", "name": "Hazard", "range": [1, 2], "then_roll": ["table.b"] },
                { "id": "calm", "name": "Calm", "range": [3, 4] } ] },
            { "id": "table.b", "name": "B", "dice": "1d4", "results": [
                { "id": "x", "name": "X", "range": [1, 4] } ] }
        ]
    }))
    .unwrap()
}

#[test]
fn a_result_rolls_its_secondary_table_next() {
    let c = secondary();
    assert!(!validate(&c).has_errors());
    let log = Generator::new(&c)
        .unwrap()
        .run(&["table.a".into()], &mut ScriptedFaces::new([1, 4]))
        .unwrap();
    let b = log.record_for(&"table.b".into()).unwrap();
    assert_eq!(b.rolled_after.as_ref().unwrap().name, "Hazard");
    assert!(b.is_routed());
    // Calm rolls nothing more.
    let log = Generator::new(&c)
        .unwrap()
        .run(&["table.a".into()], &mut ScriptedFaces::new([3]))
        .unwrap();
    assert!(log.record_for(&"table.b".into()).is_none());
}

#[test]
fn secondary_tables_are_validated() {
    let mut c = secondary();
    c.tables[0].results[0].then_roll = vec!["table.none".into()];
    assert!(
        validate(&c)
            .issues
            .iter()
            .any(|i| i.severity == Severity::Error
                && i.kind
                    == IssueKind::MissingTable {
                        table: "table.none".into()
                    })
    );
    // A table rolling itself after every result never ends.
    let mut c = secondary();
    for r in &mut c.tables[1].results {
        r.then_roll = vec!["table.b".into()];
    }
    assert!(
        validate(&c)
            .issues
            .iter()
            .any(|i| i.severity == Severity::Error
                && matches!(
                    &i.kind,
                    IssueKind::RoutingCycle {
                        guaranteed: true,
                        ..
                    }
                ))
    );
}

// ----- customisations ------------------------------------------------------------

fn layers() -> ContentLayers {
    ContentLayers::new(secondary(), UserOverrides::default())
}

#[test]
fn reordering_a_bundled_tables_results_is_kept() {
    let mut l = layers();
    let mut t = l.merged().content.table(&"table.a".into()).unwrap().clone();
    t.results.reverse();
    l.put_table(t).unwrap();
    let merged = l.merged().content;
    let order: Vec<&str> = merged
        .table(&"table.a".into())
        .unwrap()
        .results
        .iter()
        .map(|r| r.id.as_str())
        .collect();
    assert_eq!(order, ["calm", "hazard"]);
    // Back to the default order: the customisation disappears.
    let mut t = merged.table(&"table.a".into()).unwrap().clone();
    t.results.reverse();
    l.put_table(t).unwrap();
    assert!(l.overrides().is_empty());
}

#[test]
fn secondary_tables_and_shared_links_are_customisable() {
    let mut l = layers();
    let mut t = l.merged().content.table(&"table.a".into()).unwrap().clone();
    t.results[1].then_roll = vec!["table.b".into()];
    t.results[0].then_roll.clear();
    l.put_table(t).unwrap();
    let json = serde_json::to_value(l.overrides()).unwrap();
    assert_eq!(
        json["tables"]["table.a"]["results"]["calm"]["then_roll"],
        json!(["table.b"])
    );
    let merged = l.merged().content;
    let a = merged.table(&"table.a".into()).unwrap();
    assert!(a.results[0].then_roll.is_empty());
}

#[test]
fn customisations_saved_before_these_fields_still_apply() {
    // The override format of earlier versions, with no order, then_roll or
    // shared fields.
    let overrides: UserOverrides = serde_json::from_value(json!({
        "format_version": 1,
        "tables": { "table.a": { "action": "modify", "base": "0000000000000000",
            "name": "Renamed",
            "results": { "calm": { "action": "modify", "base": "0000000000000000", "name": "Quiet" } } } }
    }))
    .unwrap();
    let merged = ContentLayers::new(secondary(), overrides).merged();
    let a = merged.content.table(&"table.a".into()).unwrap();
    assert_eq!(a.name, "Renamed");
    assert_eq!(a.results[1].name, "Quiet");
    assert_eq!(a.results[0].then_roll, [TableId::from("table.b")]);
}

// ----- usages and safe deletion --------------------------------------------------

#[test]
fn bundled_tables_report_what_uses_them() {
    let c = settlement_core::bundled::bundled_content().unwrap();
    let size: Vec<String> = table_usages(&c, &"table.size".into())
        .into_iter()
        .map(|u| u.description)
        .collect();
    assert!(
        size.contains(&"Pipeline stage “Size”".to_owned()),
        "{size:?}"
    );
    assert!(
        size.iter().any(|d| d.contains("Notable Locations")),
        "{size:?}"
    );
    let relationship: Vec<String> = table_usages(&c, &"table.authorityRelationship".into())
        .into_iter()
        .map(|u| u.description)
        .collect();
    assert_eq!(relationship, ["Pipeline stage “Authority Figures”"]);
}

#[test]
fn a_table_in_use_cannot_be_deleted_and_says_why() {
    let mut s = service();
    let err = s.delete_table(&"table.size".into()).unwrap_err();
    assert!(
        err.contains("still used by: Pipeline stage “Size”"),
        "{err}"
    );
}

#[test]
fn a_result_in_use_reports_its_uses() {
    let s = service();
    let types = s
        .table_detail(&"table.notableLocationType".into())
        .unwrap()
        .table;
    let uses = s
        .result_usages(&types, &"locationType.hospitality".into())
        .unwrap();
    assert!(
        uses.iter()
            .any(|u| u.description.contains("Notable Locations")),
        "{uses:?}"
    );
}

// ----- drafts, previews and copies -----------------------------------------------

#[test]
fn drafts_are_kept_until_saved_or_discarded() {
    let mut s = service();
    let mut t = s.copy_table(&"table.economy".into()).unwrap();
    assert!(t.id.is_user());
    assert!(t.name.starts_with("Copy of "));
    assert!(s.table_draft(&t.id).unwrap().is_none());
    t.name = "Draft".into();
    s.save_table_draft(&t).unwrap();
    assert_eq!(s.table_draft(&t.id).unwrap().unwrap().name, "Draft");
    // Saving the table removes the draft.
    s.save_table(t.clone()).unwrap();
    assert!(s.table_draft(&t.id).unwrap().is_none());
    s.save_table_draft(&t).unwrap();
    s.discard_table_draft(&t.id).unwrap();
    assert!(s.table_draft(&t.id).unwrap().is_none());
    // Settings are untouched by drafts.
    assert_eq!(s.settings().unwrap(), Default::default());
}

#[test]
fn a_preview_roll_uses_the_drafts_own_dice() {
    let s = service();
    let mut t = s.table_detail(&"table.economy".into()).unwrap().table;
    t.name = "Trial economy".into();
    let preview = s.preview_roll(t).unwrap();
    let GenerationEvent::TableRolled(record) = &preview.events[0] else {
        panic!("a roll")
    };
    assert_eq!(record.table.name, "Trial economy");
    assert_eq!(record.dice.as_deref(), Some("1d12"));
    assert!(!record.attempts.is_empty());
    assert!(preview.note.contains("each die is rolled separately"));
    // A draft with problems is not rolled.
    let mut broken = s.table_detail(&"table.economy".into()).unwrap().table;
    broken.results[0].range = Some(FaceRange::new(1, 99));
    assert!(s.preview_roll(broken).is_err());
}
