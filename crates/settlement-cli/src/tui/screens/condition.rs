//! A stepped editor for conditions (has tag / all of / any of / not /
//! setting on), shown as an indented tree. Used for every condition in the
//! editors. In a flow it also tests earlier results, numbers and choice
//! settings, and shows the condition in plain words.

use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};
use settlement_core::content::Condition;
use settlement_core::framework::describe::{
    ConditionSetting, ConditionSubject, FlowContext, SettingType,
};
use settlement_core::ids::{ResultId, SettingId, StepId, TagId};

use crate::tui::Ctx;
use crate::tui::widgets::{Modal, ModalResult, PickItem, Row, RowsState};

/// Where an edit applies: a path of child indices from the root.
type Path = Vec<usize>;

#[derive(Clone, Debug, PartialEq)]
enum Act {
    Node(Path),
    AddRoot,
}

enum Pending {
    Kind(Path, Vec<Kind>),
    Tag(Path),
    Setting(Path),
    /// Choosing the step for a result or number test.
    Subject(Path, Kind),
    Results(Path, StepId),
    Range(Path, StepId),
    Choice(Path),
    ChoiceOption(Path, SettingId),
    /// A number, multi-select or text setting, then its value.
    OtherSetting(Path, Kind),
    SettingRange(Path, SettingId),
    SettingTick(Path, SettingId),
    SettingText(Path, SettingId),
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Kind {
    HasTag,
    All,
    Any,
    Not,
    SettingOn,
    ResultIs,
    ValueIn,
    SettingIs,
    SettingNumber,
    SettingHas,
    SettingText,
    Remove,
}

impl Kind {
    fn label(self) -> &'static str {
        match self {
            Kind::HasTag => "Has tag",
            Kind::All => "All of",
            Kind::Any => "Any of",
            Kind::Not => "Not",
            Kind::SettingOn => "Setting is on",
            Kind::ResultIs => "Earlier result is",
            Kind::ValueIn => "Earlier number is in a range",
            Kind::SettingIs => "Setting option is",
            Kind::SettingNumber => "Number setting is in a range",
            Kind::SettingHas => "Multi-select setting includes",
            Kind::SettingText => "Text setting is",
            Kind::Remove => "Remove",
        }
    }
}

pub enum Outcome {
    Stay,
    Done,
}

pub struct ConditionEditor {
    pub title: String,
    pub value: Option<Condition>,
    /// The condition in plain words, kept up to date by the screen.
    pub sentence: Option<String>,
    tags: Vec<(TagId, String)>,
    /// The on/off settings a condition can test.
    settings: Vec<(SettingId, String)>,
    /// In a flow: the steps whose results and numbers can be tested.
    subjects: Vec<ConditionSubject>,
    /// In a flow: dropdown settings.
    choices: Vec<ConditionSetting>,
    /// In a flow: number, multi-select and text settings.
    others: Vec<ConditionSetting>,
    state: RowsState,
    modal: Option<(Modal, Pending)>,
}

impl ConditionEditor {
    pub fn new(
        title: &str,
        value: Option<Condition>,
        tags: Vec<(TagId, String)>,
        settings: Vec<(SettingId, String)>,
    ) -> Self {
        ConditionEditor {
            title: title.into(),
            value,
            sentence: None,
            tags,
            settings,
            subjects: Vec::new(),
            choices: Vec::new(),
            others: Vec::new(),
            state: RowsState::default(),
            modal: None,
        }
    }

    /// An editor for a condition inside a flow, with everything `context`
    /// offers to test.
    pub fn for_flow(title: &str, value: Option<Condition>, context: &FlowContext) -> Self {
        let mut editor = ConditionEditor::new(
            title,
            value,
            context
                .tags
                .iter()
                .map(|t| (TagId::new(&t.id), t.name.clone()))
                .collect(),
            context
                .settings
                .iter()
                .filter(|s| s.kind == SettingType::Toggle)
                .map(|s| (SettingId::new(&s.id), s.name.clone()))
                .collect(),
        );
        editor.subjects = context.subjects.clone();
        editor.choices = context
            .settings
            .iter()
            .filter(|s| s.kind == SettingType::Choice)
            .cloned()
            .collect();
        editor.others = context
            .settings
            .iter()
            .filter(|s| {
                matches!(
                    s.kind,
                    SettingType::Number | SettingType::MultiSelect | SettingType::Text
                )
            })
            .cloned()
            .collect();
        editor
    }

