//! Generation workflow (Automatic, Incremental, One-table, God Mode) over the
//! bundled v0.1.0 content.

use std::sync::Arc;

use settlement_core::bundled::bundled_content;
use settlement_core::content::{ContentSet, ContentStatus};
use settlement_core::dice::{RandomFaces, ScriptedFaces};
use settlement_core::engine::{
    AttemptOutcome, GenerationError, GenerationEvent, GenerationOptions, Selections, SkipReason,
};
use settlement_core::session::{
    GenerationMode, GenerationSession, StageStatus, StageView, ViewOptions,
};
use settlement_core::settings::{GenerationMethod, GenerationPresentation, Settings};

/// Kurovian regions with naming data: all eight.
const NAMED_REGIONS: [&str; 8] = [
    "Ordon",
    "Halingo",
    "Skgurr",
    "Shinkira",
    "Bratislor",
    "Hjartheland",
    "Nareth",
    "Targoth",
];

// Stage indices of the approved pipeline.
const PROFILE: usize = 0;
const REGION: usize = 1;
const NAME: usize = 2;
const ENVIRONMENT: usize = 3;
const CONNECTIONS: usize = 4;
const SIZE: usize = 5;
const ORIGIN: usize = 6;
const PURPOSE: usize = 7;
const AGE: usize = 8;
const ECONOMY: usize = 9;
const AUTHORITY: usize = 10;
const REPRESENTATION: usize = 11;
const AUTHORITY_SIZE: usize = 12;
const ROLES: usize = 13;
const FIGURES: usize = 14;
const SECURITY: usize = 15;
const CRIME: usize = 16;
const STABILITY: usize = 17;
const LOCATIONS: usize = 18;

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
    GenerationSession::new(
        Arc::new(c.clone()),
        GenerationOptions {
            god_mode,
            ..Default::default()
        },
        mode,
    )
    .unwrap()
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
    s
}

