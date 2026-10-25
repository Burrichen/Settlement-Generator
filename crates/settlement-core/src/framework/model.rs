//! The framework's content types: shared results, scales, settings, object
//! types and flows. All of them are data; nothing here names a settlement
//! concept.

use serde::{Deserialize, Serialize};

use crate::content::{Condition, EffectStatus, LabelVariant, Modifier};
use crate::ids::{
    FlowId, GeneratorId, ObjectTypeId, ResultId, ScaleId, SettingId, SharedResultId, StepId,
    TableId, TagId,
};
use crate::naming::NamingSubject;

fn yes() -> bool {
    true
}

fn is_true(b: &bool) -> bool {
    *b
}

fn is_false(b: &bool) -> bool {
    !*b
}

// ----- shared results and scales ---------------------------------------------

/// A canonical result that several tables can list (see
/// [`TableResult::shared`](crate::content::TableResult::shared)). Defining an
/// outcome once keeps its name, tags and meaning the same everywhere it is
/// rolled.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SharedResult {
    pub id: SharedResultId,
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<TagId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub eligibility: Option<Condition>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<i64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub label_variants: Vec<LabelVariant>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub generator: Option<GeneratorId>,
}

/// An ordered list of results, lowest first, that Shift and Cap modifiers
/// move along. A table result is on the scale when its ID, or the ID of the
/// shared result it lists, is one of the steps, so one scale can serve
/// several tables.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Scale {
    pub id: ScaleId,
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    pub steps: Vec<ResultId>,
}

// ----- settings --------------------------------------------------------------

/// Settings built into the application, which conditions can test like
/// content-defined settings.
pub const APP_SETTINGS: [&str; 2] = [GOD_MODE_SETTING, BARE_MINIMUM_BUILDINGS_SETTING];
/// God Mode, as a toggle setting.
pub const GOD_MODE_SETTING: &str = "app.godMode";
/// The former ID of Bare Minimum Buildings, kept as an alias of
/// [`BARE_MINIMUM_BUILDINGS`] so content and flows that name it still work.
pub const BARE_MINIMUM_BUILDINGS_SETTING: &str = "app.bareMinimumBuildings";
/// Bare Minimum Buildings: an ordinary on/off setting declared by the
/// bundled content.
pub const BARE_MINIMUM_BUILDINGS: &str = "setting.bareMinimumBuildings";

/// The setting `id` stands for: an alias's setting, else itself.
pub fn canonical_setting(id: &SettingId) -> SettingId {
    if id.as_str() == BARE_MINIMUM_BUILDINGS_SETTING {
        SettingId::new(BARE_MINIMUM_BUILDINGS)
    } else {
        id.clone()
    }
}

/// Longest text a text setting may hold.
pub const MAX_SETTING_TEXT: usize = 200;
/// Prefix reserved for the application's own settings.
pub const APP_SETTING_PREFIX: &str = "app.";

/// A setting's value: on/off, a number, the ID of one of its options (or
/// a text setting's text), or the IDs of the options ticked.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum SettingValue {
    Toggle(bool),
    Number(i64),
    Choice(String),
    Many(Vec<String>),
}

/// A generation setting the user chooses (e.g. "Include ruins"), which
/// conditions can test to change what is generated.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SettingDef {
    pub id: SettingId,
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    #[serde(flatten)]
    pub kind: SettingKind,
    /// A `planned` setting is documented but not offered: generation uses
    /// its default.
    #[serde(default, skip_serializing_if = "EffectStatus::is_active")]
    pub status: EffectStatus,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub note: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SettingKind {
    /// On or off.
    Toggle {
        #[serde(default)]
        default: bool,
    },
    /// A whole number, optionally between `min` and `max`.
    Number {
        #[serde(default)]
        default: i64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        min: Option<i64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        max: Option<i64>,
    },
    /// One of a list of options (a dropdown).
    Choice {
        options: Vec<SettingOption>,
        default: String,
    },
    /// Any number of a list of options.
    MultiSelect {
        options: Vec<SettingOption>,
        #[serde(default)]
        default: Vec<String>,
    },
    /// Free text (e.g. a name to use), at most [`MAX_SETTING_TEXT`]
    /// characters.
    Text {
        #[serde(default)]
        default: String,
    },
}

