//! Editing operations on a flow, shared by both flow editors. A flow is a
//! tree: each step sits in a list (the top level, a repeat's body, a case
//! of a branch, …). The GUI's node graph and the terminal's tree both turn
//! the user's actions into these operations, so the rules live here once.

use serde::{Deserialize, Serialize};

use crate::content::{Condition, Modifier};
use crate::ids::{FlowId, ObjectTypeId, SettingId, StepId, TableId, TagId};

use super::{BranchCase, Count, Flow, FlowStep, StepAction};

/// A list of steps in a flow.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "list", rename_all = "snake_case")]
pub enum Slot {
    /// The flow's own steps.
    Top,
    /// The steps inside a repeat, For Each or Create Object.
    Body { step: StepId },
    /// The steps of one case of a branch.
    Case { step: StepId, case: usize },
    /// A branch's Otherwise steps.
    Otherwise { step: StepId },
}

/// A place in a list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Position {
    #[serde(flatten)]
    pub slot: Slot,
    /// Index in the list; past the end appends.
    pub index: usize,
}

/// The kinds of step the editors offer, as the palette names them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StepKind {
    RollTable,
    RunSubflow,
    Condition,
    CreateObject,
    RepeatTimes,
    RepeatForEach,
    SetValue,
    AddTag,
    RemoveTag,
    ApplyModifier,
    ReadSetting,
    Stop,
}

impl StepKind {
    pub const ALL: [StepKind; 12] = [
        StepKind::RollTable,
        StepKind::RunSubflow,
        StepKind::Condition,
        StepKind::CreateObject,
        StepKind::RepeatTimes,
        StepKind::RepeatForEach,
        StepKind::SetValue,
        StepKind::AddTag,
        StepKind::RemoveTag,
        StepKind::ApplyModifier,
        StepKind::ReadSetting,
        StepKind::Stop,
    ];

    pub fn label(self) -> &'static str {
        match self {
            StepKind::RollTable => "Roll Table",
            StepKind::RunSubflow => "Run Subflow",
            StepKind::Condition => "If / Condition",
            StepKind::CreateObject => "Create Object",
            StepKind::RepeatTimes => "Repeat N Times",
            StepKind::RepeatForEach => "Repeat For Each",
            StepKind::SetValue => "Set Value",
            StepKind::AddTag => "Add Tag",
            StepKind::RemoveTag => "Remove Tag",
            StepKind::ApplyModifier => "Apply Modifier",
            StepKind::ReadSetting => "Read Setting",
            StepKind::Stop => "Stop / End",
        }
    }

    fn id_base(self) -> &'static str {
        match self {
            StepKind::RollTable => "roll",
            StepKind::RunSubflow => "subflow",
            StepKind::Condition => "if",
            StepKind::CreateObject => "object",
            StepKind::RepeatTimes => "repeat",
            StepKind::RepeatForEach => "each",
            StepKind::SetValue => "value",
            StepKind::AddTag => "add-tag",
            StepKind::RemoveTag => "remove-tag",
            StepKind::ApplyModifier => "modifier",
            StepKind::ReadSetting => "setting",
            StepKind::Stop => "stop",
        }
    }

    /// The kind of an existing step, if the palette offers it.
    pub fn of(action: &StepAction) -> Option<StepKind> {
        Some(match action {
            StepAction::Roll { .. } => StepKind::RollTable,
            StepAction::RunFlow { .. } => StepKind::RunSubflow,
            StepAction::Branch { .. } => StepKind::Condition,
            StepAction::CreateObject { .. } => StepKind::CreateObject,
            StepAction::Repeat { .. } => StepKind::RepeatTimes,
            StepAction::ForEach { .. } => StepKind::RepeatForEach,
            StepAction::SetValue { .. } => StepKind::SetValue,
            StepAction::AddTag { .. } => StepKind::AddTag,
            StepAction::RemoveTag { .. } => StepKind::RemoveTag,
            StepAction::ApplyModifier { .. } => StepKind::ApplyModifier,
            StepAction::ReadSetting { .. } => StepKind::ReadSetting,
            StepAction::Stop => StepKind::Stop,
            _ => return None,
        })
    }
}

