//! Authority Role Allocation: one role per Authority Figure, allocated
//! before the figures, with lead and member roles for groups and the
//! deterministic unique-role fallback.

use std::sync::Arc;

use serde_json::json;
use settlement_core::bundled::bundled_content;
use settlement_core::content::ContentSet;
use settlement_core::dice::{RandomFaces, ScriptedFaces};
use settlement_core::engine::{GenerationError, GenerationOptions, SkipReason};
use settlement_core::roles::{Allocation, RoleAllocation};
use settlement_core::session::{GenerationMode, GenerationSession, StageStatus, ViewOptions};
use settlement_core::settlements::SettlementSnapshot;
use settlement_core::validation::{IssueKind, Severity, validate};

const AUTHORITY: usize = 10;
const REPRESENTATION: usize = 11;
const AUTHORITY_SIZE: usize = 12;
const ROLES: usize = 13;
const FIGURES: usize = 14;

fn content() -> ContentSet {
    bundled_content().unwrap()
}

/// A One-table session in Kurovia with every stage rolled, then the given
/// Authority, Representation (for Guild and Military) and Authority Size
/// (for a group) chosen. Roles are left to the test.
fn with(c: ContentSet, authority: &str, group: Option<&str>) -> GenerationSession {
    let mut s = GenerationSession::new(
        Arc::new(c),
        GenerationOptions::default(),
        GenerationMode::OneTable,
    )
    .unwrap();
    s.choose(0, "kurovia".into(), &mut RandomFaces::seeded(1))
        .unwrap();
    s.roll_remaining(&mut RandomFaces::seeded(2)).unwrap();
    s.choose(AUTHORITY, authority.into(), &mut RandomFaces::seeded(3))
        .unwrap();
    if matches!(authority, "guild" | "military") {
        let rep = if group.is_some() { "group" } else { "single" };
        s.choose(REPRESENTATION, rep.into(), &mut RandomFaces::seeded(4))
            .unwrap();
    }
    if let Some(size) = group {
        s.choose(AUTHORITY_SIZE, size.into(), &mut RandomFaces::seeded(5))
            .unwrap();
    }
    s
}

fn roles(s: &GenerationSession) -> RoleAllocation {
    s.view(ROLES, ViewOptions::default())
        .unwrap()
        .roles
        .unwrap()
        .record
        .expect("roles allocated")
}

/// Allocates roles with exactly these dice (every one must be used).
fn allocate(s: &mut GenerationSession, faces: &[u32]) -> RoleAllocation {
    let mut dice = ScriptedFaces::new(faces.iter().copied());
    s.roll(ROLES, &mut dice).unwrap();
    assert_eq!(dice.remaining(), 0, "one die per role, never a reroll");
    roles(s)
}

/// `(allocation, rolled, given)` role IDs, in figure order.
fn summary(r: &RoleAllocation) -> Vec<(Allocation, String, String)> {
    r.roles
        .iter()
        .map(|a| {
            (
                a.allocation,
                a.initial.id.to_string(),
                a.role.id.to_string(),
            )
        })
        .collect()
}

fn row(allocation: Allocation, rolled: &str, given: &str) -> (Allocation, String, String) {
    (allocation, rolled.to_owned(), given.to_owned())
}

use Allocation::{Lead, Member, Single};

// ----- groups ------------------------------------------------------------------------

#[test]
fn local_council_has_one_lead_and_size_minus_one_members() {
    for (size, count) in [
        ("two", 2),
        ("three", 3),
        ("four", 4),
        ("five", 5),
        ("six", 6),
    ] {
        let mut s = with(content(), "council", Some(size));
        s.roll(ROLES, &mut RandomFaces::seeded(7)).unwrap();
        let r = roles(&s);
        assert_eq!(r.roles.len(), count, "{size}");
        assert_eq!(r.roles[0].allocation, Lead);
        assert_eq!(
            r.roles[0].roll.table.id.as_str(),
            "table.authorityRole.council.lead"
        );
        for m in &r.roles[1..] {
            assert_eq!(m.allocation, Member);
            assert_eq!(
                m.roll.table.id.as_str(),
                "table.authorityRole.council.member"
            );
        }
    }
}

