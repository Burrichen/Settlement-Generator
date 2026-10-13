//! Start: Automatic, Manual Incremental and Manual One-table generation, the
//! completed settlement, and editing a saved settlement. Every rule comes
//! from the core session; this screen only presents it and forwards keys.

use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};
use settlement_core::engine::{GenerationError, ResultOption, Selections, SkipReason};
use settlement_core::figures::{FIGURE_FIELDS, FigureName};
use settlement_core::ids::ResultId;
use settlement_core::locations::{LocationSource, TableOptions};
use settlement_core::naming::NameSource;
use settlement_core::roles::Allocation;
use settlement_core::service::ActiveGeneration;
use settlement_core::session::{GenerationMode, SessionView, StageStatus, StageView};
use settlement_core::settlements::{MAX_NAME_LENGTH, SettlementId};

use super::common::{
    SheetRow, applies, figure_lines, location_label, location_lines, outcome_lines, sheet_lines,
    tag_line,
};
use crate::tui::widgets::{Modal, ModalResult, PickItem, Row, RowsState};
use crate::tui::{Ctx, Nav, Screen};

enum Pending {
    /// Choosing a result for a stage (One-table picker); the IDs map picker
    /// indices to results. On the name stage the results are methods.
    Choose(usize, Vec<ResultId>),
    /// Typing a name on the name stage.
    CustomName(usize),
    /// Picking which part of the settlement's generated name to change.
    SettlementNamePart(usize),
    /// Rolling (`None`) or choosing one part of the settlement's name:
    /// stage, part, results.
    SettlementNameResult(usize, usize, Vec<Option<ResultId>>),
    /// Picking what to change on the Authority Figures stage.
    FigureTarget(usize, Vec<FigureTarget>),
    /// Choosing a figure field's result: stage, figure, field, results.
    FigureField(usize, usize, String, Vec<ResultId>),
    /// Choosing a relationship's result: stage, relationship, results.
    Relationship(usize, usize, Vec<ResultId>),
    /// Choosing one part of a figure's name: stage, figure, part, results.
    NamePart(usize, usize, usize, Vec<ResultId>),
    /// Typing an Authority Figure's name: stage, figure.
    FigureName(usize, usize),
    /// Picking which role to change: stage.
    RoleSlot(usize),
    /// Choosing a role: stage, slot, results.
    Role(usize, usize, Vec<ResultId>),
    /// Picking what to change on the Notable Locations stage.
    LocationTarget(usize, Vec<LocationTarget>),
    /// Choosing a Notable Locations result: stage, target, results (`None`
    /// rolls again).
    LocationResult(usize, LocationTarget, Vec<Option<ResultId>>),
    /// Naming the settlement to save it.
    Name,
    ConfirmLeave,
    ConfirmDelete,
}

/// Something on the Authority Figures stage that can be changed.
#[derive(Clone)]
enum FigureTarget {
    Field(usize, String),
    TypeName(usize),
    RollName(usize),
    /// A part of a generated name, e.g. the given name: figure, part.
    NamePart(usize, usize),
    Relationship(usize),
}

/// Something on the Notable Locations stage that can be changed. The
/// guaranteed Tavern is fixed.
#[derive(Clone, Copy)]
enum LocationTarget {
    Districts,
    /// One area's Notable Location Count.
    Count(usize),
    /// One rolled location's Type (by position in the list).
    Type(usize),
}

/// How the name stage's name came about, and anything that explains it:
/// no naming data, routing and every roll (the last two only with Tag
/// View, which is when the core includes them).
fn naming_lines(stage: &StageView, ctx: &Ctx) -> Vec<Line<'static>> {
    let Some(naming) = &stage.naming else {
        return Vec::new();
    };
    let s = &ctx.styles;
    let mut lines = Vec::new();
    match (naming.source, &naming.method) {
        (Some(NameSource::Custom), _) => lines.push(Line::styled("Typed name", s.dim())),
        (Some(NameSource::Generated), Some(method)) => {
            let scheme = naming.scheme.as_ref().map_or("", |s| s.name.as_str());
            lines.push(Line::styled(
                format!("{scheme} naming · {}", method.name),
                s.dim(),
            ));
        }
        _ => {}
    }
    match stage.skipped {
        Some(SkipReason::NoNamingScheme) => lines.push(Line::styled(
            "No naming data for this region yet: press t to type a name, or name it when saving.",
            s.dim(),
        )),
        Some(SkipReason::NotRecorded) => lines.push(Line::styled(
            "Saved before settlement naming existed.",
            s.dim(),
        )),
        _ => {}
    }
    if !naming.routing.is_empty() {
        let tags: Vec<&str> = naming.routing.iter().map(|t| t.name.as_str()).collect();
        let scheme = naming.scheme.as_ref().map_or("", |s| s.name.as_str());
        lines.push(Line::styled(
            format!("Scheme: {scheme} — chosen by {}", tags.join(" + ")),
            s.dim(),
        ));
    }
    for part in &naming.parts {
        let roll = match (&part.roll, &part.dice) {
            (Some(roll), Some(dice)) if !part.chosen => format!(" — rolled {roll} on {dice}"),
            _ => " — chosen".into(),
        };
        lines.push(Line::styled(
            format!(
                "{}: {} ({}){roll}",
                part.table.name, part.result.name, part.result.id
            ),
            s.dim(),
        ));
    }
    if let Some(assembly) = &naming.assembly {
        let how = match (&assembly.exception, &assembly.curated) {
            (Some(e), _) => {
                let note = if e.note.is_empty() {
                    String::new()
                } else {
                    format!(" ({})", e.note)
                };
                format!("Spelling exception: {}{note}", e.name)
            }
            (None, Some(c)) => format!(
                "Approved name: {} ({}) chosen by {}",
                c.source, c.table, c.variant
            ),
            (None, None) => format!("Joined: {}", assembly.join),
        };
        lines.push(Line::styled(how, s.dim()));
    }
    if naming.part_options.iter().any(|p| !p.options.is_empty()) {
        lines.push(Line::from(vec![
            Span::styled("c", s.key()),
            Span::styled(" change one part of the name", s.dim()),
        ]));
    }
    lines
}

#[derive(PartialEq, Eq)]
enum Phase {
    Working,
    Completed,
}

const COMPLETED_ACTIONS: [&str; 3] = ["Re-roll", "Save", "Delete"];

pub struct GenerateScreen {
    active: ActiveGeneration,
    view: SessionView,
    error: Option<String>,
    phase: Phase,
    rows: RowsState,
    action: usize,
    /// How far the completed settlement sheet is scrolled.
    sheet_scroll: u16,
    modal: Option<(Modal, Pending)>,
    /// For edits: the saved settlement and its current name.
    edit: Option<(SettlementId, String)>,
}

impl GenerateScreen {
    fn new(active: ActiveGeneration, edit: Option<(SettlementId, String)>) -> Result<Self, String> {
        let view = active.view()?;
        Ok(GenerateScreen {
            active,
            view,
            error: None,
            phase: Phase::Working,
            rows: RowsState::default(),
            action: 0,
            sheet_scroll: 0,
            modal: None,
            edit,
        })
    }

    /// Starts a new settlement with the current settings.
    pub fn start(ctx: &mut Ctx) -> Result<Self, String> {
        let mut screen = Self::new(ctx.service.start_generation()?, None)?;
        if screen.view.mode == GenerationMode::Automatic {
            screen.run_automatic(ctx);
        }
        Ok(screen)
    }