/// The bundled content with Size's distribution removed, to exercise
/// incomplete content at a stage that has results to choose from.
fn with_incomplete_size(mut c: ContentSet) -> ContentSet {
    let size = c
        .tables
        .iter_mut()
        .find(|t| t.id.as_str() == "table.size")
        .unwrap();
    size.status = ContentStatus::Incomplete;
    size.status_note = "Test fixture.".into();
    size.dice = None;
    for r in &mut size.results {
        r.range = None;
    }
    c
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
            "Name",
            "Environment",
            "Connections",
            "Size",
            "Origin",
            "Purpose",
            "Age",
            "Economy",
            "Authority",
            "Representation",
            "Authority Size",
            "Roles",
            "Authority Figures",
            "Security",
            "Crime",
            "Stability",
            "Notable Locations"
        ]
    );
    // Every stage is resolved, except Name, which is skipped unless the
    // region has a naming scheme, and the Authority sub-stages, which are
    // skipped where they do not apply.
    let ordon = NAMED_REGIONS.contains(&views[REGION].result.as_ref().unwrap().name.as_str());
    let mut sub_rolls = 0;
    for v in &views {
        if v.index == NAME && !ordon {
            assert_eq!(v.skipped, Some(SkipReason::NoNamingScheme));
        } else if v.sub_stage && v.status == StageStatus::Skipped {
            assert_eq!(v.skipped, Some(SkipReason::NotEligible));
        } else {
            // Role Allocation keeps its rolls in its own record, not the log.
            sub_rolls += usize::from(v.sub_stage && v.roles.is_none());
            assert_eq!(v.status, StageStatus::Resolved, "{}", v.stage.name);
        }
    }
    // Every rolled stage recorded its dice.
    let log = s.log();
    let rolled: Vec<_> = log.table_rolls().filter(|r| !r.selected).collect();
    // Region, Environment, Connections, Size, Origin, Purpose, Age,
    // Economy, Authority, Security, Crime and Stability, plus the Authority
    // sub-stages that applied.
    assert_eq!(rolled.len(), 12 + sub_rolls);
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
    selections.insert("table.settingProfile".into(), "lowFantasy".into());
    let err = GenerationSession::automatic(
        Arc::new(c.clone()),
        GenerationOptions::default(),
        &selections,
        &mut RandomFaces::seeded(1),
    )
    .err()
    .unwrap();
    assert!(
        err.to_string()
            .starts_with("Environment cannot be generated yet"),
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
    // Shinkira has a naming scheme: Hana + sawa.
    s.roll(NAME, &mut dice(&[1, 7, 4])).unwrap();
    assert_eq!(result(&s, NAME).as_deref(), Some("Hanasawa"));
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
    s.roll(NAME, &mut dice(&[1, 7, 4])).unwrap(); // Kirisawa
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
    s.advance().unwrap();
    s.roll(ECONOMY, &mut dice(&[9])).unwrap(); // Prosperous
    s.advance().unwrap();
    s.roll(AUTHORITY, &mut dice(&[12])).unwrap(); // Popular Assembly
    // A group: Representation is decided and passed over; Authority Size
    // is next.
    assert_eq!(s.advance().unwrap(), AUTHORITY_SIZE);
    s.roll(AUTHORITY_SIZE, &mut dice(&[3])).unwrap(); // 3 Representatives
    // Roles are allocated before the figures are generated.
    assert_eq!(s.advance().unwrap(), ROLES);
    s.roll(ROLES, &mut RandomFaces::seeded(5)).unwrap();
    s.advance().unwrap();
    s.roll(FIGURES, &mut RandomFaces::seeded(5)).unwrap();
    // Then Security, Crime and Stability.
    for (stage, face) in [(SECURITY, 6), (CRIME, 6), (STABILITY, 10)] {
        assert_eq!(s.advance().unwrap(), stage);
        s.roll(stage, &mut dice(&[face])).unwrap();
    }
    assert_eq!(s.advance().unwrap(), LOCATIONS);
    s.roll(LOCATIONS, &mut RandomFaces::seeded(6)).unwrap();
    assert!(s.is_complete());

    // Go back to Environment and change Coast to Woodland: the coastal
    // upgrade no longer applies, so Connections is invalidated. Size,
    // Origin, Purpose, Age, Economy, Authority, Security, Crime and
    // Stability do not depend on it and are kept. Going back passes over
    // the skipped Representation.
    for stage in [STABILITY, CRIME, SECURITY, FIGURES] {
        assert_eq!(s.back(), stage);
    }
    assert_eq!(s.back(), ROLES);
    assert_eq!(s.back(), AUTHORITY_SIZE);
    s.back();
    assert_eq!(s.cursor(), AUTHORITY);
    while s.cursor() > ENVIRONMENT {
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
    assert_eq!(result(&s, ECONOMY).as_deref(), Some("Prosperous"));
    assert_eq!(result(&s, AUTHORITY).as_deref(), Some("Popular Assembly"));
    assert!(!s.is_complete());

    // Changing the Region changes which Environment table applies, and
    // which naming scheme names the settlement: the Shinkira name is
    // invalidated (and shown as "was Hanasawa").
    s.back();
    s.back();
    let change = s.roll(REGION, &mut dice(&[1])).unwrap(); // Ordon
    assert_eq!(change.invalidated, [NAME, ENVIRONMENT]);
    assert_eq!(view(&s, NAME).status, StageStatus::Pending);
    assert_eq!(view(&s, NAME).previous.unwrap().name, "Hanasawa");
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
    // Without a Kurovia region, Environment has no designed content, so it
    // cannot be rolled.
    let err = s
        .roll(ENVIRONMENT, &mut RandomFaces::seeded(1))
        .unwrap_err();
    assert!(matches!(err, GenerationError::IncompleteContent { .. }));
    assert!(!view(&s, ENVIRONMENT).can_roll);
    assert!(view(&s, ENVIRONMENT).incomplete.is_some());
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
    assert_eq!(views.len(), 19);
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

    // Roll everything still pending.
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
    assert_eq!(log.table_rolls().count(), 14);
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
    let c = with_incomplete_size(content());
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
    // Region and Size were chosen on rollable tables, so they are rolled
    // afresh.
    let log = s.log();
    for table in ["table.kuroviaRegion", "table.size"] {
        assert!(!log.record_for(&table.into()).unwrap().selected, "{table}");
    }
}

#[test]
fn reroll_all_keeps_a_choice_on_incomplete_content() {
    let c = with_incomplete_size(content());
    let mut s = session(&c, GenerationMode::OneTable, false);
    s.choose(PROFILE, "kurovia".into(), &mut none()).unwrap();
    s.choose(SIZE, "capital".into(), &mut none()).unwrap();
    s.roll_remaining(&mut RandomFaces::seeded(1)).unwrap();
    s.reroll_all(&mut RandomFaces::seeded(2)).unwrap();
    assert_eq!(result(&s, SIZE).as_deref(), Some("Capital"));
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
    assert_eq!(tagged.stages.len(), 19);
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
    assert_eq!(view(&s, SIZE).dice.as_deref(), Some("1d10"));
}

#[test]
fn size_can_be_rolled_or_chosen_in_every_manual_mode() {
    let c = content();
    let all = ["Trading Post", "Village", "Town", "City", "Capital"];
    for mode in [GenerationMode::Incremental, GenerationMode::OneTable] {
        let mut s = session(&c, mode, false);
        let next = |s: &mut GenerationSession| {
            if mode == GenerationMode::Incremental {
                s.advance().unwrap();
            }
        };
        s.choose(PROFILE, "kurovia".into(), &mut none()).unwrap();
        next(&mut s);
        // Resolve the stages before Size in order (Region 1 = Ordon, which
        // has a naming scheme).
        s.roll(REGION, &mut dice(&[1])).unwrap();
        next(&mut s);
        s.roll(NAME, &mut RandomFaces::seeded(1)).unwrap();
        next(&mut s);
        s.roll(ENVIRONMENT, &mut dice(&[1])).unwrap();
        next(&mut s);
        s.roll(CONNECTIONS, &mut dice(&[1])).unwrap();
        next(&mut s);
        let v = view(&s, SIZE);
        assert!(v.can_roll && v.incomplete.is_none(), "{mode:?}");
        assert_eq!(legal(&v), all, "{mode:?}");

        s.roll(SIZE, &mut dice(&[7])).unwrap();
        assert_eq!(result(&s, SIZE).as_deref(), Some("City"), "{mode:?}");
        s.choose(SIZE, "tradingPost".into(), &mut none()).unwrap();
        assert_eq!(
            result(&s, SIZE).as_deref(),
            Some("Trading Post"),
            "{mode:?}"
        );
        let tags: Vec<_> = s
            .log()
            .record_for(&"table.size".into())
            .unwrap()
            .tags
            .iter()
            .map(|g| g.id.to_string())
            .collect();
        assert_eq!(tags, ["size.tradingPost"], "{mode:?}");
    }
}

#[test]
fn god_mode_offers_every_size() {
    let c = content();
    let mut s = session(&c, GenerationMode::OneTable, true);
    s.choose(PROFILE, "kurovia".into(), &mut none()).unwrap();
    let names: Vec<_> = view(&s, SIZE)
        .options
        .iter()
        .map(|o| o.result.name.clone())
        .collect();
    assert_eq!(
        names,
        ["Trading Post", "Village", "Town", "City", "Capital"]
    );
    assert_eq!(legal(&view(&s, SIZE)), names);
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
            "Name",
            "Environment",
            "Connections",
            "Size",
            "Origin",
            "Purpose",
            "Age",
            "Economy",
            "Authority",
            "Representation",
            "Authority Size",
            "Roles",
            "Authority Figures",
            "Security",
            "Crime",
            "Stability",
            "Notable Locations"
        ]
    );
    assert_eq!(stages[1].table.as_ref().unwrap().name, "Kurovia Region");
    let ordon = NAMED_REGIONS.contains(&stages[REGION].result.as_ref().unwrap().name.as_str());
    assert!(stages.iter().enumerate().all(|(i, s)| s.result.is_some()
        || (i == NAME && !ordon && s.skipped)
        || (s.sub_stage && s.skipped)));
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
    // One per table rolled: the ten stage tables and any Authority sub-rolls.
    let sub_rolls = legacy
        .log
        .table_rolls()
        .filter(|r| {
            r.table.id.as_str().starts_with("table.authority")
                && r.table.id.as_str() != "table.authority"
        })
        .count();
    // One per table rolled: the thirteen stage tables and any Authority
    // sub-rolls.
    assert_eq!(shown.len(), 13 + sub_rolls);
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
        GenerationOptions {
            god_mode: false,
            ..Default::default()
        },
        GenerationMode::OneTable,
    );
    assert!(matches!(result, Err(GenerationError::EmptySequence)));
}

