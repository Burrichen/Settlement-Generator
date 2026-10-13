//! Ordon Quality Pass v2 naming: the five methods and their exact d100
//! weights, the generic `curated_variant` join (Founder, Industry,
//! Transport), whole Inherited names, every supplied spelling exception, the
//! complete catalogue of 1,530 paths, validation of approved names, manual
//! part selection, generation traces, persistence and user overrides.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::Arc;

use settlement_core::bundled::bundled_content;
use settlement_core::content::{ContentSet, FaceRange, TableResult};
use settlement_core::db::Database;
use settlement_core::dice::{RandomFaces, ScriptedFaces};
use settlement_core::editor;
use settlement_core::engine::{GenerationError, GenerationOptions, Selections};
use settlement_core::ids::{ResultId, TableId};
use settlement_core::layers::{ContentLayers, UserOverrides};
use settlement_core::naming::{
    ComposeError, JoinStyle, NAME_STAGE_ID, NamePart, NameRecord, NameSource, NamingMethod,
    NamingScheme, SpellingException,
};
use settlement_core::service::AppService;
use settlement_core::session::{GenerationMode, GenerationSession, StageStatus, ViewOptions};
use settlement_core::settlements::SettlementSnapshot;
use settlement_core::validation::{IssueKind, validate};

const PROFILE: usize = 0;
const REGION: usize = 1;
const NAME: usize = 2;
const SCHEME: &str = "naming.kurovia.ordon";
const METHOD: &str = "table.naming.ordon.method";
const FOUNDER: &str = "table.naming.ordon.founder";
const FOUNDER_FORM: &str = "table.naming.ordon.founderEnding";
const INDUSTRY: &str = "table.naming.ordon.industryFirst";
const INDUSTRY_FORM: &str = "table.naming.ordon.industrySecond";
const ROUTE: &str = "table.naming.ordon.routeFirst";
const INHERITED: &str = "table.naming.ordon.inheritedName";

/// The authoring folder that ships with the pack (not bundled content).
const AUTHORING: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../authoring/naming/ordon");

fn content() -> ContentSet {
    bundled_content().unwrap()
}

fn dice(values: &[u32]) -> ScriptedFaces {
    ScriptedFaces::new(values.iter().copied())
}

fn shown() -> ViewOptions {
    ViewOptions {
        show_tags: true,
        debug: false,
    }
}

fn session(c: &ContentSet, mode: GenerationMode) -> GenerationSession {
    GenerationSession::new(Arc::new(c.clone()), GenerationOptions::default(), mode).unwrap()
}

/// One-table at Kurovia (region die face 1 is Ordon).
fn at_region(c: &ContentSet, face: u32) -> GenerationSession {
    let mut s = session(c, GenerationMode::OneTable);
    s.choose(PROFILE, "kurovia".into(), &mut dice(&[])).unwrap();
    s.roll(REGION, &mut dice(&[face])).unwrap();
    s
}

fn ordon(c: &ContentSet) -> GenerationSession {
    at_region(c, 1)
}

fn named(c: &ContentSet, faces: &[u32]) -> String {
    let mut s = ordon(c);
    s.roll(NAME, &mut dice(faces)).unwrap();
    s.name().unwrap()
}

fn scheme(c: &ContentSet) -> &NamingScheme {
    c.naming_scheme(&SCHEME.into()).unwrap()
}

fn method<'c>(c: &'c ContentSet, id: &str) -> &'c NamingMethod {
    scheme(c).method(&id.into()).unwrap()
}

fn table_mut<'c>(c: &'c mut ContentSet, id: &str) -> &'c mut settlement_core::content::Table {
    c.tables.iter_mut().find(|t| t.id.as_str() == id).unwrap()
}

/// The lowest face of a result.
fn face(c: &ContentSet, table: &str, result: &str) -> u32 {
    c.table(&TableId::new(table))
        .unwrap()
        .result(&result.into())
        .unwrap()
        .range
        .unwrap()
        .min as u32
}

/// Every path through the scheme: (method, name), via the engine.
fn catalogue(c: &ContentSet) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for m in &scheme(c).methods {
        let tables: Vec<_> = m.components.iter().map(|t| c.table(t).unwrap()).collect();
        let mut paths: Vec<Vec<&TableResult>> = vec![Vec::new()];
        for t in &tables {
            paths = paths
                .into_iter()
                .flat_map(|p| {
                    t.results.iter().map(move |r| {
                        let mut p = p.clone();
                        p.push(r);
                        p
                    })
                })
                .collect();
        }
        for path in paths {
            let parts: Vec<NamePart> = path
                .iter()
                .zip(&tables)
                .map(|(r, t)| NamePart {
                    table: &t.id,
                    result: &r.id,
                    text: &r.name,
                })
                .collect();
            out.push((m.result.to_string(), m.compose(c, &parts).unwrap().text));
        }
    }
    out
}

/// A minimal CSV reader (quoted fields, BOM) for the authoring exports.
fn read_csv(text: &str) -> Vec<HashMap<String, String>> {
    let text = text.trim_start_matches('\u{feff}');
    let split = |line: &str| {
        let mut fields = Vec::new();
        let (mut field, mut quoted) = (String::new(), false);
        let mut chars = line.chars().peekable();
        while let Some(ch) = chars.next() {
            match (ch, quoted) {
                ('"', true) if chars.peek() == Some(&'"') => {
                    field.push('"');
                    chars.next();
                }
                ('"', _) => quoted = !quoted,
                (',', false) => fields.push(std::mem::take(&mut field)),
                (ch, _) => field.push(ch),
            }
        }
        fields.push(field);
        fields
    };
    let mut lines = text.lines().filter(|l| !l.trim().is_empty());
    let header = split(lines.next().unwrap());
    lines
        .map(|l| header.iter().cloned().zip(split(l)).collect())
        .collect()
}

