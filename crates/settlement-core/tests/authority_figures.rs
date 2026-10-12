//! Authority Figures: the bundled tables, generation over the session, the
//! one-relationship-per-pair rule, names, manual choices and snapshots.

use std::collections::HashSet;
use std::sync::Arc;

use serde_json::json;
use settlement_core::bundled::bundled_content;
use settlement_core::content::{ContentSet, Table};
use settlement_core::dice::{RandomFaces, ScriptedFaces};
use settlement_core::engine::{GenerationError, GenerationOptions, SkipReason};
use settlement_core::figures::{AuthorityFigures, FIGURE_FIELDS, FigureName};
use settlement_core::session::{GenerationMode, GenerationSession, StageStatus, ViewOptions};
use settlement_core::settlements::SettlementSnapshot;

const AUTHORITY: usize = 10;
const AUTHORITY_SIZE: usize = 12;
const ROLES: usize = 13;
const FIGURES: usize = 14;

/// Generates the Authority Figures, allocating their roles first if they
/// are not allocated yet (roles always come first).
fn roll_figures(
    s: &mut GenerationSession,
    faces: &mut dyn settlement_core::dice::FaceSource,
) -> Result<settlement_core::session::StageChange, GenerationError> {
    let roles = s.view(ROLES, ViewOptions::default()).unwrap();
    if roles.status == settlement_core::session::StageStatus::Pending {
        s.roll(ROLES, &mut RandomFaces::seeded(99))?;
    }
    s.roll(FIGURES, faces)
}

fn content() -> ContentSet {
    bundled_content().unwrap()
}

fn table<'a>(c: &'a ContentSet, id: &str) -> &'a Table {
    c.table(&id.into())
        .unwrap_or_else(|| panic!("missing {id}"))
}

/// `(min, max, result name)` of a table's base layout, checking that every
/// face of `faces` is claimed exactly once.
fn layout(t: &Table, dice: &str, faces: i32) -> Vec<(i32, i32, String)> {
    assert_eq!(t.dice.as_deref(), Some(dice), "{}", t.id);
    for face in 1..=faces {
        let claims = t
            .results
            .iter()
            .filter(|r| r.range.unwrap().contains(face))
            .count();
        assert_eq!(claims, 1, "{}: face {face}", t.id);
    }
    t.results
        .iter()
        .map(|r| {
            let range = r.range.unwrap();
            (range.min, range.max, r.name.clone())
        })
        .collect()
}

fn rows(spec: &[(i32, i32, &str)]) -> Vec<(i32, i32, String)> {
    spec.iter()
        .map(|(a, b, n)| (*a, *b, n.to_string()))
        .collect()
}

// ----- the bundled tables -------------------------------------------------------

#[test]
fn species_is_the_temporary_kurovia_d20() {
    let c = content();
    let t = table(&c, "table.authorityFigure.species.kurovia");
    assert_eq!(
        layout(t, "1d20", 20),
        rows(&[
            (1, 4, "Human"),
            (5, 6, "High Elf"),
            (7, 8, "Wood Elf"),
            (9, 9, "Dark Elf"),
            (10, 10, "Dragonborn"),
            (11, 11, "Earth Genasi"),
            (12, 12, "Air Genasi"),
            (13, 13, "Fire Genasi"),
            (14, 14, "Water Genasi"),
            (15, 15, "Yuan-Ti"),
            (16, 17, "Dwarf"),
            (18, 18, "Eladrin"),
            (19, 20, "Firbolg"),
        ])
    );
    let tags: Vec<_> = t.results.iter().map(|r| r.tags[0].as_str()).collect();
    assert_eq!(
        tags,
        [
            "species.human",
            "species.highElf",
            "species.woodElf",
            "species.darkElf",
            "species.dragonborn",
            "species.earthGenasi",
            "species.airGenasi",
            "species.fireGenasi",
            "species.waterGenasi",
            "species.yuanTi",
            "species.dwarf",
            "species.eladrin",
            "species.firbolg",
        ]
    );
    // Kurovia reaches it through the generic species table.
    let generic = table(&c, "table.authorityFigure.species");
    assert!(generic.results.is_empty());
}

