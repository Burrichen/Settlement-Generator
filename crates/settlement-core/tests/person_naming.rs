//! Species-specific Authority Figure names: routing by species (and gender,
//! where a species uses it), the bundled name tables, how each species puts
//! a name together, and saved names.

use std::collections::HashSet;
use std::sync::Arc;

use settlement_core::bundled::bundled_content;
use settlement_core::content::ContentSet;
use settlement_core::dice::{RandomFaces, ScriptedFaces};
use settlement_core::engine::{GenerationError, GenerationOptions};
use settlement_core::figures::{AuthorityFigure, FigureName};
use settlement_core::ids::TagId;
use settlement_core::naming::{NamingScheme, NamingSubject, scheme_for_subject};
use settlement_core::session::{GenerationMode, GenerationSession, ViewOptions};

const AUTHORITY: usize = 10;
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

/// Every current Kurovia species: (result ID, d20 face, shared name pool).
const SPECIES: [(&str, u32, bool); 13] = [
    ("human", 1, false),
    ("highElf", 5, false),
    ("woodElf", 7, false),
    ("darkElf", 9, false),
    ("dragonborn", 10, false),
    ("earthGenasi", 11, false),
    ("airGenasi", 12, false),
    ("fireGenasi", 13, true),
    ("waterGenasi", 14, true),
    ("yuanTi", 15, true),
    ("dwarf", 16, false),
    ("eladrin", 18, true),
    ("firbolg", 19, true),
];

const GENDERS: [&str; 3] = ["man", "woman", "nonBinary"];

fn content() -> ContentSet {
    bundled_content().unwrap()
}

/// The person scheme for a species and gender in Kurovia.
fn scheme<'c>(c: &'c ContentSet, species: &str, gender: &str) -> Option<&'c NamingScheme> {
    let tags: Vec<TagId> = [
        "world.kurovia".to_owned(),
        format!("species.{species}"),
        format!("gender.{gender}"),
    ]
    .into_iter()
    .map(TagId::new)
    .collect();
    scheme_for_subject(c, NamingSubject::Person, &tags)
}

/// Every table a scheme can roll: its method table and components.
fn tables(s: &NamingScheme) -> HashSet<String> {
    let mut out: HashSet<String> = s
        .methods
        .iter()
        .flat_map(|m| m.components.iter().map(|t| t.to_string()))
        .collect();
    out.insert(s.method_table.to_string());
    out
}

// ----- routing -----------------------------------------------------------------------

#[test]
fn every_species_and_gender_routes_to_its_own_species_tables() {
    let c = content();
    for (species, _, _) in SPECIES {
        for gender in GENDERS {
            let s = scheme(&c, species, gender)
                .unwrap_or_else(|| panic!("{species} ({gender}) has no name table"));
            assert!(
                s.id.as_str()
                    .starts_with(&format!("naming.person.{species}"))
            );
            let prefix = format!("table.personName.{species}.");
            for t in tables(s) {
                assert!(t.starts_with(&prefix), "{species} ({gender}) uses {t}");
            }
        }
    }
}

#[test]
fn human_and_elf_names_never_share_tables() {
    let c = content();
    let human = tables(scheme(&c, "human", "woman").unwrap());
    for elf in ["highElf", "woodElf", "darkElf", "eladrin"] {
        let other = tables(scheme(&c, elf, "woman").unwrap());
        assert!(human.is_disjoint(&other), "human and {elf}");
    }
    let high = tables(scheme(&c, "highElf", "man").unwrap());
    let wood = tables(scheme(&c, "woodElf", "man").unwrap());
    assert!(
        high.is_disjoint(&wood),
        "High Elf and Wood Elf are separate"
    );
}

#[test]
fn each_genasi_ancestry_has_its_own_route() {
    let c = content();
    let ancestries = ["earthGenasi", "airGenasi", "fireGenasi", "waterGenasi"];
    let schemes: Vec<_> = ancestries
        .iter()
        .map(|a| scheme(&c, a, "man").unwrap())
        .collect();
    let ids: HashSet<_> = schemes.iter().map(|s| s.id.as_str()).collect();
    assert_eq!(ids.len(), 4);
    for (i, a) in schemes.iter().enumerate() {
        for b in &schemes[i + 1..] {
            assert!(tables(a).is_disjoint(&tables(b)), "{} / {}", a.id, b.id);
        }
    }
}

