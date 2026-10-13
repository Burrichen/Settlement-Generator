//! The shared application service used by the GUI and the terminal.

use settlement_core::db::Database;
use settlement_core::dice::RandomFaces;
use settlement_core::engine::Selections;
use settlement_core::ids::{ResultId, TableId, TagId};
use settlement_core::service::AppService;
use settlement_core::settings::GenerationMethod;

fn service() -> AppService {
    AppService::new(Database::open_in_memory().unwrap())
}

#[test]
fn generation_round_trip_through_the_service() {
    let mut s = service();
    let mut active = s.start_generation().unwrap();
    assert!(active.editing.is_none());
    assert!(
        s.save_new(&active, "Too early").is_err(),
        "incomplete is refused"
    );

    active
        .session
        .choose(0, ResultId::new("kurovia"), &mut RandomFaces::seeded(1))
        .unwrap();
    active
        .session
        .run_automatic(&Selections::new(), &mut RandomFaces::seeded(2))
        .unwrap();
    let saved = s.save_new(&active, "Riverside").unwrap();
    assert_eq!(s.saved_list().unwrap()[0].id, saved.id);

    // Editing reopens the snapshot; saving revises it.
    let edit = s.start_edit(&saved.id).unwrap();
    assert!(edit.session.is_complete());
    let revised = s.save_edit(&edit, "Riverside II").unwrap();
    assert_eq!(revised.summary.name, "Riverside II");
    assert_eq!(
        revised.stages.len(),
        19,
        "thirteen table stages, Name, three Authority sub-stages, Authority Figures and Notable Locations"
    );
}

#[test]
fn favourites_are_protected_through_the_service() {
    let mut s = service();
    let mut active = s.start_generation().unwrap();
    active
        .session
        .choose(0, ResultId::new("kurovia"), &mut RandomFaces::seeded(1))
        .unwrap();
    active
        .session
        .run_automatic(&Selections::new(), &mut RandomFaces::seeded(3))
        .unwrap();
    let saved = s.save_new(&active, "Keep").unwrap();
    s.saved_set_favourite(&saved.id, true).unwrap();
    assert!(s.saved_delete(&saved.id).is_err());
    s.saved_set_favourite(&saved.id, false).unwrap();
    s.saved_delete(&saved.id).unwrap();
}

#[test]
fn content_edits_are_checked_and_persisted() {
    let mut s = service();
    let mut table = s.table_detail(&TableId::new("table.age")).unwrap().table;
    table.dice = Some("1d20".into());
    let check = s.check_table(table.clone()).unwrap();
    assert!(!check.is_acceptable());
    assert!(s.save_table(table).is_err());

    let mut tag = s.tag_detail(&TagId::new("age.old")).unwrap().tag;
    tag.name = "Venerable".into();
    s.save_tag(tag).unwrap();
    let overview = s.overview().unwrap();
    let old = overview
        .tags
        .iter()
        .find(|t| t.id.as_str() == "age.old")
        .unwrap();
    assert_eq!(old.name, "Venerable");
    s.reset_all().unwrap();
    assert_eq!(
        s.tag_detail(&TagId::new("age.old")).unwrap().tag.name,
        "Old"
    );
}

#[test]
fn settings_go_through_the_core_rules() {
    let mut s = service();
    let mut settings = s.settings().unwrap();
    settings.generation_method = GenerationMethod::Manual;
    let stored = s.save_settings(&settings).unwrap();
    assert_eq!(stored.generation_method, GenerationMethod::Manual);
}

