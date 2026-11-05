//! What running a flow produced. Like table records, these copy every name
//! at the time of generation, so a saved record never depends on current
//! content.

use serde::{Deserialize, Serialize};

use crate::dice::DiceRoll;
use crate::engine::{GenerationEvent, Named, SkipReason, TableRollRecord};
use crate::ids::{FlowId, NamingSchemeId, ObjectTypeId, ResultId, StepId, TableId, TagId};

/// One step as it ran.
///
/// Its outcome is flattened into the same JSON object, so no outcome field
/// may be called `step` or `label` (see [`StepRecord`]'s `Deserialize`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StepRecord {
    pub step: StepId,
    pub label: String,
    #[serde(flatten)]
    pub outcome: StepOutcome,
}

/// Read by hand rather than derived, to open settlements saved before
/// [`StepOutcome::SecondaryRoll`]'s label was stored as `roll_label`. Those
/// were written with two `label` keys in one object (the step's, then the
/// roll's), which the derived reader refuses as a duplicate field, so the
/// whole settlement could not be opened. The step's label always comes
/// first, so a second `label` is the roll's.
///
/// The record is first read whole into a JSON value by [`Repaired`], which
/// renames such a second `label` at every depth. Records nest (a Repeat's
/// iterations hold records), and a plain JSON value keeps only the last of
/// two equal keys, so the repair has to happen while the text is read.
impl<'de> Deserialize<'de> for StepRecord {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        use serde::de::Error;

        let Repaired(value) = Repaired::deserialize(deserializer)?;
        let serde_json::Value::Object(mut fields) = value else {
            return Err(D::Error::custom("expected a flow step record"));
        };
        let mut take = |name: &'static str| {
            fields
                .remove(name)
                .ok_or_else(|| D::Error::missing_field(name))
        };
        let step = serde_json::from_value(take("step")?).map_err(D::Error::custom)?;
        let label = serde_json::from_value(take("label")?).map_err(D::Error::custom)?;
        let outcome = StepOutcome::deserialize(serde_json::Value::Object(fields))
            .map_err(D::Error::custom)?;
        Ok(StepRecord {
            step,
            label,
            outcome,
        })
    }
}

/// A JSON value read so that, in any object, a second `label` key is kept
/// as `roll_label` instead of being refused or overwriting the first (see
/// [`StepRecord`]'s `Deserialize`). Any other repeated key is an error.
struct Repaired(serde_json::Value);

impl<'de> Deserialize<'de> for Repaired {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        use serde::de::{Error, MapAccess, SeqAccess, Visitor};
        use serde_json::Value;

        struct RepairVisitor;

