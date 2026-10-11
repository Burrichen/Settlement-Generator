//! Reusable terminal widgets: selectable row lists, text input and modals.

use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Paragraph, Wrap};

use super::theme::Styles;

// ----- selectable rows ----------------------------------------------------------

/// One line of a list or form. Rows without an action cannot be selected
/// (headings, information, spacing).
pub struct Row<A> {
    pub line: Line<'static>,
    pub action: Option<A>,
}

impl<A> Row<A> {
    pub fn item(line: impl Into<Line<'static>>, action: A) -> Self {
        Row {
            line: line.into(),
            action: Some(action),
        }
    }

    pub fn info(line: impl Into<Line<'static>>) -> Self {
        Row {
            line: line.into(),
            action: None,
        }
    }

    pub fn blank() -> Self {
        Row::info(Line::default())
    }
}

/// Selection and scrolling for a list of [`Row`]s.
#[derive(Debug, Default, Clone)]
pub struct RowsState {
    pub selected: usize,
    offset: usize,
}

impl RowsState {
    fn selectable<A>(rows: &[Row<A>]) -> Vec<usize> {
        rows.iter()
            .enumerate()
            .filter(|(_, r)| r.action.is_some())
            .map(|(i, _)| i)
            .collect()
    }

    /// Keeps the selection on a selectable row.
    pub fn clamp<A>(&mut self, rows: &[Row<A>]) {
        let selectable = Self::selectable(rows);
        if selectable.is_empty() {
            self.selected = 0;
        } else if !selectable.contains(&self.selected) {
            self.selected = *selectable
                .iter()
                .find(|&&i| i >= self.selected)
                .unwrap_or(selectable.last().unwrap());
        }
    }

    pub fn move_by<A>(&mut self, rows: &[Row<A>], delta: i32) {
        let selectable = Self::selectable(rows);
        if selectable.is_empty() {
            return;
        }
        let pos = selectable
            .iter()
            .position(|&i| i == self.selected)
            .unwrap_or(0) as i32;
        let next = (pos + delta).clamp(0, selectable.len() as i32 - 1);
        self.selected = selectable[next as usize];
    }

    pub fn first<A>(&mut self, rows: &[Row<A>]) {
        if let Some(&i) = Self::selectable(rows).first() {
            self.selected = i;
        }
    }

    pub fn last<A>(&mut self, rows: &[Row<A>]) {
        if let Some(&i) = Self::selectable(rows).last() {
            self.selected = i;
        }
    }

    /// Selects the row whose action satisfies `pred`, if any.
    pub fn select_where<A>(&mut self, rows: &[Row<A>], pred: impl Fn(&A) -> bool) {
        if let Some(i) = rows
            .iter()
            .position(|r| r.action.as_ref().is_some_and(&pred))
        {
            self.selected = i;
        }
    }

    pub fn current<'a, A>(&self, rows: &'a [Row<A>]) -> Option<&'a A> {
        rows.get(self.selected).and_then(|r| r.action.as_ref())
    }

    /// Handles Up/Down/Home/End/PageUp/PageDown. Returns true if used.
    pub fn handle_nav<A>(&mut self, rows: &[Row<A>], key: KeyEvent) -> bool {
        match key.code {
            KeyCode::Up => self.move_by(rows, -1),
            KeyCode::Down => self.move_by(rows, 1),
            KeyCode::Home => self.first(rows),
            KeyCode::End => self.last(rows),
            KeyCode::PageUp => self.move_by(rows, -10),
            KeyCode::PageDown => self.move_by(rows, 10),
            _ => return false,
        }
        true
    }

    /// Draws the rows, scrolled so the selection is visible. The selected
    /// row gets a marker and the accent; only when `focused`.
    pub fn render<A>(
        &mut self,
        f: &mut Frame,
        area: Rect,
        rows: &[Row<A>],
        styles: &Styles,
        focused: bool,
    ) {
        self.clamp(rows);
        let height = area.height as usize;
        if height == 0 {
            return;
        }
        if self.selected < self.offset {
            self.offset = self.selected;
        } else if self.selected >= self.offset + height {
            self.offset = self.selected + 1 - height;
        }
        self.offset = self.offset.min(rows.len().saturating_sub(height));
        let lines: Vec<Line> = rows
            .iter()
            .enumerate()
            .skip(self.offset)
            .take(height)
            .map(|(i, row)| {
                let is_selected = focused && i == self.selected && row.action.is_some();
                let marker = if is_selected {
                    Span::styled("▸ ", styles.selected())
                } else {
                    Span::raw("  ")
                };
                let mut spans = vec![marker];
                spans.extend(row.line.spans.iter().cloned().map(|s| {
                    if is_selected {
                        let style = s.style.patch(Style::new().add_modifier(Modifier::BOLD));
                        Span::styled(s.content, style)
                    } else {
                        s
                    }
                }));
                let mut line = Line::from(spans);
                if is_selected && row.line.style.fg.is_none() {
                    line = line.style(Style::new().fg(styles.accent));
                } else {
                    line = line.style(row.line.style);
                }
                line
            })
            .collect();
        f.render_widget(Paragraph::new(lines), area);
        if rows.len() > height {
            let note = format!(" {}/{} ", self.selected + 1, rows.len());
            let w = note.chars().count() as u16;
            if area.width > w {
                let corner = Rect::new(area.right() - w, area.bottom() - 1, w, 1);
                f.render_widget(Paragraph::new(note).style(styles.dim()), corner);
            }
        }
    }
}