impl SettingKind {
    /// The kind's name as the editors show it.
    pub fn label(&self) -> &'static str {
        match self {
            SettingKind::Toggle { .. } => "On/off",
            SettingKind::Number { .. } => "Number",
            SettingKind::Choice { .. } => "Dropdown",
            SettingKind::MultiSelect { .. } => "Multi-select",
            SettingKind::Text { .. } => "Text",
        }
    }

    /// The options of a dropdown or multi-select.
    pub fn options(&self) -> &[SettingOption] {
        match self {
            SettingKind::Choice { options, .. } | SettingKind::MultiSelect { options, .. } => {
                options
            }
            _ => &[],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SettingOption {
    pub id: String,
    pub name: String,
}

impl SettingDef {
    pub fn default_value(&self) -> SettingValue {
        match &self.kind {
            SettingKind::Toggle { default } => SettingValue::Toggle(*default),
            SettingKind::Number { default, .. } => SettingValue::Number(*default),
            SettingKind::Choice { default, .. } => SettingValue::Choice(default.clone()),
            SettingKind::MultiSelect { default, .. } => SettingValue::Many(default.clone()),
            SettingKind::Text { default } => SettingValue::Choice(default.clone()),
        }
    }

    /// Whether `value` is one this setting can take.
    pub fn accepts(&self, value: &SettingValue) -> bool {
        match (&self.kind, value) {
            (SettingKind::Toggle { .. }, SettingValue::Toggle(_)) => true,
            (SettingKind::Number { min, max, .. }, SettingValue::Number(n)) => {
                min.is_none_or(|m| *n >= m) && max.is_none_or(|m| *n <= m)
            }
            (SettingKind::Choice { options, .. }, SettingValue::Choice(v)) => {
                options.iter().any(|o| &o.id == v)
            }
            (SettingKind::MultiSelect { options, .. }, SettingValue::Many(v)) => {
                v.iter().all(|id| options.iter().any(|o| &o.id == id))
            }
            // An empty multi-select reads like an empty list.
            (SettingKind::Text { .. }, SettingValue::Choice(v)) => {
                v.chars().count() <= MAX_SETTING_TEXT
            }
            _ => false,
        }
    }

    pub fn is_planned(&self) -> bool {
        !self.status.is_active()
    }

    /// A value in words, e.g. "On", "3", "Large" or "Coast, Forest".
    pub fn describe_value(&self, value: &SettingValue) -> String {
        let name = |id: &str| {
            self.kind
                .options()
                .iter()
                .find(|o| o.id == id)
                .map_or_else(|| id.to_owned(), |o| o.name.clone())
        };
        match value {
            SettingValue::Toggle(true) => "On".into(),
            SettingValue::Toggle(false) => "Off".into(),
            SettingValue::Number(n) => n.to_string(),
            SettingValue::Choice(v) if matches!(self.kind, SettingKind::Text { .. }) => {
                format!("“{v}”")
            }
            SettingValue::Choice(v) => name(v),
            SettingValue::Many(v) if v.is_empty() => "None".into(),
            SettingValue::Many(v) => v.iter().map(|id| name(id)).collect::<Vec<_>>().join(", "),
        }
    }
}

// ----- objects ---------------------------------------------------------------

/// A kind of thing a flow creates, such as a person or a building. Each
/// object gets its own stable ID, its own tags (which do not become the
/// generation's tags unless shared) and named fields filled by its steps.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObjectType {
    pub id: ObjectTypeId,
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    /// What an object is called until it is named, with `{n}` for its
    /// number, e.g. `Figure {n}`. Defaults to the type's name and number.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub label: String,
    /// Start of each object's ID, followed by its number, e.g. `building`
    /// gives `building-01`. Defaults to the type ID.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub id_prefix: String,
    /// The object's fields, in display order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fields: Vec<ObjectField>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObjectField {
    /// Stable within the type, e.g. `species`.
    pub id: String,
    pub name: String,
}

impl ObjectType {
    pub fn field(&self, id: &str) -> Option<&ObjectField> {
        self.fields.iter().find(|f| f.id == id)
    }

    /// The label of object number `n` (1-based) before it is named.
    pub fn label_for(&self, n: usize) -> String {
        if self.label.is_empty() {
            format!("{} {n}", self.name)
        } else {
            self.label.replace("{n}", &n.to_string())
        }
    }

    /// The stable ID of object number `n`.
    pub fn id_for(&self, n: usize) -> String {
        let prefix = if self.id_prefix.is_empty() {
            self.id.as_str()
        } else {
            self.id_prefix.as_str()
        };
        format!("{prefix}-{n:02}")
    }
}

// ----- flows -----------------------------------------------------------------

/// An ordered list of generation steps. The content's `start` flow is the
/// pipeline a generation runs, one stage per top-level step; any flow can
/// also be run from another as a subflow.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Flow {
    pub id: FlowId,
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    pub steps: Vec<FlowStep>,
    /// How the flow editor lays the flow out. Never affects generation.
    #[serde(default, skip_serializing_if = "FlowLayout::is_empty")]
    pub editor: FlowLayout,
}

