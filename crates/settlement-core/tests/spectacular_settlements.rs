//! The "Spectacular Settlements Generation" profile follows the Trading
//! Posts builder of Spectacular Settlements in place of the generic stages.

use std::sync::Arc;

use settlement_core::bundled::bundled_content;
use settlement_core::dice::{RandomFaces, ScriptedFaces};
use settlement_core::engine::GenerationOptions;
use settlement_core::framework::{SettingValue, SettingValues};
use settlement_core::ids::SettingId;
use settlement_core::session::{
    GenerationMode, GenerationSession, SessionView, StageStatus, StageView, ViewOptions,
};

const PROFILE: &str = "spectacularSettlements";

/// A session with `profile` preselected. Under this profile, the Settlement
/// Type stage is chosen as a Trading Post (see [`village`] for Villages).
fn session(mode: GenerationMode, profile: &str) -> GenerationSession {
    of_type(mode, profile, "tradingPost")
}

/// A Spectacular Settlements session building a Village.
fn village(mode: GenerationMode) -> GenerationSession {
    of_type(mode, PROFILE, "village")
}

fn of_type(mode: GenerationMode, profile: &str, kind: &str) -> GenerationSession {
    let content = Arc::new(bundled_content().expect("bundled content loads"));
    let mut values = SettingValues::new();
    values.insert(
        SettingId::new("setting.fantasyProfile"),
        SettingValue::Choice(profile.into()),
    );
    let mut s = GenerationSession::new(content, GenerationOptions::default(), mode)
        .expect("session starts")
        .with_settings(values);
    s.apply_presets(&mut RandomFaces::seeded(7)).unwrap();
    // Incremental generation reaches Settlement Type only after the stages
    // before it (Name), so it is not chosen ahead here.
    if profile == PROFILE && mode != GenerationMode::Incremental {
        // Chosen, never rolled: the book has no table for it.
        let at = index_of(&view(&s), "table.ss.settlementType");
        s.choose(at, kind.into(), &mut RandomFaces::seeded(7))
            .unwrap();
    }
    s
}

fn view(s: &GenerationSession) -> SessionView {
    s.snapshot(ViewOptions {
        show_tags: true,
        debug: false,
    })
    .unwrap()
}

fn stage<'a>(v: &'a SessionView, name: &str) -> &'a StageView {
    v.stages
        .iter()
        .find(|s| {
            s.stage.name == name
                && (s.stage.id.as_str().starts_with("table.ss.") || s.flow.is_some())
        })
        .unwrap_or_else(|| panic!("no stage {name}"))
}

fn index_of(v: &SessionView, table: &str) -> usize {
    v.stages
        .iter()
        .position(|s| s.stage.id.as_str() == table)
        .unwrap_or_else(|| panic!("no stage for {table}"))
}

#[test]
fn automatic_generation_runs_the_trading_post_builder_only() {
    for seed in 0..20 {
        let mut s = session(GenerationMode::Automatic, PROFILE);
        s.run_automatic(&Default::default(), &mut RandomFaces::seeded(seed))
            .unwrap_or_else(|e| panic!("seed {seed}: {e}"));
        let v = view(&s);
        assert!(v.complete, "seed {seed}: generation completes");
        for st in &v.stages {
            let id = st.stage.id.as_str();
            let trading_post = id.starts_with("table.ss.tradingPost.")
                || id.starts_with("step.ssTradingPost")
                || id == "table.ss.settlementType";
            if id.starts_with("table.ss.village.") || id.starts_with("step.ssVillage") {
                // Another settlement type's builder: skipped and hidden.
                assert_eq!(st.status, StageStatus::Skipped, "seed {seed}: {id}");
                assert!(st.hide_when_skipped, "seed {seed}: {id} is hidden");
            } else if trading_post {
                // Only the Place of Worship details depend on a roll.
                if !id.contains("placeOfWorshipSize")
                    && !id.contains("fervency")
                    && !id.contains("faithAlignment")
                {
                    assert_eq!(st.status, StageStatus::Resolved, "seed {seed}: {id} runs");
                }
            } else if !matches!(id, "table.settingProfile" | "name") && st.stage.name != "Name" {
                // Every generic stage is skipped, and left out of view.
                assert_eq!(
                    st.status,
                    StageStatus::Skipped,
                    "seed {seed}: {id} is skipped"
                );
                assert!(st.hide_when_skipped, "seed {seed}: {id} is hidden");
            }
        }
    }
}

#[test]
fn the_generic_profiles_never_see_the_trading_post_stages() {
    let mut s = session(GenerationMode::Automatic, "kurovia");
    s.run_automatic(&Default::default(), &mut RandomFaces::seeded(3))
        .unwrap();
    let v = view(&s);
    for st in v.stages.iter().filter(|s| {
        s.stage.id.as_str().starts_with("table.ss.")
            || s.stage.id.as_str().starts_with("step.ssTradingPost")
    }) {
        assert_eq!(st.status, StageStatus::Skipped, "{}", st.stage.id);
        assert!(st.hide_when_skipped);
    }
    // The generic stages still run and are shown.
    let region = v.stages.iter().find(|s| s.stage.name == "Region").unwrap();
    assert_eq!(region.status, StageStatus::Resolved);
    assert!(!region.hide_when_skipped || region.status != StageStatus::Skipped);
}

#[test]
fn age_modifies_visitor_traffic_as_the_book_says() {
    // Unknown age gives +4: a 16 on the d20 is read as 20, Masses.
    let mut s = session(GenerationMode::OneTable, PROFILE);
    let v = view(&s);
    let age = index_of(&v, "table.ss.tradingPost.age");
    let traffic = index_of(&v, "table.ss.tradingPost.visitorTraffic");
    s.choose(age, "unknown".into(), &mut RandomFaces::seeded(1))
        .unwrap();
    s.roll(traffic, &mut ScriptedFaces::new([16])).unwrap();
    let v = view(&s);
    assert_eq!(
        stage(&v, "Visitor Traffic")
            .result
            .as_ref()
            .unwrap()
            .id
            .as_str(),
        "masses"
    );
}

#[test]
fn modifiers_are_capped_at_the_ends_of_the_table() {
    // Law Enforcement None (-8) and a 3 on the d20: read as 1, Regular.
    let mut s = session(GenerationMode::OneTable, PROFILE);
    let v = view(&s);
    let law = index_of(&v, "table.ss.tradingPost.lawEnforcement");
    let crime = index_of(&v, "table.ss.tradingPost.crime");
    s.choose(law, "none".into(), &mut RandomFaces::seeded(1))
        .unwrap();
    s.roll(crime, &mut ScriptedFaces::new([3])).unwrap();
    let v = view(&s);
    assert_eq!(
        stage(&v, "Crime").result.as_ref().unwrap().id.as_str(),
        "regular"
    );
}

#[test]
fn incremental_generation_starts_past_the_preset_profile() {
    let s = session(GenerationMode::Incremental, PROFILE);
    let v = view(&s);
    assert_eq!(v.stages[0].status, StageStatus::Resolved);
    assert!(s.cursor() >= 1);
}

