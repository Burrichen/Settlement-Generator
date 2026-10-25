//! Plain-language descriptions of conditions and flows, shared by both
//! interfaces: the flow editor's read-only pseudocode summary, and the words
//! the condition builder shows.

use serde::{Deserialize, Serialize};

use crate::content::{Condition, ContentSet};
use crate::engine::Named;
use crate::ids::{ResultId, StepId};
use crate::naming::NamingSubject;

use super::{BranchCase, Count, Flow, FlowStep, SettingKind, StepAction};

/// One line of a flow's pseudocode.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PseudoLine {
    pub depth: usize,
    /// The step the line belongs to, for jumping to it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub step: Option<StepId>,
    pub text: String,
}

/// What a flow editor shows on one step's node.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NodeText {
    pub step: StepId,
    /// The kind of step, e.g. "Roll Table".
    pub kind: String,
    /// What it does, e.g. "Roll Authority".
    pub text: String,
    /// Its own condition in words, if it has one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub when: Option<String>,
    /// Each case of a branch in words.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cases: Vec<String>,
}

/// Names steps, tables, results and settings for descriptions.
pub struct Describer<'a> {
    content: &'a ContentSet,
    steps: Vec<FlowStep>,
}

impl<'a> Describer<'a> {
    /// Knows the steps of `flow` and the stages of the pipeline.
    pub fn new(content: &'a ContentSet, flow: Option<&Flow>) -> Self {
        let mut steps = content.pipeline();
        if let Some(flow) = flow {
            steps.extend(flow.all_steps().into_iter().cloned());
        }
        Describer { content, steps }
    }

    fn step(&self, id: &StepId) -> Option<&FlowStep> {
        self.steps.iter().rev().find(|s| &s.id == id)
    }

    /// What a step is called: its name, else its table's name.
    pub fn step_label(&self, id: &StepId) -> String {
        let Some(step) = self.step(id) else {
            return id.to_string();
        };
        if let Some(name) = &step.name {
            return name.clone();
        }
        match &step.action {
            StepAction::Roll { table, .. } | StepAction::SetResult { table, .. } => {
                self.table_name(table)
            }
            StepAction::ReadSetting { setting } => self
                .content
                .setting(setting)
                .map_or_else(|| setting.to_string(), |s| s.name.clone()),
            _ => id.to_string(),
        }
    }

    fn table_name(&self, id: &crate::ids::TableId) -> String {
        self.content
            .table(id)
            .map_or_else(|| id.to_string(), |t| t.name.clone())
    }

    /// The name of result `result` of the table step `step` rolls (or of
    /// the option of the setting it reads).
    pub fn result_name(&self, step: &StepId, result: &ResultId) -> String {
        let named = self.step(step).and_then(|s| match &s.action {
            StepAction::Roll { table, .. } | StepAction::SetResult { table, .. } => self
                .content
                .table(table)
                .and_then(|t| t.result(result))
                .map(|r| r.name.clone()),
            StepAction::ReadSetting { setting } => self
                .content
                .setting(&super::canonical_setting(setting))
                .and_then(|s| {
                    s.kind
                        .options()
                        .iter()
                        .find(|o| o.id == result.as_str())
                        .map(|o| o.name.clone())
                }),
            _ => None,
        });
        named.unwrap_or_else(|| result.to_string())
    }

    fn tag_name(&self, id: &crate::ids::TagId) -> String {
        self.content
            .tag(id)
            .map_or_else(|| id.to_string(), |t| t.name.clone())
    }

    fn setting_name(&self, id: &crate::ids::SettingId) -> String {
        match id.as_str() {
            super::GOD_MODE_SETTING => "God Mode".into(),
            super::BARE_MINIMUM_BUILDINGS_SETTING => "Bare Minimum Buildings".into(),
            _ => self
                .content
                .setting(id)
                .map_or_else(|| id.to_string(), |s| s.name.clone()),
        }
    }

    fn option_name(&self, setting: &crate::ids::SettingId, value: &str) -> String {
        self.content
            .setting(&super::canonical_setting(setting))
            .and_then(|s| s.kind.options().iter().find(|o| o.id == value))
            .map_or_else(|| value.to_owned(), |o| o.name.clone())
    }

    /// One test in words, e.g. "Authority is Military Authority".
    pub fn condition(&self, condition: &Condition) -> String {
        self.describe(condition, false)
    }

