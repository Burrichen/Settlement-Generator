//! Test runs of a table or a flow: roll once, step through, or run it many
//! times and compare what came up with what the dice should give. These
//! are previews with software dice; nothing is saved and generation itself
//! still uses physical dice.

use std::collections::BTreeMap;
use std::sync::Arc;

use rand::rngs::StdRng;
use rand::{RngExt, SeedableRng};
use serde::{Deserialize, Serialize};

use crate::content::{ContentSet, SelectionMode};
use crate::dice::{FaceSource, RandomFaces};
use crate::engine::{
    AppliedModifier, GenerationEvent, GenerationOptions, Generator, Named, Selections,
    TableRollRecord,
};
use crate::framework::describe::Describer;
use crate::framework::runner::{FlowRunner, Memory};
use crate::framework::{Flow, StepAction, StepOutcome, StepRecord};
use crate::ids::{ResultId, TableId, TagId};
use crate::session::{GenerationMode, GenerationSession, StageStatus, ViewOptions};
use crate::trace::{RollTrace, trace_roll};
use crate::validation::{IssueKind, ValidationIssue, validate};

/// Most runs one simulation may make.
pub const MAX_RUNS: u32 = 10_000;

/// One outcome and how often it came up.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OutcomeStat {
    pub id: String,
    pub name: String,
    pub count: u32,
    /// Share of the runs, 0–100.
    pub actual: f64,
    /// What the dice should give, 0–100 (tables only).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected: Option<f64>,
}

/// A modifier and how many runs it shaped.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModifierUse {
    pub label: String,
    pub count: u32,
}

/// An error and how many runs it stopped.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ErrorCount {
    pub message: String,
    pub count: u32,
}

/// Many rolls of one table.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TableSimulation {
    pub runs: u32,
    pub table: Named<TableId>,
    pub results: Vec<OutcomeStat>,
    /// Results that can never come up now (no faces, disabled, excluded or
    /// ineligible with these tags).
    pub unreachable: Vec<Named<ResultId>>,
    /// Rolls made again (results that could not be had), on average.
    pub rerolls: f64,
    pub modifiers: Vec<ModifierUse>,
    pub errors: Vec<ErrorCount>,
    pub issues: Vec<ValidationIssue>,
    pub note: String,
}

/// What one step (or stage) gave over many runs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StepStat {
    pub step: String,
    pub label: String,
    /// Runs in which the step ran at all.
    pub reached: u32,
    pub outcomes: Vec<OutcomeStat>,
}

/// How often each case of a condition was taken.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BranchStat {
    pub step: String,
    pub label: String,
    pub cases: Vec<OutcomeStat>,
}

/// Many runs of one flow.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FlowSimulation {
    pub runs: u32,
    pub steps: Vec<StepStat>,
    pub branches: Vec<BranchStat>,
    /// Cases never taken, and conditions validation proves impossible.
    pub impossible: Vec<String>,
    pub modifiers: Vec<ModifierUse>,
    pub errors: Vec<ErrorCount>,
    pub issues: Vec<ValidationIssue>,
    pub note: String,
}

/// One run of a flow, to roll once or step through.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FlowPreview {
    pub steps: Vec<StepRecord>,
    /// The run as indented lines, in order (what Step Through reveals).
    pub outline: Vec<crate::framework::OutlineLine>,
    /// Every table roll, explained, in order.
    pub traces: Vec<RollTrace>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

fn dice(seed: Option<u64>) -> RandomFaces<StdRng> {
    match seed {
        Some(s) => RandomFaces::seeded(s),
        None => RandomFaces::new(StdRng::seed_from_u64(rand::random())),
    }
}

/// A result of every choose-only table, picked at random for each run.
fn random_choices(content: &ContentSet, rng: &mut StdRng) -> Selections {
    let mut out = Selections::new();
    for table in content
        .tables
        .iter()
        .filter(|t| t.selection == SelectionMode::Choose)
    {
        let enabled: Vec<&ResultId> = table
            .results
            .iter()
            .filter(|r| r.enabled)
            .map(|r| &r.id)
            .collect();
        if !enabled.is_empty() {
            let i = rng.random_range(0..enabled.len());
            out.insert(table.id.clone(), enabled[i].clone());
        }
    }
    out
}