#[test]
fn shops_and_services_follow_the_size_counts() {
    // Large: the default General Store plus 1d8 + 8 shops; the default Inn
    // plus 1d6 + 5 services.
    let mut s = session(GenerationMode::OneTable, PROFILE);
    let v = view(&s);
    let size = index_of(&v, "table.ss.tradingPost.size");
    let shops = v
        .stages
        .iter()
        .position(|s| s.stage.name == "Shops")
        .unwrap();
    let services = v
        .stages
        .iter()
        .position(|s| s.stage.name == "Services")
        .unwrap();
    s.choose(size, "large".into(), &mut RandomFaces::seeded(1))
        .unwrap();
    s.roll(shops, &mut RandomFaces::seeded(5)).unwrap();
    s.roll(services, &mut RandomFaces::seeded(5)).unwrap();
    let v = view(&s);
    let lines = |name: &str, label: &str| -> Vec<String> {
        let flow = stage(&v, name).flow.as_ref().unwrap();
        flow.record
            .as_ref()
            .unwrap()
            .outline()
            .iter()
            .filter(|l| l.label == label)
            .map(|l| l.value.clone())
            .collect()
    };
    let dice = |name: &str, label: &str| -> usize {
        lines(name, label)[0]
            .split_whitespace()
            .next()
            .unwrap()
            .parse()
            .unwrap()
    };
    assert_eq!(lines("Shops", "Default: General Store"), ["General Store"]);
    assert_eq!(
        lines("Shops", "Shop").len(),
        dice("Shops", "Shop locations (1d8)") + 8
    );
    assert_eq!(lines("Services", "Default: Inn"), ["Inn"]);
    assert_eq!(
        lines("Services", "Service").len(),
        dice("Services", "Service locations (1d6)") + 5
    );
}

// ----- Villages (Chapter 3) ----------------------------------------------------

/// The result a stage gave, by stage ID.
fn result_of(v: &SessionView, id: &str) -> String {
    let st = &v.stages[index_of(v, id)];
    st.result
        .as_ref()
        .unwrap_or_else(|| panic!("{id} has no result"))
        .id
        .as_str()
        .to_owned()
}

/// The values of a flow stage's outline lines with this label.
fn flow_lines(v: &SessionView, id: &str, label: &str) -> Vec<String> {
    let st = &v.stages[index_of(v, id)];
    st.flow
        .as_ref()
        .and_then(|f| f.record.as_ref())
        .unwrap_or_else(|| panic!("{id} has no flow record"))
        .outline()
        .iter()
        .filter(|l| l.label == label)
        .map(|l| l.value.clone())
        .collect()
}

#[test]
fn automatic_generation_runs_the_village_builder_only() {
    let mut sizes = std::collections::HashSet::new();
    for seed in 0..40 {
        let mut s = village(GenerationMode::Automatic);
        s.run_automatic(&Default::default(), &mut RandomFaces::seeded(seed))
            .unwrap_or_else(|e| panic!("seed {seed}: {e}"));
        let v = view(&s);
        assert!(v.complete, "seed {seed}: generation completes");
        let no_danger = result_of(&v, "table.ss.village.dangerLevel") == "none";
        for st in &v.stages {
            let id = st.stage.id.as_str();
            if id.starts_with("table.ss.village.") || id.starts_with("step.ssVillage") {
                if id == "table.ss.village.dangerType" && no_danger {
                    // "Skip danger type table" when there is no danger.
                    assert_eq!(st.status, StageStatus::Skipped, "seed {seed}");
                } else {
                    assert_eq!(st.status, StageStatus::Resolved, "seed {seed}: {id} runs");
                }
            } else if id.starts_with("table.ss.tradingPost.")
                || id.starts_with("step.ssTradingPost")
            {
                assert_eq!(st.status, StageStatus::Skipped, "seed {seed}: {id}");
                assert!(st.hide_when_skipped, "seed {seed}: {id} is hidden");
            }
        }
        sizes.insert(result_of(&v, "table.ss.village.size"));
    }
    assert!(sizes.len() >= 3, "several sizes seen: {sizes:?}");
}

#[test]
fn hardships_lower_the_attributes_their_types_list() {
    // The book's own example: three hardships. Raid / Catastrophic (-5 to
    // size, wealth and density), Famine / Moderate (-2 to density),
    // Conflict / Heavy (-3 to density and disposition). Density totals -10.
    let mut s = village(GenerationMode::OneTable);
    let v = view(&s);
    let at = |id: &str| index_of(&v, id);
    s.choose(
        at("table.ss.village.age"),
        "mature".into(),
        &mut RandomFaces::seeded(1),
    )
    .unwrap();
    // Likelihood 17 (Three Hardships), then type and outcome per hardship.
    s.roll(
        at("step.ssVillage-hardships"),
        &mut ScriptedFaces::new([17, 3, 1, 4, 7, 6, 5]),
    )
    .unwrap();
    let after = view(&s);
    assert_eq!(
        flow_lines(&after, "step.ssVillage-hardships", "Hardship Type"),
        [
            "Raid",
            "Famine / Food Shortage",
            "Inter-Community Conflict/Violence"
        ]
    );
    assert_eq!(
        flow_lines(&after, "step.ssVillage-hardships", "Hardship Outcome"),
        ["Catastrophic Losses", "Moderate Losses", "Heavy Losses"]
    );
    let roll = |s: &mut GenerationSession, id: &str, face: u32| {
        s.roll(at(id), &mut ScriptedFaces::new([face])).unwrap();
        result_of(&view(s), id)
    };
    // Condition is listed by none of these types: a 10 stays Simple.
    assert_eq!(roll(&mut s, "table.ss.village.condition", 10), "simple");
    // 9 - 5 = 4: Small.
    assert_eq!(roll(&mut s, "table.ss.village.size", 9), "small");
    // 15 - 10 = 5: Sparse.
    assert_eq!(
        roll(&mut s, "table.ss.village.populationDensity", 15),
        "sparse"
    );
    // 9 - 3 = 6: Unfriendly.
    assert_eq!(
        roll(&mut s, "table.ss.village.disposition", 9),
        "unfriendly"
    );
    // 9 - 5 = 4: Impoverished.
    assert_eq!(
        roll(&mut s, "table.ss.village.populationWealth", 9),
        "impoverished"
    );
}

#[test]
fn age_sets_how_many_hardships_a_village_has_seen() {
    // Recent (-4): a 5 on the d20 is read as 1, No Hardship, and no
    // Hardship Type is rolled. Ancient (+4): a 15 is read as 19, Four.
    for (age, face, expected, count) in [
        ("recent", 5, "No Hardship", 0),
        ("ancient", 15, "Four Hardships", 4),
    ] {
        let mut s = village(GenerationMode::OneTable);
        let v = view(&s);
        s.choose(
            index_of(&v, "table.ss.village.age"),
            age.into(),
            &mut RandomFaces::seeded(1),
        )
        .unwrap();
        let mut faces = vec![face];
        faces.extend(std::iter::repeat_n(5, 2 * count));
        s.roll(
            index_of(&v, "step.ssVillage-hardships"),
            &mut ScriptedFaces::new(faces),
        )
        .unwrap();
        let v = view(&s);
        assert_eq!(
            flow_lines(&v, "step.ssVillage-hardships", "Hardship Likelihood"),
            [expected],
            "{age}"
        );
        assert_eq!(
            flow_lines(&v, "step.ssVillage-hardships", "Hardship Type").len(),
            count,
            "{age}"
        );
    }
}