/// One change to a flow.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum FlowEdit {
    /// Add a new step of `kind` at `at`.
    Add {
        kind: StepKind,
        at: Position,
    },
    /// Put a step at `at` (e.g. pasted, or restored).
    Insert {
        step: FlowStep,
        at: Position,
    },
    /// Replace a step's settings (its children are kept unless the new
    /// value has its own).
    Update {
        step: FlowStep,
    },
    /// Delete a step and everything inside it.
    Remove {
        step: StepId,
    },
    /// Move a step (and what is inside it) to `to`.
    Move {
        step: StepId,
        to: Position,
    },
    /// Take a step out of the flow without deleting it.
    Detach {
        step: StepId,
    },
    /// Connect a detached step at `to`.
    Attach {
        step: StepId,
        to: Position,
    },
    /// Copy a step (and what is inside it) just after itself.
    Duplicate {
        step: StepId,
    },
    /// Add a case to a branch.
    AddCase {
        step: StepId,
    },
    RemoveCase {
        step: StepId,
        case: usize,
    },
    /// Remember where the editor placed a node.
    Place {
        step: StepId,
        at: [f64; 2],
    },
}

/// Every step ID in the flow, detached ones included.
fn all_ids(flow: &Flow) -> Vec<StepId> {
    let mut out: Vec<StepId> = flow.all_steps().iter().map(|s| s.id.clone()).collect();
    for step in &flow.editor.detached {
        collect_ids(step, &mut out);
    }
    out
}

fn collect_ids(step: &FlowStep, out: &mut Vec<StepId>) {
    out.push(step.id.clone());
    for child in step.children() {
        collect_ids(child, out);
    }
}

/// A step ID starting with `base` that `flow` does not use yet.
pub fn fresh_id(flow: &Flow, base: &str, taken: &[StepId]) -> StepId {
    let used = all_ids(flow);
    let free = |id: &str| !used.iter().chain(taken).any(|u| u.as_str() == id);
    if free(base) {
        return StepId::new(base);
    }
    (2..)
        .map(|n| format!("{base}-{n}"))
        .find(|id| free(id))
        .map(StepId::new)
        .expect("an unused ID")
}

/// A new step of `kind` with sensible blanks, for the user to fill in.
pub fn new_step(flow: &Flow, kind: StepKind) -> FlowStep {
    let action = match kind {
        StepKind::RollTable => StepAction::Roll {
            table: TableId::default(),
            field: None,
            modifiers: Vec::new(),
        },
        StepKind::RunSubflow => StepAction::RunFlow {
            flow: FlowId::default(),
            max_depth: None,
        },
        StepKind::Condition => StepAction::Branch {
            cases: vec![BranchCase {
                name: None,
                when: Condition::All(Vec::new()),
                steps: Vec::new(),
            }],
            otherwise: Vec::new(),
        },
        StepKind::CreateObject => StepAction::CreateObject {
            object: ObjectTypeId::default(),
            steps: Vec::new(),
            share_tags: false,
        },
        StepKind::RepeatTimes => StepAction::Repeat {
            count: Count::Times(1),
            steps: Vec::new(),
        },
        StepKind::RepeatForEach => StepAction::ForEach {
            object: ObjectTypeId::default(),
            pairs: false,
            steps: Vec::new(),
        },
        StepKind::SetValue => StepAction::SetValue {
            field: String::new(),
            value: 1,
        },
        StepKind::AddTag => StepAction::AddTag {
            tag: TagId::default(),
        },
        StepKind::RemoveTag => StepAction::RemoveTag {
            tag: TagId::default(),
        },
        StepKind::ApplyModifier => StepAction::ApplyModifier {
            table: TableId::default(),
            modifier: Modifier::Shift(1),
        },
        StepKind::ReadSetting => StepAction::ReadSetting {
            setting: SettingId::default(),
        },
        StepKind::Stop => StepAction::Stop,
    };
    FlowStep {
        id: fresh_id(flow, kind.id_base(), &[]),
        name: None,
        description: String::new(),
        when: None,
        active: true,
        status: Default::default(),
        note: String::new(),
        sub_stage: false,
        action,
    }
}