#[test]
fn gender_selects_the_pool_where_a_species_uses_one() {
    let c = content();
    for (species, _, shared) in SPECIES {
        let given = |gender: &str| {
            scheme(&c, species, gender).unwrap().methods[0]
                .components
                .clone()
        };
        if shared {
            // One pool for everyone.
            assert_eq!(given("man"), given("woman"), "{species}");
            assert_eq!(given("man"), given("nonBinary"), "{species}");
        } else {
            let pool = |gender: &str| scheme(&c, species, gender).unwrap().id.to_string();
            assert_eq!(pool("man"), format!("naming.person.{species}.masculine"));
            assert_eq!(pool("woman"), format!("naming.person.{species}.feminine"));
            assert_eq!(
                pool("nonBinary"),
                format!("naming.person.{species}.neutral")
            );
            // A gender added later falls back to the species' neutral pool,
            // never to another species.
            assert_eq!(
                pool("user.gender.other"),
                format!("naming.person.{species}.neutral")
            );
        }
    }
}

#[test]
fn person_schemes_never_name_settlements() {
    let c = content();
    let tags = [TagId::new("world.kurovia"), TagId::new("species.human")];
    assert!(settlement_core::naming::scheme_for(&c, &tags).is_none());
}

// ----- the bundled name tables ----------------------------------------------------------

#[test]
fn every_name_table_is_dense_and_fully_covered() {
    let c = content();
    let report = settlement_core::validation::validate(&c);
    assert!(!report.has_errors(), "{report}");
    let mut seen = 0;
    for t in c
        .tables
        .iter()
        .filter(|t| t.id.as_str().starts_with("table.personName."))
    {
        seen += 1;
        let faces: i32 = match t.dice.as_deref() {
            Some("1d4") => 4,
            Some("1d20") => 20,
            Some("1d100") => 100,
            other => panic!("{}: {other:?}", t.id),
        };
        let size = t.results.len();
        for face in 1..=faces {
            let claims = t
                .results
                .iter()
                .filter(|r| r.range.unwrap().contains(face))
                .count();
            assert_eq!(claims, 1, "{}: face {face}", t.id);
        }
        let ids: HashSet<_> = t.results.iter().map(|r| r.id.as_str()).collect();
        let names: HashSet<_> = t.results.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(ids.len(), size, "{}: stable IDs are unique", t.id);
        assert_eq!(names.len(), size, "{}: names are unique", t.id);
    }
    // Every species' method table and name pools, including the revamped
    // packs' culture and sub-pool tables.
    assert!(seen > 1000, "{seen}");
}

#[test]
fn no_well_known_published_or_famous_names_are_used() {
    // Names from published game tables and famous fantasy characters.
    let avoid = [
        "Drizzt",
        "Elminster",
        "Bruenor",
        "Tordek",
        "Thorin",
        "Gimli",
        "Legolas",
        "Galadriel",
        "Elrond",
        "Arwen",
        "Aragorn",
        "Raistlin",
        "Tanis",
        "Tasslehoff",
        "Jarlaxle",
        "Baenre",
        "Mialee",
        "Ivellios",
        "Aramil",
        "Eberk",
        "Medrash",
        "Kava",
        "Nimue",
        "Isolde",
        "Battlehammer",
        "Fireforge",
        "Frostbeard",
        "Ironfist",
        "Amakiir",
        "Galanodel",
        "Varric",
        "Perrin",
        "Garet",
        "Oakenshield",
        "Underhill",
    ];
    let c = content();
    for t in c
        .tables
        .iter()
        .filter(|t| t.id.as_str().starts_with("table.personName."))
    {
        for r in &t.results {
            assert!(!avoid.contains(&r.name.as_str()), "{}: {}", t.id, r.name);
        }
    }
}