#[test]
fn points_of_interest_follow_the_size_counts() {
    // Large: 1d2 + 1 places of worship, 1d2 places of gathering, 2d2 other
    // locations. Very Small: exactly one place of worship, 1d2 - 1 of the
    // others.
    for (size, worship, gathering, other) in [
        ("large", 2..=3, 1..=2, 2..=4),
        ("verySmall", 1..=1, 0..=1, 0..=1),
    ] {
        for seed in 0..10 {
            let mut s = village(GenerationMode::OneTable);
            let v = view(&s);
            s.choose(
                index_of(&v, "table.ss.village.size"),
                size.into(),
                &mut RandomFaces::seeded(1),
            )
            .unwrap();
            for id in [
                "step.ssVillage-placesOfWorship",
                "step.ssVillage-placesOfGathering",
                "step.ssVillage-otherLocations",
            ] {
                s.roll(index_of(&v, id), &mut RandomFaces::seeded(seed))
                    .unwrap();
            }
            let v = view(&s);
            let count = |id: &str, label: &str| flow_lines(&v, id, label).len();
            let places = count("step.ssVillage-placesOfWorship", "Place of Worship Size");
            assert!(
                worship.contains(&places),
                "{size} seed {seed}: {places} places of worship"
            );
            assert_eq!(
                count(
                    "step.ssVillage-placesOfWorship",
                    "Fervency of Local Following"
                ),
                places,
                "each place of worship has its own following"
            );
            assert_eq!(
                count("step.ssVillage-placesOfWorship", "Alignment of the Faith"),
                places,
                "each place of worship has a faith"
            );
            let halls = count("step.ssVillage-placesOfGathering", "Place of Gathering");
            assert!(
                gathering.contains(&halls),
                "{size} seed {seed}: {halls} places of gathering"
            );
            let others = count("step.ssVillage-otherLocations", "Location");
            assert!(
                other.contains(&others),
                "{size} seed {seed}: {others} other locations"
            );
        }
    }
}

#[test]
fn a_burned_down_business_says_what_it_was() {
    // Small: 1d2 locations. A 1 on the coin, then 100 (burned down), then
    // what it was: a 100 there is rolled again, then 3 is a Baker.
    let mut s = village(GenerationMode::OneTable);
    let v = view(&s);
    s.choose(
        index_of(&v, "table.ss.village.size"),
        "small".into(),
        &mut RandomFaces::seeded(1),
    )
    .unwrap();
    s.roll(
        index_of(&v, "step.ssVillage-otherLocations"),
        &mut ScriptedFaces::new([1, 100, 100, 3]),
    )
    .unwrap();
    let v = view(&s);
    assert_eq!(
        flow_lines(&v, "step.ssVillage-otherLocations", "Location"),
        ["Burned Down or Abandoned Business"]
    );
    let record = v.stages[index_of(&v, "step.ssVillage-otherLocations")]
        .flow
        .as_ref()
        .unwrap()
        .record
        .as_ref()
        .unwrap();
    let text = serde_json::to_string(record).unwrap();
    assert!(
        text.contains("Former Business") && text.contains("Baker"),
        "{text}"
    );
}

#[test]
fn harsh_weather_keeps_to_its_regions() {
    // Dust storms only in a desert, hurricanes only on a coast, storms or
    // blizzards never in a desert: elsewhere the face is rolled again (then
    // 1: Ideal).
    for (environment, face, expected) in [
        ("plains", 7, "ideal"),
        ("desert", 7, "dustStorms"),
        ("plains", 8, "ideal"),
        ("coastal", 8, "hurricane"),
        ("desert", 4, "ideal"),
        ("plains", 4, "storms"),
    ] {
        let mut s = village(GenerationMode::OneTable);
        let v = view(&s);
        s.choose(
            index_of(&v, "table.ss.village.environment"),
            environment.into(),
            &mut RandomFaces::seeded(1),
        )
        .unwrap();
        let weather = index_of(&v, "table.ss.village.harshWeather");
        s.roll(weather, &mut ScriptedFaces::new([face, 1])).unwrap();
        assert_eq!(
            result_of(&view(&s), "table.ss.village.harshWeather"),
            expected,
            "{environment} {face}"
        );
    }
}

// ----- Towns (Chapter 4) -------------------------------------------------------

/// A Spectacular Settlements session building a Town.
fn town(mode: GenerationMode) -> GenerationSession {
    of_type(mode, PROFILE, "town")
}

/// Chooses `result` at the stage of `table`.
fn pick(s: &mut GenerationSession, table: &str, result: &str) {
    let at = index_of(&view(s), table);
    s.choose(at, result.into(), &mut RandomFaces::seeded(1))
        .unwrap_or_else(|e| panic!("{table} {result}: {e}"));
}

/// Rolls the stage of `id` on these faces and gives its result.
fn roll_on(s: &mut GenerationSession, id: &str, faces: &[u32]) -> String {
    let at = index_of(&view(s), id);
    s.roll(at, &mut ScriptedFaces::new(faces.to_vec())).unwrap();
    result_of(&view(s), id)
}

fn tag_ids(s: &GenerationSession) -> Vec<String> {
    s.active_tags()
        .iter()
        .map(|t| t.id.as_str().to_owned())
        .collect()
}

#[test]
fn automatic_generation_runs_the_town_builder_only() {
    let mut sizes = std::collections::HashSet::new();
    for seed in 0..40 {
        let mut s = town(GenerationMode::Automatic);
        s.run_automatic(&Default::default(), &mut RandomFaces::seeded(seed))
            .unwrap_or_else(|e| panic!("seed {seed}: {e}"));
        let v = view(&s);
        assert!(v.complete, "seed {seed}: generation completes");
        let port = result_of(&v, "table.ss.town.origin") == "port";
        for st in &v.stages {
            let id = st.stage.id.as_str();
            if id.starts_with("table.ss.town.") || id.starts_with("step.ssTown") {
                if id == "table.ss.town.environment" && port {
                    // A Port's environment comes with its origin.
                    assert_eq!(st.status, StageStatus::Skipped, "seed {seed}");
                } else {
                    assert_eq!(st.status, StageStatus::Resolved, "seed {seed}: {id} runs");
                }
            } else if id.starts_with("table.ss.tradingPost.")
                || id.starts_with("step.ssTradingPost")
                || id.starts_with("table.ss.village.")
                || id.starts_with("step.ssVillage")
            {
                assert_eq!(st.status, StageStatus::Skipped, "seed {seed}: {id}");
                assert!(st.hide_when_skipped, "seed {seed}: {id} is hidden");
            }
        }
        sizes.insert(result_of(&v, "table.ss.town.size"));
    }
    assert!(sizes.len() >= 3, "several sizes seen: {sizes:?}");
}

