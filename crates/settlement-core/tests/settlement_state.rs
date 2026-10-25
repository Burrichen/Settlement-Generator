//! Security, Crime and Stability: three ordered 1d12 settlement-state
//! tables after the Authority, with planned (direction-only) modifiers.

use std::sync::Arc;

use settlement_core::bundled::bundled_content;
use settlement_core::content::{ContentSet, Effect, IntentChange};
use settlement_core::db::Database;
use settlement_core::dice::{RandomFaces, ScriptedFaces};
use settlement_core::engine::{GenerationOptions, Generator};
use settlement_core::ids::{ResultId, TableId};
use settlement_core::layers::{ContentLayers, UserOverrides};
use settlement_core::service::AppService;
use settlement_core::session::{GenerationMode, GenerationSession, ViewOptions};
use settlement_core::settlements::SettlementSnapshot;
use settlement_core::validation::IssueKind;

const SECURITY: usize = 15;
const CRIME: usize = 16;
const STABILITY: usize = 17;

fn content() -> ContentSet {
    bundled_content().unwrap()
}

/// `(face range, result ID, name, tag)` of each table, as approved.
fn spec(table: &str) -> Vec<(i32, i32, &'static str, &'static str, String)> {
    let rows: [(i32, i32, &str, &str); 5] = match table {
        "security" => [
            (1, 1, "undefended", "Undefended"),
            (2, 4, "lightlyDefended", "Lightly Defended"),
            (5, 8, "guarded", "Guarded"),
            (9, 11, "wellDefended", "Well Defended"),
            (12, 12, "fortified", "Fortified"),
        ],
        "crime" => [
            (1, 1, "minimal", "Minimal"),
            (2, 4, "low", "Low"),
            (5, 8, "moderate", "Moderate"),
            (9, 11, "high", "High"),
            (12, 12, "rampant", "Rampant"),
        ],
        "stability" => [
            (1, 1, "volatile", "Volatile"),
            (2, 4, "unsettled", "Unsettled"),
            (5, 8, "stable", "Stable"),
            (9, 11, "cohesive", "Cohesive"),
            (12, 12, "unified", "Unified"),
        ],
        _ => unreachable!(),
    };
    rows.iter()
        .map(|(a, b, id, name)| (*a, *b, *id, *name, format!("{table}.{id}")))
        .collect()
}

// ----- the tables ---------------------------------------------------------------------

#[test]
fn each_table_is_an_ordered_1d12_with_rarer_extremes() {
    let c = content();
    for table in ["security", "crime", "stability"] {
        let t = c.table(&format!("table.{table}").as_str().into()).unwrap();
        assert_eq!(t.dice.as_deref(), Some("1d12"), "{table}");
        assert!(t.ordered, "{table} is an ordered scale");
        for face in 1..=12 {
            let claims = t
                .results
                .iter()
                .filter(|r| r.range.unwrap().contains(face))
                .count();
            assert_eq!(claims, 1, "{table}: face {face}");
        }
        let actual: Vec<_> = t
            .results
            .iter()
            .map(|r| {
                let range = r.range.unwrap();
                (
                    range.min,
                    range.max,
                    r.id.to_string(),
                    r.name.clone(),
                    r.tags[0].to_string(),
                )
            })
            .collect();
        let expected: Vec<_> = spec(table)
            .into_iter()
            .map(|(a, b, id, n, tag)| (a, b, id.to_owned(), n.to_owned(), tag))
            .collect();
        assert_eq!(actual, expected, "{table}");
        for r in &t.results {
            assert!(c.tag(&r.tags[0]).is_some(), "{} is defined", r.tags[0]);
        }
    }
}

#[test]
fn every_face_gives_its_result_and_tag() {
    let c = content();
    let generator = Generator::new(&c).unwrap();
    for table in ["security", "crime", "stability"] {
        let id = TableId::new(format!("table.{table}"));
        for (min, max, _, name, tag) in spec(table) {
            for face in min..=max {
                let mut dice = ScriptedFaces::new([face as u32]);
                let log = generator.run(std::slice::from_ref(&id), &mut dice).unwrap();
                assert_eq!(dice.remaining(), 0);
                let record = log.record_for(&id).unwrap();
                assert_eq!(record.dice.as_deref(), Some("1d12"));
                assert_eq!(record.attempts[0].roll.total, face);
                assert_eq!(record.result.name, name, "{table} {face}");
                let tags: Vec<_> = record.tags.iter().map(|t| t.id.to_string()).collect();
                assert_eq!(tags, std::slice::from_ref(&tag), "{table} {face}");
            }
        }
    }
}