// ----- methods and probabilities ----------------------------------------------------

#[test]
fn the_five_methods_have_exactly_the_approved_d100_chances() {
    let c = content();
    let table = c.table(&METHOD.into()).unwrap();
    let coverage = editor::coverage(table);
    assert_eq!(coverage.len(), 1);
    assert!(coverage[0].is_valid(), "every face covered once");
    let chances: Vec<(String, i64)> = coverage[0]
        .results
        .iter()
        .map(|r| (r.result.clone(), (r.chance * 100.0).round() as i64))
        .collect();
    assert_eq!(
        chances,
        [
            ("geographic", 42),
            ("founder", 20),
            ("industrial", 18),
            ("route", 12),
            ("inherited", 8),
        ]
        .map(|(m, p)| (m.to_owned(), p))
    );
    // Exactly five methods, one per method-table result, no sixth.
    let methods: Vec<&str> = scheme(&c)
        .methods
        .iter()
        .map(|m| m.result.as_str())
        .collect();
    assert_eq!(
        methods,
        ["geographic", "founder", "industrial", "route", "inherited"]
    );
}

#[test]
fn changing_the_method_ranges_in_content_changes_the_odds() {
    let mut c = content();
    let method = table_mut(&mut c, METHOD);
    method.results[0].range = Some(FaceRange::new(1, 40));
    method.results[1].range = Some(FaceRange::new(41, 62));
    assert!(!validate(&c).has_errors());
    let mut s = ordon(&c);
    s.roll(NAME, &mut dice(&[41, 28, 3])).unwrap();
    assert_eq!(s.name().as_deref(), Some("Oswaldwick"), "41 is Founder now");
    // A gap is a validation error, never a silent reroll.
    table_mut(&mut c, METHOD).results[1].range = Some(FaceRange::new(42, 62));
    assert!(
        validate(&c)
            .errors()
            .any(|i| matches!(i.kind, IssueKind::UncoveredOutcomes { .. }))
    );
}

#[test]
fn the_component_tables_are_the_approved_sizes() {
    let c = content();
    let count = |t: &str| c.table(&t.into()).unwrap().results.len();
    assert_eq!(count("table.naming.ordon.firstElement"), 50);
    assert_eq!(count("table.naming.ordon.secondElement"), 20);
    assert_eq!(count(FOUNDER), 100);
    assert_eq!(count(INDUSTRY), 40);
    assert_eq!(count(ROUTE), 30);
    assert_eq!(count(INHERITED), 50);
    for (source, forms) in [(FOUNDER, 2), (INDUSTRY, 4), (ROUTE, 4)] {
        for r in &c.table(&source.into()).unwrap().results {
            assert_eq!(r.approved_names.len(), forms, "{source} › {}", r.id);
            assert!(r.approved_names.values().all(|n| !n.trim().is_empty()));
        }
    }
    for (table, dice) in [
        (METHOD, "1d100"),
        ("table.naming.ordon.firstElement", "1d100"),
        ("table.naming.ordon.secondElement", "1d20"),
        (FOUNDER, "1d100"),
        (FOUNDER_FORM, "1d4"),
        (INDUSTRY, "1d100"),
        (INDUSTRY_FORM, "1d4"),
        (ROUTE, "1d100"),
        ("table.naming.ordon.routeSecond", "1d4"),
        (INHERITED, "1d100"),
    ] {
        let t = c.table(&table.into()).unwrap();
        assert_eq!(t.dice.as_deref(), Some(dice), "{table}");
        assert!(
            editor::coverage(t).iter().all(|c| c.is_valid()),
            "{table} covers its dice exactly once"
        );
    }
}

// ----- the five methods with physical dice ----------------------------------------------

#[test]
fn geographic_names_join_and_apply_the_listed_exceptions() {
    let c = content();
    let first = |r| face(&c, "table.naming.ordon.firstElement", r);
    let second = |r| face(&c, "table.naming.ordon.secondElement", r);
    for (a, b, expected) in [
        ("alder", "wick", "Alderwick"),
        ("bracken", "bury", "Brackenbury"),
        ("ash", "ham", "Asham"),
        ("ash", "hurst", "Ashurst"),
        ("hill", "ley", "Hildley"),
        ("mill", "ley", "Milnley"),
    ] {
        assert_eq!(named(&c, &[1, first(a), second(b)]), expected);
    }
}

/// Every exception in the pack as supplied (by display name), checked with
/// real dice against the installed (by-ID) scheme.
#[test]
fn every_supplied_exception_gives_its_exact_spelling() {
    let c = content();
    let supplied: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(format!("{AUTHORING}/supplied/SUPPLIED_NAMING_SCHEME.json"))
            .unwrap(),
    )
    .unwrap();
    let exceptions = supplied["naming_schemes"][0]["methods"][0]["exceptions"]
        .as_array()
        .unwrap();
    assert_eq!(exceptions.len(), 17);
    assert_eq!(method(&c, "geographic").exceptions.len(), 17);
    let by_name = |table: &str, name: &str| {
        c.table(&table.into())
            .unwrap()
            .results
            .iter()
            .find(|r| r.name == name)
            .unwrap()
            .range
            .unwrap()
            .min as u32
    };
    for e in exceptions {
        let parts = e["components"].as_array().unwrap();
        let (a, b) = (parts[0].as_str().unwrap(), parts[1].as_str().unwrap());
        let faces = [
            1,
            by_name("table.naming.ordon.firstElement", a),
            by_name("table.naming.ordon.secondElement", b),
        ];
        let mut s = ordon(&c);
        s.roll(NAME, &mut dice(&faces)).unwrap();
        assert_eq!(
            s.name().unwrap(),
            e["output"].as_str().unwrap(),
            "{a} + {b}"
        );
        // The trace names the exception, with its note.
        let assembly = s
            .view(NAME, shown())
            .unwrap()
            .naming
            .unwrap()
            .assembly
            .unwrap();
        let used = assembly.exception.unwrap();
        assert_eq!(used.name, e["output"].as_str().unwrap());
        assert_eq!(used.note, e["note"].as_str().unwrap());
    }
}

