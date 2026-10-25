//! Support for the no-code table editor, shared by both interfaces:
//! applying a Dice Assistant layout to a table, and finding everything that
//! depends on a table or a result so nothing is deleted while in use.

use serde::{Deserialize, Serialize};

use crate::content::{Condition, ContentSet, Effect, Table, TableResult};
use crate::dice_assist::DiceFit;
use crate::framework::StepAction;
use crate::framework::builtin::Builtin;
use crate::ids::{ResultId, TableId};
use crate::naming::JoinStyle;

/// Display name of the result that marks faces rolled again.
pub const ROLL_AGAIN_NAME: &str = "Roll again";
/// Its description.
pub const ROLL_AGAIN_NOTE: &str =
    "Faces that are rolled again, so every other result keeps its exact chance.";

/// Whether `result` is the editor's "Roll again" entry: disabled, so its
/// faces are rolled again, and carrying nothing else.
pub fn is_roll_again(result: &TableResult) -> bool {
    !result.enabled
        && result.name == ROLL_AGAIN_NAME
        && result.tags.is_empty()
        && result.then_roll.is_empty()
}

/// `table` with `fit` applied: its dice, and each of `entries` (results, in
/// the order the fit's ranges are given) on its faces. Other results are
/// taken off the dice (they can still be chosen). Faces rolled again are
/// given to the table's "Roll again" entry, created with `roll_again_id`
/// when there is none; a "Roll again" entry the fit does not need is
/// removed.
pub fn apply_fit(
    table: &Table,
    entries: &[ResultId],
    fit: &DiceFit,
    roll_again_id: ResultId,
) -> Result<Table, String> {
    if entries.len() != fit.ranges.len() {
        return Err(format!(
            "The layout has {} results but {} were given.",
            fit.ranges.len(),
            entries.len()
        ));
    }
    let mut out = table.clone();
    out.dice = Some(fit.dice.clone());
    for result in &mut out.results {
        result.range = entries
            .iter()
            .position(|e| e == &result.id)
            .map(|i| fit.ranges[i]);
    }
    let existing = out.results.iter().position(is_roll_again);
    match (fit.roll_again, existing) {
        (Some(range), Some(i)) => out.results[i].range = Some(range),
        (Some(range), None) => out.results.push(TableResult {
            id: roll_again_id,
            name: ROLL_AGAIN_NAME.to_owned(),
            shared: None,
            description: ROLL_AGAIN_NOTE.to_owned(),
            range: Some(range),
            tags: Vec::new(),
            eligibility: None,
            enabled: false,
            generator: None,
            value: None,
            unique: false,
            fallback: None,
            label_variants: Vec::new(),
            then_roll: Vec::new(),
        }),
        (None, Some(i)) => {
            out.results.remove(i);
        }
        (None, None) => {}
    }
    for entry in entries {
        if out.result(entry).is_none() {
            return Err(format!("The table has no result `{entry}`."));
        }
    }
    Ok(out)
}

/// One place that depends on a table or a result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Usage {
    /// Plain-English description, e.g. "Pipeline stage “Size”".
    pub description: String,
    /// The item that depends on it, so an interface can link to it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub table: Option<TableId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tag: Option<crate::ids::TagId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub flow: Option<crate::ids::FlowId>,
}

impl Usage {
    fn new(description: String) -> Self {
        Usage {
            description,
            table: None,
            tag: None,
            flow: None,
        }
    }
}

/// Every active place that rolls, chooses, redirects to or otherwise names
/// `table`. Planned rules are design notes and are not listed.
pub fn table_usages(content: &ContentSet, table: &TableId) -> Vec<Usage> {
    let mut out = Vec::new();
    for flow in &content.flows {
        for step in flow
            .all_steps()
            .into_iter()
            .filter(|s| s.status.is_active())
        {
            let uses = match &step.action {
                StepAction::Roll { table: t, .. } | StepAction::SetResult { table: t, .. } => {
                    t == table
                }
                StepAction::Builtin { handler, config } => {
                    Builtin::parse(handler, config).is_ok_and(|b| b.tables().contains(&table))
                }
                _ => false,
            };
            if uses {
                let label = step.name.clone().unwrap_or_else(|| step.id.to_string());
                let what = if content.start.as_ref() == Some(&flow.id) {
                    format!("Pipeline stage “{label}”")
                } else {
                    format!("Step “{label}” of flow “{}”", flow.name)
                };
                out.push(Usage {
                    flow: Some(flow.id.clone()),
                    ..Usage::new(what)
                });
            }
        }
    }
    if content.start.is_none() && content.sequence_tables().contains(table) {
        out.push(Usage::new("The generation pipeline".into()));
    }
    for tag in &content.tags {
        for def in tag.effects.iter().filter(|d| d.is_active()) {
            let target = match &def.effect {
                Effect::RedirectTable { to, .. } if to == table => true,
                effect => effect.target_table() == Some(table),
            };
            if target {
                out.push(Usage {
                    tag: Some(tag.id.clone()),
                    ..Usage::new(format!("An effect of tag “{}”", tag.name))
                });
                break;
            }
        }
    }
    for other in &content.tables {
        if other.id != *table && other.results.iter().any(|r| r.then_roll.contains(table)) {
            out.push(Usage {
                table: Some(other.id.clone()),
                ..Usage::new(format!("A result of table “{}” rolls it next", other.name))
            });
        }
    }
    for scheme in &content.naming_schemes {
        let named = &scheme.method_table == table
            || scheme.methods.iter().any(|m| m.components.contains(table));
        if named {
            out.push(Usage::new(format!("Naming scheme “{}”", scheme.name)));
        }
    }
    for scheme in &content.role_schemes {
        if [&scheme.single, &scheme.lead, &scheme.member]
            .into_iter()
            .flatten()
            .any(|t| t == table)
        {
            out.push(Usage::new(format!("Role scheme “{}”", scheme.name)));
        }
    }
    for scheme in &content.location_schemes {
        if &scheme.count == table || scheme.districts.as_ref() == Some(table) {
            out.push(Usage::new(format!("Location scheme “{}”", scheme.name)));
        }
    }
    out
}