    fn kinds(&self, adding: bool) -> Vec<Kind> {
        let mut out = vec![
            Kind::HasTag,
            Kind::All,
            Kind::Any,
            Kind::Not,
            Kind::SettingOn,
        ];
        if self.subjects.iter().any(|s| !s.results.is_empty()) {
            out.push(Kind::ResultIs);
        }
        if self.subjects.iter().any(|s| s.numeric) {
            out.push(Kind::ValueIn);
        }
        if !self.choices.is_empty() {
            out.push(Kind::SettingIs);
        }
        for (kind, ty) in [
            (Kind::SettingNumber, SettingType::Number),
            (Kind::SettingHas, SettingType::MultiSelect),
            (Kind::SettingText, SettingType::Text),
        ] {
            if self.others.iter().any(|s| s.kind == ty) {
                out.push(kind);
            }
        }
        if !adding {
            out.push(Kind::Remove);
        }
        out
    }

    fn kind_picker(&self, path: Path, adding: bool) -> (Modal, Pending) {
        let kinds = self.kinds(adding);
        (
            Modal::pick(
                "Condition type",
                kinds.iter().map(|k| PickItem::new(k.label())).collect(),
                0,
            ),
            Pending::Kind(path, kinds),
        )
    }

    fn tag_name(&self, id: &TagId) -> String {
        if id.as_str().is_empty() {
            return "(choose a tag)".into();
        }
        self.tags
            .iter()
            .find(|(t, _)| t == id)
            .map_or_else(|| format!("{id} (missing)"), |(_, n)| n.clone())
    }

    fn subject(&self, step: &StepId) -> Option<&ConditionSubject> {
        self.subjects.iter().find(|s| &s.step == step)
    }

    fn step_label(&self, step: &StepId) -> String {
        if step.as_str().is_empty() {
            return "(choose a step)".into();
        }
        self.subject(step)
            .map_or_else(|| format!("{step} (missing)"), |s| s.label.clone())
    }

    fn label(&self, c: &Condition) -> String {
        match c {
            Condition::HasTag(t) => format!("Has tag: {}", self.tag_name(t)),
            Condition::All(_) => "All of:".into(),
            Condition::Any(_) => "Any of:".into(),
            Condition::Not(_) => "Not:".into(),
            Condition::SettingOn(id) => format!("Setting is on: {}", self.setting_name(id)),
            Condition::ResultIs { step, results } if !self.subjects.is_empty() => {
                let names: Vec<String> = results
                    .iter()
                    .map(|r| {
                        self.subject(step)
                            .and_then(|s| s.results.iter().find(|o| o.id == r.as_str()))
                            .map_or_else(|| r.to_string(), |o| o.name.clone())
                    })
                    .collect();
                format!("{} is {}", self.step_label(step), names.join(" or "))
            }
            Condition::ValueIn { step, min, max } if !self.subjects.is_empty() => {
                let label = self.step_label(step);
                match (min, max) {
                    (Some(a), Some(b)) => format!("{label} is between {a} and {b}"),
                    (Some(a), None) => format!("{label} is at least {a}"),
                    (None, Some(b)) => format!("{label} is at most {b}"),
                    (None, None) => format!("{label} has a number"),
                }
            }
            Condition::SettingIs { setting, value } if !self.choices.is_empty() => {
                let def = self.choices.iter().find(|s| s.id == setting.as_str());
                let option = def
                    .and_then(|d| d.options.iter().find(|o| &o.id == value))
                    .map_or_else(|| value.clone(), |o| o.name.clone());
                format!(
                    "{} is {option}",
                    def.map_or_else(|| setting.to_string(), |d| d.name.clone())
                )
            }
            Condition::SettingBetween { setting, min, max } => {
                let label = self.other_name(setting);
                match (min, max) {
                    (Some(a), Some(b)) => format!("{label} is between {a} and {b}"),
                    (Some(a), None) => format!("{label} is at least {a}"),
                    (None, Some(b)) => format!("{label} is at most {b}"),
                    (None, None) => format!("{label} has a number"),
                }
            }
            Condition::SettingHas { setting, value } => {
                let option = self
                    .others
                    .iter()
                    .find(|s| s.id == setting.as_str())
                    .and_then(|s| s.options.iter().find(|o| &o.id == value))
                    .map_or_else(|| value.clone(), |o| o.name.clone());
                format!("{} includes {option}", self.other_name(setting))
            }
            Condition::SettingIs { setting, value }
                if self.others.iter().any(|s| s.id == setting.as_str()) =>
            {
                format!("{} is “{value}”", self.other_name(setting))
            }
            other => other.describe_test().unwrap_or_default(),
        }
    }