fn modifier_label(m: &AppliedModifier, table: &str) -> String {
    let source = match (&m.tag, &m.step) {
        (Some(tag), _) => tag.name.clone(),
        (None, Some(step)) => format!("step “{step}”"),
        (None, None) => "a rule".into(),
    };
    let what = match m.effect.modifier() {
        Some(modifier) => modifier.to_string(),
        None => serde_json::to_value(&m.effect)
            .ok()
            .and_then(|v| v["kind"].as_str().map(|k| k.replace('_', " ")))
            .unwrap_or_else(|| "effect".into()),
    };
    format!("{what} on {table} ({source})")
}

/// Counts by label, in first-seen order.
#[derive(Default)]
struct Tally {
    order: Vec<(String, String)>,
    counts: BTreeMap<String, u32>,
}

impl Tally {
    fn add(&mut self, id: &str, name: &str) {
        if !self.counts.contains_key(id) {
            self.order.push((id.to_owned(), name.to_owned()));
        }
        *self.counts.entry(id.to_owned()).or_default() += 1;
    }

    fn stats(&self, runs: u32) -> Vec<OutcomeStat> {
        self.order
            .iter()
            .map(|(id, name)| {
                let count = self.counts[id];
                OutcomeStat {
                    id: id.clone(),
                    name: name.clone(),
                    count,
                    actual: percent(count, runs),
                    expected: None,
                }
            })
            .collect()
    }

    fn uses(&self) -> Vec<ModifierUse> {
        let mut out: Vec<ModifierUse> = self
            .order
            .iter()
            .map(|(id, _)| ModifierUse {
                label: id.clone(),
                count: self.counts[id],
            })
            .collect();
        out.sort_by_key(|m| std::cmp::Reverse(m.count));
        out
    }

    fn errors(&self) -> Vec<ErrorCount> {
        self.order
            .iter()
            .map(|(id, _)| ErrorCount {
                message: id.clone(),
                count: self.counts[id],
            })
            .collect()
    }
}

fn percent(count: u32, runs: u32) -> f64 {
    if runs == 0 {
        0.0
    } else {
        f64::from(count) * 100.0 / f64::from(runs)
    }
}

/// The main record among a roll's events (not one a tag routed to).
fn main_record(events: &[GenerationEvent]) -> Option<&TableRollRecord> {
    events.iter().find_map(|e| match e {
        GenerationEvent::TableRolled(r) if !r.is_routed() => Some(&**r),
        _ => None,
    })
}