#[test]
fn both_interfaces_name_settlements_through_the_same_service() {
    use settlement_core::dice::ScriptedFaces;
    use settlement_core::naming::{NAME_STAGE_ID, NameSource};
    // The GUI's commands and the terminal interface both drive the
    // ActiveGeneration the service starts; there is no other name generator.
    let mut s = service();
    let mut active = s.start_generation().unwrap();
    active
        .session
        .choose(0, ResultId::new("kurovia"), &mut RandomFaces::seeded(1))
        .unwrap();
    let mut faces = ScriptedFaces::new([1]); // Ordon
    active.session.roll(1, &mut faces).unwrap();
    active
        .session
        .roll(2, &mut ScriptedFaces::new([43, 74, 1])) // Founder: Godwin, form A
        .unwrap();
    let view = active.view().unwrap();
    assert_eq!(view.suggested_name.as_deref(), Some("Godwinsbury"));
    active
        .session
        .run_automatic(&Selections::new(), &mut RandomFaces::seeded(2))
        .unwrap();
    let saved = s.save_new(&active, "Godwinsbury").unwrap();

    let shown = s.saved_get(&saved.id).unwrap();
    let name_stage = shown
        .stages
        .iter()
        .find(|st| st.stage_table.as_str() == NAME_STAGE_ID)
        .unwrap();
    assert_eq!(name_stage.result.as_ref().unwrap().name, "Godwinsbury");
    let record = name_stage.name.as_ref().unwrap();
    assert_eq!(record.source, NameSource::Generated);
    assert_eq!(record.rolls.len(), 3, "method roll plus two parts");
}

#[test]
fn size_is_an_ordinary_editable_table() {
    let mut s = service();
    let item = s
        .overview()
        .unwrap()
        .tables
        .into_iter()
        .find(|t| t.id.as_str() == "table.size")
        .unwrap();
    assert!(!item.incomplete && item.active && item.errors == 0);

    let detail = s.table_detail(&TableId::new("table.size")).unwrap();
    assert!(detail.table.ordered);
    assert!(detail.issues.is_empty(), "{:?}", detail.issues);
    let base = &detail.coverage[0];
    assert_eq!(base.dice, "1d10");
    assert!(base.uncovered.is_empty() && base.overlapping.is_empty());
    assert_eq!(base.faces.len(), 10);
    let faces: Vec<_> = base.faces.iter().map(|f| f.face).collect();
    assert_eq!(faces, (1..=10).collect::<Vec<_>>());
    assert!(base.faces.iter().all(|f| f.claimed_by.len() == 1));
    assert_eq!(base.results.len(), 5);
    assert!(base.results.iter().all(|r| (r.chance - 0.2).abs() < 1e-9));

    // A customisation is kept and used; the scale order is not editable.
    let mut table = detail.table;
    table.results[4].name = "Royal Seat".into();
    table.ordered = false;
    let saved = s.save_table(table).unwrap();
    assert_eq!(saved.table.results[4].name, "Royal Seat");
    assert!(saved.table.ordered);
    let mut active = s.start_generation().unwrap();
    active
        .session
        .choose(0, ResultId::new("kurovia"), &mut RandomFaces::seeded(1))
        .unwrap();
    let mut capital = Selections::new();
    capital.insert("table.size".into(), "capital".into());
    active
        .session
        .run_automatic(&capital, &mut RandomFaces::seeded(2))
        .unwrap();
    let log = active.session.log();
    let size = log.record_for(&TableId::new("table.size")).unwrap();
    assert_eq!(size.result.name, "Royal Seat");
    assert_eq!(size.tags[0].id, TagId::new("size.capital"));
}

fn economy_draft(s: &AppService) -> settlement_core::content::Table {
    s.table_detail(&TableId::new("table.economy"))
        .unwrap()
        .table
}

fn has(check: &settlement_core::service::DraftCheck, code: &str) -> bool {
    check
        .issues
        .iter()
        .any(|i| serde_json::to_value(&i.kind).unwrap()["code"] == code)
}

