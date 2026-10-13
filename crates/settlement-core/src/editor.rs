//! Support for the content editors, shared by every interface: physical-dice
//! coverage maps, where tags are used, the effect schema, and which
//! validation issues concern the item being edited. Nothing here is specific
//! to any particular table or tag.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use crate::content::{Condition, ContentSet, Effect, FaceRange, SelectionMode, Table};
use crate::dice::DiceExpr;
use crate::engine::Named;
use crate::ids::{NamingSchemeId, ResultId, TableId, TagId, VariantId};
use crate::layout::Layout;
use crate::naming::{JoinRule, JoinStyle, NamingMethod, NamingScheme, SpellingException};
use crate::validation::{Location, ValidationIssue, ValidationReport};

// ----- coverage --------------------------------------------------------------

/// Which results claim one total of the dice.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FaceClaim {
    pub face: i32,
    /// Chance of rolling this total with the dice.
    pub chance: f64,
    /// Result IDs (or follow-up outcome labels) assigned to it. Empty means
    /// uncovered; more than one means overlapping.
    pub claimed_by: Vec<String>,
}

/// A result's share of the dice in one layout.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResultChance {
    pub result: String,
    pub range: FaceRange,
    /// Probability of the range under the dice (0 for impossible faces).
    pub chance: f64,
}

/// The faces of one layout (base, a variant, or a follow-up roll).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Coverage {
    /// "Base layout", "Variant: …" or "Follow-up: …".
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub variant: Option<VariantId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub follow_up: Option<String>,
    pub dice: String,
    /// Why the dice cannot be read, if they cannot.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dice_error: Option<String>,
    /// Every possible total, in order.
    pub faces: Vec<FaceClaim>,
    pub results: Vec<ResultChance>,
    pub uncovered: Vec<FaceRange>,
    pub overlapping: Vec<FaceRange>,
    /// Ranges (with their owner) that include faces the dice cannot roll.
    pub impossible: Vec<ResultChance>,
}

impl Coverage {
    pub fn is_valid(&self) -> bool {
        self.dice_error.is_none()
            && self.uncovered.is_empty()
            && self.overlapping.is_empty()
            && self.impossible.is_empty()
    }
}

/// Coverage of every rolled layout of `table`: the base layout, each variant
/// that changes dice or ranges, and each follow-up roll.
pub fn coverage(table: &Table) -> Vec<Coverage> {
    let mut out = Vec::new();
    if table.selection == SelectionMode::Roll {
        for layout in Layout::all(table) {
            let Some(dice) = layout.dice else { continue };
            if let Some(v) = layout.variant
                && v.dice.is_none()
                && v.ranges.is_none()
            {
                continue;
            }
            let entries: Vec<(String, FaceRange)> = layout
                .entries
                .iter()
                .map(|(id, range)| (id.to_string(), *range))
                .collect();
            out.push(layout_coverage(
                layout.variant.map_or_else(
                    || "Base layout".to_owned(),
                    |v| format!("Variant: {}", v.name),
                ),
                layout.variant.map(|v| v.id.clone()),
                None,
                dice,
                &entries,
            ));
        }
    }
    for follow_up in &table.follow_ups {
        let entries: Vec<(String, FaceRange)> = follow_up
            .outcomes
            .iter()
            .map(|o| {
                let label = o
                    .becomes
                    .as_ref()
                    .map_or_else(|| "(keep)".to_owned(), ToString::to_string);
                (label, o.range)
            })
            .collect();
        out.push(layout_coverage(
            format!("Follow-up: {}", follow_up.name),
            None,
            Some(follow_up.id.clone()),
            &follow_up.dice,
            &entries,
        ));
    }
    out
}