// ----- text input ---------------------------------------------------------------

#[derive(Debug, Default, Clone)]
pub struct TextInput {
    pub value: String,
    cursor: usize,
}

impl TextInput {
    pub fn new(value: &str) -> Self {
        TextInput {
            value: value.to_owned(),
            cursor: value.chars().count(),
        }
    }

    fn byte_index(&self) -> usize {
        self.value
            .char_indices()
            .nth(self.cursor)
            .map_or(self.value.len(), |(i, _)| i)
    }

    /// Edits the text. Returns true if the key was used.
    pub fn handle(&mut self, key: KeyEvent) -> bool {
        match key.code {
            KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                let at = self.byte_index();
                self.value.insert(at, c);
                self.cursor += 1;
            }
            KeyCode::Backspace if self.cursor > 0 => {
                self.cursor -= 1;
                let at = self.byte_index();
                self.value.remove(at);
            }
            KeyCode::Delete if self.cursor < self.value.chars().count() => {
                let at = self.byte_index();
                self.value.remove(at);
            }
            KeyCode::Left => self.cursor = self.cursor.saturating_sub(1),
            KeyCode::Right => self.cursor = (self.cursor + 1).min(self.value.chars().count()),
            KeyCode::Home => self.cursor = 0,
            KeyCode::End => self.cursor = self.value.chars().count(),
            _ => return false,
        }
        true
    }

    pub fn line(&self, styles: &Styles) -> Line<'static> {
        let chars: Vec<char> = self.value.chars().collect();
        let before: String = chars[..self.cursor].iter().collect();
        let at: String = chars.get(self.cursor).map_or(" ".into(), |c| c.to_string());
        let after: String = chars
            .get(self.cursor + 1..)
            .map_or(String::new(), |r| r.iter().collect());
        Line::from(vec![
            Span::raw(before),
            Span::styled(at, Style::new().add_modifier(Modifier::REVERSED)),
            Span::raw(after),
        ])
        .style(styles.title())
    }
}

// ----- modals ----------------------------------------------------------------------

pub struct PickItem {
    pub label: String,
    pub hint: String,
    pub enabled: bool,
}

impl PickItem {
    pub fn new(label: impl Into<String>) -> Self {
        PickItem {
            label: label.into(),
            hint: String::new(),
            enabled: true,
        }
    }

    pub fn hint(mut self, hint: impl Into<String>) -> Self {
        self.hint = hint.into();
        self
    }
}

pub enum Modal {
    Text {
        title: String,
        help: String,
        input: TextInput,
        error: Option<String>,
    },
    Pick {
        title: String,
        items: Vec<PickItem>,
        filter: TextInput,
        state: RowsState,
    },
    Check {
        title: String,
        items: Vec<String>,
        checked: Vec<bool>,
        state: RowsState,
    },
    Confirm {
        message: String,
        yes: String,
        choice_yes: bool,
    },
    Message {
        title: String,
        text: String,
    },
}

pub enum ModalResult {
    Open,
    Cancelled,
    Text(String),
    Picked(usize),
    Checked(Vec<bool>),
    Yes,
    Closed,
}

impl Modal {
    pub fn text(title: &str, help: &str, value: &str) -> Self {
        Modal::Text {
            title: title.into(),
            help: help.into(),
            input: TextInput::new(value),
            error: None,
        }
    }

    pub fn pick(title: &str, items: Vec<PickItem>, selected: usize) -> Self {
        Modal::Pick {
            title: title.into(),
            items,
            filter: TextInput::default(),
            state: RowsState {
                selected,
                ..Default::default()
            },
        }
    }

    pub fn check(title: &str, items: Vec<String>, checked: Vec<bool>) -> Self {
        Modal::Check {
            title: title.into(),
            items,
            checked,
            state: RowsState::default(),
        }
    }