#[test]
fn economy_is_editable_validated_and_resettable() {
    use settlement_core::content::FaceRange;
    let mut s = service();
    let detail = s.table_detail(&TableId::new("table.economy")).unwrap();
    assert!(detail.built_in && detail.table.ordered);
    assert!(detail.issues.is_empty(), "{:?}", detail.issues);
    let base = &detail.coverage[0];
    assert_eq!(base.dice, "1d12");
    let faces: Vec<_> = base.faces.iter().map(|f| f.face).collect();
    assert_eq!(faces, (1..=12).collect::<Vec<_>>());
    assert!(base.faces.iter().all(|f| f.claimed_by.len() == 1));
    assert!(base.uncovered.is_empty() && base.overlapping.is_empty());

    // Validation catches uncovered faces, overlaps, bad dice and missing tags.
    let mut d = economy_draft(&s);
    d.results[4].range = Some(FaceRange::new(11, 11));
    assert!(has(&s.check_table(d).unwrap(), "uncovered_outcomes"));
    let mut d = economy_draft(&s);
    d.results[1].range = Some(FaceRange::new(2, 4));
    assert!(has(&s.check_table(d).unwrap(), "overlapping_ranges"));
    let mut d = economy_draft(&s);
    d.dice = Some("1d13".into());
    assert!(has(&s.check_table(d).unwrap(), "invalid_dice"));
    let mut d = economy_draft(&s);
    d.results[0].tags = vec![TagId::new("economy.nonexistent")];
    let check = s.check_table(d.clone()).unwrap();
    assert!(has(&check, "missing_tag") && !check.is_acceptable());
    assert!(s.save_table(d).is_err(), "invalid drafts are refused");

    // Rename the table and a result, re-range, disable, remove and add
    // results, and change emitted tags.
    let mut d = economy_draft(&s);
    d.name = "Wealth".into();
    d.results[2].name = "Steady".into();
    d.results[2].tags = vec![TagId::new("economy.prosperous")];
    d.results[0].enabled = false;
    d.results.remove(1); // Struggling
    d.results[0].range = Some(FaceRange::new(1, 3)); // Failing takes 1-3
    let mut added = d.results[0].clone();
    added.id = ResultId::new(settlement_core::layers::new_user_id("result"));
    added.name = "Booming".into();
    added.range = Some(FaceRange::new(13, 14));
    d.dice = Some("1d14".into()); // invalid die: refused
    assert!(s.save_table(d.clone()).is_err());
    d.dice = Some("1d20".into());
    added.range = Some(FaceRange::new(13, 20));
    d.results.push(added);
    let saved = s.save_table(d).unwrap();
    assert_eq!(saved.table.name, "Wealth");
    let names: Vec<_> = saved
        .table
        .results
        .iter()
        .map(|r| r.name.as_str())
        .collect();
    assert_eq!(
        names,
        ["Failing", "Steady", "Prosperous", "Wealthy", "Booming"]
    );
    assert!(!saved.table.results[0].enabled);
    assert!(saved.table.ordered, "the scale flag cannot be edited away");

    // The customised table drives generation.
    let mut active = s.start_generation().unwrap();
    active
        .session
        .choose(0, ResultId::new("kurovia"), &mut RandomFaces::seeded(1))
        .unwrap();
    let mut steady = Selections::new();
    steady.insert("table.economy".into(), "stable".into());
    active
        .session
        .run_automatic(&steady, &mut RandomFaces::seeded(2))
        .unwrap();
    let log = active.session.log();
    let record = log.record_for(&TableId::new("table.economy")).unwrap();
    assert_eq!(record.result.name, "Steady");
    assert_eq!(record.tags[0].id, TagId::new("economy.prosperous"));
    let saved_settlement = s.save_new(&active, "Ashford").unwrap();

    // Reset restores the bundled defaults; the saved settlement keeps what
    // it was generated with.
    s.reset_table(&TableId::new("table.economy")).unwrap();
    let reset = economy_draft(&s);
    assert_eq!(
        reset,
        settlement_core::bundled::bundled_content()
            .unwrap()
            .table(&TableId::new("table.economy"))
            .unwrap()
            .clone()
    );
    let view = s.saved_get(&saved_settlement.id).unwrap();
    assert_eq!(view.summary.app_version, settlement_core::VERSION);
    let economy = view
        .stages
        .iter()
        .find(|st| st.stage_table.as_str() == "table.economy")
        .unwrap();
    assert_eq!(economy.label, "Economy");
    assert_eq!(economy.result.as_ref().unwrap().name, "Steady");
    assert_eq!(economy.tags[0].id, TagId::new("economy.prosperous"));
}