fn layout_coverage(
    label: String,
    variant: Option<VariantId>,
    follow_up: Option<String>,
    dice: &str,
    entries: &[(String, FaceRange)],
) -> Coverage {
    let mut coverage = Coverage {
        label,
        variant,
        follow_up,
        dice: dice.to_owned(),
        dice_error: None,
        faces: Vec::new(),
        results: Vec::new(),
        uncovered: Vec::new(),
        overlapping: Vec::new(),
        impossible: Vec::new(),
    };
    let expr = match DiceExpr::parse(dice) {
        Ok(expr) => expr,
        Err(e) => {
            coverage.dice_error = Some(e.to_string());
            return coverage;
        }
    };
    coverage.dice = expr.to_string();
    let probabilities = expr.probabilities();
    let chance_of = |range: &FaceRange| -> f64 {
        probabilities
            .iter()
            .filter(|(t, _)| range.contains(*t))
            .map(|(_, p)| p)
            .sum()
    };
    let possible = FaceRange::new(expr.min_total(), expr.max_total());
    for (result, range) in entries {
        let share = ResultChance {
            result: result.clone(),
            range: *range,
            chance: chance_of(range),
        };
        if range.is_empty() || range.min < possible.min || range.max > possible.max {
            coverage.impossible.push(share.clone());
        }
        coverage.results.push(share);
    }
    for (face, chance) in &probabilities {
        let claimed_by: Vec<String> = entries
            .iter()
            .filter(|(_, r)| r.contains(*face))
            .map(|(id, _)| id.clone())
            .collect();
        coverage.faces.push(FaceClaim {
            face: *face,
            chance: *chance,
            claimed_by,
        });
    }
    coverage.uncovered = runs(&coverage.faces, |f| f.claimed_by.is_empty());
    coverage.overlapping = runs(&coverage.faces, |f| f.claimed_by.len() > 1);
    coverage
}

/// Consecutive runs of faces matching `pred`, as ranges.
fn runs(faces: &[FaceClaim], pred: impl Fn(&FaceClaim) -> bool) -> Vec<FaceRange> {
    let mut out: Vec<FaceRange> = Vec::new();
    for face in faces.iter().filter(|f| pred(f)) {
        match out.last_mut() {
            Some(last) if last.max + 1 == face.face => last.max = face.face,
            _ => out.push(FaceRange::single(face.face)),
        }
    }
    out
}

// ----- tag usage -------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UsageKind {
    /// A result emits the tag.
    EmittedBy,
    /// A condition checks the tag.
    CheckedBy,
    /// Another tag's `add_tag` effect adds it.
    AddedBy,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TagUsage {
    pub kind: UsageKind,
    pub location: Location,
    /// e.g. "emitted by table `table.age` › result `old`".
    pub description: String,
}

/// Everywhere `tag` is referenced in `content`.
pub fn tag_usages(content: &ContentSet, tag: &TagId) -> Vec<TagUsage> {
    let mut out = Vec::new();
    let mut push = |kind: UsageKind, location: Location| {
        let verb = match kind {
            UsageKind::EmittedBy => "emitted by",
            UsageKind::CheckedBy => "checked by",
            UsageKind::AddedBy => "added by",
        };
        out.push(TagUsage {
            description: format!("{verb} {location}"),
            kind,
            location,
        });
    };
    let mentions = |c: &Option<Condition>| c.as_ref().is_some_and(|c| c.tags().contains(&tag));
    for table in &content.tables {
        let loc = Location::table(&table.id);
        if mentions(&table.eligibility) {
            push(UsageKind::CheckedBy, loc.clone());
        }
        for result in &table.results {
            let rloc = loc.clone().with_result(&result.id);
            if result.tags.contains(tag) {
                push(UsageKind::EmittedBy, rloc.clone());
            }
            if mentions(&result.eligibility) {
                push(UsageKind::CheckedBy, rloc);
            }
        }
        for variant in &table.variants {
            if mentions(&variant.when) {
                push(
                    UsageKind::CheckedBy,
                    loc.clone().with_variant(Some(&variant.id)),
                );
            }
        }
        for follow_up in &table.follow_ups {
            if mentions(&follow_up.when) {
                push(
                    UsageKind::CheckedBy,
                    loc.clone().with_follow_up(&follow_up.id),
                );
            }
        }
    }
    for other in &content.tags {
        for (index, def) in other.effects.iter().enumerate() {
            if matches!(&def.effect, Effect::AddTag { tag: t } if t == tag) {
                push(
                    UsageKind::AddedBy,
                    Location::tag(&other.id).with_effect(index),
                );
            }
        }
    }
    out
}

// ----- issues ----------------------------------------------------------------

/// Issues in `preview` that concern the edited item, plus any errors the
/// edit would newly cause elsewhere.
pub fn relevant_issues(
    current: &ValidationReport,
    preview: &ValidationReport,
    table: Option<&TableId>,
    tag: Option<&TagId>,
) -> Vec<ValidationIssue> {
    let existing: HashSet<&str> = current.errors().map(|i| i.message.as_str()).collect();
    preview
        .issues
        .iter()
        .filter(|i| {
            let here = (table.is_some() && i.location.table.as_ref() == table)
                || (tag.is_some() && i.location.tag.as_ref() == tag);
            let newly_broken = i.severity == crate::validation::Severity::Error
                && !existing.contains(i.message.as_str());
            here || newly_broken
        })
        .cloned()
        .collect()
}

