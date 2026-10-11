//! Generation workflow (Automatic, Incremental, One-table, God Mode) over the
//! bundled v0.1.0 content.

use std::sync::Arc;

use settlement_core::bundled::bundled_content;
use settlement_core::content::ContentSet;
use settlement_core::dice::{RandomFaces, ScriptedFaces};
use settlement_core::engine::{
    AttemptOutcome, GenerationError, GenerationEvent, GenerationOptions, Selections,
};
use settlement_core::session::{
    GenerationMode, GenerationSession, StageStatus, StageView, ViewOptions,
};
use settlement_core::settings::{GenerationMethod, GenerationPresentation, Settings};

// Stage indices of the approved pipeline.
const PROFILE: usize = 0;
const REGION: usize = 1;
const ENVIRONMENT: usize = 2;
const CONNECTIONS: usize = 3;
const SIZE: usize = 4;
const ORIGIN: usize = 5;
const PURPOSE: usize = 6;
const AGE: usize = 7;

fn content() -> ContentSet {
    bundled_content().unwrap()
}

fn none() -> ScriptedFaces {
    ScriptedFaces::new([])
}

fn dice(values: &[u32]) -> ScriptedFaces {
    ScriptedFaces::new(values.iter().copied())
}

fn session(c: &ContentSet, mode: GenerationMode, god_mode: bool) -> GenerationSession {
    GenerationSession::new(Arc::new(c.clone()), GenerationOptions { god_mode }, mode).unwrap()
}

fn view(s: &GenerationSession, i: usize) -> StageView {
    s.view(i, ViewOptions::default()).unwrap()
}

fn result(s: &GenerationSession, i: usize) -> Option<String> {
    view(s, i).result.map(|r| r.name)
}

fn legal(v: &StageView) -> Vec<String> {
    v.options
        .iter()
        .filter(|o| o.legal)
        .map(|o| o.result.name.clone())
        .collect()
}

fn kurovia_selections() -> Selections {
    let mut s = Selections::new();
    s.insert("table.settingProfile".into(), "kurovia".into());
    s.insert("table.size".into(), "town".into());
    s
}

#[test]
fn mode_follows_settings() {
    let mut s = Settings::default();
    assert_eq!(GenerationMode::from_settings(&s), GenerationMode::Automatic);
    s.generation_method = GenerationMethod::Manual;
    assert_eq!(
        GenerationMode::from_settings(&s),
        GenerationMode::Incremental
    );
    s.generation_presentation = GenerationPresentation::OneTable;
    assert_eq!(GenerationMode::from_settings(&s), GenerationMode::OneTable);
}

// ----- Automatic -------------------------------------------------------------

#[test]
fn automatic_resolves_every_stage_with_real_dice() {
    let c = content();
    let s = GenerationSession::automatic(
        Arc::new(c.clone()),
        GenerationOptions::default(),
        &kurovia_selections(),
        &mut RandomFaces::seeded(11),
    )
    .unwrap();
    assert!(s.is_complete());
    let views = s.views(ViewOptions::default()).unwrap();
    let names: Vec<_> = views.iter().map(|v| v.stage.name.as_str()).collect();
    assert_eq!(
        names,
        [
            "Setting Profile",
            "Region",
            "Environment",
            "Connections",
            "Size",
            "Origin",
            "Purpose",
            "Age"
        ]
    );
    assert!(views.iter().all(|v| v.status == StageStatus::Resolved));
    // Every rolled stage recorded its dice.
    let log = s.log();
    let rolled: Vec<_> = log.table_rolls().filter(|r| !r.selected).collect();
    assert_eq!(rolled.len(), 6);
    assert!(
        rolled
            .iter()
            .all(|r| r.accepted().is_some() && r.dice.is_some())
    );
}

#[test]
fn automatic_is_reproducible_with_a_seed() {
    let c = content();
    let run = |seed| {
        GenerationSession::automatic(
            Arc::new(c.clone()),
            GenerationOptions::default(),
            &kurovia_selections(),
            &mut RandomFaces::seeded(seed),
        )
        .unwrap()
        .log()
    };
    assert_eq!(run(5), run(5));
}