    fn other_name(&self, id: &SettingId) -> String {
        self.others
            .iter()
            .find(|s| s.id == id.as_str())
            .map_or_else(|| id.to_string(), |s| s.name.clone())
    }

    fn others_of(&self, kind: Kind) -> Vec<&ConditionSetting> {
        let ty = match kind {
            Kind::SettingNumber => SettingType::Number,
            Kind::SettingHas => SettingType::MultiSelect,
            _ => SettingType::Text,
        };
        self.others.iter().filter(|s| s.kind == ty).collect()
    }

    fn rows(&self, ctx: &Ctx) -> Vec<Row<Act>> {
        let s = &ctx.styles;
        let Some(root) = &self.value else {
            return vec![
                Row::info(Line::styled("Always (no condition).", s.dim())),
                Row::item(Line::raw("+ Add condition"), Act::AddRoot),
            ];
        };
        let mut rows = Vec::new();
        self.flatten(root, Vec::new(), 0, &mut rows);
        rows
    }

    fn flatten(&self, c: &Condition, path: Path, depth: usize, out: &mut Vec<Row<Act>>) {
        out.push(Row::item(
            Line::raw(format!("{}{}", "    ".repeat(depth), self.label(c))),
            Act::Node(path.clone()),
        ));
        match c {
            Condition::All(items) | Condition::Any(items) => {
                for (i, child) in items.iter().enumerate() {
                    let mut p = path.clone();
                    p.push(i);
                    self.flatten(child, p, depth + 1, out);
                }
            }
            Condition::Not(child) => {
                let mut p = path.clone();
                p.push(0);
                self.flatten(child, p, depth + 1, out);
            }
            _ => {}
        }
    }

