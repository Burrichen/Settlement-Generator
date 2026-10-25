//! Data-driven content model: tables, results, variants, tags and tag effects.
//!
//! Nothing here names a specific settlement concept. Every table, tag and rule
//! is data, and behaviour comes from the generic [`Effect`] and [`Condition`]
//! vocabularies interpreted by the engine. See `docs/DATA_MODEL.md`.

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::framework::{Flow, ObjectType, Scale, SettingDef, SettingValue, SharedResult};
use crate::ids::{
    FlowId, GeneratorId, NamingSchemeId, ResultId, ScaleId, SettingId, SharedResultId, StepId,
    TableId, TagId, VariantId,
};
use crate::locations::LocationScheme;
use crate::naming::NamingScheme;
use crate::roles::RoleScheme;

/// Version of the content file format understood by this build.
pub const CONTENT_SCHEMA_VERSION: u32 = 1;

fn yes() -> bool {
    true
}

fn is_true(b: &bool) -> bool {
    *b
}

fn is_false(b: &bool) -> bool {
    !*b
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
    /// Ways of naming settlements, chosen by tags (see `naming.rs`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub naming_schemes: Vec<NamingScheme>,
    /// The role tables each Authority uses, chosen by tags (see `roles.rs`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub role_schemes: Vec<RoleScheme>,
    /// The Notable Location tables each Size uses, chosen by tags (see
    /// `locations.rs`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub location_schemes: Vec<LocationScheme>,
    /// Canonical results that tables can list by ID (see
    /// [`TableResult::shared`]), so one outcome is defined once.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub shared_results: Vec<SharedResult>,
    /// Ordered scales that Shift and Cap modifiers move along.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub scales: Vec<Scale>,
    /// Generation settings that conditions can test.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub settings: Vec<SettingDef>,
    /// Kinds of object that flows create (e.g. a person).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub objects: Vec<ObjectType>,
    /// Ordered generation steps. The [`start`](Self::start) flow is the
    /// pipeline; other flows run as subflows.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub flows: Vec<Flow>,
    /// The flow a new generation runs. Without it, the legacy `sequence` is
    /// the pipeline.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start: Option<FlowId>,
}

impl Default for ContentSet {
    fn default() -> Self {
        Self {
            schema_version: CONTENT_SCHEMA_VERSION,
            sequence: Vec::new(),
            tables: Vec::new(),
            tags: Vec::new(),
            naming_schemes: Vec::new(),
            role_schemes: Vec::new(),
            location_schemes: Vec::new(),
            shared_results: Vec::new(),
            scales: Vec::new(),
            settings: Vec::new(),
            objects: Vec::new(),
            flows: Vec::new(),
            start: None,
        }
    }
}

/// One stage of the generation pipeline. Written as a bare table ID, as
/// `{ "table": id, "name": "Region" }` to give the stage its own label
/// (shown even when tags redirect the stage to another table), or as
/// `{ "naming": true, "name": "Name" }` for the settlement-name stage.
/// A table step may add `"sub_stage": true`: see [`SequenceStep::sub_stage`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "SequenceStepRepr", into = "SequenceStepRepr")]
pub struct SequenceStep {
    pub target: StepTarget,
    pub name: Option<String>,
    /// The stage elaborates the stage before it (e.g. Authority Size after
    /// Authority). It is usually conditional, through its table's
    /// eligibility: interfaces show it with the stage it belongs to, and
    /// leave it out where it does not apply.
    pub sub_stage: bool,
}

/// What a pipeline stage resolves.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StepTarget {
    /// Roll or choose a result of this table.
    Table(TableId),
    /// Name the settlement with the naming scheme that applies, if any.
    Naming,
    /// Generate the Authority Figures of the Authority resolved earlier
    /// (see [`crate::figures`]). Written `{ "authority_figures": true }`.
    AuthorityFigures,
    /// Allocate a Role / Title to each Authority Figure before the figures
    /// are generated (see [`crate::roles`]). Written
    /// `{ "authority_roles": true }`.
    AuthorityRoles,
    /// Generate the settlement's Notable Locations (see
    /// [`crate::locations`]). Written `{ "notable_locations": true }`.
    NotableLocations,
}