fn children_mut<'a>(step: &'a mut FlowStep, slot: &Slot) -> Option<&'a mut Vec<FlowStep>> {
    match (&mut step.action, slot) {
        (
            StepAction::Repeat { steps, .. }
            | StepAction::ForEach { steps, .. }
            | StepAction::CreateObject { steps, .. },
            Slot::Body { .. },
        ) => Some(steps),
        (StepAction::Branch { cases, .. }, Slot::Case { case, .. }) => {
            cases.get_mut(*case).map(|c| &mut c.steps)
        }
        (StepAction::Branch { otherwise, .. }, Slot::Otherwise { .. }) => Some(otherwise),
        _ => None,
    }
}

fn find_mut<'a>(steps: &'a mut [FlowStep], id: &StepId) -> Option<&'a mut FlowStep> {
    for step in steps {
        if &step.id == id {
            return Some(step);
        }
        let found = match &mut step.action {
            StepAction::Repeat { steps, .. }
            | StepAction::ForEach { steps, .. }
            | StepAction::CreateObject { steps, .. } => find_mut(steps, id),
            StepAction::Branch { cases, otherwise } => {
                let mut hit = None;
                for case in cases.iter_mut() {
                    if let Some(s) = find_mut(&mut case.steps, id) {
                        hit = Some(s);
                        break;
                    }
                }
                match hit {
                    Some(s) => Some(s),
                    None => find_mut(otherwise, id),
                }
            }
            _ => None,
        };
        if found.is_some() {
            return found;
        }
    }
    None
}

fn list_mut<'a>(flow: &'a mut Flow, slot: &Slot) -> Result<&'a mut Vec<FlowStep>, String> {
    let parent = match slot {
        Slot::Top => return Ok(&mut flow.steps),
        Slot::Body { step } | Slot::Case { step, .. } | Slot::Otherwise { step } => step,
    };
    let step = find_mut(&mut flow.steps, parent)
        .ok_or_else(|| format!("There is no step `{parent}` in this flow."))?;
    let kind = StepKind::of(&step.action).map_or("This step", StepKind::label);
    children_mut(step, slot).ok_or_else(|| format!("{kind} cannot hold steps there."))
}

/// Removes step `id` (and what is inside it) from wherever it is.
fn take(steps: &mut Vec<FlowStep>, id: &StepId) -> Option<FlowStep> {
    if let Some(i) = steps.iter().position(|s| &s.id == id) {
        return Some(steps.remove(i));
    }
    for step in steps.iter_mut() {
        let lists: Vec<&mut Vec<FlowStep>> = match &mut step.action {
            StepAction::Repeat { steps, .. }
            | StepAction::ForEach { steps, .. }
            | StepAction::CreateObject { steps, .. } => vec![steps],
            StepAction::Branch { cases, otherwise } => cases
                .iter_mut()
                .map(|c| &mut c.steps)
                .chain(std::iter::once(otherwise))
                .collect(),
            _ => Vec::new(),
        };
        for list in lists {
            if let Some(found) = take(list, id) {
                return Some(found);
            }
        }
    }
    None
}

fn insert(flow: &mut Flow, step: FlowStep, at: &Position) -> Result<(), String> {
    // A step cannot go inside itself.
    if let Slot::Body { step: parent }
    | Slot::Case { step: parent, .. }
    | Slot::Otherwise { step: parent } = &at.slot
    {
        let mut inside = Vec::new();
        collect_ids(&step, &mut inside);
        if inside.contains(parent) {
            return Err("A step cannot be placed inside itself.".into());
        }
    }
    let list = list_mut(flow, &at.slot)?;
    let index = at.index.min(list.len());
    list.insert(index, step);
    Ok(())
}

