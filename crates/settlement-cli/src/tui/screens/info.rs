//! Roadmap and Credits.

use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Alignment, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph};

use crate::tui::widgets::centered;
use crate::tui::{Ctx, Nav, Screen};

fn back(key: KeyEvent) -> Nav {
    match key.code {
        KeyCode::Esc | KeyCode::Backspace | KeyCode::Left | KeyCode::Enter => Nav::Pop,
        _ => Nav::Stay,
    }
}

pub struct RoadmapScreen;

impl Screen for RoadmapScreen {
    fn title(&self) -> String {
        "Roadmap".into()
    }

    fn render(&mut self, f: &mut Frame, area: Rect, ctx: &Ctx) {
        let s = &ctx.styles;
        let mut lines = vec![
            Line::styled(
                format!("Installed version {}", ctx.info.display_version()),
                s.dim(),
            ),
            Line::default(),
        ];
        for entry in settlement_core::roadmap::roadmap() {
            lines.push(Line::from(vec![
                Span::styled("◆ ", s.heading()),
                Span::styled(entry.label.to_uppercase(), s.dim()),
            ]));
            lines.push(Line::styled(format!("  {}", entry.summary), s.title()));
            lines.push(Line::default());
        }
        f.render_widget(Paragraph::new(lines), area);
    }

    fn handle(&mut self, key: KeyEvent, _ctx: &mut Ctx) -> Nav {
        back(key)
    }

    fn hints(&self) -> Vec<(&'static str, &'static str)> {
        vec![("Esc", "back")]
    }
}

pub struct CreditsScreen;

impl Screen for CreditsScreen {
    fn title(&self) -> String {
        "Credits".into()
    }

    fn render(&mut self, f: &mut Frame, area: Rect, ctx: &Ctx) {
        let s = &ctx.styles;
        let frame = centered(area, 44, 7);
        let block = Block::bordered()
            .border_type(ratatui::widgets::BorderType::Double)
            .border_style(ratatui::style::Style::new().fg(s.accent));
        let inner = block.inner(frame);
        f.render_widget(block, frame);
        let lines = vec![
            Line::styled("─── ◆ ───", s.border()),
            Line::default(),
            Line::styled(
                "designed and made by Burrichen",
                s.title().add_modifier(ratatui::style::Modifier::ITALIC),
            ),
            Line::default(),
            Line::styled("─── ◆ ───", s.border()),
        ];
        f.render_widget(Paragraph::new(lines).alignment(Alignment::Center), inner);
    }

    fn handle(&mut self, key: KeyEvent, _ctx: &mut Ctx) -> Nav {
        back(key)
    }

    fn hints(&self) -> Vec<(&'static str, &'static str)> {
        vec![("Esc", "back")]
    }
}
