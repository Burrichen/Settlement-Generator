//! Settings: the same persistent settings as the GUI, saved immediately.

use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use settlement_core::framework::{SettingKind, SettingValue};
use settlement_core::service::ContentSettingView;
use settlement_core::settings::{GenerationMethod, GenerationPresentation, Settings, Theme};

use super::flows::FlowsScreen;
use super::info::CreditsScreen;
use super::setting_defs::SettingDefsScreen;
use super::tables::TablesScreen;
use super::tags::TagsScreen;
use super::transfer::TransferScreen;
use crate::tui::widgets::{Modal, ModalResult, Row, RowsState};
use crate::tui::{Ctx, Nav, Screen};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Item {
    Method,
    Presentation,
    GodMode,
    TagView,
    /// A setting defined by the content (Bare Minimum Buildings among
    /// them), by position among those offered.
    Content(usize),
    Theme,
    Tags,
    Tables,
    Flows,
    SettingDefs,
    Transfer,
    RollDetails,
    Credits,
}

#[derive(Default)]
pub struct SettingsScreen {
    state: RowsState,
    message: Option<(String, bool)>,
    /// Typing a number or text, or ticking options, for content setting
    /// `.1`.
    modal: Option<(Modal, usize)>,
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
        let mut rows = vec![
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
        ];
        // Settings the generator content defines for itself.
        rows.extend(
            Self::content_settings(ctx)
                .iter()
                .enumerate()
                .map(|(i, view)| Row::item(content_line(view, ctx), Item::Content(i))),
        );
        rows.extend([
            Row::blank(),
            Row::info(Line::styled("CONTENT", s.dim())),
            Row::item(Line::raw("View Tags  ›"), Item::Tags),
            Row::item(Line::raw("View Tables  ›"), Item::Tables),
            Row::item(Line::raw("View Flows  ›"), Item::Flows),
            Row::item(Line::raw("View Generation Settings  ›"), Item::SettingDefs),
            Row::item(Line::raw("Import / Export  ›"), Item::Transfer),
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
            Row::item(
                toggle("Roll details (advanced)", set.roll_details, ctx),
                Item::RollDetails,
            ),
            Row::blank(),
            Row::info(Line::styled("ABOUT", s.dim())),
            Row::info(Line::from(vec![
                Span::raw(format!("{:<26}", "Version")),
                Span::styled(ctx.info.display_version(), s.title()),
            ])),
            Row::item(Line::raw("Credits  ›"), Item::Credits),
        ]);
        rows
    }

    /// The content-defined settings offered (planned ones are not).
    fn content_settings(ctx: &Ctx) -> Vec<ContentSettingView> {
        ctx.service
            .content_settings()
            .unwrap_or_default()
            .into_iter()
            .filter(|v| !v.setting.is_planned())
            .collect()
    }

    fn set_content(
        &mut self,
        ctx: &mut Ctx,
        id: &settlement_core::ids::SettingId,
        value: SettingValue,
    ) {
        self.message = Some(match ctx.service.set_content_setting(id, value) {
            Ok(_) => ("Saved".into(), false),
            Err(e) => (e, true),
        });
    }

    /// Enter on a number, text or multi-select setting: type or tick it.
    fn open_content(&mut self, index: usize, ctx: &mut Ctx) -> bool {
        let Some(view) = Self::content_settings(ctx).into_iter().nth(index) else {
            return false;
        };
        let modal = match (&view.setting.kind, &view.value) {
            (SettingKind::Number { min, max, .. }, SettingValue::Number(n)) => {
                let range = match (min, max) {
                    (Some(a), Some(b)) => format!("A whole number from {a} to {b}."),
                    (Some(a), None) => format!("A whole number, at least {a}."),
                    (None, Some(b)) => format!("A whole number, at most {b}."),
                    (None, None) => "A whole number.".to_owned(),
                };
                Modal::text(&view.setting.name, &range, &n.to_string())
            }
            (SettingKind::Text { .. }, SettingValue::Choice(text)) => {
                Modal::text(&view.setting.name, "Up to 200 characters.", text)
            }
            (SettingKind::MultiSelect { options, .. }, SettingValue::Many(ticked)) => Modal::check(
                &format!("{} (Space to tick)", view.setting.name),
                options.iter().map(|o| o.name.clone()).collect(),
                options.iter().map(|o| ticked.contains(&o.id)).collect(),
            ),
            _ => return false,
        };
        self.modal = Some((modal, index));
        true
    }

    fn finish_content(&mut self, index: usize, result: ModalResult, ctx: &mut Ctx) {
        let Some(view) = Self::content_settings(ctx).into_iter().nth(index) else {
            return;
        };
        let value = match (&view.setting.kind, result) {
            (SettingKind::Number { .. }, ModalResult::Text(t)) => match t.trim().parse::<i64>() {
                Ok(n) => SettingValue::Number(n),
                Err(_) => {
                    self.message = Some((format!("“{t}” is not a whole number."), true));
                    return;
                }
            },
            (SettingKind::Text { .. }, ModalResult::Text(t)) => SettingValue::Choice(t),
            (SettingKind::MultiSelect { options, .. }, ModalResult::Checked(ticks)) => {
                SettingValue::Many(
                    options
                        .iter()
                        .zip(ticks)
                        .filter(|(_, t)| *t)
                        .map(|(o, _)| o.id.clone())
                        .collect(),
                )
            }
            _ => return,
        };
        self.set_content(ctx, &view.setting.id, value);
    }

    /// Moves a content-defined setting to its next (or previous) value.
    fn cycle_content(&mut self, index: usize, ctx: &mut Ctx, forward: bool) {
        let Some(view) = Self::content_settings(ctx).into_iter().nth(index) else {
            return;
        };
        let next = match (&view.setting.kind, &view.value) {
            (SettingKind::Toggle { .. }, SettingValue::Toggle(on)) => SettingValue::Toggle(!on),
            (SettingKind::Choice { options, .. }, SettingValue::Choice(current)) => {
                let at = options.iter().position(|o| &o.id == current).unwrap_or(0);
                let n = options.len().max(1);
                let next = if forward {
                    (at + 1) % n
                } else {
                    (at + n - 1) % n
                };
                SettingValue::Choice(options[next].id.clone())
            }
            // ←→ step a number within its range.
            (SettingKind::Number { min, max, .. }, SettingValue::Number(n)) => {
                let next = if forward { n + 1 } else { n - 1 };
                if min.is_some_and(|m| next < m) || max.is_some_and(|m| next > m) {
                    return;
                }
                SettingValue::Number(next)
            }
            _ => return,
        };
        self.set_content(ctx, &view.setting.id, next);
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
        match item {
            Item::Content(i) => self.cycle_content(i, ctx, forward),
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
            Item::RollDetails => self.apply(ctx, |s| s.roll_details = !s.roll_details),
            Item::TagView => self.apply(ctx, |s| s.tag_view = !s.tag_view),
            Item::Theme => self.apply(ctx, |s| {
                s.theme = match s.theme {
                    Theme::ModernDark => Theme::OldTimeyDark,
                    Theme::OldTimeyDark => Theme::ModernDark,
                }
            }),
            Item::Tags
            | Item::Tables
            | Item::Flows
            | Item::SettingDefs
            | Item::Transfer
            | Item::Credits => {}
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
        if let Some((modal, _)) = &mut self.modal {
            modal.render(f, area, s);
        }
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
        if let Some((modal, index)) = &mut self.modal {
            let index = *index;
            match modal.handle(key, &ctx.styles) {
                ModalResult::Open => {}
                result => {
                    self.modal = None;
                    self.finish_content(index, result, ctx);
                }
            }
            return Nav::Stay;
        }
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
                Item::Flows => Nav::Push(Box::new(FlowsScreen::new(ctx))),
                Item::SettingDefs => Nav::Push(Box::new(SettingDefsScreen::new(ctx))),
                Item::Transfer => Nav::Push(Box::new(TransferScreen::new(ctx))),
                Item::Content(i) if key.code != KeyCode::Right && self.open_content(i, ctx) => {
                    Nav::Stay
                }
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

/// A content-defined setting as a toggle, a list of choices, or its value.
fn content_line(view: &ContentSettingView, ctx: &Ctx) -> Line<'static> {
    let s = &ctx.styles;
    let shown = |hint: &str| {
        Line::from(vec![
            Span::raw(format!("{:<26}", view.setting.name)),
            Span::styled(
                format!("[ {} ]", view.setting.describe_value(&view.value)),
                s.heading(),
            ),
            Span::styled(format!("  {hint}"), s.dim()),
        ])
    };
    match (&view.setting.kind, &view.value) {
        (SettingKind::Number { .. }, _) => shown("←→ or Enter to type"),
        (SettingKind::Text { .. }, _) => shown("Enter to type"),
        (SettingKind::MultiSelect { .. }, _) => shown("Enter to tick"),
        (SettingKind::Choice { options, .. }, SettingValue::Choice(current)) => {
            let names: Vec<&str> = options.iter().map(|o| o.name.as_str()).collect();
            let at = options.iter().position(|o| &o.id == current).unwrap_or(0);
            choice(&view.setting.name, &names, at, ctx, true)
        }
        (_, value) => toggle(
            &view.setting.name,
            *value == SettingValue::Toggle(true),
            ctx,
        ),
    }
}