/// A copy of `step` (and what is inside it) with fresh IDs.
fn copy_with_fresh_ids(flow: &Flow, step: &FlowStep, taken: &mut Vec<StepId>) -> FlowStep {
    let mut copy = step.clone();
    let base = step
        .id
        .as_str()
        .trim_end_matches(|c: char| c.is_ascii_digit() || c == '-');
    let base = if base.is_empty() { "step" } else { base };
    copy.id = fresh_id(flow, base, taken);
    taken.push(copy.id.clone());
    let renew = |list: &mut Vec<FlowStep>, taken: &mut Vec<StepId>| {
        for child in list.iter_mut() {
            *child = copy_with_fresh_ids(flow, child, taken);
        }
    };
    match &mut copy.action {
        StepAction::Repeat { steps, .. }
        | StepAction::ForEach { steps, .. }
        | StepAction::CreateObject { steps, .. } => renew(steps, taken),
        StepAction::Branch { cases, otherwise } => {
            for case in cases.iter_mut() {
                renew(&mut case.steps, taken);
            }
            renew(otherwise, taken);
        }
        _ => {}
    }
    copy
}

/// Where step `id` is: its list and index.
pub fn position_of(flow: &Flow, id: &StepId) -> Option<Position> {
    fn search(steps: &[FlowStep], slot: Slot, id: &StepId) -> Option<Position> {
        for (index, step) in steps.iter().enumerate() {
            if &step.id == id {
                return Some(Position {
                    slot: slot.clone(),
                    index,
                });
            }
            let found = match &step.action {
                StepAction::Repeat { steps, .. }
                | StepAction::ForEach { steps, .. }
                | StepAction::CreateObject { steps, .. } => search(
                    steps,
                    Slot::Body {
                        step: step.id.clone(),
                    },
                    id,
                ),
                StepAction::Branch { cases, otherwise } => cases
                    .iter()
                    .enumerate()
                    .find_map(|(case, c)| {
                        search(
                            &c.steps,
                            Slot::Case {
                                step: step.id.clone(),
                                case,
                            },
                            id,
                        )
                    })
                    .or_else(|| {
                        search(
                            otherwise,
                            Slot::Otherwise {
                                step: step.id.clone(),
                            },
                            id,
                        )
                    }),
                _ => None,
            };
            if found.is_some() {
                return found;
            }
        }
        None
    }
    search(&flow.steps, Slot::Top, id)
}