#[test]
fn a_second_treasurer_or_civic_steward_becomes_a_councillor() {
    let mut s = with(content(), "council", Some("five"));
    // Lead 1 Council Chair; members 5 Treasurer, 5 Treasurer, 6 Civic
    // Steward, 6 Civic Steward.
    let r = allocate(&mut s, &[1, 5, 5, 6, 6]);
    assert_eq!(
        summary(&r),
        [
            row(Lead, "role.council.chair", "role.council.chair"),
            row(Member, "role.council.treasurer", "role.council.treasurer"),
            row(Member, "role.council.treasurer", "role.council.councillor"),
            row(
                Member,
                "role.council.civicSteward",
                "role.council.civicSteward"
            ),
            row(
                Member,
                "role.council.civicSteward",
                "role.council.councillor"
            ),
        ]
    );
    // Provenance: the die, the rolled role and the fallback given.
    let dup = &r.roles[2];
    assert_eq!(dup.roll.dice.as_deref(), Some("1d6"));
    assert_eq!(dup.roll.attempts.len(), 1, "no reroll");
    assert_eq!(dup.roll.attempts[0].roll.total, 5);
    assert_eq!(dup.initial.name, "Treasurer");
    assert_eq!(dup.fallback.as_ref().unwrap().name, "Councillor");
    assert_eq!(dup.role.name, "Councillor");
    assert!(r.roles[1].fallback.is_none());
}

#[test]
fn merchant_group_uses_lead_and_member_tables_with_its_fallbacks() {
    let mut s = with(content(), "guild", Some("six"));
    // Lead 4 Trade Master; members 5 Treasurer, 5, 6 Trade Speaker, 6, 3.
    let r = allocate(&mut s, &[4, 5, 5, 6, 6, 3]);
    assert_eq!(
        r.roles[0].roll.table.id.as_str(),
        "table.authorityRole.guild.lead"
    );
    assert_eq!(
        summary(&r),
        [
            row(Lead, "role.guild.tradeMaster", "role.guild.tradeMaster"),
            row(Member, "role.guild.treasurer", "role.guild.treasurer"),
            row(Member, "role.guild.treasurer", "role.guild.representative"),
            row(Member, "role.guild.tradeSpeaker", "role.guild.tradeSpeaker"),
            row(
                Member,
                "role.guild.tradeSpeaker",
                "role.guild.representative"
            ),
            row(
                Member,
                "role.guild.seniorMerchant",
                "role.guild.seniorMerchant"
            ),
        ]
    );
}

#[test]
fn military_group_uses_lead_and_member_tables_with_its_fallbacks() {
    let mut s = with(content(), "military", Some("five"));
    let r = allocate(&mut s, &[6, 5, 5, 6, 6]);
    assert_eq!(
        r.roles[0].roll.table.id.as_str(),
        "table.authorityRole.military.lead"
    );
    assert_eq!(
        summary(&r),
        [
            row(Lead, "role.military.warden", "role.military.warden"),
            row(
                Member,
                "role.military.quartermaster",
                "role.military.quartermaster"
            ),
            row(
                Member,
                "role.military.quartermaster",
                "role.military.seniorOfficer"
            ),
            row(Member, "role.military.adjutant", "role.military.adjutant"),
            row(
                Member,
                "role.military.adjutant",
                "role.military.seniorOfficer"
            ),
        ]
    );
}

#[test]
fn elders_repeat_freely_with_one_lead() {
    let mut s = with(content(), "elders", Some("six"));
    let r = allocate(&mut s, &[5, 1, 1, 2, 6, 6]);
    assert_eq!(
        summary(&r),
        [
            row(
                Lead,
                "role.elders.communitySpeaker",
                "role.elders.communitySpeaker"
            ),
            row(Member, "role.elders.elder", "role.elders.elder"),
            row(Member, "role.elders.elder", "role.elders.elder"),
            row(Member, "role.elders.elder", "role.elders.elder"),
            row(Member, "role.elders.seniorElder", "role.elders.seniorElder"),
            row(Member, "role.elders.seniorElder", "role.elders.seniorElder"),
        ]
    );
    assert!(r.roles.iter().all(|a| a.fallback.is_none()));
}