#[test]
fn automatic_stops_on_incomplete_content_instead_of_inventing_it() {
    let c = content();
    let mut selections = Selections::new();
    selections.insert("table.settingProfile".into(), "kurovia".into());
    let err = GenerationSession::automatic(
        Arc::new(c.clone()),
        GenerationOptions::default(),
        &selections,
        &mut RandomFaces::seeded(1),
    )
    .err()
    .unwrap();
    assert!(
        err.to_string().starts_with("Size cannot be generated yet"),
        "{err}"
    );
}

#[test]
fn automatic_needs_a_setting_profile() {
    let c = content();
    let err = GenerationSession::automatic(
        Arc::new(c.clone()),
        GenerationOptions::default(),
        &Selections::new(),
        &mut RandomFaces::seeded(1),
    )
    .err()
    .unwrap();
    assert!(matches!(err, GenerationError::SelectionRequired { .. }));
}

// ----- Incremental -----------------------------------------------------------

#[test]
fn incremental_presents_one_stage_at_a_time() {
    let c = content();
    let mut s = session(&c, GenerationMode::Incremental, false);
    let first = view(&s, PROFILE);
    assert!(first.current && first.actionable);
    assert!(!first.can_roll, "Setting Profile is chosen, not rolled");
    assert_eq!(legal(&first).len(), 6);
    assert!(!view(&s, REGION).actionable);

    // Only the current stage can be acted on, and it must be done first.
    assert!(matches!(
        s.roll(REGION, &mut none()),
        Err(GenerationError::NotCurrentStage { .. })
    ));
    assert!(matches!(
        s.advance(),
        Err(GenerationError::StageNotResolved { .. })
    ));

    s.choose(PROFILE, "kurovia".into(), &mut none()).unwrap();
    s.advance().unwrap();
    let region = view(&s, REGION);
    assert_eq!(region.table.name, "Kurovia Region");
    assert_eq!(legal(&region).len(), 8);
    s.roll(REGION, &mut dice(&[4])).unwrap(); // Shinkira
    assert_eq!(result(&s, REGION).as_deref(), Some("Shinkira"));
    s.advance().unwrap();

    assert_eq!(
        view(&s, ENVIRONMENT).table.name,
        "Kurovia Environment: Shinkira"
    );
    s.roll(ENVIRONMENT, &mut dice(&[15])).unwrap(); // Coast
    s.advance().unwrap();

    let conn = view(&s, CONNECTIONS);
    assert_eq!(
        legal(&conn),
        ["Isolated", "Local Road", "Regional Road"],
        "Major Route is not on the Shinkira layout"
    );
    let major = conn
        .options
        .iter()
        .find(|o| o.result.name == "Major Route")
        .unwrap();
    assert_eq!(major.reason, Some(AttemptOutcome::NotInLayout));
    s.roll(CONNECTIONS, &mut dice(&[6, 4])).unwrap(); // Local Road, upgrade
    assert_eq!(result(&s, CONNECTIONS).as_deref(), Some("Major Route"));
}