impl SequenceStep {
    /// The stage's table, for table stages.
    pub fn table(&self) -> Option<&TableId> {
        match &self.target {
            StepTarget::Table(t) => Some(t),
            StepTarget::Naming
            | StepTarget::AuthorityFigures
            | StepTarget::AuthorityRoles
            | StepTarget::NotableLocations => None,
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(untagged)]
enum SequenceStepRepr {
    Table(TableId),
    Step {
        table: TableId,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        name: Option<String>,
        #[serde(default, skip_serializing_if = "is_false")]
        sub_stage: bool,
    },
    Naming {
        naming: NamingMarker,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        name: Option<String>,
    },
    AuthorityFigures {
        authority_figures: NamingMarker,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        name: Option<String>,
    },
    NotableLocations {
        notable_locations: NamingMarker,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        name: Option<String>,
    },
    AuthorityRoles {
        authority_roles: NamingMarker,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        name: Option<String>,
        #[serde(default, skip_serializing_if = "is_false")]
        sub_stage: bool,
    },
}

/// The literal `true` in `{ "naming": true }` and
/// `{ "authority_figures": true }`.
#[derive(Clone, Copy, Serialize, Deserialize)]
#[serde(try_from = "bool", into = "bool")]
struct NamingMarker;

impl TryFrom<bool> for NamingMarker {
    type Error = &'static str;
    fn try_from(value: bool) -> Result<Self, Self::Error> {
        if value {
            Ok(NamingMarker)
        } else {
            Err("the stage marker must be true")
        }
    }
}

impl From<NamingMarker> for bool {
    fn from(_: NamingMarker) -> bool {
        true
    }
}

impl From<SequenceStepRepr> for SequenceStep {
    fn from(repr: SequenceStepRepr) -> Self {
        match repr {
            SequenceStepRepr::Table(table) => SequenceStep {
                target: StepTarget::Table(table),
                name: None,
                sub_stage: false,
            },
            SequenceStepRepr::Step {
                table,
                name,
                sub_stage,
            } => SequenceStep {
                target: StepTarget::Table(table),
                name,
                sub_stage,
            },
            SequenceStepRepr::Naming { name, .. } => SequenceStep {
                target: StepTarget::Naming,
                name,
                sub_stage: false,
            },
            SequenceStepRepr::AuthorityFigures { name, .. } => SequenceStep {
                target: StepTarget::AuthorityFigures,
                name,
                sub_stage: false,
            },
            SequenceStepRepr::NotableLocations { name, .. } => SequenceStep {
                target: StepTarget::NotableLocations,
                name,
                sub_stage: false,
            },
            SequenceStepRepr::AuthorityRoles {
                name, sub_stage, ..
            } => SequenceStep {
                target: StepTarget::AuthorityRoles,
                name,
                sub_stage,
            },
        }
    }
}

impl From<SequenceStep> for SequenceStepRepr {
    fn from(step: SequenceStep) -> Self {
        match (step.target, step.name) {
            (StepTarget::Table(table), None) if !step.sub_stage => SequenceStepRepr::Table(table),
            (StepTarget::Table(table), name) => SequenceStepRepr::Step {
                table,
                name,
                sub_stage: step.sub_stage,
            },
            (StepTarget::Naming, name) => SequenceStepRepr::Naming {
                naming: NamingMarker,
                name,
            },
            (StepTarget::AuthorityFigures, name) => SequenceStepRepr::AuthorityFigures {
                authority_figures: NamingMarker,
                name,
            },
            (StepTarget::NotableLocations, name) => SequenceStepRepr::NotableLocations {
                notable_locations: NamingMarker,
                name,
            },
            (StepTarget::AuthorityRoles, name) => SequenceStepRepr::AuthorityRoles {
                authority_roles: NamingMarker,
                name,
                sub_stage: step.sub_stage,
            },
        }
    }
}

impl ContentSet {
    /// The tables of the generation pipeline, in order. Naming stages have
    /// no table of their own and are left out.
    pub fn sequence_tables(&self) -> Vec<TableId> {
        self.pipeline()
            .into_iter()
            .filter(|s| s.runs())
            .filter_map(|s| match s.action {
                crate::framework::StepAction::Roll { table, .. } => Some(table),
                _ => None,
            })
            .collect()
    }

    pub fn naming_scheme(&self, id: &NamingSchemeId) -> Option<&NamingScheme> {
        self.naming_schemes.iter().find(|s| &s.id == id)
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

    pub fn shared_result(&self, id: &SharedResultId) -> Option<&SharedResult> {
        self.shared_results.iter().find(|r| &r.id == id)
    }

    pub fn scale(&self, id: &ScaleId) -> Option<&Scale> {
        self.scales.iter().find(|s| &s.id == id)
    }

    /// The setting `id` names (an alias names the setting it stands for).
    pub fn setting(&self, id: &SettingId) -> Option<&SettingDef> {
        let id = crate::framework::canonical_setting(id);
        self.settings.iter().find(|s| s.id == id)
    }

    pub fn flow(&self, id: &FlowId) -> Option<&Flow> {
        self.flows.iter().find(|f| &f.id == id)
    }

    pub fn object_type(&self, id: &crate::ids::ObjectTypeId) -> Option<&ObjectType> {
        self.objects.iter().find(|o| &o.id == id)
    }

    /// The order of `table`'s results on its scale, lowest first: the
    /// table's own `scale`, else (for an `ordered` table) its results in
    /// listed order. `None` for a table without a scale.
    pub fn scale_order<'t>(&self, table: &'t Table) -> Option<Vec<&'t ResultId>> {
        match &table.scale {
            Some(id) => {
                let scale = self.scale(id)?;
                Some(
                    scale
                        .steps
                        .iter()
                        .filter_map(|step| {
                            table
                                .results
                                .iter()
                                .find(|r| {
                                    &r.id == step
                                        || r.shared.as_ref().map(|s| s.as_str())
                                            == Some(step.as_str())
                                })
                                .map(|r| &r.id)
                        })
                        .collect(),
                )
            }
            None if table.ordered => Some(table.results.iter().map(|r| &r.id).collect()),
            None => None,
        }
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
    /// The results form an ordered scale, listed lowest first (e.g. Size:
    /// Trading Post → Capital), and the base layout's faces ascend in the
    /// same order, so a dice modifier moves the result along the scale.
    /// Like `selection` and `status`, it cannot be changed by a customisation.
    #[serde(default, skip_serializing_if = "is_false")]
    pub ordered: bool,
    /// The scale this table's results lie on, for Shift and Cap modifiers.
    /// Without it, an `ordered` table's results in listed order are its
    /// scale.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scale: Option<ScaleId>,
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

    /// The result given instead of `rolled` when it is unique and already
    /// `held`: its fallback, so a unique result is never rerolled.
    pub fn fallback_for(&self, rolled: &TableResult, held: &[ResultId]) -> Option<&TableResult> {
        let fallback = rolled.fallback.as_ref()?;
        (rolled.unique && held.contains(&rolled.id))
            .then(|| self.result(fallback))
            .flatten()
    }
}

/// One outcome of a table.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TableResult {
    pub id: ResultId,
    /// May be left empty when the result lists a shared result, whose name
    /// it then takes.
    #[serde(default)]
    pub name: String,
    /// The shared result this entry lists. Its name, description, tags,
    /// eligibility, value, label variants and generator are used for every
    /// field this entry leaves empty; faces, enabled and uniqueness belong
    /// to the table.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shared: Option<SharedResultId>,
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
    /// A subordinate generator that will elaborate this result later (e.g.
    /// `authorityNpcs.council` for the members of a Local Council). Optional
    /// and not yet resolved: no generators exist in this version, so it has
    /// no effect on generation. Like `ordered`, a customisation of a
    /// bundled result keeps the default's reference.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub generator: Option<GeneratorId>,
    /// The number the result stands for, as structured data (e.g. `4` for
    /// "4 Representatives"). Recorded with the roll. Must be a positive
    /// integer; within a table, either every result has one or none does.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<i64>,
    /// When allocating roles (see `roles.rs`), a unique result can be held
    /// by one person only. Rolling it again resolves as `fallback`, never a
    /// reroll. Results are repeatable unless marked unique.
    #[serde(default, skip_serializing_if = "is_false")]
    pub unique: bool,
    /// The repeatable result (in this table) a unique result resolves as
    /// when it is already held. Required for, and only for, unique results.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fallback: Option<ResultId>,
    /// Display labels chosen by a tag of the person who holds the result,
    /// first match first (e.g. `gender.woman` → "Lady"). Otherwise `name`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub label_variants: Vec<LabelVariant>,
    /// Tables rolled next whenever this result comes up (after any tables
    /// its tags route to), e.g. a Hazard table after "Dangerous".
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub then_roll: Vec<TableId>,
}

/// A display label for a result, used when its holder has `tag`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LabelVariant {
    pub tag: TagId,
    pub name: String,
}