#[test]
fn economy_customisations_survive_an_update_of_the_defaults() {
    use settlement_core::layers::{ContentLayers, Origin};
    use settlement_core::validation::IssueKind;
    let installed = settlement_core::bundled::bundled_content().unwrap();
    let mut layers = ContentLayers::new(installed.clone(), Default::default());
    let mut table = installed
        .table(&TableId::new("table.economy"))
        .unwrap()
        .clone();
    table.results[4].name = "Rich".into();
    layers.put_table(table).unwrap();
    let overrides = layers.overrides().clone();

    // A later version changes the Economy default.
    let mut updated = installed;
    let economy = updated
        .tables
        .iter_mut()
        .find(|t| t.id.as_str() == "table.economy")
        .unwrap();
    economy.description = "Updated description.".into();
    economy.results[0].name = "Collapsing".into();

    let merged = ContentLayers::new(updated, overrides).merged();
    let table = merged
        .content
        .table(&TableId::new("table.economy"))
        .unwrap();
    assert_eq!(table.results[4].name, "Rich", "the user's change is kept");
    assert_eq!(
        table.results[0].name, "Collapsing",
        "untouched defaults update"
    );
    assert_eq!(table.description, "Updated description.");
    assert_eq!(
        merged.origins.tables[&TableId::new("table.economy")],
        Origin::Modified
    );
    assert!(!merged.report.has_errors(), "{}", merged.report);
    assert!(
        merged
            .report
            .warnings()
            .any(|w| w.kind == IssueKind::DefaultChanged)
    );
}

fn authority_draft(s: &AppService) -> settlement_core::content::Table {
    s.table_detail(&TableId::new("table.authority"))
        .unwrap()
        .table
}