// ----- Economy ---------------------------------------------------------------------

const ECONOMIES: [&str; 5] = ["Failing", "Struggling", "Stable", "Prosperous", "Wealthy"];

/// Resolves every stage before Economy in order (Region 1 = Ordon, which
/// has a naming scheme).
fn resolve_up_to_economy(s: &mut GenerationSession, mode: GenerationMode) {
    let next = |s: &mut GenerationSession| {
        if mode == GenerationMode::Incremental {
            s.advance().unwrap();
        }
    };
    s.choose(PROFILE, "kurovia".into(), &mut none()).unwrap();
    next(s);
    s.roll(REGION, &mut dice(&[1])).unwrap();
    next(s);
    s.roll(NAME, &mut RandomFaces::seeded(1)).unwrap();
    next(s);
    for stage in [ENVIRONMENT, CONNECTIONS, SIZE, ORIGIN, PURPOSE, AGE] {
        s.roll(stage, &mut dice(&[1])).unwrap();
        next(s);
    }
}

#[test]
fn economy_can_be_rolled_or_chosen_in_every_manual_mode() {
    let c = content();
    for mode in [GenerationMode::Incremental, GenerationMode::OneTable] {
        let mut s = session(&c, mode, false);
        resolve_up_to_economy(&mut s, mode);
        let v = view(&s, ECONOMY);
        assert_eq!(v.stage.name, "Economy", "{mode:?}");
        assert!(v.actionable && v.can_roll, "{mode:?}");
        assert_eq!(v.dice.as_deref(), Some("1d12"));
        assert_eq!(legal(&v), ECONOMIES, "{mode:?}");

        s.roll(ECONOMY, &mut dice(&[11])).unwrap();
        assert_eq!(result(&s, ECONOMY).as_deref(), Some("Wealthy"));
        assert_eq!(s.next_pending(), Some(AUTHORITY), "{mode:?}");
        s.choose(ECONOMY, "failing".into(), &mut none()).unwrap();
        assert_eq!(result(&s, ECONOMY).as_deref(), Some("Failing"));

        // Tag View shows the emitted tag; without it, no tags are sent.
        assert!(view(&s, ECONOMY).tags.is_none());
        let tagged = s
            .view(
                ECONOMY,
                ViewOptions {
                    show_tags: true,
                    debug: false,
                },
            )
            .unwrap();
        let tags: Vec<_> = tagged
            .tags
            .unwrap()
            .iter()
            .map(|g| g.id.to_string())
            .collect();
        assert_eq!(tags, ["economy.failing"], "{mode:?}");
    }
}