    /// Reopens a saved settlement for editing.
    pub fn edit(ctx: &mut Ctx, id: &SettlementId, name: &str) -> Result<Self, String> {
        Self::new(
            ctx.service.start_edit(id)?,
            Some((id.clone(), name.to_owned())),
        )
    }

    /// Runs a session action and refreshes the view.
    fn act<T>(
        &mut self,
        ctx: &mut Ctx,
        action: impl FnOnce(
            &mut ActiveGeneration,
            &mut dyn settlement_core::dice::FaceSource,
        ) -> Result<T, GenerationError>,
    ) -> bool {
        let outcome = action(&mut self.active, ctx.dice.as_mut());
        let ok = outcome.is_ok();
        match self.active.respond(outcome) {
            Ok(response) => {
                self.view = response.view;
                self.error = response.error;
            }
            Err(e) => self.error = Some(e),
        }
        ok
    }

    fn run_automatic(&mut self, ctx: &mut Ctx) {
        self.act(ctx, |a, dice| {
            a.session.run_automatic(&Selections::new(), dice)
        });
        if self.view.complete {
            self.phase = Phase::Completed;
        }
        // A choose-only stage simply waits for the user; only show real
        // problems.
        if let Some(i) = self.view.next_pending
            && self.view.stages[i].incomplete.is_none()
        {
            self.error = None;
        }
    }

    fn mode_label(&self) -> String {
        let mode = if self.edit.is_some() {
            "Editing saved settlement"
        } else {
            match self.view.mode {
                GenerationMode::Automatic => "Automatic",
                GenerationMode::Incremental => "Manual · Incremental",
                GenerationMode::OneTable => "Manual · One-table",
            }
        };
        if self.view.god_mode {
            format!("{mode} · God Mode")
        } else {
            mode.into()
        }
    }

    /// The stage the keys act on now, outside the completed view.
    fn focused_stage(&self) -> Option<&StageView> {
        if self.phase == Phase::Completed {
            return None;
        }
        let index = if self.edit.is_some() || self.view.mode == GenerationMode::OneTable {
            // Rows are the stages that apply, in order.
            self.view
                .stages
                .iter()
                .filter(|s| applies(s))
                .nth(self.rows.selected)?
                .index
        } else if self.view.mode == GenerationMode::Automatic {
            self.view.next_pending?
        } else {
            self.view.cursor
        };
        self.view.stages.get(index)
    }

    fn anything_resolved(&self) -> bool {
        self.view
            .stages
            .iter()
            .any(|s| s.status == StageStatus::Resolved)
    }

    /// Rows of legal results for `stage`.
    fn result_rows(stage: &StageView, ctx: &Ctx) -> Vec<Row<ResultId>> {
        let s = &ctx.styles;
        stage
            .options
            .iter()
            .filter(|o| o.legal)
            .map(|o| {
                let faces = o.range.map_or(String::new(), |r| r.to_string());
                let mut spans = vec![
                    Span::styled(format!("{faces:>6}  "), s.dim()),
                    Span::raw(o.result.name.clone()),
                ];
                if stage.result.as_ref().is_some_and(|r| r.id == o.result.id) {
                    spans.push(Span::styled("  ✓", s.heading()));
                }
                if !o.tags.is_empty() {
                    let names: Vec<&str> = o.tags.iter().map(|t| t.name.as_str()).collect();
                    spans.push(Span::styled(format!("   {}", names.join(", ")), s.dim()));
                }
                Row::item(Line::from(spans), o.result.id.clone())
            })
            .collect()
    }

    fn stage_rows(&self, ctx: &Ctx) -> Vec<Row<usize>> {
        let s = &ctx.styles;
        self.view
            .stages
            .iter()
            .filter(|stage| applies(stage))
            .map(|stage| {
                let result = stage.result.as_ref().map_or_else(
                    || {
                        if stage.skipped == Some(SkipReason::NoNamingScheme) {
                            "no naming data".into()
                        } else if stage.skipped == Some(SkipReason::NotRecorded) {
                            "not recorded".into()
                        } else if stage.skipped.is_some() {
                            "skipped".into()
                        } else if stage.previous.is_some() {
                            "redo".into()
                        } else {
                            "—".into()
                        }
                    },
                    |r| r.name.clone(),
                );
                let roll = stage.outcome.as_ref().map_or(String::new(), |o| {
                    if o.chosen {
                        "chosen".into()
                    } else {
                        format!(
                            "{} on {}",
                            o.roll.clone().unwrap_or_default(),
                            o.dice.clone().unwrap_or_default()
                        )
                    }
                });
                Row::item(
                    Line::from(vec![
                        Span::raw(format!("{:<18}", stage_label(stage))),
                        Span::styled(
                            format!("{result:<26}"),
                            if stage.status == StageStatus::Resolved {
                                s.title()
                            } else {
                                s.dim()
                            },
                        ),
                        Span::styled(roll, s.dim()),
                    ]),
                    stage.index,
                )
            })
            .collect()
    }