#[test]
fn gender_age_and_personality_tables() {
    let c = content();
    let gender = table(&c, "table.authorityFigure.gender");
    assert_eq!(
        layout(gender, "1d20", 20),
        rows(&[(1, 9, "Man"), (10, 18, "Woman"), (19, 20, "Non-binary")])
    );
    let ids: Vec<_> = gender.results.iter().map(|r| r.id.as_str()).collect();
    assert_eq!(ids, ["man", "woman", "nonBinary"]);
    assert_eq!(
        layout(table(&c, "table.authorityFigure.age"), "1d12", 12),
        rows(&[
            (1, 1, "Young Adult"),
            (2, 5, "Adult"),
            (6, 9, "Middle-aged"),
            (10, 11, "Older"),
            (12, 12, "Elderly"),
        ])
    );
    assert_eq!(
        layout(table(&c, "table.authorityFigure.disposition"), "1d12", 12),
        rows(&[
            (1, 1, "Hostile"),
            (2, 3, "Suspicious"),
            (4, 5, "Reserved"),
            (6, 7, "Neutral"),
            (8, 9, "Courteous"),
            (10, 11, "Friendly"),
            (12, 12, "Warm"),
        ])
    );
    assert_eq!(
        layout(table(&c, "table.authorityFigure.motive"), "1d12", 12),
        rows(&[
            (1, 2, "Protect the Settlement"),
            (3, 3, "Maintain Order"),
            (4, 4, "Increase Prosperity"),
            (5, 5, "Gain Influence"),
            (6, 6, "Uphold Tradition"),
            (7, 7, "Reform the Settlement"),
            (8, 8, "Serve Their Faith / Ideals"),
            (9, 9, "Protect Their Own People"),
            (10, 10, "Personal Wealth"),
            (11, 11, "Build a Legacy"),
            (12, 12, "Preserve Independence"),
        ])
    );
    assert_eq!(
        layout(table(&c, "table.authorityFigure.reputation"), "1d12", 12),
        rows(&[
            (1, 1, "Beloved"),
            (2, 3, "Respected"),
            (4, 5, "Trusted"),
            (6, 7, "Divisive"),
            (8, 8, "Feared"),
            (9, 10, "Disliked"),
            (11, 11, "Mocked"),
            (12, 12, "Mysterious"),
        ])
    );
    assert_eq!(
        layout(table(&c, "table.authorityFigure.flaw"), "1d12", 12),
        rows(&[
            (1, 1, "Arrogant"),
            (2, 2, "Indecisive"),
            (3, 3, "Greedy"),
            (4, 4, "Overly Trusting"),
            (5, 5, "Vindictive"),
            (6, 6, "Rigid / Inflexible"),
            (7, 7, "Secretive"),
            (8, 8, "Easily Influenced"),
            (9, 9, "Impulsive"),
            (10, 10, "Cowardly"),
            (11, 11, "Nepotistic"),
            (12, 12, "Obsessive"),
        ])
    );
}

#[test]
fn relationship_is_a_d10_without_tense_or_deep_admiration() {
    let c = content();
    let t = table(&c, "table.authorityRelationship");
    assert_eq!(
        layout(t, "1d10", 10),
        rows(&[
            (1, 1, "Hates"),
            (2, 2, "Despises"),
            (3, 3, "Distrusts"),
            (4, 4, "Dislikes"),
            (5, 6, "Neutral"),
            (7, 7, "Respects"),
            (8, 8, "Likes"),
            (9, 9, "Trusts"),
            (10, 10, "Admires"),
        ])
    );
    assert!(
        !t.results
            .iter()
            .any(|r| r.name.contains("Tense") || r.name.contains("Deeply"))
    );
}

// ----- generation -------------------------------------------------------------------

fn session(c: ContentSet, god_mode: bool) -> GenerationSession {
    GenerationSession::new(
        Arc::new(c),
        GenerationOptions { god_mode },
        GenerationMode::OneTable,
    )
    .unwrap()
}