#[test]
fn a_port_rolls_its_environment_with_its_origin() {
    // Port: 1-3 coastal, 4-6 river, and the Environment table is skipped.
    for (face, environment) in [(2, "coastal"), (5, "river")] {
        let mut s = town(GenerationMode::OneTable);
        assert_eq!(roll_on(&mut s, "table.ss.town.origin", &[6, face]), "port");
        assert!(
            tag_ids(&s).contains(&format!("ss.town.environment.{environment}")),
            "{face}: {:?}",
            tag_ids(&s)
        );
        let v = view(&s);
        let st = &v.stages[index_of(&v, "table.ss.town.environment")];
        assert_eq!(st.status, StageStatus::Skipped);
        assert!(st.hide_when_skipped);
    }
}

#[test]
fn production_rolls_the_specialty_on_a_d4() {
    let mut s = town(GenerationMode::OneTable);
    pick(&mut s, "table.ss.town.priority", "production");
    assert_eq!(roll_on(&mut s, "table.ss.town.specialty", &[3]), "industry");
    assert_eq!(
        roll_on(&mut s, "table.ss.town.specialty", &[4]),
        "uniqueShipping"
    );
    // Any other priority keeps the d8: an 8 is Hospitality.
    let mut s = town(GenerationMode::OneTable);
    pick(&mut s, "table.ss.town.priority", "magic");
    assert_eq!(
        roll_on(&mut s, "table.ss.town.specialty", &[8]),
        "hospitality"
    );
}

#[test]
fn government_rerolls_leadership_from_91_to_100() {
    let mut s = town(GenerationMode::OneTable);
    pick(&mut s, "table.ss.town.priority", "government");
    // 95 (Underworld) and 100 (Commune) are rolled again; 20 is Mayor.
    assert_eq!(
        roll_on(&mut s, "table.ss.town.leadership", &[95, 100, 20]),
        "mayor"
    );
    let mut s = town(GenerationMode::OneTable);
    pick(&mut s, "table.ss.town.priority", "military");
    assert_eq!(
        roll_on(&mut s, "table.ss.town.leadership", &[95]),
        "underworld"
    );
}

#[test]
fn prosperity_modifies_visitor_traffic_and_population_wealth() {
    // Abysmal Failure: -8 to traffic, -13 to wealth.
    // 11 - 8 = 3: Mostly Locals.
    let mut s = town(GenerationMode::OneTable);
    pick(&mut s, "table.ss.town.condition", "moderate");
    pick(&mut s, "table.ss.town.prosperity", "abysmalFailure");
    assert_eq!(
        roll_on(&mut s, "table.ss.town.visitorTraffic", &[11]),
        "mostlyLocals"
    );
    // 20 - 13 = 7: Average.
    assert_eq!(
        roll_on(&mut s, "table.ss.town.populationWealth", &[20]),
        "average"
    );
}

#[test]
fn farming_towns_always_have_farms_or_resources() {
    // Very Small: 1 roll, Medium: 2, Very Large: 3. With the Farming
    // specialty the roll is the d12 variant, which never gives None.
    for (size, rolls) in [("verySmall", 1), ("medium", 2), ("veryLarge", 3)] {
        for seed in 0..10 {
            let mut s = town(GenerationMode::OneTable);
            pick(&mut s, "table.ss.town.specialty", "farming");
            pick(&mut s, "table.ss.town.size", size);
            let at = index_of(&view(&s), "step.ssTown-farmsAndResources");
            s.roll(at, &mut RandomFaces::seeded(seed)).unwrap();
            let lines = flow_lines(
                &view(&s),
                "step.ssTown-farmsAndResources",
                "Farms & Resources",
            );
            assert_eq!(lines.len(), rolls, "{size}");
            assert!(
                !lines.iter().any(|l| l == "None"),
                "{size} seed {seed}: {lines:?}"
            );
        }
    }
}

#[test]
fn locations_follow_the_size_counts_and_the_free_locations() {
    // Commercial: Medium 8, one size larger (10) with the Economic
    // priority, and 2 more under a Merchant Monarch; the four defaults
    // always. Non-commercial: Large 4, plus the free ones.
    for (priority, leadership, commercial) in [
        ("military", "mayor", 8),
        ("economic", "mayor", 10),
        ("economic", "merchantMonarch", 12),
    ] {
        let mut s = town(GenerationMode::OneTable);
        pick(&mut s, "table.ss.town.priority", priority);
        pick(&mut s, "table.ss.town.size", "medium");
        pick(&mut s, "table.ss.town.leadership", leadership);
        let at = index_of(&view(&s), "step.ssTown-commercialLocations");
        s.roll(at, &mut RandomFaces::seeded(3)).unwrap();
        let v = view(&s);
        let id = "step.ssTown-commercialLocations";
        assert_eq!(
            flow_lines(&v, id, "Shop or Service").len(),
            commercial,
            "{priority} {leadership}"
        );
        let shops = flow_lines(&v, id, "Shop").len();
        let services = flow_lines(&v, id, "Service").len();
        assert_eq!(shops + services, commercial);
        for default in [
            "Default: General Store",
            "Default: Smithy",
            "Default: Inn",
            "Default: Tavern",
        ] {
            assert_eq!(flow_lines(&v, id, default).len(), 1, "{default}");
        }
    }

    // Religious priority, Education specialty and a Town Council: a free
    // place of worship (+5 to its size), a place of education and a Town
    // Hall, then 4 rolled locations for a Large town.
    let mut s = town(GenerationMode::OneTable);
    pick(&mut s, "table.ss.town.priority", "religious");
    pick(&mut s, "table.ss.town.specialty", "education");
    pick(&mut s, "table.ss.town.size", "large");
    pick(&mut s, "table.ss.town.leadership", "townCouncil");
    let at = index_of(&view(&s), "step.ssTown-nonCommercialLocations");
    // Free worship: size 15 + 5 = 20 (Great Temple), then fervency and
    // alignment; free education 1 (Academy); then four locations, all
    // places of gathering (type 2, then 3: Gathering Hall).
    let mut faces = vec![15, 10, 10, 1];
    faces.extend([2, 3].repeat(4));
    s.roll(at, &mut ScriptedFaces::new(faces)).unwrap();
    let v = view(&s);
    let id = "step.ssTown-nonCommercialLocations";
    assert_eq!(
        flow_lines(&v, id, "Place of Worship Size"),
        ["Great Temple"]
    );
    assert_eq!(
        flow_lines(&v, id, "Free: Place of Education (Education specialty)"),
        ["Academy/University"]
    );
    assert_eq!(
        flow_lines(&v, id, "Free: Town Hall (Town Council)"),
        ["Town Hall"]
    );
    assert_eq!(flow_lines(&v, id, "Non-Commercial Location Type").len(), 4);
    assert_eq!(
        flow_lines(&v, id, "Place of Gathering"),
        ["Gathering Hall"; 4]
    );
}

// ----- Cities (Chapter 5) ------------------------------------------------------

/// A Spectacular Settlements session building a City.
fn city(mode: GenerationMode) -> GenerationSession {
    of_type(mode, PROFILE, "city")
}

/// Each district a city's Districts stage made: its outline lines.
fn districts(v: &SessionView) -> Vec<Vec<(String, String)>> {
    districts_of(v, "step.ssCity-districts")
}