#[test]
fn exceptions_written_by_name_still_work_and_by_id_survive_renaming() {
    let mut c = content();
    // The supplied by-name form is read and matched as written.
    let geographic = c
        .naming_schemes
        .iter_mut()
        .find(|s| s.id.as_str() == SCHEME)
        .unwrap()
        .methods
        .iter_mut()
        .find(|m| m.result.as_str() == "geographic")
        .unwrap();
    geographic.exceptions = serde_json::from_value(serde_json::json!([
        { "components": ["Hill", "ley"], "output": "Hildley", "note": "Avoid Hillley" }
    ]))
    .unwrap();
    assert!(!validate(&c).has_errors(), "{}", validate(&c));
    let (hill, ley) = (
        face(&c, "table.naming.ordon.firstElement", "hill"),
        face(&c, "table.naming.ordon.secondElement", "ley"),
    );
    assert_eq!(named(&c, &[1, hill, ley]), "Hildley");

    // Renaming "Hill" breaks a by-name exception loudly, never silently...
    table_mut(&mut c, "table.naming.ordon.firstElement").results[41].name = "Hyll".into();
    assert!(validate(&c).errors().any(|i| matches!(
        &i.kind,
        IssueKind::NamingExceptionName { name, matches: 0, .. } if name == "Hill"
    )));
    // ...while the installed by-ID exceptions keep working.
    let mut c = content();
    table_mut(&mut c, "table.naming.ordon.firstElement").results[41].name = "Hyll".into();
    assert!(!validate(&c).has_errors());
    assert_eq!(named(&c, &[1, hill, ley]), "Hildley");
}

#[test]
fn founder_names_are_the_approved_a_and_b_forms() {
    let c = content();
    let f = |r| face(&c, FOUNDER, r);
    assert_eq!(named(&c, &[43, f("oswald"), 1]), "Oswalden");
    assert_eq!(named(&c, &[43, f("oswald"), 3]), "Oswaldwick");
    assert_eq!(named(&c, &[43, f("godwin"), 2]), "Godwinsbury");
    assert_eq!(named(&c, &[43, f("godwin"), 4]), "Godwinham");
    assert_eq!(named(&c, &[43, f("nerys"), 1]), "Nerysbury");
    // The approved form already has its possessive: none is added again.
    assert_eq!(named(&c, &[43, f("nerys"), 4]), "Neryswell");
}

#[test]
fn the_founder_form_die_is_a_on_1_2_and_b_on_3_4() {
    let c = content();
    let oswald = face(&c, FOUNDER, "oswald");
    let forms: Vec<String> = (1..=4).map(|d| named(&c, &[50, oswald, d])).collect();
    assert_eq!(forms, ["Oswalden", "Oswalden", "Oswaldwick", "Oswaldwick"]);
    let table = c.table(&FOUNDER_FORM.into()).unwrap();
    let chances: Vec<f64> = editor::coverage(table)[0]
        .results
        .iter()
        .map(|r| r.chance)
        .collect();
    assert_eq!(chances, [0.5, 0.5]);
}

#[test]
fn industry_and_transport_names_are_the_approved_forms() {
    let c = content();
    let foundry = face(&c, INDUSTRY, "foundry");
    let industry: Vec<String> = (1..=4).map(|d| named(&c, &[63, foundry, d])).collect();
    assert_eq!(
        industry,
        ["Foulden", "Fouldham", "Founderswick", "Fouldbridge"]
    );
    let market = face(&c, ROUTE, "market");
    let route: Vec<String> = (1..=4).map(|d| named(&c, &[81, market, d])).collect();
    assert_eq!(
        route,
        ["Market Cross", "Market Bridge", "Market Ford", "Marketgate"],
        "spaces kept exactly"
    );
}

#[test]
fn inherited_names_are_used_whole_and_exactly_as_written() {
    let c = content();
    for r in &c.table(&INHERITED.into()).unwrap().results {
        let face = r.range.unwrap().min as u32;
        let mut s = ordon(&c);
        s.roll(NAME, &mut dice(&[93, face])).unwrap();
        assert_eq!(s.name().unwrap(), r.name, "{}", r.id);
    }
    assert_eq!(named(&c, &[93, 1]), "Netherleigh");
    let cuthred = face(&c, INHERITED, "st-cuthred");
    assert_eq!(named(&c, &[100, cuthred]), "St. Cuthred");
}

#[test]
fn the_physical_dice_example_reproduces_and_is_traced() {
    let c = content();
    let mut s = ordon(&c);
    s.roll(NAME, &mut dice(&[50, 28, 3])).unwrap();
    let view = s.view(NAME, shown()).unwrap();
    assert_eq!(
        view.outcome.as_ref().unwrap().dice.as_deref(),
        Some("1d100")
    );
    assert_eq!(view.outcome.as_ref().unwrap().roll.as_deref(), Some("50"));
    let naming = view.naming.unwrap();
    assert_eq!(naming.scheme.unwrap().id.as_str(), SCHEME);
    assert_eq!(naming.method.unwrap().name, "Founders, Manors & Estates");
    let parts: Vec<(&str, &str, &str, &str, &str)> = naming
        .parts
        .iter()
        .map(|p| {
            (
                p.table.id.as_str(),
                p.result.id.as_str(),
                p.result.name.as_str(),
                p.dice.as_deref().unwrap(),
                p.roll.as_deref().unwrap(),
            )
        })
        .collect();
    assert_eq!(
        parts,
        [
            (FOUNDER, "oswald", "Oswald", "1d100", "28"),
            (FOUNDER_FORM, "b", "B", "1d4", "3"),
        ]
    );
    let assembly = naming.assembly.unwrap();
    assert_eq!(assembly.join, "curated_variant");
    let curated = assembly.curated.unwrap();
    assert_eq!(
        (
            curated.table.as_str(),
            curated.source.as_str(),
            curated.variant.as_str()
        ),
        (FOUNDER, "oswald", "b")
    );
    assert_eq!(naming.name.as_deref(), Some("Oswaldwick"));
    let routing: Vec<&str> = naming.routing.iter().map(|t| t.id.as_str()).collect();
    assert_eq!(routing, ["world.kurovia", "region.ordon"]);
}

