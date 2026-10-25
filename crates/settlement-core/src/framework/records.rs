//! What running a flow produced. Like table records, these copy every name
//! at the time of generation, so a saved record never depends on current
//! content.

use serde::{Deserialize, Serialize};

use crate::dice::DiceRoll;
use crate::engine::{GenerationEvent, Named, SkipReason, TableRollRecord};
use crate::ids::{FlowId, NamingSchemeId, ObjectTypeId, ResultId, StepId, TableId, TagId};

/// One step as it ran.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StepRecord {
    pub step: StepId,
    pub label: String,
    #[serde(flatten)]
    pub outcome: StepOutcome,
}

/// Why a step did not run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StepSkip {
    /// Its `when` condition did not hold.
    ConditionFalse,
    /// Its table is inactive or not eligible.
    Table(SkipReason),
    /// No naming scheme applies.
    NoNamingScheme,
    /// A flow running itself reached the step's maximum depth.
    RecursionLimit,
}

/// One pass of a For Each: the object (or pair of objects) it ran for.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ForEachIteration {
    /// The object's ID and label; two for a pair.
    pub objects: Vec<Named<String>>,
    pub steps: Vec<StepRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum StepOutcome {
    /// A table roll (or choice): the table's record first, then any tables
    /// its tags routed to and secondary rolls they made.
    Roll {
        events: Vec<GenerationEvent>,
        /// The object field it filled.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        field: Option<String>,
        /// The result given instead, when the rolled result is unique and
        /// was already held in this run.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        fallback: Option<Named<ResultId>>,
    },
    Skipped {
        reason: StepSkip,
    },
    SecondaryRoll {
        label: String,
        roll: DiceRoll,
    },
    AddTag {
        tag: Named<TagId>,
        /// Tables the tag routed to and secondary rolls it made.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        events: Vec<GenerationEvent>,
    },
    SetResult {
        table: Named<TableId>,
        result: Named<ResultId>,
        /// The result's value, if it has one.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        value: Option<i64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        field: Option<String>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        tags: Vec<Named<TagId>>,
        /// Tables its tags routed to and secondary rolls they made.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        events: Vec<GenerationEvent>,
    },
    SetValue {
        field: String,
        value: i64,
    },
    Branch {
        /// The case taken (by position), or `None` for `otherwise`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        case: Option<usize>,
        steps: Vec<StepRecord>,
    },
    Repeat {
        count: u32,
        /// The count roll, for a count rolled with dice.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        count_roll: Option<DiceRoll>,
        iterations: Vec<Vec<StepRecord>>,
    },
    Subflow {
        flow: Named<FlowId>,
        steps: Vec<StepRecord>,
    },
    ForEach {
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        pairs: bool,
        iterations: Vec<ForEachIteration>,
    },
    RemoveTag {
        tag: Named<TagId>,
    },
    ApplyModifier {
        table: Named<TableId>,
        modifier: crate::content::Modifier,
    },
    ReadSetting {
        setting: Named<crate::ids::SettingId>,
        value: super::SettingValue,
    },
    Stop,
    Object {
        object: Box<ObjectRecord>,
    },
    Name {
        name: String,
        scheme: Named<NamingSchemeId>,
        method: Named<ResultId>,
        rolls: Vec<TableRollRecord>,
    },
}

/// One generated object.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObjectRecord {
    #[serde(rename = "type")]
    pub object_type: Named<ObjectTypeId>,
    /// Stable, e.g. `building-01`; never derived from display text.
    pub id: String,
    /// Its name if it was named, else its numbered label.
    pub label: String,
    /// Its fields in the type's order, then any others.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fields: Vec<FieldValue>,
    /// Tags the object's own results activated.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<Named<TagId>>,
    pub steps: Vec<StepRecord>,
}

/// One field of an object.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FieldValue {
    pub field: String,
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<Named<ResultId>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<i64>,
}