    fn describe(&self, condition: &Condition, nested: bool) -> String {
        let either = |names: Vec<String>| names.join(" or ");
        match condition {
            Condition::HasTag(t) => format!("{} is present", self.tag_name(t)),
            Condition::SettingOn(s) => format!("{} is on", self.setting_name(s)),
            Condition::SettingIs { setting, value } => {
                let text = self
                    .content
                    .setting(setting)
                    .is_some_and(|s| matches!(s.kind, SettingKind::Text { .. }));
                let option = if text {
                    format!("“{value}”")
                } else {
                    self.option_name(setting, value)
                };
                format!("{} is {option}", self.setting_name(setting))
            }
            Condition::SettingBetween { setting, min, max } => {
                let label = self.setting_name(setting);
                match (min, max) {
                    (Some(a), Some(b)) if a == b => format!("{label} is {a}"),
                    (Some(a), Some(b)) => format!("{label} is between {a} and {b}"),
                    (Some(a), None) => format!("{label} is at least {a}"),
                    (None, Some(b)) => format!("{label} is at most {b}"),
                    (None, None) => format!("{label} has a number"),
                }
            }
            Condition::SettingHas { setting, value } => format!(
                "{} includes {}",
                self.setting_name(setting),
                self.option_name(setting, value)
            ),
            Condition::ResultIs { step, results } => format!(
                "{} is {}",
                self.step_label(step),
                either(results.iter().map(|r| self.result_name(step, r)).collect())
            ),
            Condition::ValueIn { step, min, max } => {
                let label = self.step_label(step);
                match (min, max) {
                    (Some(a), Some(b)) if a == b => format!("{label} is {a}"),
                    (Some(a), Some(b)) => format!("{label} is between {a} and {b}"),
                    (Some(a), None) => format!("{label} is at least {a}"),
                    (None, Some(b)) => format!("{label} is at most {b}"),
                    (None, None) => format!("{label} has a number"),
                }
            }
            Condition::Not(inner) => match &**inner {
                Condition::HasTag(t) => format!("{} is not present", self.tag_name(t)),
                Condition::SettingOn(s) => format!("{} is off", self.setting_name(s)),
                Condition::SettingIs { setting, value } => format!(
                    "{} is not {}",
                    self.setting_name(setting),
                    self.option_name(setting, value)
                ),
                Condition::SettingHas { setting, value } => format!(
                    "{} does not include {}",
                    self.setting_name(setting),
                    self.option_name(setting, value)
                ),
                Condition::ResultIs { step, results } => format!(
                    "{} is not {}",
                    self.step_label(step),
                    either(results.iter().map(|r| self.result_name(step, r)).collect())
                ),
                Condition::Any(items) => format!(
                    "none of ({})",
                    items
                        .iter()
                        .map(|c| self.describe(c, true))
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
                other => format!("not ({})", self.describe(other, true)),
            },
            Condition::All(items) if items.is_empty() => "always".into(),
            Condition::Any(items) if items.is_empty() => "never".into(),
            Condition::All(items) | Condition::Any(items) => {
                let word = if matches!(condition, Condition::All(_)) {
                    " AND "
                } else {
                    " OR "
                };
                let text = items
                    .iter()
                    .map(|c| self.describe(c, true))
                    .collect::<Vec<_>>()
                    .join(word);
                if nested && items.len() > 1 {
                    format!("({text})")
                } else {
                    text
                }
            }
        }
    }

    /// The condition as lines: "IF a", "AND b", … (or "OR b").
    fn condition_lines(&self, condition: &Condition, first: &str) -> Vec<String> {
        match condition {
            Condition::All(items) if items.len() > 1 => items
                .iter()
                .enumerate()
                .map(|(i, c)| {
                    format!(
                        "{} {}",
                        if i == 0 { first } else { "AND" },
                        self.describe(c, true)
                    )
                })
                .collect(),
            Condition::Any(items) if items.len() > 1 => items
                .iter()
                .enumerate()
                .map(|(i, c)| {
                    format!(
                        "{} {}",
                        if i == 0 { first } else { "OR" },
                        self.describe(c, true)
                    )
                })
                .collect(),
            other => vec![format!("{first} {}", self.describe(other, false))],
        }
    }

    /// What one step does, as a short sentence (without its children).
    pub fn action(&self, step: &FlowStep) -> String {
        let content = self.content;
        let text = match &step.action {
            StepAction::Roll {
                table,
                field,
                modifiers,
            } => {
                let mut text = format!("roll {}", self.table_name(table));
                if let Some(field) = field {
                    text.push_str(&format!(" as the {field}"));
                }
                for m in modifiers {
                    text.push_str(&format!(" with {}", m.modifier));
                    if let Some(when) = &m.when {
                        text.push_str(&format!(" when {}", self.condition(when)));
                    }
                }
                text
            }
            StepAction::SecondaryRoll { label, dice } => format!("roll {dice} for {label}"),
            StepAction::AddTag { tag } => format!("add {}", self.tag_name(tag)),
            StepAction::RemoveTag { tag } => format!("remove {}", self.tag_name(tag)),
            StepAction::SetResult {
                table,
                result,
                field,
            } => {
                let name = content
                    .table(table)
                    .and_then(|t| t.result(result))
                    .map_or_else(|| result.to_string(), |r| r.name.clone());
                match field {
                    Some(field) => format!("set the {field} to {name}"),
                    None => format!("set {} to {name}", self.table_name(table)),
                }
            }
            StepAction::SetValue { field, value } => format!("set the {field} to {value}"),
            StepAction::Branch { .. } => "choose".into(),
            StepAction::Repeat { count, .. } => match count {
                Count::Times(n) => format!("repeat {n} time{}", if *n == 1 { "" } else { "s" }),
                Count::ValueOf { step, add } => {
                    let base = format!("repeat {} times", self.step_label(step));
                    match add {
                        0 => base,
                        a if *a > 0 => format!("{base} plus {a}"),
                        a => format!("{base} minus {}", -a),
                    }
                }
                Count::Dice(dice) => format!("repeat {dice} times"),
            },
            StepAction::ForEach { object, pairs, .. } => {
                let name = content
                    .object_type(object)
                    .map_or_else(|| object.to_string(), |o| o.name.clone());
                if *pairs {
                    format!("for each pair of {name}")
                } else {
                    format!("for each {name}")
                }
            }
            StepAction::RunFlow { flow, max_depth } => {
                let name = content
                    .flow(flow)
                    .map_or_else(|| flow.to_string(), |f| f.name.clone());
                match max_depth {
                    Some(d) => format!("run {name} (at most {d} level(s) deep)"),
                    None => format!("run {name}"),
                }
            }
            StepAction::CreateObject {
                object, share_tags, ..
            } => {
                let name = content
                    .object_type(object)
                    .map_or_else(|| object.to_string(), |o| o.name.clone());
                if *share_tags {
                    format!("create a {name} (its tags are shared)")
                } else {
                    format!("create a {name}")
                }
            }
            StepAction::Name { subject } => match subject {
                NamingSubject::Settlement => "name the settlement".into(),
                NamingSubject::Person => "name the person".into(),
            },
            StepAction::ApplyModifier { table, modifier } => format!(
                "apply {modifier} to every later roll of {}",
                self.table_name(table)
            ),
            StepAction::ReadSetting { setting } => {
                format!("read the setting {}", self.setting_name(setting))
            }
            StepAction::Stop => "stop".into(),
            StepAction::Builtin { handler, .. } => format!("run the built-in step {handler}"),
        };
        let mut text = capitalise(&text);
        if !step.status.is_active() {
            text = format!("(planned) {text}");
        } else if !step.active {
            text = format!("(off) {text}");
        }
        text
    }

    /// The node text of every step of `flow`, detached ones included.
    pub fn nodes(&self, flow: &Flow) -> Vec<NodeText> {
        let mut steps: Vec<&FlowStep> = flow.all_steps();
        for detached in &flow.editor.detached {
            collect(detached, &mut steps);
        }
        steps
            .into_iter()
            .map(|step| NodeText {
                step: step.id.clone(),
                kind: kind_label(&step.action).into(),
                text: self.action(step),
                when: step.when.as_ref().map(|c| self.condition(c)),
                cases: match &step.action {
                    StepAction::Branch { cases, .. } => {
                        cases.iter().map(|c| self.condition(&c.when)).collect()
                    }
                    _ => Vec::new(),
                },
            })
            .collect()
    }

    /// The whole flow as indented pseudocode.
    pub fn pseudocode(&self, flow: &Flow) -> Vec<PseudoLine> {
        let mut out = Vec::new();
        self.lines(&flow.steps, 0, &mut out);
        if out.is_empty() {
            out.push(PseudoLine {
                depth: 0,
                step: None,
                text: "(No steps yet.)".into(),
            });
        }
        out
    }

    fn lines(&self, steps: &[FlowStep], depth: usize, out: &mut Vec<PseudoLine>) {
        for step in steps {
            self.step_lines(step, depth, out);
        }
    }

    fn push(out: &mut Vec<PseudoLine>, depth: usize, step: &FlowStep, text: String) {
        out.push(PseudoLine {
            depth,
            step: Some(step.id.clone()),
            text,
        });
    }

    fn step_lines(&self, step: &FlowStep, depth: usize, out: &mut Vec<PseudoLine>) {
        // A step's own condition wraps it.
        let depth = match &step.when {
            Some(when) => {
                for line in self.condition_lines(when, "IF") {
                    Self::push(out, depth, step, line);
                }
                Self::push(out, depth, step, "THEN".into());
                depth + 1
            }
            None => depth,
        };
        match &step.action {
            StepAction::Branch { cases, otherwise } => {
                self.branch_lines(step, cases, otherwise, depth, out);
            }
            StepAction::Repeat { steps, .. }
            | StepAction::ForEach { steps, .. }
            | StepAction::CreateObject { steps, .. } => {
                Self::push(out, depth, step, format!("{}:", self.action(step)));
                self.lines(steps, depth + 1, out);
            }
            _ => Self::push(out, depth, step, format!("{}.", self.action(step))),
        }
    }

    fn branch_lines(
        &self,
        step: &FlowStep,
        cases: &[BranchCase],
        otherwise: &[FlowStep],
        depth: usize,
        out: &mut Vec<PseudoLine>,
    ) {
        for (i, case) in cases.iter().enumerate() {
            let first = if i == 0 { "IF" } else { "OTHERWISE IF" };
            for line in self.condition_lines(&case.when, first) {
                Self::push(out, depth, step, line);
            }
            match case.steps.as_slice() {
                [only] if only.when.is_none() && only.children().is_empty() => {
                    Self::push(
                        out,
                        depth,
                        only,
                        format!("THEN {}.", lower_first(&self.action(only))),
                    );
                }
                steps => {
                    Self::push(out, depth, step, "THEN".into());
                    self.lines(steps, depth + 1, out);
                }
            }
        }
        if !otherwise.is_empty() {
            Self::push(out, depth, step, "OTHERWISE".into());
            self.lines(otherwise, depth + 1, out);
        }
    }
}

fn collect<'a>(step: &'a FlowStep, out: &mut Vec<&'a FlowStep>) {
    out.push(step);
    for child in step.children() {
        collect(child, out);
    }
}

