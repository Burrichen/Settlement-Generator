//! Data-driven content model: tables, results, variants, tags and tag effects.
//!
//! Nothing here names a specific settlement concept. Every table, tag and rule
//! is data, and behaviour comes from the generic [`Effect`] and [`Condition`]
//! vocabularies interpreted by the engine. See `docs/DATA_MODEL.md`.

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::ids::{ResultId, TableId, TagId, VariantId};

/// Version of the content file format understood by this build.
pub const CONTENT_SCHEMA_VERSION: u32 = 1;

fn yes() -> bool {
    true
}

fn is_true(b: &bool) -> bool {
    *b
}

fn default_schema_version() -> u32 {
    CONTENT_SCHEMA_VERSION
}

/// A complete, self-contained set of content (tables and tags).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContentSet {
    #[serde(default = "default_schema_version")]
    pub schema_version: u32,
    /// Tables generated for a settlement, in order. Routed tables follow the
    /// step that routed to them.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sequence: Vec<SequenceStep>,
    #[serde(default)]
    pub tables: Vec<Table>,
    #[serde(default)]
    pub tags: Vec<Tag>,
}

impl Default for ContentSet {
    fn default() -> Self {
        Self {
            schema_version: CONTENT_SCHEMA_VERSION,
            sequence: Vec::new(),
            tables: Vec::new(),
            tags: Vec::new(),
        }
    }
}

/// One stage of the generation pipeline. Written as a bare table ID, or as
/// `{ "table": id, "name": "Region" }` to give the stage its own label
/// (shown even when tags redirect the stage to another table).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "SequenceStepRepr", into = "SequenceStepRepr")]
pub struct SequenceStep {
    pub table: TableId,
    pub name: Option<String>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(untagged)]
enum SequenceStepRepr {
    Table(TableId),
    Step {
        table: TableId,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        name: Option<String>,
    },
}

impl From<SequenceStepRepr> for SequenceStep {
    fn from(repr: SequenceStepRepr) -> Self {
        match repr {
            SequenceStepRepr::Table(table) => SequenceStep { table, name: None },
            SequenceStepRepr::Step { table, name } => SequenceStep { table, name },
        }
    }
}

impl From<SequenceStep> for SequenceStepRepr {
    fn from(step: SequenceStep) -> Self {
        match step.name {
            None => SequenceStepRepr::Table(step.table),
            name => SequenceStepRepr::Step {
                table: step.table,
                name,
            },
        }
    }
}

impl ContentSet {
    /// The tables of the generation pipeline, in order.
    pub fn sequence_tables(&self) -> Vec<TableId> {
        self.sequence.iter().map(|s| s.table.clone()).collect()
    }
}

/// How a table's result is determined.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SelectionMode {
    /// Rolled with the table's dice (a result may also be chosen manually).
    #[default]
    Roll,
    /// Always chosen by the user; the table has no dice.
    Choose,
}

impl SelectionMode {
    fn is_roll(&self) -> bool {
        matches!(self, SelectionMode::Roll)
    }
}

/// Whether content is approved for use.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContentStatus {
    #[default]
    Final,
    /// Not finished (e.g. dice distribution not decided). Reported by
    /// validation, skipped by generation unless a result is chosen manually,
    /// and must be resolved before a release.
    Incomplete,
}

impl ContentStatus {
    fn is_final(&self) -> bool {
        matches!(self, ContentStatus::Final)
    }
}

impl ContentSet {
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }

    pub fn to_json_pretty(&self) -> String {
        serde_json::to_string_pretty(self).expect("content serialises")
    }

    pub fn table(&self, id: &TableId) -> Option<&Table> {
        self.tables.iter().find(|t| &t.id == id)
    }

    pub fn tag(&self, id: &TagId) -> Option<&Tag> {
        self.tags.iter().find(|t| &t.id == id)
    }
}