#[test]
fn authority_is_editable_validated_and_resettable() {
    use settlement_core::content::FaceRange;
    let mut s = service();
    let detail = s.table_detail(&TableId::new("table.authority")).unwrap();
    assert!(detail.built_in);
    assert!(detail.issues.is_empty(), "{:?}", detail.issues);
    let base = &detail.coverage[0];
    assert_eq!(base.dice, "1d12");
    let faces: Vec<_> = base.faces.iter().map(|f| f.face).collect();
    assert_eq!(faces, (1..=12).collect::<Vec<_>>());
    assert!(base.faces.iter().all(|f| f.claimed_by.len() == 1));

    // Validation catches uncovered faces, overlaps, bad dice and missing tags.
    let mut d = authority_draft(&s);
    d.results[7].range = None;
    assert!(has(&s.check_table(d).unwrap(), "uncovered_outcomes"));
    let mut d = authority_draft(&s);
    d.results[0].range = Some(FaceRange::new(1, 4));
    assert!(has(&s.check_table(d).unwrap(), "overlapping_ranges"));
    let mut d = authority_draft(&s);
    d.dice = Some("1d11".into());
    assert!(has(&s.check_table(d).unwrap(), "invalid_dice"));
    let mut d = authority_draft(&s);
    d.results[2].tags = vec![TagId::new("authority.missing")];
    assert!(!s.check_table(d).unwrap().is_acceptable());

    // Rename, change dice and ranges, remove, disable, add and retag. The
    // draft also drops every generator reference, as an editor without a
    // generator field might: the bundled references are kept.
    let mut d = authority_draft(&s);
    d.name = "Governance".into();
    d.results[0].name = "Town Council".into();
    d.results[1].tags = vec![TagId::new("authority.appointed")];
    d.results[6].enabled = false;
    d.results.remove(7); // Popular Assembly
    d.dice = Some("1d20".into());
    let mut added = d.results[0].clone();
    added.id = ResultId::new(settlement_core::layers::new_user_id("result"));
    added.name = "Pirate Captain".into();
    added.description = String::new();
    added.range = Some(FaceRange::new(12, 20));
    added.tags = vec![TagId::new("authority.military")];
    added.generator = None;
    for r in &mut d.results {
        r.generator = None;
    }
    d.results.push(added);
    let saved = s.save_table(d).unwrap();
    assert_eq!(saved.table.name, "Governance");
    assert_eq!(saved.table.dice.as_deref(), Some("1d20"));
    let names: Vec<_> = saved
        .table
        .results
        .iter()
        .map(|r| r.name.as_str())
        .collect();
    assert_eq!(names[0], "Town Council");
    assert_eq!(names.last(), Some(&"Pirate Captain"));
    assert!(!names.contains(&"Popular Assembly"));
    assert!(!saved.table.results[6].enabled);
    assert_eq!(
        saved.table.results[0].generator.as_ref().unwrap().as_str(),
        "authorityNpcs.council"
    );
    assert!(saved.table.results.last().unwrap().generator.is_none());

    // The customisation drives generation; the unresolved generator
    // references have no effect.
    let mut active = s.start_generation().unwrap();
    active
        .session
        .choose(0, ResultId::new("kurovia"), &mut RandomFaces::seeded(1))
        .unwrap();
    active
        .session
        .run_automatic(&Selections::new(), &mut RandomFaces::seeded(2))
        .unwrap();
    assert!(active.session.is_complete());
    let log = active.session.log();
    let record = log.record_for(&TableId::new("table.authority")).unwrap();
    assert_eq!(record.dice.as_deref(), Some("1d20"));
    let kept = s.save_new(&active, "Ashford").unwrap();

    // Reset restores the bundled table; the saved settlement is unchanged.
    s.reset_table(&TableId::new("table.authority")).unwrap();
    assert_eq!(
        authority_draft(&s),
        settlement_core::bundled::bundled_content()
            .unwrap()
            .table(&TableId::new("table.authority"))
            .unwrap()
            .clone()
    );
    let view = s.saved_get(&kept.id).unwrap();
    assert_eq!(view.summary.app_version, settlement_core::VERSION);
    let stored = view
        .stages
        .iter()
        .find(|st| st.stage_table.as_str() == "table.authority")
        .unwrap();
    assert_eq!(stored.result.as_ref().unwrap().id, record.result.id);
    assert_eq!(stored.result.as_ref().unwrap().name, record.result.name);
}

#[test]
fn authority_tags_and_their_intents_are_editable_in_the_tag_editor() {
    let mut s = service();
    // Pending modifier metadata is visible on the source tag.
    let capital = s.tag_detail(&TagId::new("size.capital")).unwrap();
    let authority_intents: Vec<_> = capital
        .tag
        .effects
        .iter()
        .filter(|d| d.effect.target_table().map(|t| t.as_str()) == Some("table.authority"))
        .collect();
    assert_eq!(authority_intents.len(), 4);
    assert!(authority_intents.iter().all(|d| !d.is_active()));
    assert!(capital.issues.is_empty());

    // An intent may not be made executable.
    let mut bad = capital.tag.clone();
    let i = bad
        .effects
        .iter()
        .position(|d| d.effect.target_table().map(|t| t.as_str()) == Some("table.authority"))
        .unwrap();
    bad.effects[i].status = settlement_core::content::EffectStatus::Active;
    assert!(!s.check_tag(bad.clone()).unwrap().is_acceptable());
    assert!(s.save_tag(bad).is_err());

    // Renaming an Authority tag changes only its label; the result still
    // emits it by ID.
    let mut tag = s.tag_detail(&TagId::new("authority.guild")).unwrap().tag;
    tag.name = "Guild-run".into();
    s.save_tag(tag).unwrap();
    let detail = s.tag_detail(&TagId::new("authority.guild")).unwrap();
    assert_eq!(detail.tag.name, "Guild-run");
    assert!(!detail.usages.is_empty());
    let table = authority_draft(&s);
    assert_eq!(
        table.result(&ResultId::new("guild")).unwrap().tags,
        [TagId::new("authority.guild")]
    );
    s.reset_tag(&TagId::new("authority.guild")).unwrap();
    assert_eq!(
        s.tag_detail(&TagId::new("authority.guild"))
            .unwrap()
            .tag
            .name,
        "Merchant / Guild Rule"
    );
}