/// One line of a flow record as interfaces show it: an indented label and
/// value (e.g. "Colour: Red"), so both interfaces present flows the same way.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutlineLine {
    pub depth: usize,
    pub label: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub value: String,
}

impl StepRecord {
    /// The record as indented lines: each roll and value with its label,
    /// each object and repeat with what it holds below it. Branches show
    /// only the steps they ran.
    pub fn outline(&self) -> Vec<OutlineLine> {
        let mut out = Vec::new();
        self.outline_into(0, &mut out);
        out
    }

    /// The outline without the step's own line (which a stage already
    /// shows as its result): what it holds, one level less indented.
    pub fn outline_below(&self) -> Vec<OutlineLine> {
        let mut lines = self.outline();
        if !matches!(self.outcome, StepOutcome::Branch { .. }) && !lines.is_empty() {
            lines.remove(0);
            for line in &mut lines {
                line.depth = line.depth.saturating_sub(1);
            }
        }
        lines
    }

    fn outline_into(&self, depth: usize, out: &mut Vec<OutlineLine>) {
        let mut line = |label: &str, value: String| {
            out.push(OutlineLine {
                depth,
                label: label.to_owned(),
                value,
            });
        };
        match &self.outcome {
            StepOutcome::Roll { events, .. } => {
                let moved = events.iter().find_map(|e| match e {
                    GenerationEvent::TableRolled(r) if !r.is_routed() => {
                        r.adjusted_from.as_ref().map(|f| f.name.clone())
                    }
                    _ => None,
                });
                let value = match moved {
                    Some(from) => format!("{} (moved from {from})", self.summary()),
                    None => self.summary(),
                };
                line(&self.label, value);
            }
            StepOutcome::Skipped { .. } => line(&self.label, "Not applicable".into()),
            StepOutcome::SecondaryRoll { roll, .. } => {
                line(&self.label, format!("{} ({})", roll.total, roll.expression));
            }
            StepOutcome::Branch { steps, .. } => {
                for step in steps {
                    step.outline_into(depth, out);
                }
            }
            StepOutcome::Repeat {
                count, iterations, ..
            } => {
                line(&self.label, format!("{count} ×"));
                for step in iterations.iter().flatten() {
                    step.outline_into(depth + 1, out);
                }
            }
            StepOutcome::Subflow { flow, steps } => {
                line(&flow.name, String::new());
                for step in steps {
                    step.outline_into(depth + 1, out);
                }
            }
            StepOutcome::Object { object } => {
                line(&object.label, object.object_type.name.clone());
                for step in &object.steps {
                    step.outline_into(depth + 1, out);
                }
            }
            StepOutcome::ForEach { iterations, .. } => {
                for iteration in iterations {
                    let who: Vec<&str> =
                        iteration.objects.iter().map(|o| o.name.as_str()).collect();
                    out.push(OutlineLine {
                        depth,
                        label: who.join(" & "),
                        value: String::new(),
                    });
                    for step in &iteration.steps {
                        step.outline_into(depth + 1, out);
                    }
                }
            }
            _ => line(&self.label, self.summary()),
        }
    }

    /// Tags this step's Remove Tag steps took away from the generation
    /// (not those removed inside an object or a For Each, which only
    /// affect that object or iteration).
    pub fn removed_tags(&self) -> Vec<TagId> {
        let mut out = Vec::new();
        self.collect_removed(&mut out);
        out
    }

    fn collect_removed(&self, out: &mut Vec<TagId>) {
        match &self.outcome {
            StepOutcome::RemoveTag { tag } => out.push(tag.id.clone()),
            StepOutcome::Object { .. } | StepOutcome::ForEach { .. } => {}
            _ => {
                for child in self.children() {
                    child.collect_removed(out);
                }
            }
        }
    }

    /// The steps nested directly inside this one.
    pub fn children(&self) -> Vec<&StepRecord> {
        match &self.outcome {
            StepOutcome::Branch { steps, .. } | StepOutcome::Subflow { steps, .. } => {
                steps.iter().collect()
            }
            StepOutcome::Repeat { iterations, .. } => iterations.iter().flatten().collect(),
            StepOutcome::Object { object } => object.steps.iter().collect(),
            StepOutcome::ForEach { iterations, .. } => {
                iterations.iter().flat_map(|i| i.steps.iter()).collect()
            }
            _ => Vec::new(),
        }
    }