impl LabelVariant {
    /// The label for a holder with `tags`: the first matching variant, else
    /// `name`.
    pub fn pick(variants: &[LabelVariant], name: &str, tags: &[TagId]) -> String {
        variants
            .iter()
            .find(|v| tags.contains(&v.tag))
            .map_or_else(|| name.to_owned(), |v| v.name.clone())
    }
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

/// A boolean test over what generation knows so far: the active tags, the
/// generation settings and, inside flows, the results and values earlier
/// steps stored. `all` is AND, `any` is OR.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Condition {
    HasTag(TagId),
    All(Vec<Condition>),
    Any(Vec<Condition>),
    Not(Box<Condition>),
    /// A toggle setting is on, e.g. `{ "setting_on": "app.godMode" }`.
    SettingOn(SettingId),
    /// A dropdown setting has the option `value` (or a text setting the
    /// text `value`).
    SettingIs {
        setting: SettingId,
        value: String,
    },
    /// A number setting lies within `min..=max`.
    SettingBetween {
        setting: SettingId,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        min: Option<i64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        max: Option<i64>,
    },
    /// A multi-select setting has the option `value` ticked.
    SettingHas {
        setting: SettingId,
        value: String,
    },
    /// Flows only: the result an earlier step stored is one of `results`.
    ResultIs {
        step: StepId,
        results: Vec<ResultId>,
    },
    /// Flows only: the number an earlier step stored (a result's `value`
    /// or a secondary roll's total) lies within `min..=max`.
    ValueIn {
        step: StepId,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        min: Option<i64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        max: Option<i64>,
    },
}