    fn node_mut<'a>(c: &'a mut Condition, path: &[usize]) -> &'a mut Condition {
        match (path.split_first(), c) {
            (None, c) => c,
            (Some((i, rest)), Condition::All(v) | Condition::Any(v)) => {
                Self::node_mut(&mut v[*i], rest)
            }
            (Some((_, rest)), Condition::Not(b)) => Self::node_mut(b, rest),
            (Some(_), c) => c,
        }
    }

    fn set_node(&mut self, path: &[usize], node: Option<Condition>) {
        let Some(root) = &mut self.value else {
            self.value = node;
            return;
        };
        match (path.split_last(), node) {
            (None, node) => self.value = node,
            (Some((last, parent)), node) => {
                let parent_node = Self::node_mut(root, parent);
                match (parent_node, node) {
                    (Condition::All(v) | Condition::Any(v), Some(n)) => v[*last] = n,
                    (Condition::All(v) | Condition::Any(v), None) => {
                        v.remove(*last);
                    }
                    (Condition::Not(b), Some(n)) => **b = n,
                    // Removing the only child of "Not" leaves a blank test.
                    (p @ Condition::Not(_), None) => *p = Condition::HasTag(TagId::new("")),
                    _ => {}
                }
            }
        }
    }

    fn setting_name(&self, id: &SettingId) -> String {
        if id.as_str().is_empty() {
            return "(choose a setting)".into();
        }
        self.settings
            .iter()
            .find(|(s, _)| s == id)
            .map_or_else(|| format!("{id} (missing)"), |(_, n)| n.clone())
    }

    fn setting_picker(&self) -> Modal {
        Modal::pick(
            "Choose a setting",
            self.settings
                .iter()
                .map(|(id, name)| PickItem::new(name.clone()).hint(id.to_string()))
                .collect(),
            0,
        )
    }

    fn tag_picker(&self) -> Modal {
        Modal::pick(
            "Choose a tag",
            self.tags
                .iter()
                .map(|(id, name)| PickItem::new(name.clone()).hint(id.to_string()))
                .collect(),
            0,
        )
    }

    /// The steps a result (or number) test can name.
    fn subjects_for(&self, kind: Kind) -> Vec<&ConditionSubject> {
        self.subjects
            .iter()
            .filter(|s| match kind {
                Kind::ValueIn => s.numeric,
                _ => !s.results.is_empty(),
            })
            .collect()
    }

    fn subject_picker(&self, kind: Kind) -> Modal {
        Modal::pick(
            "Which earlier step?",
            self.subjects_for(kind)
                .iter()
                .map(|s| {
                    PickItem::new(s.label.clone()).hint(if s.pipeline {
                        "earlier stage".to_owned()
                    } else {
                        s.step.to_string()
                    })
                })
                .collect(),
            0,
        )
    }

    fn results_picker(&self, step: &StepId, chosen: &[ResultId]) -> Modal {
        let options = self
            .subject(step)
            .map(|s| s.results.clone())
            .unwrap_or_default();
        Modal::check(
            "It is any of (Space to tick)",
            options.iter().map(|o| o.name.clone()).collect(),
            options
                .iter()
                .map(|o| chosen.iter().any(|c| c.as_str() == o.id))
                .collect(),
        )
    }

    fn open_editor_for(&mut self, path: Path) {
        let node = self
            .value
            .as_mut()
            .map(|r| Self::node_mut(r, &path).clone());
        self.modal = Some(match node {
            Some(Condition::HasTag(_)) => (self.tag_picker(), Pending::Tag(path)),
            Some(Condition::SettingOn(_)) => (self.setting_picker(), Pending::Setting(path)),
            Some(Condition::ResultIs { step, results }) if !self.subjects.is_empty() => (
                self.results_picker(&step, &results),
                Pending::Results(path, step),
            ),
            Some(Condition::ValueIn { step, min, max }) if !self.subjects.is_empty() => (
                Modal::text(
                    "Number range",
                    "e.g. 2-4, 3+ (at least 3) or -5 (at most 5).",
                    &range_text(min, max),
                ),
                Pending::Range(path, step),
            ),
            Some(Condition::SettingBetween { setting, min, max }) => (
                Modal::text(
                    "Number range",
                    "e.g. 2-4, 3+ (at least 3) or -5 (at most 5).",
                    &range_text(min, max),
                ),
                Pending::SettingRange(path, setting),
            ),
            Some(Condition::SettingHas { setting, .. }) => {
                let items = self.option_items(&setting);
                (
                    Modal::pick("Which option?", items, 0),
                    Pending::SettingTick(path, setting),
                )
            }
            Some(Condition::SettingIs { setting, value })
                if self.others.iter().any(|s| s.id == setting.as_str()) =>
            {
                (
                    Modal::text("Text", "The exact text to match.", &value),
                    Pending::SettingText(path, setting),
                )
            }
            Some(Condition::SettingIs { setting, .. }) if !self.choices.is_empty() => {
                let options = self.options_of(&setting);
                (
                    Modal::pick("Which option?", options, 0),
                    Pending::ChoiceOption(path, setting),
                )
            }
            _ => self.kind_picker(path, false),
        });
    }

    fn options_of(&self, setting: &SettingId) -> Vec<PickItem> {
        self.choices
            .iter()
            .find(|s| s.id == setting.as_str())
            .map(|s| {
                s.options
                    .iter()
                    .map(|o| PickItem::new(o.name.clone()))
                    .collect()
            })
            .unwrap_or_default()
    }

    fn option_items(&self, setting: &SettingId) -> Vec<PickItem> {
        self.others
            .iter()
            .find(|s| s.id == setting.as_str())
            .map(|s| {
                s.options
                    .iter()
                    .map(|o| PickItem::new(o.name.clone()))
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn handle(&mut self, key: KeyEvent, ctx: &mut Ctx) -> Outcome {
        if let Some((modal, _)) = &mut self.modal {
            let result = modal.handle(key, &ctx.styles);
            if let ModalResult::Open = result {
                return Outcome::Stay;
            }
            let (_, pending) = self.modal.take().expect("an open modal");
            self.finish(result, pending);
            return Outcome::Stay;
        }
        let rows = self.rows(ctx);
        if self.state.handle_nav(&rows, key) {
            return Outcome::Stay;
        }
        let current = self.state.current(&rows).cloned();
        match (key.code, current) {
            (KeyCode::Esc | KeyCode::Backspace, _) => return Outcome::Done,
            (KeyCode::Enter, Some(Act::AddRoot)) => {
                self.modal = Some(self.kind_picker(Vec::new(), true));
            }
            (KeyCode::Enter, Some(Act::Node(path))) => self.open_editor_for(path),
            (KeyCode::Char('t'), Some(Act::Node(path))) => {
                self.modal = Some(self.kind_picker(path, false));
            }
            (KeyCode::Char('a'), Some(Act::Node(path))) => {
                if let Some(root) = &mut self.value
                    && let Condition::All(v) | Condition::Any(v) = Self::node_mut(root, &path)
                {
                    v.push(Condition::HasTag(TagId::new("")));
                    let mut child = path.clone();
                    child.push(v.len() - 1);
                    self.modal = Some(self.kind_picker(child, true));
                }
            }
            (KeyCode::Char('d') | KeyCode::Delete, Some(Act::Node(path))) => {
                self.set_node(&path, None);
            }
            _ => {}
        }
        Outcome::Stay
    }

    /// Applies what a closed picker gave.
    fn finish(&mut self, result: ModalResult, pending: Pending) {
        match (result, pending) {
            (ModalResult::Picked(k), Pending::Kind(path, kinds)) => {
                let blank = || Condition::HasTag(TagId::new(""));
                let kind = kinds[k];
                let node = match kind {
                    Kind::HasTag => Some(blank()),
                    Kind::All => Some(Condition::All(vec![blank()])),
                    Kind::Any => Some(Condition::Any(vec![blank()])),
                    Kind::Not => Some(Condition::Not(Box::new(blank()))),
                    Kind::SettingOn => Some(Condition::SettingOn(SettingId::new(""))),
                    Kind::ResultIs => Some(Condition::ResultIs {
                        step: StepId::new(""),
                        results: Vec::new(),
                    }),
                    Kind::ValueIn => Some(Condition::ValueIn {
                        step: StepId::new(""),
                        min: Some(1),
                        max: None,
                    }),
                    Kind::SettingIs | Kind::SettingText => Some(Condition::SettingIs {
                        setting: SettingId::new(""),
                        value: String::new(),
                    }),
                    Kind::SettingNumber => Some(Condition::SettingBetween {
                        setting: SettingId::new(""),
                        min: Some(1),
                        max: None,
                    }),
                    Kind::SettingHas => Some(Condition::SettingHas {
                        setting: SettingId::new(""),
                        value: String::new(),
                    }),
                    Kind::Remove => None,
                };
                self.set_node(&path, node);
                self.modal = match kind {
                    Kind::HasTag => Some((self.tag_picker(), Pending::Tag(path))),
                    Kind::SettingOn => Some((self.setting_picker(), Pending::Setting(path))),
                    Kind::ResultIs | Kind::ValueIn => {
                        Some((self.subject_picker(kind), Pending::Subject(path, kind)))
                    }
                    Kind::SettingIs => Some((
                        Modal::pick(
                            "Which setting?",
                            self.choices
                                .iter()
                                .map(|s| PickItem::new(s.name.clone()))
                                .collect(),
                            0,
                        ),
                        Pending::Choice(path),
                    )),
                    Kind::SettingNumber | Kind::SettingHas | Kind::SettingText => Some((
                        Modal::pick(
                            "Which setting?",
                            self.others_of(kind)
                                .iter()
                                .map(|s| PickItem::new(s.name.clone()))
                                .collect(),
                            0,
                        ),
                        Pending::OtherSetting(path, kind),
                    )),
                    _ => None,
                };
            }
            (ModalResult::Picked(k), Pending::OtherSetting(path, kind)) => {
                let setting = SettingId::new(&self.others_of(kind)[k].id);
                self.modal = Some(match kind {
                    Kind::SettingNumber => {
                        self.set_node(
                            &path,
                            Some(Condition::SettingBetween {
                                setting: setting.clone(),
                                min: Some(1),
                                max: None,
                            }),
                        );
                        (
                            Modal::text(
                                "Number range",
                                "e.g. 2-4, 3+ (at least 3) or -5 (at most 5).",
                                "1+",
                            ),
                            Pending::SettingRange(path, setting),
                        )
                    }
                    Kind::SettingHas => {
                        let items = self.option_items(&setting);
                        (
                            Modal::pick("Which option?", items, 0),
                            Pending::SettingTick(path, setting),
                        )
                    }
                    _ => (
                        Modal::text("Text", "The exact text to match.", ""),
                        Pending::SettingText(path, setting),
                    ),
                });
            }
            (ModalResult::Text(text), Pending::SettingRange(path, setting)) => {
                if let Some((min, max)) = parse_range_text(&text) {
                    self.set_node(&path, Some(Condition::SettingBetween { setting, min, max }));
                }
            }
            (ModalResult::Picked(k), Pending::SettingTick(path, setting)) => {
                let value = self
                    .others
                    .iter()
                    .find(|s| s.id == setting.as_str())
                    .and_then(|s| s.options.get(k))
                    .map(|o| o.id.clone())
                    .unwrap_or_default();
                self.set_node(&path, Some(Condition::SettingHas { setting, value }));
            }
            (ModalResult::Text(value), Pending::SettingText(path, setting)) => {
                self.set_node(&path, Some(Condition::SettingIs { setting, value }));
            }
            (ModalResult::Picked(k), Pending::Tag(path)) => {
                let tag = self.tags[k].0.clone();
                self.set_node(&path, Some(Condition::HasTag(tag)));
            }
            (ModalResult::Picked(k), Pending::Setting(path)) => {
                let setting = self.settings[k].0.clone();
                self.set_node(&path, Some(Condition::SettingOn(setting)));
            }
            (ModalResult::Picked(k), Pending::Subject(path, kind)) => {
                let step = self.subjects_for(kind)[k].step.clone();
                if kind == Kind::ValueIn {
                    self.set_node(
                        &path,
                        Some(Condition::ValueIn {
                            step: step.clone(),
                            min: Some(1),
                            max: None,
                        }),
                    );
                    self.modal = Some((
                        Modal::text(
                            "Number range",
                            "e.g. 2-4, 3+ (at least 3) or -5 (at most 5).",
                            "1+",
                        ),
                        Pending::Range(path, step),
                    ));
                } else {
                    self.set_node(
                        &path,
                        Some(Condition::ResultIs {
                            step: step.clone(),
                            results: Vec::new(),
                        }),
                    );
                    self.modal = Some((
                        self.results_picker(&step, &[]),
                        Pending::Results(path, step),
                    ));
                }
            }
            (ModalResult::Checked(ticks), Pending::Results(path, step)) => {
                let options = self
                    .subject(&step)
                    .map(|s| s.results.clone())
                    .unwrap_or_default();
                let results = options
                    .iter()
                    .zip(ticks)
                    .filter(|(_, t)| *t)
                    .map(|(o, _)| ResultId::new(&o.id))
                    .collect();
                self.set_node(&path, Some(Condition::ResultIs { step, results }));
            }
            (ModalResult::Text(text), Pending::Range(path, step)) => {
                if let Some((min, max)) = parse_range_text(&text) {
                    self.set_node(&path, Some(Condition::ValueIn { step, min, max }));
                }
            }
            (ModalResult::Picked(k), Pending::Choice(path)) => {
                let setting = SettingId::new(&self.choices[k].id);
                let options = self.options_of(&setting);
                self.modal = Some((
                    Modal::pick("Which option?", options, 0),
                    Pending::ChoiceOption(path, setting),
                ));
            }
            (ModalResult::Picked(k), Pending::ChoiceOption(path, setting)) => {
                let value = self
                    .choices
                    .iter()
                    .find(|s| s.id == setting.as_str())
                    .and_then(|s| s.options.get(k))
                    .map(|o| o.id.clone())
                    .unwrap_or_default();
                self.set_node(&path, Some(Condition::SettingIs { setting, value }));
            }
            _ => {}
        }
    }

    pub fn render(&mut self, f: &mut Frame, area: Rect, ctx: &Ctx) {
        let s = &ctx.styles;
        let head_height = if self.sentence.is_some() { 5 } else { 3 };
        let [head, list] =
            Layout::vertical([Constraint::Length(head_height), Constraint::Fill(1)]).areas(area);
        let mut lines = vec![
            Line::styled(self.title.clone(), s.title()),
            Line::from(vec![
                Span::styled("Enter", s.key()),
                Span::styled(" edit  ", s.dim()),
                Span::styled("t", s.key()),
                Span::styled(" change type  ", s.dim()),
                Span::styled("a", s.key()),
                Span::styled(" add to all/any  ", s.dim()),
                Span::styled("d", s.key()),
                Span::styled(" remove  ", s.dim()),
                Span::styled("Esc", s.key()),
                Span::styled(" done", s.dim()),
            ]),
        ];
        if let Some(sentence) = &self.sentence {
            lines.push(Line::from(vec![
                Span::styled("WHEN ", s.key()),
                Span::raw(sentence.clone()),
            ]));
        }
        f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), head);
        let rows = self.rows(ctx);
        self.state.render(f, list, &rows, s, true);
        if let Some((modal, _)) = &mut self.modal {
            modal.render(f, area, s);
        }
    }
}