// ----- the complete catalogue ---------------------------------------------------------------

#[test]
fn the_catalogue_has_1530_paths_and_1497_spellings() {
    let c = content();
    let paths = catalogue(&c);
    let per_method: BTreeMap<&str, usize> = paths.iter().fold(BTreeMap::new(), |mut m, (k, _)| {
        *m.entry(k.as_str()).or_default() += 1;
        m
    });
    assert_eq!(
        per_method,
        BTreeMap::from([
            ("founder", 200),
            ("geographic", 1000),
            ("industrial", 160),
            ("inherited", 50),
            ("route", 120),
        ])
    );
    assert_eq!(paths.len(), 1530);
    let spellings: BTreeSet<String> = paths.iter().map(|(_, n)| n.to_lowercase()).collect();
    assert_eq!(spellings.len(), 1497);
    let mut seen: HashMap<String, usize> = HashMap::new();
    for (_, n) in &paths {
        *seen.entry(n.to_lowercase()).or_default() += 1;
    }
    assert_eq!(seen.values().filter(|n| **n > 1).count(), 33);
    for banned in ["Oswaldsworth", "Foundrywell", "Market Market", "Grand Road"] {
        assert!(
            paths.iter().all(|(_, n)| n != banned),
            "{banned} must never be produced"
        );
    }
}

/// The engine produces exactly the names the authoring tool catalogued.
/// After editing the pack, run `AUTHORING_PREVIEW.py --export` again.
#[test]
fn the_engine_matches_the_authoring_catalogue() {
    let c = content();
    let csv = std::fs::read_to_string(format!("{AUTHORING}/COMPLETE_CATALOG.csv")).unwrap();
    let mut expected: Vec<(String, String)> = read_csv(&csv)
        .into_iter()
        .map(|row| (row["method"].clone(), row["name"].clone()))
        .collect();
    let mut actual = catalogue(&c);
    expected.sort();
    actual.sort();
    assert_eq!(
        actual, expected,
        "re-run authoring/naming/ordon/AUTHORING_PREVIEW.py --export"
    );
}

#[test]
fn automatic_generation_only_produces_catalogued_names() {
    let c = content();
    let names: BTreeSet<String> = catalogue(&c).into_iter().map(|(_, n)| n).collect();
    let mut selections = Selections::new();
    selections.insert("table.settingProfile".into(), "kurovia".into());
    selections.insert("table.kuroviaRegion".into(), "ordon".into());
    let mut methods = BTreeSet::new();
    for seed in 0..200 {
        let s = GenerationSession::automatic(
            Arc::new(c.clone()),
            GenerationOptions::default(),
            &selections,
            &mut RandomFaces::seeded(seed),
        )
        .unwrap();
        let region = s
            .view(REGION, ViewOptions::default())
            .unwrap()
            .result
            .unwrap();
        assert_eq!(region.id.as_str(), "ordon");
        let name = s.name().unwrap();
        assert!(names.contains(&name), "{name}");
        methods.insert(
            s.view(NAME, ViewOptions::default())
                .unwrap()
                .naming
                .unwrap()
                .method
                .unwrap()
                .id,
        );
    }
    assert_eq!(methods.len(), 5, "every method turns up: {methods:?}");
}

// ----- curated_variant: generic, validated, never improvised -----------------------------------

#[test]
fn a_missing_approved_name_is_a_clear_validation_error() {
    let mut c = content();
    table_mut(&mut c, FOUNDER).results[27]
        .approved_names
        .remove(&ResultId::new("b"));
    let report = validate(&c);
    let issue = report
        .errors()
        .find(|i| matches!(i.kind, IssueKind::ApprovedNameMissing { .. }))
        .expect("reported");
    assert_eq!(issue.location.table.as_ref().unwrap().as_str(), FOUNDER);
    assert_eq!(issue.location.result.as_ref().unwrap().as_str(), "oswald");
    assert!(
        issue.message.contains("variant \"B\"") && issue.message.contains(FOUNDER_FORM),
        "{}",
        issue.message
    );
    // An empty name counts as missing.
    let mut c = content();
    table_mut(&mut c, FOUNDER).results[27]
        .approved_names
        .insert("b".into(), "  ".into());
    assert!(validate(&c).has_errors());
    // Generation refuses invalid content outright.
    assert!(matches!(
        GenerationSession::new(
            Arc::new(c),
            GenerationOptions::default(),
            GenerationMode::OneTable
        ),
        Err(GenerationError::InvalidContent(_))
    ));
}

#[test]
fn adding_a_variant_requires_a_name_from_every_source() {
    let mut c = content();
    let forms = table_mut(&mut c, INDUSTRY_FORM);
    forms.dice = Some("1d6".into());
    let mut e = forms.results[3].clone();
    e.id = "e".into();
    e.name = "E".into();
    e.range = Some(FaceRange::new(5, 6));
    forms.results.push(e);
    let missing = validate(&c)
        .errors()
        .filter(|i| matches!(&i.kind, IssueKind::ApprovedNameMissing { variant, .. } if variant.as_str() == "e"))
        .count();
    assert_eq!(missing, 40, "every industry source needs its E name");
    for r in &mut table_mut(&mut c, INDUSTRY).results {
        let name = format!("{} Green", r.name);
        r.approved_names.insert("e".into(), name);
    }
    assert!(!validate(&c).has_errors(), "{}", validate(&c));
    let foundry = face(&c, INDUSTRY, "foundry");
    assert_eq!(named(&c, &[63, foundry, 6]), "Foundry Green");
}