#[test]
fn moving_back_and_forth_keeps_economy_intact() {
    let c = content();
    let mode = GenerationMode::Incremental;
    let mut s = session(&c, mode, false);
    resolve_up_to_economy(&mut s, mode);
    s.roll(ECONOMY, &mut dice(&[5])).unwrap(); // Stable
    for _ in 0..(ECONOMY - ORIGIN) {
        s.back();
    }
    assert_eq!(s.cursor(), ORIGIN);
    // Economy does not depend on Origin, so changing it keeps Economy.
    let change = s.choose(ORIGIN, "tradeStop".into(), &mut none()).unwrap();
    assert!(change.invalidated.is_empty());
    for _ in ORIGIN..ECONOMY {
        s.advance().unwrap();
    }
    assert_eq!(s.cursor(), ECONOMY);
    assert_eq!(result(&s, ECONOMY).as_deref(), Some("Stable"));
    assert_eq!(s.next_pending(), Some(AUTHORITY), "only Authority is left");
}

#[test]
fn god_mode_offers_every_economy_and_keeps_its_tag() {
    let c = content();
    let mut s = session(&c, GenerationMode::OneTable, true);
    s.choose(PROFILE, "kurovia".into(), &mut none()).unwrap();
    let v = view(&s, ECONOMY);
    let names: Vec<_> = v.options.iter().map(|o| o.result.name.clone()).collect();
    assert_eq!(names, ECONOMIES);
    assert_eq!(legal(&v), ECONOMIES);
    s.choose(ECONOMY, "prosperous".into(), &mut none()).unwrap();
    assert!(s.log().has_tag(&"economy.prosperous".into()));
}

#[test]
fn economy_is_stored_in_the_snapshot_and_later_table_changes_do_not_touch_it() {
    let c = content();
    let snapshot = saved_settlement(&c);
    let economy = |snap: &SettlementSnapshot| {
        snap.display_stages()
            .into_iter()
            .find(|s| s.stage_table.as_str() == "table.economy")
            .unwrap()
    };
    let stored = economy(&snapshot);
    assert_eq!(stored.label, "Economy");
    let result = stored.result.clone().unwrap();
    assert!(ECONOMIES.contains(&result.name.as_str()));
    let tags: Vec<_> = stored.tags.iter().map(|g| g.id.to_string()).collect();
    assert_eq!(tags, [format!("economy.{}", result.id)]);
    let record = snapshot.log.record_for(&"table.economy".into()).unwrap();
    assert_eq!(record.dice.as_deref(), Some("1d12"));
    let total = record.attempts.last().unwrap().roll.total;
    assert!((1..=12).contains(&total));
    assert_eq!(record.result, result);

    // Afterwards the Economy table is customised: renamed, re-ranged on
    // other dice, its results renamed and retagged.
    let mut layers = ContentLayers::new(c, UserOverrides::default());
    let mut table = layers
        .merged()
        .content
        .table(&"table.economy".into())
        .unwrap()
        .clone();
    table.name = "Wealth".into();
    table.dice = Some("1d20".into());
    for (i, r) in table.results.iter_mut().enumerate() {
        r.name = format!("Level {i}");
        r.range = Some(settlement_core::content::FaceRange::new(
            i as i32 * 4 + 1,
            i as i32 * 4 + 4,
        ));
        r.tags = vec!["economy.stable".into()];
    }
    layers.apply_checked(|l| l.put_table(table)).unwrap();
    let changed = layers.merged().content;

    assert_eq!(economy(&snapshot), stored, "display uses the stored copy");
    let reopened = GenerationSession::from_snapshot(
        Arc::new(changed),
        GenerationOptions::default(),
        GenerationMode::OneTable,
        &snapshot,
    )
    .unwrap();
    assert_eq!(reopened.to_snapshot(), snapshot, "nothing re-resolved");
}

#[test]
fn a_snapshot_saved_before_economy_existed_still_displays_and_can_be_finished() {
    let c = content();
    let mut old = saved_settlement(&c);
    // As saved by a build without the Economy stage.
    old.stages
        .retain(|s| s.stage_table.as_str() != "table.economy");
    old.log.events.retain(
        |e| !matches!(e, GenerationEvent::TableRolled(r) if r.table.id.as_str() == "table.economy"),
    );
    old.log
        .tags
        .retain(|g| !g.id.as_str().starts_with("economy."));
    let labels: Vec<_> = old.display_stages().into_iter().map(|s| s.label).collect();
    assert!(!labels.iter().any(|l| l == "Economy"), "{labels:?}");

    let mut s = GenerationSession::from_snapshot(
        Arc::new(c),
        GenerationOptions::default(),
        GenerationMode::OneTable,
        &old,
    )
    .unwrap();
    assert_eq!(s.next_pending(), Some(ECONOMY));
    assert_eq!(view(&s, AGE).status, StageStatus::Resolved);
    s.roll(ECONOMY, &mut dice(&[12])).unwrap();
    assert!(s.is_complete());
}

// ----- Authority -------------------------------------------------------------------

const AUTHORITIES: [&str; 8] = [
    "Local Council",
    "Noble / Landed Ruler",
    "Appointed Official",
    "Merchant / Guild Rule",
    "Religious Authority",
    "Military Authority",
    "Elders / Community Leaders",
    "Popular Assembly",
];