/// What conditions are evaluated against. Unknown settings, results and
/// values make their tests false.
pub trait Facts {
    fn has_tag(&self, tag: &TagId) -> bool;

    fn setting(&self, _id: &SettingId) -> Option<SettingValue> {
        None
    }

    fn result_of(&self, _step: &StepId) -> Option<ResultId> {
        None
    }

    fn value_of(&self, _step: &StepId) -> Option<i64> {
        None
    }
}

/// Facts that are only a set of tags.
struct TagFacts<'a>(&'a dyn Fn(&TagId) -> bool);

impl Facts for TagFacts<'_> {
    fn has_tag(&self, tag: &TagId) -> bool {
        (self.0)(tag)
    }
}

impl Condition {
    /// Evaluates against tags only; setting, result and value tests are
    /// false.
    pub fn evaluate(&self, has_tag: &dyn Fn(&TagId) -> bool) -> bool {
        self.evaluate_with(&TagFacts(has_tag))
    }

    pub fn evaluate_with(&self, facts: &dyn Facts) -> bool {
        match self {
            Condition::HasTag(t) => facts.has_tag(t),
            Condition::All(cs) => cs.iter().all(|c| c.evaluate_with(facts)),
            Condition::Any(cs) => cs.iter().any(|c| c.evaluate_with(facts)),
            Condition::Not(c) => !c.evaluate_with(facts),
            Condition::SettingOn(id) => facts.setting(id) == Some(SettingValue::Toggle(true)),
            Condition::SettingIs { setting, value } => {
                facts.setting(setting) == Some(SettingValue::Choice(value.clone()))
            }
            Condition::SettingBetween { setting, min, max } => match facts.setting(setting) {
                Some(SettingValue::Number(n)) => {
                    min.is_none_or(|min| n >= min) && max.is_none_or(|max| n <= max)
                }
                _ => false,
            },
            Condition::SettingHas { setting, value } => matches!(
                facts.setting(setting),
                Some(SettingValue::Many(v)) if v.contains(value)
            ),
            Condition::ResultIs { step, results } => {
                facts.result_of(step).is_some_and(|r| results.contains(&r))
            }
            Condition::ValueIn { step, min, max } => facts
                .value_of(step)
                .is_some_and(|v| min.is_none_or(|min| v >= min) && max.is_none_or(|max| v <= max)),
        }
    }