/// A One-table session in Kurovia with every stage before Authority rolled,
/// the given Authority (and Authority Size for a group) chosen.
fn with_authority(c: ContentSet, authority: &str, size: Option<&str>) -> GenerationSession {
    let mut s = session(c, false);
    s.choose(0, "kurovia".into(), &mut RandomFaces::seeded(1))
        .unwrap();
    s.roll_remaining(&mut RandomFaces::seeded(2)).unwrap();
    s.choose(AUTHORITY, authority.into(), &mut RandomFaces::seeded(3))
        .unwrap();
    if let Some(size) = size {
        s.choose(AUTHORITY_SIZE, size.into(), &mut RandomFaces::seeded(4))
            .unwrap();
    }
    s
}

fn figures(s: &GenerationSession) -> AuthorityFigures {
    s.view(
        FIGURES,
        ViewOptions {
            show_tags: true,
            debug: false,
        },
    )
    .unwrap()
    .figures
    .unwrap()
    .record
    .expect("figures generated")
}

#[test]
fn a_single_ruler_has_one_figure_and_no_relationships() {
    let mut s = with_authority(content(), "noble", None);
    roll_figures(&mut s, &mut RandomFaces::seeded(5)).unwrap();
    assert!(s.is_complete());
    let f = figures(&s);
    assert_eq!(f.request.size, 1);
    assert_eq!(f.request.authority.name, "Noble / Landed Ruler");
    assert_eq!(f.figures.len(), 1);
    assert!(f.relationships.is_empty());
}

#[test]
fn groups_get_exactly_their_size_and_one_relationship_per_pair() {
    for (size, count) in [
        ("two", 2),
        ("three", 3),
        ("four", 4),
        ("five", 5),
        ("six", 6),
    ] {
        let mut s = with_authority(content(), "council", Some(size));
        roll_figures(&mut s, &mut RandomFaces::seeded(6)).unwrap();
        let f = figures(&s);
        assert_eq!(f.figures.len(), count, "{size}");
        assert_eq!(f.relationships.len(), count * (count - 1) / 2, "{size}");

        let ids: HashSet<_> = f.figures.iter().map(|x| x.id.clone()).collect();
        assert_eq!(ids.len(), count, "figure IDs are unique");
        let mut pairs = HashSet::new();
        for r in &f.relationships {
            assert_ne!(r.a, r.b, "no self-relationship");
            assert!(ids.contains(&r.a) && ids.contains(&r.b));
            // Unordered: A↔B and B↔A are the same pair, recorded once.
            let pair = if r.a < r.b {
                (r.a.clone(), r.b.clone())
            } else {
                (r.b.clone(), r.a.clone())
            };
            assert!(pairs.insert(pair), "duplicate pair");
            assert_eq!(r.roll.dice.as_deref(), Some("1d10"));
            assert!((1..=10).contains(&r.roll.attempts[0].roll.total));
        }
    }
}

#[test]
fn every_figure_has_every_field_rolled_with_physical_dice() {
    let mut s = with_authority(content(), "elders", Some("three"));
    roll_figures(&mut s, &mut RandomFaces::seeded(7)).unwrap();
    for (i, figure) in figures(&s).figures.iter().enumerate() {
        assert!(figure.id.as_str().starts_with("npc-"));
        // Roles were allocated first: the first figure leads.
        let role = figure.role.as_ref().expect("a role");
        let expected = if i == 0 {
            settlement_core::roles::Allocation::Lead
        } else {
            settlement_core::roles::Allocation::Member
        };
        assert_eq!(role.allocation, expected, "figure {}", i + 1);
        let fields: Vec<_> = figure.fields.iter().map(|f| f.field.as_str()).collect();
        let expected: Vec<_> = FIGURE_FIELDS.iter().map(|f| f.id).collect();
        assert_eq!(fields, expected);
        for f in &figure.fields {
            assert!(!f.roll.selected);
            assert!(f.roll.dice.is_some() && !f.roll.attempts.is_empty());
        }
        // Species comes from the Kurovia list, through the redirect.
        let species = figure.field("species").unwrap();
        assert_eq!(
            species.table.id.as_str(),
            "table.authorityFigure.species.kurovia"
        );
        // The figure's own tags (species, gender) are the figure's, not the
        // settlement's.
        let tags: Vec<_> = figure.tags.iter().map(|t| t.id.as_str()).collect();
        assert!(tags.iter().any(|t| t.starts_with("species.")), "{tags:?}");
        assert!(tags.iter().any(|t| t.starts_with("gender.")), "{tags:?}");
        // Every Kurovia species has a name table, so every figure is named.
        assert_eq!(
            Some(figure.label.as_str()),
            figure.name.text(),
            "figure {}",
            i + 1
        );
        assert_eq!(figure.app_version, settlement_core::VERSION);
    }
    assert!(
        !s.active_tags()
            .iter()
            .any(|t| t.id.as_str().starts_with("species.")),
        "figure tags never become settlement tags"
    );
}