        impl<'de> Visitor<'de> for RepairVisitor {
            type Value = Repaired;

            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("a JSON value")
            }
            fn visit_bool<E>(self, v: bool) -> Result<Repaired, E> {
                Ok(Repaired(Value::Bool(v)))
            }
            fn visit_i64<E>(self, v: i64) -> Result<Repaired, E> {
                Ok(Repaired(Value::from(v)))
            }
            fn visit_u64<E>(self, v: u64) -> Result<Repaired, E> {
                Ok(Repaired(Value::from(v)))
            }
            fn visit_f64<E>(self, v: f64) -> Result<Repaired, E> {
                Ok(Repaired(Value::from(v)))
            }
            fn visit_str<E>(self, v: &str) -> Result<Repaired, E> {
                Ok(Repaired(Value::String(v.to_owned())))
            }
            fn visit_string<E>(self, v: String) -> Result<Repaired, E> {
                Ok(Repaired(Value::String(v)))
            }
            fn visit_unit<E>(self) -> Result<Repaired, E> {
                Ok(Repaired(Value::Null))
            }
            fn visit_none<E>(self) -> Result<Repaired, E> {
                Ok(Repaired(Value::Null))
            }
            fn visit_some<D: serde::Deserializer<'de>>(self, d: D) -> Result<Repaired, D::Error> {
                Repaired::deserialize(d)
            }
            fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Repaired, A::Error> {
                let mut items = Vec::new();
                while let Some(Repaired(item)) = seq.next_element()? {
                    items.push(item);
                }
                Ok(Repaired(Value::Array(items)))
            }
            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Repaired, A::Error> {
                let mut fields = serde_json::Map::new();
                while let Some(key) = map.next_key::<String>()? {
                    let Repaired(value) = map.next_value()?;
                    let key = match key.as_str() {
                        "label" if fields.contains_key("label") => "roll_label".to_owned(),
                        _ => key,
                    };
                    if fields.contains_key(&key) {
                        return Err(A::Error::custom(format!("duplicate field `{key}`")));
                    }
                    fields.insert(key, value);
                }
                Ok(Repaired(Value::Object(fields)))
            }
        }

        deserializer.deserialize_any(RepairVisitor)
    }
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
        /// Stored as `roll_label`: the step record around it has its own
        /// `label` in the same JSON object.
        #[serde(rename = "roll_label")]
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dice::DieRoll;

    fn roll() -> DiceRoll {
        DiceRoll {
            expression: "1d6".into(),
            dice: vec![DieRoll {
                die: crate::dice::Die::D6,
                value: 4,
            }],
            modifier: 0,
            total: 4,
        }
    }

    fn secondary(step: &str, label: &str, roll_label: &str) -> StepRecord {
        StepRecord {
            step: StepId::new(step),
            label: label.into(),
            outcome: StepOutcome::SecondaryRoll {
                label: roll_label.into(),
                roll: roll(),
            },
        }
    }

    #[test]
    fn a_secondary_roll_reads_back_with_both_labels() {
        let record = secondary("s1", "Shop count", "Extra shops");
        let json = serde_json::to_string(&record).unwrap();
        assert_eq!(json.matches("\"label\"").count(), 1, "{json}");
        assert!(json.contains("\"roll_label\":\"Extra shops\""), "{json}");
        let back: StepRecord = serde_json::from_str(&json).unwrap();
        assert_eq!(back, record);
    }

    #[test]
    fn a_secondary_roll_saved_with_two_label_keys_still_opens() {
        // How these were stored before `roll_label`: the step's label, then
        // the roll's, under the same key in one object.
        let old = r#"{"step":"s1","label":"Shop count","kind":"secondary_roll","label":"Extra shops","roll":{"expression":"1d6","dice":[{"die":"d6","value":4}],"modifier":0,"total":4}}"#;
        let back: StepRecord = serde_json::from_str(old).unwrap();
        assert_eq!(back, secondary("s1", "Shop count", "Extra shops"));

        // The same inside a count flow's iterations (as the Spectacular
        // Settlements Shops and Services stages saved them).
        let nested = format!(
            r#"{{"step":"count","label":"Shops","kind":"repeat","count":1,"iterations":[[{old}]]}}"#
        );
        let back: StepRecord = serde_json::from_str(&nested).unwrap();
        assert_eq!(
            back,
            StepRecord {
                step: StepId::new("count"),
                label: "Shops".into(),
                outcome: StepOutcome::Repeat {
                    count: 1,
                    count_roll: None,
                    iterations: vec![vec![secondary("s1", "Shop count", "Extra shops")]],
                },
            }
        );
    }

    #[test]
    fn nested_records_round_trip() {
        let record = StepRecord {
            step: StepId::new("count"),
            label: "Shops".into(),
            outcome: StepOutcome::Repeat {
                count: 2,
                count_roll: Some(roll()),
                iterations: vec![
                    vec![secondary("a", "First", "Roll A")],
                    vec![StepRecord {
                        step: StepId::new("b"),
                        label: "Second".into(),
                        outcome: StepOutcome::Branch {
                            case: Some(1),
                            steps: vec![secondary("c", "Inner", "Roll C")],
                        },
                    }],
                ],
            },
        };
        let json = serde_json::to_string(&record).unwrap();
        assert_eq!(serde_json::from_str::<StepRecord>(&json).unwrap(), record);
        assert_eq!(
            serde_json::from_str::<StepRecord>(r#"{"label":"x","kind":"stop"}"#)
                .unwrap_err()
                .to_string(),
            "missing field `step`"
        );
    }
}