// ----- generated names --------------------------------------------------------------------

/// A noble (one figure) in Kurovia with the given faces for the figure:
/// species, gender, five more fields (all 1), then the name's dice.
fn noble(faces: &[u32]) -> (GenerationSession, AuthorityFigure) {
    let mut s = GenerationSession::new(
        Arc::new(content()),
        GenerationOptions::default(),
        GenerationMode::OneTable,
    )
    .unwrap();
    s.choose(0, "kurovia".into(), &mut RandomFaces::seeded(1))
        .unwrap();
    s.roll_remaining(&mut RandomFaces::seeded(2)).unwrap();
    s.choose(AUTHORITY, "noble".into(), &mut RandomFaces::seeded(3))
        .unwrap();
    let mut dice = ScriptedFaces::new(faces.iter().copied());
    roll_figures(&mut s, &mut dice).unwrap();
    assert_eq!(dice.remaining(), 0, "every die is used, none hidden");
    let figure = s
        .view(FIGURES, ViewOptions::default())
        .unwrap()
        .figures
        .unwrap()
        .record
        .unwrap()
        .figures[0]
        .clone();
    (s, figure)
}

fn generated(figure: &AuthorityFigure) -> (&str, Vec<(&str, &str)>) {
    let FigureName::Generated { name, parts, .. } = &figure.name else {
        panic!("generated: {:?}", figure.name);
    };
    (
        name,
        parts
            .iter()
            .map(|p| (p.id.as_str(), p.result.name.as_str()))
            .collect(),
    )
}

/// The name a table gives on a face.
fn at(table: &str, face: i32) -> String {
    let c = content();
    let t = c.tables.iter().find(|t| t.id.as_str() == table).unwrap();
    let r = t
        .results
        .iter()
        .find(|r| r.range.unwrap().contains(face))
        .unwrap();
    r.name.clone()
}

#[test]
fn each_species_puts_its_name_together_its_own_way() {
    // Dwarf man, method 1: given name + family name.
    let (_, f) = noble(&[16, 1, 1, 1, 1, 1, 1, 1, 2, 3]);
    let given = at("table.personName.dwarf.given.masculine", 2);
    let family = at("table.personName.dwarf.family", 3);
    assert_eq!(
        generated(&f),
        (
            format!("{given} {family}").as_str(),
            vec![("given", given.as_str()), ("family", family.as_str())]
        )
    );
    // Dragonborn woman: family name first.
    let (_, f) = noble(&[10, 10, 1, 1, 1, 1, 1, 1, 1, 1]);
    let family = at("table.personName.dragonborn.family", 1);
    let given = at("table.personName.dragonborn.given.feminine", 1);
    assert_eq!(
        generated(&f),
        (
            format!("{family} {given}").as_str(),
            vec![("family", family.as_str()), ("given", given.as_str())]
        )
    );
    // Firbolg (any gender): a single name from a d100 pool.
    let (_, f) = noble(&[19, 19, 1, 1, 1, 1, 1, 1, 3]);
    assert_eq!(generated(&f), ("Bannoch", vec![("given", "Bannoch")]));
    // Earth Genasi man: given name + family name.
    let (_, f) = noble(&[11, 1, 1, 1, 1, 1, 1, 1, 100, 20]);
    let given = at("table.personName.earthGenasi.given.masculine", 100);
    let family = at("table.personName.earthGenasi.family", 20);
    assert_eq!(
        generated(&f),
        (
            format!("{given} {family}").as_str(),
            vec![("given", given.as_str()), ("family", family.as_str())]
        )
    );
    // Air Genasi woman: given name + family name.
    let (_, f) = noble(&[12, 10, 1, 1, 1, 1, 1, 1, 1, 1]);
    let given = at("table.personName.airGenasi.given.feminine", 1);
    let family = at("table.personName.airGenasi.family", 1);
    assert_eq!(
        generated(&f),
        (
            format!("{given} {family}").as_str(),
            vec![("given", given.as_str()), ("family", family.as_str())]
        )
    );
    // High Elf non-binary: the neutral pool + house name.
    let (_, f) = noble(&[5, 19, 1, 1, 1, 1, 1, 1, 1, 1]);
    assert_eq!(
        generated(&f),
        (
            "Aethis Aranvelis",
            vec![("given", "Aethis"), ("house", "Aranvelis")]
        )
    );
}