/// Each district the Districts stage `stage` made: its outline lines.
fn districts_of(v: &SessionView, stage: &str) -> Vec<Vec<(String, String)>> {
    let st = &v.stages[index_of(v, stage)];
    let record = st.flow.as_ref().unwrap().record.as_ref().unwrap();
    record
        .objects()
        .iter()
        .map(|o| {
            o.steps
                .iter()
                .flat_map(|s| s.outline())
                .map(|l| (l.label, l.value))
                .collect()
        })
        .collect()
}

fn value<'a>(lines: &'a [(String, String)], label: &str) -> Option<&'a str> {
    lines
        .iter()
        .find(|(l, _)| l == label)
        .map(|(_, v)| v.as_str())
}

fn count(lines: &[(String, String)], label: &str) -> usize {
    lines.iter().filter(|(l, _)| l == label).count()
}

#[test]
fn automatic_generation_runs_the_city_builder_and_its_districts() {
    let conditions = [
        "Squalid",
        "Dilapidated",
        "Decent",
        "Impressive",
        "Magnificent",
    ];
    let crimes = ["Dangerous", "Frequent", "Common", "Uncommon", "Infrequent"];
    let step = |s: &str| match s {
        "Far Worse" => -2,
        "Worse" => -1,
        "Equal" => 0,
        "Better" => 1,
        "Far Better" => 2,
        other => panic!("not a step: {other}"),
    };
    let moved = |levels: &[&str], from: &str, by: i32| {
        let at = levels.iter().position(|l| *l == from).unwrap() as i32;
        levels[(at + by).clamp(0, levels.len() as i32 - 1) as usize].to_owned()
    };
    let additional = |size: &str| match size {
        "verySmall" => 1,
        "small" => 2,
        "medium" => 3,
        "large" => 4,
        "veryLarge" => 5,
        other => panic!("{other}"),
    };
    let mut types = std::collections::HashSet::new();
    for seed in 0..30 {
        let mut s = city(GenerationMode::Automatic);
        s.run_automatic(&Default::default(), &mut RandomFaces::seeded(seed))
            .unwrap_or_else(|e| panic!("seed {seed}: {e}"));
        let v = view(&s);
        assert!(v.complete, "seed {seed}: generation completes");
        let crime = result_of(&v, "table.ss.city.generalCrime");
        let organized = matches!(crime.as_str(), "dangerous" | "frequent" | "common")
            || result_of(&v, "table.ss.city.leadership") == "underworld";
        for st in &v.stages {
            let id = st.stage.id.as_str();
            if id.starts_with("table.ss.city.") || id.starts_with("step.ssCity") {
                let expected = if id == "table.ss.city.organizedCrime" && !organized {
                    StageStatus::Skipped
                } else {
                    StageStatus::Resolved
                };
                assert_eq!(st.status, expected, "seed {seed}: {id}");
            } else if id.starts_with("table.ss.") && id != "table.ss.settlementType"
                || (id.starts_with("step.ss") && !id.starts_with("step.ssCity"))
            {
                assert_eq!(st.status, StageStatus::Skipped, "seed {seed}: {id}");
            }
        }

        // One district per Number of Districts.
        let ds = districts(&v);
        let wanted: usize = flow_lines(&v, "step.ssCity-districts", "Number of Districts")[0]
            .parse()
            .unwrap();
        assert_eq!(ds.len(), wanted, "seed {seed}");

        let general_condition = v.stages[index_of(&v, "table.ss.city.generalCondition")]
            .result
            .as_ref()
            .unwrap()
            .name
            .clone();
        let general_crime = v.stages[index_of(&v, "table.ss.city.generalCrime")]
            .result
            .as_ref()
            .unwrap()
            .name
            .clone();
        let size = result_of(&v, "table.ss.city.size");
        let coastal_or_river = matches!(
            result_of(&v, "table.ss.city.environment").as_str(),
            "coastal" | "river"
        );
        for d in &ds {
            let kind = value(d, "District Type").unwrap().to_owned();
            types.insert(kind.clone());
            // A district's level is the city's moved by its steps, stopping
            // at either end.
            let steps = value(d, "District Condition").unwrap();
            assert_eq!(
                value(d, "Condition Level").unwrap(),
                moved(&conditions, &general_condition, step(steps)),
                "seed {seed}: {general_condition} {steps}"
            );
            let crime_steps = value(d, "District Crime").unwrap();
            assert_eq!(
                value(d, "Crime Degree").unwrap(),
                moved(&crimes, &general_crime, step(crime_steps)),
                "seed {seed}: {general_crime} {crime_steps}"
            );
            if kind == "Slums" {
                assert!(
                    matches!(steps, "Far Worse" | "Worse"),
                    "Slums roll a d4: {steps}"
                );
            }
            if matches!(kind.as_str(), "Slums" | "Upper Class") {
                assert_ne!(
                    value(d, "Housing"),
                    Some("None"),
                    "{kind} always has housing"
                );
            }
            if kind == "Docks" {
                assert!(coastal_or_river, "seed {seed}: Docks need water");
            }
            // Additional locations by city size; the first notable ones
            // roll their notability.
            let rolled = count(d, "Additional Location");
            assert_eq!(rolled, additional(&size), "seed {seed}");
            let notable = match value(d, "District Notable Locations").unwrap() {
                "None" => 0,
                "One" => 1,
                "Two" => 2,
                "Three" => 3,
                other => panic!("{other}"),
            };
            assert_eq!(
                count(d, "Location Notability"),
                notable.min(rolled),
                "seed {seed}"
            );
        }
    }
    assert!(types.len() >= 6, "several district types seen: {types:?}");
}

#[test]
fn a_priority_and_a_leader_can_each_give_a_district() {
    // Enlightenment gives a Scholar district, a Merchant Monarch a Market
    // one; each takes one of the rolled number.
    for seed in 0..5 {
        let mut s = city(GenerationMode::OneTable);
        pick(&mut s, "table.ss.city.priority", "enlightenment");
        pick(&mut s, "table.ss.city.leadership", "merchantMonarch");
        pick(&mut s, "table.ss.city.size", "medium");
        let at = index_of(&view(&s), "step.ssCity-districts");
        s.roll(at, &mut RandomFaces::seeded(seed)).unwrap();
        let v = view(&s);
        let ds = districts(&v);
        let wanted: usize = flow_lines(&v, "step.ssCity-districts", "Number of Districts")[0]
            .parse()
            .unwrap();
        assert_eq!(ds.len(), wanted);
        assert_eq!(value(&ds[0], "District Type"), Some("Scholar"));
        assert_eq!(value(&ds[1], "District Type"), Some("Market"));
    }
    // A theocracy's district is a Temple district.
    let mut s = city(GenerationMode::OneTable);
    let at = index_of(&view(&s), "table.ss.city.leadership");
    // 80: Oligarchy, then 3 on its d4: priests.
    s.roll(at, &mut ScriptedFaces::new([80, 3])).unwrap();
    pick(&mut s, "table.ss.city.priority", "trade");
    let at = index_of(&view(&s), "step.ssCity-districts");
    s.roll(at, &mut RandomFaces::seeded(2)).unwrap();
    assert_eq!(
        value(&districts(&view(&s))[0], "District Type"),
        Some("Temple")
    );
}

