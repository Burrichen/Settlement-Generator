//! Runs flows: the interpreter behind every flow step. Table rolls go
//! through the same [`Generator`] as every stage, so redirects, variants,
//! eligibility, modifiers, God Mode and follow-up rolls all apply, and every
//! die is recorded.

use std::collections::HashMap;

use crate::content::{ContentSet, Facts, Modifier};
use crate::dice::{DiceExpr, FaceSource};
use crate::engine::{
    GenerationError, GenerationEvent, Generator, Named, Request, Selections, State, setting_value,
};
use crate::ids::{FlowId, ObjectTypeId, ResultId, SettingId, StepId, TableId, TagId};
use crate::naming::{GeneratedName, generate_name, scheme_for_subject};

use super::{
    Count, FieldValue, FlowStep, ForEachIteration, MAX_FLOW_DEPTH, MAX_FLOW_STEPS, MAX_REPEAT,
    ObjectRecord, SettingValue, StepAction, StepOutcome, StepRecord, StepSkip,
};

/// What a step stored for later steps: its result and/or number.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Stored {
    pub result: Option<ResultId>,
    pub value: Option<i64>,
}

/// Results and values stored by step ID, innermost scope last, plus what
/// lasts for the rest of the generation: the objects created so far and the
/// modifiers applied to later rolls.
#[derive(Debug, Clone, Default)]
pub(crate) struct Memory {
    scopes: Vec<HashMap<StepId, Stored>>,
    created: Vec<Created>,
    applied: Vec<Applied>,
}

/// What an earlier stage left for later ones; a change invalidates them.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Carried {
    stored: Vec<(StepId, Stored)>,
    created: Vec<Created>,
    applied: Vec<Applied>,
}

/// A modifier an Apply Modifier step put on every later roll of a table.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Applied {
    step: StepId,
    table: TableId,
    modifier: Modifier,
}

impl Memory {
    /// Memory seeded with what earlier stages stored.
    pub fn with(stored: impl IntoIterator<Item = (StepId, Stored)>) -> Self {
        Memory {
            scopes: vec![stored.into_iter().collect()],
            ..Default::default()
        }
    }

    /// Adds the objects an earlier stage created and the modifiers it
    /// applied, read from its record.
    pub fn carry(&mut self, record: &StepRecord) {
        for child in record.children() {
            self.carry(child);
        }
        match &record.outcome {
            StepOutcome::Object { object } => self.created.push(Created {
                object_type: object.object_type.id.clone(),
                named: Named::new(&object.id, &object.label),
                tags: object.tags.iter().map(|t| t.id.clone()).collect(),
            }),
            StepOutcome::ApplyModifier { table, modifier } => self.applied.push(Applied {
                step: record.step.clone(),
                table: table.id.clone(),
                modifier: *modifier,
            }),
            _ => {}
        }
    }

    pub fn get(&self, step: &StepId) -> Option<&Stored> {
        self.scopes.iter().rev().find_map(|s| s.get(step))
    }

    fn set(&mut self, step: &StepId, stored: Stored) {
        if self.scopes.is_empty() {
            self.scopes.push(HashMap::new());
        }
        self.scopes
            .last_mut()
            .expect("a scope")
            .insert(step.clone(), stored);
    }

    /// Everything stored in the outermost scope.
    /// Modifiers earlier Apply Modifier steps put on rolls of `table`.
    pub fn applied_to(&self, table: &TableId) -> Vec<(StepId, Modifier)> {
        self.applied
            .iter()
            .filter(|a| &a.table == table)
            .map(|a| (a.step.clone(), a.modifier))
            .collect()
    }