// ----- effect schema ---------------------------------------------------------

/// What kind of value an effect field holds, so editors can offer the right
/// input (a table picker, a tag picker, a dice box…).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum FieldKind {
    Table,
    /// A variant of the table named by field `table_field`.
    Variant {
        table_field: String,
    },
    /// One result of the table named by `table_field`.
    Result {
        table_field: String,
    },
    /// Several results of the table named by `table_field`.
    ResultList {
        table_field: String,
    },
    Tag,
    Dice,
    Text,
    Integer,
    Choice {
        options: Vec<ChoiceOption>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChoiceOption {
    pub value: String,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EffectField {
    pub name: String,
    pub label: String,
    pub kind: FieldKind,
    pub required: bool,
    pub help: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EffectKindSchema {
    /// The `kind` value in content.
    pub kind: String,
    pub label: String,
    pub description: String,
    /// False for kinds that can only be stored as planned (intents).
    pub executable: bool,
    pub fields: Vec<EffectField>,
}

fn field(name: &str, label: &str, kind: FieldKind, required: bool, help: &str) -> EffectField {
    EffectField {
        name: name.into(),
        label: label.into(),
        kind,
        required,
        help: help.into(),
    }
}

fn of_table(f: fn(String) -> FieldKind) -> FieldKind {
    f("table".into())
}

/// Every effect kind and its fields. Editors render effect forms from this,
/// so a new effect kind needs no interface-specific code.
pub fn effect_schema() -> Vec<EffectKindSchema> {
    let table = |help: &str| field("table", "Table", FieldKind::Table, true, help);
    let kind =
        |kind: &str, label: &str, description: &str, fields: Vec<EffectField>| EffectKindSchema {
            kind: kind.into(),
            label: label.into(),
            description: description.into(),
            executable: kind != "intent",
            fields,
        };
    vec![
        kind(
            "route_to",
            "Route to table",
            "Each time the tag is emitted, roll another table next.",
            vec![
                table("The table to roll."),
                field(
                    "variant",
                    "Variant",
                    of_table(|t| FieldKind::Variant { table_field: t }),
                    false,
                    "Force this variant of the table.",
                ),
            ],
        ),
        kind(
            "use_variant",
            "Use table variant",
            "While the tag is active, the table uses this variant.",
            vec![
                table("The table whose variant is used."),
                field(
                    "variant",
                    "Variant",
                    of_table(|t| FieldKind::Variant { table_field: t }),
                    true,
                    "",
                ),
            ],
        ),
        kind(
            "restrict_results",
            "Restrict results",
            "While the tag is active, only some results of the table can be rolled; others are rerolled.",
            vec![
                table("The table to restrict."),
                field(
                    "only",
                    "Only these results",
                    of_table(|t| FieldKind::ResultList { table_field: t }),
                    false,
                    "Leave empty for no limit.",
                ),
                field(
                    "exclude",
                    "Never these results",
                    of_table(|t| FieldKind::ResultList { table_field: t }),
                    false,
                    "",
                ),
            ],
        ),
        kind(
            "modify_dice",
            "Change dice",
            "While the tag is active, the table is rolled with other dice and/or a modifier.",
            vec![
                table("The table whose dice change."),
                field(
                    "dice",
                    "Replacement dice",
                    FieldKind::Dice,
                    false,
                    "e.g. 2d6",
                ),
                field(
                    "modifier",
                    "Modifier",
                    FieldKind::Integer,
                    false,
                    "Added to the total; totals past the table read as its end.",
                ),
            ],
        ),
        kind(
            "secondary_roll",
            "Secondary roll",
            "Each time the tag is emitted, roll extra dice and record the result.",
            vec![
                field(
                    "label",
                    "Label",
                    FieldKind::Text,
                    true,
                    "What the roll is for.",
                ),
                field("dice", "Dice", FieldKind::Dice, true, "e.g. 1d4"),
            ],
        ),
        kind(
            "add_tag",
            "Add tag",
            "Each time the tag is emitted, also emit another tag.",
            vec![field("tag", "Tag", FieldKind::Tag, true, "")],
        ),
        kind(
            "redirect_table",
            "Redirect table",
            "While the tag is active, rolling one table rolls another instead.",
            vec![
                table("The table that is replaced."),
                field(
                    "to",
                    "Replacement",
                    FieldKind::Table,
                    true,
                    "The table rolled instead.",
                ),
            ],
        ),
        kind(
            "intent",
            "Intent (no numbers yet)",
            "Records an approved direction without approved numbers. Never executed; must stay planned.",
            vec![
                field(
                    "table",
                    "Table",
                    FieldKind::Table,
                    false,
                    "If the destination table exists.",
                ),
                field(
                    "results",
                    "Results",
                    of_table(|t| FieldKind::ResultList { table_field: t }),
                    false,
                    "",
                ),
                field(
                    "concept",
                    "Concept",
                    FieldKind::Text,
                    false,
                    "When no table exists yet, e.g. \"security-related rolls\".",
                ),
                field(
                    "change",
                    "Change",
                    FieldKind::Choice {
                        options: [
                            ("increase", "Increase"),
                            ("strong_increase", "Strong increase"),
                            ("decrease", "Decrease"),
                            ("strong_decrease", "Strong decrease"),
                            ("guarantee", "Guarantee"),
                        ]
                        .into_iter()
                        .map(|(value, label)| ChoiceOption {
                            value: value.into(),
                            label: label.into(),
                        })
                        .collect(),
                    },
                    true,
                    "",
                ),
            ],
        ),
    ]
}

/// IDs of a table's results (for result pickers).
pub fn result_ids(table: &Table) -> Vec<ResultId> {
    table.results.iter().map(|r| r.id.clone()).collect()
}

// ----- naming ----------------------------------------------------------------

/// A naming method whose `curated_variant` join reads approved names from
/// this table's results: each result needs one approved name per variant.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApprovedNameSlots {
    pub scheme: Named<NamingSchemeId>,
    pub method: Named<ResultId>,
    /// The table whose result IDs select between the approved names.
    pub variant_table: Named<TableId>,
    /// One approved name is needed for each of these, in table order.
    pub variants: Vec<Named<ResultId>>,
}

/// The `curated_variant` methods whose approved names live in `table`.
pub fn approved_name_slots(content: &ContentSet, table: &TableId) -> Vec<ApprovedNameSlots> {
    let mut out = Vec::new();
    for scheme in &content.naming_schemes {
        for method in &scheme.methods {
            let JoinStyle::CuratedVariant {
                source_from_result_id: source,
                variant_from_result_id: variant,
                ..
            } = &method.join.style
            else {
                continue;
            };
            if method.components.get(*source) != Some(table) {
                continue;
            }
            let Some(variants) = method
                .components
                .get(*variant)
                .and_then(|t| content.table(t))
            else {
                continue;
            };
            out.push(ApprovedNameSlots {
                scheme: Named::new(&scheme.id, &scheme.name),
                method: Named::new(&method.result, &method_name(content, scheme, method)),
                variant_table: Named::new(&variants.id, &variants.name),
                variants: variants
                    .results
                    .iter()
                    .map(|r| Named::new(&r.id, &r.name))
                    .collect(),
            });
        }
    }
    out
}

/// One component of a naming method, with its results (for exception
/// pickers).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NamingComponent {
    pub table: Named<TableId>,
    pub results: Vec<Named<ResultId>>,
}

/// A naming method of a scheme whose method table is the edited table: its
/// joining rule and spelling exceptions, which an editor can change.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NamingMethodDetail {
    pub scheme: Named<NamingSchemeId>,
    pub method: Named<ResultId>,
    pub join: JoinRule,
    pub exceptions: Vec<SpellingException>,
    pub components: Vec<NamingComponent>,
    /// The user changed the join or exceptions of the bundled method.
    pub modified: bool,
    /// The method is part of the bundled content (so it can be reset).
    pub built_in: bool,
}