    fn stage_notes(stage: &StageView, ctx: &Ctx) -> Vec<Line<'static>> {
        let s = &ctx.styles;
        let mut lines = Vec::new();
        if stage.table.name != stage.stage.name {
            lines.push(Line::styled(stage.table.name.clone(), s.dim()));
        }
        if let (Some(previous), StageStatus::Pending) = (&stage.previous, stage.status) {
            lines.push(Line::styled(
                format!(
                    "An earlier change affects this stage; it was {}.",
                    previous.name
                ),
                ratatui::style::Style::new().fg(s.accent),
            ));
        }
        if let Some(note) = &stage.incomplete {
            lines.push(Line::styled(
                format!("{note} Choose a result to continue."),
                ratatui::style::Style::new().fg(s.accent),
            ));
        }
        if stage.skipped.is_some() && stage.naming.is_none() {
            lines.push(Line::styled(
                "This stage does not apply and is skipped.",
                s.dim(),
            ));
        }
        lines.extend(naming_lines(stage, ctx));
        lines
    }

    // ----- rendering -----------------------------------------------------------

    fn render_rail(&self, f: &mut Frame, area: Rect, current: usize, ctx: &Ctx) {
        let s = &ctx.styles;
        let lines: Vec<Line> = self
            .view
            .stages
            .iter()
            .filter(|stage| applies(stage))
            .map(|stage| {
                let (mark, style) = if stage.index == current {
                    ("▸", s.selected())
                } else if stage.status == StageStatus::Resolved {
                    ("✓", s.dim())
                } else if stage.previous.is_some() {
                    ("↺", ratatui::style::Style::new().fg(s.accent))
                } else {
                    ("·", s.dim())
                };
                let result = stage
                    .result
                    .as_ref()
                    .map_or(String::new(), |r| format!("  {}", r.name));
                Line::from(vec![
                    Span::styled(format!("{mark} {}", stage_label(stage)), style),
                    Span::styled(result, s.dim()),
                ])
            })
            .collect();
        f.render_widget(Paragraph::new(lines), area);
    }

    fn render_active_tags(&self, f: &mut Frame, area: Rect, ctx: &Ctx) {
        let s = &ctx.styles;
        if let Some(tags) = &self.view.active_tags {
            let mut lines = vec![Line::styled("ACTIVE TAGS", s.dim())];
            if tags.is_empty() {
                lines.push(Line::styled("none yet", s.dim()));
            }
            lines.extend(tags.iter().map(|t| Line::styled(t.name.clone(), s.dim())));
            f.render_widget(Paragraph::new(lines), area);
        }
    }

    /// Splits off the rail (left) and tag panel (right) when wide enough.
    fn columns(&self, area: Rect) -> (Option<Rect>, Rect, Option<Rect>) {
        let tags = self.view.active_tags.is_some() && area.width >= 100;
        let rail = area.width >= 72;
        let mut constraints = Vec::new();
        if rail {
            constraints.push(Constraint::Length(30));
        }
        constraints.push(Constraint::Fill(1));
        if tags {
            constraints.push(Constraint::Length(24));
        }
        let parts = Layout::horizontal(constraints).spacing(2).split(area);
        let mut i = 0;
        let rail_area = rail.then(|| {
            i += 1;
            parts[0]
        });
        let main = parts[i];
        let tag_area = tags.then(|| parts[i + 1]);
        (rail_area, main, tag_area)
    }

    fn render_stage_panel(
        &mut self,
        f: &mut Frame,
        area: Rect,
        index: usize,
        ctx: &Ctx,
        heading: String,
    ) {
        let s = &ctx.styles;
        let stage = self.view.stages[index].clone();
        let mut top = vec![
            Line::styled(
                format!("STAGE {} OF {}", index + 1, self.view.stages.len()),
                s.dim(),
            ),
            Line::styled(heading, s.title()),
        ];
        if let Some(locations) = &stage.locations {
            top.extend(Self::stage_notes(&stage, ctx));
            if let Some(e) = &self.error {
                top.push(Line::styled(e.clone(), s.error()));
            }
            if stage.can_roll {
                top.push(Line::from(vec![
                    Span::styled("r", s.key()),
                    Span::styled(
                        if locations.record.is_some() {
                            " re-roll all Notable Locations"
                        } else {
                            " roll Notable Locations"
                        },
                        s.dim(),
                    ),
                ]));
            } else if locations.record.is_none() {
                top.push(Line::styled("Resolve the Size first.", s.dim()));
            }
            if locations.bare_minimum_buildings {
                top.push(Line::styled(
                    "Bare Minimum Buildings: one extra Tavern is added.",
                    s.dim(),
                ));
            }
            if let Some(record) = &locations.record {
                top.push(Line::from(vec![
                    Span::styled("Enter", s.key()),
                    Span::styled(" change a count or a location's type", s.dim()),
                ]));
                top.push(Line::default());
                top.extend(location_lines(record, s, 0));
            }
            f.render_widget(Paragraph::new(top).wrap(Wrap { trim: false }), area);
            return;
        }
        if let Some(roles) = &stage.roles {
            top.extend(Self::stage_notes(&stage, ctx));
            if let Some(e) = &self.error {
                top.push(Line::styled(e.clone(), s.error()));
            }
            if stage.can_roll {
                top.push(Line::from(vec![
                    Span::styled("r", s.key()),
                    Span::styled(
                        if roles.record.is_some() {
                            " reallocate all roles"
                        } else {
                            " allocate roles"
                        },
                        s.dim(),
                    ),
                ]));
            }
            if roles.record.is_some() {
                top.push(Line::from(vec![
                    Span::styled("Enter", s.key()),
                    Span::styled(" change a role", s.dim()),
                ]));
            }
            top.push(Line::default());
            top.extend(role_lines(&stage, s));
            f.render_widget(Paragraph::new(top).wrap(Wrap { trim: false }), area);
            return;
        }
        if let Some(figures) = &stage.figures {
            top.extend(Self::stage_notes(&stage, ctx));
            if let Some(e) = &self.error {
                top.push(Line::styled(e.clone(), s.error()));
            }
            if stage.can_roll {
                top.push(Line::from(vec![
                    Span::styled("r", s.key()),
                    Span::styled(
                        if figures.record.is_some() {
                            " reroll all figures"
                        } else {
                            " generate figures"
                        },
                        s.dim(),
                    ),
                ]));
            } else if figures.record.is_none() {
                top.push(Line::styled(
                    "Resolve the Authority and its Authority Size first.",
                    s.dim(),
                ));
            }
            if let Some(record) = &figures.record {
                top.push(Line::from(vec![
                    Span::styled("Enter", s.key()),
                    Span::styled(" change a figure's details or a relationship", s.dim()),
                ]));
                top.push(Line::default());
                top.extend(figure_lines(record, s, 0, stage.tags.is_some()));
            }
            f.render_widget(Paragraph::new(top).wrap(Wrap { trim: false }), area);
            return;
        }
        if let Some(naming) = &stage.naming {
            top.push(match &naming.name {
                Some(name) => Line::styled(name.clone(), s.heading()),
                None => Line::styled("Not named yet", s.dim()),
            });
        }
        top.extend(Self::stage_notes(&stage, ctx));
        if let Some(e) = &self.error {
            top.push(Line::styled(e.clone(), s.error()));
        }
        top.extend(outcome_lines(stage.outcome.as_ref(), s));
        let naming = stage.naming.is_some();
        if stage.can_roll {
            top.push(Line::from(vec![
                Span::styled("r", s.key()),
                Span::styled(
                    format!(
                        " {}{}",
                        if naming { "roll name" } else { "roll" },
                        stage
                            .dice
                            .as_ref()
                            .map_or(String::new(), |d| format!(" {d}"))
                    ),
                    s.dim(),
                ),
            ]));
        }
        if naming {
            top.push(Line::from(vec![
                Span::styled("t", s.key()),
                Span::styled(" type a name", s.dim()),
            ]));
        }
        if let Some(tags) = &stage.tags {
            if !tags.is_empty() {
                top.push(tag_line(tags, s, 0));
            }
        }
        top.push(Line::default());
        top.push(Line::styled(
            match (naming, stage.can_roll) {
                (true, _) => "OR CHOOSE A NAMING METHOD",
                (false, true) => "OR CHOOSE A RESULT",
                (false, false) => "CHOOSE A RESULT",
            },
            s.dim(),
        ));
        let height = (top.len() as u16).min(area.height);
        let [head, list] =
            Layout::vertical([Constraint::Length(height), Constraint::Fill(1)]).areas(area);
        f.render_widget(Paragraph::new(top).wrap(Wrap { trim: false }), head);
        let rows = Self::result_rows(&stage, ctx);
        if rows.is_empty() {
            let empty = if naming {
                "No naming method applies here."
            } else {
                "No result can be chosen here."
            };
            f.render_widget(Paragraph::new(Line::styled(empty, s.dim())), list);
        } else {
            self.rows.render(f, list, &rows, s, true);
        }
    }

    fn render_completed(&mut self, f: &mut Frame, area: Rect, ctx: &Ctx) {
        let s = &ctx.styles;
        let rows: Vec<SheetRow> = self
            .view
            .stages
            .iter()
            .map(|stage| SheetRow {
                label: stage.stage.name.clone(),
                value: stage.result.as_ref().map_or_else(
                    || {
                        if stage.skipped.is_some() {
                            "Not applicable".into()
                        } else {
                            "—".into()
                        }
                    },
                    |r| r.name.clone(),
                ),
                source: (stage.table.name != stage.stage.name).then(|| stage.table.name.clone()),
                tags: stage.tags.clone(),
                sub_stage: stage.sub_stage,
                // Roles are shown with each Authority Figure instead.
                skipped: stage.status == StageStatus::Skipped || stage.roles.is_some(),
                figures: stage.figures.as_ref().and_then(|f| f.record.clone()),
                locations: stage.locations.as_ref().and_then(|l| l.record.clone()),
            })
            .collect();
        let mut lines = vec![Line::styled("─── ◆ ───", s.border()), Line::default()];
        lines.extend(sheet_lines(&rows, s));
        // The sheet scrolls (Authority Figures can make it long); the status
        // and actions below it always stay in view.
        let mut footer = vec![Line::default(), Line::styled("Not saved yet", s.dim())];
        if let Some(e) = &self.error {
            footer.push(Line::styled(e.clone(), s.error()));
        }
        footer.push(Line::default());
        let mut actions = Vec::new();
        for (i, label) in COMPLETED_ACTIONS.iter().enumerate() {
            if i > 0 {
                actions.push(Span::raw("   "));
            }
            actions.push(if i == self.action {
                Span::styled(format!("[ {label} ]"), s.selected())
            } else {
                Span::styled(format!("  {label}  "), s.dim())
            });
        }
        footer.push(Line::from(actions));
        let footer_height = (footer.len() as u16 + 1).min(area.height);
        let [sheet, bottom] =
            Layout::vertical([Constraint::Fill(1), Constraint::Length(footer_height)]).areas(area);
        let overflow = (lines.len() as u16).saturating_sub(sheet.height);
        self.sheet_scroll = self.sheet_scroll.min(overflow);
        if overflow > 0 {
            footer.push(Line::from(vec![
                Span::styled("↑↓", s.key()),
                Span::styled(" scroll the settlement", s.dim()),
            ]));
        }
        f.render_widget(
            Paragraph::new(lines)
                .wrap(Wrap { trim: false })
                .scroll((self.sheet_scroll, 0)),
            sheet,
        );
        f.render_widget(Paragraph::new(footer).wrap(Wrap { trim: false }), bottom);
    }

    // ----- input -----------------------------------------------------------------

    fn handle_modal(&mut self, key: KeyEvent, ctx: &mut Ctx) -> Option<Nav> {
        let (modal, pending) = self.modal.as_mut()?;
        let result = modal.handle(key, &ctx.styles);
        match (result, pending) {
            (ModalResult::Open, _) => return Some(Nav::Stay),
            (ModalResult::Picked(i), Pending::Choose(stage, ids)) => {
                let (stage, id) = (*stage, ids[i].clone());
                self.modal = None;
                self.act(ctx, |a, dice| a.session.choose(stage, id, dice));
            }
            (ModalResult::Picked(i), Pending::SettlementNamePart(stage)) => {
                let stage = *stage;
                self.modal = None;
                self.open_settlement_name_part(stage, i);
            }
            (ModalResult::Picked(i), Pending::SettlementNameResult(stage, part, ids)) => {
                let (stage, part, id) = (*stage, *part, ids[i].clone());
                self.modal = None;
                self.act(ctx, |a, dice| match id {
                    None => a.session.roll_name_part(stage, part, dice),
                    Some(id) => a.session.choose_name_part(stage, part, id, dice),
                });
            }
            (ModalResult::Picked(i), Pending::LocationTarget(stage, targets)) => {
                let (stage, target) = (*stage, targets[i]);
                self.modal = None;
                self.open_location_target(stage, target);
            }
            (ModalResult::Picked(i), Pending::LocationResult(stage, target, ids)) => {
                let (stage, target, id) = (*stage, *target, ids[i].clone());
                self.modal = None;
                self.act(ctx, |a, dice| match (target, id) {
                    (LocationTarget::Districts, None) => a.session.roll_district_count(stage, dice),
                    (LocationTarget::Districts, Some(id)) => {
                        a.session.choose_district_count(stage, id, dice)
                    }
                    (LocationTarget::Count(area), None) => {
                        a.session.roll_location_count(stage, area, dice)
                    }
                    (LocationTarget::Count(area), Some(id)) => {
                        a.session.choose_location_count(stage, area, id, dice)
                    }
                    (LocationTarget::Type(l), None) => a.session.roll_location_type(stage, l, dice),
                    (LocationTarget::Type(l), Some(id)) => {
                        a.session.choose_location_type(stage, l, id, dice)
                    }
                });
            }
            (ModalResult::Picked(i), Pending::RoleSlot(stage)) => {
                let stage = *stage;
                self.modal = None;
                self.open_role_slot(stage, i);
            }
            (ModalResult::Picked(i), Pending::Role(stage, slot, ids)) => {
                let (stage, slot, id) = (*stage, *slot, ids[i].clone());
                self.modal = None;
                self.act(ctx, |a, dice| a.session.choose_role(stage, slot, id, dice));
            }
            (ModalResult::Picked(i), Pending::FigureTarget(stage, targets)) => {
                let (stage, target) = (*stage, targets[i].clone());
                self.modal = None;
                self.open_figure_target(ctx, stage, target);
            }
            (ModalResult::Picked(i), Pending::FigureField(stage, figure, field, ids)) => {
                let (stage, figure, field, id) = (*stage, *figure, field.clone(), ids[i].clone());
                self.modal = None;
                self.act(ctx, |a, dice| {
                    a.session
                        .choose_figure_field(stage, figure, &field, id, dice)
                });
            }
            (ModalResult::Picked(i), Pending::NamePart(stage, figure, part, ids)) => {
                let (stage, figure, part, id) = (*stage, *figure, *part, ids[i].clone());
                self.modal = None;
                self.act(ctx, |a, dice| {
                    a.session
                        .choose_figure_name_part(stage, figure, part, id, dice)
                });
            }
            (ModalResult::Picked(i), Pending::Relationship(stage, rel, ids)) => {
                let (stage, rel, id) = (*stage, *rel, ids[i].clone());
                self.modal = None;
                self.act(ctx, |a, dice| {
                    a.session.choose_relationship(stage, rel, id, dice)
                });
            }
            (ModalResult::Text(name), Pending::FigureName(stage, figure)) => {
                match self.active.session.set_figure_name(*stage, *figure, &name) {
                    Ok(_) => {
                        if let Ok(view) = self.active.view() {
                            self.view = view;
                        }
                        self.error = None;
                        self.modal = None;
                    }
                    Err(e) => modal.set_error(e.to_string()),
                }
            }
            (ModalResult::Text(name), Pending::CustomName(stage)) => {
                match self.active.session.set_custom_name(*stage, &name) {
                    Ok(_) => {
                        if let Ok(view) = self.active.view() {
                            self.view = view;
                        }
                        self.error = None;
                        self.modal = None;
                    }
                    // Keep the dialog open with the reason (empty, too long).
                    Err(e) => modal.set_error(e.to_string()),
                }
            }
            (ModalResult::Text(name), Pending::Name) => {
                let name = name.trim().to_owned();
                let saved = match &self.edit {
                    Some(_) => ctx.service.save_edit(&self.active, &name).map(|_| ()),
                    None => ctx.service.save_new(&self.active, &name).map(|_| ()),
                };
                match saved {
                    Ok(()) => {
                        ctx.flash = Some(format!("Saved “{name}”"));
                        self.modal = None;
                        return Some(if self.edit.is_some() {
                            Nav::Pop
                        } else {
                            Nav::Home
                        });
                    }
                    // Keep the dialog open with the reason (empty, too long…).
                    Err(e) => modal.set_error(e),
                }
            }
            (ModalResult::Yes, Pending::ConfirmLeave | Pending::ConfirmDelete) => {
                self.modal = None;
                return Some(if self.edit.is_some() {
                    Nav::Pop
                } else {
                    Nav::Home
                });
            }
            _ => self.modal = None,
        }
        Some(Nav::Stay)
    }

    /// Opens the list of what can be changed on the Notable Locations stage:
    /// the District Count, each area's count and each rolled location's Type.
    fn open_location_editor(&mut self, index: usize) {
        let Some(view) = self.view.stages[index].locations.as_ref() else {
            return;
        };
        let Some(record) = view.record.as_ref() else {
            self.error = Some("Roll the Notable Locations first (r).".into());
            return;
        };
        let mut items = Vec::new();
        let mut targets = Vec::new();
        if let Some(districts) = &record.districts {
            items.push(PickItem::new("District Count").hint(districts.roll.result.name.clone()));
            targets.push(LocationTarget::Districts);
        }
        for (i, area) in record.areas.iter().enumerate() {
            let label = area.district.as_ref().map_or_else(
                || "Notable Location Count".to_owned(),
                |d| format!("{}: Notable Location Count", d.label),
            );
            items.push(PickItem::new(label).hint(area.count.roll.result.name.clone()));
            targets.push(LocationTarget::Count(i));
        }
        for (i, location) in record.locations.iter().enumerate() {
            if location.source == LocationSource::Rolled {
                items.push(
                    PickItem::new(format!("Location {}", i + 1))
                        .hint(location_label(record, location)),
                );
                targets.push(LocationTarget::Type(i));
            }
        }
        self.modal = Some((
            Modal::pick("Change Notable Locations", items, 0),
            Pending::LocationTarget(index, targets),
        ));
    }

    /// Opens one Notable Locations table's legal results, after "Roll again".
    fn open_location_target(&mut self, index: usize, target: LocationTarget) {
        let Some(view) = self.view.stages[index].locations.as_ref() else {
            return;
        };
        let Some(record) = view.record.as_ref() else {
            return;
        };
        let (options, current): (Option<&TableOptions>, Option<ResultId>) = match target {
            LocationTarget::Districts => (
                view.options.districts.as_ref(),
                record.districts.as_ref().map(|d| d.roll.result.id.clone()),
            ),
            LocationTarget::Count(area) => (
                view.options.count.as_ref(),
                record
                    .areas
                    .get(area)
                    .map(|a| a.count.roll.result.id.clone()),
            ),
            LocationTarget::Type(l) => (
                view.options.types.as_ref(),
                record.locations.get(l).map(|l| l.location_type.id.clone()),
            ),
        };
        let Some(options) = options else {
            return;
        };
        let legal: Vec<&ResultOption> = options.options.iter().filter(|o| o.legal).collect();
        let mut items = vec![PickItem::new("Roll again")];
        let mut ids = vec![None];
        for o in legal {
            items.push(
                PickItem::new(o.result.name.clone())
                    .hint(o.range.map_or(String::new(), |r| r.to_string())),
            );
            ids.push(Some(o.result.id.clone()));
        }
        let at = current
            .and_then(|c| ids.iter().position(|id| id.as_ref() == Some(&c)))
            .unwrap_or(0);
        self.modal = Some((
            Modal::pick(&format!("Choose {}", options.table.name), items, at),
            Pending::LocationResult(index, target, ids),
        ));
    }

    /// Opens the list of roles to change on the Role Allocation stage.
    fn open_role_editor(&mut self, index: usize) {
        let Some(view) = self.view.stages[index].roles.as_ref() else {
            return;
        };
        let Some(record) = view.record.as_ref() else {
            self.error = Some("Allocate the roles first (r).".into());
            return;
        };
        let items = record
            .roles
            .iter()
            .enumerate()
            .map(|(i, r)| PickItem::new(role_slot_label(i, r.allocation)).hint(r.role.name.clone()))
            .collect();
        self.modal = Some((
            Modal::pick("Change a role", items, 0),
            Pending::RoleSlot(index),
        ));
    }

    /// Opens the role results for one slot: only its own table's.
    fn open_role_slot(&mut self, index: usize, slot: usize) {
        let Some(view) = self.view.stages[index].roles.as_ref() else {
            return;
        };
        let Some(options) = view.slots.get(slot) else {
            return;
        };
        let current = view
            .record
            .as_ref()
            .and_then(|r| r.roles.get(slot))
            .map(|r| r.role.id.clone());
        let legal: Vec<&ResultOption> = options.options.iter().filter(|o| o.legal).collect();
        let items = legal
            .iter()
            .map(|o| {
                PickItem::new(o.result.name.clone())
                    .hint(o.range.map_or(String::new(), |r| r.to_string()))
            })
            .collect();
        let ids: Vec<ResultId> = legal.iter().map(|o| o.result.id.clone()).collect();
        let at = current
            .and_then(|c| ids.iter().position(|id| *id == c))
            .unwrap_or(0);
        self.modal = Some((
            Modal::pick(&format!("Choose {}", options.table.name), items, at),
            Pending::Role(index, slot, ids),
        ));
    }

    /// Opens the list of what can be changed on the Authority Figures stage:
    /// each figure's name and fields, and each relationship.
    fn open_figure_editor(&mut self, index: usize) {
        let Some(view) = self.view.stages[index].figures.as_ref() else {
            return;
        };
        let Some(record) = view.record.as_ref() else {
            self.error = Some("Generate the Authority Figures first (r).".into());
            return;
        };
        let mut items = Vec::new();
        let mut targets = Vec::new();
        for (i, figure) in record.figures.iter().enumerate() {
            items.push(PickItem::new(format!("{} · Type a name", figure.label)));
            targets.push(FigureTarget::TypeName(i));
            if figure.name.text().is_some() {
                items.push(PickItem::new(format!("{} · Roll a name", figure.label)));
                targets.push(FigureTarget::RollName(i));
            }
            // Each part of a generated name, from the species' own tables.
            for part in view.name_parts.get(i).into_iter().flatten() {
                let current = match &figure.name {
                    FigureName::Generated { rolls, .. } => rolls
                        .get(part.part + 1)
                        .map_or_else(String::new, |r| r.result.name.clone()),
                    _ => String::new(),
                };
                items.push(
                    PickItem::new(format!("{} · {}", figure.label, part.label)).hint(current),
                );
                targets.push(FigureTarget::NamePart(i, part.part));
            }
            for def in &FIGURE_FIELDS {
                let current = figure
                    .field(def.id)
                    .map_or_else(String::new, |r| r.result.name.clone());
                items
                    .push(PickItem::new(format!("{} · {}", figure.label, def.label)).hint(current));
                targets.push(FigureTarget::Field(i, def.id.to_owned()));
            }
        }
        for (i, r) in record.relationships.iter().enumerate() {
            let label = |id| {
                record
                    .figure(id)
                    .map_or_else(|| id.to_string(), |f| f.label.clone())
            };
            items.push(
                PickItem::new(format!("{} ↔ {}", label(&r.a), label(&r.b)))
                    .hint(r.roll.result.name.clone()),
            );
            targets.push(FigureTarget::Relationship(i));
        }
        self.modal = Some((
            Modal::pick("Change an Authority Figure", items, 0),
            Pending::FigureTarget(index, targets),
        ));
    }

    fn open_figure_target(&mut self, ctx: &mut Ctx, index: usize, target: FigureTarget) {
        let Some(figures) = self.view.stages[index].figures.clone() else {
            return;
        };
        let pick = |options: Vec<&ResultOption>, current: Option<&ResultId>| {
            let items = options
                .iter()
                .map(|o| {
                    PickItem::new(o.result.name.clone())
                        .hint(o.range.map_or(String::new(), |r| r.to_string()))
                })
                .collect();
            let ids: Vec<ResultId> = options.iter().map(|o| o.result.id.clone()).collect();
            let at = current
                .and_then(|c| ids.iter().position(|id| id == c))
                .unwrap_or(0);
            (items, ids, at)
        };
        match target {
            FigureTarget::TypeName(figure) => {
                let current = figures
                    .record
                    .as_ref()
                    .and_then(|r| r.figures.get(figure))
                    .and_then(|f| f.name.text().map(str::to_owned))
                    .unwrap_or_default();
                self.modal = Some((
                    Modal::text(
                        "Authority Figure name",
                        &format!("Type a name, up to {MAX_NAME_LENGTH} characters."),
                        &current,
                    ),
                    Pending::FigureName(index, figure),
                ));
            }
            FigureTarget::RollName(figure) => {
                self.act(ctx, |a, dice| {
                    a.session.roll_figure_name(index, figure, dice)
                });
            }
            FigureTarget::NamePart(figure, part) => {
                let Some(options) = figures
                    .name_parts
                    .get(figure)
                    .and_then(|parts| parts.iter().find(|p| p.part == part))
                else {
                    return;
                };
                let current = figures
                    .record
                    .as_ref()
                    .and_then(|r| r.figures.get(figure))
                    .and_then(|f| match &f.name {
                        FigureName::Generated { rolls, .. } => {
                            rolls.get(part + 1).map(|r| r.result.id.clone())
                        }
                        _ => None,
                    });
                let legal: Vec<&ResultOption> =
                    options.options.iter().filter(|o| o.legal).collect();
                let (items, ids, at) = pick(legal, current.as_ref());
                self.modal = Some((
                    Modal::pick(&format!("Choose {}", options.label), items, at),
                    Pending::NamePart(index, figure, part, ids),
                ));
            }
            FigureTarget::Field(figure, field) => {
                let Some(options) = figures.fields.iter().find(|f| f.field == field) else {
                    return;
                };
                let current = figures
                    .record
                    .as_ref()
                    .and_then(|r| r.figures.get(figure))
                    .and_then(|f| f.field(&field))
                    .map(|r| r.result.id.clone());
                let legal: Vec<&ResultOption> =
                    options.options.iter().filter(|o| o.legal).collect();
                let (items, ids, at) = pick(legal, current.as_ref());
                self.modal = Some((
                    Modal::pick(&format!("Choose {}", options.label), items, at),
                    Pending::FigureField(index, figure, field, ids),
                ));
            }
            FigureTarget::Relationship(rel) => {
                let current = figures
                    .record
                    .as_ref()
                    .and_then(|r| r.relationships.get(rel))
                    .map(|r| r.roll.result.id.clone());
                let legal: Vec<&ResultOption> = figures
                    .relationship_options
                    .iter()
                    .filter(|o| o.legal)
                    .collect();
                let (items, ids, at) = pick(legal, current.as_ref());
                self.modal = Some((
                    Modal::pick("Choose a relationship", items, at),
                    Pending::Relationship(index, rel, ids),
                ));
            }
        }
    }

    /// Lists the parts of the settlement's generated name, to change one.
    fn open_settlement_name_parts(&mut self, index: usize) {
        let Some(naming) = self.view.stages[index].naming.as_ref() else {
            return;
        };
        if naming.part_options.is_empty() {
            self.error = Some("Roll a name first (r); a typed name has no parts.".into());
            return;
        }
        let items = naming
            .part_options
            .iter()
            .map(|p| {
                let current = naming
                    .parts
                    .get(p.part)
                    .map(|r| r.result.name.clone())
                    .unwrap_or_default();
                PickItem::new(p.label.clone()).hint(current)
            })
            .collect();
        self.modal = Some((
            Modal::pick("Change part of the name", items, 0),
            Pending::SettlementNamePart(index),
        ));
    }

    /// Lists one part's results, each with the name it would give.
    fn open_settlement_name_part(&mut self, index: usize, part: usize) {
        let Some(options) = self.view.stages[index]
            .naming
            .as_ref()
            .and_then(|n| n.part_options.get(part))
        else {
            return;
        };
        let mut items = vec![PickItem::new(format!("Roll {} again", options.label))];
        let mut ids = vec![None];
        for (i, o) in options.options.iter().enumerate().filter(|(_, o)| o.legal) {
            let preview = options.previews.get(i).cloned().flatten();
            let range = o.range.map_or(String::new(), |r| r.to_string());
            let hint = match preview {
                Some(name) => format!("{range} → {name}"),
                None => range,
            };
            items.push(PickItem::new(o.result.name.clone()).hint(hint));
            ids.push(Some(o.result.id.clone()));
        }
        self.modal = Some((
            Modal::pick(&format!("Choose {}", options.label), items, 0),
            Pending::SettlementNameResult(index, part, ids),
        ));
    }

    /// Asks for a typed name on the name stage `index`.
    fn ask_custom_name(&mut self, index: usize) {
        let current = self.view.stages[index]
            .naming
            .as_ref()
            .and_then(|n| n.name.clone())
            .unwrap_or_default();
        self.modal = Some((
            Modal::text(
                "Settlement name",
                &format!("Type a name, up to {MAX_NAME_LENGTH} characters."),
                &current,
            ),
            Pending::CustomName(index),
        ));
    }

    fn ask_name(&mut self) {
        // An edit keeps its saved name; a new settlement is offered the name
        // the name stage generated (or that was typed there).
        let current = match &self.edit {
            Some((_, name)) => name.clone(),
            None => self.view.suggested_name.clone().unwrap_or_default(),
        };
        self.modal = Some((
            Modal::text(
                "Settlement name",
                &format!("Up to {MAX_NAME_LENGTH} characters."),
                &current,
            ),
            Pending::Name,
        ));
    }

    fn ask_leave(&mut self, ctx: &Ctx) -> Nav {
        // An edit asks only if it differs from what is saved (the core
        // decides); a new settlement asks once anything is generated.
        let ask = match &self.edit {
            Some((_, name)) => ctx
                .service
                .edit_has_changes(&self.active, name)
                .unwrap_or(true),
            None => self.anything_resolved(),
        };
        if ask {
            let message = if self.edit.is_some() {
                "Discard your changes to this settlement?"
            } else {
                "Leave and discard this settlement? It has not been saved."
            };
            self.modal = Some((Modal::confirm(message, "Discard"), Pending::ConfirmLeave));
            Nav::Stay
        } else if self.edit.is_some() {
            Nav::Pop
        } else {
            Nav::Home
        }
    }

    fn handle_completed(&mut self, key: KeyEvent, ctx: &mut Ctx) -> Nav {
        let pick = match key.code {
            KeyCode::Left => {
                self.action = self.action.saturating_sub(1);
                None
            }
            KeyCode::Right | KeyCode::Tab => {
                self.action = (self.action + 1).min(COMPLETED_ACTIONS.len() - 1);
                None
            }
            KeyCode::Up => {
                self.sheet_scroll = self.sheet_scroll.saturating_sub(1);
                None
            }
            KeyCode::Down => {
                self.sheet_scroll = self.sheet_scroll.saturating_add(1);
                None
            }
            KeyCode::PageUp => {
                self.sheet_scroll = self.sheet_scroll.saturating_sub(10);
                None
            }
            KeyCode::PageDown => {
                self.sheet_scroll = self.sheet_scroll.saturating_add(10);
                None
            }
            KeyCode::Home => {
                self.sheet_scroll = 0;
                None
            }
            KeyCode::End => {
                // Clamped to the end when drawn.
                self.sheet_scroll = u16::MAX;
                None
            }
            KeyCode::Enter => Some(self.action),
            KeyCode::Char('r') => Some(0),
            KeyCode::Char('s') => Some(1),
            KeyCode::Char('d') => Some(2),
            KeyCode::Esc => return self.ask_leave(ctx),
            _ => None,
        };
        match pick {
            Some(0) => {
                self.sheet_scroll = 0;
                self.act(ctx, |a, dice| a.session.reroll_all(dice));
                if !self.view.complete {
                    self.phase = Phase::Working;
                }
            }
            Some(1) => self.ask_name(),
            Some(2) => {
                self.modal = Some((
                    Modal::confirm(
                        "Delete this settlement? It has not been saved and cannot be recovered.",
                        "Delete",
                    ),
                    Pending::ConfirmDelete,
                ));
            }
            _ => {}
        }
        Nav::Stay
    }
}