#[test]
fn authority_can_be_rolled_or_chosen_in_every_manual_mode() {
    let c = content();
    for mode in [GenerationMode::Incremental, GenerationMode::OneTable] {
        let mut s = session(&c, mode, false);
        resolve_up_to_economy(&mut s, mode);
        s.roll(ECONOMY, &mut dice(&[1])).unwrap();
        if mode == GenerationMode::Incremental {
            s.advance().unwrap();
        }
        let v = view(&s, AUTHORITY);
        assert_eq!(v.stage.name, "Authority", "{mode:?}");
        assert!(v.actionable && v.can_roll, "{mode:?}");
        assert_eq!(v.dice.as_deref(), Some("1d12"));
        assert_eq!(legal(&v), AUTHORITIES, "{mode:?}");

        s.roll(AUTHORITY, &mut dice(&[6])).unwrap();
        assert_eq!(result(&s, AUTHORITY).as_deref(), Some("Appointed Official"));
        assert_eq!(s.next_pending(), Some(ROLES), "{mode:?}");
        s.choose(AUTHORITY, "elders".into(), &mut none()).unwrap();
        assert_eq!(
            result(&s, AUTHORITY).as_deref(),
            Some("Elders / Community Leaders")
        );

        assert!(view(&s, AUTHORITY).tags.is_none());
        let tagged = s
            .view(
                AUTHORITY,
                ViewOptions {
                    show_tags: true,
                    debug: false,
                },
            )
            .unwrap();
        let tags: Vec<_> = tagged
            .tags
            .unwrap()
            .iter()
            .map(|g| g.id.to_string())
            .collect();
        // Elders always has a group, so Authority also emits that.
        assert_eq!(
            tags,
            ["authority.elders", "authorityRepresentation.group"],
            "{mode:?}"
        );
    }
}

#[test]
fn moving_back_and_forth_keeps_authority_intact() {
    let c = content();
    let mode = GenerationMode::Incremental;
    let mut s = session(&c, mode, false);
    resolve_up_to_economy(&mut s, mode);
    s.roll(ECONOMY, &mut dice(&[5])).unwrap();
    s.advance().unwrap();
    s.roll(AUTHORITY, &mut dice(&[9])).unwrap(); // Religious Authority
    for _ in 0..(AUTHORITY - PURPOSE) {
        s.back();
    }
    assert_eq!(s.cursor(), PURPOSE);
    // Purpose intents on Authority are planned only, so changing Purpose
    // does not invalidate Authority.
    let change = s.choose(PURPOSE, "military".into(), &mut none()).unwrap();
    assert!(change.invalidated.is_empty());
    for _ in PURPOSE..AUTHORITY {
        s.advance().unwrap();
    }
    assert_eq!(s.cursor(), AUTHORITY);
    assert_eq!(
        result(&s, AUTHORITY).as_deref(),
        Some("Religious Authority")
    );
    assert_eq!(s.next_pending(), Some(ROLES));
}

#[test]
fn god_mode_offers_every_authority_and_keeps_its_tag() {
    let c = content();
    let mut s = session(&c, GenerationMode::OneTable, true);
    s.choose(PROFILE, "kurovia".into(), &mut none()).unwrap();
    let v = view(&s, AUTHORITY);
    let names: Vec<_> = v.options.iter().map(|o| o.result.name.clone()).collect();
    assert_eq!(names, AUTHORITIES);
    assert_eq!(legal(&v), AUTHORITIES);
    s.choose(AUTHORITY, "military".into(), &mut none()).unwrap();
    assert!(s.log().has_tag(&"authority.military".into()));
}

#[test]
fn authority_is_stored_in_the_snapshot_and_later_default_changes_do_not_touch_it() {
    let c = content();
    let snapshot = saved_settlement(&c);
    let authority = |snap: &SettlementSnapshot| {
        snap.display_stages()
            .into_iter()
            .find(|s| s.stage_table.as_str() == "table.authority")
            .unwrap()
    };
    let stored = authority(&snapshot);
    assert_eq!(stored.label, "Authority");
    let result = stored.result.clone().unwrap();
    assert!(AUTHORITIES.contains(&result.name.as_str()));
    let tags: Vec<_> = stored.tags.iter().map(|g| g.id.to_string()).collect();
    assert_eq!(tags[0], format!("authority.{}", result.id));
    let record = snapshot.log.record_for(&"table.authority".into()).unwrap();
    assert_eq!(record.dice.as_deref(), Some("1d12"));
    assert!((1..=12).contains(&record.attempts.last().unwrap().roll.total));
    assert_eq!(record.result, result);

    // A later version changes the Authority defaults: other names, other
    // dice and ranges, a different tag, and no generator references.
    let mut updated = c.clone();
    let table = updated
        .tables
        .iter_mut()
        .find(|t| t.id.as_str() == "table.authority")
        .unwrap();
    table.name = "Rulers".into();
    table.dice = Some("1d8".into());
    for (i, r) in table.results.iter_mut().enumerate() {
        r.name = format!("Ruler {i}");
        r.range = Some(settlement_core::content::FaceRange::single(i as i32 + 1));
        r.tags = vec!["authority.council".into()];
        r.generator = None;
    }
    assert_eq!(authority(&snapshot), stored, "display uses the stored copy");
    let reopened = GenerationSession::from_snapshot(
        Arc::new(updated),
        GenerationOptions::default(),
        GenerationMode::OneTable,
        &snapshot,
    )
    .unwrap();
    assert_eq!(reopened.to_snapshot(), snapshot, "nothing re-resolved");
}

