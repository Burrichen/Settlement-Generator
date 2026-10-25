//! Test a table or flow: Roll once, Step through (one line at a time), and
//! Run 100 / 1,000 / 10,000 with what came up compared to what the dice
//! should give. Software dice; nothing is saved.

use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use settlement_core::content::Table;
use settlement_core::framework::Flow;
use settlement_core::simulate::{ErrorCount, ModifierUse, OutcomeStat};
use settlement_core::trace::RollTrace;

use crate::tui::widgets::{Row, RowsState};
use crate::tui::{Ctx, Nav, Screen};

pub enum Target {
    Table(Table),
    Flow(Flow),
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Act {
    Once,
    Step,
    Next,
    Runs(u32),
}

pub struct TestScreen {
    target: Target,
    state: RowsState,
    /// What the last action showed.
    lines: Vec<Line<'static>>,
    /// Step through: how many of `lines` are shown.
    shown: Option<usize>,
    error: Option<String>,
}

impl TestScreen {
    pub fn new(target: Target) -> Self {
        TestScreen {
            target,
            state: RowsState::default(),
            lines: Vec::new(),
            shown: None,
            error: None,
        }
    }

    fn name(&self) -> &str {
        match &self.target {
            Target::Table(t) => &t.name,
            Target::Flow(f) => &f.name,
        }
    }

    fn rows(&self, ctx: &Ctx) -> Vec<Row<Act>> {
        let s = &ctx.styles;
        let mut rows = vec![
            Row::info(Line::styled(
                "Software dice: nothing is saved, and real generation still uses your dice.",
                s.dim(),
            )),
            Row::item(Line::raw("Roll once"), Act::Once),
            Row::item(Line::raw("Step through"), Act::Step),
            Row::item(Line::raw("Run 100"), Act::Runs(100)),
            Row::item(Line::raw("Run 1,000"), Act::Runs(1000)),
            Row::item(Line::raw("Run 10,000"), Act::Runs(10_000)),
        ];
        if let Some(e) = &self.error {
            rows.push(Row::info(Line::styled(e.clone(), s.error())));
        }
        if !self.lines.is_empty() {
            rows.push(Row::blank());
        }
        let shown = self.shown.unwrap_or(self.lines.len()).min(self.lines.len());
        rows.extend(self.lines[..shown].iter().cloned().map(Row::info));
        if self.shown.is_some_and(|n| n < self.lines.len()) {
            rows.push(Row::item(Line::styled("Next step  ›", s.key()), Act::Next));
        }
        rows
    }

    fn act(&mut self, act: Act, ctx: &mut Ctx) {
        self.error = None;
        match act {
            Act::Once | Act::Step => match self.once(ctx) {
                Ok(lines) => {
                    self.lines = lines;
                    self.shown = (act == Act::Step).then_some(1);
                }
                Err(e) => self.error = Some(e),
            },
            Act::Next => {
                self.shown = self.shown.map(|n| n + 1);
            }
            Act::Runs(n) => {
                let result = match &self.target {
                    Target::Table(t) => ctx
                        .service
                        .simulate_table(t, n, &[])
                        .map(|sim| table_lines(&sim, ctx)),
                    Target::Flow(f) => ctx
                        .service
                        .simulate_flow(f, n)
                        .map(|sim| flow_lines(&sim, ctx)),
                };
                match result {
                    Ok(lines) => {
                        self.lines = lines;
                        self.shown = None;
                    }
                    Err(e) => self.error = Some(e),
                }
            }
        }
    }

    fn once(&self, ctx: &mut Ctx) -> Result<Vec<Line<'static>>, String> {
        let s = &ctx.styles;
        match &self.target {
            Target::Table(t) => {
                let preview = ctx.service.preview_roll(t.clone())?;
                Ok(trace_lines(&preview.traces, ctx))
            }
            Target::Flow(f) => {
                let preview = ctx.service.preview_flow(f)?;
                let mut lines: Vec<Line> = preview
                    .outline
                    .iter()
                    .map(|l| {
                        Line::raw(format!(
                            "{}{}{}",
                            "  ".repeat(l.depth),
                            l.label,
                            if l.value.is_empty() {
                                String::new()
                            } else {
                                format!(": {}", l.value)
                            }
                        ))
                    })
                    .collect();
                if let Some(e) = preview.error {
                    lines.push(Line::styled(e, s.error()));
                }
                if !preview.traces.is_empty() {
                    lines.push(Line::styled("HOW EACH ROLL WAS REACHED", s.dim()));
                    lines.extend(trace_lines(&preview.traces, ctx));
                }
                Ok(lines)
            }
        }
    }
}

fn trace_lines(traces: &[RollTrace], ctx: &Ctx) -> Vec<Line<'static>> {
    let s = &ctx.styles;
    traces
        .iter()
        .flat_map(|t| {
            std::iter::once(Line::styled(t.title.clone(), s.title()))
                .chain(t.lines.iter().map(|l| Line::raw(format!("  {l}"))))
        })
        .collect()
}

fn bar(percent: f64) -> String {
    "█".repeat(((percent / 100.0) * 30.0).round().clamp(0.0, 30.0) as usize)
}