    pub fn confirm(message: &str, yes: &str) -> Self {
        Modal::Confirm {
            message: message.into(),
            yes: yes.into(),
            choice_yes: false,
        }
    }

    pub fn message(title: &str, text: &str) -> Self {
        Modal::Message {
            title: title.into(),
            text: text.into(),
        }
    }

    /// Shows a validation problem inside a text modal and keeps it open.
    pub fn set_error(&mut self, message: String) {
        if let Modal::Text { error, .. } = self {
            *error = Some(message);
        }
    }

    fn pick_rows(items: &[PickItem], filter: &str, styles: &Styles) -> Vec<Row<usize>> {
        let needle = filter.trim().to_lowercase();
        items
            .iter()
            .enumerate()
            .filter(|(_, item)| {
                needle.is_empty()
                    || item.label.to_lowercase().contains(&needle)
                    || item.hint.to_lowercase().contains(&needle)
            })
            .map(|(i, item)| {
                let mut spans = vec![Span::raw(item.label.clone())];
                if !item.hint.is_empty() {
                    spans.push(Span::styled(format!("  {}", item.hint), styles.dim()));
                }
                let line = Line::from(spans);
                if item.enabled {
                    Row::item(line, i)
                } else {
                    Row::info(line.style(styles.dim()))
                }
            })
            .collect()
    }

    pub fn handle(&mut self, key: KeyEvent, styles: &Styles) -> ModalResult {
        match self {
            Modal::Text { input, .. } => match key.code {
                KeyCode::Esc => ModalResult::Cancelled,
                KeyCode::Enter => ModalResult::Text(input.value.clone()),
                _ => {
                    input.handle(key);
                    ModalResult::Open
                }
            },
            Modal::Pick {
                items,
                filter,
                state,
                ..
            } => {
                let rows = Self::pick_rows(items, &filter.value, styles);
                match key.code {
                    KeyCode::Esc => ModalResult::Cancelled,
                    KeyCode::Enter => {
                        state.clamp(&rows);
                        state
                            .current(&rows)
                            .map_or(ModalResult::Open, |&i| ModalResult::Picked(i))
                    }
                    _ if state.handle_nav(&rows, key) => ModalResult::Open,
                    _ => {
                        if filter.handle(key) {
                            let rows = Self::pick_rows(items, &filter.value, styles);
                            state.first(&rows);
                        }
                        ModalResult::Open
                    }
                }
            }
            Modal::Check {
                items,
                checked,
                state,
                ..
            } => {
                let rows: Vec<Row<usize>> = (0..items.len())
                    .map(|i| Row::item(Line::raw(""), i))
                    .collect();
                match key.code {
                    KeyCode::Esc => ModalResult::Cancelled,
                    KeyCode::Enter => ModalResult::Checked(checked.clone()),
                    KeyCode::Char(' ') => {
                        if let Some(c) = checked.get_mut(state.selected) {
                            *c = !*c;
                        }
                        ModalResult::Open
                    }
                    _ => {
                        state.handle_nav(&rows, key);
                        ModalResult::Open
                    }
                }
            }
            Modal::Confirm { choice_yes, .. } => match key.code {
                KeyCode::Esc => ModalResult::Cancelled,
                KeyCode::Left | KeyCode::Right | KeyCode::Tab => {
                    *choice_yes = !*choice_yes;
                    ModalResult::Open
                }
                KeyCode::Char('y') | KeyCode::Char('Y') => ModalResult::Yes,
                KeyCode::Char('n') | KeyCode::Char('N') => ModalResult::Cancelled,
                KeyCode::Enter if *choice_yes => ModalResult::Yes,
                KeyCode::Enter => ModalResult::Cancelled,
                _ => ModalResult::Open,
            },
            Modal::Message { .. } => match key.code {
                KeyCode::Esc | KeyCode::Enter | KeyCode::Char(' ') => ModalResult::Closed,
                _ => ModalResult::Open,
            },
        }
    }