#[test]
fn popular_assembly_has_one_lead_and_delegates() {
    let mut s = with(content(), "assembly", Some("four"));
    let r = allocate(&mut s, &[3, 1, 4, 2]);
    assert_eq!(r.roles[0].roll.dice.as_deref(), Some("1d4"));
    assert_eq!(
        summary(&r),
        [
            row(Lead, "role.assembly.convenor", "role.assembly.convenor"),
            row(Member, "role.assembly.delegate", "role.assembly.delegate"),
            row(
                Member,
                "role.assembly.citizenRepresentative",
                "role.assembly.citizenRepresentative"
            ),
            row(Member, "role.assembly.delegate", "role.assembly.delegate"),
        ]
    );
}

// ----- single representatives -----------------------------------------------------------

#[test]
fn single_authorities_get_exactly_one_role_from_their_own_table() {
    for (authority, table, face, role) in [
        ("noble", "noble.single", 3, "role.noble.baron"),
        ("appointed", "appointed.single", 5, "role.appointed.prefect"),
        ("religious", "religious.single", 4, "role.religious.prior"),
        ("guild", "guild.single", 6, "role.guild.representative"),
        ("military", "military.single", 3, "role.military.captain"),
    ] {
        let mut s = with(content(), authority, None);
        let r = allocate(&mut s, &[face]);
        assert_eq!(summary(&r), [row(Single, role, role)], "{authority}");
        assert_eq!(
            r.roles[0].roll.table.id.as_str(),
            format!("table.authorityRole.{table}")
        );
    }
}

#[test]
fn every_authority_and_size_gets_exactly_one_role_per_figure() {
    for authority in ["council", "elders", "assembly", "guild", "military"] {
        for (size, count) in [
            ("two", 2),
            ("three", 3),
            ("four", 4),
            ("five", 5),
            ("six", 6),
        ] {
            let mut s = with(content(), authority, Some(size));
            s.roll(ROLES, &mut RandomFaces::seeded(count as u64))
                .unwrap();
            s.roll(FIGURES, &mut RandomFaces::seeded(8)).unwrap();
            let r = roles(&s);
            assert_eq!(r.roles.len(), count, "{authority} {size}");
            assert_eq!(r.roles.iter().filter(|a| a.allocation == Lead).count(), 1);
            let figures = s
                .view(FIGURES, ViewOptions::default())
                .unwrap()
                .figures
                .unwrap()
                .record
                .unwrap();
            assert_eq!(figures.figures.len(), count);
            for (figure, role) in figures.figures.iter().zip(&r.roles) {
                assert_eq!(figure.role.as_ref().unwrap().role, role.role);
            }
        }
    }
    for authority in ["noble", "appointed", "religious", "guild", "military"] {
        let mut s = with(content(), authority, None);
        s.roll(ROLES, &mut RandomFaces::seeded(9)).unwrap();
        assert_eq!(roles(&s).roles.len(), 1, "{authority}");
    }
}

// ----- the stage -------------------------------------------------------------------------

#[test]
fn roles_are_allocated_before_the_figures() {
    let mut s = with(content(), "council", Some("three"));
    // Figures wait for the roles.
    let err = s.roll(FIGURES, &mut RandomFaces::seeded(1)).unwrap_err();
    assert!(matches!(err, GenerationError::RolesNotAllocated));
    s.roll(ROLES, &mut RandomFaces::seeded(2)).unwrap();
    s.roll(FIGURES, &mut RandomFaces::seeded(3)).unwrap();
    assert!(s.is_complete());
}