#[test]
fn changing_an_earlier_stage_invalidates_dependent_stages_only() {
    let c = content();
    let mut s = session(&c, GenerationMode::Incremental, false);
    s.choose(PROFILE, "kurovia".into(), &mut none()).unwrap();
    s.advance().unwrap();
    s.roll(REGION, &mut dice(&[4])).unwrap(); // Shinkira
    s.advance().unwrap();
    s.roll(ENVIRONMENT, &mut dice(&[15])).unwrap(); // Coast
    s.advance().unwrap();
    s.roll(CONNECTIONS, &mut dice(&[6, 4])).unwrap(); // → Major Route
    s.advance().unwrap();
    s.choose(SIZE, "town".into(), &mut none()).unwrap();
    s.advance().unwrap();
    s.roll(ORIGIN, &mut dice(&[1])).unwrap();
    s.advance().unwrap();
    s.roll(PURPOSE, &mut dice(&[1])).unwrap();
    s.advance().unwrap();
    s.roll(AGE, &mut dice(&[1])).unwrap();
    assert!(s.is_complete());

    // Go back to Environment and change Coast to Woodland: the coastal
    // upgrade no longer applies, so Connections is invalidated. Size,
    // Origin, Purpose and Age do not depend on it and are kept.
    for _ in 0..(AGE - ENVIRONMENT) {
        s.back();
    }
    assert_eq!(s.cursor(), ENVIRONMENT);
    let change = s
        .choose(ENVIRONMENT, "woodland".into(), &mut none())
        .unwrap();
    assert_eq!(change.invalidated, [CONNECTIONS]);
    let conn = view(&s, CONNECTIONS);
    assert_eq!(conn.status, StageStatus::Pending);
    assert_eq!(conn.previous.unwrap().name, "Major Route");
    assert_eq!(result(&s, AGE).as_deref(), Some("New"));
    assert!(!s.is_complete());

    // Changing the Region changes which Environment table applies.
    s.back();
    let change = s.roll(REGION, &mut dice(&[1])).unwrap(); // Ordon
    assert_eq!(change.invalidated, [ENVIRONMENT]);
    assert_eq!(
        view(&s, ENVIRONMENT).table.name,
        "Kurovia Environment: Ordon"
    );
    assert_eq!(view(&s, ENVIRONMENT).previous.unwrap().name, "Woodland");
}

#[test]
fn failed_actions_leave_the_session_unchanged() {
    let c = content();
    let mut s = session(&c, GenerationMode::OneTable, false);
    s.choose(PROFILE, "kurovia".into(), &mut none()).unwrap();
    let before = s.log();
    // Size has no distribution, so it cannot be rolled.
    let err = s.roll(SIZE, &mut RandomFaces::seeded(1)).unwrap_err();
    assert!(matches!(err, GenerationError::IncompleteContent { .. }));
    assert!(!view(&s, SIZE).can_roll);
    assert!(view(&s, SIZE).incomplete.is_some());
    // Not a legal Kurovia region result.
    assert!(s.choose(REGION, "coldlands".into(), &mut none()).is_err());
    assert_eq!(s.log(), before);
}

// ----- One-table -------------------------------------------------------------

#[test]
fn one_table_allows_any_stage_and_rechecks_dependants() {
    let c = content();
    let mut s = session(&c, GenerationMode::OneTable, false);
    let views = s.views(ViewOptions::default()).unwrap();
    assert_eq!(views.len(), 8);
    assert!(views.iter().all(|v| v.actionable));

    // Connections rolled before a Region exists: Standard layout.
    s.roll(CONNECTIONS, &mut dice(&[18])).unwrap();
    assert_eq!(result(&s, CONNECTIONS).as_deref(), Some("Major Route"));
    s.roll(ORIGIN, &mut dice(&[20])).unwrap();

    // Choosing Kurovia changes the Region table but not Connections yet.
    let change = s.choose(PROFILE, "kurovia".into(), &mut none()).unwrap();
    assert!(change.invalidated.is_empty());

    // Targoth selects a different Connections layout: Connections must be
    // redone; Origin is independent and kept.
    let change = s.roll(REGION, &mut dice(&[8])).unwrap();
    assert_eq!(change.invalidated, [CONNECTIONS]);
    assert_eq!(result(&s, ORIGIN).as_deref(), Some("Organic Growth"));

    // Roll everything still pending (Size must be chosen first).
    s.choose(SIZE, "village".into(), &mut none()).unwrap();
    s.roll_remaining(&mut RandomFaces::seeded(3)).unwrap();
    assert!(s.is_complete());
    assert_ne!(result(&s, CONNECTIONS).as_deref(), Some("Major Route"));
}

#[test]
fn rerolling_a_stage_keeps_independent_later_stages() {
    let c = content();
    let mut s = session(&c, GenerationMode::OneTable, false);
    s.choose(PROFILE, "kurovia".into(), &mut none()).unwrap();
    s.choose(SIZE, "city".into(), &mut none()).unwrap();
    s.roll_remaining(&mut RandomFaces::seeded(9)).unwrap();
    let purpose = result(&s, PURPOSE);
    let change = s.roll(ORIGIN, &mut RandomFaces::seeded(10)).unwrap();
    assert!(change.invalidated.is_empty());
    assert_eq!(result(&s, PURPOSE), purpose);
}