    pub fn outer(&self) -> Carried {
        let mut stored: Vec<_> = self
            .scopes
            .first()
            .map(|s| s.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
            .unwrap_or_default();
        stored.sort_by(|a, b| a.0.cmp(&b.0));
        Carried {
            stored,
            created: self.created.clone(),
            applied: self.applied.clone(),
        }
    }
}

/// What a recorded step (and the steps inside it, outside any object)
/// stored for later steps, rebuilt from the record alone.
pub(crate) fn stored_values(record: &StepRecord) -> Vec<(StepId, Stored)> {
    let mut out = Vec::new();
    collect_stored(record, &mut out);
    out
}

fn collect_stored(record: &StepRecord, out: &mut Vec<(StepId, Stored)>) {
    let stored = match &record.outcome {
        StepOutcome::Roll {
            events, fallback, ..
        } => events.iter().find_map(|e| match e {
            GenerationEvent::TableRolled(r) if !r.is_routed() => Some(match fallback {
                Some(f) => Stored {
                    result: Some(f.id.clone()),
                    value: None,
                },
                None => Stored {
                    result: Some(r.result.id.clone()),
                    value: r.value,
                },
            }),
            _ => None,
        }),
        StepOutcome::SecondaryRoll { roll, .. } => Some(Stored {
            result: None,
            value: Some(i64::from(roll.total)),
        }),
        StepOutcome::SetResult { result, value, .. } => Some(Stored {
            result: Some(result.id.clone()),
            value: *value,
        }),
        StepOutcome::SetValue { value, .. } => Some(Stored {
            result: None,
            value: Some(*value),
        }),
        StepOutcome::ReadSetting { value, .. } => Some(read_setting(value)),
        _ => None,
    };
    if let Some(stored) = stored {
        out.retain(|(id, _)| id != &record.step);
        out.push((record.step.clone(), stored));
    }
    // An object's steps store in the object's own scope.
    if !matches!(record.outcome, StepOutcome::Object { .. }) {
        for child in record.children() {
            collect_stored(child, out);
        }
    }
}

/// What Read Setting stores: a dropdown option (or text) as its result, a
/// toggle as 1 or 0, a number as itself, a multi-select as how many are
/// ticked.
fn read_setting(value: &SettingValue) -> Stored {
    match value {
        SettingValue::Choice(c) => Stored {
            result: Some(ResultId::new(c.as_str())),
            value: None,
        },
        SettingValue::Toggle(on) => Stored {
            result: None,
            value: Some(i64::from(*on)),
        },
        SettingValue::Number(n) => Stored {
            result: None,
            value: Some(*n),
        },
        SettingValue::Many(v) => Stored {
            result: None,
            value: Some(v.len() as i64),
        },
    }
}

/// An object created in this generation, for For Each.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Created {
    object_type: ObjectTypeId,
    named: Named<String>,
    tags: Vec<TagId>,
}

/// What conditions inside a flow see: the active tags, the settings, and
/// what earlier steps stored.
pub(crate) struct FlowFacts<'a> {
    pub generator: &'a Generator<'a>,
    pub state: &'a State,
    pub memory: &'a Memory,
}

impl Facts for FlowFacts<'_> {
    fn has_tag(&self, tag: &TagId) -> bool {
        self.state.has(tag)
    }

    fn setting(&self, id: &SettingId) -> Option<SettingValue> {
        setting_value(
            self.generator.content(),
            self.generator.options(),
            self.generator.settings(),
            id,
        )
    }

    fn result_of(&self, step: &StepId) -> Option<ResultId> {
        self.memory.get(step)?.result.clone()
    }

    fn value_of(&self, step: &StepId) -> Option<i64> {
        self.memory.get(step)?.value
    }
}

/// An object being built.
struct Building {
    fields: Vec<FieldValue>,
    named: Option<String>,
}

/// Runs flow steps with shared content and physical dice.
pub(crate) struct FlowRunner<'g, 'c> {
    pub generator: &'g Generator<'c>,
    pub selections: &'g Selections,
    pub memory: Memory,
    /// Unique results already given in this run (see
    /// [`Table::fallback_for`](crate::content::Table::fallback_for)).
    held: Vec<ResultId>,
    /// Objects created so far, per type, for numbering.
    counts: HashMap<ObjectTypeId, usize>,
    objects: Vec<Building>,
    steps: usize,
    depth: usize,
    /// The flows being run, outermost first (recursion protection).
    running: Vec<crate::ids::FlowId>,
    /// A Stop step ended the current flow.
    stopped: bool,
}

