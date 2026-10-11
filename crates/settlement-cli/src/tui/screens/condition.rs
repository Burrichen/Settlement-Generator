//! A stepped editor for tag conditions (has tag / all of / any of / not),
//! shown as an indented tree. Used for every condition in the editors.

use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use settlement_core::content::Condition;
use settlement_core::ids::TagId;

use crate::tui::Ctx;
use crate::tui::widgets::{Modal, ModalResult, PickItem, Row, RowsState};

/// Where an edit applies: a path of child indices from the root.
type Path = Vec<usize>;

#[derive(Clone, Debug, PartialEq)]
enum Act {
    Node(Path),
    AddRoot,
}

enum Pending {
    Kind(Path),
    Tag(Path),
}

pub enum Outcome {
    Stay,
    Done,
}

pub struct ConditionEditor {
    pub title: String,
    pub value: Option<Condition>,
    tags: Vec<(TagId, String)>,
    state: RowsState,
    modal: Option<(Modal, Pending)>,
}

const KINDS: [&str; 5] = ["Has tag", "All of", "Any of", "Not", "Remove"];

impl ConditionEditor {
    pub fn new(title: &str, value: Option<Condition>, tags: Vec<(TagId, String)>) -> Self {
        ConditionEditor {
            title: title.into(),
            value,
            tags,
            state: RowsState::default(),
            modal: None,
        }
    }

    fn tag_name(&self, id: &TagId) -> String {
        if id.as_str().is_empty() {
            return "(choose a tag)".into();
        }
        self.tags
            .iter()
            .find(|(t, _)| t == id)
            .map_or_else(|| format!("{id} (missing)"), |(_, n)| n.clone())
    }

    fn rows(&self, ctx: &Ctx) -> Vec<Row<Act>> {
        let s = &ctx.styles;
        let Some(root) = &self.value else {
            return vec![
                Row::info(Line::styled("Always (no condition).", s.dim())),
                Row::item(Line::raw("+ Add condition"), Act::AddRoot),
            ];
        };
        let mut rows = Vec::new();
        self.flatten(root, Vec::new(), 0, &mut rows);
        rows
    }

    fn flatten(&self, c: &Condition, path: Path, depth: usize, out: &mut Vec<Row<Act>>) {
        let label = match c {
            Condition::HasTag(t) => format!("Has tag: {}", self.tag_name(t)),
            Condition::All(_) => "All of:".into(),
            Condition::Any(_) => "Any of:".into(),
            Condition::Not(_) => "Not:".into(),
        };
        out.push(Row::item(
            Line::raw(format!("{}{label}", "    ".repeat(depth))),
            Act::Node(path.clone()),
        ));
        match c {
            Condition::All(items) | Condition::Any(items) => {
                for (i, child) in items.iter().enumerate() {
                    let mut p = path.clone();
                    p.push(i);
                    self.flatten(child, p, depth + 1, out);
                }
            }
            Condition::Not(child) => {
                let mut p = path.clone();
                p.push(0);
                self.flatten(child, p, depth + 1, out);
            }
            Condition::HasTag(_) => {}
        }
    }