#[test]
fn security_precedes_crime_which_precedes_stability_after_the_authority() {
    let names: Vec<_> = content()
        .pipeline()
        .into_iter()
        .map(|s| s.name.unwrap())
        .collect();
    let at = |n: &str| names.iter().position(|x| x == n).unwrap();
    assert_eq!(at("Security"), SECURITY);
    assert_eq!(at("Crime"), CRIME);
    assert_eq!(at("Stability"), STABILITY);
    assert!(at("Authority Figures") < at("Security"));
    assert_eq!(names.last().unwrap(), "Notable Locations");
}

// ----- modifiers: direction only ---------------------------------------------------------

/// `(table or concept, change)` of each planned intent on a tag.
fn intents(c: &ContentSet, tag: &str) -> Vec<(String, IntentChange)> {
    c.tag(&tag.into())
        .unwrap_or_else(|| panic!("{tag}"))
        .effects
        .iter()
        .filter_map(|d| match &d.effect {
            Effect::Intent {
                table,
                results,
                concept,
                change,
            } => {
                assert!(!d.is_active(), "{tag}: intents are never executed");
                let scale = table.as_ref().is_some_and(|t| {
                    matches!(
                        t.as_str(),
                        "table.security" | "table.crime" | "table.stability"
                    )
                });
                if scale {
                    assert!(results.is_empty(), "{tag}: a shift along the scale");
                }
                Some((
                    concept
                        .clone()
                        .unwrap_or_else(|| table.as_ref().unwrap().to_string()),
                    *change,
                ))
            }
            _ => None,
        })
        .collect()
}

fn on(c: &ContentSet, tag: &str, target: &str) -> Option<IntentChange> {
    let all: Vec<_> = intents(c, tag)
        .into_iter()
        .filter(|(t, _)| t == target)
        .collect();
    assert!(all.len() <= 1, "{tag} → {target}: {all:?}");
    all.first().map(|(_, c)| *c)
}

use IntentChange::{Decrease, Increase, StrongDecrease, StrongIncrease};

#[test]
fn origin_raises_security_and_security_moves_crime() {
    let c = content();
    assert_eq!(
        on(&c, "origin.strategicPosition", "table.security"),
        Some(Increase)
    );
    assert_eq!(
        on(&c, "origin.militaryOutpost", "table.security"),
        Some(Increase)
    );
    let crime = |tag: &str| on(&c, tag, "table.crime");
    assert_eq!(crime("security.undefended"), Some(StrongIncrease));
    assert_eq!(crime("security.lightlyDefended"), Some(Increase));
    assert_eq!(crime("security.guarded"), None);
    assert_eq!(crime("security.wellDefended"), Some(Decrease));
    assert_eq!(crime("security.fortified"), Some(StrongDecrease));
}

#[test]
fn economy_affects_crime_in_a_u_shape_and_stability_in_a_line() {
    let c = content();
    let both = |tier: &str| {
        let tag = format!("economy.{tier}");
        (on(&c, &tag, "table.crime"), on(&c, &tag, "table.stability"))
    };
    // Poor and rich settlements both see more crime; only poverty
    // unsettles.
    assert_eq!(
        both("failing"),
        (Some(StrongIncrease), Some(StrongDecrease))
    );
    assert_eq!(both("struggling"), (Some(Increase), Some(Decrease)));
    assert_eq!(both("stable"), (None, None));
    assert_eq!(both("prosperous"), (Some(Increase), Some(Increase)));
    assert_eq!(both("wealthy"), (Some(StrongIncrease), Some(Increase)));
}

#[test]
fn crime_moves_stability_and_feeds_future_systems() {
    let c = content();
    let stability = |tag: &str| on(&c, tag, "table.stability");
    assert_eq!(stability("crime.minimal"), Some(Increase));
    assert_eq!(stability("crime.low"), Some(Increase));
    assert_eq!(stability("crime.moderate"), None);
    assert_eq!(stability("crime.high"), Some(Decrease));
    assert_eq!(stability("crime.rampant"), Some(StrongDecrease));
    // Higher crime raises future systems that do not exist yet.
    for tag in ["crime.high", "crime.rampant"] {
        for concept in [
            "pressures",
            "opportunities",
            "criminal organisations / factions",
            "demand for guards / security",
            "crime-related noticeboard jobs",
            "crime-related buildings / services",
        ] {
            assert_eq!(on(&c, tag, concept), Some(Increase), "{tag} → {concept}");
        }
    }
    for tag in ["crime.minimal", "crime.low", "crime.moderate"] {
        assert_eq!(on(&c, tag, "opportunities"), None, "{tag}");
    }
}