#[test]
fn stray_variant_keys_and_bad_join_settings_are_reported() {
    let mut c = content();
    table_mut(&mut c, FOUNDER).results[0]
        .approved_names
        .insert("z".into(), "Zedton".into());
    assert!(validate(&c).errors().any(|i| matches!(
        &i.kind,
        IssueKind::ApprovedNameUnknownVariant { variant, .. } if variant.as_str() == "z"
    )));

    let mut c = content();
    let founder = c
        .naming_schemes
        .iter_mut()
        .find(|s| s.id.as_str() == SCHEME)
        .unwrap()
        .methods
        .iter_mut()
        .find(|m| m.result.as_str() == "founder")
        .unwrap();
    founder.join.style = JoinStyle::CuratedVariant {
        source_field: "nicknames".into(),
        source_from_result_id: 0,
        variant_from_result_id: 1,
        on_missing: Default::default(),
    };
    assert!(
        validate(&c)
            .errors()
            .any(|i| matches!(i.kind, IssueKind::CuratedVariantField { .. }))
    );
    let founder = c
        .naming_schemes
        .iter_mut()
        .find(|s| s.id.as_str() == SCHEME)
        .unwrap()
        .methods
        .iter_mut()
        .find(|m| m.result.as_str() == "founder")
        .unwrap();
    founder.join.style = JoinStyle::CuratedVariant {
        source_field: "approved_names".into(),
        source_from_result_id: 0,
        variant_from_result_id: 2,
        on_missing: Default::default(),
    };
    assert!(
        validate(&c)
            .errors()
            .any(|i| matches!(i.kind, IssueKind::CuratedVariantComponents { .. }))
    );
}

#[test]
fn curated_variant_never_falls_back_to_joining_the_parts() {
    let mut c = content();
    table_mut(&mut c, FOUNDER).results[27]
        .approved_names
        .clear();
    let m = method(&c, "founder");
    let (founder, form) = (TableId::new(FOUNDER), TableId::new(FOUNDER_FORM));
    let (oswald, b) = (ResultId::new("oswald"), ResultId::new("b"));
    let parts = [
        NamePart {
            table: &founder,
            result: &oswald,
            text: "Oswald",
        },
        NamePart {
            table: &form,
            result: &b,
            text: "B",
        },
    ];
    assert!(matches!(
        m.compose(&c, &parts),
        Err(ComposeError::MissingApprovedName { .. })
    ));
}

#[test]
fn curated_variant_is_generic_for_any_scheme() {
    let c: ContentSet = serde_json::from_value(serde_json::json!({
        "sequence": [ "t.a", { "naming": true, "name": "Name" } ],
        "tables": [
            { "id": "t.a", "name": "A", "dice": "1d4", "results": [
                { "id": "x", "name": "X", "range": [1, 4], "tags": ["g.x"] } ] },
            { "id": "t.m", "name": "Methods", "dice": "1d4", "results": [
                { "id": "m", "name": "Only", "range": [1, 4] } ] },
            { "id": "t.form", "name": "Form", "dice": "1d6", "results": [
                { "id": "low", "name": "Low", "range": [1, 3] },
                { "id": "high", "name": "High", "range": [4, 6] } ] },
            { "id": "t.src", "name": "Source", "dice": "1d4", "results": [
                { "id": "mill", "name": "Mill", "range": [1, 4],
                  "approved_names": { "low": "Nether Mill", "high": "Up-Mill" } } ] }
        ],
        "tags": [ { "id": "g.x", "name": "X" } ],
        "naming_schemes": [ {
            "id": "n.x", "name": "X names", "when": { "has_tag": "g.x" },
            "method_table": "t.m",
            "methods": [ { "result": "m", "components": ["t.form", "t.src"],
                "join": { "style": "curated_variant", "source_from_result_id": 1,
                          "variant_from_result_id": 0 } } ]
        } ]
    }))
    .unwrap();
    assert!(!validate(&c).has_errors(), "{}", validate(&c));
    let mut s = session(&c, GenerationMode::OneTable);
    s.roll(0, &mut dice(&[1])).unwrap();
    s.roll(1, &mut dice(&[1, 5, 2])).unwrap();
    assert_eq!(s.name().as_deref(), Some("Up-Mill"));
}

// ----- manual parts, methods and rerolls ---------------------------------------------------

#[test]
fn one_part_can_be_chosen_or_rolled_with_the_others_kept() {
    let c = content();
    let mut s = ordon(&c);
    s.roll(NAME, &mut dice(&[50, 28, 3])).unwrap();
    assert_eq!(s.name().as_deref(), Some("Oswaldwick"));
    // Every option previews the name it would give.
    let view = s
        .view(NAME, ViewOptions::default())
        .unwrap()
        .naming
        .unwrap();
    assert_eq!(view.part_options.len(), 2);
    let forms = &view.part_options[1];
    let previews: Vec<(&str, Option<&str>)> = forms
        .options
        .iter()
        .zip(&forms.previews)
        .map(|(o, p)| (o.result.id.as_str(), p.as_deref()))
        .collect();
    assert_eq!(
        previews,
        [("a", Some("Oswalden")), ("b", Some("Oswaldwick"))]
    );

    // Choose form A: no die is rolled for it, and the trace says so.
    s.choose_name_part(NAME, 1, "a".into(), &mut dice(&[]))
        .unwrap();
    assert_eq!(s.name().as_deref(), Some("Oswalden"));
    let parts = s.view(NAME, shown()).unwrap().naming.unwrap().parts;
    assert!(!parts[0].chosen && parts[1].chosen);
    // Choose another founder, keeping form A.
    s.choose_name_part(NAME, 0, "godwin".into(), &mut dice(&[]))
        .unwrap();
    assert_eq!(s.name().as_deref(), Some("Godwinsbury"));
    // Roll the founder again with real dice.
    s.roll_name_part(NAME, 0, &mut dice(&[95])).unwrap();
    assert_eq!(s.name().as_deref(), Some("Nerysbury"));
    // The method roll is untouched throughout.
    let view = s.view(NAME, ViewOptions::default()).unwrap();
    assert_eq!(view.outcome.unwrap().roll.as_deref(), Some("50"));

    // A typed name has no parts to change.
    s.set_custom_name(NAME, "Riverside").unwrap();
    assert!(matches!(
        s.roll_name_part(NAME, 0, &mut dice(&[1])),
        Err(GenerationError::NameNotGenerated)
    ));
}