    fn node_mut<'a>(c: &'a mut Condition, path: &[usize]) -> &'a mut Condition {
        match (path.split_first(), c) {
            (None, c) => c,
            (Some((i, rest)), Condition::All(v) | Condition::Any(v)) => {
                Self::node_mut(&mut v[*i], rest)
            }
            (Some((_, rest)), Condition::Not(b)) => Self::node_mut(b, rest),
            (Some(_), c) => c,
        }
    }

    fn set_node(&mut self, path: &[usize], node: Option<Condition>) {
        let Some(root) = &mut self.value else {
            self.value = node;
            return;
        };
        match (path.split_last(), node) {
            (None, node) => self.value = node,
            (Some((last, parent)), node) => {
                let parent_node = Self::node_mut(root, parent);
                match (parent_node, node) {
                    (Condition::All(v) | Condition::Any(v), Some(n)) => v[*last] = n,
                    (Condition::All(v) | Condition::Any(v), None) => {
                        v.remove(*last);
                    }
                    (Condition::Not(b), Some(n)) => **b = n,
                    // Removing the only child of "Not" leaves a blank test.
                    (p @ Condition::Not(_), None) => *p = Condition::HasTag(TagId::new("")),
                    _ => {}
                }
            }
        }
    }

    fn tag_picker(&self) -> Modal {
        Modal::pick(
            "Choose a tag",
            self.tags
                .iter()
                .map(|(id, name)| PickItem::new(name.clone()).hint(id.to_string()))
                .collect(),
            0,
        )
    }

    pub fn handle(&mut self, key: KeyEvent, ctx: &mut Ctx) -> Outcome {
        if let Some((modal, pending)) = &mut self.modal {
            let result = modal.handle(key, &ctx.styles);
            match (result, pending) {
                (ModalResult::Open, _) => return Outcome::Stay,
                (ModalResult::Picked(k), Pending::Kind(path)) => {
                    let path = path.clone();
                    self.modal = None;
                    let blank = || Condition::HasTag(TagId::new(""));
                    let node = match k {
                        0 => Some(blank()),
                        1 => Some(Condition::All(vec![blank()])),
                        2 => Some(Condition::Any(vec![blank()])),
                        3 => Some(Condition::Not(Box::new(blank()))),
                        _ => None,
                    };
                    let ask_tag = k == 0;
                    self.set_node(&path, node);
                    if ask_tag {
                        self.modal = Some((self.tag_picker(), Pending::Tag(path)));
                    }
                }
                (ModalResult::Picked(k), Pending::Tag(path)) => {
                    let path = path.clone();
                    self.modal = None;
                    let tag = self.tags[k].0.clone();
                    self.set_node(&path, Some(Condition::HasTag(tag)));
                }
                _ => self.modal = None,
            }
            return Outcome::Stay;
        }
        let rows = self.rows(ctx);
        if self.state.handle_nav(&rows, key) {
            return Outcome::Stay;
        }
        let current = self.state.current(&rows).cloned();
        match (key.code, current) {
            (KeyCode::Esc | KeyCode::Backspace, _) => return Outcome::Done,
            (KeyCode::Enter, Some(Act::AddRoot)) => {
                self.modal = Some((
                    Modal::pick(
                        "Condition type",
                        KINDS[..4].iter().map(|k| PickItem::new(*k)).collect(),
                        0,
                    ),
                    Pending::Kind(Vec::new()),
                ));
            }
            (KeyCode::Enter, Some(Act::Node(path))) => {
                let node = self
                    .value
                    .as_mut()
                    .map(|r| Self::node_mut(r, &path).clone());
                if let Some(Condition::HasTag(_)) = node {
                    // Has-tag nodes: pick the tag; "t" changes the type.
                    self.modal = Some((self.tag_picker(), Pending::Tag(path)));
                } else {
                    self.modal = Some((
                        Modal::pick(
                            "Condition type",
                            KINDS.iter().map(|k| PickItem::new(*k)).collect(),
                            0,
                        ),
                        Pending::Kind(path),
                    ));
                }
            }
            (KeyCode::Char('t'), Some(Act::Node(path))) => {
                self.modal = Some((
                    Modal::pick(
                        "Condition type",
                        KINDS.iter().map(|k| PickItem::new(*k)).collect(),
                        0,
                    ),
                    Pending::Kind(path),
                ));
            }
            (KeyCode::Char('a'), Some(Act::Node(path))) => {
                if let Some(root) = &mut self.value
                    && let Condition::All(v) | Condition::Any(v) = Self::node_mut(root, &path)
                {
                    v.push(Condition::HasTag(TagId::new("")));
                    let mut child = path.clone();
                    child.push(v.len() - 1);
                    self.modal = Some((self.tag_picker(), Pending::Tag(child)));
                }
            }
            (KeyCode::Char('d') | KeyCode::Delete, Some(Act::Node(path))) => {
                self.set_node(&path, None);
            }
            _ => {}
        }
        Outcome::Stay
    }

    pub fn render(&mut self, f: &mut Frame, area: Rect, ctx: &Ctx) {
        let s = &ctx.styles;
        let [head, list] =
            Layout::vertical([Constraint::Length(3), Constraint::Fill(1)]).areas(area);
        f.render_widget(
            Paragraph::new(vec![
                Line::styled(self.title.clone(), s.title()),
                Line::from(vec![
                    Span::styled("Enter", s.key()),
                    Span::styled(" edit  ", s.dim()),
                    Span::styled("t", s.key()),
                    Span::styled(" change type  ", s.dim()),
                    Span::styled("a", s.key()),
                    Span::styled(" add to all/any  ", s.dim()),
                    Span::styled("d", s.key()),
                    Span::styled(" remove  ", s.dim()),
                    Span::styled("Esc", s.key()),
                    Span::styled(" done", s.dim()),
                ]),
            ]),
            head,
        );
        let rows = self.rows(ctx);
        self.state.render(f, list, &rows, s, true);
        if let Some((modal, _)) = &mut self.modal {
            modal.render(f, area, s);
        }
    }
}

/// One-line summary of a condition.
pub fn summary(c: &Option<Condition>, tags: &[(TagId, String)]) -> String {
    fn go(c: &Condition, tags: &[(TagId, String)]) -> String {
        match c {
            Condition::HasTag(t) => tags
                .iter()
                .find(|(id, _)| id == t)
                .map_or_else(|| t.to_string(), |(_, n)| n.clone()),
            Condition::All(v) => format!(
                "all of ({})",
                v.iter().map(|c| go(c, tags)).collect::<Vec<_>>().join(", ")
            ),
            Condition::Any(v) => format!(
                "any of ({})",
                v.iter().map(|c| go(c, tags)).collect::<Vec<_>>().join(", ")
            ),
            Condition::Not(b) => format!("not {}", go(b, tags)),
        }
    }
    c.as_ref().map_or_else(|| "always".into(), |c| go(c, tags))
}