fn outcome_lines(rows: &[OutcomeStat], expected: bool, ctx: &Ctx) -> Vec<Line<'static>> {
    let s = &ctx.styles;
    let mut out = vec![Line::styled(
        if expected {
            format!(
                "{:<28}{:>10}{:>10}{:>8}",
                "RESULT", "EXPECTED", "ACTUAL", "COUNT"
            )
        } else {
            format!("{:<28}{:>10}{:>8}", "OUTCOME", "ACTUAL", "COUNT")
        },
        s.dim(),
    )];
    for r in rows {
        let mut text = format!("{:<28}", truncate(&r.name, 27));
        if expected {
            text.push_str(&format!(
                "{:>10}",
                r.expected.map_or("—".into(), |e| format!("{e:.1}%"))
            ));
        }
        text.push_str(&format!(
            "{:>10}{:>8}  ",
            format!("{:.1}%", r.actual),
            r.count
        ));
        let off = expected && r.expected.is_some_and(|e| (e - r.actual).abs() >= 5.0);
        out.push(Line::from(vec![
            Span::styled(text, if r.count == 0 { s.dim() } else { s.title() }),
            Span::styled(bar(r.actual), if off { s.key() } else { s.good() }),
        ]));
    }
    out
}

fn truncate(text: &str, n: usize) -> String {
    if text.chars().count() <= n {
        text.to_owned()
    } else {
        format!("{}…", text.chars().take(n - 1).collect::<String>())
    }
}

fn usage_lines(modifiers: &[ModifierUse], errors: &[ErrorCount], ctx: &Ctx) -> Vec<Line<'static>> {
    let s = &ctx.styles;
    let mut out = Vec::new();
    if !modifiers.is_empty() {
        out.push(Line::styled("MODIFIERS USED", s.dim()));
        out.extend(
            modifiers
                .iter()
                .map(|m| Line::raw(format!("  {}: {} time(s)", m.label, m.count))),
        );
    }
    if !errors.is_empty() {
        out.push(Line::styled("RUNS THAT STOPPED WITH AN ERROR", s.dim()));
        out.extend(
            errors
                .iter()
                .map(|e| Line::styled(format!("  {} × {}", e.count, e.message), s.error())),
        );
    }
    out
}

fn table_lines(sim: &settlement_core::simulate::TableSimulation, ctx: &Ctx) -> Vec<Line<'static>> {
    let s = &ctx.styles;
    let mut out = vec![Line::styled(
        format!("{}: {} rolls", sim.table.name, sim.runs),
        s.title(),
    )];
    out.extend(outcome_lines(&sim.results, true, ctx));
    if !sim.unreachable.is_empty() {
        let names: Vec<&str> = sim.unreachable.iter().map(|r| r.name.as_str()).collect();
        out.push(Line::styled(
            format!("Never possible now: {}.", names.join(", ")),
            s.key(),
        ));
    }
    if sim.rerolls > 0.0 {
        out.push(Line::styled(
            format!(
                "On average {:.2} roll(s) made again per result.",
                sim.rerolls
            ),
            s.dim(),
        ));
    }
    out.extend(usage_lines(&sim.modifiers, &sim.errors, ctx));
    out.push(Line::styled(sim.note.clone(), s.dim()));
    out
}

fn flow_lines(sim: &settlement_core::simulate::FlowSimulation, ctx: &Ctx) -> Vec<Line<'static>> {
    let s = &ctx.styles;
    let mut out = Vec::new();
    for step in &sim.steps {
        out.push(Line::styled(
            format!(
                "{} (ran in {} of {} runs)",
                step.label, step.reached, sim.runs
            ),
            s.title(),
        ));
        out.extend(outcome_lines(
            &step.outcomes[..step.outcomes.len().min(10)],
            false,
            ctx,
        ));
        if step.outcomes.len() > 10 {
            out.push(Line::styled(
                format!("  …and {} more", step.outcomes.len() - 10),
                s.dim(),
            ));
        }
    }
    for b in &sim.branches {
        out.push(Line::styled(format!("Condition “{}”", b.label), s.title()));
        out.extend(outcome_lines(&b.cases, false, ctx));
    }
    if !sim.impossible.is_empty() {
        out.push(Line::styled("BRANCHES THAT NEVER RAN", s.dim()));
        out.extend(
            sim.impossible
                .iter()
                .map(|m| Line::styled(format!("  {m}"), s.key())),
        );
    }
    out.extend(usage_lines(&sim.modifiers, &sim.errors, ctx));
    out.push(Line::styled(sim.note.clone(), s.dim()));
    out
}

impl Screen for TestScreen {
    fn title(&self) -> String {
        format!("Test › {}", self.name())
    }

    fn render(&mut self, f: &mut Frame, area: Rect, ctx: &Ctx) {
        let rows = self.rows(ctx);
        self.state.render(f, area, &rows, &ctx.styles, true);
    }

    fn handle(&mut self, key: KeyEvent, ctx: &mut Ctx) -> Nav {
        let rows = self.rows(ctx);
        if self.state.handle_nav(&rows, key) {
            return Nav::Stay;
        }
        match key.code {
            KeyCode::Esc | KeyCode::Backspace => Nav::Pop,
            KeyCode::Enter | KeyCode::Char(' ') => {
                if let Some(&act) = self.state.current(&rows) {
                    self.act(act, ctx);
                    if act == Act::Next || act == Act::Step {
                        let rows = self.rows(ctx);
                        self.state.select_where(&rows, |a| *a == Act::Next);
                    }
                }
                Nav::Stay
            }
            _ => Nav::Stay,
        }
    }

    fn hints(&self) -> Vec<(&'static str, &'static str)> {
        vec![("↑↓", "move"), ("Enter", "run"), ("Esc", "back")]
    }
}
