use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::text::Line;
use ratatui::widgets::Paragraph;

use super::generate::GenerateScreen;
use super::info::RoadmapScreen;
use super::saved::SavedScreen;
use super::settings::SettingsScreen;
use crate::tui::widgets::{Modal, ModalResult, Row, RowsState};
use crate::tui::{Ctx, Nav, Screen};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Item {
    Start,
    Settings,
    View,
    Roadmap,
    Exit,
}

#[derive(Default)]
pub struct MainMenu {
    state: RowsState,
    modal: Option<Modal>,
}

impl MainMenu {
    fn rows(ctx: &Ctx) -> Vec<Row<Item>> {
        let s = &ctx.styles;
        vec![
            Row::item(Line::raw("Start"), Item::Start),
            Row::item(Line::raw("Settings"), Item::Settings),
            Row::item(Line::raw("View"), Item::View),
            Row::item(Line::raw("Roadmap"), Item::Roadmap),
            // Disabled: dimmed, struck through, and not selectable.
            Row::info(Line::styled("DLC Content", s.disabled())),
            Row::item(Line::raw("Exit"), Item::Exit),
        ]
    }
}

impl Screen for MainMenu {
    fn title(&self) -> String {
        "Main Menu".into()
    }

    fn render(&mut self, f: &mut Frame, area: Rect, ctx: &Ctx) {
        let s = &ctx.styles;
        let rows = Self::rows(ctx);
        let width = 30.min(area.width);
        let height = (rows.len() as u16 + 4).min(area.height);
        let x = area.x + (area.width - width) / 2;
        let y = area.y + area.height.saturating_sub(height) / 2;
        let block = Rect::new(x, y, width, height);
        let [title, ornament, list] = Layout::vertical([
            Constraint::Length(1),
            Constraint::Length(2),
            Constraint::Fill(1),
        ])
        .areas(block);
        f.render_widget(
            Paragraph::new(Line::styled(ctx.info.name.clone(), s.title()))
                .alignment(ratatui::layout::Alignment::Center),
            title,
        );
        f.render_widget(
            Paragraph::new(Line::styled("─────── ◆ ───────", s.border()))
                .alignment(ratatui::layout::Alignment::Center),
            ornament,
        );
        self.state.render(f, list, &rows, s, true);
        if let Some(modal) = &mut self.modal {
            modal.render(f, area, s);
        }
    }

    fn handle(&mut self, key: KeyEvent, ctx: &mut Ctx) -> Nav {
        if let Some(modal) = &mut self.modal {
            if !matches!(modal.handle(key, &ctx.styles), ModalResult::Open) {
                self.modal = None;
            }
            return Nav::Stay;
        }
        let rows = Self::rows(ctx);
        if self.state.handle_nav(&rows, key) {
            return Nav::Stay;
        }
        match key.code {
            KeyCode::Enter => match self.state.current(&rows) {
                Some(Item::Start) => match GenerateScreen::start(ctx) {
                    Ok(screen) => Nav::Push(Box::new(screen)),
                    Err(e) => {
                        self.modal = Some(Modal::message("Generation could not start", &e));
                        Nav::Stay
                    }
                },
                Some(Item::Settings) => Nav::Push(Box::new(SettingsScreen::default())),
                Some(Item::View) => Nav::Push(Box::new(SavedScreen::new(ctx))),
                Some(Item::Roadmap) => Nav::Push(Box::new(RoadmapScreen)),
                Some(Item::Exit) => Nav::Quit,
                None => Nav::Stay,
            },
            KeyCode::Esc | KeyCode::Char('q') => Nav::Quit,
            _ => Nav::Stay,
        }
    }

    fn hints(&self) -> Vec<(&'static str, &'static str)> {
        vec![("↑↓", "move"), ("Enter", "select"), ("q", "exit")]
    }
}