#[test]
fn a_snapshot_saved_before_authority_existed_still_displays_and_can_be_finished() {
    let c = content();
    let mut old = saved_settlement(&c);
    // As saved by a build without Authority, its sub-stages and figures.
    let added = |t: &str| {
        t.starts_with("table.authority")
            || t == "stage.authorityFigures"
            || t == "stage.authorityRoles"
    };
    old.stages.retain(|s| !added(s.stage_table.as_str()));
    old.log.events.retain(|e| match e {
        GenerationEvent::TableRolled(r) => !added(r.table.id.as_str()),
        GenerationEvent::TableSkipped { table, .. } => !added(table.id.as_str()),
        GenerationEvent::SecondaryRoll { .. } => true,
    });
    old.log
        .tags
        .retain(|g| !g.id.as_str().starts_with("authority"));
    old.authority = None;
    assert!(!old.display_stages().iter().any(|s| s.label == "Authority"));

    let mut s = GenerationSession::from_snapshot(
        Arc::new(c),
        GenerationOptions::default(),
        GenerationMode::OneTable,
        &old,
    )
    .unwrap();
    assert_eq!(s.next_pending(), Some(AUTHORITY));
    assert_eq!(view(&s, ECONOMY).status, StageStatus::Resolved);
    // Noble / Landed Ruler: single, so the sub-stages are skipped.
    s.roll(AUTHORITY, &mut dice(&[4])).unwrap();
    // Authority Figures were not recorded either, and are not generated
    // behind the user's back.
    assert!(s.is_complete());
    assert_eq!(view(&s, ROLES).skipped, Some(SkipReason::NotRecorded));
    assert_eq!(view(&s, FIGURES).skipped, Some(SkipReason::NotRecorded));
    assert_eq!(view(&s, AUTHORITY_SIZE).status, StageStatus::Skipped);
}

// ----- Authority representation and size ---------------------------------------------

use settlement_core::authority::{AuthorityRepresentatives, Representation};

/// A session with every stage up to and including Economy resolved, the
/// cursor on Authority.
fn at_authority(mode: GenerationMode, god_mode: bool) -> GenerationSession {
    let c = content();
    let mut s = session(&c, mode, god_mode);
    resolve_up_to_economy(&mut s, mode);
    s.roll(ECONOMY, &mut dice(&[1])).unwrap();
    if mode == GenerationMode::Incremental {
        s.advance().unwrap();
    }
    s
}

fn representatives(s: &GenerationSession) -> Option<(Representation, u32)> {
    AuthorityRepresentatives::from_log(&s.log()).map(|r| (r.representation, r.size))
}

#[test]
fn incremental_single_authority_needs_no_further_rolls() {
    let mut s = at_authority(GenerationMode::Incremental, false);
    s.roll(AUTHORITY, &mut dice(&[5])).unwrap(); // Noble / Landed Ruler
    for sub in [REPRESENTATION, AUTHORITY_SIZE] {
        let v = view(&s, sub);
        assert!(v.sub_stage);
        assert_eq!(v.status, StageStatus::Skipped);
        assert_eq!(v.skipped, Some(SkipReason::NotEligible));
    }
    assert_eq!(s.next_pending(), Some(ROLES));
    // Next passes over both sub-stages to the end.
    assert_eq!(s.advance().unwrap(), AUTHORITY_SIZE + 1);
    assert_eq!(representatives(&s), Some((Representation::Single, 1)));
}

#[test]
fn incremental_group_authority_goes_to_authority_size() {
    let mut s = at_authority(GenerationMode::Incremental, false);
    s.roll(AUTHORITY, &mut dice(&[2])).unwrap(); // Local Council
    assert_eq!(view(&s, REPRESENTATION).status, StageStatus::Skipped);
    assert_eq!(s.advance().unwrap(), AUTHORITY_SIZE);
    let v = view(&s, AUTHORITY_SIZE);
    assert!(v.current && v.can_roll && v.sub_stage);
    assert_eq!(v.dice.as_deref(), Some("1d10"));
    assert_eq!(legal(&v).len(), 5);
    s.roll(AUTHORITY_SIZE, &mut dice(&[6])).unwrap();
    assert_eq!(
        result(&s, AUTHORITY_SIZE).as_deref(),
        Some("4 Representatives")
    );
    assert_eq!(representatives(&s), Some((Representation::Group, 4)));
    // Back from Authority Size passes over Representation to Authority.
    assert_eq!(s.back(), AUTHORITY);
}