#[test]
fn incremental_allocates_roles_after_authority_size() {
    let mut s = GenerationSession::new(
        Arc::new(content()),
        GenerationOptions::default(),
        GenerationMode::Incremental,
    )
    .unwrap();
    s.choose(0, "kurovia".into(), &mut RandomFaces::seeded(1))
        .unwrap();
    while s.cursor() < AUTHORITY {
        if s.view(s.cursor(), ViewOptions::default()).unwrap().status == StageStatus::Pending {
            s.roll(s.cursor(), &mut RandomFaces::seeded(2)).unwrap();
        }
        s.advance().unwrap();
    }
    s.choose(AUTHORITY, "noble".into(), &mut RandomFaces::seeded(3))
        .unwrap();
    // Representation and Authority Size are passed over: Roles is next.
    assert_eq!(s.advance().unwrap(), ROLES);
    let v = s.view(ROLES, ViewOptions::default()).unwrap();
    assert!(v.current && v.can_roll && v.sub_stage);
    let slots = v.roles.unwrap().slots;
    assert_eq!(slots.len(), 1);
    assert_eq!(slots[0].allocation, Single);
    assert_eq!(
        slots[0].table.id.as_str(),
        "table.authorityRole.noble.single"
    );
    s.roll(ROLES, &mut ScriptedFaces::new([4])).unwrap();
    assert_eq!(roles(&s).roles[0].role.name, "Count / Countess");
    assert_eq!(s.advance().unwrap(), FIGURES);
}

#[test]
fn only_the_active_authoritys_role_tables_are_offered() {
    let mut s = with(content(), "military", Some("three"));
    let slots = s
        .view(ROLES, ViewOptions::default())
        .unwrap()
        .roles
        .unwrap()
        .slots;
    let tables: Vec<_> = slots
        .iter()
        .map(|sl| (sl.allocation, sl.table.id.as_str()))
        .collect();
    assert_eq!(
        tables,
        [
            (Lead, "table.authorityRole.military.lead"),
            (Member, "table.authorityRole.military.member"),
            (Member, "table.authorityRole.military.member"),
        ]
    );
    // Single military: only its single table.
    s.choose(REPRESENTATION, "single".into(), &mut RandomFaces::seeded(1))
        .unwrap();
    let slots = s
        .view(ROLES, ViewOptions::default())
        .unwrap()
        .roles
        .unwrap()
        .slots;
    assert_eq!(slots.len(), 1);
    assert_eq!(
        slots[0].table.id.as_str(),
        "table.authorityRole.military.single"
    );
}

#[test]
fn a_role_can_be_rolled_or_chosen_and_unique_roles_stay_unique() {
    let mut s = with(content(), "council", Some("four"));
    allocate(&mut s, &[1, 5, 1, 1]); // Chair, Treasurer, Councillor, Councillor
    // Choosing Treasurer for another member gives its fallback.
    s.choose_role(
        ROLES,
        3,
        "role.council.treasurer".into(),
        &mut RandomFaces::seeded(1),
    )
    .unwrap();
    let r = roles(&s);
    assert_eq!(r.roles[3].initial.id.as_str(), "role.council.treasurer");
    assert_eq!(r.roles[3].role.id.as_str(), "role.council.councillor");
    assert!(r.roles[3].roll.selected);
    // Once the Treasurer becomes a Councillor, the role is free again.
    s.choose_role(
        ROLES,
        1,
        "role.council.councillor".into(),
        &mut RandomFaces::seeded(1),
    )
    .unwrap();
    s.roll_role(ROLES, 2, &mut ScriptedFaces::new([5])).unwrap();
    assert_eq!(
        roles(&s).roles[2].role.id.as_str(),
        "role.council.treasurer"
    );
    // The lead is chosen from the lead table only.
    assert!(
        s.choose_role(
            ROLES,
            0,
            "role.council.treasurer".into(),
            &mut RandomFaces::seeded(1)
        )
        .is_err()
    );
    s.choose_role(
        ROLES,
        0,
        "role.council.civicSpeaker".into(),
        &mut RandomFaces::seeded(1),
    )
    .unwrap();
    assert!(s.roll_role(ROLES, 9, &mut RandomFaces::seeded(1)).is_err());
}