impl Screen for GenerateScreen {
    fn title(&self) -> String {
        match (&self.edit, &self.phase) {
            (Some((_, name)), _) => format!("Edit · {name}"),
            (None, Phase::Completed) => "Settlement".into(),
            (None, Phase::Working) => "New Settlement".into(),
        }
    }

    fn render(&mut self, f: &mut Frame, area: Rect, ctx: &Ctx) {
        let s = ctx.styles;
        let [label, body] =
            Layout::vertical([Constraint::Length(2), Constraint::Fill(1)]).areas(area);
        f.render_widget(
            Paragraph::new(Line::styled(self.mode_label(), s.dim())),
            label,
        );

        if self.phase == Phase::Completed {
            self.render_completed(f, body, ctx);
        } else if self.edit.is_some() || self.view.mode == GenerationMode::OneTable {
            let (_, main, tags) = self.columns(body);
            let rows = self.stage_rows(ctx);
            // On the Authority Figures row the notes show the figures, so they
            // take the room the stage list does not need.
            let on_figures = self.rows.current(&rows).is_some_and(|&i| {
                let stage = &self.view.stages[i];
                stage.figures.is_some() || stage.roles.is_some() || stage.locations.is_some()
            });
            let constraints = if on_figures {
                [
                    Constraint::Length(rows.len() as u16 + 1),
                    Constraint::Fill(1),
                ]
            } else {
                [Constraint::Fill(1), Constraint::Length(8)]
            };
            let [table, notes] = Layout::vertical(constraints).areas(main);
            self.rows.render(f, table, &rows, &s, true);
            let mut lines = Vec::new();
            if let Some(&i) = self.rows.current(&rows) {
                let stage = &self.view.stages[i];
                lines.extend(Self::stage_notes(stage, ctx));
                if stage.roles.is_some() {
                    lines.push(Line::from(vec![
                        Span::styled("Enter", s.key()),
                        Span::styled(" change a role", s.dim()),
                    ]));
                    lines.extend(role_lines(stage, &s));
                }
                if let Some(record) = stage.locations.as_ref().and_then(|l| l.record.as_ref()) {
                    lines.push(Line::from(vec![
                        Span::styled("Enter", s.key()),
                        Span::styled(" change a count or a location's type", s.dim()),
                    ]));
                    lines.extend(location_lines(record, &s, 0));
                }
                if let Some(record) = stage.figures.as_ref().and_then(|f| f.record.as_ref()) {
                    lines.push(Line::from(vec![
                        Span::styled("Enter", s.key()),
                        Span::styled(" change a figure's details or a relationship", s.dim()),
                    ]));
                    lines.extend(figure_lines(record, &s, 0, stage.tags.is_some()));
                }
                lines.extend(outcome_lines(stage.outcome.as_ref(), &s));
                if let Some(t) = stage.tags.as_ref().filter(|t| !t.is_empty()) {
                    lines.push(tag_line(t, &s, 0));
                }
            }
            if let Some(e) = &self.error {
                lines.push(Line::styled(e.clone(), s.error()));
            }
            f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), notes);
            if let Some(t) = tags {
                self.render_active_tags(f, t, ctx);
            }
        } else {
            let index = match self.view.mode {
                GenerationMode::Automatic => self.view.next_pending.unwrap_or(0),
                _ => self.view.cursor.min(self.view.stages.len() - 1),
            };
            let (rail, main, tags) = self.columns(body);
            if let Some(r) = rail {
                self.render_rail(f, r, index, ctx);
            }
            let stage = &self.view.stages[index];
            let heading = if self.view.mode == GenerationMode::Automatic {
                if stage.incomplete.is_some() {
                    format!("{} needs a choice", stage.stage.name)
                } else {
                    format!("Choose a {}", stage.stage.name)
                }
            } else {
                stage.stage.name.clone()
            };
            self.render_stage_panel(f, main, index, ctx, heading);
            if let Some(t) = tags {
                self.render_active_tags(f, t, ctx);
            }
        }

        if let Some((modal, _)) = &mut self.modal {
            modal.render(f, area, &s);
        }
    }

    fn handle(&mut self, key: KeyEvent, ctx: &mut Ctx) -> Nav {
        if let Some(nav) = self.handle_modal(key, ctx) {
            return nav;
        }
        if self.phase == Phase::Completed {
            return self.handle_completed(key, ctx);
        }

        // One-table (and edits): every stage in one list.
        if self.edit.is_some() || self.view.mode == GenerationMode::OneTable {
            let rows = self.stage_rows(ctx);
            if self.rows.handle_nav(&rows, key) {
                return Nav::Stay;
            }
            let Some(&stage) = self.rows.current(&rows) else {
                return Nav::Stay;
            };
            match key.code {
                KeyCode::Char('r') => {
                    self.act(ctx, |a, dice| a.session.roll(stage, dice));
                }
                KeyCode::Char('t') if self.view.stages[stage].naming.is_some() => {
                    self.ask_custom_name(stage);
                }
                KeyCode::Char('c') if self.view.stages[stage].naming.is_some() => {
                    self.open_settlement_name_parts(stage);
                }
                // As in the GUI: roll the rest only when generating, not editing.
                KeyCode::Char('a') if self.edit.is_none() => {
                    self.act(ctx, |a, dice| a.session.roll_remaining(dice));
                }
                KeyCode::Enter if self.view.stages[stage].figures.is_some() => {
                    self.open_figure_editor(stage);
                }
                KeyCode::Enter if self.view.stages[stage].roles.is_some() => {
                    self.open_role_editor(stage);
                }
                KeyCode::Enter if self.view.stages[stage].locations.is_some() => {
                    self.open_location_editor(stage);
                }
                KeyCode::Enter => {
                    let view = &self.view.stages[stage];
                    let legal: Vec<_> = view.options.iter().filter(|o| o.legal).collect();
                    if legal.is_empty() {
                        self.error = Some("No result can be chosen for this stage.".into());
                    } else {
                        let items = legal
                            .iter()
                            .map(|o| {
                                PickItem::new(o.result.name.clone())
                                    .hint(o.range.map_or(String::new(), |r| r.to_string()))
                            })
                            .collect();
                        let ids = legal.iter().map(|o| o.result.id.clone()).collect();
                        let current = legal
                            .iter()
                            .position(|o| view.result.as_ref().is_some_and(|r| r.id == o.result.id))
                            .unwrap_or(0);
                        self.modal = Some((
                            Modal::pick(&format!("Choose {}", view.stage.name), items, current),
                            Pending::Choose(stage, ids),
                        ));
                    }
                }
                KeyCode::Char('f') if self.edit.is_none() => {
                    if self.view.complete {
                        self.phase = Phase::Completed;
                    } else {
                        self.error = Some("Resolve every stage first.".into());
                    }
                }
                KeyCode::Char('s') if self.edit.is_some() => {
                    if self.view.complete {
                        self.ask_name();
                    } else {
                        self.error = Some("Finish every stage before saving.".into());
                    }
                }
                KeyCode::Esc | KeyCode::Backspace => return self.ask_leave(ctx),
                _ => {}
            }
            return Nav::Stay;
        }

        // Automatic (waiting for a choice) and Incremental: one stage.
        let index = match self.view.mode {
            GenerationMode::Automatic => self.view.next_pending.unwrap_or(0),
            _ => self.view.cursor.min(self.view.stages.len() - 1),
        };
        let stage = self.view.stages[index].clone();
        let rows = Self::result_rows(&stage, ctx);
        if self.rows.handle_nav(&rows, key) {
            return Nav::Stay;
        }
        let automatic = self.view.mode == GenerationMode::Automatic;
        match key.code {
            KeyCode::Enter if stage.figures.is_some() => self.open_figure_editor(index),
            KeyCode::Enter if stage.roles.is_some() => self.open_role_editor(index),
            KeyCode::Enter if stage.locations.is_some() => self.open_location_editor(index),
            KeyCode::Enter => {
                if let Some(id) = self.rows.current(&rows).cloned() {
                    if self.act(ctx, |a, dice| a.session.choose(index, id, dice)) && automatic {
                        self.run_automatic(ctx);
                    }
                }
            }
            KeyCode::Char('r') if !automatic && stage.can_roll => {
                self.act(ctx, |a, dice| a.session.roll(index, dice));
            }
            KeyCode::Char('t') if !automatic && stage.naming.is_some() => {
                self.ask_custom_name(index);
            }
            KeyCode::Char('c') if !automatic && stage.naming.is_some() => {
                self.open_settlement_name_parts(index);
            }
            KeyCode::Char('n') | KeyCode::Right if !automatic => {
                let last = index + 1 == self.view.stages.len();
                if self.act(ctx, |a, _| a.session.advance()) {
                    self.rows = RowsState::default();
                    if last {
                        self.phase = Phase::Completed;
                    }
                }
            }
            KeyCode::Char('p') | KeyCode::Left if !automatic => {
                self.act(ctx, |a, _| Ok::<usize, GenerationError>(a.session.back()));
                self.rows = RowsState::default();
            }
            KeyCode::Esc | KeyCode::Backspace => return self.ask_leave(ctx),
            _ => {}
        }
        Nav::Stay
    }

    fn hints(&self) -> Vec<(&'static str, &'static str)> {
        if self.modal.is_some() {
            return vec![];
        }
        if self.phase == Phase::Completed {
            return vec![
                ("←→", "action"),
                ("Enter", "do"),
                ("r", "re-roll"),
                ("s", "save"),
                ("d", "delete"),
                ("Esc", "leave"),
            ];
        }
        // On the name stage, Enter chooses a method, r rolls a name and t
        // types one.
        let on_name = self.focused_stage().is_some_and(|s| s.naming.is_some());
        let on_figures = self.focused_stage().is_some_and(|s| s.figures.is_some());
        let on_roles = self.focused_stage().is_some_and(|s| s.roles.is_some());
        let on_locations = self.focused_stage().is_some_and(|s| s.locations.is_some());
        let (choose, roll) = if on_name {
            ("method", "roll name")
        } else if on_figures {
            ("edit figures", "generate")
        } else if on_roles {
            ("change role", "allocate")
        } else if on_locations {
            ("change", "roll all")
        } else {
            ("choose", "roll")
        };
        let mut hints = if self.edit.is_some() {
            vec![
                ("↑↓", "stage"),
                ("Enter", choose),
                ("r", roll),
                ("s", "save"),
                ("Esc", "cancel"),
            ]
        } else {
            match self.view.mode {
                GenerationMode::Automatic => {
                    vec![("↑↓", "move"), ("Enter", "choose"), ("Esc", "leave")]
                }
                GenerationMode::Incremental => vec![
                    ("↑↓", "result"),
                    ("Enter", choose),
                    ("r", roll),
                    ("→/n", "next"),
                    ("←/p", "previous"),
                    ("Esc", "leave"),
                ],
                GenerationMode::OneTable => vec![
                    ("↑↓", "stage"),
                    ("Enter", choose),
                    ("r", roll),
                    ("a", "roll rest"),
                    ("f", "finish"),
                    ("Esc", "leave"),
                ],
            }
        };
        if on_name && (self.edit.is_some() || self.view.mode != GenerationMode::Automatic) {
            hints.insert(3, ("t", "type name"));
            hints.insert(4, ("c", "name part"));
        }
        hints
    }
}