    /// Every tag the condition refers to.
    pub fn tags(&self) -> Vec<&TagId> {
        let mut out = Vec::new();
        self.visit(&mut |c| {
            if let Condition::HasTag(t) = c {
                out.push(t);
            }
        });
        out
    }

    /// Every setting the condition refers to.
    pub fn settings(&self) -> Vec<&SettingId> {
        let mut out = Vec::new();
        self.visit(&mut |c| match c {
            Condition::SettingOn(id)
            | Condition::SettingIs { setting: id, .. }
            | Condition::SettingBetween { setting: id, .. }
            | Condition::SettingHas { setting: id, .. } => out.push(id),
            _ => {}
        });
        out
    }

    /// Every flow step whose stored result or value the condition reads.
    pub fn steps(&self) -> Vec<&StepId> {
        let mut out = Vec::new();
        self.visit(&mut |c| match c {
            Condition::ResultIs { step, .. } | Condition::ValueIn { step, .. } => out.push(step),
            _ => {}
        });
        out
    }

    /// A plain-language account of a setting, result or value test (the
    /// tests interfaces show as they are), e.g. "Setting Extras is on".
    /// `None` for tag tests and for all / any / not.
    pub fn describe_test(&self) -> Option<String> {
        match self {
            Condition::SettingOn(id) => Some(format!("Setting {id} is on")),
            Condition::SettingIs { setting, value } => {
                Some(format!("Setting {setting} is {value}"))
            }
            Condition::SettingBetween { setting, min, max } => Some(match (min, max) {
                (Some(a), Some(b)) => format!("Setting {setting} is {a} to {b}"),
                (Some(a), None) => format!("Setting {setting} is at least {a}"),
                (None, Some(b)) => format!("Setting {setting} is at most {b}"),
                (None, None) => format!("Setting {setting} has a number"),
            }),
            Condition::SettingHas { setting, value } => {
                Some(format!("Setting {setting} includes {value}"))
            }
            Condition::ResultIs { step, results } => {
                let list: Vec<&str> = results.iter().map(ResultId::as_str).collect();
                Some(format!("Step {step} gave {}", list.join(" or ")))
            }
            Condition::ValueIn { step, min, max } => Some(match (min, max) {
                (Some(a), Some(b)) => format!("Step {step}'s number is {a} to {b}"),
                (Some(a), None) => format!("Step {step}'s number is at least {a}"),
                (None, Some(b)) => format!("Step {step}'s number is at most {b}"),
                (None, None) => format!("Step {step} stored a number"),
            }),
            Condition::HasTag(_) | Condition::All(_) | Condition::Any(_) | Condition::Not(_) => {
                None
            }
        }
    }

    /// Calls `f` on this condition and every condition inside it.
    pub fn visit<'a>(&'a self, f: &mut dyn FnMut(&'a Condition)) {
        f(self);
        match self {
            Condition::All(cs) | Condition::Any(cs) => cs.iter().for_each(|c| c.visit(f)),
            Condition::Not(c) => c.visit(f),
            _ => {}
        }
    }
}

/// A change to how a table's result comes out. **Shift** moves the result
/// along the table's ordered scale (results, not faces); **Plus** and
/// **Minus** change the rolled total before it is read; **Cap at Highest**
/// and **Cap at Lowest** give the highest or lowest result on the scale.
/// Serialised as `{ "shift": 1 }`, `{ "plus": 2 }`, `{ "minus": 1 }`,
/// `"cap_at_highest"` or `"cap_at_lowest"`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Modifier {
    /// Steps along the scale: positive is higher, negative is lower.
    Shift(i32),
    Plus(i32),
    Minus(i32),
    CapAtHighest,
    CapAtLowest,
}

/// Largest shift, in steps, a modifier may make.
pub const MAX_SHIFT: i32 = 100;
/// Largest amount a Plus or Minus modifier may add or subtract.
pub const MAX_PLUS_MINUS: i32 = 1000;
/// Largest percentage a Weight effect may set (ten times as likely).
pub const MAX_WEIGHT_PERCENT: u32 = 1000;

impl Modifier {
    /// The change to the rolled total (Plus and Minus only).
    pub fn total_delta(&self) -> i32 {
        match self {
            Modifier::Plus(n) => *n,
            Modifier::Minus(n) => -n,
            _ => 0,
        }
    }