#[test]
fn production_makes_the_first_outside_roll_on_a_d10_rerolling_1_to_4() {
    // Its d6 variant: never None. Very Small: 5 rolls in all.
    for seed in 0..10 {
        let mut s = city(GenerationMode::OneTable);
        pick(&mut s, "table.ss.city.priority", "production");
        pick(&mut s, "table.ss.city.size", "verySmall");
        let at = index_of(&view(&s), "step.ssCity-outsideTheCity");
        s.roll(at, &mut RandomFaces::seeded(seed)).unwrap();
        let lines = flow_lines(&view(&s), "step.ssCity-outsideTheCity", "Outside the City");
        assert_eq!(lines.len(), 5, "seed {seed}");
        assert!(
            matches!(
                lines[0].as_str(),
                "Farming [Agriculture]" | "Farming [Livestock]" | "Resource Harvesting"
            ),
            "seed {seed}: {}",
            lines[0]
        );
    }
}

// ----- Capitals (Chapter 6) ----------------------------------------------------

/// A Spectacular Settlements session building a Capital.
fn capital(mode: GenerationMode) -> GenerationSession {
    of_type(mode, PROFILE, "capital")
}

#[test]
fn automatic_generation_runs_the_capital_builder_and_its_districts() {
    let conditions = [
        "Squalid",
        "Dilapidated",
        "Decent",
        "Impressive",
        "Magnificent",
    ];
    let crimes = ["Dangerous", "Frequent", "Common", "Uncommon", "Infrequent"];
    let step = |s: &str| match s {
        "Far Worse" => -2,
        "Worse" => -1,
        "Equal" => 0,
        "Better" => 1,
        "Far Better" => 2,
        other => panic!("not a step: {other}"),
    };
    let moved = |levels: &[&str], from: &str, by: i32| {
        let at = levels.iter().position(|l| *l == from).unwrap() as i32;
        levels[(at + by).clamp(0, levels.len() as i32 - 1) as usize].to_owned()
    };
    for seed in 0..30 {
        let mut s = capital(GenerationMode::Automatic);
        s.run_automatic(&Default::default(), &mut RandomFaces::seeded(seed))
            .unwrap_or_else(|e| panic!("seed {seed}: {e}"));
        let v = view(&s);
        assert!(v.complete, "seed {seed}: generation completes");
        let r = |id: &str| result_of(&v, &format!("table.ss.capital.{id}"));
        let skipped_when = |id: &str| -> bool {
            match id {
                "infiltrationDepth" => r("spyNetworkSize") == "none",
                "counterintelligenceWatchfulness" => r("counterintelligence") == "none",
                "standingOrDisbanded"
                | "recruitmentType"
                | "sizeOfForce"
                | "specialization"
                | "militaryFacilities" => r("militaryForce") == "no",
                "pursuitOfCulture" => r("culture") == "nonSpecific",
                "organizedCrime" => {
                    !matches!(
                        r("generalCrime").as_str(),
                        "dangerous" | "frequent" | "common"
                    ) && r("leadership") != "underworld"
                }
                _ => false,
            }
        };
        for st in &v.stages {
            let id = st.stage.id.as_str();
            if let Some(key) = id.strip_prefix("table.ss.capital.") {
                let expected = if skipped_when(key) {
                    StageStatus::Skipped
                } else {
                    StageStatus::Resolved
                };
                assert_eq!(st.status, expected, "seed {seed}: {id}");
            } else if id.starts_with("step.ssCapital") {
                assert_eq!(st.status, StageStatus::Resolved, "seed {seed}: {id}");
            } else if id.starts_with("table.ss.") && id != "table.ss.settlementType"
                || (id.starts_with("step.ss") && !id.starts_with("step.ssCapital"))
            {
                assert_eq!(st.status, StageStatus::Skipped, "seed {seed}: {id}");
            }
        }

        let ds = districts_of(&v, "step.ssCapital-districts");
        let wanted: usize = flow_lines(&v, "step.ssCapital-districts", "Number of Districts")[0]
            .parse()
            .unwrap();
        assert_eq!(ds.len(), wanted, "seed {seed}");
        let name = |id: &str| {
            v.stages[index_of(&v, &format!("table.ss.capital.{id}"))]
                .result
                .as_ref()
                .unwrap()
                .name
                .clone()
        };
        for d in &ds {
            let kind = value(d, "District Type").unwrap();
            assert_eq!(
                value(d, "Condition Level").unwrap(),
                moved(
                    &conditions,
                    &name("generalCondition"),
                    step(value(d, "District Condition").unwrap())
                ),
            );
            assert_eq!(
                value(d, "Crime Degree").unwrap(),
                moved(
                    &crimes,
                    &name("generalCrime"),
                    step(value(d, "District Crime").unwrap())
                ),
            );
            if matches!(kind, "Slums" | "Upper Class") {
                assert_ne!(
                    value(d, "District Entry"),
                    Some("Gated & Guarded with Toll"),
                    "{kind}: no toll"
                );
                assert_ne!(
                    value(d, "Housing"),
                    Some("None"),
                    "{kind} always has housing"
                );
            }
        }

        // Each primary noble lives in one of the districts there are.
        let nobles: usize = name("primaryNobles").parse().unwrap();
        let homes = flow_lines(&v, "step.ssCapital-nobleResidences", "Noble Residence");
        assert_eq!(homes.len(), nobles, "seed {seed}");
        for home in homes {
            let n: usize = home.trim_start_matches("District ").parse().unwrap();
            assert!(
                n >= 1 && n <= ds.len(),
                "seed {seed}: {home} of {}",
                ds.len()
            );
        }
        // One relation per connection, a role and reason per visitor.
        let count = |stage: &str, label: &str| flow_lines(&v, stage, label).len();
        let connections =
            match flow_lines(&v, "step.ssCapital-connections", "Number of Connections")[0].as_str()
            {
                "One" => 1,
                "Two" => 2,
                "Three" => 3,
                _ => 4,
            };
        assert_eq!(
            count("step.ssCapital-connections", "Connection Relation"),
            connections
        );
        let visitors = count("step.ssCapital-notableVisitors", "Visitor Role");
        assert_eq!(
            count("step.ssCapital-notableVisitors", "Reason for Visit"),
            visitors
        );
        assert!((1..=4).contains(&visitors));
    }
}

#[test]
fn cloak_and_dagger_reads_spies_as_modest_and_rerolls_no_counterintelligence() {
    let mut s = capital(GenerationMode::OneTable);
    pick(
        &mut s,
        "table.ss.capital.governingPriority",
        "cloakAndDagger",
    );
    assert_eq!(
        roll_on(&mut s, "table.ss.capital.spyNetworkSize", &[5]),
        "modest"
    );
    // 5 (None) is rolled again; 12 is Modest.
    assert_eq!(
        roll_on(&mut s, "table.ss.capital.counterintelligence", &[5, 12]),
        "modest"
    );
    // Otherwise a 5 is None for both.
    let mut s = capital(GenerationMode::OneTable);
    pick(&mut s, "table.ss.capital.governingPriority", "humanitarian");
    assert_eq!(
        roll_on(&mut s, "table.ss.capital.spyNetworkSize", &[5]),
        "none"
    );
    assert_eq!(
        roll_on(&mut s, "table.ss.capital.counterintelligence", &[5]),
        "none"
    );
}