// ----- God Mode --------------------------------------------------------------

#[test]
fn god_mode_makes_every_result_selectable_and_keeps_its_tags() {
    let c = content();
    for god_mode in [false, true] {
        let mut s = session(&c, GenerationMode::OneTable, god_mode);
        s.choose(PROFILE, "kurovia".into(), &mut none()).unwrap();
        s.roll(REGION, &mut dice(&[8])).unwrap(); // Targoth
        let conn = view(&s, CONNECTIONS);
        let choose_major = s.choose(CONNECTIONS, "majorRoute".into(), &mut none());
        if god_mode {
            assert_eq!(legal(&conn).len(), 4);
            choose_major.unwrap();
            let v = s
                .view(
                    CONNECTIONS,
                    ViewOptions {
                        show_tags: true,
                        debug: false,
                    },
                )
                .unwrap();
            assert_eq!(v.result.unwrap().name, "Major Route");
            assert_eq!(v.tags.unwrap()[0].id.as_str(), "connection.majorRoute");
            assert!(s.log().options.god_mode);
        } else {
            assert_eq!(legal(&conn).len(), 3);
            assert!(matches!(
                choose_major,
                Err(GenerationError::SelectionNotAllowed {
                    reason: AttemptOutcome::NotInLayout,
                    ..
                })
            ));
        }
    }
}

#[test]
fn god_mode_still_applies_downstream_rules() {
    // Choosing Coast in Shinkira under God Mode still triggers the coastal
    // follow-up roll on Connections.
    let c = content();
    let mut s = session(&c, GenerationMode::OneTable, true);
    s.choose(PROFILE, "kurovia".into(), &mut none()).unwrap();
    s.choose(REGION, "shinkira".into(), &mut none()).unwrap();
    s.choose(ENVIRONMENT, "coast".into(), &mut none()).unwrap();
    s.choose(CONNECTIONS, "localRoad".into(), &mut dice(&[4]))
        .unwrap();
    assert_eq!(result(&s, CONNECTIONS).as_deref(), Some("Major Route"));
}

// ----- views: Tag View and debug ----------------------------------------------

#[test]
fn tags_and_details_are_only_included_when_requested() {
    let c = content();
    let mut s = session(&c, GenerationMode::OneTable, false);
    s.choose(PROFILE, "kurovia".into(), &mut none()).unwrap();
    s.roll(REGION, &mut dice(&[1])).unwrap();

    let plain = view(&s, REGION);
    assert!(plain.tags.is_none() && plain.details.is_none());
    assert!(plain.options.iter().all(|o| o.tags.is_empty()));

    let tagged = s
        .view(
            REGION,
            ViewOptions::from_settings(&Settings {
                tag_view: true,
                ..Settings::default()
            }),
        )
        .unwrap();
    assert_eq!(tagged.tags.unwrap()[0].id.as_str(), "region.ordon");
    assert!(tagged.details.is_none());

    let debug = s
        .view(
            REGION,
            ViewOptions {
                show_tags: false,
                debug: true,
            },
        )
        .unwrap();
    let details = debug.details.unwrap();
    assert_eq!(details.input_tags[0].id.as_str(), "world.kurovia");
    let GenerationEvent::TableRolled(record) = &details.events[0] else {
        panic!("expected a roll");
    };
    // Reproducibility: input tags, redirect, dice, roll, result, emitted tags.
    assert_eq!(record.input_tags[0].id.as_str(), "world.kurovia");
    assert_eq!(
        record.redirected_from.as_ref().unwrap().name,
        "Generic Region"
    );
    assert_eq!(record.dice.as_deref(), Some("1d8"));
    assert_eq!(record.accepted().unwrap().roll.total, 1);
    assert_eq!(record.result.name, "Ordon");
    assert_eq!(record.tags[0].id.as_str(), "region.ordon");
}