/// Rolls `table` (already in `content`) `runs` times with `tags` active.
pub fn simulate_table(
    content: &ContentSet,
    table: &TableId,
    runs: u32,
    tags: &[TagId],
    seed: Option<u64>,
) -> Result<TableSimulation, String> {
    let runs = runs.clamp(1, MAX_RUNS);
    let generator = Generator::new(content).map_err(|e| e.to_string())?;
    let (used, expected) = generator
        .expected_chances(table, tags)
        .map_err(|e| e.to_string())?;
    let def = content
        .table(&used.id)
        .ok_or_else(|| format!("There is no table `{}`.", used.id))?;
    let mut faces = dice(seed);
    let mut pick = StdRng::seed_from_u64(seed.unwrap_or_else(rand::random) ^ 0x5eed);
    let mut results = Tally::default();
    let mut modifiers = Tally::default();
    let mut errors = Tally::default();
    let mut rerolls = 0u64;
    for _ in 0..runs {
        let selections = random_choices(content, &mut pick);
        match generator.roll_with_tags(table, tags, &selections, &mut faces) {
            Ok(events) => match main_record(&events) {
                Some(record) => {
                    results.add(record.result.id.as_str(), &record.result.name);
                    rerolls += record.attempts.len().saturating_sub(1) as u64;
                    for m in record.modifiers.iter().filter(|m| !m.ignored) {
                        let label = modifier_label(m, &record.table.name);
                        modifiers.add(&label, &label);
                    }
                }
                None => errors.add("The table was skipped", ""),
            },
            Err(e) => {
                let message = e.to_string();
                errors.add(&message, "");
            }
        }
    }
    // Every result, in table order, with its expected and actual share.
    let stats = results.stats(runs);
    let results_out: Vec<OutcomeStat> = def
        .results
        .iter()
        .map(|r| {
            let count = stats
                .iter()
                .find(|s| s.id == r.id.as_str())
                .map_or(0, |s| s.count);
            OutcomeStat {
                id: r.id.to_string(),
                name: r.name.clone(),
                count,
                actual: percent(count, runs),
                expected: Some(
                    expected
                        .iter()
                        .find(|(id, _)| id == &r.id)
                        .map_or(0.0, |(_, p)| p * 100.0),
                ),
            }
        })
        .collect();
    let unreachable = results_out
        .iter()
        .filter(|s| s.expected == Some(0.0) && s.count == 0)
        .map(|s| Named {
            id: ResultId::new(&s.id),
            name: s.name.clone(),
        })
        .collect();
    let issues = validate(content)
        .issues
        .into_iter()
        .filter(|i| i.location.table.as_ref() == Some(&used.id))
        .collect();
    Ok(TableSimulation {
        runs,
        table: used,
        results: results_out,
        unreachable,
        rerolls: rerolls as f64 / f64::from(runs),
        modifiers: modifiers.uses(),
        errors: errors.errors(),
        issues,
        note: format!(
            "{runs} test rolls with software dice{}. Expected chances come from the dice and the table's rules (follow-up rolls are not included). Nothing is saved.",
            if tags.is_empty() {
                ", with no tags active".to_owned()
            } else {
                format!(", with {} tag(s) active", tags.len())
            }
        ),
    })
}

/// Statistics gathered from run records.
#[derive(Default)]
struct FlowTally {
    steps: Vec<(String, String)>,
    reached: BTreeMap<String, u32>,
    outcomes: BTreeMap<String, Tally>,
    cases: BTreeMap<String, Tally>,
    modifiers: Tally,
}

impl FlowTally {
    fn step(&mut self, id: &str, label: &str) -> &mut Tally {
        if !self.reached.contains_key(id) {
            self.steps.push((id.to_owned(), label.to_owned()));
        }
        *self.reached.entry(id.to_owned()).or_default() += 1;
        self.outcomes.entry(id.to_owned()).or_default()
    }

    fn walk(&mut self, record: &StepRecord, case_names: &BTreeMap<String, Vec<String>>) {
        let id = record.step.to_string();
        match &record.outcome {
            StepOutcome::Roll { events, .. } => {
                if let Some(main) = main_record(events) {
                    self.step(&id, &record.label)
                        .add(main.result.id.as_str(), &main.result.name);
                }
                for e in events {
                    if let GenerationEvent::TableRolled(r) = e {
                        for m in r.modifiers.iter().filter(|m| !m.ignored) {
                            let label = modifier_label(m, &r.table.name);
                            self.modifiers.add(&label, &label);
                        }
                    }
                }
            }
            StepOutcome::Branch { case, .. } => {
                let names = case_names.get(&id);
                let (key, name) = match case {
                    Some(i) => (
                        format!("case-{i}"),
                        names
                            .and_then(|n| n.get(*i))
                            .cloned()
                            .unwrap_or_else(|| format!("Case {}", i + 1)),
                    ),
                    None => ("otherwise".into(), "Otherwise".into()),
                };
                self.step(&id, &record.label);
                self.cases.entry(id.clone()).or_default().add(&key, &name);
            }
            StepOutcome::Skipped { .. } => {
                self.step(&id, &record.label).add("(skipped)", "(skipped)");
            }
            _ => {
                let summary = record.summary();
                self.step(&id, &record.label).add(&summary, &summary);
            }
        }
        for child in record.children() {
            self.walk(child, case_names);
        }
    }
}