#[test]
fn incremental_guild_presents_the_representation_roll() {
    let mut s = at_authority(GenerationMode::Incremental, false);
    s.roll(AUTHORITY, &mut dice(&[7])).unwrap(); // Merchant / Guild Rule
    assert_eq!(s.advance().unwrap(), REPRESENTATION);
    let v = view(&s, REPRESENTATION);
    assert!(v.current && v.can_roll);
    assert_eq!(v.dice.as_deref(), Some("1d4"));
    assert_eq!(
        legal(&v),
        ["Single Representative", "Group Representatives"]
    );
    // Size is not reached until Representation is decided.
    assert_eq!(view(&s, AUTHORITY_SIZE).status, StageStatus::Pending);

    // Rolled single: Authority Size is skipped.
    s.roll(REPRESENTATION, &mut dice(&[2])).unwrap();
    assert_eq!(view(&s, AUTHORITY_SIZE).status, StageStatus::Skipped);
    assert_eq!(representatives(&s), Some((Representation::Single, 1)));
    // Chosen group instead: Authority Size reopens and must be rolled.
    let change = s
        .choose(REPRESENTATION, "group".into(), &mut none())
        .unwrap();
    assert_eq!(change.reopened, [AUTHORITY_SIZE]);
    assert_eq!(representatives(&s), None, "not finished");
    assert_eq!(s.advance().unwrap(), AUTHORITY_SIZE);
    s.choose(AUTHORITY_SIZE, "six".into(), &mut none()).unwrap();
    assert_eq!(representatives(&s), Some((Representation::Group, 6)));
    assert_eq!(s.next_pending(), Some(ROLES));
}

#[test]
fn military_representation_is_the_same_50_50_roll() {
    for (d4, expected) in [
        (1, Representation::Single),
        (2, Representation::Single),
        (3, Representation::Group),
        (4, Representation::Group),
    ] {
        let mut s = at_authority(GenerationMode::OneTable, false);
        s.roll(AUTHORITY, &mut dice(&[10])).unwrap(); // Military Authority
        s.roll(REPRESENTATION, &mut dice(&[d4])).unwrap();
        let record = s
            .log()
            .record_for(&"table.authorityRepresentation".into())
            .unwrap()
            .clone();
        assert_eq!(record.attempts[0].roll.total, d4 as i32);
        if expected == Representation::Group {
            s.roll(AUTHORITY_SIZE, &mut dice(&[1])).unwrap();
            assert_eq!(representatives(&s), Some((Representation::Group, 2)));
        } else {
            assert_eq!(representatives(&s), Some((Representation::Single, 1)));
        }
        assert_eq!(s.next_pending(), Some(ROLES), "d4 {d4}");
    }
}

#[test]
fn changing_authority_invalidates_incompatible_representation_and_size() {
    let mut s = at_authority(GenerationMode::OneTable, false);
    s.roll(AUTHORITY, &mut dice(&[1])).unwrap(); // Local Council
    s.roll(AUTHORITY_SIZE, &mut dice(&[5])).unwrap(); // 4
    assert_eq!(representatives(&s), Some((Representation::Group, 4)));

    // Council → Noble: the group size no longer applies and is removed.
    let change = s.choose(AUTHORITY, "noble".into(), &mut none()).unwrap();
    assert_eq!(change.invalidated, [AUTHORITY_SIZE]);
    let size = view(&s, AUTHORITY_SIZE);
    assert_eq!(size.status, StageStatus::Skipped);
    assert!(size.result.is_none() && size.previous.is_none());
    assert_eq!(s.next_pending(), Some(ROLES));
    assert_eq!(representatives(&s), Some((Representation::Single, 1)));
    assert!(
        s.log().record_for(&"table.authoritySize".into()).is_none(),
        "no obsolete size roll is kept"
    );

    // Noble → Guild: Representation now has to be rolled.
    let change = s.choose(AUTHORITY, "guild".into(), &mut none()).unwrap();
    assert_eq!(change.reopened, [REPRESENTATION]);
    assert!(!s.is_complete());
    s.roll(REPRESENTATION, &mut dice(&[4])).unwrap(); // group
    s.roll(AUTHORITY_SIZE, &mut dice(&[9])).unwrap(); // 6

    // Guild (group, 6) → Elders: still a group, so the size stands, and the
    // rolled Representation gives way to the one Elders decides.
    let change = s.choose(AUTHORITY, "elders".into(), &mut none()).unwrap();
    assert_eq!(change.invalidated, [REPRESENTATION]);
    assert_eq!(view(&s, REPRESENTATION).status, StageStatus::Skipped);
    assert_eq!(
        result(&s, AUTHORITY_SIZE).as_deref(),
        Some("6 Representatives")
    );
    assert_eq!(representatives(&s), Some((Representation::Group, 6)));
}

#[test]
fn god_mode_never_opens_authority_size_for_a_single_ruler() {
    let mut s = at_authority(GenerationMode::OneTable, true);
    s.choose(AUTHORITY, "appointed".into(), &mut none())
        .unwrap();
    let size = view(&s, AUTHORITY_SIZE);
    assert_eq!(size.status, StageStatus::Skipped);
    // Neither a roll nor a choice can make it apply: it stays skipped and
    // records nothing.
    s.roll(AUTHORITY_SIZE, &mut dice(&[1])).unwrap();
    s.choose(AUTHORITY_SIZE, "three".into(), &mut none())
        .unwrap();
    assert_eq!(view(&s, AUTHORITY_SIZE).status, StageStatus::Skipped);
    assert!(s.log().record_for(&"table.authoritySize".into()).is_none());
    assert_eq!(representatives(&s), Some((Representation::Single, 1)));

    // With a group, God Mode offers every structurally valid size.
    s.choose(AUTHORITY, "assembly".into(), &mut none()).unwrap();
    let v = view(&s, AUTHORITY_SIZE);
    assert_eq!(v.status, StageStatus::Pending);
    assert_eq!(legal(&v).len(), 5);
}