#[test]
fn session_log_is_a_saveable_snapshot() {
    let c = content();
    let s = GenerationSession::automatic(
        Arc::new(c.clone()),
        GenerationOptions::default(),
        &kurovia_selections(),
        &mut RandomFaces::seeded(21),
    )
    .unwrap();
    let log = s.log();
    assert_eq!(log.table_rolls().count(), 8);
    assert!(log.has_tag(&"world.kurovia".into()));
    let json = serde_json::to_string(&log).unwrap();
    assert_eq!(
        serde_json::from_str::<settlement_core::engine::GenerationLog>(&json).unwrap(),
        log
    );
}

#[test]
fn stage_label_stays_while_the_table_is_redirected() {
    let c = content();
    let mut s = session(&c, GenerationMode::OneTable, false);
    s.choose(PROFILE, "kurovia".into(), &mut none()).unwrap();
    let region = view(&s, REGION);
    assert_eq!(region.stage.name, "Region");
    assert_eq!(region.table.name, "Kurovia Region");
}

// ----- resuming Automatic and rerolling -----------------------------------------

#[test]
fn automatic_can_resume_after_a_blocked_stage_is_chosen() {
    let c = content();
    let mut s = session(&c, GenerationMode::Automatic, false);
    let mut profile = Selections::new();
    profile.insert("table.settingProfile".into(), "kurovia".into());
    let err = s
        .run_automatic(&profile, &mut RandomFaces::seeded(4))
        .unwrap_err();
    assert!(matches!(err, GenerationError::IncompleteContent { .. }));
    // Earlier stages are kept; Size is where it stopped.
    assert_eq!(s.next_pending(), Some(SIZE));
    assert_eq!(view(&s, CONNECTIONS).status, StageStatus::Resolved);

    s.choose(SIZE, "town".into(), &mut none()).unwrap();
    s.run_automatic(&Selections::new(), &mut RandomFaces::seeded(4))
        .unwrap();
    assert!(s.is_complete());
    assert_eq!(s.next_pending(), None);
}

#[test]
fn reroll_all_keeps_only_unrollable_choices() {
    let c = content();
    let mut s = session(&c, GenerationMode::OneTable, false);
    s.choose(PROFILE, "kurovia".into(), &mut none()).unwrap();
    s.choose(REGION, "targoth".into(), &mut none()).unwrap();
    s.choose(SIZE, "capital".into(), &mut none()).unwrap();
    s.roll_remaining(&mut RandomFaces::seeded(1)).unwrap();
    let before = s.log();

    s.reroll_all(&mut RandomFaces::seeded(2)).unwrap();
    assert!(s.is_complete());
    assert_ne!(s.log(), before);
    assert_eq!(result(&s, PROFILE).as_deref(), Some("Kurovia"));
    assert_eq!(result(&s, SIZE).as_deref(), Some("Capital"));
    // Region was chosen on a rollable table, so it is rolled afresh.
    let region = s.log();
    let record = region.record_for(&"table.kuroviaRegion".into()).unwrap();
    assert!(!record.selected);
}

#[test]
fn session_snapshot_includes_active_tags_only_with_tag_view() {
    let c = content();
    let mut s = session(&c, GenerationMode::OneTable, false);
    s.choose(PROFILE, "kurovia".into(), &mut none()).unwrap();
    let plain = s.snapshot(ViewOptions::default()).unwrap();
    assert!(plain.active_tags.is_none());
    assert_eq!(plain.next_pending, Some(REGION));
    let tagged = s
        .snapshot(ViewOptions {
            show_tags: true,
            debug: false,
        })
        .unwrap();
    assert_eq!(tagged.active_tags.unwrap()[0].id.as_str(), "world.kurovia");
    assert_eq!(tagged.stages.len(), 8);
}