/// "IF …" words for each case of every condition step of every flow.
fn case_names(content: &ContentSet) -> BTreeMap<String, Vec<String>> {
    let mut out = BTreeMap::new();
    for flow in &content.flows {
        let describer = Describer::new(content, Some(flow));
        for step in flow.all_steps() {
            if let StepAction::Branch { cases, .. } = &step.action {
                out.entry(step.id.to_string()).or_insert_with(|| {
                    cases
                        .iter()
                        .enumerate()
                        .map(|(i, c)| format!("Case {}: {}", i + 1, describer.condition(&c.when)))
                        .collect()
                });
            }
        }
    }
    out
}

/// Runs `flow` (already in `content`) `runs` times. The content's start
/// flow is run as whole generations (every stage, built-in ones included);
/// any other flow is run on its own, with nothing stored before it.
pub fn simulate_flow(
    content: &Arc<ContentSet>,
    flow: &Flow,
    runs: u32,
    seed: Option<u64>,
) -> Result<FlowSimulation, String> {
    let runs = runs.clamp(1, MAX_RUNS);
    let generator = Generator::new(content).map_err(|e| e.to_string())?;
    let names = case_names(content);
    let mut faces = dice(seed);
    let mut pick = StdRng::seed_from_u64(seed.unwrap_or_else(rand::random) ^ 0x5eed);
    let mut tally = FlowTally::default();
    let mut errors = Tally::default();
    let start = content.start.as_ref() == Some(&flow.id);
    for _ in 0..runs {
        let selections = random_choices(content, &mut pick);
        if start {
            if let Err(e) = run_generation(content, &selections, &mut faces, &mut tally, &names) {
                errors.add(&e, "");
            }
        } else {
            let mut runner = FlowRunner::new(&generator, &selections, Memory::default());
            let mut state = Default::default();
            match runner.run_steps(&flow.steps, &mut state, &mut faces) {
                Ok(records) => records.iter().for_each(|r| tally.walk(r, &names)),
                Err(e) => errors.add(&e.to_string(), ""),
            }
        }
    }
    let steps = tally
        .steps
        .iter()
        .map(|(id, label)| StepStat {
            step: id.clone(),
            label: label.clone(),
            reached: tally.reached[id],
            outcomes: tally.outcomes[id].stats(tally.reached[id]),
        })
        .collect();
    // Every case of every condition the runs reached, never-taken ones too.
    let mut branches = Vec::new();
    let mut impossible = Vec::new();
    for (id, label) in &tally.steps {
        let Some(taken) = tally.cases.get(id) else {
            continue;
        };
        let reached = tally.reached[id];
        let mut cases = taken.stats(reached);
        for (i, name) in names.get(id).into_iter().flatten().enumerate() {
            let key = format!("case-{i}");
            if !cases.iter().any(|c| c.id == key) {
                impossible.push(format!("“{label}” › {name}: never taken in {runs} runs"));
                cases.push(OutcomeStat {
                    id: key,
                    name: name.clone(),
                    count: 0,
                    actual: 0.0,
                    expected: None,
                });
            }
        }
        branches.push(BranchStat {
            step: id.clone(),
            label: label.clone(),
            cases,
        });
    }
    let report = validate(content);
    let flow_id = flow.id.to_string();
    for issue in &report.issues {
        if issue.location.flow.as_deref() == Some(flow_id.as_str())
            && matches!(
                issue.kind,
                IssueKind::ImpossibleCondition { .. } | IssueKind::UnreachableStep { .. }
            )
        {
            impossible.push(issue.message.clone());
        }
    }
    Ok(FlowSimulation {
        runs,
        steps,
        branches,
        impossible,
        modifiers: tally.modifiers.uses(),
        errors: errors.errors(),
        issues: report
            .issues
            .into_iter()
            .filter(|i| i.location.flow.as_deref() == Some(flow_id.as_str()))
            .collect(),
        note: if start {
            format!(
                "{runs} whole test generations with software dice; tables that are chosen get a random choice. Nothing is saved."
            )
        } else {
            format!(
                "{runs} test runs of this flow on its own, with software dice; earlier stages are not run, so tests of their results do not hold. Nothing is saved."
            )
        },
    })
}