#[test]
fn resources_moves_population_wealth_by_the_relationship_with_the_people() {
    // Negative (Awful): 7 - 2 = 5, Impoverished. Positive (Tepid): 14 + 2 =
    // 16, Prosperous. Root 3 is Resources.
    for (relationship, face, expected) in
        [("awful", 7, "impoverished"), ("tepid", 14, "prosperous")]
    {
        let mut s = capital(GenerationMode::OneTable);
        pick(&mut s, "table.ss.capital.stewardship", "adequate");
        pick(&mut s, "table.ss.capital.typeOfNobility", "blood");
        pick(
            &mut s,
            "table.ss.capital.relationshipWithPeople",
            relationship,
        );
        let at = index_of(&view(&s), "step.ssCapital-rootOfRelationship");
        s.roll(at, &mut ScriptedFaces::new([3])).unwrap();
        assert_eq!(
            roll_on(&mut s, "table.ss.capital.populationWealth", &[face]),
            expected,
            "{relationship}"
        );
    }
}

#[test]
fn a_governing_priority_and_a_leader_can_each_give_a_district() {
    // Industry & Innovation gives an Industrial district, a Merchant
    // Monarch a Merchant one (a city's gives a Market one).
    for seed in 0..5 {
        let mut s = capital(GenerationMode::OneTable);
        pick(&mut s, "table.ss.capital.governingPriority", "industry");
        pick(&mut s, "table.ss.capital.leadership", "merchantMonarch");
        pick(&mut s, "table.ss.capital.size", "medium");
        let at = index_of(&view(&s), "step.ssCapital-districts");
        s.roll(at, &mut RandomFaces::seeded(seed)).unwrap();
        let v = view(&s);
        let ds = districts_of(&v, "step.ssCapital-districts");
        let wanted: usize = flow_lines(&v, "step.ssCapital-districts", "Number of Districts")[0]
            .parse()
            .unwrap();
        assert_eq!(ds.len(), wanted);
        assert_eq!(value(&ds[0], "District Type"), Some("Industrial"));
        assert_eq!(value(&ds[1], "District Type"), Some("Merchant"));
    }
}

// ----- Fortresses (Chapter 7) --------------------------------------------------

/// A fortress flow stage's lines with this label, leaving out the steps
/// skipped by their condition ("Not applicable").
fn rolled_lines(v: &SessionView, id: &str, label: &str) -> Vec<String> {
    let mut lines = flow_lines(v, id, label);
    lines.retain(|l| l != "Not applicable");
    lines
}

/// A Spectacular Settlements session building a Fortress of this type
/// (General Only, Keep, Castle or Stronghold).
fn fortress(mode: GenerationMode, kind: &str) -> GenerationSession {
    let mut s = of_type(mode, PROFILE, "fortress");
    pick(&mut s, "table.ss.fortress.fortressType", kind);
    s
}

/// Which part of the chapter a fortress stage belongs to: the Keep, Castle
/// or Stronghold (`None` for the General section).
fn fortress_part(id: &str) -> Option<&'static str> {
    const KEEP: [&str; 8] = [
        "shape",
        "keepSize",
        "exteriorWallThickness",
        "numberOfFloors",
        "raisedEntrance",
        "entryDefenses",
        "ssFortress-rooms",
        "furnishings",
    ];
    const CASTLE: [&str; 14] = [
        "geographicAdvantages",
        "style",
        "castleSize",
        "curtainWallThickness",
        "curtainWallHeight",
        "ssFortress-towers",
        "ssFortress-curtainWallDefenses",
        "barbican",
        "gatehouse",
        "moat",
        "moatWidth",
        "moatDepth",
        "barracksLocation",
        "ssFortress-outbuildings",
    ];
    const STRONGHOLD: [&str; 5] = [
        "additionalThickness",
        "additionalHeight",
        "ssFortress-additionalTowers",
        "styleAmplification",
        "brutalDefenses",
    ];
    let key = id
        .strip_prefix("table.ss.fortress.")
        .or_else(|| id.strip_prefix("step."))?;
    if KEEP.contains(&key) {
        Some("keep")
    } else if CASTLE.contains(&key) {
        Some("castle")
    } else if STRONGHOLD.contains(&key) {
        Some("stronghold")
    } else {
        None
    }
}

#[test]
fn each_fortress_type_builds_on_the_one_before() {
    let built = |kind: &str| -> &[&str] {
        match kind {
            "general" => &[],
            "keep" => &["keep"],
            "castle" => &["keep", "castle"],
            _ => &["keep", "castle", "stronghold"],
        }
    };
    for kind in ["general", "keep", "castle", "stronghold"] {
        for seed in 0..15 {
            let mut s = fortress(GenerationMode::Automatic, kind);
            s.run_automatic(&Default::default(), &mut RandomFaces::seeded(seed))
                .unwrap_or_else(|e| panic!("{kind} seed {seed}: {e}"));
            let v = view(&s);
            assert!(v.complete, "{kind} seed {seed}: generation completes");
            for st in &v.stages {
                let id = st.stage.id.as_str();
                let ours =
                    id.starts_with("table.ss.fortress.") || id.starts_with("step.ssFortress");
                if !ours {
                    let other_type = (id.starts_with("table.ss.")
                        && id != "table.ss.settlementType")
                        || id.starts_with("step.ss");
                    if other_type {
                        assert_eq!(st.status, StageStatus::Skipped, "{kind} {seed}: {id}");
                    }
                    continue;
                }
                let runs = fortress_part(id).is_none_or(|part| built(kind).contains(&part))
                    && match id {
                        // Only with two or more above-ground floors.
                        "table.ss.fortress.raisedEntrance" => !["one", "two"]
                            .contains(&result_of(&v, "table.ss.fortress.numberOfFloors").as_str()),
                        // Not without a moat.
                        "table.ss.fortress.moatWidth" | "table.ss.fortress.moatDepth" => {
                            result_of(&v, "table.ss.fortress.moat") != "none"
                        }
                        _ => true,
                    };
                let want = if runs {
                    StageStatus::Resolved
                } else {
                    StageStatus::Skipped
                };
                assert_eq!(st.status, want, "{kind} seed {seed}: {id}");
            }
        }
    }
}

#[test]
fn age_modifies_condition_and_residents() {
    // New: +4 to Condition (16 -> 20, Impeccable), -5 to Residents
    // (13 -> 8, Original). Ancient: -4 (7 -> 3, Poor), Residents as rolled.
    for (age, face, condition, residents) in [
        ("new", 16, "impeccable", "original"),
        ("ancient", 7, "poor", "newPeaceful"),
    ] {
        let mut s = fortress(GenerationMode::OneTable, "keep");
        pick(&mut s, "table.ss.fortress.age", age);
        assert_eq!(
            roll_on(&mut s, "table.ss.fortress.condition", &[face]),
            condition
        );
        assert_eq!(
            roll_on(&mut s, "table.ss.fortress.residents", &[13]),
            residents
        );
    }
}