#[test]
fn authority_size_and_representation_are_editable_validated_and_resettable() {
    use settlement_core::authority::{AuthorityRepresentatives, Representation};
    use settlement_core::content::FaceRange;
    let mut s = service();
    let size_id = TableId::new("table.authoritySize");
    let detail = s.table_detail(&size_id).unwrap();
    assert!(detail.issues.is_empty(), "{:?}", detail.issues);
    assert_eq!(detail.coverage[0].dice, "1d10");
    assert!(
        detail.coverage[0]
            .results
            .iter()
            .all(|r| (r.chance - 0.2).abs() < 1e-9)
    );

    // Values must stay positive whole numbers, on every result.
    let mut d = detail.table.clone();
    d.results[0].value = Some(0);
    assert!(has(&s.check_table(d.clone()).unwrap(), "invalid_value"));
    assert!(s.save_table(d).is_err());
    let mut d = detail.table.clone();
    d.results[1].value = None;
    assert!(has(&s.check_table(d).unwrap(), "missing_value"));

    // Rename, re-dice and re-range, and change the counts.
    let mut d = detail.table.clone();
    d.name = "Leaders".into();
    d.dice = Some("1d20".into());
    for (i, r) in d.results.iter_mut().enumerate() {
        let i = i as i32;
        r.range = Some(FaceRange::new(i * 4 + 1, i * 4 + 4));
        r.value = Some(i64::from(i) + 3);
        r.name = format!("{} leaders", i + 3);
    }
    let saved = s.save_table(d).unwrap();
    assert_eq!(saved.table.results[0].value, Some(3));
    assert_eq!(saved.table.results[0].name, "3 leaders");

    // The Representation d4 rule is ordinary table data: make it always a
    // group.
    let rep_id = TableId::new("table.authorityRepresentation");
    let mut rep = s.table_detail(&rep_id).unwrap().table;
    rep.results[0].range = None;
    rep.results[1].range = Some(FaceRange::new(1, 4));
    s.save_table(rep).unwrap();

    // Generation uses the customised tables.
    let mut active = s.start_generation().unwrap();
    active
        .session
        .choose(0, ResultId::new("kurovia"), &mut RandomFaces::seeded(1))
        .unwrap();
    let mut guild = Selections::new();
    guild.insert("table.authority".into(), "guild".into());
    active
        .session
        .run_automatic(&guild, &mut RandomFaces::seeded(2))
        .unwrap();
    let log = active.session.log();
    let r = AuthorityRepresentatives::from_log(&log).unwrap();
    assert_eq!(r.representation, Representation::Group);
    assert!((3..=7).contains(&r.size), "{}", r.size);
    let size = log.record_for(&size_id).unwrap();
    assert_eq!(size.dice.as_deref(), Some("1d20"));
    let saved_settlement = s.save_new(&active, "Ashford").unwrap();

    // Reset both tables; the saved settlement keeps its generated values.
    s.reset_table(&size_id).unwrap();
    s.reset_table(&rep_id).unwrap();
    let bundled = settlement_core::bundled::bundled_content().unwrap();
    assert_eq!(
        &s.table_detail(&size_id).unwrap().table,
        bundled.table(&size_id).unwrap()
    );
    assert_eq!(
        &s.table_detail(&rep_id).unwrap().table,
        bundled.table(&rep_id).unwrap()
    );
    let kept = s.saved_get(&saved_settlement.id).unwrap();
    let stored = kept
        .stages
        .iter()
        .find(|st| st.stage_table == size_id)
        .unwrap();
    assert!(stored.sub_stage);
    assert_eq!(
        stored.result.as_ref().unwrap().name,
        format!("{} leaders", r.size)
    );
}