#[test]
fn stage_outcome_summarises_rolls_choices_and_follow_ups() {
    let c = content();
    let mut s = session(&c, GenerationMode::OneTable, false);
    s.choose(PROFILE, "kurovia".into(), &mut none()).unwrap();
    s.roll(REGION, &mut dice(&[4])).unwrap(); // Shinkira
    s.roll(ENVIRONMENT, &mut dice(&[15])).unwrap(); // Coast
    s.roll(CONNECTIONS, &mut dice(&[6, 4])).unwrap(); // upgrade

    let chosen = view(&s, PROFILE).outcome.unwrap();
    assert!(chosen.chosen && chosen.dice.is_none());

    let conn = view(&s, CONNECTIONS).outcome.unwrap();
    assert_eq!(conn.dice.as_deref(), Some("1d20"));
    assert_eq!(conn.roll.as_deref(), Some("6"));
    assert_eq!(conn.follow_ups.len(), 1);
    assert_eq!(conn.follow_ups[0].roll, "4");
    assert_eq!(conn.follow_ups[0].becomes.as_deref(), Some("Major Route"));
    assert!(view(&s, AGE).outcome.is_none());
}

#[test]
fn stage_views_report_the_dice_a_roll_would_use() {
    let c = content();
    let mut s = session(&c, GenerationMode::OneTable, false);
    assert_eq!(view(&s, PROFILE).dice, None);
    assert_eq!(view(&s, REGION).dice.as_deref(), Some("1d10"));
    s.choose(PROFILE, "kurovia".into(), &mut none()).unwrap();
    assert_eq!(view(&s, REGION).dice.as_deref(), Some("1d8"));
    assert_eq!(view(&s, SIZE).dice, None);
}

// ----- saved snapshots: display and editing ----------------------------------------

use settlement_core::layers::{ContentLayers, UserOverrides};
use settlement_core::settlements::SettlementSnapshot;

fn saved_settlement(c: &ContentSet) -> SettlementSnapshot {
    let mut selections = Selections::new();
    selections.insert("table.settingProfile".into(), "kurovia".into());
    selections.insert("table.size".into(), "town".into());
    GenerationSession::automatic(
        Arc::new(c.clone()),
        GenerationOptions::default(),
        &selections,
        &mut RandomFaces::seeded(31),
    )
    .unwrap()
    .to_snapshot()
}

#[test]
fn snapshots_record_every_stage_for_display() {
    let snapshot = saved_settlement(&content());
    let stages = snapshot.display_stages();
    let labels: Vec<_> = stages.iter().map(|s| s.label.as_str()).collect();
    assert_eq!(
        labels,
        [
            "Setting Profile",
            "Region",
            "Environment",
            "Connections",
            "Size",
            "Origin",
            "Purpose",
            "Age"
        ]
    );
    assert_eq!(stages[1].table.as_ref().unwrap().name, "Kurovia Region");
    assert!(stages.iter().all(|s| s.result.is_some()));
    assert_eq!(stages[0].tags[0].id.as_str(), "world.kurovia");
}

#[test]
fn reopening_a_snapshot_changes_nothing() {
    let c = content();
    let snapshot = saved_settlement(&c);
    let s = GenerationSession::from_snapshot(
        Arc::new(c),
        GenerationOptions::default(),
        GenerationMode::OneTable,
        &snapshot,
    )
    .unwrap();
    assert!(s.is_complete());
    assert_eq!(s.to_snapshot(), snapshot);
}

#[test]
fn changed_defaults_never_silently_regenerate_a_saved_settlement() {
    let c = content();
    let snapshot = saved_settlement(&c);

    // Defaults change afterwards: renamed tables, results and tags, and
    // different Connections dice.
    let mut layers = ContentLayers::new(c, UserOverrides::default());
    layers
        .rename_table(&"table.connections".into(), "Roads")
        .unwrap();
    layers
        .rename_tag(&"world.kurovia".into(), "Kurovia (renamed)")
        .unwrap();
    layers
        .rename_result(&"table.age".into(), &"ancient".into(), "Very Old")
        .unwrap();
    let changed = layers.merged().content;

    let s = GenerationSession::from_snapshot(
        Arc::new(changed),
        GenerationOptions::default(),
        GenerationMode::OneTable,
        &snapshot,
    )
    .unwrap();
    assert!(s.is_complete(), "nothing was invalidated");
    assert_eq!(s.to_snapshot(), snapshot, "saving unchanged keeps it exact");
}