/// The palette name of a step's kind (or a name for kinds it does not
/// offer).
fn kind_label(action: &StepAction) -> &'static str {
    match super::edit::StepKind::of(action) {
        Some(kind) => kind.label(),
        None => match action {
            StepAction::SecondaryRoll { .. } => "Secondary Roll",
            StepAction::SetResult { .. } => "Set Result",
            StepAction::Name { .. } => "Name",
            _ => "Built-in Step",
        },
    }
}

fn capitalise(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        Some(c) => c.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

fn lower_first(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        Some(c) => c.to_lowercase().chain(chars).collect(),
        None => String::new(),
    }
}

/// Something a condition can test: a step that stores a result or a number.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConditionSubject {
    pub step: StepId,
    pub label: String,
    /// The results it can give (empty when it only stores a number).
    pub results: Vec<Named<String>>,
    /// It stores a number, so "is between" tests apply.
    pub numeric: bool,
    /// It is a stage of the pipeline rather than a step of this flow.
    pub pipeline: bool,
}

/// The kind of a setting, for the tests a condition builder offers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SettingType {
    Toggle,
    Number,
    Choice,
    MultiSelect,
    Text,
}

impl SettingType {
    pub fn of(kind: &SettingKind) -> Self {
        match kind {
            SettingKind::Toggle { .. } => SettingType::Toggle,
            SettingKind::Number { .. } => SettingType::Number,
            SettingKind::Choice { .. } => SettingType::Choice,
            SettingKind::MultiSelect { .. } => SettingType::MultiSelect,
            SettingKind::Text { .. } => SettingType::Text,
        }
    }
}

