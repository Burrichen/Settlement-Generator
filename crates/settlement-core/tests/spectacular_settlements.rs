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

fn session(mode: GenerationMode, profile: &str) -> GenerationSession {
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
            let ss = st.stage.id.as_str().starts_with("table.ss.")
                || st.stage.id.as_str().starts_with("step.ssTradingPost");
            let id = st.stage.id.as_str();
            if ss {
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