fn condition_names(condition: &Option<Condition>, result: &ResultId) -> bool {
    condition.as_ref().is_some_and(|c| {
        let mut found = false;
        c.visit(&mut |c| {
            if let Condition::ResultIs { results, .. } = c
                && results.contains(result)
            {
                found = true;
            }
        });
        found
    })
}

/// Every active place that names result `result` of `table`, in the rest of
/// the content and in the table itself (follow-up rolls, fallbacks,
/// variants).
pub fn result_usages(content: &ContentSet, table: &Table, result: &ResultId) -> Vec<Usage> {
    let mut out = Vec::new();
    let named = |r: &TableResult| r.name.clone();
    for other in table.results.iter().filter(|r| &r.id != result) {
        if other.fallback.as_ref() == Some(result) {
            out.push(Usage::new(format!(
                "Result “{}” becomes it when already held",
                named(other)
            )));
        }
    }
    for follow_up in &table.follow_ups {
        let uses = follow_up.on_results.contains(result)
            || follow_up
                .outcomes
                .iter()
                .any(|o| o.becomes.as_ref() == Some(result));
        if uses {
            out.push(Usage::new(format!("Follow-up roll “{}”", follow_up.name)));
        }
    }
    for variant in &table.variants {
        if variant.ranges.iter().flatten().any(|a| &a.result == result) {
            out.push(Usage::new(format!("Variant “{}”", variant.name)));
        }
    }
    for tag in &content.tags {
        for def in tag.effects.iter().filter(|d| d.is_active()) {
            if let Effect::RestrictResults {
                table: t,
                only,
                exclude,
            } = &def.effect
                && t == &table.id
                && (only.iter().flatten().any(|r| r == result) || exclude.contains(result))
            {
                out.push(Usage {
                    tag: Some(tag.id.clone()),
                    ..Usage::new(format!("A restriction of tag “{}”", tag.name))
                });
            }
        }
    }
    for scheme in &content.naming_schemes {
        let method_result =
            scheme.method_table == table.id && scheme.methods.iter().any(|m| &m.result == result);
        let in_names = scheme.methods.iter().any(|m| {
            m.components.contains(&table.id)
                && (m.exceptions.iter().any(|e| e.results.contains(result))
                    || matches!(&m.join.style, JoinStyle::Possessive { before: Some(b), .. } if b.contains(result)))
        });
        if method_result || in_names {
            out.push(Usage::new(format!("Naming scheme “{}”", scheme.name)));
        }
    }
    for flow in &content.flows {
        for step in flow
            .all_steps()
            .into_iter()
            .filter(|s| s.status.is_active())
        {
            let sets = matches!(&step.action,
                StepAction::SetResult { table: t, result: r, .. } if t == &table.id && r == result);
            let builtin = matches!(&step.action, StepAction::Builtin { handler, config }
                if Builtin::parse(handler, config).is_ok_and(|b| matches!(b,
                    Builtin::Locations(c) if c.type_table == table.id
                        && c.guaranteed.as_ref().is_some_and(|g| &g.location_type == result))));
            let tests = condition_names(&step.when, result);
            if sets || builtin || tests {
                let label = step.name.clone().unwrap_or_else(|| step.id.to_string());
                out.push(Usage {
                    flow: Some(flow.id.clone()),
                    ..Usage::new(format!("Step “{label}” of flow “{}”", flow.name))
                });
            }
        }
    }
    out.dedup();
    out
}