#[test]
fn choosing_a_method_rolls_only_its_own_parts_without_a_method_roll() {
    let c = content();
    let mut s = ordon(&c);
    s.roll(NAME, &mut dice(&[50, 28, 3])).unwrap();
    s.choose(NAME, "industrial".into(), &mut dice(&[40, 2]))
        .unwrap();
    assert_eq!(s.name().as_deref(), Some("Fouldham"));
    let view = s.view(NAME, shown()).unwrap();
    let outcome = view.outcome.unwrap();
    assert!(outcome.chosen && outcome.roll.is_none(), "no method die");
    let parts = view.naming.unwrap().parts;
    let tables: Vec<&str> = parts.iter().map(|p| p.table.id.as_str()).collect();
    assert_eq!(tables, [INDUSTRY, INDUSTRY_FORM], "founder parts are gone");
    s.choose(NAME, "inherited".into(), &mut dice(&[1])).unwrap();
    assert_eq!(s.name().as_deref(), Some("Netherleigh"));
}

#[test]
fn rerolling_the_name_or_the_settlement_keeps_the_state_consistent() {
    let c = content();
    let mut s = ordon(&c);
    s.roll(NAME, &mut dice(&[50, 28, 3])).unwrap();
    s.roll_remaining(&mut RandomFaces::seeded(5)).ok();
    s.choose(5, "town".into(), &mut dice(&[])).unwrap();
    s.roll_remaining(&mut RandomFaces::seeded(6)).unwrap();
    assert!(s.is_complete());
    let tags = s.active_tags();
    // A name reroll changes nothing else.
    s.roll(NAME, &mut dice(&[1, 1, 5])).unwrap();
    assert_eq!(s.name().as_deref(), Some("Alderwick"));
    assert_eq!(s.active_tags(), tags);
    assert!(s.is_complete());
    // Re-roll (the Completed Settlement action) rerolls everything,
    // including the name.
    let names: BTreeSet<String> = catalogue(&c).into_iter().map(|(_, n)| n).collect();
    let mut rerolled = BTreeSet::new();
    for seed in 0..10 {
        s.reroll_all(&mut RandomFaces::seeded(seed)).unwrap();
        if let Some(name) = s.name() {
            let region = s
                .view(REGION, ViewOptions::default())
                .unwrap()
                .result
                .unwrap();
            if region.id.as_str() == "ordon" {
                assert!(names.contains(&name), "{name}");
            }
            rerolled.insert(name);
        }
    }
    assert!(rerolled.len() > 1, "the name is rerolled too");
}

#[test]
fn incremental_and_one_table_generation_both_name_ordon() {
    let c = content();
    let mut s = session(&c, GenerationMode::Incremental);
    s.choose(PROFILE, "kurovia".into(), &mut dice(&[])).unwrap();
    s.advance().unwrap();
    s.roll(REGION, &mut dice(&[1])).unwrap();
    s.advance().unwrap();
    assert_eq!(s.cursor(), NAME);
    s.choose(NAME, "route".into(), &mut dice(&[53, 4])).unwrap();
    assert_eq!(s.name().as_deref(), Some("Marketgate"));
    s.choose_name_part(NAME, 1, "a".into(), &mut dice(&[]))
        .unwrap();
    assert_eq!(s.name().as_deref(), Some("Market Cross"));
    s.advance().unwrap();

    let mut s = ordon(&c);
    s.roll(NAME, &mut dice(&[81, 53, 2])).unwrap();
    assert_eq!(s.name().as_deref(), Some("Market Bridge"));
    assert_eq!(
        s.view(NAME, ViewOptions::default()).unwrap().status,
        StageStatus::Resolved
    );
}

// ----- routing ----------------------------------------------------------------------------

#[test]
fn only_kurovia_and_ordon_route_to_the_ordon_scheme() {
    let c = content();
    let expected = [
        (1, "naming.kurovia.ordon"),
        (2, "naming.kurovia.halingo"),
        (3, "naming.kurovia.skgurr"),
        (4, "naming.kurovia.shinkira"),
        (5, "naming.kurovia.bratislor"),
        (6, "naming.kurovia.hjartheland"),
        (7, "naming.kurovia.nareth"),
        (8, "naming.kurovia.targoth"),
    ];
    for (face, scheme) in expected {
        let s = at_region(&c, face);
        let view = s.view(NAME, ViewOptions::default()).unwrap();
        assert_eq!(view.naming.unwrap().scheme.unwrap().id.as_str(), scheme);
    }
    // Ordon's scheme needs both tags.
    let ordon = scheme(&c);
    assert!(!ordon.when.evaluate(&|t| t.as_str() == "region.ordon"));
    assert!(!ordon.when.evaluate(&|t| t.as_str() == "world.kurovia"));
    assert!(
        ordon
            .when
            .evaluate(&|t| ["world.kurovia", "region.ordon"].contains(&t.as_str()))
    );
    // Routing is by tag, not by display name: renaming the region changes
    // nothing.
    let mut renamed = c.clone();
    let region = table_mut(&mut renamed, "table.kuroviaRegion");
    region.results[0].name = "The Ordon Marches".into();
    let s = at_region(&renamed, 1);
    let view = s.view(NAME, ViewOptions::default()).unwrap();
    assert_eq!(view.naming.unwrap().scheme.unwrap().id.as_str(), SCHEME);
    // Exactly one Ordon scheme is registered.
    assert_eq!(
        c.naming_schemes
            .iter()
            .filter(|s| s.id.as_str() == SCHEME)
            .count(),
        1
    );
}