/// Bundled content plus a user-added species with no name table.
fn with_custom_species() -> ContentSet {
    let mut c = content();
    let species = c
        .tables
        .iter_mut()
        .find(|t| t.id.as_str() == "table.authorityFigure.species.kurovia")
        .unwrap();
    species.dice = Some("1d20".into());
    let human = species
        .results
        .iter_mut()
        .find(|r| r.id.as_str() == "human")
        .unwrap();
    human.range = Some(settlement_core::content::FaceRange::new(1, 3));
    species.results.push(
        serde_json::from_value(json!({
            "id": "user.result.giant", "name": "Giant", "range": 4,
            "tags": ["user.tag.giant"]
        }))
        .unwrap(),
    );
    c.tags
        .push(serde_json::from_value(json!({ "id": "user.tag.giant", "name": "Giant" })).unwrap());
    c
}

#[test]
fn figures_are_reproducible_with_physical_dice() {
    // A noble: one figure, seven fields, a name, no relationships.
    let mut s = with_authority(content(), "noble", None);
    // Species 1 Human, Gender 12 Woman, Age 7 Middle-aged, Disposition 4
    // Reserved, Motive 12 Preserve Independence, Reputation 2 Respected,
    // Flaw 6 Rigid / Inflexible; then the name: method 1, given 13 (Mila),
    // family 1 (Varen).
    let mut dice = ScriptedFaces::new([1, 12, 7, 4, 12, 2, 6, 1, 13, 1]);
    roll_figures(&mut s, &mut dice).unwrap();
    assert_eq!(dice.remaining(), 0, "no hidden rolls");
    let f = &figures(&s).figures[0];
    let results: Vec<_> = f
        .fields
        .iter()
        .map(|x| x.roll.result.name.as_str())
        .collect();
    assert_eq!(
        results,
        [
            "Human",
            "Woman",
            "Middle-aged",
            "Reserved",
            "Preserve Independence",
            "Respected",
            "Rigid / Inflexible"
        ]
    );
    let FigureName::Generated {
        name,
        species,
        scheme,
        method,
        parts,
        rolls,
    } = &f.name
    else {
        panic!("generated: {:?}", f.name);
    };
    assert_eq!(name, "Mila Varen");
    assert_eq!(f.label, "Mila Varen");
    assert_eq!(species.as_ref().unwrap().name, "Human");
    assert_eq!(scheme.id.as_str(), "naming.person.human.feminine");
    assert_eq!(method.id.as_str(), "givenSurname");
    // Provenance: the given and family results, and every die.
    let ids: Vec<_> = parts
        .iter()
        .map(|p| (p.id.as_str(), p.result.id.as_str()))
        .collect();
    assert_eq!(ids, [("given", "mila"), ("family", "varen")]);
    let dice: Vec<_> = rolls
        .iter()
        .map(|r| {
            (
                r.table.id.as_str(),
                r.dice.as_deref().unwrap(),
                r.attempts[0].roll.total,
            )
        })
        .collect();
    assert_eq!(
        dice,
        [
            ("table.personName.human.method", "1d4", 1),
            ("table.personName.human.given.feminine", "1d20", 13),
            ("table.personName.human.family", "1d20", 1),
        ]
    );
}

