//! The Fantasy Profile setting preselects the Setting Profile table, so a
//! generation starts past that choose-only stage. "Choose" leaves it to the
//! user, as before.

use std::sync::Arc;

use settlement_core::bundled::bundled_content;
use settlement_core::dice::RandomFaces;
use settlement_core::engine::GenerationOptions;
use settlement_core::framework::{SettingValue, SettingValues};
use settlement_core::ids::{SettingId, TableId};
use settlement_core::session::{GenerationMode, GenerationSession};

const SETTING: &str = "setting.fantasyProfile";

fn session(mode: GenerationMode, profile: Option<&str>) -> GenerationSession {
    let content = Arc::new(bundled_content().expect("bundled content loads"));
    let mut values = SettingValues::new();
    if let Some(p) = profile {
        values.insert(SettingId::new(SETTING), SettingValue::Choice(p.into()));
    }
    let mut s = GenerationSession::new(content, GenerationOptions::default(), mode)
        .expect("session starts")
        .with_settings(values);
    s.apply_presets(&mut RandomFaces::from_entropy())
        .expect("presets apply");
    s
}

fn tag_ids(s: &GenerationSession) -> Vec<String> {
    s.active_tags()
        .into_iter()
        .map(|t| t.id.to_string())
        .collect()
}

#[test]
fn choose_by_default_leaves_the_setting_profile_to_the_user() {
    for profile in [None, Some("choose")] {
        let s = session(GenerationMode::Incremental, profile);
        assert!(s.presets().is_empty());
        assert_eq!(s.next_pending(), Some(0), "Setting Profile still to choose");
        assert_eq!(s.cursor(), 0);
    }
}

#[test]
fn a_set_profile_resolves_the_stage_and_moves_past_it() {
    let s = session(GenerationMode::Incremental, Some("kurovia"));
    assert_eq!(
        s.presets().get(&TableId::new("table.settingProfile")),
        Some(&"kurovia".into())
    );
    assert_eq!(s.next_pending(), Some(1));
    assert_eq!(s.cursor(), 1, "Incremental starts on the stage after it");
    assert!(tag_ids(&s).contains(&"world.kurovia".to_string()));
}

#[test]
fn every_profile_option_is_a_setting_profile_result() {
    for profile in [
        "kurovia",
        "lowFantasy",
        "mediumFantasy",
        "highFantasy",
        "forgottenRealms",
        "horror",
    ] {
        let s = session(GenerationMode::OneTable, Some(profile));
        assert_eq!(s.next_pending(), Some(1), "{profile} is preselected");
        assert!(tag_ids(&s).contains(&format!("world.{profile}")));
    }
}

#[test]
fn an_explicit_choice_still_overrides_the_setting() {
    let mut s = session(GenerationMode::OneTable, Some("kurovia"));
    s.choose(0, "horror".into(), &mut RandomFaces::from_entropy())
        .expect("choosing another profile works");
    let tags = tag_ids(&s);
    assert!(tags.contains(&"world.horror".to_string()));
    assert!(!tags.contains(&"world.kurovia".to_string()));
}
