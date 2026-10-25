//! Plain-English explanations of effects and modifiers, with a step-by-step
//! preview of what they do to a roll, e.g. "Roll 1d12 → add 2 → cap at 12 →
//! resolve result". Shared by every editor, so both interfaces say the same.

use serde::{Deserialize, Serialize};

use crate::content::{ContentSet, Effect, EffectDef, IntentChange, Modifier, Table, Tag};
use crate::dice::DiceExpr;
use crate::engine::Named;
use crate::ids::{ResultId, TableId};
use crate::layout::Layout;

/// Something an effect or modifier changes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AffectedKind {
    Table,
    Result,
    Tag,
    Flow,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Affected {
    pub kind: AffectedKind,
    pub id: String,
    pub name: String,
}

/// One effect (or modifier) in words.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Explanation {
    /// Short name, e.g. "Shift +1 on Crime".
    pub title: String,
    /// A sentence saying what it does.
    pub summary: String,
    /// What happens to a roll, step by step (empty when it changes no roll).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub preview: Vec<String>,
    /// Planned: documented, never run.
    pub planned: bool,
    pub affects: Vec<Affected>,
}

/// Every modifier a tag puts on one table, together.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TablePreview {
    pub table: Named<TableId>,
    pub steps: Vec<String>,
}

fn table_name(content: &ContentSet, id: &TableId) -> String {
    content
        .table(id)
        .map_or_else(|| format!("{id} (missing)"), |t| t.name.clone())
}

fn result_name(content: &ContentSet, table: &TableId, id: &ResultId) -> String {
    content
        .table(table)
        .and_then(|t| t.result(id))
        .map_or_else(|| id.to_string(), |r| r.name.clone())
}