#[test]
fn a_species_without_a_name_table_is_a_clear_state_not_a_fallback() {
    let mut s = with_authority(with_custom_species(), "noble", None);
    // Species 4 = the user's Giant.
    roll_figures(&mut s, &mut ScriptedFaces::new([4, 12, 7, 4, 12, 2, 6])).unwrap();
    let f = &figures(&s).figures[0];
    assert_eq!(f.field("species").unwrap().result.name, "Giant");
    assert_eq!(
        f.name,
        FigureName::Missing {
            reason: "No name table configured for Giant.".into()
        }
    );
    assert_eq!(f.label, "Figure 1", "never a borrowed Human name");
    // Asking for a generated name fails clearly; a typed one always works.
    let err = s
        .roll_figure_name(FIGURES, 0, &mut RandomFaces::seeded(1))
        .unwrap_err();
    assert!(matches!(err, GenerationError::NoFigureNamingScheme));
    assert!(err.to_string().contains("no name table"));
    s.set_figure_name(FIGURES, 0, "  Hrolda Stonebrow ")
        .unwrap();
    let f = &figures(&s).figures[0];
    assert_eq!(f.name.text(), Some("Hrolda Stonebrow"));
    assert_eq!(f.label, "Hrolda Stonebrow");
    assert!(s.set_figure_name(FIGURES, 0, "   ").is_err());
    // No name parts are offered for a name that was not generated.
    let view = s
        .view(FIGURES, ViewOptions::default())
        .unwrap()
        .figures
        .unwrap();
    assert!(view.name_parts[0].is_empty());
}

#[test]
fn a_new_species_routes_the_name_afresh_and_never_borrows_a_pool() {
    let mut s = with_authority(content(), "noble", None);
    roll_figures(
        &mut s,
        &mut ScriptedFaces::new([1, 12, 7, 4, 12, 2, 6, 1, 13, 1]),
    )
    .unwrap();
    // Human → Dark Elf (chosen): the name comes from the Dark Elf tables.
    s.choose_figure_field(
        FIGURES,
        0,
        "species",
        "darkElf".into(),
        &mut RandomFaces::seeded(1),
    )
    .unwrap();
    let f = &figures(&s).figures[0];
    assert!(f.field("species").unwrap().selected);
    let FigureName::Generated { scheme, rolls, .. } = &f.name else {
        panic!("{:?}", f.name);
    };
    assert_eq!(scheme.id.as_str(), "naming.person.darkElf.feminine");
    assert!(
        rolls
            .iter()
            .all(|r| r.table.id.as_str().starts_with("table.personName.darkElf."))
    );
    assert_eq!(f.label, f.name.text().unwrap());
}

#[test]
fn a_typed_figure_name_survives_a_change_of_species() {
    let mut s = with_authority(content(), "noble", None);
    roll_figures(&mut s, &mut RandomFaces::seeded(8)).unwrap();
    s.set_figure_name(FIGURES, 0, "Tess").unwrap();
    s.choose_figure_field(
        FIGURES,
        0,
        "species",
        "dwarf".into(),
        &mut RandomFaces::seeded(1),
    )
    .unwrap();
    assert_eq!(figures(&s).figures[0].name.text(), Some("Tess"));
}