/// A random table rolled with physical dice.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Table {
    pub id: TableId,
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    /// Dice expression for the base layout, e.g. `1d12` or `2d6`. Absent for
    /// choose-only tables and for incomplete tables awaiting a distribution.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dice: Option<String>,
    #[serde(default, skip_serializing_if = "SelectionMode::is_roll")]
    pub selection: SelectionMode,
    #[serde(default, skip_serializing_if = "ContentStatus::is_final")]
    pub status: ContentStatus,
    /// Why the content is not final, shown wherever the status is.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub status_note: String,
    /// Inactive tables are skipped when generation reaches them.
    #[serde(default = "yes", skip_serializing_if = "is_true")]
    pub active: bool,
    /// When present, the table is only rolled if this holds; otherwise it is
    /// skipped.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub eligibility: Option<Condition>,
    /// Ordered results. Each result's `range` forms the base layout.
    #[serde(default)]
    pub results: Vec<TableResult>,
    /// Alternative layouts (dice and/or ranges), selected by tags.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub variants: Vec<TableVariant>,
    /// Conditional extra rolls made after a result is determined, which may
    /// change it (e.g. "on Local Road, roll 1d4; on 4 upgrade").
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub follow_ups: Vec<FollowUpRoll>,
}

/// A conditional roll applied after the table's result is determined.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FollowUpRoll {
    /// Unique within the table.
    pub id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    /// Only applies while this holds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub when: Option<Condition>,
    /// Only applies when the table's result is one of these.
    pub on_results: Vec<ResultId>,
    pub dice: String,
    /// Must cover every total of `dice` exactly once.
    pub outcomes: Vec<FollowUpOutcome>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FollowUpOutcome {
    pub range: FaceRange,
    /// The table result to change to; absent means keep the original.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub becomes: Option<ResultId>,
}

impl Table {
    pub fn result(&self, id: &ResultId) -> Option<&TableResult> {
        self.results.iter().find(|r| &r.id == id)
    }

    pub fn variant(&self, id: &VariantId) -> Option<&TableVariant> {
        self.variants.iter().find(|v| &v.id == id)
    }
}

/// One outcome of a table.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TableResult {
    pub id: ResultId,
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    /// Faces of the base layout that select this result. `None` means the
    /// result only appears in variant layouts.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub range: Option<FaceRange>,
    /// Tags emitted when this result is selected.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<TagId>,
    /// When present, the result is only eligible if this holds. Rolling an
    /// ineligible result means rolling again, as at the table.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub eligibility: Option<Condition>,
    /// Disabled results keep their faces but are rerolled if rolled.
    #[serde(default = "yes", skip_serializing_if = "is_true")]
    pub enabled: bool,
}

/// An alternative layout of a table, chosen by tags.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TableVariant {
    pub id: VariantId,
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    /// Selected automatically when this holds (first matching variant wins).
    /// Without it the variant is only used when an effect asks for it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub when: Option<Condition>,
    /// Replacement dice; defaults to the table's dice.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dice: Option<String>,
    /// Replacement ranges over the table's results; defaults to the base
    /// ranges. Results not listed cannot be rolled in this variant.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ranges: Option<Vec<RangeAssignment>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RangeAssignment {
    pub result: ResultId,
    pub range: FaceRange,
}

/// An inclusive range of dice totals. Serialised as `[min, max]`, or as a
/// bare number for a single face.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(from = "FaceRangeRepr", into = "FaceRangeRepr")]
pub struct FaceRange {
    pub min: i32,
    pub max: i32,
}

impl FaceRange {
    pub fn new(min: i32, max: i32) -> Self {
        Self { min, max }
    }

    pub fn single(face: i32) -> Self {
        Self::new(face, face)
    }

    pub fn contains(&self, total: i32) -> bool {
        (self.min..=self.max).contains(&total)
    }

    pub fn is_empty(&self) -> bool {
        self.min > self.max
    }

    pub fn intersection(&self, other: &FaceRange) -> Option<FaceRange> {
        let r = FaceRange::new(self.min.max(other.min), self.max.min(other.max));
        (!r.is_empty()).then_some(r)
    }
}

impl fmt::Display for FaceRange {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.min == self.max {
            write!(f, "{}", self.min)
        } else {
            write!(f, "{}-{}", self.min, self.max)
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(untagged)]
enum FaceRangeRepr {
    Single(i32),
    Pair([i32; 2]),
}

impl From<FaceRangeRepr> for FaceRange {
    fn from(r: FaceRangeRepr) -> Self {
        match r {
            FaceRangeRepr::Single(v) => FaceRange::single(v),
            FaceRangeRepr::Pair([min, max]) => FaceRange::new(min, max),
        }
    }
}

impl From<FaceRange> for FaceRangeRepr {
    fn from(r: FaceRange) -> Self {
        if r.min == r.max {
            FaceRangeRepr::Single(r.min)
        } else {
            FaceRangeRepr::Pair([r.min, r.max])
        }
    }
}

/// A boolean test over the tags active during generation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Condition {
    HasTag(TagId),
    All(Vec<Condition>),
    Any(Vec<Condition>),
    Not(Box<Condition>),
}