#[test]
fn only_low_stability_creates_opportunities() {
    let c = content();
    let opportunities = |tag: &str| on(&c, tag, "opportunities");
    assert_eq!(opportunities("stability.volatile"), Some(StrongIncrease));
    assert_eq!(opportunities("stability.unsettled"), Some(Increase));
    for tag in [
        "stability.stable",
        "stability.cohesive",
        "stability.unified",
    ] {
        assert!(intents(&c, tag).is_empty(), "{tag}");
    }
    // Authority does not decide Stability.
    for tag in c
        .tags
        .iter()
        .filter(|t| t.id.as_str().starts_with("authority"))
    {
        assert_eq!(
            on(&c, tag.id.as_str(), "table.stability"),
            None,
            "{}",
            tag.id
        );
    }
}

#[test]
fn no_executable_modifier_touches_the_new_tables() {
    let c = content();
    for tag in &c.tags {
        for def in &tag.effects {
            let target = def.effect.target_table().map(|t| t.as_str());
            if matches!(
                target,
                Some("table.security" | "table.crime" | "table.stability")
            ) {
                assert!(
                    matches!(def.effect, Effect::Intent { .. }) && !def.is_active(),
                    "{}: only planned intents",
                    tag.id
                );
            }
        }
    }
}

// ----- generation --------------------------------------------------------------------------

fn one_table(god_mode: bool) -> GenerationSession {
    let mut s = GenerationSession::new(
        Arc::new(content()),
        GenerationOptions {
            god_mode,
            ..Default::default()
        },
        GenerationMode::OneTable,
    )
    .unwrap();
    s.choose(0, "kurovia".into(), &mut RandomFaces::seeded(1))
        .unwrap();
    s.roll_remaining(&mut RandomFaces::seeded(2)).unwrap();
    s
}

fn record_tags(s: &GenerationSession, table: &str) -> Vec<String> {
    s.log()
        .record_for(&TableId::new(table))
        .unwrap()
        .input_tags
        .iter()
        .map(|t| t.id.to_string())
        .collect()
}

#[test]
fn changing_security_keeps_crime_and_stability_with_current_traces() {
    let mut s = one_table(false);
    s.choose(SECURITY, "undefended".into(), &mut RandomFaces::seeded(3))
        .unwrap();
    let crime = s.view(CRIME, ViewOptions::default()).unwrap().result;
    assert!(record_tags(&s, "table.crime").contains(&"security.undefended".into()));

    // No approved numeric modifier: Crime and Stability stand, but their
    // traces name the Security they now follow, not the old one.
    let change = s
        .choose(SECURITY, "fortified".into(), &mut RandomFaces::seeded(4))
        .unwrap();
    assert!(change.invalidated.is_empty());
    assert_eq!(s.view(CRIME, ViewOptions::default()).unwrap().result, crime);
    for table in ["table.crime", "table.stability"] {
        let tags = record_tags(&s, table);
        assert!(
            tags.contains(&"security.fortified".into()),
            "{table}: {tags:?}"
        );
        assert!(
            !tags.contains(&"security.undefended".into()),
            "{table}: {tags:?}"
        );
    }
    // The stage's own routed records keep what they activated themselves.
    assert!(s.is_complete());
}

#[test]
fn god_mode_offers_every_result_and_keeps_tags() {
    let mut s = one_table(true);
    for (stage, table) in [
        (SECURITY, "security"),
        (CRIME, "crime"),
        (STABILITY, "stability"),
    ] {
        let v = s.view(stage, ViewOptions::default()).unwrap();
        assert_eq!(v.options.len(), 5);
        assert!(v.options.iter().all(|o| o.legal), "{table}");
        let (_, _, id, _, tag) = spec(table).remove(4);
        s.choose(stage, id.into(), &mut RandomFaces::seeded(1))
            .unwrap();
        assert!(s.log().has_tag(&tag.as_str().into()), "{tag}");
    }
}

