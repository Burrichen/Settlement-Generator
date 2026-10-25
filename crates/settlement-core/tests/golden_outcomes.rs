//! Generation outcomes over the bundled content, pinned by fingerprint.
//!
//! Each case generates settlements with seeded software dice and fingerprints
//! the complete saved snapshot (every result, die, tag and label). The
//! fingerprints were recorded before the generation framework refactor, so
//! any change to what the bundled content generates fails here. Random NPC
//! IDs are replaced by their order of appearance before fingerprinting.
//!
//! To re-record after an intended content change, run with
//! `GOLDEN_WRITE=1` and review the diff of `fixtures/golden_outcomes.txt`.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::sync::Arc;

use settlement_core::bundled::bundled_content;
use settlement_core::content::ContentSet;
use settlement_core::dice::RandomFaces;
use settlement_core::engine::{GenerationOptions, Selections};
use settlement_core::session::{GenerationMode, GenerationSession, ViewOptions};
use settlement_core::settlements::SettlementSnapshot;

const SEEDS: u64 = 120;
const FIXTURE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/golden_outcomes.txt"
);

fn content() -> Arc<ContentSet> {
    Arc::new(bundled_content().expect("bundled content loads"))
}

fn fnv(text: &str) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in text.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    hash
}

/// The app version the fingerprints were recorded with. Snapshots record
/// the version that generated them, so it is pinned to keep a version bump
/// from changing every fingerprint.
const RECORDED_VERSION: &str = "0.1.5";

/// The snapshot as canonical JSON, with random NPC IDs numbered in order
/// and the app version pinned.
fn canonical(snapshot: &SettlementSnapshot) -> String {
    let json = serde_json::to_string(snapshot)
        .expect("snapshot serialises")
        .replace(
            &format!("\"app_version\":\"{}\"", settlement_core::VERSION),
            &format!("\"app_version\":\"{RECORDED_VERSION}\""),
        );
    let mut ids: BTreeMap<String, usize> = BTreeMap::new();
    let mut out = String::with_capacity(json.len());
    let mut rest = json.as_str();
    while let Some(at) = rest.find("npc-") {
        out.push_str(&rest[..at]);
        let id = &rest[at..(at + 36).min(rest.len())];
        let next = ids.len();
        let n = *ids.entry(id.to_owned()).or_insert(next);
        let _ = write!(out, "npc#{n}");
        rest = &rest[(at + 36).min(rest.len())..];
    }
    out.push_str(rest);
    out
}

fn kurovia() -> Selections {
    let mut selections = Selections::new();
    selections.insert("table.settingProfile".into(), "kurovia".into());
    selections
}

fn stage_index(session: &GenerationSession, id: &str) -> Option<usize> {
    session
        .views(ViewOptions::default())
        .ok()?
        .iter()
        .position(|s| s.stage.id.as_str() == id)
}

/// Automatic generation for one seed and option set.
fn automatic(content: &Arc<ContentSet>, seed: u64, options: GenerationOptions) -> String {
    let mut faces = RandomFaces::seeded(seed);
    match GenerationSession::automatic(content.clone(), options, &kurovia(), &mut faces) {
        Ok(session) => {
            let snapshot = session.to_snapshot();
            // Reopening a snapshot keeps every record exactly.
            let reopened = GenerationSession::from_snapshot(
                content.clone(),
                options,
                GenerationMode::OneTable,
                &snapshot,
            )
            .expect("snapshot reopens");
            assert_eq!(reopened.to_snapshot(), snapshot, "seed {seed} reopened");
            format!("{:016x}", fnv(&canonical(&snapshot)))
        }
        Err(err) => format!("error: {err}"),
    }
}