#[test]
fn changing_a_role_retitles_the_figure_without_regenerating_it() {
    let mut s = with(content(), "council", Some("two"));
    allocate(&mut s, &[1, 1]);
    s.roll(FIGURES, &mut RandomFaces::seeded(4)).unwrap();
    let figure = |s: &GenerationSession| {
        s.view(FIGURES, ViewOptions::default())
            .unwrap()
            .figures
            .unwrap()
            .record
            .unwrap()
            .figures[1]
            .clone()
    };
    let before = figure(&s);
    let change = s
        .choose_role(
            ROLES,
            1,
            "role.council.treasurer".into(),
            &mut RandomFaces::seeded(1),
        )
        .unwrap();
    assert!(change.invalidated.is_empty());
    let after = figure(&s);
    assert_eq!(after.role.as_ref().unwrap().label, "Treasurer");
    assert_eq!((after.id, after.name), (before.id, before.name));
}

#[test]
fn changing_the_authority_or_representation_invalidates_the_roles() {
    let mut s = with(content(), "guild", None);
    allocate(&mut s, &[1]);
    let change = s
        .choose(REPRESENTATION, "group".into(), &mut RandomFaces::seeded(1))
        .unwrap();
    assert!(change.invalidated.contains(&ROLES));
    // Authority Size is pending (a group needs one), so the roles wait.
    s.choose(AUTHORITY_SIZE, "two".into(), &mut RandomFaces::seeded(1))
        .unwrap();
    assert_eq!(
        s.view(ROLES, ViewOptions::default()).unwrap().status,
        StageStatus::Pending
    );
    allocate(&mut s, &[1, 1]);
    let change = s
        .choose(AUTHORITY, "noble".into(), &mut RandomFaces::seeded(1))
        .unwrap();
    assert!(change.invalidated.contains(&ROLES));
    let v = s.view(ROLES, ViewOptions::default()).unwrap();
    assert_eq!(v.status, StageStatus::Pending);
    assert_eq!(
        v.previous.unwrap().name,
        "Guildmaster, Guild Representative"
    );
}

// ----- titles, tags and IDs -----------------------------------------------------------------

#[test]
fn gendered_titles_share_one_stable_role() {
    let mut s = with(content(), "noble", None);
    allocate(&mut s, &[1]); // Lord / Lady
    let title = |s: &mut GenerationSession, gender: &str| {
        s.choose_figure_field(
            FIGURES,
            0,
            "gender",
            gender.into(),
            &mut RandomFaces::seeded(1),
        )
        .unwrap();
        let role = s
            .view(FIGURES, ViewOptions::default())
            .unwrap()
            .figures
            .unwrap()
            .record
            .unwrap()
            .figures[0]
            .role
            .clone()
            .unwrap();
        (role.role.id.to_string(), role.label)
    };
    s.roll(FIGURES, &mut RandomFaces::seeded(5)).unwrap();
    assert_eq!(
        title(&mut s, "man"),
        ("role.noble.lord".into(), "Lord".into())
    );
    assert_eq!(
        title(&mut s, "woman"),
        ("role.noble.lord".into(), "Lady".into())
    );
    assert_eq!(
        title(&mut s, "nonBinary"),
        ("role.noble.lord".into(), "Lord / Lady".into())
    );
}

#[test]
fn roles_never_become_settlement_tags() {
    let c = content();
    for t in c
        .tables
        .iter()
        .filter(|t| t.id.as_str().starts_with("table.authorityRole."))
    {
        assert!(t.results.iter().all(|r| r.tags.is_empty()), "{}", t.id);
    }
    let mut s = with(content(), "council", Some("three"));
    s.roll(ROLES, &mut RandomFaces::seeded(1)).unwrap();
    s.roll(FIGURES, &mut RandomFaces::seeded(2)).unwrap();
    assert!(
        !s.active_tags()
            .iter()
            .any(|t| t.id.as_str().starts_with("role."))
    );
}

