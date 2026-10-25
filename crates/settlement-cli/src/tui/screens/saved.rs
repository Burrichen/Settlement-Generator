//! View: saved settlements (list left, snapshot right), with Edit, Maps,
//! Export, Favourite/Unfavourite and Delete, and `i` to import a settlement
//! export file. A favourite never offers Delete, and the core refuses it
//! anyway.

use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};
use settlement_core::service::SavedSettlementView;
use settlement_core::settlements::{SettlementId, SettlementSummary};

use super::common::{SheetRow, sheet_lines};
use super::generate::GenerateScreen;
use super::maps::MapsScreen;
use crate::tui::widgets::{Modal, ModalResult, Row, RowsState};
use crate::tui::{Ctx, Nav, Screen};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Action {
    Edit,
    Maps,
    Export,
    Favourite,
    Unfavourite,
    Delete,
}

impl Action {
    fn label(self) -> &'static str {
        match self {
            Action::Edit => "Edit",
            Action::Maps => "Maps",
            Action::Export => "Export",
            Action::Favourite => "Favourite",
            Action::Unfavourite => "Unfavourite",
            Action::Delete => "Delete",
        }
    }
}

pub struct SavedScreen {
    list: Vec<SettlementSummary>,
    state: RowsState,
    detail: Option<SavedSettlementView>,
    /// Focus is on the actions (Right) rather than the list.
    in_actions: bool,
    /// How far the selected settlement's details are scrolled.
    detail_scroll: u16,
    action: usize,
    error: Option<String>,
    modal: Option<Modal>,
    /// The open modal asks for a file to import (not a delete).
    importing: bool,
}

impl SavedScreen {
    pub fn new(ctx: &mut Ctx) -> Self {
        let mut screen = SavedScreen {
            list: Vec::new(),
            state: RowsState::default(),
            detail: None,
            in_actions: false,
            detail_scroll: 0,
            action: 0,
            error: None,
            modal: None,
            importing: false,
        };
        screen.reload(ctx, None);
        screen
    }

    fn reload(&mut self, ctx: &mut Ctx, select: Option<SettlementId>) {
        match ctx.service.saved_list() {
            Ok(list) => self.list = list,
            Err(e) => self.error = Some(e),
        }
        let rows = self.rows(ctx);
        let wanted = select.or_else(|| self.selected_id(&rows));
        self.state = RowsState::default();
        if let Some(id) = wanted {
            self.state.select_where(&rows, |i| self.list[*i].id == id);
        }
        self.load_detail(ctx);
    }

    fn rows(&self, ctx: &Ctx) -> Vec<Row<usize>> {
        let s = &ctx.styles;
        self.list
            .iter()
            .enumerate()
            .map(|(i, item)| {
                // Favourites: a star before the name, in gold.
                let line = if item.favourite {
                    Line::from(vec![
                        Span::styled("★ ", s.favourite()),
                        Span::styled(item.name.clone(), s.favourite()),
                    ])
                    .style(s.favourite())
                } else {
                    Line::from(vec![Span::raw("  "), Span::raw(item.name.clone())])
                };
                Row::item(line, i)
            })
            .collect()
    }

    fn selected_id(&self, rows: &[Row<usize>]) -> Option<SettlementId> {
        self.state
            .current(rows)
            .and_then(|&i| self.list.get(i))
            .map(|s| s.id.clone())
    }

    fn load_detail(&mut self, ctx: &mut Ctx) {
        let rows = self.rows(ctx);
        self.state.clamp(&rows);
        self.detail = self
            .selected_id(&rows)
            .and_then(|id| match ctx.service.saved_get(&id) {
                Ok(d) => Some(d),
                Err(e) => {
                    self.error = Some(e);
                    None
                }
            });
    }

    fn actions(&self) -> Vec<Action> {
        match &self.detail {
            Some(d) if d.summary.favourite => {
                vec![
                    Action::Edit,
                    Action::Maps,
                    Action::Export,
                    Action::Unfavourite,
                ]
            }
            Some(_) => vec![
                Action::Edit,
                Action::Maps,
                Action::Export,
                Action::Favourite,
                Action::Delete,
            ],
            None => vec![],
        }
    }