#[test]
fn name_parts_can_be_chosen_from_the_species_own_tables() {
    let mut s = with_authority(content(), "noble", None);
    roll_figures(
        &mut s,
        &mut ScriptedFaces::new([1, 12, 7, 4, 12, 2, 6, 1, 13, 1]),
    )
    .unwrap();
    let view = s
        .view(FIGURES, ViewOptions::default())
        .unwrap()
        .figures
        .unwrap();
    let parts = &view.name_parts[0];
    let tables: Vec<_> = parts
        .iter()
        .map(|p| {
            (
                p.id.as_str(),
                p.label.as_str(),
                p.table.id.as_str(),
                p.options.len(),
            )
        })
        .collect();
    // Only the pools for a Human woman: no masculine, neutral or Elf pools.
    assert_eq!(
        tables,
        [
            (
                "given",
                "Given name",
                "table.personName.human.given.feminine",
                20
            ),
            ("family", "Family name", "table.personName.human.family", 20),
        ]
    );
    // Choose another family name; the given name stays.
    s.choose_figure_name_part(
        FIGURES,
        0,
        1,
        "tremayne".into(),
        &mut RandomFaces::seeded(1),
    )
    .unwrap();
    assert_eq!(figures(&s).figures[0].name.text(), Some("Mila Tremayne"));
    // Reroll the given name with a physical die.
    s.roll_figure_name_part(FIGURES, 0, 0, &mut ScriptedFaces::new([1]))
        .unwrap();
    let f = &figures(&s).figures[0];
    assert_eq!(f.name.text(), Some("Adelis Tremayne"));
    assert_eq!(f.label, "Adelis Tremayne");
    let FigureName::Generated { parts, rolls, .. } = &f.name else {
        panic!();
    };
    assert_eq!(parts[0].result.id.as_str(), "adelis");
    assert!(rolls[2].selected, "the family name was chosen");
    // A Wood Elf result is not a Human given name.
    assert!(
        s.choose_figure_name_part(FIGURES, 0, 0, "bryony".into(), &mut RandomFaces::seeded(1))
            .is_err()
    );
    assert!(
        s.choose_figure_name_part(FIGURES, 0, 5, "mila".into(), &mut RandomFaces::seeded(1))
            .is_err()
    );
}

#[test]
fn fields_and_relationships_can_be_chosen_or_rerolled_individually() {
    let mut s = with_authority(content(), "council", Some("three"));
    roll_figures(&mut s, &mut RandomFaces::seeded(9)).unwrap();
    let before = figures(&s);

    s.choose_figure_field(
        FIGURES,
        1,
        "motive",
        "buildLegacy".into(),
        &mut RandomFaces::seeded(1),
    )
    .unwrap();
    s.roll_figure_field(FIGURES, 2, "age", &mut ScriptedFaces::new([12]))
        .unwrap();
    s.choose_relationship(FIGURES, 2, "admires".into(), &mut RandomFaces::seeded(1))
        .unwrap();
    s.roll_relationship(FIGURES, 0, &mut ScriptedFaces::new([3]))
        .unwrap();

    let after = figures(&s);
    let motive = after.figures[1].field("motive").unwrap();
    assert!(motive.selected && motive.result.name == "Build a Legacy");
    let age = after.figures[2].field("age").unwrap();
    assert_eq!(
        (age.attempts[0].roll.total, age.result.name.as_str()),
        (12, "Elderly")
    );
    assert_eq!(after.relationships[2].roll.result.name, "Admires");
    assert!(after.relationships[2].roll.selected);
    assert_eq!(after.relationships[0].roll.result.name, "Distrusts");
    // Everything else is untouched, IDs included.
    assert_eq!(after.figures[0], before.figures[0]);
    assert_eq!(after.relationships[1], before.relationships[1]);
    for (a, b) in after.figures.iter().zip(&before.figures) {
        assert_eq!(a.id, b.id);
    }

    // Unknown results, fields, figures and relationships are refused.
    let mut faces = RandomFaces::seeded(1);
    assert!(
        s.choose_figure_field(FIGURES, 0, "motive", "nope".into(), &mut faces)
            .is_err()
    );
    assert!(
        s.choose_figure_field(FIGURES, 0, "height", "x".into(), &mut faces)
            .is_err()
    );
    assert!(s.roll_figure_field(FIGURES, 9, "age", &mut faces).is_err());
    assert!(s.roll_relationship(FIGURES, 3, &mut faces).is_err());
    assert_eq!(figures(&s), after, "failed actions change nothing");
}