#[test]
fn snapshots_store_representation_size_and_every_roll() {
    let mut s = at_authority(GenerationMode::OneTable, false);
    s.roll(AUTHORITY, &mut dice(&[8])).unwrap(); // Guild
    s.roll(REPRESENTATION, &mut dice(&[3])).unwrap(); // group
    s.roll(AUTHORITY_SIZE, &mut dice(&[8])).unwrap(); // 5
    let snapshot = s.to_snapshot();
    let stored = snapshot.authority.clone().unwrap();
    assert_eq!(stored.authority.id.as_str(), "guild");
    assert_eq!(stored.authority.name, "Merchant / Guild Rule");
    assert_eq!(
        (stored.representation, stored.size),
        (Representation::Group, 5)
    );
    let rolls: Vec<_> = [
        "table.authority",
        "table.authorityRepresentation",
        "table.authoritySize",
    ]
    .iter()
    .map(|t| {
        let r = snapshot.log.record_for(&(*t).into()).unwrap();
        (r.dice.clone().unwrap(), r.attempts[0].roll.total)
    })
    .collect();
    assert_eq!(
        rolls,
        [
            ("1d12".to_string(), 8),
            ("1d4".to_string(), 3),
            ("1d10".to_string(), 8)
        ]
    );
    let stages = snapshot.display_stages();
    let size = stages
        .iter()
        .find(|st| st.stage_table.as_str() == "table.authoritySize")
        .unwrap();
    assert!(size.sub_stage);
    assert_eq!(size.result.as_ref().unwrap().name, "5 Representatives");

    // JSON round trip; reopening changes nothing.
    let json = serde_json::to_string(&snapshot).unwrap();
    let back: SettlementSnapshot = serde_json::from_str(&json).unwrap();
    assert_eq!(back, snapshot);
    let reopened = GenerationSession::from_snapshot(
        Arc::new(content()),
        GenerationOptions::default(),
        GenerationMode::OneTable,
        &back,
    )
    .unwrap();
    assert_eq!(reopened.to_snapshot(), snapshot);
}

#[test]
fn later_size_table_changes_do_not_alter_a_saved_size() {
    let mut s = at_authority(GenerationMode::OneTable, false);
    s.roll(AUTHORITY, &mut dice(&[1])).unwrap();
    s.roll(AUTHORITY_SIZE, &mut dice(&[1])).unwrap(); // 2
    let snapshot = s.to_snapshot();
    let mut changed = content();
    let size = changed
        .tables
        .iter_mut()
        .find(|t| t.id.as_str() == "table.authoritySize")
        .unwrap();
    for r in &mut size.results {
        r.value = Some(r.value.unwrap() * 10);
        r.name = "Many".into();
    }
    let reopened = GenerationSession::from_snapshot(
        Arc::new(changed),
        GenerationOptions::default(),
        GenerationMode::OneTable,
        &snapshot,
    )
    .unwrap();
    assert_eq!(reopened.to_snapshot(), snapshot);
    assert_eq!(snapshot.authority_representatives().unwrap().size, 2);
}

#[test]
fn snapshots_saved_before_representation_load_safely_and_are_not_rerolled() {
    let c = content();
    // Generate a settlement whose Authority is a group, then strip what a
    // build without representation would not have saved.
    let mut s = at_authority(GenerationMode::OneTable, false);
    s.roll(AUTHORITY, &mut dice(&[11])).unwrap(); // Elders
    s.roll(AUTHORITY_SIZE, &mut dice(&[2])).unwrap();
    let mut json = serde_json::to_value(s.to_snapshot()).unwrap();
    json.as_object_mut().unwrap().remove("authority");
    let stages = json["stages"].as_array_mut().unwrap();
    stages.retain(|st| {
        let t = st["stage_table"].as_str().unwrap();
        t != "table.authorityRepresentation" && t != "table.authoritySize"
    });
    for st in stages.iter_mut() {
        st.as_object_mut().unwrap().remove("sub_stage");
    }
    let events = json["log"]["events"].as_array_mut().unwrap();
    events.retain(|e| {
        let t = e["table"]["id"].as_str().unwrap_or("");
        t != "table.authorityRepresentation" && t != "table.authoritySize"
    });
    let old: SettlementSnapshot = serde_json::from_value(json).unwrap();
    assert!(old.authority.is_none());
    assert!(old.display_stages().iter().all(|st| !st.sub_stage));
    // The Authority itself is still there, but its size was never recorded.
    assert!(old.authority_representatives().is_none());

    let mut reopened = GenerationSession::from_snapshot(
        Arc::new(c),
        GenerationOptions::default(),
        GenerationMode::OneTable,
        &old,
    )
    .unwrap();
    assert_eq!(
        result(&reopened, AUTHORITY).as_deref(),
        Some("Elders / Community Leaders")
    );
    // Nothing is rolled on load: the size waits for the user.
    assert_eq!(view(&reopened, REPRESENTATION).status, StageStatus::Skipped);
    assert_eq!(view(&reopened, AUTHORITY_SIZE).status, StageStatus::Pending);
    assert!(!reopened.is_complete());
    reopened.roll(AUTHORITY_SIZE, &mut dice(&[10])).unwrap();
    assert_eq!(representatives(&reopened), Some((Representation::Group, 6)));
}
