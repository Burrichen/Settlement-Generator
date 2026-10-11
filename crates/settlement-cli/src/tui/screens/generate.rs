//! Start: Automatic, Manual Incremental and Manual One-table generation, the
//! completed settlement, and editing a saved settlement. Every rule comes
//! from the core session; this screen only presents it and forwards keys.

use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};
use settlement_core::engine::{GenerationError, Selections};
use settlement_core::ids::ResultId;
use settlement_core::service::ActiveGeneration;
use settlement_core::session::{GenerationMode, SessionView, StageStatus, StageView};
use settlement_core::settlements::{MAX_NAME_LENGTH, SettlementId};

use super::common::{SheetRow, outcome_lines, sheet_lines, tag_line};
use crate::tui::widgets::{Modal, ModalResult, PickItem, Row, RowsState};
use crate::tui::{Ctx, Nav, Screen};

enum Pending {
    /// Choosing a result for a stage (One-table picker); the IDs map picker
    /// indices to results.
    Choose(usize, Vec<ResultId>),
    Name,
    ConfirmLeave,
    ConfirmDelete,
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
            .map(|stage| {
                let result = stage.result.as_ref().map_or_else(
                    || {
                        if stage.skipped.is_some() {
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
                        Span::raw(format!("{:<18}", stage.stage.name)),
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
        if stage.skipped.is_some() {
            lines.push(Line::styled(
                "This stage does not apply and is skipped.",
                s.dim(),
            ));
        }
        lines
    }

    // ----- rendering -----------------------------------------------------------

    fn render_rail(&self, f: &mut Frame, area: Rect, current: usize, ctx: &Ctx) {
        let s = &ctx.styles;
        let lines: Vec<Line> = self
            .view
            .stages
            .iter()
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
                    Span::styled(format!("{mark} {}", stage.stage.name), style),
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
        top.extend(Self::stage_notes(&stage, ctx));
        if let Some(e) = &self.error {
            top.push(Line::styled(e.clone(), s.error()));
        }
        top.extend(outcome_lines(stage.outcome.as_ref(), s));
        if stage.can_roll {
            top.push(Line::from(vec![
                Span::styled("r", s.key()),
                Span::styled(
                    format!(
                        " roll{}",
                        stage
                            .dice
                            .as_ref()
                            .map_or(String::new(), |d| format!(" {d}"))
                    ),
                    s.dim(),
                ),
            ]));
        }
        if let Some(tags) = &stage.tags {
            if !tags.is_empty() {
                top.push(tag_line(tags, s, 0));
            }
        }
        top.push(Line::default());
        top.push(Line::styled(
            if stage.can_roll {
                "OR CHOOSE A RESULT"
            } else {
                "CHOOSE A RESULT"
            },
            s.dim(),
        ));
        let height = (top.len() as u16).min(area.height);
        let [head, list] =
            Layout::vertical([Constraint::Length(height), Constraint::Fill(1)]).areas(area);
        f.render_widget(Paragraph::new(top).wrap(Wrap { trim: false }), head);
        let rows = Self::result_rows(&stage, ctx);
        if rows.is_empty() {
            f.render_widget(
                Paragraph::new(Line::styled("No result can be chosen here.", s.dim())),
                list,
            );
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
            })
            .collect();
        let mut lines = vec![Line::styled("─── ◆ ───", s.border()), Line::default()];
        lines.extend(sheet_lines(&rows, s));
        lines.push(Line::default());
        lines.push(Line::styled("Not saved yet", s.dim()));
        if let Some(e) = &self.error {
            lines.push(Line::styled(e.clone(), s.error()));
        }
        lines.push(Line::default());
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
        lines.push(Line::from(actions));
        f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), area);
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

    fn ask_name(&mut self) {
        let current = self
            .edit
            .as_ref()
            .map_or("", |(_, n)| n.as_str())
            .to_owned();
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
            KeyCode::Enter => Some(self.action),
            KeyCode::Char('r') => Some(0),
            KeyCode::Char('s') => Some(1),
            KeyCode::Char('d') => Some(2),
            KeyCode::Esc => return self.ask_leave(ctx),
            _ => None,
        };
        match pick {
            Some(0) => {
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
            let [table, notes] =
                Layout::vertical([Constraint::Fill(1), Constraint::Length(6)]).areas(main);
            let rows = self.stage_rows(ctx);
            self.rows.render(f, table, &rows, &s, true);
            let mut lines = Vec::new();
            if let Some(&i) = self.rows.current(&rows) {
                let stage = &self.view.stages[i];
                lines.extend(Self::stage_notes(stage, ctx));
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
                // As in the GUI: roll the rest only when generating, not editing.
                KeyCode::Char('a') if self.edit.is_none() => {
                    self.act(ctx, |a, dice| a.session.roll_remaining(dice));
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
        if self.edit.is_some() {
            return vec![
                ("↑↓", "stage"),
                ("Enter", "choose"),
                ("r", "roll"),
                ("s", "save"),
                ("Esc", "cancel"),
            ];
        }
        match self.view.mode {
            GenerationMode::Automatic => {
                vec![("↑↓", "move"), ("Enter", "choose"), ("Esc", "leave")]
            }
            GenerationMode::Incremental => vec![
                ("↑↓", "result"),
                ("Enter", "choose"),
                ("r", "roll"),
                ("→/n", "next"),
                ("←/p", "previous"),
                ("Esc", "leave"),
            ],
            GenerationMode::OneTable => vec![
                ("↑↓", "stage"),
                ("Enter", "choose"),
                ("r", "roll"),
                ("a", "roll rest"),
                ("f", "finish"),
                ("Esc", "leave"),
            ],
        }
    }
}