#[test]
fn god_mode_lifts_result_eligibility_for_figure_fields_only() {
    // Make Dwarf ineligible in Kurovia (a customisation).
    let mut c = content();
    c.tables
        .iter_mut()
        .find(|t| t.id.as_str() == "table.authorityFigure.species.kurovia")
        .unwrap()
        .results
        .iter_mut()
        .find(|r| r.id.as_str() == "dwarf")
        .unwrap()
        .eligibility =
        Some(serde_json::from_value(json!({ "not": { "has_tag": "world.kurovia" } })).unwrap());

    let mut s = with_authority(c.clone(), "noble", None);
    roll_figures(&mut s, &mut RandomFaces::seeded(1)).unwrap();
    let view = s.view(FIGURES, ViewOptions::default()).unwrap();
    let species = &view.figures.unwrap().fields[0];
    assert!(
        !species
            .options
            .iter()
            .find(|o| o.result.id.as_str() == "dwarf")
            .unwrap()
            .legal
    );
    assert!(
        s.choose_figure_field(
            FIGURES,
            0,
            "species",
            "dwarf".into(),
            &mut RandomFaces::seeded(1)
        )
        .is_err()
    );

    let mut god = session(c, true);
    god.choose(0, "kurovia".into(), &mut RandomFaces::seeded(1))
        .unwrap();
    god.roll_remaining(&mut RandomFaces::seeded(2)).unwrap();
    god.choose(AUTHORITY, "noble".into(), &mut RandomFaces::seeded(3))
        .unwrap();
    // Changing the Authority invalidated the figures: generate them again.
    roll_figures(&mut god, &mut RandomFaces::seeded(4)).unwrap();
    let species = &god
        .view(FIGURES, ViewOptions::default())
        .unwrap()
        .figures
        .unwrap()
        .fields[0];
    assert!(species.options.iter().all(|o| o.legal));
    god.choose_figure_field(
        FIGURES,
        0,
        "species",
        "dwarf".into(),
        &mut RandomFaces::seeded(1),
    )
    .unwrap();
    // Still the Kurovia species table: God Mode never merges tables.
    assert_eq!(
        species.table.id.as_str(),
        "table.authorityFigure.species.kurovia"
    );
}

#[test]
fn changing_the_authority_invalidates_the_figures() {
    let mut s = with_authority(content(), "council", Some("four"));
    roll_figures(&mut s, &mut RandomFaces::seeded(10)).unwrap();
    assert!(s.is_complete());

    // A different size: the four figures no longer fit.
    let change = s
        .choose(AUTHORITY_SIZE, "two".into(), &mut RandomFaces::seeded(1))
        .unwrap();
    // The roles no longer fit either.
    assert_eq!(change.invalidated, [ROLES, FIGURES]);
    let v = s.view(FIGURES, ViewOptions::default()).unwrap();
    assert_eq!(v.status, StageStatus::Pending);
    assert_eq!(v.previous.unwrap().name, "4 Authority Figures");
    roll_figures(&mut s, &mut RandomFaces::seeded(11)).unwrap();
    assert_eq!(figures(&s).figures.len(), 2);

    // A different Authority of the same size: also redone.
    s.choose(AUTHORITY, "assembly".into(), &mut RandomFaces::seeded(1))
        .unwrap();
    let v = s.view(FIGURES, ViewOptions::default()).unwrap();
    assert_eq!(v.status, StageStatus::Pending);

    // Rerolling an unrelated earlier stage keeps them.
    roll_figures(&mut s, &mut RandomFaces::seeded(12)).unwrap();
    let kept = figures(&s);
    let change = s.roll(6, &mut RandomFaces::seeded(13)).unwrap(); // Origin
    assert!(!change.invalidated.contains(&FIGURES));
    assert_eq!(figures(&s), kept);
}

#[test]
fn figures_need_the_authority_first() {
    let mut s = session(content(), false);
    s.choose(0, "kurovia".into(), &mut RandomFaces::seeded(1))
        .unwrap();
    let err = s.roll(FIGURES, &mut RandomFaces::seeded(1)).unwrap_err();
    assert!(matches!(err, GenerationError::FiguresNeedAuthority));
    assert!(!s.view(FIGURES, ViewOptions::default()).unwrap().can_roll);
}

#[test]
fn automatic_generation_includes_the_figures() {
    let mut selections = settlement_core::engine::Selections::new();
    selections.insert("table.settingProfile".into(), "kurovia".into());
    let s = GenerationSession::automatic(
        Arc::new(content()),
        GenerationOptions::default(),
        &selections,
        &mut RandomFaces::seeded(14),
    )
    .unwrap();
    assert!(s.is_complete());
    let f = figures(&s);
    let expected =
        settlement_core::authority::AuthorityRepresentatives::from_log(&s.log()).unwrap();
    assert_eq!(f.request, expected);
    assert_eq!(f.figures.len(), expected.size as usize);
}