#[test]
fn snapshots_keep_all_three_and_ignore_later_edits() {
    let s = one_table(false);
    let snapshot = s.to_snapshot();
    let stored: Vec<_> = snapshot
        .display_stages()
        .into_iter()
        .filter(|st| ["Security", "Crime", "Stability"].contains(&st.label.as_str()))
        .collect();
    assert_eq!(stored.len(), 3);
    for st in &stored {
        let result = st.result.as_ref().unwrap();
        let record = snapshot.log.record_for(&st.stage_table).unwrap();
        assert_eq!(&record.result, result);
        assert_eq!(record.dice.as_deref(), Some("1d12"));
        assert!((1..=12).contains(&record.attempts[0].roll.total));
        assert_eq!(st.tags.len(), 1);
    }

    // Later defaults: renamed results and different faces.
    let mut changed = content();
    for t in changed.tables.iter_mut().filter(|t| {
        matches!(
            t.id.as_str(),
            "table.security" | "table.crime" | "table.stability"
        )
    }) {
        let (a, b) = (t.results[0].range, t.results[4].range);
        t.results[0].range = b;
        t.results[4].range = a;
        for r in &mut t.results {
            r.name = format!("{} (edited)", r.name);
        }
    }
    let json = serde_json::to_string(&snapshot).unwrap();
    let back: SettlementSnapshot = serde_json::from_str(&json).unwrap();
    let reopened = GenerationSession::from_snapshot(
        Arc::new(changed),
        GenerationOptions::default(),
        GenerationMode::OneTable,
        &back,
    )
    .unwrap();
    assert_eq!(reopened.to_snapshot(), snapshot);
}

#[test]
fn settlements_saved_before_these_stages_load_with_them_pending() {
    let s = one_table(false);
    let mut old = s.to_snapshot();
    let added = |t: &str| matches!(t, "table.security" | "table.crime" | "table.stability");
    old.stages.retain(|st| !added(st.stage_table.as_str()));
    old.log.events.retain(|e| match e {
        settlement_core::engine::GenerationEvent::TableRolled(r) => !added(r.table.id.as_str()),
        _ => true,
    });
    assert!(old.display_stages().iter().all(|st| st.label != "Security"));
    let reopened = GenerationSession::from_snapshot(
        Arc::new(content()),
        GenerationOptions::default(),
        GenerationMode::OneTable,
        &old,
    )
    .unwrap();
    // Nothing is rolled on load: the new stages wait for the user.
    assert_eq!(reopened.next_pending(), Some(SECURITY));
}

// ----- editing -----------------------------------------------------------------------------

#[test]
fn the_tables_are_editable_and_resettable() {
    let mut s = AppService::new(Database::open_in_memory().unwrap());
    for table in ["table.security", "table.crime", "table.stability"] {
        let id = TableId::new(table);
        let detail = s.table_detail(&id).unwrap();
        assert!(detail.issues.is_empty(), "{table}: {:?}", detail.issues);
        let mut draft = detail.table.clone();
        draft.name = "Renamed".into();
        draft.results[0].name = "Renamed result".into();
        draft.results[0].enabled = false;
        let saved = s.save_table(draft).unwrap();
        assert_eq!(saved.table.name, "Renamed");
        assert!(saved.table.ordered, "the scale flag stays");
        // A layout out of scale order is allowed but warned about.
        let mut swapped = saved.table.clone();
        let (a, b) = (swapped.results[0].range, swapped.results[1].range);
        swapped.results[0].range = b;
        swapped.results[1].range = a;
        let check = s.check_table(swapped).unwrap();
        assert!(
            check
                .issues
                .iter()
                .any(|i| i.kind == IssueKind::ScaleOutOfOrder),
            "{table}"
        );
        s.reset_table(&id).unwrap();
        assert_eq!(
            &s.table_detail(&id).unwrap().table,
            content().table(&id).unwrap()
        );
    }
}

#[test]
fn customisations_survive_an_update_of_the_defaults() {
    let installed = content();
    let mut layers = ContentLayers::new(installed.clone(), UserOverrides::default());
    let mut crime = installed
        .table(&TableId::new("table.crime"))
        .unwrap()
        .clone();
    crime.results[4].name = "Lawless".into();
    layers.put_table(crime).unwrap();
    let overrides = layers.overrides().clone();

    let mut updated = installed;
    let crime = updated
        .tables
        .iter_mut()
        .find(|t| t.id.as_str() == "table.crime")
        .unwrap();
    crime.description = "Updated.".into();
    crime.results[0].name = "Negligible".into();
    let merged = ContentLayers::new(updated, overrides).merged();
    let table = merged.content.table(&TableId::new("table.crime")).unwrap();
    assert_eq!(table.results[4].name, "Lawless");
    assert_eq!(table.results[0].name, "Negligible");
    assert!(!merged.report.has_errors(), "{}", merged.report);
    assert!(
        table
            .result(&ResultId::new("rampant"))
            .is_some_and(|r| r.tags[0].as_str() == "crime.rampant")
    );
}
