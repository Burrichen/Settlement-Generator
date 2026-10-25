//! Generation traces: every generated value explained, line by line, from
//! the roll's own record. For example:
//!
//! ```text
//! Crime
//! Base roll: 10 (1d12: 10)
//! Security modifier: -1
//! Economy modifier: +2
//! Final roll: 11
//! Result: High
//! ```
//!
//! Shifts along an ordered scale show the path the result moved along.

use serde::{Deserialize, Serialize};

use crate::content::{ContentSet, Effect, Modifier};
use crate::engine::{AppliedModifier, AttemptOutcome, GenerationEvent, TableRollRecord};

/// One table roll explained.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RollTrace {
    /// The table's name.
    pub title: String,
    pub lines: Vec<String>,
}

/// Who a modifier belongs to, in words.
fn source(m: &AppliedModifier) -> String {
    match (&m.tag, &m.step) {
        (Some(tag), _) => tag.name.clone(),
        (None, Some(step)) => format!("Step “{step}”"),
        (None, None) => "A rule".into(),
    }
}

fn signed(n: i32) -> String {
    format!("{n:+}")
}

/// The path a result moved along its table's scale, e.g. "High → Very
/// High → Rampant".
fn scale_path(content: &ContentSet, record: &TableRollRecord, from: &str, to: &str) -> String {
    let Some(table) = content.table(&record.table.id) else {
        return format!("{from} → {to}");
    };
    let Some(order) = content.scale_order(table) else {
        return format!("{from} → {to}");
    };
    let name = |id: &crate::ids::ResultId| {
        table
            .result(id)
            .map_or_else(|| id.to_string(), |r| r.name.clone())
    };
    let names: Vec<String> = order.iter().map(|id| name(id)).collect();
    let (Some(a), Some(b)) = (
        names.iter().position(|n| n == from),
        names.iter().position(|n| n == to),
    ) else {
        return format!("{from} → {to}");
    };
    let path: Vec<&str> = if a <= b {
        names[a..=b].iter().map(String::as_str).collect()
    } else {
        names[b..=a].iter().rev().map(String::as_str).collect()
    };
    path.join(" → ")
}

/// Explains one table roll from its record. `content` names scale steps;
/// a record from content that has since changed still explains itself.
pub fn trace_roll(content: &ContentSet, record: &TableRollRecord) -> RollTrace {
    let mut lines = Vec::new();
    if let Some(from) = &record.redirected_from {
        lines.push(format!("Redirected from {}", from.name));
    }
    if let Some(tag) = &record.routed_by {
        lines.push(format!("Rolled because of {}", tag.name));
    }
    if let Some(result) = &record.rolled_after {
        lines.push(format!("Rolled after {}", result.name));
    }
    if let Some(variant) = &record.variant {
        lines.push(format!("Variant: {}", variant.name));
    }
    for m in record.modifiers.iter().filter(|m| m.ignored) {
        lines.push(format!("{} restriction ignored (God Mode)", source(m)));
    }
    if record.selected {
        if let Some(from) = &record.adjusted_from {
            lines.push(format!("Chosen: {}", from.name));
        } else {
            lines.push(format!("Chosen: {}", record.result.name));
        }
    }
    // Earlier attempts that were rolled again.
    let rerolls = record.attempts.len().saturating_sub(1);
    for attempt in &record.attempts[..rerolls] {
        lines.push(format!(
            "Rolled {} → {} ({}, rolled again)",
            attempt.roll.total, attempt.result.name, attempt.outcome
        ));
    }
    if let Some(accepted) = record.accepted() {
        let roll = &accepted.roll;
        let base = roll.total - roll.modifier;
        let dice: Vec<String> = roll.dice.iter().map(|d| d.value.to_string()).collect();
        lines.push(format!(
            "Base roll: {base} ({}: {})",
            without_modifier(record.dice.as_deref().unwrap_or(&roll.expression)),
            dice.join(" + ")
        ));
        // The roll's modifier is the dice's own plus every Plus, Minus and
        // dice modifier applied; list them by source.
        let mut explained = 0;
        for m in record.modifiers.iter().filter(|m| !m.ignored) {
            let delta = match &m.effect {
                Effect::ModifyDice { modifier, .. } => *modifier,
                effect => effect.modifier().map_or(0, |x| x.total_delta()),
            };
            if delta != 0 {
                lines.push(format!("{} modifier: {}", source(m), signed(delta)));
                explained += delta;
            }
        }
        if roll.modifier != explained {
            lines.push(format!(
                "Dice modifier: {}",
                signed(roll.modifier - explained)
            ));
        }
        if roll.modifier != 0 {
            lines.push(format!("Final roll: {}", roll.total));
        }
        if accepted.face != roll.total {
            lines.push(format!(
                "Read as {} (past the end of the table)",
                accepted.face
            ));
        }
        let rolled = record
            .adjusted_from
            .as_ref()
            .map_or(&accepted.result.name, |r| &r.name);
        lines.push(format!("Rolled result: {rolled}"));
    }
    if let Some(from) = &record.adjusted_from {
        let to = record
            .follow_ups
            .first()
            .map_or(&record.result.name, |f| &f.from.name);
        let moves: Vec<String> = record
            .modifiers
            .iter()
            .filter(|m| !m.ignored)
            .filter_map(|m| {
                let modifier = m.effect.modifier()?;
                modifier
                    .needs_scale()
                    .then(|| format!("{} ({})", modifier_name(modifier), source(m)))
            })
            .collect();
        lines.push(format!(
            "{}: {}",
            if moves.is_empty() {
                "Moved along the scale".to_owned()
            } else {
                moves.join(", ")
            },
            scale_path(content, record, &from.name, to)
        ));
    }
    for f in &record.follow_ups {
        match &f.becomes {
            Some(to) => lines.push(format!(
                "{}: rolled {} → becomes {}",
                f.name, f.roll.total, to.name
            )),
            None => lines.push(format!(
                "{}: rolled {} → stays {}",
                f.name, f.roll.total, f.from.name
            )),
        }
    }
    lines.push(format!("Result: {}", record.result.name));
    if let Some(v) = record.value {
        lines.push(format!("Value: {v}"));
    }
    if !record.tags.is_empty() {
        let names: Vec<&str> = record.tags.iter().map(|t| t.name.as_str()).collect();
        lines.push(format!("Tags given: {}", names.join(", ")));
    }
    RollTrace {
        title: record.table.name.clone(),
        lines,
    }
}