#[test]
fn a_renamed_role_keeps_its_stable_id() {
    let mut c = content();
    c.tables
        .iter_mut()
        .find(|t| t.id.as_str() == "table.authorityRole.council.member")
        .unwrap()
        .results
        .iter_mut()
        .find(|r| r.id.as_str() == "role.council.treasurer")
        .unwrap()
        .name = "Purse-Keeper".into();
    let mut s = with(c, "council", Some("three"));
    let r = allocate(&mut s, &[1, 5, 5]);
    assert_eq!(r.roles[1].role.id.as_str(), "role.council.treasurer");
    assert_eq!(r.roles[1].role.name, "Purse-Keeper");
    // Uniqueness follows the ID, not the name.
    assert_eq!(r.roles[2].role.id.as_str(), "role.council.councillor");
}

// ----- snapshots -------------------------------------------------------------------------------

#[test]
fn saved_roles_keep_their_labels_and_rolls() {
    let mut s = with(content(), "council", Some("three"));
    allocate(&mut s, &[1, 5, 5]);
    s.roll(FIGURES, &mut RandomFaces::seeded(6)).unwrap();
    let snapshot = s.to_snapshot();
    let saved = snapshot.authority_roles().unwrap().clone();
    assert_eq!(saved.roles[2].initial.name, "Treasurer");
    assert_eq!(saved.roles[2].role.name, "Councillor");
    assert_eq!(saved.app_version, settlement_core::VERSION);
    let json = serde_json::to_string(&snapshot).unwrap();
    let back: SettlementSnapshot = serde_json::from_str(&json).unwrap();
    assert_eq!(back, snapshot);

    // Later edits rename every role and make Councillor unique-less.
    let mut changed = content();
    for t in changed
        .tables
        .iter_mut()
        .filter(|t| t.id.as_str().starts_with("table.authorityRole."))
    {
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
    let figures = snapshot.authority_figures().unwrap();
    assert_eq!(
        figures.figures[2].role.as_ref().unwrap().label,
        "Councillor"
    );
}

#[test]
fn settlements_saved_before_roles_existed_load_without_allocating_any() {
    let mut s = with(content(), "council", Some("two"));
    allocate(&mut s, &[1, 1]);
    s.roll(FIGURES, &mut RandomFaces::seeded(7)).unwrap();
    let mut old = s.to_snapshot();
    old.stages
        .retain(|st| st.stage_table.as_str() != "stage.authorityRoles");
    let reopened = GenerationSession::from_snapshot(
        Arc::new(content()),
        GenerationOptions::default(),
        GenerationMode::OneTable,
        &old,
    )
    .unwrap();
    let v = reopened.view(ROLES, ViewOptions::default()).unwrap();
    assert_eq!(v.skipped, Some(SkipReason::NotRecorded));
    assert!(reopened.is_complete(), "nothing is allocated on load");
}

// ----- content and validation ------------------------------------------------------------------

#[test]
fn every_authority_has_the_role_tables_its_representation_needs() {
    let c = content();
    assert!(!validate(&c).has_errors());
    for (authority, single, group) in [
        ("council", false, true),
        ("noble", true, false),
        ("appointed", true, false),
        ("guild", true, true),
        ("religious", true, false),
        ("military", true, true),
        ("elders", false, true),
        ("assembly", false, true),
    ] {
        let tags = [format!("authority.{authority}").as_str().into()];
        let scheme = settlement_core::roles::scheme_for(&c, &tags).unwrap();
        assert_eq!(scheme.single.is_some(), single, "{authority}");
        assert_eq!(
            scheme.lead.is_some() && scheme.member.is_some(),
            group,
            "{authority}"
        );
        assert_eq!(
            scheme.lead.is_some(),
            scheme.member.is_some(),
            "{authority}"
        );
    }
}

#[test]
fn role_tables_match_the_approved_lists() {
    let c = content();
    let t = |id: &str| {
        c.table(&format!("table.authorityRole.{id}").as_str().into())
            .unwrap()
    };
    let rows = |id: &str| -> Vec<(i32, i32, String, bool)> {
        t(id)
            .results
            .iter()
            .map(|r| {
                let range = r.range.unwrap();
                (range.min, range.max, r.name.clone(), r.unique)
            })
            .collect()
    };
    let r = |a: i32, b: i32, n: &str, u: bool| (a, b, n.to_owned(), u);
    assert_eq!(
        rows("council.member"),
        [
            r(1, 3, "Councillor", false),
            r(4, 4, "Community Representative", false),
            r(5, 5, "Treasurer", true),
            r(6, 6, "Civic Steward", true),
        ]
    );
    assert_eq!(
        rows("assembly.lead"),
        [
            r(1, 2, "Speaker", false),
            r(3, 3, "Convenor", false),
            r(4, 4, "Presiding Delegate", false),
        ]
    );
    // Every role table covers its die exactly.
    for table in c
        .tables
        .iter()
        .filter(|t| t.id.as_str().starts_with("table.authorityRole."))
    {
        let faces = match table.dice.as_deref() {
            Some("1d6") => 6,
            Some("1d4") => 4,
            other => panic!("{}: {other:?}", table.id),
        };
        for face in 1..=faces {
            let claims = table
                .results
                .iter()
                .filter(|r| r.range.unwrap().contains(face))
                .count();
            assert_eq!(claims, 1, "{}: face {face}", table.id);
        }
    }
}

#[test]
fn role_metadata_and_schemes_are_validated() {
    let fixture = |member: serde_json::Value, scheme: serde_json::Value| -> ContentSet {
        serde_json::from_value(json!({
            "tables": [
                { "id": "t.lead", "name": "Lead", "dice": "1d4",
                  "results": [{ "id": "boss", "name": "Boss", "range": [1, 4] }] },
                { "id": "t.member", "name": "Member", "dice": "1d4", "results": member }
            ],
            "tags": [{ "id": "g.x", "name": "X" }],
            "role_schemes": [scheme]
        }))
        .unwrap()
    };
    let scheme = json!({ "id": "roles.x", "name": "X", "when": { "has_tag": "g.x" },
                         "lead": "t.lead", "member": "t.member" });
    let ok = json!([
        { "id": "a", "name": "A", "range": [1, 3] },
        { "id": "b", "name": "B", "range": 4, "unique": true, "fallback": "a",
          "label_variants": [{ "tag": "g.x", "name": "Bee" }] }
    ]);
    assert!(
        validate(&fixture(ok.clone(), scheme.clone()))
            .issues
            .is_empty()
    );

    let has = |c: ContentSet, kind: IssueKind| {
        let report = validate(&c);
        assert!(
            report
                .issues
                .iter()
                .any(|i| i.severity == Severity::Error && i.kind == kind),
            "{kind:?}: {report}"
        );
    };
    let mut bad = ok.clone();
    bad[1].as_object_mut().unwrap().remove("fallback");
    has(
        fixture(bad, scheme.clone()),
        IssueKind::UniqueWithoutFallback,
    );
    let mut bad = ok.clone();
    bad[0]["fallback"] = json!("b");
    has(
        fixture(bad, scheme.clone()),
        IssueKind::FallbackWithoutUnique,
    );
    let mut bad = ok.clone();
    bad[1]["fallback"] = json!("ghost");
    has(
        fixture(bad, scheme.clone()),
        IssueKind::BrokenResultReference {
            table: "t.member".into(),
            result: "ghost".into(),
        },
    );
    let mut bad = ok.clone();
    bad[0]["unique"] = json!(true);
    bad[0]["fallback"] = json!("b");
    has(
        fixture(bad, scheme.clone()),
        IssueKind::FallbackNotRepeatable,
    );
    let mut bad = ok.clone();
    bad[1]["label_variants"][0]["tag"] = json!("g.missing");
    has(
        fixture(bad, scheme.clone()),
        IssueKind::MissingTag {
            tag: "g.missing".into(),
        },
    );
    let mut lone = scheme.clone();
    lone.as_object_mut().unwrap().remove("member");
    has(fixture(ok.clone(), lone), IssueKind::RoleSchemeIncomplete);
    let mut missing = scheme;
    missing["member"] = json!("t.nowhere");
    has(
        fixture(ok, missing),
        IssueKind::MissingTable {
            table: "t.nowhere".into(),
        },
    );
}
