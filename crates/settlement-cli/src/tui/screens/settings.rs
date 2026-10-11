//! Settings: the same persistent settings as the GUI, saved immediately.

use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use settlement_core::settings::{GenerationMethod, GenerationPresentation, Settings, Theme};

use super::info::CreditsScreen;
use super::tables::TablesScreen;
use super::tags::TagsScreen;
use crate::tui::widgets::{Row, RowsState};
use crate::tui::{Ctx, Nav, Screen};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Item {
    Method,
    Presentation,
    GodMode,
    TagView,
    Theme,
    Tags,
    Tables,
    Credits,
}

#[derive(Default)]
pub struct SettingsScreen {
    state: RowsState,
    message: Option<(String, bool)>,
}

/// "Label    [ A ]  B" with the current choice marked.
fn choice(
    label: &str,
    options: &[&str],
    current: usize,
    ctx: &Ctx,
    enabled: bool,
) -> Line<'static> {
    let s = &ctx.styles;
    let mut spans = vec![Span::raw(format!("{label:<26}"))];
    for (i, option) in options.iter().enumerate() {
        if i == current {
            spans.push(Span::styled(
                format!("[ {option} ]"),
                if enabled { s.heading() } else { s.dim() },
            ));
        } else {
            spans.push(Span::styled(format!("  {option}  "), s.dim()));
        }
    }
    let line = Line::from(spans);
    if enabled { line } else { line.style(s.dim()) }
}

fn toggle(label: &str, on: bool, ctx: &Ctx) -> Line<'static> {
    choice(label, &["Off", "On"], usize::from(on), ctx, true)
}

impl SettingsScreen {
    fn rows(ctx: &Ctx) -> Vec<Row<Item>> {
        let s = &ctx.styles;
        let set = &ctx.settings;
        let manual = set.presentation_applies();
        let presentation = choice(
            "Generation Presentation",
            &["Incremental", "One-table"],
            usize::from(set.generation_presentation == GenerationPresentation::OneTable),
            ctx,
            manual,
        );
        vec![
            Row::info(Line::styled("GENERATION", s.dim())),
            Row::item(
                choice(
                    "Generation Method",
                    &["Automatic", "Manual"],
                    usize::from(manual),
                    ctx,
                    true,
                ),
                Item::Method,
            ),
            // Disabled completely while Automatic: dimmed and not selectable.
            if manual {
                Row::item(presentation, Item::Presentation)
            } else {
                Row::info(Line::from(
                    presentation
                        .spans
                        .into_iter()
                        .chain([Span::styled("  (Manual only)", s.dim())])
                        .collect::<Vec<_>>(),
                ))
            },
            Row::item(toggle("God Mode", set.god_mode, ctx), Item::GodMode),
            Row::item(toggle("Tag View", set.tag_view, ctx), Item::TagView),
            Row::blank(),
            Row::info(Line::styled("CONTENT", s.dim())),
            Row::item(Line::raw("View Tags  ›"), Item::Tags),
            Row::item(Line::raw("View Tables  ›"), Item::Tables),
            Row::blank(),
            Row::info(Line::styled("APPEARANCE", s.dim())),
            Row::item(
                choice(
                    "Theme",
                    &["Modern Dark", "Old-Timey Dark"],
                    usize::from(set.theme == Theme::OldTimeyDark),
                    ctx,
                    true,
                ),
                Item::Theme,
            ),
            Row::blank(),
            Row::info(Line::styled("ABOUT", s.dim())),
            Row::info(Line::from(vec![
                Span::raw(format!("{:<26}", "Version")),
                Span::styled(ctx.info.display_version(), s.title()),
            ])),
            Row::item(Line::raw("Credits  ›"), Item::Credits),
        ]
    }

    /// Applies a change through the core and reports the outcome.
    fn apply(&mut self, ctx: &mut Ctx, change: impl FnOnce(&mut Settings)) {
        let mut next = ctx.settings;
        change(&mut next);
        self.message = Some(match ctx.save_settings(next) {
            Ok(()) => ("Saved".into(), false),
            Err(e) => (e, true),
        });
    }

    /// Changes the selected setting (`forward` for Right, else Left).
    fn cycle(&mut self, item: Item, ctx: &mut Ctx, forward: bool) {
        let _ = forward; // two-option settings: either direction toggles
        match item {
            Item::Method => self.apply(ctx, |s| {
                s.generation_method = match s.generation_method {
                    GenerationMethod::Automatic => GenerationMethod::Manual,
                    GenerationMethod::Manual => GenerationMethod::Automatic,
                }
            }),
            Item::Presentation => {
                let mut next = ctx.settings;
                let presentation = match next.generation_presentation {
                    GenerationPresentation::Incremental => GenerationPresentation::OneTable,
                    GenerationPresentation::OneTable => GenerationPresentation::Incremental,
                };
                // Refused by the core while Automatic; say so rather than
                // reporting a save that did not happen.
                match next.set_generation_presentation(presentation) {
                    Ok(()) => self.apply(ctx, |s| *s = next),
                    Err(e) => self.message = Some((e.to_string(), true)),
                }
            }
            Item::GodMode => self.apply(ctx, |s| s.god_mode = !s.god_mode),
            Item::TagView => self.apply(ctx, |s| s.tag_view = !s.tag_view),
            Item::Theme => self.apply(ctx, |s| {
                s.theme = match s.theme {
                    Theme::ModernDark => Theme::OldTimeyDark,
                    Theme::OldTimeyDark => Theme::ModernDark,
                }
            }),
            Item::Tags | Item::Tables | Item::Credits => {}
        }
    }
}

impl Screen for SettingsScreen {
    fn title(&self) -> String {
        "Settings".into()
    }

    fn render(&mut self, f: &mut Frame, area: Rect, ctx: &Ctx) {
        let s = &ctx.styles;
        let [list, status] =
            Layout::vertical([Constraint::Fill(1), Constraint::Length(1)]).areas(area);
        let rows = Self::rows(ctx);
        self.state.render(f, list, &rows, s, true);
        if let Some((text, error)) = &self.message {
            f.render_widget(
                Paragraph::new(Line::styled(
                    text.clone(),
                    if *error { s.error() } else { s.dim() },
                )),
                status,
            );
        }
    }

    fn handle(&mut self, key: KeyEvent, ctx: &mut Ctx) -> Nav {
        let rows = Self::rows(ctx);
        if self.state.handle_nav(&rows, key) {
            return Nav::Stay;
        }
        let Some(&item) = self.state.current(&rows) else {
            return Nav::Stay;
        };
        match key.code {
            KeyCode::Esc | KeyCode::Backspace => Nav::Pop,
            KeyCode::Left => {
                self.cycle(item, ctx, false);
                Nav::Stay
            }
            KeyCode::Right | KeyCode::Enter | KeyCode::Char(' ') => match item {
                Item::Tags => Nav::Push(Box::new(TagsScreen::new(ctx))),
                Item::Tables => Nav::Push(Box::new(TablesScreen::new(ctx))),
                Item::Credits => Nav::Push(Box::new(CreditsScreen)),
                other => {
                    self.cycle(other, ctx, true);
                    Nav::Stay
                }
            },
            _ => Nav::Stay,
        }
    }

    fn hints(&self) -> Vec<(&'static str, &'static str)> {
        vec![
            ("↑↓", "move"),
            ("←→", "change"),
            ("Enter", "change/open"),
            ("Esc", "back"),
        ]
    }

    fn resume(&mut self, _ctx: &mut Ctx) {
        self.message = None;
    }
}