/// A setting a condition can test.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConditionSetting {
    pub id: String,
    pub name: String,
    pub kind: SettingType,
    /// The options of a dropdown or multi-select.
    pub options: Vec<Named<String>>,
}

/// Everything the flow editor offers in its pickers, by name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FlowContext {
    pub subjects: Vec<ConditionSubject>,
    pub tables: Vec<Named<String>>,
    pub tags: Vec<Named<String>>,
    pub settings: Vec<ConditionSetting>,
    pub objects: Vec<Named<String>>,
    pub flows: Vec<Named<String>>,
    /// The step kinds of the palette.
    pub kinds: Vec<Named<String>>,
}

fn named(id: impl ToString, name: impl Into<String>) -> Named<String> {
    Named {
        id: id.to_string(),
        name: name.into(),
    }
}

impl Describer<'_> {
    /// The pickers' contents while editing `flow`.
    pub fn context(&self, flow: &Flow) -> FlowContext {
        let content = self.content;
        let pipeline: Vec<StepId> = content.pipeline().into_iter().map(|s| s.id).collect();
        let mut subjects: Vec<ConditionSubject> = Vec::new();
        for step in &self.steps {
            let (results, numeric) = match &step.action {
                StepAction::Roll { table, .. } | StepAction::SetResult { table, .. } => {
                    match content.table(table) {
                        Some(t) => (
                            t.results
                                .iter()
                                .map(|r| named(&r.id, r.name.clone()))
                                .collect(),
                            t.results.iter().any(|r| r.value.is_some()),
                        ),
                        None => (Vec::new(), false),
                    }
                }
                StepAction::SecondaryRoll { .. } | StepAction::SetValue { .. } => {
                    (Vec::new(), true)
                }
                StepAction::ReadSetting { setting } => {
                    match content.setting(&super::canonical_setting(setting)) {
                        Some(s) => match &s.kind {
                            SettingKind::Choice { options, .. } => (
                                options
                                    .iter()
                                    .map(|o| named(&o.id, o.name.clone()))
                                    .collect(),
                                false,
                            ),
                            SettingKind::Text { .. } => (Vec::new(), false),
                            // A toggle is 1 or 0, a multi-select the number
                            // ticked.
                            SettingKind::Toggle { .. }
                            | SettingKind::Number { .. }
                            | SettingKind::MultiSelect { .. } => (Vec::new(), true),
                        },
                        None => (Vec::new(), false),
                    }
                }
                _ => continue,
            };
            // A flow step shadows a pipeline stage with the same ID.
            subjects.retain(|s| s.step != step.id);
            subjects.push(ConditionSubject {
                step: step.id.clone(),
                label: self.step_label(&step.id),
                results,
                numeric,
                pipeline: pipeline.contains(&step.id)
                    && !flow.all_steps().iter().any(|s| s.id == step.id),
            });
        }
        let mut settings = vec![ConditionSetting {
            id: super::GOD_MODE_SETTING.into(),
            name: "God Mode".into(),
            kind: SettingType::Toggle,
            options: Vec::new(),
        }];
        settings.extend(content.settings.iter().map(|s| {
            ConditionSetting {
                id: s.id.to_string(),
                name: s.name.clone(),
                kind: SettingType::of(&s.kind),
                options: s
                    .kind
                    .options()
                    .iter()
                    .map(|o| named(&o.id, o.name.clone()))
                    .collect(),
            }
        }));
        let by_name = |mut list: Vec<Named<String>>| {
            list.sort_by_key(|n| n.name.to_lowercase());
            list
        };
        FlowContext {
            subjects,
            tables: by_name(
                content
                    .tables
                    .iter()
                    .map(|t| named(&t.id, t.name.clone()))
                    .collect(),
            ),
            tags: by_name(
                content
                    .tags
                    .iter()
                    .map(|t| named(&t.id, t.name.clone()))
                    .collect(),
            ),
            settings,
            objects: by_name(
                content
                    .objects
                    .iter()
                    .map(|o| named(&o.id, o.name.clone()))
                    .collect(),
            ),
            flows: by_name(
                content
                    .flows
                    .iter()
                    .map(|f| named(&f.id, f.name.clone()))
                    .collect(),
            ),
            kinds: super::edit::StepKind::ALL
                .iter()
                .map(|k| {
                    named(
                        serde_json::to_value(k)
                            .ok()
                            .and_then(|v| v.as_str().map(str::to_string))
                            .unwrap_or_default(),
                        k.label(),
                    )
                })
                .collect(),
        }
    }
}