fn range_text(min: Option<i64>, max: Option<i64>) -> String {
    match (min, max) {
        (Some(a), Some(b)) => format!("{a}-{b}"),
        (Some(a), None) => format!("{a}+"),
        (None, Some(b)) => format!("-{b}"),
        (None, None) => String::new(),
    }
}

/// Reads "2-4", "3+", "-5" or "3".
fn parse_range_text(text: &str) -> Option<(Option<i64>, Option<i64>)> {
    let t = text.trim();
    if let Some(at_most) = t.strip_prefix('-') {
        return at_most.trim().parse().ok().map(|b| (None, Some(b)));
    }
    if let Some(at_least) = t.strip_suffix('+') {
        return at_least.trim().parse().ok().map(|a| (Some(a), None));
    }
    if let Some((a, b)) = t.split_once('-') {
        let (a, b) = (a.trim().parse().ok()?, b.trim().parse().ok()?);
        return (a <= b).then_some((Some(a), Some(b)));
    }
    t.parse().ok().map(|n| (Some(n), Some(n)))
}

/// One-line summary of a condition.
pub fn summary(c: &Option<Condition>, tags: &[(TagId, String)]) -> String {
    fn go(c: &Condition, tags: &[(TagId, String)]) -> String {
        match c {
            Condition::HasTag(t) => tags
                .iter()
                .find(|(id, _)| id == t)
                .map_or_else(|| t.to_string(), |(_, n)| n.clone()),
            Condition::All(v) => format!(
                "all of ({})",
                v.iter().map(|c| go(c, tags)).collect::<Vec<_>>().join(", ")
            ),
            Condition::Any(v) => format!(
                "any of ({})",
                v.iter().map(|c| go(c, tags)).collect::<Vec<_>>().join(", ")
            ),
            Condition::Not(b) => format!("not {}", go(b, tags)),
            Condition::SettingOn(id) => format!("setting {id} on"),
            other => other.describe_test().unwrap_or_default().to_lowercase(),
        }
    }
    c.as_ref().map_or_else(|| "always".into(), |c| go(c, tags))
}

#[cfg(test)]
mod tests {
    use super::parse_range_text;

    #[test]
    fn number_ranges_read_as_typed() {
        assert_eq!(parse_range_text("2-4"), Some((Some(2), Some(4))));
        assert_eq!(parse_range_text("3+"), Some((Some(3), None)));
        assert_eq!(parse_range_text("-5"), Some((None, Some(5))));
        assert_eq!(parse_range_text("3"), Some((Some(3), Some(3))));
        assert_eq!(parse_range_text("4-2"), None);
        assert_eq!(parse_range_text("x"), None);
    }
}