    fn run(&mut self, action: Action, ctx: &mut Ctx) -> Nav {
        let Some(detail) = &self.detail else {
            return Nav::Stay;
        };
        let id = detail.summary.id.clone();
        match action {
            Action::Edit => match GenerateScreen::edit(ctx, &id, &detail.summary.name) {
                Ok(screen) => return Nav::Push(Box::new(screen)),
                Err(e) => self.error = Some(e),
            },
            Action::Maps => {
                let name = detail.summary.name.clone();
                return Nav::Push(Box::new(MapsScreen::new(ctx, &id, &name)));
            }
            Action::Export => match ctx
                .service
                .settlement_export(&id, ctx.export_dir.as_deref())
            {
                Ok(path) => {
                    self.error = None;
                    ctx.flash = Some(format!("Exported with all its maps to {}", path.display()));
                }
                Err(e) => self.error = Some(e),
            },
            Action::Favourite | Action::Unfavourite => {
                match ctx
                    .service
                    .saved_set_favourite(&id, action == Action::Favourite)
                {
                    Ok(_) => {
                        self.error = None;
                        self.reload(ctx, Some(id));
                        self.action = self.action.min(self.actions().len().saturating_sub(1));
                    }
                    Err(e) => self.error = Some(e),
                }
            }
            Action::Delete => {
                self.modal = Some(Modal::confirm(
                    &format!("Delete “{}”? This cannot be undone.", detail.summary.name),
                    "Delete",
                ));
            }
        }
        Nav::Stay
    }

    fn render_detail(&mut self, f: &mut Frame, area: Rect, ctx: &Ctx) {
        let s = &ctx.styles;
        let Some(d) = &self.detail else {
            f.render_widget(
                Paragraph::new(Line::styled("Nothing selected.", s.dim())),
                area,
            );
            return;
        };
        let mut title = vec![];
        if d.summary.favourite {
            title.push(Span::styled("★ ", s.favourite()));
        }
        title.push(Span::styled(
            d.summary.name.clone(),
            if d.summary.favourite {
                s.title().fg(s.favourite)
            } else {
                s.title()
            },
        ));
        let created = d.summary.created_at.date();
        let mut lines = vec![
            Line::from(title),
            Line::styled(
                format!(
                    "Generated with v{} · created {created}{}",
                    d.summary.app_version,
                    if d.summary.updated_at != d.summary.created_at {
                        format!(" · edited {}", d.summary.updated_at.date())
                    } else {
                        String::new()
                    }
                ),
                s.dim(),
            ),
            Line::default(),
        ];
        // Actions: highlighted when focused.
        let mut spans = Vec::new();
        for (i, action) in self.actions().iter().enumerate() {
            if i > 0 {
                spans.push(Span::raw("   "));
            }
            spans.push(if self.in_actions && i == self.action {
                Span::styled(format!("[ {} ]", action.label()), s.selected())
            } else {
                Span::styled(format!("  {}  ", action.label()), s.dim())
            });
        }
        lines.push(Line::from(spans));
        if let Some(e) = &self.error {
            lines.push(Line::styled(e.clone(), s.error()));
        }
        lines.push(Line::default());
        let rows: Vec<SheetRow> = d
            .stages
            .iter()
            .map(|st| SheetRow {
                label: st.label.clone(),
                value: st.result.as_ref().map_or_else(
                    || {
                        if st.skipped {
                            "Not applicable".into()
                        } else {
                            "—".into()
                        }
                    },
                    |r| r.name.clone(),
                ),
                source: st
                    .table
                    .as_ref()
                    .filter(|t| t.name != st.label)
                    .map(|t| t.name.clone()),
                tags: ctx.settings.tag_view.then(|| st.tags.clone()),
                sub_stage: st.sub_stage,
                // Roles are shown with each Authority Figure instead.
                skipped: st.skipped || st.roles.is_some(),
                figures: st.figures.clone(),
                locations: st.locations.clone(),
            })
            .collect();
        lines.extend(sheet_lines(&rows, s));
        lines.push(Line::default());
        lines.push(Line::styled(
            "A saved snapshot: later changes to tables do not alter it.",
            s.dim(),
        ));
        // Long settlements (e.g. many Authority Figures) scroll with ↑↓ once
        // the actions are focused.
        let overflow = (lines.len() as u16).saturating_sub(area.height);
        self.detail_scroll = self.detail_scroll.min(overflow);
        f.render_widget(
            Paragraph::new(lines)
                .wrap(Wrap { trim: false })
                .scroll((self.detail_scroll, 0)),
            area,
        );
    }
}

impl Screen for SavedScreen {
    fn title(&self) -> String {
        "Saved Settlements".into()
    }

    fn render(&mut self, f: &mut Frame, area: Rect, ctx: &Ctx) {
        let s = &ctx.styles;
        if self.list.is_empty() {
            let mut lines = vec![Line::styled(
                "No saved settlements yet. Generate one from Start and save it, or press i to \
                 import one.",
                s.dim(),
            )];
            if let Some(e) = &self.error {
                lines.push(Line::styled(e.clone(), s.error()));
            }
            f.render_widget(Paragraph::new(lines), area);
            if let Some(modal) = &mut self.modal {
                modal.render(f, area, s);
            }
            return;
        }
        let rows = self.rows(ctx);
        // Split pane where there is room; stacked otherwise.
        if area.width >= 80 {
            let [left, sep, right] = Layout::horizontal([
                Constraint::Percentage(25),
                Constraint::Length(3),
                Constraint::Fill(1),
            ])
            .areas(area);
            self.state.render(f, left, &rows, s, !self.in_actions);
            let bar: Vec<Line> = (0..sep.height)
                .map(|_| Line::styled(" │", s.border()))
                .collect();
            f.render_widget(Paragraph::new(bar), sep);
            self.render_detail(f, right, ctx);
        } else {
            let list_height = (rows.len() as u16 + 1).min(area.height / 3).max(3);
            let [top, bottom] =
                Layout::vertical([Constraint::Length(list_height), Constraint::Fill(1)])
                    .areas(area);
            self.state.render(f, top, &rows, s, !self.in_actions);
            self.render_detail(f, bottom, ctx);
        }
        if let Some(modal) = &mut self.modal {
            modal.render(f, area, s);
        }
    }