/// The naming methods driven by `table`, when it is a scheme's method
/// table. `modified` says which the user changed, by scheme.
pub fn naming_methods(
    content: &ContentSet,
    bundled: &ContentSet,
    modified: &std::collections::BTreeMap<NamingSchemeId, std::collections::BTreeSet<ResultId>>,
    table: &TableId,
) -> Vec<NamingMethodDetail> {
    content
        .naming_schemes
        .iter()
        .filter(|s| &s.method_table == table)
        .flat_map(|scheme| {
            scheme.methods.iter().map(move |method| NamingMethodDetail {
                scheme: Named::new(&scheme.id, &scheme.name),
                method: Named::new(&method.result, &method_name(content, scheme, method)),
                join: method.join.clone(),
                exceptions: method.exceptions.clone(),
                components: method
                    .components
                    .iter()
                    .map(|id| NamingComponent {
                        table: Named::new(
                            id,
                            &content
                                .table(id)
                                .map_or_else(|| id.to_string(), |t| t.name.clone()),
                        ),
                        results: content
                            .table(id)
                            .map(|t| {
                                t.results
                                    .iter()
                                    .map(|r| Named::new(&r.id, &r.name))
                                    .collect()
                            })
                            .unwrap_or_default(),
                    })
                    .collect(),
                modified: modified
                    .get(&scheme.id)
                    .is_some_and(|m| m.contains(&method.result)),
                built_in: bundled
                    .naming_scheme(&scheme.id)
                    .and_then(|s| s.method(&method.result))
                    .is_some(),
            })
        })
        .collect()
}