/// The flow editor's own data: where nodes were placed, and steps the user
/// has taken out of the flow without deleting them. Detached steps never
/// run (validation reports them as unreachable).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct FlowLayout {
    /// Node positions by step ID, `[x, y]`.
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub positions: std::collections::BTreeMap<StepId, [f64; 2]>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub detached: Vec<FlowStep>,
}

impl FlowLayout {
    pub fn is_empty(&self) -> bool {
        self.positions.is_empty() && self.detached.is_empty()
    }
}

impl Flow {
    /// Every step of the flow, nested ones included, in order.
    pub fn all_steps(&self) -> Vec<&FlowStep> {
        let mut out = Vec::new();
        for step in &self.steps {
            step.collect(&mut out);
        }
        out
    }

    pub fn step(&self, id: &StepId) -> Option<&FlowStep> {
        self.all_steps().into_iter().find(|s| &s.id == id)
    }
}

/// One step of a flow.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FlowStep {
    /// Unique within the flow. Later steps refer to what this step stored
    /// by this ID.
    pub id: StepId,
    /// Display label (e.g. "Region"); defaults to the table's name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    /// The step only runs while this holds; otherwise it is skipped.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub when: Option<Condition>,
    /// Inactive steps are skipped, but still checked.
    #[serde(default = "yes", skip_serializing_if = "is_true")]
    pub active: bool,
    /// `planned` steps are design notes: never run, and their references are
    /// not checked.
    #[serde(default, skip_serializing_if = "EffectStatus::is_active")]
    pub status: EffectStatus,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub note: String,
    /// Top-level steps only: the stage elaborates the one before it and is
    /// shown with it.
    #[serde(default, skip_serializing_if = "is_false")]
    pub sub_stage: bool,
    #[serde(flatten)]
    pub action: StepAction,
}

impl FlowStep {
    /// Whether the step takes part in generation (active and not planned).
    pub fn runs(&self) -> bool {
        self.active && self.status.is_active()
    }

    /// The steps directly inside this one.
    pub fn children(&self) -> Vec<&FlowStep> {
        match &self.action {
            StepAction::Branch { cases, otherwise } => cases
                .iter()
                .flat_map(|c| c.steps.iter())
                .chain(otherwise)
                .collect(),
            StepAction::Repeat { steps, .. }
            | StepAction::CreateObject { steps, .. }
            | StepAction::ForEach { steps, .. } => steps.iter().collect(),
            _ => Vec::new(),
        }
    }

    fn collect<'a>(&'a self, out: &mut Vec<&'a FlowStep>) {
        out.push(self);
        for child in self.children() {
            child.collect(out);
        }
    }
}