    fn handle(&mut self, key: KeyEvent, ctx: &mut Ctx) -> Nav {
        if self.importing {
            if let Some(modal) = &mut self.modal {
                match modal.handle(key, &ctx.styles) {
                    ModalResult::Open => {}
                    ModalResult::Text(path) => {
                        let path = path.trim().trim_matches('"');
                        match ctx
                            .service
                            .settlement_import_file(std::path::Path::new(path))
                        {
                            Ok(imported) => {
                                self.modal = None;
                                self.importing = false;
                                self.error = None;
                                self.reload(ctx, Some(imported.id));
                                ctx.flash = Some(format!(
                                    "Imported “{}” as a new settlement",
                                    imported.name
                                ));
                            }
                            Err(e) => modal.set_error(e),
                        }
                    }
                    _ => {
                        self.modal = None;
                        self.importing = false;
                    }
                }
                return Nav::Stay;
            }
        }
        if let Some(modal) = &mut self.modal {
            match modal.handle(key, &ctx.styles) {
                ModalResult::Open => {}
                ModalResult::Yes => {
                    self.modal = None;
                    if let Some(d) = &self.detail {
                        let id = d.summary.id.clone();
                        let rows = self.rows(ctx);
                        let index = self.state.current(&rows).copied().unwrap_or(0);
                        match ctx.service.saved_delete(&id) {
                            Ok(()) => {
                                let neighbour = self
                                    .list
                                    .get(index + 1)
                                    .or_else(|| index.checked_sub(1).and_then(|i| self.list.get(i)))
                                    .map(|s| s.id.clone());
                                self.in_actions = false;
                                self.reload(ctx, neighbour);
                                ctx.flash = Some("Deleted".into());
                            }
                            Err(e) => self.error = Some(e),
                        }
                    }
                }
                _ => self.modal = None,
            }
            return Nav::Stay;
        }

        if self.in_actions {
            let actions = self.actions();
            match key.code {
                KeyCode::Right | KeyCode::Tab => {
                    self.action = (self.action + 1).min(actions.len().saturating_sub(1));
                }
                KeyCode::Left if self.action > 0 => self.action -= 1,
                KeyCode::Left | KeyCode::Esc | KeyCode::Backspace => self.in_actions = false,
                KeyCode::Up => self.detail_scroll = self.detail_scroll.saturating_sub(1),
                KeyCode::Down => self.detail_scroll = self.detail_scroll.saturating_add(1),
                KeyCode::Enter | KeyCode::Char(' ') => {
                    if let Some(&a) = actions.get(self.action) {
                        return self.run(a, ctx);
                    }
                }
                _ => {}
            }
            return Nav::Stay;
        }

        let rows = self.rows(ctx);
        if self.state.handle_nav(&rows, key) {
            self.error = None;
            self.detail_scroll = 0;
            self.load_detail(ctx);
            return Nav::Stay;
        }
        match key.code {
            KeyCode::Right | KeyCode::Enter if self.detail.is_some() => {
                self.in_actions = true;
                self.action = 0;
                Nav::Stay
            }
            KeyCode::Char('i') => {
                self.importing = true;
                self.modal = Some(Modal::text(
                    "Import a settlement",
                    "Path to a .settlement.json file. It is added as a new settlement.",
                    "",
                ));
                Nav::Stay
            }
            KeyCode::Esc | KeyCode::Backspace | KeyCode::Left => Nav::Pop,
            _ => Nav::Stay,
        }
    }

    fn hints(&self) -> Vec<(&'static str, &'static str)> {
        if self.in_actions {
            vec![
                ("←→", "action"),
                ("↑↓", "scroll"),
                ("Enter", "do"),
                ("Esc", "back to list"),
            ]
        } else {
            vec![
                ("↑↓", "settlement"),
                ("→", "actions"),
                ("i", "import"),
                ("Esc", "back"),
            ]
        }
    }

    fn resume(&mut self, ctx: &mut Ctx) {
        let rows = self.rows(ctx);
        let id = self.selected_id(&rows);
        self.in_actions = false;
        self.reload(ctx, id);
    }
}