impl<'g, 'c> FlowRunner<'g, 'c> {
    pub fn new(generator: &'g Generator<'c>, selections: &'g Selections, memory: Memory) -> Self {
        // Objects made by earlier stages keep their numbers.
        let mut counts: HashMap<ObjectTypeId, usize> = HashMap::new();
        for made in &memory.created {
            *counts.entry(made.object_type.clone()).or_default() += 1;
        }
        FlowRunner {
            generator,
            selections,
            memory,
            held: Vec::new(),
            counts,
            objects: Vec::new(),
            steps: 0,
            depth: 0,
            running: Vec::new(),
            stopped: false,
        }
    }

    fn content(&self) -> &'c ContentSet {
        self.generator.content()
    }

    fn holds(&self, condition: &crate::content::Condition, state: &State) -> bool {
        condition.evaluate_with(&FlowFacts {
            generator: self.generator,
            state,
            memory: &self.memory,
        })
    }

    /// Runs `steps` in order. Inactive and planned steps take no part.
    pub fn run_steps(
        &mut self,
        steps: &[FlowStep],
        state: &mut State,
        faces: &mut dyn FaceSource,
    ) -> Result<Vec<StepRecord>, GenerationError> {
        let mut out = Vec::new();
        for step in steps.iter().filter(|s| s.runs()) {
            if self.stopped {
                break;
            }
            // Containers run their own steps (and triggers) inside.
            let leaf =
                step.children().is_empty() && !matches!(step.action, StepAction::RunFlow { .. });
            let before: Vec<TagId> = state.active().to_vec();
            out.push(self.run_step(step, state, faces)?);
            if leaf {
                self.run_triggers(step, &before, state, faces, &mut out)?;
            }
        }
        Ok(out)
    }

    /// Runs the subflows of Run Subflow effects on tags `step` just added.
    /// Each runs once per tag; a flow already running (through a trigger or
    /// otherwise) is not started again inside itself.
    fn run_triggers(
        &mut self,
        step: &FlowStep,
        before: &[TagId],
        state: &mut State,
        faces: &mut dyn FaceSource,
        out: &mut Vec<StepRecord>,
    ) -> Result<(), GenerationError> {
        let added: Vec<TagId> = state
            .active()
            .iter()
            .filter(|t| !before.contains(t))
            .cloned()
            .collect();
        for tag_id in added {
            let Some(tag) = self.content().tag(&tag_id).filter(|t| t.enabled) else {
                continue;
            };
            let flows: Vec<FlowId> = tag
                .effects
                .iter()
                .filter(|d| d.is_active())
                .filter_map(|d| match &d.effect {
                    crate::content::Effect::RunFlow { flow } => Some(flow.clone()),
                    _ => None,
                })
                .collect();
            for flow in flows {
                if self.stopped {
                    return Ok(());
                }
                let name = self
                    .content()
                    .flow(&flow)
                    .map_or_else(|| flow.to_string(), |f| f.name.clone());
                let trigger = FlowStep {
                    id: StepId::new(format!("{}~{}", step.id, tag.id)),
                    name: Some(format!("{name} (triggered by {})", tag.name)),
                    description: String::new(),
                    when: None,
                    active: true,
                    status: Default::default(),
                    note: String::new(),
                    sub_stage: false,
                    action: StepAction::RunFlow {
                        flow,
                        max_depth: None,
                    },
                };
                out.push(self.run_step(&trigger, state, faces)?);
            }
        }
        Ok(())
    }

    /// Runs one step (whose `when` is checked here).
    pub fn run_step(
        &mut self,
        step: &FlowStep,
        state: &mut State,
        faces: &mut dyn FaceSource,
    ) -> Result<StepRecord, GenerationError> {
        self.steps += 1;
        if self.steps > MAX_FLOW_STEPS {
            return Err(GenerationError::FlowStepLimit);
        }
        let label = self.label(step);
        if step.when.as_ref().is_some_and(|c| !self.holds(c, state)) {
            return Ok(StepRecord {
                step: step.id.clone(),
                label,
                outcome: StepOutcome::Skipped {
                    reason: StepSkip::ConditionFalse,
                },
            });
        }
        let outcome = self.outcome(step, state, faces)?;
        let label = match &outcome {
            StepOutcome::Object { object } => object.label.clone(),
            _ => label,
        };
        Ok(StepRecord {
            step: step.id.clone(),
            label,
            outcome,
        })
    }

    /// The label a step is shown with: its name, else what it rolls or
    /// does.
    pub fn label(&self, step: &FlowStep) -> String {
        if let Some(name) = &step.name {
            return name.clone();
        }
        let content = self.content();
        match &step.action {
            StepAction::Roll { table, .. } | StepAction::SetResult { table, .. } => content
                .table(table)
                .map_or_else(|| table.to_string(), |t| t.name.clone()),
            StepAction::SecondaryRoll { label, .. } => label.clone(),
            StepAction::AddTag { tag } => content
                .tag(tag)
                .map_or_else(|| tag.to_string(), |t| t.name.clone()),
            StepAction::SetValue { field, .. } => field.clone(),
            StepAction::Branch { .. } => "Branch".to_owned(),
            StepAction::Repeat { .. } => "Repeat".to_owned(),
            StepAction::RunFlow { flow, .. } => content
                .flow(flow)
                .map_or_else(|| flow.to_string(), |f| f.name.clone()),
            StepAction::ForEach { object, pairs, .. } => {
                let name = content
                    .object_type(object)
                    .map_or_else(|| object.to_string(), |o| o.name.clone());
                if *pairs {
                    format!("Each pair of {name}")
                } else {
                    format!("Each {name}")
                }
            }
            StepAction::RemoveTag { tag } => content
                .tag(tag)
                .map_or_else(|| tag.to_string(), |t| t.name.clone()),
            StepAction::ApplyModifier { table, modifier } => format!(
                "{modifier} on {}",
                content
                    .table(table)
                    .map_or_else(|| table.to_string(), |t| t.name.clone())
            ),
            StepAction::ReadSetting { setting } => content
                .setting(setting)
                .map_or_else(|| setting.to_string(), |s| s.name.clone()),
            StepAction::Stop => "Stop".to_owned(),
            StepAction::CreateObject { object, .. } => content
                .object_type(object)
                .map_or_else(|| object.to_string(), |o| o.name.clone()),
            StepAction::Name { .. } => "Name".to_owned(),
            StepAction::Builtin { handler, .. } => handler.clone(),
        }
    }

    fn outcome(
        &mut self,
        step: &FlowStep,
        state: &mut State,
        faces: &mut dyn FaceSource,
    ) -> Result<StepOutcome, GenerationError> {
        Ok(match &step.action {
            StepAction::Roll {
                table,
                field,
                modifiers: step_modifiers,
            } => {
                let mut modifiers = self.memory.applied_to(table);
                modifiers.extend(
                    step_modifiers
                        .iter()
                        .filter(|m| m.when.as_ref().is_none_or(|c| self.holds(c, state)))
                        .map(|m| (step.id.clone(), m.modifier)),
                );
                let request = Request::new(table.clone())
                    .with_choice(self.selections.get(table).cloned())
                    .with_modifiers(modifiers);
                self.generator
                    .process(state, [request].into(), self.selections, faces)?;
                let mut events = state.take_events();
                let main = events.iter().find_map(|e| match e {
                    GenerationEvent::TableRolled(r) if !r.is_routed() => Some((**r).clone()),
                    _ => None,
                });
                let Some(record) = main else {
                    let reason = events.iter().find_map(|e| match e {
                        GenerationEvent::TableSkipped { reason, .. } => Some(*reason),
                        _ => None,
                    });
                    return Ok(StepOutcome::Skipped {
                        reason: StepSkip::Table(
                            reason.unwrap_or(crate::engine::SkipReason::NotEligible),
                        ),
                    });
                };
                let used = self
                    .content()
                    .table(&record.table.id)
                    .ok_or_else(|| GenerationError::UnknownTable(record.table.id.clone()))?;
                let rolled = used
                    .result(&record.result.id)
                    .ok_or_else(|| GenerationError::UnknownTable(record.table.id.clone()))?;
                let mut fallback = None;
                let given = match used.fallback_for(rolled, &self.held) {
                    Some(given) => {
                        fallback = Some(Named::new(&given.id, &given.name));
                        self.generator.emit_and_follow(
                            state,
                            &given.tags,
                            self.selections,
                            faces,
                        )?;
                        events.extend(state.take_events());
                        given
                    }
                    None => rolled,
                };
                self.held.push(given.id.clone());
                // As `stored_values` reads it back: a fallback keeps no value.
                self.memory.set(
                    &step.id,
                    Stored {
                        result: Some(given.id.clone()),
                        value: if fallback.is_some() {
                            None
                        } else {
                            record.value
                        },
                    },
                );
                if let Some(field) = field {
                    self.set_field(
                        &step.id,
                        field,
                        Some(Named::new(&given.id, &given.name)),
                        given.value,
                    )?;
                }
                StepOutcome::Roll {
                    events,
                    field: field.clone(),
                    fallback,
                }
            }
            StepAction::SecondaryRoll { label, dice } => {
                let roll = DiceExpr::parse(dice)
                    .map_err(|error| GenerationError::InvalidDice {
                        expression: dice.clone(),
                        error,
                    })?
                    .roll(faces)?;
                self.memory.set(
                    &step.id,
                    Stored {
                        result: None,
                        value: Some(i64::from(roll.total)),
                    },
                );
                StepOutcome::SecondaryRoll {
                    label: label.clone(),
                    roll,
                }
            }
            StepAction::AddTag { tag } => {
                self.generator.emit_and_follow(
                    state,
                    std::slice::from_ref(tag),
                    self.selections,
                    faces,
                )?;
                let name = self
                    .content()
                    .tag(tag)
                    .map_or_else(|| tag.to_string(), |t| t.name.clone());
                StepOutcome::AddTag {
                    tag: Named::new(tag, &name),
                    events: state.take_events(),
                }
            }
            StepAction::SetResult {
                table,
                result,
                field,
            } => {
                let def = self
                    .content()
                    .table(table)
                    .ok_or_else(|| GenerationError::UnknownTable(table.clone()))?;
                let given =
                    def.result(result)
                        .ok_or_else(|| GenerationError::UnknownSelection {
                            table: table.clone(),
                            result: result.clone(),
                        })?;
                self.generator
                    .emit_and_follow(state, &given.tags, self.selections, faces)?;
                let events = state.take_events();
                self.memory.set(
                    &step.id,
                    Stored {
                        result: Some(given.id.clone()),
                        value: given.value,
                    },
                );
                let named = Named::new(&given.id, &given.name);
                if let Some(field) = field {
                    self.set_field(&step.id, field, Some(named.clone()), given.value)?;
                }
                let tags = given
                    .tags
                    .iter()
                    .filter_map(|t| self.content().tag(t))
                    .filter(|t| t.enabled)
                    .map(|t| Named::new(&t.id, &t.name))
                    .collect();
                StepOutcome::SetResult {
                    table: Named::new(&def.id, &def.name),
                    result: named,
                    value: given.value,
                    field: field.clone(),
                    tags,
                    events,
                }
            }
            StepAction::SetValue { field, value } => {
                self.memory.set(
                    &step.id,
                    Stored {
                        result: None,
                        value: Some(*value),
                    },
                );
                self.set_field(&step.id, field, None, Some(*value))?;
                StepOutcome::SetValue {
                    field: field.clone(),
                    value: *value,
                }
            }
            StepAction::Branch { cases, otherwise } => {
                let taken = cases.iter().position(|c| self.holds(&c.when, state));
                let steps = match taken {
                    Some(i) => &cases[i].steps,
                    None => otherwise,
                };
                StepOutcome::Branch {
                    case: taken,
                    steps: self.run_steps(steps, state, faces)?,
                }
            }
            StepAction::Repeat { count, steps } => {
                let (n, count_roll) = match count {
                    Count::Times(n) => (i64::from(*n), None),
                    Count::ValueOf { step: from, add } => {
                        let value =
                            self.memory.get(from).and_then(|s| s.value).ok_or_else(|| {
                                GenerationError::NoStoredValue { step: from.clone() }
                            })?;
                        ((value + add).max(0), None)
                    }
                    Count::Dice(dice) => {
                        let roll = DiceExpr::parse(dice)
                            .map_err(|error| GenerationError::InvalidDice {
                                expression: dice.clone(),
                                error,
                            })?
                            .roll(faces)?;
                        (i64::from(roll.total).max(0), Some(roll))
                    }
                };
                if n > i64::from(MAX_REPEAT) {
                    return Err(GenerationError::TooManyRepeats {
                        step: step.id.clone(),
                        count: n,
                    });
                }
                let mut iterations = Vec::new();
                for _ in 0..n {
                    iterations.push(self.run_steps(steps, state, faces)?);
                }
                StepOutcome::Repeat {
                    count: u32::try_from(n).unwrap_or(0),
                    count_roll,
                    iterations,
                }
            }
            StepAction::RunFlow { flow, max_depth } => {
                let def = self
                    .content()
                    .flow(flow)
                    .ok_or_else(|| GenerationError::UnknownFlow(flow.clone()))?;
                // Deliberate recursion stops quietly at its own limit.
                let inside = self.running.iter().filter(|f| *f == flow).count() as u32;
                if inside > 0 && inside > max_depth.unwrap_or(0) {
                    return Ok(StepOutcome::Skipped {
                        reason: StepSkip::RecursionLimit,
                    });
                }
                if self.depth >= MAX_FLOW_DEPTH {
                    return Err(GenerationError::FlowTooDeep { flow: flow.clone() });
                }
                self.depth += 1;
                self.running.push(flow.clone());
                let held = std::mem::take(&mut self.held);
                let steps = self.run_steps(&def.steps, state, faces);
                self.held = held;
                self.running.pop();
                // Stop ends the subflow only.
                self.stopped = false;
                self.depth -= 1;
                StepOutcome::Subflow {
                    flow: Named::new(&def.id, &def.name),
                    steps: steps?,
                }
            }
            StepAction::CreateObject {
                object,
                steps,
                share_tags,
            } => {
                let def = self
                    .content()
                    .object_type(object)
                    .ok_or_else(|| GenerationError::UnknownObjectType(object.clone()))?;
                let n = {
                    let count = self.counts.entry(object.clone()).or_default();
                    *count += 1;
                    *count
                };
                let mut own = State::with_tags(state.active());
                let before = state.active().to_vec();
                self.memory.scopes.push(HashMap::new());
                self.objects.push(Building {
                    fields: Vec::new(),
                    named: None,
                });
                let ran = self.run_steps(steps, &mut own, faces);
                self.memory.scopes.pop();
                let built = self.objects.pop().expect("object being built");
                let steps = ran?;
                let tags: Vec<TagId> = own
                    .active()
                    .iter()
                    .filter(|t| !before.contains(t))
                    .cloned()
                    .collect();
                if *share_tags {
                    state.activate(&tags);
                }
                let mut fields: Vec<FieldValue> = def
                    .fields
                    .iter()
                    .filter_map(|f| built.fields.iter().find(|v| v.field == f.id).cloned())
                    .collect();
                fields.extend(
                    built
                        .fields
                        .iter()
                        .filter(|v| def.field(&v.field).is_none())
                        .cloned(),
                );
                let label = built.named.unwrap_or_else(|| def.label_for(n));
                self.memory.created.push(Created {
                    object_type: def.id.clone(),
                    named: Named::new(&def.id_for(n), &label),
                    tags: tags.clone(),
                });
                StepOutcome::Object {
                    object: Box::new(ObjectRecord {
                        object_type: Named::new(&def.id, &def.name),
                        id: def.id_for(n),
                        label,
                        fields,
                        tags: tags
                            .iter()
                            .map(|id| {
                                let name = self
                                    .content()
                                    .tag(id)
                                    .map_or_else(|| id.to_string(), |t| t.name.clone());
                                Named::new(id, &name)
                            })
                            .collect(),
                        steps,
                    }),
                }
            }
            StepAction::Name { subject } => {
                let Some(scheme) = scheme_for_subject(self.content(), *subject, state.active())
                else {
                    return Ok(StepOutcome::Skipped {
                        reason: StepSkip::NoNamingScheme,
                    });
                };
                let GeneratedName {
                    text,
                    method,
                    rolls,
                } = generate_name(self.generator, scheme, state, None, faces)?;
                state.take_events();
                if let Some(object) = self.objects.last_mut() {
                    object.named = Some(text.clone());
                }
                StepOutcome::Name {
                    name: text,
                    scheme: Named::new(&scheme.id, &scheme.name),
                    method,
                    rolls,
                }
            }
            StepAction::ForEach {
                object,
                pairs,
                steps,
            } => {
                let of_type: Vec<(Named<String>, Vec<TagId>)> = self
                    .memory
                    .created
                    .iter()
                    .filter(|c| &c.object_type == object)
                    .map(|c| (c.named.clone(), c.tags.clone()))
                    .collect();
                let groups: Vec<Vec<usize>> = if *pairs {
                    (0..of_type.len())
                        .flat_map(|i| (i + 1..of_type.len()).map(move |j| vec![i, j]))
                        .collect()
                } else {
                    (0..of_type.len()).map(|i| vec![i]).collect()
                };
                let mut iterations = Vec::new();
                for group in groups {
                    if self.stopped {
                        break;
                    }
                    // One object: inside it, with its own tags. A pair: no
                    // one object's tags.
                    let own: Vec<TagId> = if group.len() == 1 {
                        of_type[group[0]].1.clone()
                    } else {
                        Vec::new()
                    };
                    let mut scope = State::with_tags(state.active().iter().chain(&own));
                    self.memory.scopes.push(HashMap::new());
                    let ran = self.run_steps(steps, &mut scope, faces);
                    self.memory.scopes.pop();
                    iterations.push(ForEachIteration {
                        objects: group.iter().map(|&i| of_type[i].0.clone()).collect(),
                        steps: ran?,
                    });
                }
                StepOutcome::ForEach {
                    pairs: *pairs,
                    iterations,
                }
            }
            StepAction::RemoveTag { tag } => {
                state.deactivate(tag);
                let name = self
                    .content()
                    .tag(tag)
                    .map_or_else(|| tag.to_string(), |t| t.name.clone());
                StepOutcome::RemoveTag {
                    tag: Named::new(tag, &name),
                }
            }
            StepAction::ApplyModifier { table, modifier } => {
                self.memory.applied.push(Applied {
                    step: step.id.clone(),
                    table: table.clone(),
                    modifier: *modifier,
                });
                let name = self
                    .content()
                    .table(table)
                    .map_or_else(|| table.to_string(), |t| t.name.clone());
                StepOutcome::ApplyModifier {
                    table: Named::new(table, &name),
                    modifier: *modifier,
                }
            }
            StepAction::ReadSetting { setting } => {
                let value = crate::engine::setting_value(
                    self.content(),
                    self.generator.options(),
                    self.generator.settings(),
                    setting,
                )
                .unwrap_or(SettingValue::Toggle(false));
                self.memory.set(&step.id, read_setting(&value));
                let name = self
                    .content()
                    .setting(setting)
                    .map_or_else(|| setting.to_string(), |s| s.name.clone());
                StepOutcome::ReadSetting {
                    setting: Named::new(setting, &name),
                    value,
                }
            }
            StepAction::Stop => {
                self.stopped = true;
                StepOutcome::Stop
            }
            StepAction::Builtin { handler, .. } => {
                return Err(GenerationError::InvalidBuiltin {
                    handler: handler.clone(),
                    error: "built-in steps run only as stages of the start flow".into(),
                });
            }
        })
    }

    /// Sets a field of the object being built.
    fn set_field(
        &mut self,
        step: &StepId,
        field: &str,
        result: Option<Named<ResultId>>,
        value: Option<i64>,
    ) -> Result<(), GenerationError> {
        let Some(object) = self.objects.last_mut() else {
            return Err(GenerationError::FieldOutsideObject { step: step.clone() });
        };
        let label = self
            .generator
            .content()
            .objects
            .iter()
            .flat_map(|o| o.fields.iter())
            .find(|f| f.id == field)
            .map_or_else(|| field.to_owned(), |f| f.name.clone());
        let entry = FieldValue {
            field: field.to_owned(),
            label,
            result,
            value,
        };
        match object.fields.iter_mut().find(|f| f.field == field) {
            Some(existing) => *existing = entry,
            None => object.fields.push(entry),
        }
        Ok(())
    }
}