/// What a step does.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum StepAction {
    /// Roll (or choose) a result of `table`, with every table rule applied.
    /// The result is stored under the step's ID and, inside an object, as
    /// the object's `field`.
    Roll {
        table: TableId,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        field: Option<String>,
        /// Modifiers this step applies to the roll.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        modifiers: Vec<StepModifier>,
    },
    /// Roll extra dice and record the total under `label`; the total is
    /// stored as the step's value.
    SecondaryRoll { label: String, dice: String },
    /// Emit a tag, as if a result had.
    AddTag { tag: TagId },
    /// Give a fixed result of `table` without rolling (stored like a roll).
    SetResult {
        table: TableId,
        result: ResultId,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        field: Option<String>,
    },
    /// Give the current object's `field` a fixed number.
    SetValue { field: String, value: i64 },
    /// Conditional routing: run the steps of the first case whose condition
    /// holds, else `otherwise`.
    Branch {
        cases: Vec<BranchCase>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        otherwise: Vec<FlowStep>,
    },
    /// Run `steps` a number of times.
    Repeat { count: Count, steps: Vec<FlowStep> },
    /// Run another flow here.
    RunFlow {
        flow: FlowId,
        /// Deliberate recursion: how many times this step may run its flow
        /// inside itself (at most the subflow nesting limit). Without it, a
        /// flow that runs itself is an error.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        max_depth: Option<u32>,
    },
    /// Run `steps` once for every object of type `object` created earlier
    /// in this generation, inside that object (its own tags apply) — or, with
    /// `pairs`, once for every unordered pair of them (e.g. one relationship
    /// per pair of people).
    ForEach {
        object: ObjectTypeId,
        #[serde(default, skip_serializing_if = "is_false")]
        pairs: bool,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        steps: Vec<FlowStep>,
    },
    /// Make a tag inactive from here on (effects it already applied stay).
    RemoveTag { tag: TagId },
    /// From here on in this generation, every roll of `table` gets
    /// `modifier`.
    ApplyModifier { table: TableId, modifier: Modifier },
    /// Store a setting's value under the step's ID, for later conditions:
    /// a choice as its result, a toggle as the number 1 (on) or 0 (off).
    ReadSetting { setting: SettingId },
    /// End the current flow here: no later step of it runs.
    Stop,
    /// Create one object of type `object` and run `steps` for it.
    CreateObject {
        object: ObjectTypeId,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        steps: Vec<FlowStep>,
        /// The object's tags also become the generation's tags.
        #[serde(default, skip_serializing_if = "is_false")]
        share_tags: bool,
    },
    /// Generate a name with the naming scheme that applies.
    Name {
        #[serde(default)]
        subject: NamingSubject,
    },
    /// A stage provided by the application (top level of the start flow
    /// only), configured by `config`.
    Builtin {
        handler: String,
        #[serde(default, skip_serializing_if = "serde_json::Value::is_null")]
        config: serde_json::Value,
    },
}

/// One case of a branch.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BranchCase {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub when: Condition,
    pub steps: Vec<FlowStep>,
}

/// A modifier a roll step applies, optionally only while `when` holds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StepModifier {
    pub modifier: Modifier,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub when: Option<Condition>,
}

/// How many times a repeat runs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Count {
    /// A fixed number, e.g. `{ "times": 3 }`.
    Times(u32),
    /// The number an earlier step stored, plus `add` (never below zero),
    /// e.g. `{ "value_of": { "step": "size", "add": -1 } }`.
    ValueOf {
        step: StepId,
        #[serde(default, skip_serializing_if = "is_zero")]
        add: i64,
    },
    /// Rolled with physical dice, e.g. `{ "dice": "1d4" }`.
    Dice(String),
}

fn is_zero(v: &i64) -> bool {
    *v == 0
}

/// Most times a repeat may run, guarding against runaway generation.
pub const MAX_REPEAT: u32 = 100;
/// Deepest nesting of subflows (recursion protection).
pub const MAX_FLOW_DEPTH: usize = 16;
/// Most steps one flow run may take.
pub const MAX_FLOW_STEPS: usize = 10_000;