    /// Every table record in this step and the steps inside it, in order.
    pub fn table_rolls(&self) -> Vec<&TableRollRecord> {
        let mut out = Vec::new();
        self.collect_rolls(&mut out);
        out
    }

    fn collect_rolls<'a>(&'a self, out: &mut Vec<&'a TableRollRecord>) {
        match &self.outcome {
            StepOutcome::Roll { events, .. }
            | StepOutcome::AddTag { events, .. }
            | StepOutcome::SetResult { events, .. } => {
                out.extend(events.iter().filter_map(|e| match e {
                    GenerationEvent::TableRolled(r) => Some(&**r),
                    _ => None,
                }));
            }
            StepOutcome::Name { rolls, .. } => out.extend(rolls.iter()),
            _ => {}
        }
        for child in self.children() {
            child.collect_rolls(out);
        }
    }

    /// Every object created in this step and the steps inside it.
    pub fn objects(&self) -> Vec<&ObjectRecord> {
        let mut out = Vec::new();
        if let StepOutcome::Object { object } = &self.outcome {
            out.push(&**object);
        }
        for child in self.children() {
            out.extend(child.objects());
        }
        out
    }

    /// A one-line account of the step, e.g. "Prosperous" or "3 × Building".
    pub fn summary(&self) -> String {
        match &self.outcome {
            StepOutcome::Roll {
                events, fallback, ..
            } => fallback
                .as_ref()
                .map(|f| f.name.clone())
                .or_else(|| {
                    events.iter().find_map(|e| match e {
                        GenerationEvent::TableRolled(r) => Some(r.result.name.clone()),
                        _ => None,
                    })
                })
                .unwrap_or_default(),
            StepOutcome::Skipped { .. } => "Skipped".into(),
            StepOutcome::SecondaryRoll { roll, .. } => roll.total.to_string(),
            StepOutcome::AddTag { tag, .. } => tag.name.clone(),
            StepOutcome::SetResult { result, .. } => result.name.clone(),
            StepOutcome::SetValue { value, .. } => value.to_string(),
            StepOutcome::Branch { steps, .. } | StepOutcome::Subflow { steps, .. } => {
                let parts: Vec<String> = steps
                    .iter()
                    .filter(|s| !matches!(s.outcome, StepOutcome::Skipped { .. }))
                    .map(StepRecord::summary)
                    .filter(|s| !s.is_empty())
                    .collect();
                parts.join(", ")
            }
            StepOutcome::Repeat { count, .. } => {
                let objects = self.objects();
                match objects.first() {
                    Some(first) if objects.len() == *count as usize => {
                        format!("{count} × {}", first.object_type.name)
                    }
                    _ => format!("{count} ×"),
                }
            }
            StepOutcome::Object { object } => object.label.clone(),
            StepOutcome::Name { name, .. } => name.clone(),
            StepOutcome::ForEach { pairs, iterations } => format!(
                "{} {}",
                iterations.len(),
                if *pairs { "pairs" } else { "×" }
            ),
            StepOutcome::RemoveTag { tag } => format!("removed {}", tag.name),
            StepOutcome::ApplyModifier { table, modifier } => {
                format!("{modifier} on {}", table.name)
            }
            StepOutcome::ReadSetting { value, .. } => match value {
                super::SettingValue::Toggle(on) => if *on { "On" } else { "Off" }.to_owned(),
                super::SettingValue::Number(n) => n.to_string(),
                super::SettingValue::Choice(c) => c.clone(),
                super::SettingValue::Many(v) if v.is_empty() => "None".into(),
                super::SettingValue::Many(v) => v.join(", "),
            },
            StepOutcome::Stop => "Stopped".into(),
        }
    }
}