/// `curated_variant` is additive: no other region uses it or any Ordon
/// table, and each keeps the joins its own content declares.
#[test]
fn the_other_regions_keep_their_own_joining() {
    let c = content();
    for region in [
        "halingo",
        "skgurr",
        "shinkira",
        "bratislor",
        "hjartheland",
        "nareth",
        "targoth",
    ] {
        let id = format!("naming.kurovia.{region}");
        let s = c.naming_scheme(&id.as_str().into()).unwrap();
        assert!(
            s.method_table
                .as_str()
                .starts_with(&format!("table.naming.{region}."))
        );
        for m in &s.methods {
            assert_ne!(m.join.style.key(), "curated_variant", "{id} › {}", m.result);
            for t in &m.components {
                assert!(
                    !t.as_str().starts_with("table.naming.ordon"),
                    "{id} uses Ordon content"
                );
            }
        }
    }
}

// ----- snapshots, persistence and overrides ---------------------------------------------------

fn finished(c: &ContentSet, faces: &[u32]) -> GenerationSession {
    let mut s = ordon(c);
    s.roll(NAME, &mut dice(faces)).unwrap();
    s.roll_remaining(&mut RandomFaces::seeded(5)).ok();
    s.choose(5, "town".into(), &mut dice(&[])).unwrap();
    s.roll_remaining(&mut RandomFaces::seeded(6)).unwrap();
    assert!(s.is_complete());
    s
}

fn name_record(snapshot: &SettlementSnapshot) -> NameRecord {
    snapshot
        .stages
        .iter()
        .find(|s| s.stage_table.as_str() == NAME_STAGE_ID)
        .and_then(|s| s.name.clone())
        .unwrap()
}

#[test]
fn the_snapshot_records_how_the_name_was_assembled() {
    let c = content();
    let record = name_record(&finished(&c, &[50, 28, 3]).to_snapshot());
    assert_eq!(record.name, "Oswaldwick");
    assert_eq!(record.source, NameSource::Generated);
    let assembly = record.assembly.unwrap();
    assert_eq!(assembly.join, "curated_variant");
    assert_eq!(assembly.curated.unwrap().variant.as_str(), "b");
    let ids: Vec<&str> = record.rolls.iter().map(|r| r.result.id.as_str()).collect();
    assert_eq!(ids, ["founder", "oswald", "b"]);

    let ash = face(&c, "table.naming.ordon.firstElement", "ash");
    let record = name_record(&finished(&c, &[1, ash, 2]).to_snapshot());
    assert_eq!(record.name, "Asham");
    assert_eq!(record.assembly.unwrap().exception.unwrap().name, "Asham");

    // A typed name does not pretend to come from dice.
    let mut s = finished(&c, &[1, 1, 5]);
    s.set_custom_name(NAME, "Riverside").unwrap();
    let record = name_record(&s.to_snapshot());
    assert_eq!(record.source, NameSource::Custom);
    assert!(record.rolls.is_empty() && record.assembly.is_none() && record.method.is_none());
}

#[test]
fn records_saved_without_an_assembly_still_open() {
    let c = content();
    let mut snapshot = finished(&c, &[50, 28, 3]).to_snapshot();
    let mut json = serde_json::to_value(&snapshot).unwrap();
    // A record from before assemblies were saved.
    for stage in json["stages"].as_array_mut().unwrap() {
        if let Some(name) = stage.get_mut("name").and_then(|n| n.as_object_mut()) {
            name.remove("assembly");
        }
    }
    snapshot = serde_json::from_value(json).unwrap();
    let reopened = GenerationSession::from_snapshot(
        Arc::new(c),
        GenerationOptions::default(),
        GenerationMode::OneTable,
        &snapshot,
    )
    .unwrap();
    assert_eq!(reopened.name().as_deref(), Some("Oswaldwick"));
    assert_eq!(reopened.to_snapshot(), snapshot, "saved unchanged");
}

#[test]
fn saved_names_survive_saving_reloading_and_later_edits() {
    let mut service = AppService::new(Database::open_in_memory().unwrap());
    let content = service.content().unwrap();
    let mut active = service.start_generation().unwrap();
    {
        let s = &mut active.session;
        s.choose(PROFILE, "kurovia".into(), &mut dice(&[])).unwrap();
        s.roll(REGION, &mut dice(&[1])).unwrap();
        s.roll(NAME, &mut dice(&[50, 28, 3])).unwrap();
        s.roll_remaining(&mut RandomFaces::seeded(5)).ok();
        s.choose(5, "town".into(), &mut dice(&[])).unwrap();
        s.roll_remaining(&mut RandomFaces::seeded(6)).unwrap();
    }
    drop(content);
    let summary = service.save_new(&active, "Oswaldwick").unwrap();

    // The user edits the approved name, renames the founder and changes the
    // method weights afterwards.
    let mut founders = service.table_detail(&FOUNDER.into()).unwrap().table;
    let oswald = founders
        .results
        .iter_mut()
        .find(|r| r.id.as_str() == "oswald")
        .unwrap();
    oswald.name = "Oswold".into();
    oswald.approved_names.insert("b".into(), "Oswoldby".into());
    service.save_table(founders).unwrap();
    let mut methods = service.table_detail(&METHOD.into()).unwrap().table;
    methods.results[0].range = Some(FaceRange::new(1, 40));
    methods.results[1].range = Some(FaceRange::new(41, 62));
    service.save_table(methods).unwrap();

    let saved = service.saved_get(&summary.id).unwrap();
    let stage = saved
        .stages
        .iter()
        .find(|s| s.stage_table.as_str() == NAME_STAGE_ID)
        .unwrap();
    let record = stage.name.as_ref().unwrap();
    assert_eq!(record.name, "Oswaldwick");
    assert_eq!(record.rolls[1].result.name, "Oswald");
    assert_eq!(record.assembly.as_ref().unwrap().join, "curated_variant");
    let edit = service.start_edit(&summary.id).unwrap();
    assert_eq!(edit.session.name().as_deref(), Some("Oswaldwick"));

    // New generation uses the user's override at once.
    let content = service.content().unwrap();
    let mut s = at_region(&content, 1);
    s.roll(NAME, &mut dice(&[50, 28, 3])).unwrap();
    assert_eq!(s.name().as_deref(), Some("Oswoldby"));

    // Reset restores the bundled default.
    service.reset_table(&FOUNDER.into()).unwrap();
    let content = service.content().unwrap();
    let mut s = at_region(&content, 1);
    s.roll(NAME, &mut dice(&[50, 28, 3])).unwrap();
    assert_eq!(s.name().as_deref(), Some("Oswaldwick"));
    assert_eq!(
        service.saved_get(&summary.id).unwrap().summary.name,
        "Oswaldwick"
    );
}