// ----- snapshots ---------------------------------------------------------------------

#[test]
fn snapshots_keep_the_figures_exactly() {
    let mut s = with_authority(content(), "guild", None);
    s.choose(11, "group".into(), &mut RandomFaces::seeded(1))
        .unwrap();
    s.choose(AUTHORITY_SIZE, "three".into(), &mut RandomFaces::seeded(1))
        .unwrap();
    roll_figures(&mut s, &mut RandomFaces::seeded(15)).unwrap();
    s.set_figure_name(FIGURES, 1, "Orven Daal").unwrap();
    let snapshot = s.to_snapshot();
    let saved = snapshot.authority_figures().unwrap().clone();
    assert_eq!(saved.figures.len(), 3);
    assert_eq!(saved.relationships.len(), 3);
    assert_eq!(saved.figures[1].name.text(), Some("Orven Daal"));
    let stage = snapshot
        .display_stages()
        .into_iter()
        .find(|st| st.stage_table.as_str() == "stage.authorityFigures")
        .unwrap();
    assert_eq!(stage.label, "Authority Figures");
    assert_eq!(stage.result.unwrap().name, "3 Authority Figures");

    // JSON round trip.
    let json = serde_json::to_string(&snapshot).unwrap();
    let back: SettlementSnapshot = serde_json::from_str(&json).unwrap();
    assert_eq!(back, snapshot);

    // Later content changes (renamed and re-ranged tables) change nothing.
    let mut changed = content();
    for t in changed.tables.iter_mut().filter(|t| {
        t.id.as_str().starts_with("table.authorityFigure")
            || t.id.as_str() == "table.authorityRelationship"
    }) {
        for r in &mut t.results {
            r.name = format!("{} (edited)", r.name);
        }
    }
    let reopened = GenerationSession::from_snapshot(
        Arc::new(changed),
        GenerationOptions::default(),
        GenerationMode::OneTable,
        &back,
    )
    .unwrap();
    assert!(reopened.is_complete());
    assert_eq!(reopened.to_snapshot(), snapshot);
    assert_eq!(back.authority_figures().unwrap(), &saved);
}

#[test]
fn settlements_saved_before_figures_existed_load_without_generating_any() {
    let mut s = with_authority(content(), "council", Some("two"));
    roll_figures(&mut s, &mut RandomFaces::seeded(16)).unwrap();
    let mut old = s.to_snapshot();
    old.stages
        .retain(|st| st.stage_table.as_str() != "stage.authorityFigures");
    let json = serde_json::to_value(&old).unwrap();
    assert!(!json.to_string().contains("authorityFigures"));

    let mut reopened = GenerationSession::from_snapshot(
        Arc::new(content()),
        GenerationOptions::default(),
        GenerationMode::OneTable,
        &old,
    )
    .unwrap();
    let v = reopened.view(FIGURES, ViewOptions::default()).unwrap();
    assert_eq!(v.skipped, Some(SkipReason::NotRecorded));
    assert!(v.figures.unwrap().record.is_none());
    assert!(reopened.is_complete());
    assert!(reopened.to_snapshot().authority_figures().is_none());
    // The user may generate them deliberately.
    roll_figures(&mut reopened, &mut RandomFaces::seeded(17)).unwrap();
    assert_eq!(figures(&reopened).figures.len(), 2);
}

#[test]
fn figure_tags_are_only_shown_with_tag_view() {
    let mut s = with_authority(content(), "noble", None);
    roll_figures(&mut s, &mut RandomFaces::seeded(18)).unwrap();
    let plain = s
        .view(FIGURES, ViewOptions::default())
        .unwrap()
        .figures
        .unwrap()
        .record
        .unwrap();
    assert!(plain.figures[0].tags.is_empty());
    assert_eq!(figures(&s).figures[0].tags.len(), 2, "species and gender");
}