#[test]
fn saved_names_survive_later_name_table_edits() {
    let (s, f) = noble(&[1, 10, 1, 1, 1, 1, 1, 1, 13, 1]);
    assert_eq!(generated(&f).0, "Mila Varen");
    let snapshot = s.to_snapshot();

    // Later the user renames the given name and the family name.
    let mut changed = content();
    for t in changed
        .tables
        .iter_mut()
        .filter(|t| t.id.as_str().starts_with("table.personName.human."))
    {
        for r in &mut t.results {
            r.name = format!("{}-edited", r.name);
        }
    }
    let reopened = GenerationSession::from_snapshot(
        Arc::new(changed),
        GenerationOptions::default(),
        GenerationMode::OneTable,
        &snapshot,
    )
    .unwrap();
    assert_eq!(reopened.to_snapshot(), snapshot);
    let saved = snapshot.authority_figures().unwrap();
    assert_eq!(saved.figures[0].name.text(), Some("Mila Varen"));
}

#[test]
fn renaming_a_name_result_keeps_routing_by_stable_id() {
    // A user renames "Mila" (ID `mila`): rolling it gives the new text.
    let mut c = content();
    c.tables
        .iter_mut()
        .find(|t| t.id.as_str() == "table.personName.human.given.feminine")
        .unwrap()
        .results
        .iter_mut()
        .find(|r| r.id.as_str() == "mila")
        .unwrap()
        .name = "Milla".into();
    let mut s = GenerationSession::new(
        Arc::new(c),
        GenerationOptions::default(),
        GenerationMode::OneTable,
    )
    .unwrap();
    s.choose(0, "kurovia".into(), &mut RandomFaces::seeded(1))
        .unwrap();
    s.roll_remaining(&mut RandomFaces::seeded(2)).unwrap();
    s.choose(AUTHORITY, "noble".into(), &mut RandomFaces::seeded(3))
        .unwrap();
    roll_figures(
        &mut s,
        &mut ScriptedFaces::new([1, 10, 1, 1, 1, 1, 1, 1, 13, 1]),
    )
    .unwrap();
    let record = s
        .view(FIGURES, ViewOptions::default())
        .unwrap()
        .figures
        .unwrap()
        .record
        .unwrap();
    let f = &record.figures[0];
    assert_eq!(f.name.text(), Some("Milla Varen"));
    let FigureName::Generated { parts, .. } = &f.name else {
        panic!();
    };
    assert_eq!(parts[0].result.id.as_str(), "mila");
}

#[test]
fn god_mode_never_merges_species_pools() {
    let mut s = GenerationSession::new(
        Arc::new(content()),
        GenerationOptions {
            god_mode: true,
            ..Default::default()
        },
        GenerationMode::OneTable,
    )
    .unwrap();
    s.choose(0, "kurovia".into(), &mut RandomFaces::seeded(1))
        .unwrap();
    s.roll_remaining(&mut RandomFaces::seeded(2)).unwrap();
    s.choose(AUTHORITY, "noble".into(), &mut RandomFaces::seeded(3))
        .unwrap();
    roll_figures(
        &mut s,
        &mut ScriptedFaces::new([16, 1, 1, 1, 1, 1, 1, 1, 1, 1]),
    )
    .unwrap();
    let view = s
        .view(FIGURES, ViewOptions::default())
        .unwrap()
        .figures
        .unwrap();
    let parts = &view.name_parts[0];
    assert_eq!(
        parts[0].table.id.as_str(),
        "table.personName.dwarf.given.masculine"
    );
    assert_eq!(parts[0].options.len(), 100);
    assert!(parts[0].options.iter().all(|o| o.legal));
}