/// "1d12+1" → "1d12": the dice alone (their modifiers are listed apart).
fn without_modifier(dice: &str) -> &str {
    match dice.rfind(['+', '-']) {
        Some(at)
            if at > 0
                && dice[at + 1..].chars().all(|c| c.is_ascii_digit())
                && !dice[at + 1..].is_empty() =>
        {
            &dice[..at]
        }
        _ => dice,
    }
}

fn modifier_name(m: Modifier) -> String {
    m.to_string()
}

/// Every table roll among `events`, explained.
pub fn trace_events(content: &ContentSet, events: &[GenerationEvent]) -> Vec<RollTrace> {
    let mut out = Vec::new();
    for event in events {
        match event {
            GenerationEvent::TableRolled(record) => out.push(trace_roll(content, record)),
            GenerationEvent::SecondaryRoll { tag, label, roll } => out.push(RollTrace {
                title: label.clone(),
                lines: vec![
                    format!("Rolled because of {}", tag.name),
                    format!("Roll: {} ({})", roll.total, roll),
                ],
            }),
            _ => {}
        }
    }
    out
}

/// Why an attempt was rolled again, in words (used by Step Through).
pub fn outcome_words(outcome: AttemptOutcome) -> String {
    outcome.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dice::ScriptedFaces;
    use crate::engine::Generator;

    #[test]
    fn a_modified_roll_lists_each_modifier_and_the_scale_path() {
        let content: ContentSet = serde_json::from_value(serde_json::json!({
            "tables": [
                { "id": "table.start", "name": "Start", "dice": "1d4", "results": [
                    { "id": "go", "name": "Go", "range": [1, 4], "tags": ["tag.sec", "tag.eco"] } ] },
                { "id": "table.crime", "name": "Crime", "dice": "1d12", "ordered": true, "results": [
                    { "id": "low", "name": "Low", "range": [1, 4] },
                    { "id": "some", "name": "Some", "range": [5, 8] },
                    { "id": "high", "name": "High", "range": [9, 11] },
                    { "id": "rampant", "name": "Rampant", "range": 12 } ] }
            ],
            "tags": [
                { "id": "tag.sec", "name": "Security", "effects": [
                    { "kind": "minus", "table": "table.crime", "amount": 1 } ] },
                { "id": "tag.eco", "name": "Economy", "effects": [
                    { "kind": "plus", "table": "table.crime", "amount": 2 },
                    { "kind": "shift", "table": "table.crime", "steps": 1 } ] }
            ]
        }))
        .unwrap();
        let log = Generator::new(&content)
            .unwrap()
            .run(
                &["table.start".into(), "table.crime".into()],
                &mut ScriptedFaces::new([1, 9]),
            )
            .unwrap();
        let trace = trace_roll(&content, log.record_for(&"table.crime".into()).unwrap());
        assert_eq!(trace.title, "Crime");
        assert_eq!(
            trace.lines,
            [
                "Base roll: 9 (1d12: 9)",
                "Security modifier: -1",
                "Economy modifier: +2",
                "Final roll: 10",
                "Rolled result: High",
                "Shift +1 (Economy): High → Rampant",
                "Result: Rampant",
            ]
        );
    }
}