impl Condition {
    pub fn evaluate(&self, has_tag: &dyn Fn(&TagId) -> bool) -> bool {
        match self {
            Condition::HasTag(t) => has_tag(t),
            Condition::All(cs) => cs.iter().all(|c| c.evaluate(has_tag)),
            Condition::Any(cs) => cs.iter().any(|c| c.evaluate(has_tag)),
            Condition::Not(c) => !c.evaluate(has_tag),
        }
    }

    /// Every tag the condition refers to.
    pub fn tags(&self) -> Vec<&TagId> {
        let mut out = Vec::new();
        self.collect_tags(&mut out);
        out
    }

    fn collect_tags<'a>(&'a self, out: &mut Vec<&'a TagId>) {
        match self {
            Condition::HasTag(t) => out.push(t),
            Condition::All(cs) | Condition::Any(cs) => {
                cs.iter().for_each(|c| c.collect_tags(out));
            }
            Condition::Not(c) => c.collect_tags(out),
        }
    }
}

/// A label attached to a generated settlement, carrying effects.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Tag {
    pub id: TagId,
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub effects: Vec<EffectDef>,
    /// Disabled tags are ignored: never activated, effects never run.
    #[serde(default = "yes", skip_serializing_if = "is_true")]
    pub enabled: bool,
}

/// An effect plus its lifecycle metadata.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EffectDef {
    #[serde(flatten)]
    pub effect: Effect,
    #[serde(default, skip_serializing_if = "EffectStatus::is_active")]
    pub status: EffectStatus,
    /// Free-text design note, e.g. what a planned effect is for.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub note: String,
}

impl EffectDef {
    pub fn active(effect: Effect) -> Self {
        Self {
            effect,
            status: EffectStatus::Active,
            note: String::new(),
        }
    }

    pub fn planned(effect: Effect, note: impl Into<String>) -> Self {
        Self {
            effect,
            status: EffectStatus::Planned,
            note: note.into(),
        }
    }

    pub fn is_active(&self) -> bool {
        self.status.is_active()
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EffectStatus {
    /// Executed by the engine and fully validated.
    #[default]
    Active,
    /// Stored as design metadata only: never executed, and references to
    /// tables, variants, results or tags that do not exist yet are allowed.
    Planned,
}

impl EffectStatus {
    pub fn is_active(&self) -> bool {
        matches!(self, EffectStatus::Active)
    }
}

/// What a tag does. Effects that name a `table` act on that table whenever it
/// is rolled while the tag is active, including tables rolled later in the
/// same generation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Effect {
    /// Each time the tag is emitted, roll `table` next (optionally forcing a
    /// variant).
    RouteTo {
        table: TableId,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        variant: Option<VariantId>,
    },
    /// Roll `table` using `variant` instead of selecting one by condition.
    UseVariant { table: TableId, variant: VariantId },
    /// Narrow which results of `table` are eligible; ineligible rolls are
    /// rerolled.
    RestrictResults {
        table: TableId,
        /// When present, only these results are eligible.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        only: Option<Vec<ResultId>>,
        /// These results are never eligible.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        exclude: Vec<ResultId>,
    },
    /// Change the physical dice used for `table`: replace the expression
    /// and/or add a modifier. Totals outside the layout are read as its
    /// nearest end, as at the table.
    ModifyDice {
        table: TableId,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        dice: Option<String>,
        #[serde(default, skip_serializing_if = "is_zero")]
        modifier: i32,
    },
    /// Each time the tag is emitted, roll `dice` and record the result
    /// under `label`.
    SecondaryRoll { label: String, dice: String },
    /// Each time the tag is emitted, also emit `tag`.
    AddTag { tag: TagId },
    /// While the tag is active, whenever `table` would be rolled, roll `to`
    /// instead (e.g. a setting-specific replacement for a generic table).
    RedirectTable { table: TableId, to: TableId },
    /// An approved design intention whose exact numbers are not decided yet,
    /// e.g. "Coast increases Fishing". It records direction only and can
    /// never be executed, so it must stay `planned` until it is replaced by
    /// a concrete effect.
    Intent {
        /// Destination table, when it exists or is already named.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        table: Option<TableId>,
        /// Specific results of `table` affected, if any.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        results: Vec<ResultId>,
        /// Free-text destination when no table exists yet, e.g.
        /// "security-related rolls".
        #[serde(default, skip_serializing_if = "Option::is_none")]
        concept: Option<String>,
        change: IntentChange,
    },
}

/// Direction of an [`Effect::Intent`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IntentChange {
    Increase,
    StrongIncrease,
    Decrease,
    Guarantee,
}