/// A stage's name in lists, with sub-stages indented under their stage.
fn stage_label(stage: &StageView) -> String {
    if stage.sub_stage {
        format!("  {}", stage.stage.name)
    } else {
        stage.stage.name.clone()
    }
}

/// "Figure 1 · Lead" (or Member, or just "Figure 1" for a single role).
fn role_slot_label(index: usize, allocation: Allocation) -> String {
    match allocation {
        Allocation::Single => format!("Figure {}", index + 1),
        Allocation::Lead => format!("Figure {} · Lead", index + 1),
        Allocation::Member => format!("Figure {} · Member", index + 1),
    }
}

/// Each figure's role, with the die and any unique-role fallback; or the
/// roles still to allocate.
fn role_lines(stage: &StageView, s: &crate::tui::theme::Styles) -> Vec<Line<'static>> {
    let Some(roles) = &stage.roles else {
        return Vec::new();
    };
    let mut lines = Vec::new();
    match &roles.record {
        Some(record) => {
            for (i, r) in record.roles.iter().enumerate() {
                let how = if r.roll.selected {
                    "chosen".to_owned()
                } else {
                    format!(
                        "{} on {}",
                        r.roll
                            .attempts
                            .last()
                            .map_or(String::new(), |a| a.roll.total.to_string()),
                        r.roll.dice.clone().unwrap_or_default()
                    )
                };
                lines.push(Line::from(vec![
                    Span::styled(format!("{:<18}", role_slot_label(i, r.allocation)), s.dim()),
                    Span::styled(format!("{:<28}", r.role.name), s.title()),
                    Span::styled(how, s.dim()),
                ]));
                if r.fallback.is_some() {
                    lines.push(Line::styled(
                        format!(
                            "{}{} was already held, so this becomes {}",
                            " ".repeat(18),
                            r.initial.name,
                            r.role.name
                        ),
                        s.dim(),
                    ));
                }
            }
        }
        None => {
            for (i, slot) in roles.slots.iter().enumerate() {
                lines.push(Line::from(vec![
                    Span::styled(
                        format!("{:<18}", role_slot_label(i, slot.allocation)),
                        s.dim(),
                    ),
                    Span::styled(slot.table.name.clone(), s.dim()),
                ]));
            }
        }
    }
    lines
}