/// `flow` with `edit` applied. Fails, changing nothing, when the edit does
/// not fit the flow (e.g. a step that does not exist).
pub fn apply(flow: &Flow, edit: FlowEdit) -> Result<Flow, String> {
    let mut out = flow.clone();
    let missing = |id: &StepId| format!("There is no step `{id}` in this flow.");
    match edit {
        FlowEdit::Add { kind, at } => {
            let step = new_step(&out, kind);
            insert(&mut out, step, &at)?;
        }
        FlowEdit::Insert { step, at } => {
            let mut inside = Vec::new();
            collect_ids(&step, &mut inside);
            if inside.iter().any(|id| all_ids(&out).contains(id)) {
                return Err("Those steps' IDs are already used in this flow.".into());
            }
            insert(&mut out, step, &at)?;
        }
        FlowEdit::Update { step } => {
            let target = find_mut(&mut out.steps, &step.id)
                .or_else(|| out.editor.detached.iter_mut().find(|s| s.id == step.id))
                .ok_or_else(|| missing(&step.id))?;
            let mut next = step;
            // Keep the children unless the new value brings its own.
            if next.children().is_empty() {
                match (&mut next.action, &target.action) {
                    (
                        StepAction::Repeat { steps: new, .. }
                        | StepAction::ForEach { steps: new, .. }
                        | StepAction::CreateObject { steps: new, .. },
                        StepAction::Repeat { steps: old, .. }
                        | StepAction::ForEach { steps: old, .. }
                        | StepAction::CreateObject { steps: old, .. },
                    ) => *new = old.clone(),
                    (
                        StepAction::Branch {
                            cases: new,
                            otherwise: new_otherwise,
                        },
                        StepAction::Branch {
                            cases: old,
                            otherwise: old_otherwise,
                        },
                    ) => {
                        for (n, o) in new.iter_mut().zip(old) {
                            if n.steps.is_empty() {
                                n.steps = o.steps.clone();
                            }
                        }
                        if new_otherwise.is_empty() {
                            *new_otherwise = old_otherwise.clone();
                        }
                    }
                    _ => {}
                }
            }
            *target = next;
        }
        FlowEdit::Remove { step } => {
            let removed = take(&mut out.steps, &step).or_else(|| {
                let i = out.editor.detached.iter().position(|s| s.id == step)?;
                Some(out.editor.detached.remove(i))
            });
            let removed = removed.ok_or_else(|| missing(&step))?;
            let mut gone = Vec::new();
            collect_ids(&removed, &mut gone);
            out.editor.positions.retain(|id, _| !gone.contains(id));
        }
        FlowEdit::Move { step, to } => {
            let moved = take(&mut out.steps, &step).ok_or_else(|| missing(&step))?;
            insert(&mut out, moved, &to)?;
        }
        FlowEdit::Detach { step } => {
            let moved = take(&mut out.steps, &step).ok_or_else(|| missing(&step))?;
            out.editor.detached.push(moved);
        }
        FlowEdit::Attach { step, to } => {
            let i = out
                .editor
                .detached
                .iter()
                .position(|s| s.id == step)
                .ok_or_else(|| format!("Step `{step}` is not a detached step."))?;
            let moved = out.editor.detached.remove(i);
            insert(&mut out, moved, &to)?;
        }
        FlowEdit::Duplicate { step } => {
            let original = flow
                .all_steps()
                .into_iter()
                .find(|s| s.id == step)
                .cloned()
                .ok_or_else(|| missing(&step))?;
            let copy = copy_with_fresh_ids(&out, &original, &mut Vec::new());
            let at = position_of(&out, &step).ok_or_else(|| missing(&step))?;
            insert(
                &mut out,
                copy,
                &Position {
                    index: at.index + 1,
                    ..at
                },
            )?;
        }
        FlowEdit::AddCase { step } => {
            let target = find_mut(&mut out.steps, &step).ok_or_else(|| missing(&step))?;
            let StepAction::Branch { cases, .. } = &mut target.action else {
                return Err("Only an If / Condition step has cases.".into());
            };
            cases.push(BranchCase {
                name: None,
                when: Condition::All(Vec::new()),
                steps: Vec::new(),
            });
        }
        FlowEdit::RemoveCase { step, case } => {
            let target = find_mut(&mut out.steps, &step).ok_or_else(|| missing(&step))?;
            let StepAction::Branch { cases, .. } = &mut target.action else {
                return Err("Only an If / Condition step has cases.".into());
            };
            if case >= cases.len() {
                return Err(format!("There is no case {}.", case + 1));
            }
            if cases.len() == 1 {
                return Err("An If / Condition step needs at least one case.".into());
            }
            cases.remove(case);
        }
        FlowEdit::Place { step, at } => {
            out.editor.positions.insert(step, at);
        }
    }
    Ok(out)
}

/// A copy of `flow` under a new ID and name, with every step kept.
pub fn duplicate_flow(flow: &Flow, id: FlowId) -> Flow {
    Flow {
        id,
        name: format!("Copy of {}", flow.name),
        ..flow.clone()
    }
}