#[test]
fn expense_and_condition_modify_furnishings() {
    // Extravagant +5 and Ruin -5 cancel out (10, Decent); Paltry -5 and
    // Poor -2 take 10 to 3 (Basic); Significant +2 and Strong +0 give 12.
    for (expense, condition, expected) in [
        ("extravagant", "ruin", "decent"),
        ("paltry", "poor", "basic"),
        ("significant", "strong", "decent"),
    ] {
        let mut s = fortress(GenerationMode::OneTable, "keep");
        pick(&mut s, "table.ss.fortress.expense", expense);
        pick(&mut s, "table.ss.fortress.condition", condition);
        assert_eq!(
            roll_on(&mut s, "table.ss.fortress.furnishings", &[10]),
            expected,
            "{expense} {condition}"
        );
    }
}

#[test]
fn the_keep_rolls_its_rooms_beyond_the_starting_ones() {
    // Large (BSM 3) x Three floors (the cellar counting) = 9, Strong +1:
    // 10 rooms. Bedroom, Kitchen and Dining and Receiving are 3 of them (4
    // with Guard Bunks for a Large garrison); the Privy is free.
    let id = "step.ssFortress-rooms";
    for (garrison, rolled) in [("modest", 7), ("large", 6)] {
        let mut s = fortress(GenerationMode::OneTable, "keep");
        pick(&mut s, "table.ss.fortress.condition", "strong");
        pick(&mut s, "table.ss.fortress.garrisonSize", garrison);
        pick(&mut s, "table.ss.fortress.keepSize", "large");
        pick(&mut s, "table.ss.fortress.numberOfFloors", "three");
        let at = index_of(&view(&s), id);
        s.roll(at, &mut RandomFaces::seeded(3)).unwrap();
        let v = view(&s);
        assert_eq!(rolled_lines(&v, id, "Number of Rooms"), ["10 rooms"]);
        assert_eq!(rolled_lines(&v, id, "Room").len(), rolled, "{garrison}");
        assert_eq!(rolled_lines(&v, id, "Free: Privy (not counted)"), ["Privy"]);
        let bunks = rolled_lines(&v, id, "Starting: Guard Bunks (Large or Maximum garrison)");
        assert_eq!(bunks.len(), usize::from(garrison == "large"), "{garrison}");
    }
}

#[test]
fn a_small_ruined_keep_has_no_rooms_beyond_the_starting_ones() {
    // Small (1) x One floor - 2 for a Ruin: nothing left to roll.
    let id = "step.ssFortress-rooms";
    let mut s = fortress(GenerationMode::OneTable, "keep");
    pick(&mut s, "table.ss.fortress.condition", "ruin");
    pick(&mut s, "table.ss.fortress.garrisonSize", "minimum");
    pick(&mut s, "table.ss.fortress.keepSize", "small");
    pick(&mut s, "table.ss.fortress.numberOfFloors", "one");
    let at = index_of(&view(&s), id);
    s.roll(at, &mut RandomFaces::seeded(3)).unwrap();
    let v = view(&s);
    assert_eq!(rolled_lines(&v, id, "Number of Rooms"), ["0 rooms"]);
    assert!(rolled_lines(&v, id, "Room").is_empty());
}

#[test]
fn a_ruin_has_structural_damage_as_well_as_its_current_issue() {
    // Ruin: 1d6 = 2 (Weather), severity 4 (Serious); then Current Issue 1
    // (Peaceful), with no severity.
    let id = "step.ssFortress-currentIssue";
    let mut s = fortress(GenerationMode::OneTable, "general");
    pick(&mut s, "table.ss.fortress.condition", "ruin");
    let at = index_of(&view(&s), id);
    s.roll(at, &mut ScriptedFaces::new([2, 4, 1])).unwrap();
    let v = view(&s);
    assert_eq!(
        rolled_lines(&v, id, "Structural Damage"),
        ["Structural Damage [Weather]"]
    );
    assert_eq!(rolled_lines(&v, id, "Current Issue"), ["Peaceful"]);
    assert_eq!(rolled_lines(&v, id, "Issue Severity"), ["Serious"]);

    // Any other condition: only the Current Issue, and its severity.
    let mut s = fortress(GenerationMode::OneTable, "general");
    pick(&mut s, "table.ss.fortress.condition", "average");
    let at = index_of(&view(&s), id);
    s.roll(at, &mut ScriptedFaces::new([17, 6])).unwrap();
    let v = view(&s);
    assert!(rolled_lines(&v, id, "Structural Damage").is_empty());
    assert_eq!(rolled_lines(&v, id, "Current Issue"), ["Infestation"]);
    assert_eq!(rolled_lines(&v, id, "Issue Severity"), ["Dire"]);
}

#[test]
fn geographic_advantages_are_rolled_within_the_environment() {
    for (environment, face, expected) in [
        ("coastal", 4, "water"),
        ("mountains", 4, "carved"),
        ("plains", 2, "none"),
        ("river", 1, "congested"),
        ("desert", 3, "hill"),
    ] {
        let mut s = fortress(GenerationMode::OneTable, "castle");
        pick(&mut s, "table.ss.fortress.environment", environment);
        assert_eq!(
            roll_on(&mut s, "table.ss.fortress.geographicAdvantages", &[face]),
            expected,
            "{environment}"
        );
    }
}

#[test]
fn a_barbican_with_a_drawbridge_adds_two_to_the_moat() {
    for (barbican, expected) in [("none", "dryTrapped"), ("twoDrawbridges", "filled")] {
        let mut s = fortress(GenerationMode::OneTable, "castle");
        pick(&mut s, "table.ss.fortress.barbican", barbican);
        assert_eq!(
            roll_on(&mut s, "table.ss.fortress.moat", &[7]),
            expected,
            "{barbican}"
        );
    }
}

#[test]
fn towers_outbuildings_and_defenses_follow_size_and_expense() {
    // Medium (BSM 2): 2 + 1d4 towers, each with its shape, size and roof;
    // 3 outbuildings. Significant expense: 3 curtain wall defenses.
    for seed in 0..10 {
        let mut s = fortress(GenerationMode::OneTable, "castle");
        pick(&mut s, "table.ss.fortress.expense", "significant");
        pick(&mut s, "table.ss.fortress.castleSize", "medium");
        for step in ["towers", "curtainWallDefenses", "outbuildings"] {
            let at = index_of(&view(&s), &format!("step.ssFortress-{step}"));
            s.roll(at, &mut RandomFaces::seeded(seed)).unwrap();
        }
        let v = view(&s);
        let towers = rolled_lines(&v, "step.ssFortress-towers", "Tower Shape").len();
        assert!((3..=6).contains(&towers), "seed {seed}: {towers} towers");
        for label in ["Tower Size / Height", "Tower Roof"] {
            assert_eq!(
                rolled_lines(&v, "step.ssFortress-towers", label).len(),
                towers
            );
        }
        assert_eq!(
            rolled_lines(&v, "step.ssFortress-outbuildings", "Outbuilding Size").len(),
            3
        );
        let defenses = rolled_lines(
            &v,
            "step.ssFortress-curtainWallDefenses",
            "Curtain Wall Defense",
        );
        assert_eq!(defenses.len(), 3, "seed {seed}");
    }
}
