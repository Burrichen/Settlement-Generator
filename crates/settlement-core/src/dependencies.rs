//! What depends on a tag or a setting: every place that emits, checks,
//! adds, removes or reads it, in plain words with links. Used for
//! dependency inspection in the editors and to refuse unsafe deletions.

use crate::content::{Condition, ContentSet, Effect};
use crate::editor::{TagUsage, UsageKind};
use crate::framework::{Flow, FlowStep, StepAction};
use crate::ids::{SettingId, TagId};
use crate::table_editor::Usage;
use crate::validation::Location;

/// A place a condition sits, with words for it.
struct Site<'c> {
    condition: &'c Condition,
    description: String,
    location: Location,
}

/// How a flow step is named: a pipeline stage, or a step of a flow.
fn step_place(content: &ContentSet, flow: &Flow, step: &FlowStep) -> String {
    let label = step.name.clone().unwrap_or_else(|| step.id.to_string());
    if content.start.as_ref() == Some(&flow.id) {
        format!("Pipeline stage “{label}”")
    } else {
        format!("Flow “{}” › step “{label}”", flow.name)
    }
}

fn step_location(flow: &Flow, step: &FlowStep) -> Location {
    Location {
        flow: Some(flow.id.to_string()),
        ..Default::default()
    }
    .with_step(&step.id)
}

/// Every active rule's condition in `content`, with where it is.
fn condition_sites(content: &ContentSet) -> Vec<Site<'_>> {
    let mut out = Vec::new();
    for table in &content.tables {
        let loc = Location::table(&table.id);
        if let Some(c) = &table.eligibility {
            out.push(Site {
                condition: c,
                description: format!("Table “{}” (when it can be rolled)", table.name),
                location: loc.clone(),
            });
        }
        for result in &table.results {
            if let Some(c) = &result.eligibility {
                out.push(Site {
                    condition: c,
                    description: format!("{} › {} (when it can come up)", table.name, result.name),
                    location: loc.clone().with_result(&result.id),
                });
            }
        }
        for variant in &table.variants {
            if let Some(c) = &variant.when {
                out.push(Site {
                    condition: c,
                    description: format!("{} › variant “{}”", table.name, variant.name),
                    location: loc.clone().with_variant(Some(&variant.id)),
                });
            }
        }
        for follow_up in &table.follow_ups {
            if let Some(c) = &follow_up.when {
                out.push(Site {
                    condition: c,
                    description: format!("{} › follow-up “{}”", table.name, follow_up.name),
                    location: loc.clone().with_follow_up(&follow_up.id),
                });
            }
        }
    }
    for flow in &content.flows {
        for step in flow.all_steps() {
            if !step.status.is_active() {
                continue;
            }
            let place = step_place(content, flow, step);
            let loc = step_location(flow, step);
            if let Some(c) = &step.when {
                out.push(Site {
                    condition: c,
                    description: format!("{place} (when it runs)"),
                    location: loc.clone(),
                });
            }
            match &step.action {
                StepAction::Branch { cases, .. } => {
                    for (i, case) in cases.iter().enumerate() {
                        out.push(Site {
                            condition: &case.when,
                            description: format!("{place} › case {}", i + 1),
                            location: loc.clone(),
                        });
                    }
                }
                StepAction::Roll { modifiers, .. } => {
                    for (i, m) in modifiers.iter().enumerate() {
                        if let Some(c) = &m.when {
                            out.push(Site {
                                condition: c,
                                description: format!("{place} › modifier {}", i + 1),
                                location: loc.clone(),
                            });
                        }
                    }
                }
                _ => {}
            }
        }
    }
    out
}

/// Built-in stages whose configuration names `needle` (an ID).
fn builtin_mentions<'c>(
    content: &'c ContentSet,
    needles: &[&str],
) -> Vec<(&'c Flow, &'c FlowStep)> {
    let mut out = Vec::new();
    for flow in &content.flows {
        for step in flow.all_steps() {
            if let StepAction::Builtin { config, .. } = &step.action {
                let text = config.to_string();
                if needles.iter().any(|n| text.contains(&format!("\"{n}\""))) {
                    out.push((flow, step));
                }
            }
        }
    }
    out
}