fn names(content: &ContentSet, table: &TableId, ids: &[ResultId]) -> String {
    let list: Vec<String> = ids.iter().map(|r| result_name(content, table, r)).collect();
    match list.as_slice() {
        [] => "no results".into(),
        [one] => one.clone(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

fn plural(n: u32, word: &str) -> String {
    if n == 1 {
        format!("1 {word}")
    } else {
        format!("{n} {word}s")
    }
}

/// "twice as likely", "half as likely", "150% as likely" …
pub fn likelihood(percent: u32) -> String {
    match percent {
        100 => "as likely as usual".into(),
        200 => "twice as likely".into(),
        300 => "three times as likely".into(),
        400 => "four times as likely".into(),
        50 => "half as likely".into(),
        25 => "a quarter as likely".into(),
        p if p % 100 == 0 => format!("{} times as likely", p / 100),
        p => format!("{p}% as likely"),
    }
}

/// The highest (or lowest) result on a table's scale, by name.
fn scale_end(content: &ContentSet, table: &Table, highest: bool) -> Option<String> {
    let order = content.scale_order(table)?;
    let end = if highest { order.last() } else { order.first() }?;
    table.result(end).map(|r| r.name.clone())
}

/// What `modifiers` do to a roll of `table`, step by step. Plus and Minus
/// change the total before it is read; Shift and Cap then move the result
/// along the table's scale, as the engine applies them.
pub fn roll_preview(
    content: &ContentSet,
    table_id: &TableId,
    modifiers: &[Modifier],
) -> Vec<String> {
    let Some(table) = content.table(table_id) else {
        return vec![format!("Roll {}", table_name(content, table_id))];
    };
    let delta: i32 = modifiers.iter().map(Modifier::total_delta).sum();
    let mut out = Vec::new();
    match table.dice.as_deref() {
        Some(dice) if delta != 0 => {
            out.push(format!("Roll {dice}"));
            out.push(if delta > 0 {
                format!("add {delta}")
            } else {
                format!("take away {}", -delta)
            });
            // Totals past the table read as its nearest end.
            if let (Ok(expr), Some(bounds)) = (DiceExpr::parse(dice), Layout::base(table).bounds())
            {
                if delta > 0 && expr.max_total() + delta > bounds.max {
                    out.push(format!("cap at {}", bounds.max));
                }
                if delta < 0 && expr.min_total() + delta < bounds.min {
                    out.push(format!("cap at {}", bounds.min));
                }
            }
            out.push("resolve result".into());
        }
        _ => out.push(format!("Resolve {}", table.name)),
    }
    let shift: i32 = modifiers
        .iter()
        .map(|m| match m {
            Modifier::Shift(n) => *n,
            _ => 0,
        })
        .sum();
    if shift != 0 {
        let steps = plural(shift.unsigned_abs(), "step");
        out.push(format!(
            "shift result {} {steps}",
            if shift > 0 { "up" } else { "down" }
        ));
        if let Some(end) = scale_end(content, table, shift > 0) {
            out.push(format!("cap at {end}"));
        }
    }
    let cap = modifiers
        .iter()
        .rev()
        .find(|m| matches!(m, Modifier::CapAtHighest | Modifier::CapAtLowest));
    if let Some(cap) = cap {
        let highest = matches!(cap, Modifier::CapAtHighest);
        let end = scale_end(content, table, highest).unwrap_or_else(|| "its end".into());
        out.push(format!(
            "give the {} result, {end}",
            if highest { "highest" } else { "lowest" }
        ));
    }
    out
}

/// One modifier in words, as a flow step or a tag applies it.
pub fn explain_modifier(content: &ContentSet, table: &TableId, modifier: Modifier) -> Explanation {
    let name = table_name(content, table);
    let summary = match modifier {
        Modifier::Shift(n) => format!(
            "Moves a rolled result of {name} {} {} along its scale, stopping at the end.",
            plural(n.unsigned_abs(), "step"),
            if n > 0 { "up" } else { "down" }
        ),
        Modifier::Plus(n) => format!(
            "Adds {n} to the roll of {name} before its result is read; a total past the table counts as its highest face."
        ),
        Modifier::Minus(n) => format!(
            "Takes {n} from the roll of {name} before its result is read; a total past the table counts as its lowest face."
        ),
        Modifier::CapAtHighest => format!(
            "A roll of {name} gives its highest result that can be had{}.",
            content
                .table(table)
                .and_then(|t| scale_end(content, t, true))
                .map_or_else(String::new, |e| format!(" ({e})"))
        ),
        Modifier::CapAtLowest => format!(
            "A roll of {name} gives its lowest result that can be had{}.",
            content
                .table(table)
                .and_then(|t| scale_end(content, t, false))
                .map_or_else(String::new, |e| format!(" ({e})"))
        ),
    };
    Explanation {
        title: format!("{modifier} on {name}"),
        summary,
        preview: roll_preview(content, table, &[modifier]),
        planned: false,
        affects: vec![affected_table(content, table)],
    }
}

fn affected_table(content: &ContentSet, id: &TableId) -> Affected {
    Affected {
        kind: AffectedKind::Table,
        id: id.to_string(),
        name: table_name(content, id),
    }
}

fn affected_results(content: &ContentSet, table: &TableId, ids: &[ResultId]) -> Vec<Affected> {
    ids.iter()
        .map(|r| Affected {
            kind: AffectedKind::Result,
            id: r.to_string(),
            name: format!(
                "{} › {}",
                table_name(content, table),
                result_name(content, table, r)
            ),
        })
        .collect()
}

/// A result's chance (0–100) on `table`'s base layout, before and after a
/// Weight effect.
fn weight_chances(table: &Table, result: &ResultId, percent: u32) -> Option<(f64, f64)> {
    let dice = table.dice.as_deref()?;
    let chance = |layout: &Layout<'_>| -> Option<f64> {
        let ranges: Vec<_> = layout.entries.iter().map(|(_, r)| *r).collect();
        let weights = crate::dice_assist::range_weights(&ranges, dice)?;
        let total: u64 = weights.iter().sum();
        let i = layout.entries.iter().position(|(id, _)| *id == result)?;
        (total > 0).then(|| weights[i] as f64 * 100.0 / total as f64)
    };
    let before = chance(&Layout::base(table))?;
    let after = chance(&Layout::base(table).reweighted(dice, &[(result, percent)]))?;
    Some((before, after))
}

/// One tag effect in words. `tag` names the tag in the sentence, if given.
pub fn explain_effect(content: &ContentSet, tag: Option<&Tag>, def: &EffectDef) -> Explanation {
    let while_active = match tag {
        Some(t) => format!("While {} is active", t.name),
        None => "While the tag is active".into(),
    };
    let each_time = match tag {
        Some(t) => format!("Each time {} is added", t.name),
        None => "Each time the tag is added".into(),
    };
    let tn = |id: &TableId| table_name(content, id);
    let (title, summary, preview, affects) = match &def.effect {
        Effect::Shift { table, steps } => {
            let m = Modifier::Shift(*steps);
            let e = explain_modifier(content, table, m);
            (
                e.title,
                format!("{while_active}: {}", lower_first(&e.summary)),
                e.preview,
                e.affects,
            )
        }
        Effect::Plus { table, amount } => {
            let e = explain_modifier(content, table, Modifier::Plus(*amount));
            (
                e.title,
                format!("{while_active}: {}", lower_first(&e.summary)),
                e.preview,
                e.affects,
            )
        }
        Effect::Minus { table, amount } => {
            let e = explain_modifier(content, table, Modifier::Minus(*amount));
            (
                e.title,
                format!("{while_active}: {}", lower_first(&e.summary)),
                e.preview,
                e.affects,
            )
        }
        Effect::CapAtHighest { table } => {
            let e = explain_modifier(content, table, Modifier::CapAtHighest);
            (
                e.title,
                format!("{while_active}: {}", lower_first(&e.summary)),
                e.preview,
                e.affects,
            )
        }
        Effect::CapAtLowest { table } => {
            let e = explain_modifier(content, table, Modifier::CapAtLowest);
            (
                e.title,
                format!("{while_active}: {}", lower_first(&e.summary)),
                e.preview,
                e.affects,
            )
        }
        Effect::ModifyDice {
            table,
            dice,
            modifier,
        } => {
            let mut parts = Vec::new();
            if let Some(d) = dice {
                parts.push(format!("{} is rolled with {d}", tn(table)));
            }
            if *modifier != 0 {
                parts.push(format!("{modifier:+} is added to its total"));
            }
            let mut preview = vec![format!(
                "Roll {}",
                dice.clone()
                    .or_else(|| content.table(table).and_then(|t| t.dice.clone()))
                    .unwrap_or_else(|| "its dice".into())
            )];
            if *modifier != 0 {
                preview.push(if *modifier > 0 {
                    format!("add {modifier}")
                } else {
                    format!("take away {}", -modifier)
                });
            }
            preview.push("resolve result".into());
            (
                format!("Change the dice of {}", tn(table)),
                format!("{while_active}, {}.", parts.join(" and ")),
                preview,
                vec![affected_table(content, table)],
            )
        }
        Effect::RestrictResults {
            table,
            only,
            exclude,
        } => {
            let mut parts = Vec::new();
            if let Some(only) = only {
                parts.push(format!("only {} can come up", names(content, table, only)));
            }
            if !exclude.is_empty() {
                parts.push(format!("{} cannot come up", names(content, table, exclude)));
            }
            let mut ids: Vec<ResultId> = only.clone().unwrap_or_default();
            ids.extend(exclude.iter().cloned());
            let mut affects = vec![affected_table(content, table)];
            affects.extend(affected_results(content, table, &ids));
            (
                if only.is_none() {
                    format!("Disable results of {}", tn(table))
                } else {
                    format!("Limit the results of {}", tn(table))
                },
                format!(
                    "{while_active}, {} on {}; any other roll is rolled again.",
                    if parts.is_empty() {
                        "nothing changes".to_owned()
                    } else {
                        parts.join(", and ")
                    },
                    tn(table)
                ),
                Vec::new(),
                affects,
            )
        }
        Effect::EnableResults { table, results } => {
            let mut affects = vec![affected_table(content, table)];
            affects.extend(affected_results(content, table, results));
            (
                format!("Enable results of {}", tn(table)),
                format!(
                    "{while_active}, {} can come up on {} even though disabled.",
                    names(content, table, results),
                    tn(table)
                ),
                Vec::new(),
                affects,
            )
        }
        Effect::Weight {
            table,
            result,
            percent,
        } => {
            let rname = result_name(content, table, result);
            let chances = content
                .table(table)
                .and_then(|t| weight_chances(t, result, *percent));
            let mut preview = Vec::new();
            if let Some(dice) = content.table(table).and_then(|t| t.dice.clone()) {
                preview.push(format!("Lay {} out again on {dice}", tn(table)));
                if let Some((before, after)) = chances {
                    preview.push(format!("{rname}: {before:.0}% → {after:.0}%"));
                }
                preview.push(format!("Roll {dice}"));
                preview.push("resolve result".into());
            }
            let mut affects = vec![affected_table(content, table)];
            affects.extend(affected_results(
                content,
                table,
                std::slice::from_ref(result),
            ));
            (
                format!("{rname} {}", likelihood(*percent)),
                format!(
                    "{while_active}, {rname} is {} on {}{}. The other results share the rest of the dice.",
                    likelihood(*percent),
                    tn(table),
                    chances.map_or_else(String::new, |(b, a)| format!(
                        " ({b:.0}% becomes about {a:.0}%)"
                    ))
                ),
                preview,
                affects,
            )
        }
        Effect::RouteTo { table, variant } => (
            format!("Roll {}", tn(table)),
            format!(
                "{each_time}, {} is rolled next{}.",
                tn(table),
                variant
                    .as_ref()
                    .map_or_else(String::new, |v| format!(" (variant {v})"))
            ),
            Vec::new(),
            vec![affected_table(content, table)],
        ),
        Effect::RunFlow { flow } => {
            let name = content
                .flow(flow)
                .map_or_else(|| format!("{flow} (missing)"), |f| f.name.clone());
            (
                format!("Run {name}"),
                format!(
                    "{each_time} by a step of a flow, the flow {name} runs right after that step."
                ),
                Vec::new(),
                vec![Affected {
                    kind: AffectedKind::Flow,
                    id: flow.to_string(),
                    name,
                }],
            )
        }
        Effect::AddTag { tag: other } => {
            let name = content
                .tag(other)
                .map_or_else(|| format!("{other} (missing)"), |t| t.name.clone());
            (
                format!("Also add {name}"),
                format!("{each_time}, {name} is added too."),
                Vec::new(),
                vec![Affected {
                    kind: AffectedKind::Tag,
                    id: other.to_string(),
                    name,
                }],
            )
        }
        Effect::SecondaryRoll { label, dice } => (
            format!("Roll {dice} for {label}"),
            format!("{each_time}, {dice} is rolled and recorded as “{label}”."),
            vec![format!("Roll {dice}"), format!("record it as {label}")],
            Vec::new(),
        ),
        Effect::RedirectTable { table, to } => (
            format!("Roll {} instead of {}", tn(to), tn(table)),
            format!(
                "{while_active}, whenever {} would be rolled, {} is rolled instead.",
                tn(table),
                tn(to)
            ),
            Vec::new(),
            vec![affected_table(content, table), affected_table(content, to)],
        ),
        Effect::UseVariant { table, variant } => {
            let vname = content
                .table(table)
                .and_then(|t| t.variant(variant))
                .map_or_else(|| variant.to_string(), |v| v.name.clone());
            (
                format!("Use {vname} for {}", tn(table)),
                format!("{while_active}, {} uses its variant {vname}.", tn(table)),
                Vec::new(),
                vec![affected_table(content, table)],
            )
        }
        Effect::Intent {
            table,
            results,
            concept,
            change,
        } => {
            let target = match (table, concept) {
                (Some(t), _) if !results.is_empty() => {
                    format!("{} on {}", names(content, t, results), tn(t))
                }
                (Some(t), _) => tn(t),
                (None, Some(c)) => c.clone(),
                (None, None) => "something not yet named".into(),
            };
            let verb = match change {
                IntentChange::Increase => "makes more likely",
                IntentChange::StrongIncrease => "makes much more likely",
                IntentChange::Decrease => "makes less likely",
                IntentChange::StrongDecrease => "makes much less likely",
                IntentChange::Guarantee => "guarantees",
            };
            (
                format!("Intent: {verb} {target}"),
                format!("A design intention: the tag {verb} {target}. No numbers are decided yet."),
                Vec::new(),
                table.iter().map(|t| affected_table(content, t)).collect(),
            )
        }
    };
    let planned = !def.is_active();
    Explanation {
        title,
        summary: if planned {
            format!("Planned, not run: {}", lower_first(&summary))
        } else {
            summary
        },
        preview,
        planned,
        affects,
    }
}

/// Every active modifier `tag` puts on each table, combined in the order
/// the engine applies them.
pub fn tag_previews(content: &ContentSet, tag: &Tag) -> Vec<TablePreview> {
    let mut tables: Vec<(&TableId, Vec<Modifier>)> = Vec::new();
    for def in tag.effects.iter().filter(|d| d.is_active()) {
        let (Some(m), Some(table)) = (def.effect.modifier(), def.effect.target_table()) else {
            continue;
        };
        match tables.iter_mut().find(|(t, _)| *t == table) {
            Some((_, list)) => list.push(m),
            None => tables.push((table, vec![m])),
        }
    }
    tables
        .into_iter()
        .filter(|(_, list)| list.len() > 1)
        .map(|(table, list)| TablePreview {
            table: Named::new(table, &table_name(content, table)),
            steps: roll_preview(content, table, &list),
        })
        .collect()
}

fn lower_first(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        Some(c) => c.to_lowercase().chain(chars).collect(),
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn content() -> ContentSet {
        serde_json::from_value(serde_json::json!({
            "tables": [
                { "id": "table.d12", "name": "Wealth", "dice": "1d12", "results": [
                    { "id": "poor", "name": "Poor", "range": [1, 6] },
                    { "id": "rich", "name": "Rich", "range": [7, 12] } ] },
                { "id": "table.crime", "name": "Crime", "dice": "1d4", "ordered": true, "results": [
                    { "id": "low", "name": "Low", "range": 1 },
                    { "id": "some", "name": "Some", "range": 2 },
                    { "id": "high", "name": "High", "range": 3 },
                    { "id": "rampant", "name": "Rampant", "range": 4 } ] }
            ]
        }))
        .unwrap()
    }

    #[test]
    fn previews_read_like_the_roll() {
        let c = content();
        assert_eq!(
            roll_preview(&c, &"table.d12".into(), &[Modifier::Plus(2)]),
            ["Roll 1d12", "add 2", "cap at 12", "resolve result"]
        );
        assert_eq!(
            roll_preview(&c, &"table.crime".into(), &[Modifier::Shift(1)]),
            ["Resolve Crime", "shift result up 1 step", "cap at Rampant"]
        );
        assert_eq!(
            roll_preview(&c, &"table.crime".into(), &[Modifier::CapAtLowest]),
            ["Resolve Crime", "give the lowest result, Low"]
        );
    }

    #[test]
    fn weighting_explains_the_new_chance() {
        let c = content();
        let e = explain_effect(
            &c,
            None,
            &EffectDef::active(Effect::Weight {
                table: "table.d12".into(),
                result: "rich".into(),
                percent: 200,
            }),
        );
        assert_eq!(e.title, "Rich twice as likely");
        assert!(e.summary.contains("50% becomes about 67%"), "{}", e.summary);
    }

    #[test]
    fn planned_effects_say_so() {
        let c = content();
        let e = explain_effect(
            &c,
            None,
            &EffectDef::planned(
                Effect::Plus {
                    table: "table.d12".into(),
                    amount: 1,
                },
                "later",
            ),
        );
        assert!(e.planned);
        assert!(e.summary.starts_with("Planned, not run: "), "{}", e.summary);
    }
}