fn is_zero(v: &i32) -> bool {
    *v == 0
}

impl Effect {
    /// The table this effect targets, if any.
    pub fn target_table(&self) -> Option<&TableId> {
        match self {
            Effect::RouteTo { table, .. }
            | Effect::UseVariant { table, .. }
            | Effect::RestrictResults { table, .. }
            | Effect::ModifyDice { table, .. }
            | Effect::RedirectTable { table, .. } => Some(table),
            Effect::Intent { table, .. } => table.as_ref(),
            Effect::SecondaryRoll { .. } | Effect::AddTag { .. } => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn face_range_round_trips_as_pair_or_number() {
        let r: FaceRange = serde_json::from_str("[3, 5]").unwrap();
        assert_eq!(r, FaceRange::new(3, 5));
        let r: FaceRange = serde_json::from_str("9").unwrap();
        assert_eq!(r, FaceRange::single(9));
        assert_eq!(serde_json::to_string(&FaceRange::single(9)).unwrap(), "9");
        assert_eq!(
            serde_json::to_string(&FaceRange::new(1, 4)).unwrap(),
            "[1,4]"
        );
    }

    #[test]
    fn effects_use_kind_discriminator_and_default_to_active() {
        let def: EffectDef =
            serde_json::from_str(r#"{ "kind": "add_tag", "tag": "tag.b" }"#).unwrap();
        assert_eq!(
            def,
            EffectDef::active(Effect::AddTag {
                tag: TagId::from("tag.b")
            })
        );
        let json = serde_json::to_value(&def).unwrap();
        assert_eq!(
            json,
            serde_json::json!({ "kind": "add_tag", "tag": "tag.b" })
        );
    }

    #[test]
    fn planned_effects_keep_status_and_note() {
        let def = EffectDef::planned(
            Effect::RestrictResults {
                table: "table.not-yet".into(),
                only: Some(vec!["r1".into()]),
                exclude: vec![],
            },
            "applies once the table exists",
        );
        let json = serde_json::to_string(&def).unwrap();
        let back: EffectDef = serde_json::from_str(&json).unwrap();
        assert_eq!(back, def);
        assert!(!back.is_active());
    }

    #[test]
    fn unknown_effect_kind_is_rejected() {
        assert!(serde_json::from_str::<EffectDef>(r#"{ "kind": "teleport" }"#).is_err());
    }

    #[test]
    fn conditions_evaluate_against_tags() {
        let c: Condition = serde_json::from_str(
            r#"{ "all": [ { "has_tag": "a" }, { "not": { "has_tag": "b" } } ] }"#,
        )
        .unwrap();
        let only_a = |t: &TagId| t.as_str() == "a";
        let a_and_b = |t: &TagId| matches!(t.as_str(), "a" | "b");
        assert!(c.evaluate(&only_a));
        assert!(!c.evaluate(&a_and_b));
        assert_eq!(c.tags().len(), 2);
    }

    #[test]
    fn minimal_table_json_uses_defaults() {
        let t: Table = serde_json::from_str(
            r#"{ "id": "t", "name": "T", "dice": "1d4",
                 "results": [ { "id": "r", "name": "R", "range": [1, 4] } ] }"#,
        )
        .unwrap();
        assert!(t.active);
        assert!(t.results[0].enabled);
        assert!(t.variants.is_empty());
        // Defaults are omitted again on output.
        let json = serde_json::to_value(&t).unwrap();
        assert!(json.get("active").is_none());
        assert!(json["results"][0].get("enabled").is_none());
    }
}