/// One-table generation followed by edits on every kind of stage, all with
/// the same seeded dice. Errors are part of the outcome.
fn edited(content: &Arc<ContentSet>, seed: u64) -> String {
    let mut faces = RandomFaces::seeded(seed);
    let mut notes = Vec::new();
    let options = GenerationOptions {
        god_mode: false,
        bare_minimum_buildings: seed % 2 == 0,
    };
    let mut session = GenerationSession::new(content.clone(), options, GenerationMode::OneTable)
        .expect("session starts");
    let mut note = |label: &str, result: Result<(), String>| {
        if let Err(e) = result {
            notes.push(format!("{label}: {e}"));
        }
    };
    note(
        "profile",
        session
            .choose(0, "kurovia".into(), &mut faces)
            .map(|_| ())
            .map_err(|e| e.to_string()),
    );
    note(
        "remaining",
        session
            .roll_remaining(&mut faces)
            .map(|_| ())
            .map_err(|e| e.to_string()),
    );
    if let Some(size) = stage_index(&session, "table.size") {
        note(
            "size",
            session
                .roll(size, &mut faces)
                .map(|_| ())
                .map_err(|e| e.to_string()),
        );
    }
    if let Some(economy) = stage_index(&session, "table.economy") {
        note(
            "economy",
            session
                .roll(economy, &mut faces)
                .map(|_| ())
                .map_err(|e| e.to_string()),
        );
    }
    note(
        "remaining again",
        session
            .roll_remaining(&mut faces)
            .map(|_| ())
            .map_err(|e| e.to_string()),
    );
    if let Some(roles) = stage_index(&session, "stage.authorityRoles") {
        note(
            "role",
            session
                .roll_role(roles, 0, &mut faces)
                .map(|_| ())
                .map_err(|e| e.to_string()),
        );
    }
    if let Some(figures) = stage_index(&session, "stage.authorityFigures") {
        for field in ["species", "gender", "motive"] {
            note(
                field,
                session
                    .roll_figure_field(figures, 0, field, &mut faces)
                    .map(|_| ())
                    .map_err(|e| e.to_string()),
            );
        }
        note(
            "figure name",
            session
                .roll_figure_name(figures, 0, &mut faces)
                .map(|_| ())
                .map_err(|e| e.to_string()),
        );
        note(
            "name part",
            session
                .roll_figure_name_part(figures, 0, 0, &mut faces)
                .map(|_| ())
                .map_err(|e| e.to_string()),
        );
        note(
            "relationship",
            session
                .roll_relationship(figures, 0, &mut faces)
                .map(|_| ())
                .map_err(|e| e.to_string()),
        );
    }
    if let Some(locations) = stage_index(&session, "stage.notableLocations") {
        note(
            "districts",
            session
                .roll_district_count(locations, &mut faces)
                .map(|_| ())
                .map_err(|e| e.to_string()),
        );
        note(
            "count",
            session
                .roll_location_count(locations, 0, &mut faces)
                .map(|_| ())
                .map_err(|e| e.to_string()),
        );
        note(
            "type",
            session
                .roll_location_type(locations, 1, &mut faces)
                .map(|_| ())
                .map_err(|e| e.to_string()),
        );
        note(
            "bare minimum",
            session
                .set_bare_minimum_buildings(seed % 2 == 1)
                .map_err(|e| e.to_string()),
        );
    }
    if let Some(name) = stage_index(&session, "stage.name") {
        note(
            "name",
            session
                .roll(name, &mut faces)
                .map(|_| ())
                .map_err(|e| e.to_string()),
        );
    }
    let hash = fnv(&canonical(&session.to_snapshot()));
    let mut out = format!("{hash:016x}");
    for n in notes {
        out.push_str(" | ");
        out.push_str(&n);
    }
    out
}

fn outcomes() -> String {
    let content = content();
    let mut out = String::new();
    for seed in 0..SEEDS {
        for (label, god_mode, bare_minimum_buildings) in [
            ("plain", false, false),
            ("god", true, false),
            ("bmb", false, true),
        ] {
            let options = GenerationOptions {
                god_mode,
                bare_minimum_buildings,
            };
            let _ = writeln!(
                out,
                "automatic {label} {seed}: {}",
                automatic(&content, seed, options)
            );
        }
        let _ = writeln!(out, "edited {seed}: {}", edited(&content, seed));
    }
    out
}

#[test]
fn generation_outcomes_match_the_recorded_fingerprints() {
    let actual = outcomes();
    if std::env::var_os("GOLDEN_WRITE").is_some() {
        std::fs::create_dir_all(std::path::Path::new(FIXTURE).parent().unwrap()).unwrap();
        std::fs::write(FIXTURE, &actual).unwrap();
        return;
    }
    let expected = std::fs::read_to_string(FIXTURE)
        .expect("fixture exists; record it with GOLDEN_WRITE=1")
        .replace("\r\n", "\n");
    for (line, (a, e)) in actual.lines().zip(expected.lines()).enumerate() {
        assert_eq!(a, e, "outcome line {} changed", line + 1);
    }
    assert_eq!(actual.lines().count(), expected.lines().count());
}