#[test]
fn an_override_of_one_approved_name_lets_later_defaults_through() {
    let bundled = content();
    let mut layers = ContentLayers::new(bundled.clone(), UserOverrides::default());
    let mut founders = bundled.table(&FOUNDER.into()).unwrap().clone();
    founders.results[27]
        .approved_names
        .insert("b".into(), "Oswaldby".into());
    layers.put_table(founders).unwrap();

    // A later bundled update changes a different founder: the customisation
    // stays, the update flows through, and the user is told the default
    // changed.
    let mut updated = bundled.clone();
    table_mut(&mut updated, FOUNDER).results[0]
        .approved_names
        .insert("a".into(), "Aldrington".into());
    table_mut(&mut updated, FOUNDER).results[27]
        .approved_names
        .insert("a".into(), "Oswaldene".into());
    let merged = ContentLayers::new(updated, layers.overrides().clone()).merged();
    assert!(!merged.report.has_errors(), "{}", merged.report);
    let founders = merged.content.table(&FOUNDER.into()).unwrap();
    assert_eq!(
        founders.results[0].approved_names[&ResultId::new("a")],
        "Aldrington"
    );
    assert_eq!(
        founders.results[27].approved_names[&ResultId::new("b")],
        "Oswaldby"
    );
    assert!(
        merged
            .report
            .warnings()
            .any(|w| matches!(w.kind, IssueKind::DefaultChanged))
    );
}

#[test]
fn joining_rules_and_exceptions_can_be_customised_and_reset() {
    let mut service = AppService::new(Database::open_in_memory().unwrap());
    let detail = service.table_detail(&METHOD.into()).unwrap();
    assert_eq!(detail.naming_methods.len(), 5);
    let geographic = &detail.naming_methods[0];
    assert_eq!(geographic.method.name, "Old English & Landscape");
    assert_eq!(geographic.exceptions.len(), 17);
    assert!(!geographic.modified && geographic.built_in);
    // The founder table knows it needs an A and a B name per founder.
    let founder = service.table_detail(&FOUNDER.into()).unwrap();
    let slots: Vec<&str> = founder.approved_names[0]
        .variants
        .iter()
        .map(|v| v.name.as_str())
        .collect();
    assert_eq!(slots, ["A", "B"]);

    let mut exceptions = geographic.exceptions.clone();
    exceptions.push(SpellingException {
        results: vec!["oak".into(), "ton".into()],
        components: vec![],
        name: "Ockton".into(),
        note: "Local contraction".into(),
    });
    let check = service
        .check_naming_method(
            &SCHEME.into(),
            &"geographic".into(),
            geographic.join.clone(),
            exceptions.clone(),
        )
        .unwrap();
    assert!(check.is_acceptable(), "{:?}", check.issues);
    let saved = service
        .save_naming_method(
            &SCHEME.into(),
            &"geographic".into(),
            geographic.join.clone(),
            exceptions,
        )
        .unwrap();
    assert!(saved.naming_methods[0].modified);
    let content = service.content().unwrap();
    let oak = face(&content, "table.naming.ordon.firstElement", "oak");
    let mut s = at_region(&content, 1);
    s.roll(NAME, &mut dice(&[1, oak, 1])).unwrap();
    assert_eq!(s.name().as_deref(), Some("Ockton"));

    // An exception naming a result that does not exist is refused.
    let mut broken = saved.naming_methods[0].exceptions.clone();
    broken[0].results[0] = "nowhere".into();
    assert!(
        service
            .save_naming_method(
                &SCHEME.into(),
                &"geographic".into(),
                geographic.join.clone(),
                broken,
            )
            .is_err()
    );

    service
        .reset_naming_method(&SCHEME.into(), &"geographic".into())
        .unwrap();
    let content = service.content().unwrap();
    let mut s = at_region(&content, 1);
    s.roll(NAME, &mut dice(&[1, oak, 1])).unwrap();
    assert_eq!(s.name().as_deref(), Some("Oakton"));
    assert!(!service.table_detail(&METHOD.into()).unwrap().naming_methods[0].modified);
}

#[test]
fn god_mode_never_bypasses_approved_name_validation() {
    let mut c = content();
    table_mut(&mut c, FOUNDER).results[27]
        .approved_names
        .clear();
    assert!(
        GenerationSession::new(
            Arc::new(c),
            GenerationOptions {
                god_mode: true,
                ..Default::default()
            },
            GenerationMode::OneTable,
        )
        .is_err()
    );
}