    /// Whether the modifier moves the result along a scale.
    pub fn needs_scale(&self) -> bool {
        matches!(
            self,
            Modifier::Shift(_) | Modifier::CapAtHighest | Modifier::CapAtLowest
        )
    }

    /// Why the modifier is not usable, if it is not.
    pub fn problem(&self) -> Option<String> {
        match *self {
            Modifier::Shift(0) => Some("a shift must move at least one step".into()),
            Modifier::Shift(n) if n.abs() > MAX_SHIFT => {
                Some(format!("a shift can move at most {MAX_SHIFT} steps"))
            }
            Modifier::Plus(n) | Modifier::Minus(n) if n < 1 => {
                Some("the amount must be a positive whole number".into())
            }
            Modifier::Plus(n) | Modifier::Minus(n) if n > MAX_PLUS_MINUS => {
                Some(format!("the amount can be at most {MAX_PLUS_MINUS}"))
            }
            _ => None,
        }
    }
}

impl fmt::Display for Modifier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Modifier::Shift(n) => write!(f, "Shift {n:+}"),
            Modifier::Plus(n) => write!(f, "Plus {n}"),
            Modifier::Minus(n) => write!(f, "Minus {n}"),
            Modifier::CapAtHighest => f.write_str("Cap at Highest"),
            Modifier::CapAtLowest => f.write_str("Cap at Lowest"),
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
    /// While the tag is active, a rolled result of `table` moves `steps`
    /// along the table's scale (Shift +1, Shift -2, ...).
    Shift { table: TableId, steps: i32 },
    /// While the tag is active, `amount` is added to `table`'s rolled total.
    Plus { table: TableId, amount: i32 },
    /// While the tag is active, `amount` is taken from `table`'s rolled
    /// total.
    Minus { table: TableId, amount: i32 },
    /// While the tag is active, a rolled result of `table` is the highest
    /// result on its scale that can be had.
    CapAtHighest { table: TableId },
    /// While the tag is active, a rolled result of `table` is the lowest
    /// result on its scale that can be had.
    CapAtLowest { table: TableId },
    /// While the tag is active, these disabled results of `table` can be
    /// rolled and chosen (on the faces they already have).
    EnableResults {
        table: TableId,
        results: Vec<ResultId>,
    },
    /// While the tag is active, `result` of `table` is `percent`% as
    /// likely as usual (200 is twice as likely, 50 half as likely). The
    /// table's results are laid out again on its own dice to match, so the
    /// roll stays a physical roll.
    Weight {
        table: TableId,
        result: ResultId,
        percent: u32,
    },
    /// Each time the tag is emitted by a step of a flow, run `flow` right
    /// after that step.
    RunFlow { flow: FlowId },
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
    StrongDecrease,
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
            | Effect::RedirectTable { table, .. }
            | Effect::Shift { table, .. }
            | Effect::Plus { table, .. }
            | Effect::Minus { table, .. }
            | Effect::CapAtHighest { table }
            | Effect::CapAtLowest { table }
            | Effect::EnableResults { table, .. }
            | Effect::Weight { table, .. } => Some(table),
            Effect::Intent { table, .. } => table.as_ref(),
            Effect::SecondaryRoll { .. } | Effect::AddTag { .. } | Effect::RunFlow { .. } => None,
        }
    }

    /// The modifier a Shift, Plus, Minus or Cap effect applies.
    pub fn modifier(&self) -> Option<Modifier> {
        match self {
            Effect::Shift { steps, .. } => Some(Modifier::Shift(*steps)),
            Effect::Plus { amount, .. } => Some(Modifier::Plus(*amount)),
            Effect::Minus { amount, .. } => Some(Modifier::Minus(*amount)),
            Effect::CapAtHighest { .. } => Some(Modifier::CapAtHighest),
            Effect::CapAtLowest { .. } => Some(Modifier::CapAtLowest),
            _ => None,
        }
    }

    /// The effect that applies `modifier` to `table`.
    pub fn from_modifier(table: TableId, modifier: Modifier) -> Self {
        match modifier {
            Modifier::Shift(steps) => Effect::Shift { table, steps },
            Modifier::Plus(amount) => Effect::Plus { table, amount },
            Modifier::Minus(amount) => Effect::Minus { table, amount },
            Modifier::CapAtHighest => Effect::CapAtHighest { table },
            Modifier::CapAtLowest => Effect::CapAtLowest { table },
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