    pub fn render(&mut self, f: &mut Frame, area: Rect, styles: &Styles) {
        let (title, body_height) = match self {
            Modal::Text { title, .. } => (title.clone(), 6),
            Modal::Pick { title, items, .. } => (title.clone(), (items.len() as u16 + 4).min(18)),
            Modal::Check { title, items, .. } => (title.clone(), (items.len() as u16 + 3).min(18)),
            Modal::Confirm { .. } => ("Confirm".into(), 5),
            Modal::Message { title, .. } => (title.clone(), 8),
        };
        let popup = centered(area, 64, body_height + 2);
        f.render_widget(Clear, popup);
        let block = Block::bordered()
            .border_style(Style::new().fg(styles.accent))
            .title(Span::styled(format!(" {title} "), styles.title()));
        let inner = block.inner(popup);
        f.render_widget(block, popup);

        match self {
            Modal::Text {
                help, input, error, ..
            } => {
                let mut lines = vec![input.line(styles), Line::default()];
                if let Some(e) = error {
                    lines.push(Line::styled(e.clone(), styles.error()));
                }
                lines.push(Line::styled(help.clone(), styles.dim()));
                lines.push(hint_line(&[("Enter", "accept"), ("Esc", "cancel")], styles));
                f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
            }
            Modal::Pick {
                items,
                filter,
                state,
                ..
            } => {
                let [filter_area, list_area, hint_area] = Layout::vertical([
                    Constraint::Length(1),
                    Constraint::Fill(1),
                    Constraint::Length(1),
                ])
                .areas(inner);
                let filter_line = if filter.value.is_empty() {
                    Line::styled("Type to filter", styles.dim())
                } else {
                    Line::from(vec![Span::styled("Filter: ", styles.dim())])
                        .patch_style(Style::default())
                        .spans
                        .into_iter()
                        .chain(filter.line(styles).spans)
                        .collect()
                };
                f.render_widget(Paragraph::new(filter_line), filter_area);
                let rows = Self::pick_rows(items, &filter.value, styles);
                state.render(f, list_area, &rows, styles, true);
                f.render_widget(
                    Paragraph::new(hint_line(
                        &[("↑↓", "move"), ("Enter", "choose"), ("Esc", "cancel")],
                        styles,
                    )),
                    hint_area,
                );
            }
            Modal::Check {
                items,
                checked,
                state,
                ..
            } => {
                let [list_area, hint_area] =
                    Layout::vertical([Constraint::Fill(1), Constraint::Length(1)]).areas(inner);
                let rows: Vec<Row<usize>> = items
                    .iter()
                    .enumerate()
                    .map(|(i, label)| {
                        let mark = if checked.get(i).copied().unwrap_or(false) {
                            "[x] "
                        } else {
                            "[ ] "
                        };
                        Row::item(Line::raw(format!("{mark}{label}")), i)
                    })
                    .collect();
                state.render(f, list_area, &rows, styles, true);
                f.render_widget(
                    Paragraph::new(hint_line(
                        &[("Space", "toggle"), ("Enter", "done"), ("Esc", "cancel")],
                        styles,
                    )),
                    hint_area,
                );
            }
            Modal::Confirm {
                message,
                yes,
                choice_yes,
            } => {
                let button = |label: &str, active: bool| {
                    if active {
                        Span::styled(format!("[ {label} ]"), styles.selected())
                    } else {
                        Span::styled(format!("  {label}  "), styles.dim())
                    }
                };
                let lines = vec![
                    Line::raw(message.clone()),
                    Line::default(),
                    Line::from(vec![
                        button("Keep it", !*choice_yes),
                        Span::raw("   "),
                        button(yes, *choice_yes),
                    ]),
                    hint_line(
                        &[("←→", "choose"), ("Enter", "confirm"), ("Esc", "cancel")],
                        styles,
                    ),
                ];
                f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
            }
            Modal::Message { text, .. } => {
                let mut lines: Vec<Line> = text.lines().map(|l| Line::raw(l.to_owned())).collect();
                lines.push(Line::default());
                lines.push(hint_line(&[("Enter", "close")], styles));
                f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
            }
        }
    }
}

/// A rectangle of at most `width`×`height` centred in `area`.
pub fn centered(area: Rect, width: u16, height: u16) -> Rect {
    let w = width.min(area.width.saturating_sub(2)).max(1);
    let h = height.min(area.height.saturating_sub(2)).max(1);
    Rect::new(
        area.x + (area.width.saturating_sub(w)) / 2,
        area.y + (area.height.saturating_sub(h)) / 2,
        w,
        h,
    )
}

/// "Key action · Key action" with keys in the accent.
pub fn hint_line(hints: &[(&str, &str)], styles: &Styles) -> Line<'static> {
    let mut spans = Vec::new();
    for (i, (key, action)) in hints.iter().enumerate() {
        if i > 0 {
            spans.push(Span::styled("  ·  ", styles.dim()));
        }
        spans.push(Span::styled((*key).to_owned(), styles.key()));
        spans.push(Span::raw(format!(" {action}")));
    }
    Line::from(spans)
}

/// A dim horizontal rule.
pub fn rule(width: u16, styles: &Styles) -> Line<'static> {
    Line::styled("─".repeat(width as usize), styles.border())
}