/// One whole generation, tallied stage by stage.
fn run_generation(
    content: &Arc<ContentSet>,
    selections: &Selections,
    faces: &mut dyn FaceSource,
    tally: &mut FlowTally,
    names: &BTreeMap<String, Vec<String>>,
) -> Result<(), String> {
    let mut session = GenerationSession::from_validated(
        content.clone(),
        GenerationOptions::default(),
        GenerationMode::Automatic,
    )
    .map_err(|e| e.to_string())?;
    session
        .run_automatic(selections, faces)
        .map_err(|e| e.to_string())?;
    let options = ViewOptions {
        show_tags: false,
        debug: false,
    };
    for view in session.views(options).map_err(|e| e.to_string())? {
        let id = view.stage.id.to_string();
        match (&view.status, &view.result) {
            (StageStatus::Skipped, _) => tally
                .step(&id, &view.stage.name)
                .add("(skipped)", "(skipped)"),
            (_, Some(result)) => tally
                .step(&id, &view.stage.name)
                .add(result.id.as_str(), &result.name),
            _ => {}
        }
        if let Some(record) = view.flow.and_then(|f| f.record) {
            for child in record.children() {
                tally.walk(child, names);
            }
        }
    }
    for event in &session.to_snapshot().log.events {
        if let GenerationEvent::TableRolled(r) = event {
            for m in r.modifiers.iter().filter(|m| !m.ignored) {
                let label = modifier_label(m, &r.table.name);
                tally.modifiers.add(&label, &label);
            }
        }
    }
    Ok(())
}

/// One run of `flow`, with every roll explained (Roll Once and Step
/// Through).
pub fn preview_flow(content: &ContentSet, flow: &Flow, seed: Option<u64>) -> FlowPreview {
    let generator = match Generator::new(content) {
        Ok(g) => g,
        Err(e) => {
            return FlowPreview {
                outline: Vec::new(),
                steps: Vec::new(),
                traces: Vec::new(),
                error: Some(e.to_string()),
            };
        }
    };
    let mut pick = StdRng::seed_from_u64(seed.unwrap_or_else(rand::random) ^ 0x5eed);
    let selections = random_choices(content, &mut pick);
    let mut faces = dice(seed);
    let mut runner = FlowRunner::new(&generator, &selections, Memory::default());
    let mut state = Default::default();
    // Built-in stages only run in a whole generation; skip them here.
    let steps: Vec<_> = flow
        .steps
        .iter()
        .filter(|s| {
            !matches!(
                s.action,
                StepAction::Builtin { .. } | StepAction::Name { .. }
            )
        })
        .cloned()
        .collect();
    match runner.run_steps(&steps, &mut state, &mut faces) {
        Ok(records) => FlowPreview {
            outline: records.iter().flat_map(StepRecord::outline).collect(),
            traces: records
                .iter()
                .flat_map(|r| r.table_rolls())
                .map(|r| trace_roll(content, r))
                .collect(),
            steps: records,
            error: None,
        },
        Err(e) => FlowPreview {
            outline: Vec::new(),
            steps: Vec::new(),
            traces: Vec::new(),
            error: Some(e.to_string()),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn content() -> ContentSet {
        serde_json::from_value(serde_json::json!({
            "tables": [
                { "id": "table.coin", "name": "Coin", "dice": "1d4", "results": [
                    { "id": "heads", "name": "Heads", "range": [1, 3] },
                    { "id": "tails", "name": "Tails", "range": 4 },
                    { "id": "edge", "name": "Edge", "enabled": false } ] }
            ]
        }))
        .unwrap()
    }

    #[test]
    fn many_rolls_match_the_expected_chances() {
        let c = content();
        let sim = simulate_table(&c, &"table.coin".into(), 10_000, &[], Some(3)).unwrap();
        assert_eq!(sim.runs, 10_000);
        let heads = &sim.results[0];
        assert_eq!(heads.expected, Some(75.0));
        assert!((heads.actual - 75.0).abs() < 2.0, "{}", heads.actual);
        assert_eq!(sim.unreachable.len(), 1);
        assert_eq!(sim.unreachable[0].name, "Edge");
        assert!(sim.errors.is_empty());
    }
}