/// Everywhere `tag` is used: results that give it, rules that check it,
/// tags and steps that add or remove it.
pub fn tag_usages(content: &ContentSet, tag: &TagId) -> Vec<TagUsage> {
    let mut out = Vec::new();
    let mut push = |kind: UsageKind, location: Location, description: String| {
        out.push(TagUsage {
            kind,
            location,
            description,
        });
    };
    for table in &content.tables {
        for result in &table.results {
            if result.tags.contains(tag) {
                push(
                    UsageKind::EmittedBy,
                    Location::table(&table.id).with_result(&result.id),
                    format!("Given by {} › {}", table.name, result.name),
                );
            }
        }
    }
    for site in condition_sites(content) {
        if site.condition.tags().contains(&tag) {
            push(
                UsageKind::CheckedBy,
                site.location,
                format!("Checked by {}", site.description),
            );
        }
    }
    for other in &content.tags {
        for (index, def) in other.effects.iter().enumerate() {
            if def.is_active() && matches!(&def.effect, Effect::AddTag { tag: t } if t == tag) {
                push(
                    UsageKind::AddedBy,
                    Location::tag(&other.id).with_effect(index),
                    format!("Added by tag “{}”", other.name),
                );
            }
        }
    }
    for flow in &content.flows {
        for step in flow
            .all_steps()
            .into_iter()
            .filter(|s| s.status.is_active())
        {
            let (kind, verb) = match &step.action {
                StepAction::AddTag { tag: t } if t == tag => (UsageKind::AddedBy, "Added by"),
                StepAction::RemoveTag { tag: t } if t == tag => {
                    (UsageKind::RemovedBy, "Removed by")
                }
                _ => continue,
            };
            push(
                kind,
                step_location(flow, step),
                format!("{verb} {}", step_place(content, flow, step)),
            );
        }
    }
    for (flow, step) in builtin_mentions(content, &[tag.as_str()]) {
        push(
            UsageKind::CheckedBy,
            step_location(flow, step),
            format!(
                "Named in the settings of {}",
                step_place(content, flow, step)
            ),
        );
    }
    out
}

/// Everything that runs flow `id`: Run Subflow steps, tags that trigger
/// it, and the pipeline itself when it is the start flow.
pub fn flow_usages(content: &ContentSet, id: &crate::ids::FlowId) -> Vec<Usage> {
    let mut out = Vec::new();
    if content.start.as_ref() == Some(id) {
        out.push(Usage {
            description: "The generation pipeline (it is the start flow)".into(),
            table: None,
            tag: None,
            flow: None,
        });
    }
    for flow in &content.flows {
        for step in flow
            .all_steps()
            .into_iter()
            .filter(|s| s.status.is_active())
        {
            if let StepAction::RunFlow { flow: target, .. } = &step.action
                && target == id
            {
                out.push(Usage {
                    description: format!("Run by {}", step_place(content, flow, step)),
                    table: None,
                    tag: None,
                    flow: Some(flow.id.clone()),
                });
            }
        }
    }
    for tag in &content.tags {
        if tag
            .effects
            .iter()
            .any(|d| d.is_active() && matches!(&d.effect, Effect::RunFlow { flow } if flow == id))
        {
            out.push(Usage {
                description: format!("Triggered by tag “{}”", tag.name),
                table: None,
                tag: Some(tag.id.clone()),
                flow: None,
            });
        }
    }
    out
}

/// Everywhere setting `id` is used: rules that test it, steps that read it
/// and built-in stages configured with it.
pub fn setting_usages(content: &ContentSet, id: &SettingId) -> Vec<Usage> {
    let canonical = crate::framework::canonical_setting(id);
    let names_it = |s: &SettingId| crate::framework::canonical_setting(s) == canonical;
    let mut out = Vec::new();
    let usage = |description: String, location: &Location| Usage {
        description,
        table: location.table.clone(),
        tag: None,
        flow: location.flow.as_deref().map(crate::ids::FlowId::new),
    };
    for site in condition_sites(content) {
        if site.condition.settings().into_iter().any(names_it) {
            out.push(usage(
                format!("Tested by {}", site.description),
                &site.location,
            ));
        }
    }
    for flow in &content.flows {
        for step in flow
            .all_steps()
            .into_iter()
            .filter(|s| s.status.is_active())
        {
            if let StepAction::ReadSetting { setting } = &step.action
                && names_it(setting)
            {
                out.push(usage(
                    format!("Read by {}", step_place(content, flow, step)),
                    &step_location(flow, step),
                ));
            }
        }
    }
    let mut needles = vec![canonical.as_str().to_owned()];
    if canonical.as_str() == crate::framework::BARE_MINIMUM_BUILDINGS {
        needles.push(crate::framework::BARE_MINIMUM_BUILDINGS_SETTING.to_owned());
    }
    let needles: Vec<&str> = needles.iter().map(String::as_str).collect();
    for (flow, step) in builtin_mentions(content, &needles) {
        out.push(usage(
            format!("Used by {}", step_place(content, flow, step)),
            &step_location(flow, step),
        ));
    }
    out
}