#[test]
fn editing_one_stage_revises_only_what_depends_on_it() {
    let c = content();
    let snapshot = saved_settlement(&c);
    let mut s = GenerationSession::from_snapshot(
        Arc::new(c),
        GenerationOptions::default(),
        GenerationMode::OneTable,
        &snapshot,
    )
    .unwrap();

    // Choose a different Origin: nothing else depends on it.
    let origin = view(&s, ORIGIN).result.unwrap().id;
    let other = if origin.as_str() == "tradeStop" {
        "estateSeat"
    } else {
        "tradeStop"
    };
    let change = s.choose(ORIGIN, other.into(), &mut none()).unwrap();
    assert!(change.invalidated.is_empty());
    let revised = s.to_snapshot();
    assert_ne!(revised, snapshot);
    for (i, (before, after)) in snapshot.stages.iter().zip(&revised.stages).enumerate() {
        if i == ORIGIN {
            assert_eq!(after.result.as_ref().unwrap().id.as_str(), other);
        } else {
            assert_eq!(before, after, "stage {i} must be unchanged");
        }
    }

    // Choosing a different Region invalidates Environment (another table).
    let region = view(&s, REGION).result.unwrap().id;
    let other = if region.as_str() == "ordon" {
        "halingo"
    } else {
        "ordon"
    };
    let change = s.choose(REGION, other.into(), &mut none()).unwrap();
    assert!(change.invalidated.contains(&ENVIRONMENT));
    assert!(!s.is_complete());
}

#[test]
fn snapshots_without_stage_records_still_display_and_reopen() {
    let c = content();
    let mut legacy = saved_settlement(&c);
    legacy.stages.clear();
    let shown = legacy.display_stages();
    assert_eq!(shown.len(), 8);
    assert_eq!(shown[1].label, "Generic Region");
    let s = GenerationSession::from_snapshot(
        Arc::new(c),
        GenerationOptions::default(),
        GenerationMode::OneTable,
        &legacy,
    )
    .unwrap();
    assert!(s.is_complete());
}

#[test]
fn god_mode_exposes_valid_results_but_never_disabled_or_deleted_ones() {
    let mut layers = ContentLayers::new(content(), UserOverrides::default());
    // Disable one Purpose result and delete another (customisations).
    layers
        .set_result_enabled(&"table.purpose".into(), &"scholarly".into(), false)
        .unwrap();
    layers
        .delete_result(&"table.purpose".into(), &"military".into())
        .unwrap();
    layers
        .set_result_range(
            &"table.purpose".into(),
            &"religious".into(),
            Some(settlement_core::content::FaceRange::single(16)),
        )
        .unwrap();
    layers
        .set_result_range(
            &"table.purpose".into(),
            &"transportWaystation".into(),
            Some(settlement_core::content::FaceRange::new(18, 19)),
        )
        .unwrap();
    let merged = layers.merged();
    assert!(!merged.report.has_errors(), "{}", merged.report);

    let s = session(&merged.content, GenerationMode::OneTable, true);
    let purpose = view(&s, PURPOSE);
    let names: Vec<_> = purpose
        .options
        .iter()
        .map(|o| o.result.name.as_str())
        .collect();
    assert!(!names.contains(&"Military"), "deleted results are gone");
    let scholarly = purpose
        .options
        .iter()
        .find(|o| o.result.name == "Scholarly")
        .unwrap();
    assert!(!scholarly.legal, "disabled stays unavailable in God Mode");
    assert_eq!(scholarly.reason, Some(AttemptOutcome::Disabled));
    assert_eq!(legal(&purpose).len(), 9);
}

#[test]
fn content_without_stages_is_refused_rather_than_crashing_an_interface() {
    let mut empty: ContentSet = bundled_content().unwrap();
    empty.sequence.clear();
    let result = GenerationSession::new(
        Arc::new(empty),
        GenerationOptions { god_mode: false },
        GenerationMode::OneTable,
    );
    assert!(matches!(result, Err(GenerationError::EmptySequence)));
}