/// A method's display name: its method-table result's name.
fn method_name(content: &ContentSet, scheme: &NamingScheme, method: &NamingMethod) -> String {
    content
        .table(&scheme.method_table)
        .and_then(|t| t.result(&method.result))
        .map_or_else(|| method.result.to_string(), |r| r.name.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::{EffectDef, FollowUpOutcome, FollowUpRoll, TableResult};
    use crate::content::{IntentChange, RangeAssignment, TableVariant};

    fn table(dice: &str, ranges: &[(&str, i32, i32)]) -> Table {
        Table {
            id: "t".into(),
            name: "T".into(),
            description: String::new(),
            dice: Some(dice.into()),
            selection: Default::default(),
            status: Default::default(),
            status_note: String::new(),
            ordered: false,
            active: true,
            eligibility: None,
            results: ranges
                .iter()
                .map(|(id, a, b)| TableResult {
                    id: (*id).into(),
                    name: id.to_uppercase(),
                    description: String::new(),
                    range: Some(FaceRange::new(*a, *b)),
                    tags: vec![],
                    eligibility: None,
                    enabled: true,
                    generator: None,
                    value: None,
                    unique: false,
                    fallback: None,
                    label_variants: Vec::new(),
                    approved_names: Default::default(),
                })
                .collect(),
            variants: vec![],
            follow_ups: vec![],
        }
    }

    #[test]
    fn highlights_an_uncovered_twenty() {
        let cov = coverage(&table("1d20", &[("a", 1, 10), ("b", 11, 19)]));
        assert_eq!(cov.len(), 1);
        assert_eq!(cov[0].uncovered, vec![FaceRange::single(20)]);
        assert!(cov[0].faces[19].claimed_by.is_empty());
        assert!(!cov[0].is_valid());
        assert!((cov[0].results[0].chance - 0.5).abs() < 1e-12);
    }

    #[test]
    fn reports_overlaps_impossible_ranges_and_bad_dice() {
        let cov = coverage(&table("1d6", &[("a", 1, 4), ("b", 4, 7)]));
        assert_eq!(cov[0].overlapping, vec![FaceRange::single(4)]);
        assert_eq!(cov[0].impossible[0].result, "b");
        assert_eq!(cov[0].faces[3].claimed_by, ["a", "b"]);

        let cov = coverage(&table("1d7", &[("a", 1, 7)]));
        assert!(cov[0].dice_error.is_some());
        assert!(cov[0].faces.is_empty());
    }

    #[test]
    fn two_d6_shares_follow_the_bell_curve() {
        let cov = coverage(&table(
            "2d6",
            &[("low", 2, 6), ("seven", 7, 7), ("high", 8, 12)],
        ));
        assert!(cov[0].is_valid());
        let seven = &cov[0].results[1];
        assert!((seven.chance - 6.0 / 36.0).abs() < 1e-12);
        assert_eq!(cov[0].faces.first().unwrap().face, 2);
    }

    #[test]
    fn covers_variants_and_follow_ups() {
        let mut t = table("1d4", &[("a", 1, 2), ("b", 3, 4)]);
        t.variants.push(TableVariant {
            id: "v".into(),
            name: "V".into(),
            description: String::new(),
            when: None,
            dice: Some("1d6".into()),
            ranges: Some(vec![RangeAssignment {
                result: "a".into(),
                range: FaceRange::new(1, 5),
            }]),
        });
        t.variants.push(TableVariant {
            id: "same".into(),
            name: "Same".into(),
            description: String::new(),
            when: None,
            dice: None,
            ranges: None,
        });
        t.follow_ups.push(FollowUpRoll {
            id: "f".into(),
            name: "F".into(),
            description: String::new(),
            when: None,
            on_results: vec!["a".into()],
            dice: "1d4".into(),
            outcomes: vec![
                FollowUpOutcome {
                    range: FaceRange::new(1, 3),
                    becomes: None,
                },
                FollowUpOutcome {
                    range: FaceRange::single(4),
                    becomes: Some("b".into()),
                },
            ],
        });
        let cov = coverage(&t);
        let labels: Vec<_> = cov.iter().map(|c| c.label.as_str()).collect();
        assert_eq!(labels, ["Base layout", "Variant: V", "Follow-up: F"]);
        assert_eq!(cov[1].uncovered, vec![FaceRange::single(6)]);
        assert!(cov[2].is_valid());
    }

    #[test]
    fn schema_matches_every_effect_kind() {
        let samples = [
            Effect::RouteTo {
                table: "t".into(),
                variant: Some("v".into()),
            },
            Effect::UseVariant {
                table: "t".into(),
                variant: "v".into(),
            },
            Effect::RestrictResults {
                table: "t".into(),
                only: Some(vec!["r".into()]),
                exclude: vec!["s".into()],
            },
            Effect::ModifyDice {
                table: "t".into(),
                dice: Some("1d4".into()),
                modifier: 1,
            },
            Effect::SecondaryRoll {
                label: "L".into(),
                dice: "1d4".into(),
            },
            Effect::AddTag { tag: "g".into() },
            Effect::RedirectTable {
                table: "t".into(),
                to: "u".into(),
            },
            Effect::Intent {
                table: Some("t".into()),
                results: vec!["r".into()],
                concept: Some("c".into()),
                change: IntentChange::Increase,
            },
        ];
        let schema = effect_schema();
        assert_eq!(schema.len(), samples.len(), "every kind has a schema entry");
        for effect in samples {
            let json = serde_json::to_value(EffectDef::active(effect)).unwrap();
            let kind = json["kind"].as_str().unwrap();
            let entry = schema
                .iter()
                .find(|s| s.kind == kind)
                .unwrap_or_else(|| panic!("no schema for {kind}"));
            // Every serialised field is described, and vice versa.
            let keys: HashSet<&str> = json
                .as_object()
                .unwrap()
                .keys()
                .map(String::as_str)
                .filter(|k| *k != "kind")
                .collect();
            let described: HashSet<&str> = entry.fields.iter().map(|f| f.name.as_str()).collect();
            assert_eq!(keys, described, "{kind}");
        }
    }

    #[test]
    fn intent_change_choices_match_every_intent_change() {
        use IntentChange::*;
        let expected: Vec<String> = [
            Increase,
            StrongIncrease,
            Decrease,
            StrongDecrease,
            Guarantee,
        ]
        .iter()
        .map(|c| {
            serde_json::to_value(c)
                .unwrap()
                .as_str()
                .unwrap()
                .to_owned()
        })
        .collect();
        let schema = effect_schema();
        let intent = schema.iter().find(|s| s.kind == "intent").unwrap();
        let change = intent.fields.iter().find(|f| f.name == "change").unwrap();
        let FieldKind::Choice { options } = &change.kind else {
            panic!("change is a choice");
        };
        let values: Vec<_> = options.iter().map(|o| o.value.clone()).collect();
        assert_eq!(values, expected);
    }

    #[test]
    fn finds_every_use_of_a_tag() {
        let content: ContentSet = serde_json::from_value(serde_json::json!({
            "tables": [{ "id": "t", "name": "T", "dice": "1d4",
                "eligibility": { "has_tag": "g" },
                "results": [{ "id": "r", "name": "R", "range": [1, 4], "tags": ["g"],
                              "eligibility": { "not": { "has_tag": "g" } } }] }],
            "tags": [{ "id": "g", "name": "G" },
                     { "id": "h", "name": "H", "effects": [{ "kind": "add_tag", "tag": "g" }] }]
        }))
        .unwrap();
        let usages = tag_usages(&content, &"g".into());
        let kinds: Vec<_> = usages.iter().map(|u| u.kind.clone()).collect();
        assert_eq!(
            kinds,
            [
                UsageKind::CheckedBy,
                UsageKind::EmittedBy,
                UsageKind::CheckedBy,
                UsageKind::AddedBy
            ]
        );
        assert_eq!(usages[1].description, "emitted by table `t` › result `r`");
    }
}
