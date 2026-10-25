//! View Flows: the flow list and a tree editor for one flow. The terminal
//! counterpart of the GUI's node graph: the same steps, the same edits
//! (made by the core), plain-language conditions, read-only pseudocode,
//! problems, undo/redo and kept drafts.

use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};
use settlement_core::content::{Condition, Modifier};
use settlement_core::framework::describe::FlowContext;
use settlement_core::framework::edit::{FlowEdit, Position, Slot, StepKind, position_of};
use settlement_core::framework::layers::ItemKind;
use settlement_core::framework::{Count, Flow, FlowStep, StepAction, StepModifier};
use settlement_core::ids::{FlowId, ObjectTypeId, SettingId, StepId, TableId, TagId};
use settlement_core::layers::Origin;
use settlement_core::service::{AppService, FlowCheck};
use settlement_core::validation::Severity;

use super::condition::{ConditionEditor, Outcome};
use super::open::{ItemRef, of_usage, open};
use super::tags::origin_label;
use super::test::{Target, TestScreen};
use crate::tui::widgets::{Modal, ModalResult, PickItem, Row, RowsState};
use crate::tui::{Ctx, Nav, Screen};

// ----- list ---------------------------------------------------------------------

enum ListPending {
    Filter,
    Restore(String),
}

pub struct FlowsScreen {
    items: Vec<(String, String, Option<Origin>, bool)>,
    state: RowsState,
    filter: String,
    error: Option<String>,
    modal: Option<(Modal, ListPending)>,
}

impl FlowsScreen {
    pub fn new(ctx: &mut Ctx) -> Self {
        let mut s = FlowsScreen {
            items: Vec::new(),
            state: RowsState::default(),
            filter: String::new(),
            error: None,
            modal: None,
        };
        s.resume(ctx);
        s
    }

    fn rows(&self, ctx: &Ctx) -> Vec<Row<(String, bool)>> {
        let s = &ctx.styles;
        let needle = self.filter.to_lowercase();
        self.items
            .iter()
            .filter(|(id, name, ..)| {
                needle.is_empty()
                    || name.to_lowercase().contains(&needle)
                    || id.to_lowercase().contains(&needle)
            })
            .map(|(id, name, origin, deleted)| {
                let style = if *deleted { s.dim() } else { s.title() };
                Row::item(
                    Line::from(vec![
                        Span::styled(format!("{name:<34}"), style),
                        Span::styled(format!("{id:<38}"), s.dim()),
                        Span::styled(origin_label(*origin, *deleted), s.dim()),
                    ]),
                    (id.clone(), *deleted),
                )
            })
            .collect()
    }

    fn open(&mut self, ctx: &mut Ctx, id: &str) -> Nav {
        let id = FlowId::new(id);
        match ctx.service.flow(&id) {
            Ok(saved) => {
                let origin = self
                    .items
                    .iter()
                    .find(|i| i.0 == id.as_str())
                    .and_then(|i| i.2);
                let kept = ctx.service.flow_draft(&id).ok().flatten();
                Nav::Push(Box::new(FlowEditor::open(ctx, Some(saved), kept, origin)))
            }
            Err(e) => {
                self.error = Some(e);
                Nav::Stay
            }
        }
    }
}

impl Screen for FlowsScreen {
    fn title(&self) -> String {
        "Flows".into()
    }

    fn render(&mut self, f: &mut Frame, area: Rect, ctx: &Ctx) {
        let s = &ctx.styles;
        let [head, list] =
            Layout::vertical([Constraint::Length(2), Constraint::Fill(1)]).areas(area);
        let mut line = vec![Span::styled(
            if self.filter.is_empty() {
                "All flows".to_owned()
            } else {
                format!("Filter: {}", self.filter)
            },
            s.dim(),
        )];
        if let Some(e) = &self.error {
            line.push(Span::styled(format!("   {e}"), s.error()));
        }
        f.render_widget(Paragraph::new(Line::from(line)), head);
        let rows = self.rows(ctx);
        self.state.render(f, list, &rows, s, true);
        if let Some((m, _)) = &mut self.modal {
            m.render(f, area, s);
        }
    }

    fn handle(&mut self, key: KeyEvent, ctx: &mut Ctx) -> Nav {
        if let Some((modal, pending)) = &mut self.modal {
            match (modal.handle(key, &ctx.styles), pending) {
                (ModalResult::Open, _) => return Nav::Stay,
                (ModalResult::Text(t), ListPending::Filter) => {
                    self.filter = t;
                    self.state = RowsState::default();
                }
                (ModalResult::Yes, ListPending::Restore(id)) => {
                    let id = id.clone();
                    if let Err(e) = ctx.service.reset_framework_item(ItemKind::Flow, &id) {
                        self.error = Some(e);
                    }
                }
                _ => {}
            }
            self.modal = None;
            self.resume(ctx);
            return Nav::Stay;
        }
        let rows = self.rows(ctx);
        if self.state.handle_nav(&rows, key) {
            return Nav::Stay;
        }
        match key.code {
            KeyCode::Enter => match self.state.current(&rows).cloned() {
                Some((id, true)) => {
                    self.modal = Some((
                        Modal::confirm("Restore this deleted default flow?", "Restore"),
                        ListPending::Restore(id),
                    ));
                    Nav::Stay
                }
                Some((id, false)) => self.open(ctx, &id),
                None => Nav::Stay,
            },
            KeyCode::Char('n') => Nav::Push(Box::new(FlowEditor::open(
                ctx,
                None,
                Some(AppService::new_flow()),
                None,
            ))),
            KeyCode::Char('c') => match self.state.current(&rows).cloned() {
                Some((id, false)) => match ctx.service.duplicate_flow(&FlowId::new(&id)) {
                    Ok(copy) => Nav::Push(Box::new(FlowEditor::open(ctx, None, Some(copy), None))),
                    Err(e) => {
                        self.error = Some(e);
                        Nav::Stay
                    }
                },
                _ => Nav::Stay,
            },
            KeyCode::Char('/') => {
                self.modal = Some((
                    Modal::text("Filter flows", "Name or ID; empty shows all.", &self.filter),
                    ListPending::Filter,
                ));
                Nav::Stay
            }
            KeyCode::Esc | KeyCode::Backspace => Nav::Pop,
            _ => Nav::Stay,
        }
    }

    fn hints(&self) -> Vec<(&'static str, &'static str)> {
        vec![
            ("↑↓", "move"),
            ("Enter", "edit"),
            ("n", "new flow"),
            ("c", "copy"),
            ("/", "filter"),
            ("Esc", "back"),
        ]
    }

    fn resume(&mut self, ctx: &mut Ctx) {
        match ctx.service.framework_items() {
            Ok(items) => {
                self.items = items
                    .into_iter()
                    .filter(|i| i.kind == ItemKind::Flow)
                    .map(|i| (i.id, i.name, i.origin, i.deleted))
                    .collect();
            }
            Err(e) => self.error = Some(e),
        }
    }
}

// ----- editor -------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq)]
enum View {
    Tree,
    Pseudocode,
    Problems,
}

#[derive(Clone, Debug, PartialEq)]
enum Act {
    Name,
    Description,
    Step(StepId),
    /// A place a step can be added (or a moved step put).
    Port(Position),
    Detached(StepId),
    /// Something that runs this flow; Enter opens it.
    Open(ItemRef),
}

/// A field of the step settings page.
#[derive(Clone, Debug, PartialEq)]
enum Field {
    Label,
    Table,
    StoreAs,
    Modifier(usize),
    AddModifier,
    Flow,
    Recursion,
    Case(usize),
    AddCase,
    Object,
    ShareTags,
    Count,
    Pairs,
    Target,
    SetValue,
    Tag,
    ApplyModifier,
    Setting,
    SecondaryLabel,
    SecondaryDice,
    When,
    Active,
    Planned,
    Description,
}

/// What a closed modal's answer does.
type Apply = Box<dyn FnOnce(&mut FlowEditor, ModalResult, &mut Ctx)>;

/// Which condition the condition editor is changing.
#[derive(Clone, Debug)]
enum CondTarget {
    Step(StepId),
    Case(StepId, usize),
    Modifier(StepId, usize),
}

pub struct FlowEditor {
    draft: Flow,
    saved: Option<Flow>,
    origin: Option<Origin>,
    check: Option<FlowCheck>,
    context: Option<FlowContext>,
    view: View,
    state: RowsState,
    /// The step whose settings page is open.
    form: Option<StepId>,
    form_state: RowsState,
    /// A step being moved: Enter on a place puts it there.
    moving: Option<StepId>,
    search: String,
    modal: Option<(Modal, Apply)>,
    condition: Option<(CondTarget, ConditionEditor)>,
    undo: Vec<Flow>,
    redo: Vec<Flow>,
    stepped: bool,
    message: Option<(String, bool)>,
    restored: bool,
    /// A step to select once the rows are rebuilt.
    pending_select: Option<StepId>,
    /// Set to close the editor after the current key.
    leave: bool,
    /// What runs this flow (when it is saved).
    usages: Vec<settlement_core::table_editor::Usage>,
}

impl FlowEditor {
    /// Opens `saved` (or a new flow `draft`), resuming a `kept` draft.
    pub fn open(
        ctx: &mut Ctx,
        saved: Option<Flow>,
        kept: Option<Flow>,
        origin: Option<Origin>,
    ) -> Self {
        let restored = saved.is_some() && kept.is_some() && kept != saved;
        let draft = kept
            .filter(|_| saved.is_none() || restored)
            .or_else(|| saved.clone())
            .unwrap_or_else(AppService::new_flow);
        let mut e = FlowEditor {
            draft,
            saved,
            origin,
            check: None,
            context: None,
            view: View::Tree,
            state: RowsState::default(),
            form: None,
            form_state: RowsState::default(),
            moving: None,
            search: String::new(),
            modal: None,
            condition: None,
            undo: Vec::new(),
            redo: Vec::new(),
            stepped: false,
            message: None,
            restored,
            pending_select: None,
            leave: false,
            usages: Vec::new(),
        };
        e.recheck(ctx);
        e
    }

    fn dirty(&self) -> bool {
        self.saved.as_ref() != Some(&self.draft)
    }

    fn errors(&self) -> usize {
        self.check.as_ref().map_or(0, |c| {
            c.issues
                .iter()
                .filter(|i| i.severity == Severity::Error)
                .count()
                + usize::from(c.error.is_some())
        })
    }

    fn warnings(&self) -> usize {
        self.check.as_ref().map_or(0, |c| {
            c.issues
                .iter()
                .filter(|i| i.severity == Severity::Warning)
                .count()
        })
    }

    fn recheck(&mut self, ctx: &mut Ctx) {
        match ctx.service.check_flow(&self.draft) {
            Ok(c) => self.check = Some(c),
            Err(e) => self.message = Some((e, true)),
        }
        self.context = ctx.service.flow_context(&self.draft).ok();
        if self.saved.is_some() {
            self.usages = ctx.service.flow_usages(&self.draft.id).unwrap_or_default();
        }
    }

    fn after_change(&mut self, ctx: &mut Ctx) {
        self.recheck(ctx);
        if self.dirty() {
            let _ = ctx.service.save_flow_draft(&self.draft);
        }
        if let Some(id) = &self.form
            && self.step(id).is_none()
        {
            self.form = None;
        }
    }

    /// Applies a core edit; a refused edit is shown, changing nothing.
    fn edit(&mut self, edit: FlowEdit) -> bool {
        match AppService::edit_flow(&self.draft, edit) {
            Ok(next) => {
                self.draft = next;
                true
            }
            Err(e) => {
                self.message = Some((e, true));
                false
            }
        }
    }

    fn step(&self, id: &StepId) -> Option<FlowStep> {
        fn find(steps: &[FlowStep], id: &StepId) -> Option<FlowStep> {
            steps.iter().find_map(|s| {
                if &s.id == id {
                    Some(s.clone())
                } else {
                    s.children()
                        .into_iter()
                        .find_map(|c| find(std::slice::from_ref(c), id))
                }
            })
        }
        find(&self.draft.steps, id).or_else(|| find(&self.draft.editor.detached, id))
    }

    fn update_step(&mut self, id: &StepId, change: impl FnOnce(&mut FlowStep)) {
        if let Some(mut step) = self.step(id) {
            change(&mut step);
            self.edit(FlowEdit::Update { step });
        }
    }

    fn node_text(&self, id: &StepId) -> (String, String, Option<String>, Vec<String>) {
        self.check
            .as_ref()
            .and_then(|c| c.nodes.iter().find(|n| &n.step == id))
            .map_or_else(
                || (String::new(), id.to_string(), None, Vec::new()),
                |n| {
                    (
                        n.kind.clone(),
                        n.text.clone(),
                        n.when.clone(),
                        n.cases.clone(),
                    )
                },
            )
    }

    fn problem(&self, id: &StepId) -> Option<Severity> {
        let flow = self.draft.id.as_str();
        let issues = self.check.as_ref()?.issues.iter().filter(|i| {
            i.location.flow.as_deref() == Some(flow)
                && i.location.step.as_deref() == Some(id.as_str())
        });
        let mut worst = None;
        for issue in issues {
            if issue.severity == Severity::Error {
                return Some(Severity::Error);
            }
            worst = Some(Severity::Warning);
        }
        worst
    }

    // ----- the tree -------------------------------------------------------------

    fn tree_rows(&self, ctx: &Ctx) -> Vec<Row<Act>> {
        let s = &ctx.styles;
        let mut rows = vec![
            Row::item(
                Line::from(vec![
                    Span::styled("Name         ", s.dim()),
                    Span::styled(self.draft.name.clone(), s.title()),
                ]),
                Act::Name,
            ),
            Row::item(
                Line::from(vec![
                    Span::styled("Description  ", s.dim()),
                    Span::raw(if self.draft.description.is_empty() {
                        "(none)".to_owned()
                    } else {
                        self.draft.description.clone()
                    }),
                ]),
                Act::Description,
            ),
        ];
        if !self.usages.is_empty() {
            rows.push(Row::info(Line::styled("USED BY", s.dim())));
            for u in &self.usages {
                rows.push(match of_usage(u) {
                    Some(item) => Row::item(
                        Line::raw(format!("  {}  ›", u.description)),
                        Act::Open(item),
                    ),
                    None => Row::info(Line::styled(format!("  {}", u.description), s.dim())),
                });
            }
        }
        rows.push(Row::blank());
        self.list_rows(&self.draft.steps, Slot::Top, 0, ctx, &mut rows);
        if !self.draft.editor.detached.is_empty() {
            rows.push(Row::blank());
            rows.push(Row::info(Line::styled(
                "NOT CONNECTED (never runs; m then Enter on a place to connect)",
                s.dim(),
            )));
            for step in &self.draft.editor.detached {
                let (kind, text, ..) = self.node_text(&step.id);
                rows.push(Row::item(
                    Line::from(vec![
                        Span::styled(format!("{kind}: "), s.dim()),
                        Span::raw(text),
                    ]),
                    Act::Detached(step.id.clone()),
                ));
            }
        }
        rows
    }

    fn list_rows(
        &self,
        steps: &[FlowStep],
        slot: Slot,
        depth: usize,
        ctx: &Ctx,
        out: &mut Vec<Row<Act>>,
    ) {
        let s = &ctx.styles;
        let pad = "    ".repeat(depth);
        let port = |index: usize, label: &str| {
            Row::item(
                Line::styled(format!("{pad}{label}"), s.dim()),
                Act::Port(Position {
                    slot: slot.clone(),
                    index,
                }),
            )
        };
        let moving = self.moving.is_some();
        for (index, step) in steps.iter().enumerate() {
            if moving {
                out.push(port(index, "↳ put it here"));
            }
            let (kind, text, when, cases) = self.node_text(&step.id);
            let mut spans = vec![Span::raw(pad.clone())];
            match self.problem(&step.id) {
                Some(Severity::Error) => spans.push(Span::styled("! ", s.error())),
                Some(Severity::Warning) => spans.push(Span::styled("? ", s.key())),
                None => {}
            }
            spans.push(Span::styled(format!("{kind}: "), s.dim()));
            let off = !step.active || !step.status.is_active();
            spans.push(Span::styled(text, if off { s.dim() } else { s.title() }));
            if let Some(when) = when {
                spans.push(Span::styled(format!("  (only when {when})"), s.dim()));
            }
            if self.moving.as_ref() == Some(&step.id) {
                spans.push(Span::styled("  ← moving", s.key()));
            }
            if !self.search.is_empty() && self.matches(&step.id) {
                spans.push(Span::styled("  ◆", s.key()));
            }
            out.push(Row::item(Line::from(spans), Act::Step(step.id.clone())));
            let inner = "    ".repeat(depth + 1);
            let sub = |label: String, steps: &[FlowStep], slot: Slot, out: &mut Vec<Row<Act>>| {
                out.push(Row::info(Line::styled(format!("{inner}{label}"), s.key())));
                self.list_rows(steps, slot, depth + 2, ctx, out);
            };
            match &step.action {
                StepAction::Repeat { steps, .. } => sub(
                    "Each time:".into(),
                    steps,
                    Slot::Body {
                        step: step.id.clone(),
                    },
                    out,
                ),
                StepAction::ForEach { steps, pairs, .. } => sub(
                    if *pairs {
                        "For each pair:".into()
                    } else {
                        "For each:".into()
                    },
                    steps,
                    Slot::Body {
                        step: step.id.clone(),
                    },
                    out,
                ),
                StepAction::CreateObject { steps, .. } => sub(
                    "Inside it:".into(),
                    steps,
                    Slot::Body {
                        step: step.id.clone(),
                    },
                    out,
                ),
                StepAction::Branch {
                    cases: branch_cases,
                    otherwise,
                } => {
                    for (i, case) in branch_cases.iter().enumerate() {
                        let words = cases.get(i).cloned().unwrap_or_default();
                        sub(
                            format!("{} {words}:", if i == 0 { "IF" } else { "OTHERWISE IF" }),
                            &case.steps,
                            Slot::Case {
                                step: step.id.clone(),
                                case: i,
                            },
                            out,
                        );
                    }
                    sub(
                        "OTHERWISE:".into(),
                        otherwise,
                        Slot::Otherwise {
                            step: step.id.clone(),
                        },
                        out,
                    );
                }
                _ => {}
            }
        }
        out.push(port(
            steps.len(),
            if moving {
                "↳ put it here"
            } else if steps.is_empty() {
                "+ add a step"
            } else {
                "+ add a step here"
            },
        ));
    }

    fn matches(&self, id: &StepId) -> bool {
        let needle = self.search.to_lowercase();
        let (kind, text, when, _) = self.node_text(id);
        format!("{id} {kind} {text} {}", when.unwrap_or_default())
            .to_lowercase()
            .contains(&needle)
    }

    fn pseudo_rows(&self, ctx: &Ctx) -> Vec<Row<Act>> {
        let Some(check) = &self.check else {
            return Vec::new();
        };
        check
            .pseudocode
            .iter()
            .map(|line| {
                let text = Line::raw(format!("{}{}", "    ".repeat(line.depth), line.text));
                match &line.step {
                    Some(step) => Row::item(text, Act::Step(step.clone())),
                    None => Row::info(text.style(ctx.styles.dim())),
                }
            })
            .collect()
    }

    fn problem_rows(&self, ctx: &Ctx) -> Vec<Row<Act>> {
        let s = &ctx.styles;
        let Some(check) = &self.check else {
            return Vec::new();
        };
        let mut rows = Vec::new();
        if let Some(e) = &check.error {
            rows.push(Row::info(Line::styled(format!("Error    {e}"), s.error())));
        }
        for issue in &check.issues {
            let (label, style) = match issue.severity {
                Severity::Error => ("Error  ", s.error()),
                Severity::Warning => ("Warning", s.dim()),
            };
            let line = Line::from(vec![
                Span::styled(label, style),
                Span::raw(format!("  {}", issue.message)),
            ]);
            let here = issue.location.flow.as_deref() == Some(self.draft.id.as_str());
            match (&issue.location.step, here) {
                (Some(step), true) => rows.push(Row::item(line, Act::Step(StepId::new(step)))),
                _ => rows.push(Row::info(line)),
            }
        }
        if rows.is_empty() {
            rows.push(Row::info(Line::styled("No problems found.", s.good())));
        }
        rows
    }

    fn rows(&self, ctx: &Ctx) -> Vec<Row<Act>> {
        match self.view {
            View::Tree => self.tree_rows(ctx),
            View::Pseudocode => self.pseudo_rows(ctx),
            View::Problems => self.problem_rows(ctx),
        }
    }

    fn select_step(&mut self, ctx: &Ctx, id: &StepId) {
        self.view = View::Tree;
        let rows = self.tree_rows(ctx);
        self.state.select_where(
            &rows,
            |a| matches!(a, Act::Step(s) | Act::Detached(s) if s == id),
        );
    }

    // ----- pickers ----------------------------------------------------------------

    fn context(&self) -> FlowContext {
        self.context.clone().unwrap_or(FlowContext {
            subjects: Vec::new(),
            tables: Vec::new(),
            tags: Vec::new(),
            settings: Vec::new(),
            objects: Vec::new(),
            flows: Vec::new(),
            kinds: Vec::new(),
        })
    }

    fn ask(
        &mut self,
        modal: Modal,
        apply: impl FnOnce(&mut FlowEditor, ModalResult, &mut Ctx) + 'static,
    ) {
        self.modal = Some((modal, Box::new(apply)));
    }

    fn ask_kind(&mut self, at: Position) {
        let items = StepKind::ALL
            .iter()
            .map(|k| PickItem::new(k.label()))
            .collect();
        self.ask(
            Modal::pick("Add which step?", items, 0),
            move |e, r, ctx| {
                if let ModalResult::Picked(k) = r {
                    let before: Vec<StepId> =
                        e.draft.all_steps().iter().map(|s| s.id.clone()).collect();
                    if e.edit(FlowEdit::Add {
                        kind: StepKind::ALL[k],
                        at,
                    }) {
                        e.after_change(ctx);
                        if let Some(new) = e
                            .draft
                            .all_steps()
                            .iter()
                            .find(|s| !before.contains(&s.id))
                            .map(|s| s.id.clone())
                        {
                            e.select_step(ctx, &new);
                        }
                    }
                }
            },
        );
    }

    /// A picker of `options`; Enter gives the chosen ID to `set`.
    fn ask_named(
        &mut self,
        title: &str,
        options: Vec<settlement_core::engine::Named<String>>,
        current: &str,
        set: impl FnOnce(&mut FlowEditor, String) + 'static,
    ) {
        if options.is_empty() {
            self.message = Some(("There is nothing to choose from yet.".into(), true));
            return;
        }
        let selected = options.iter().position(|o| o.id == current).unwrap_or(0);
        let items = options
            .iter()
            .map(|o| PickItem::new(o.name.clone()).hint(o.id.clone()))
            .collect();
        self.ask(Modal::pick(title, items, selected), move |e, r, _| {
            if let ModalResult::Picked(k) = r {
                set(e, options[k].id.clone());
            }
        });
    }

    fn ask_text(
        &mut self,
        title: &str,
        help: &str,
        value: &str,
        set: impl FnOnce(&mut FlowEditor, String) + 'static,
    ) {
        self.ask(Modal::text(title, help, value), move |e, r, _| {
            if let ModalResult::Text(t) = r {
                set(e, t);
            }
        });
    }

    fn ask_modifier(&mut self, set: impl Fn(&mut FlowEditor, Modifier) + Clone + 'static) {
        const PRESETS: [&str; 9] = [
            "Shift +1",
            "Shift -1",
            "Shift +2",
            "Shift -2",
            "Shift by…",
            "Plus…",
            "Minus…",
            "Cap at Highest",
            "Cap at Lowest",
        ];
        let items = PRESETS.iter().map(|p| PickItem::new(*p)).collect();
        self.ask(Modal::pick("Modifier", items, 0), move |e, r, _| {
            let ModalResult::Picked(k) = r else { return };
            let fixed = match k {
                0 => Some(Modifier::Shift(1)),
                1 => Some(Modifier::Shift(-1)),
                2 => Some(Modifier::Shift(2)),
                3 => Some(Modifier::Shift(-2)),
                7 => Some(Modifier::CapAtHighest),
                8 => Some(Modifier::CapAtLowest),
                _ => None,
            };
            if let Some(m) = fixed {
                return set(e, m);
            }
            e.ask_text(
                "Amount",
                "A whole number, e.g. 3.",
                "1",
                move |e, t| match t.trim().parse::<i32>() {
                    Ok(n) => set(
                        e,
                        match k {
                            4 => Modifier::Shift(n),
                            5 => Modifier::Plus(n.abs()),
                            _ => Modifier::Minus(n.abs()),
                        },
                    ),
                    Err(_) => e.message = Some((format!("“{t}” is not a whole number."), true)),
                },
            );
        });
    }

    // ----- the step settings page -------------------------------------------------

    fn form_rows(&self, step: &FlowStep, ctx: &Ctx) -> Vec<Row<Field>> {
        let s = &ctx.styles;
        let context = self.context.as_ref();
        let name_of = |list: Option<&Vec<settlement_core::engine::Named<String>>>, id: &str| {
            if id.is_empty() {
                return "(choose)".to_owned();
            }
            list.and_then(|l| l.iter().find(|o| o.id == id))
                .map_or_else(|| format!("{id} (missing)"), |o| o.name.clone())
        };
        let row = |label: &str, value: String, field: Field| {
            Row::item(
                Line::from(vec![
                    Span::styled(format!("{label:<22}"), s.dim()),
                    Span::raw(value),
                ]),
                field,
            )
        };
        let yes_no = |b: bool| if b { "[x]" } else { "[ ]" }.to_owned();
        let (kind, text, ..) = self.node_text(&step.id);
        let mut rows = vec![
            Row::info(Line::styled(kind, s.dim())),
            Row::info(Line::styled(text, s.title())),
            Row::blank(),
            row(
                "Label",
                step.name.clone().unwrap_or_else(|| "(default)".into()),
                Field::Label,
            ),
        ];
        match &step.action {
            StepAction::Roll {
                table,
                field,
                modifiers,
            } => {
                rows.push(row(
                    "Table",
                    name_of(context.map(|c| &c.tables), table.as_str()),
                    Field::Table,
                ));
                rows.push(row(
                    "Store on object as",
                    field.clone().unwrap_or_else(|| "(not stored)".into()),
                    Field::StoreAs,
                ));
                for (i, m) in modifiers.iter().enumerate() {
                    let when = m.when.as_ref().map_or(String::new(), |w| {
                        format!("  when {}", describe(ctx, &self.draft, w))
                    });
                    rows.push(row(
                        &format!("Modifier {}", i + 1),
                        format!("{}{when}", m.modifier),
                        Field::Modifier(i),
                    ));
                    rows.extend(modifier_lines(ctx, table, m.modifier));
                }
                rows.push(row("", "+ add modifier".into(), Field::AddModifier));
            }
            StepAction::RunFlow { flow, max_depth } => {
                rows.push(row(
                    "Flow to run",
                    name_of(context.map(|c| &c.flows), flow.as_str()),
                    Field::Flow,
                ));
                rows.push(row(
                    "Recursion limit",
                    max_depth.map_or_else(
                        || "none (a loop is an error)".into(),
                        |d| format!("at most {d} level(s)"),
                    ),
                    Field::Recursion,
                ));
            }
            StepAction::Branch { cases, .. } => {
                for (i, case) in cases.iter().enumerate() {
                    rows.push(row(
                        if i == 0 { "If" } else { "Otherwise if" },
                        describe(ctx, &self.draft, &case.when),
                        Field::Case(i),
                    ));
                }
                rows.push(row("", "+ add case".into(), Field::AddCase));
            }
            StepAction::CreateObject {
                object, share_tags, ..
            } => {
                rows.push(row(
                    "Object type",
                    name_of(context.map(|c| &c.objects), object.as_str()),
                    Field::Object,
                ));
                rows.push(row("Share its tags", yes_no(*share_tags), Field::ShareTags));
            }
            StepAction::Repeat { count, .. } => rows.push(row(
                "How many times",
                match count {
                    Count::Times(n) => format!("{n}"),
                    Count::Dice(d) => format!("roll {d}"),
                    Count::ValueOf { step, add } => {
                        let label = context
                            .and_then(|c| c.subjects.iter().find(|s| &s.step == step))
                            .map_or_else(|| step.to_string(), |s| s.label.clone());
                        match add {
                            0 => format!("the number from {label}"),
                            a => format!("the number from {label} {a:+}"),
                        }
                    }
                },
                Field::Count,
            )),
            StepAction::ForEach { object, pairs, .. } => {
                rows.push(row(
                    "Object type",
                    name_of(context.map(|c| &c.objects), object.as_str()),
                    Field::Object,
                ));
                rows.push(row("Every unique pair", yes_no(*pairs), Field::Pairs));
            }
            StepAction::SetValue { field, value } => {
                rows.push(row("Field", field.clone(), Field::Target));
                rows.push(row("Value", value.to_string(), Field::SetValue));
            }
            StepAction::AddTag { tag } | StepAction::RemoveTag { tag } => rows.push(row(
                "Tag",
                name_of(context.map(|c| &c.tags), tag.as_str()),
                Field::Tag,
            )),
            StepAction::ApplyModifier { table, modifier } => {
                rows.push(row(
                    "Every later roll of",
                    name_of(context.map(|c| &c.tables), table.as_str()),
                    Field::Table,
                ));
                rows.push(row("Modifier", modifier.to_string(), Field::ApplyModifier));
                rows.extend(modifier_lines(ctx, table, *modifier));
            }
            StepAction::ReadSetting { setting } => {
                let named: Option<Vec<_>> = context.map(|c| {
                    c.settings
                        .iter()
                        .map(|s| settlement_core::engine::Named {
                            id: s.id.clone(),
                            name: s.name.clone(),
                        })
                        .collect()
                });
                rows.push(row(
                    "Setting",
                    name_of(named.as_ref(), setting.as_str()),
                    Field::Setting,
                ));
            }
            StepAction::SecondaryRoll { label, dice } => {
                rows.push(row("Roll label", label.clone(), Field::SecondaryLabel));
                rows.push(row("Dice", dice.clone(), Field::SecondaryDice));
            }
            StepAction::Stop => rows.push(Row::info(Line::styled(
                "Ends the flow it is in. Steps after it never run.",
                s.dim(),
            ))),
            StepAction::Name { .. } | StepAction::Builtin { .. } | StepAction::SetResult { .. } => {
                rows.push(Row::info(Line::styled(
                    "Provided by the application; its own settings are edited on their own screens.",
                    s.dim(),
                )));
            }
        }
        rows.extend([
            Row::blank(),
            row(
                "Only when",
                step.when
                    .as_ref()
                    .map_or_else(|| "always".into(), |w| describe(ctx, &self.draft, w)),
                Field::When,
            ),
            row("Active", yes_no(step.active), Field::Active),
            row(
                "Planned (never runs)",
                yes_no(!step.status.is_active()),
                Field::Planned,
            ),
            row(
                "Description",
                if step.description.is_empty() {
                    "(none)".into()
                } else {
                    step.description.clone()
                },
                Field::Description,
            ),
            Row::blank(),
            Row::info(Line::styled(format!("Step ID {}", step.id), s.dim())),
        ]);
        rows
    }

    fn edit_field(&mut self, id: StepId, field: Field, ctx: &mut Ctx) {
        let Some(step) = self.step(&id) else { return };
        let context = self.context();
        match field {
            Field::Label => self.ask_text(
                "Label",
                "Shown instead of the table's name; empty for the default.",
                step.name.as_deref().unwrap_or(""),
                move |e, t| {
                    let t = t.trim().to_owned();
                    e.update_step(&id, |s| s.name = (!t.is_empty()).then_some(t));
                },
            ),
            Field::Table => {
                let current = match &step.action {
                    StepAction::Roll { table, .. } | StepAction::ApplyModifier { table, .. } => {
                        table.to_string()
                    }
                    _ => String::new(),
                };
                self.ask_named("Table", context.tables, &current, move |e, t| {
                    e.update_step(&id, |s| match &mut s.action {
                        StepAction::Roll { table, .. }
                        | StepAction::ApplyModifier { table, .. } => *table = TableId::new(&t),
                        _ => {}
                    })
                });
            }
            Field::StoreAs => self.ask_text(
                "Store on the object as",
                "A field name; empty to not store it.",
                match &step.action {
                    StepAction::Roll { field, .. } => field.as_deref().unwrap_or(""),
                    _ => "",
                },
                move |e, t| {
                    let t = t.trim().to_owned();
                    e.update_step(&id, |s| {
                        if let StepAction::Roll { field, .. } = &mut s.action {
                            *field = (!t.is_empty()).then_some(t);
                        }
                    })
                },
            ),
            Field::Modifier(i) => {
                self.ask_modifier(move |e, m| {
                    e.update_step(&id, |s| {
                        if let StepAction::Roll { modifiers, .. } = &mut s.action
                            && let Some(x) = modifiers.get_mut(i)
                        {
                            x.modifier = m;
                        }
                    })
                });
            }
            Field::AddModifier => self.ask_modifier(move |e, m| {
                e.update_step(&id, |s| {
                    if let StepAction::Roll { modifiers, .. } = &mut s.action {
                        modifiers.push(StepModifier {
                            modifier: m,
                            when: None,
                        });
                    }
                })
            }),
            Field::Flow => {
                let current = match &step.action {
                    StepAction::RunFlow { flow, .. } => flow.to_string(),
                    _ => String::new(),
                };
                self.ask_named("Flow to run", context.flows, &current, move |e, f| {
                    e.update_step(&id, |s| {
                        if let StepAction::RunFlow { flow, .. } = &mut s.action {
                            *flow = FlowId::new(&f);
                        }
                    })
                });
            }
            Field::Recursion => self.ask_text(
                "Recursion limit",
                "Levels it may run itself (1–16); empty for none, which makes a loop an error.",
                &match &step.action {
                    StepAction::RunFlow { max_depth, .. } => {
                        max_depth.map(|d| d.to_string()).unwrap_or_default()
                    }
                    _ => String::new(),
                },
                move |e, t| {
                    let t = t.trim().to_owned();
                    let depth = if t.is_empty() {
                        None
                    } else {
                        match t.parse::<u32>() {
                            Ok(d) => Some(d),
                            Err(_) => {
                                e.message = Some((format!("“{t}” is not a whole number."), true));
                                return;
                            }
                        }
                    };
                    e.update_step(&id, |s| {
                        if let StepAction::RunFlow { max_depth, .. } = &mut s.action {
                            *max_depth = depth;
                        }
                    })
                },
            ),
            Field::Case(i) => {
                if let StepAction::Branch { cases, .. } = &step.action {
                    let mut editor = ConditionEditor::for_flow(
                        &format!("Case {} condition", i + 1),
                        cases.get(i).map(|c| c.when.clone()),
                        &context,
                    );
                    editor.sentence = editor.value.as_ref().map(|c| describe(ctx, &self.draft, c));
                    self.condition = Some((CondTarget::Case(id, i), editor));
                }
            }
            Field::AddCase => {
                self.edit(FlowEdit::AddCase { step: id });
            }
            Field::Object => {
                let current = match &step.action {
                    StepAction::CreateObject { object, .. }
                    | StepAction::ForEach { object, .. } => object.to_string(),
                    _ => String::new(),
                };
                self.ask_named("Object type", context.objects, &current, move |e, o| {
                    e.update_step(&id, |s| match &mut s.action {
                        StepAction::CreateObject { object, .. }
                        | StepAction::ForEach { object, .. } => *object = ObjectTypeId::new(&o),
                        _ => {}
                    })
                });
            }
            Field::ShareTags => self.update_step(&id, |s| {
                if let StepAction::CreateObject { share_tags, .. } = &mut s.action {
                    *share_tags = !*share_tags;
                }
            }),
            Field::Pairs => self.update_step(&id, |s| {
                if let StepAction::ForEach { pairs, .. } = &mut s.action {
                    *pairs = !*pairs;
                }
            }),
            Field::Count => {
                let items = vec![
                    PickItem::new("A fixed number"),
                    PickItem::new("A number from an earlier step"),
                    PickItem::new("Roll dice"),
                ];
                self.ask(Modal::pick("How many times", items, 0), move |e, r, _| {
                    let ModalResult::Picked(k) = r else { return };
                    let set_count = |e: &mut FlowEditor, id: &StepId, c: Count| {
                        e.update_step(id, |s| {
                            if let StepAction::Repeat { count, .. } = &mut s.action {
                                *count = c;
                            }
                        })
                    };
                    match k {
                        0 => e.ask_text("Number of times", "0 to 100.", "1", move |e, t| {
                            match t.trim().parse::<u32>() {
                                Ok(n) => set_count(e, &id, Count::Times(n)),
                                Err(_) => {
                                    e.message =
                                        Some((format!("“{t}” is not a whole number."), true))
                                }
                            }
                        }),
                        2 => e.ask_text("Dice", "e.g. 1d4 or 2d6.", "1d4", move |e, t| {
                            set_count(e, &id, Count::Dice(t.trim().to_owned()))
                        }),
                        _ => {
                            let numeric: Vec<_> = e
                                .context()
                                .subjects
                                .into_iter()
                                .filter(|s| s.numeric)
                                .map(|s| settlement_core::engine::Named {
                                    id: s.step.to_string(),
                                    name: s.label,
                                })
                                .collect();
                            e.ask_named("Which step's number?", numeric, "", move |e, step| {
                                e.ask_text(
                                    "Add to the number",
                                    "e.g. 0, 1 or -1.",
                                    "0",
                                    move |e, t| match t.trim().parse::<i64>() {
                                        Ok(add) => set_count(
                                            e,
                                            &id,
                                            Count::ValueOf {
                                                step: StepId::new(&step),
                                                add,
                                            },
                                        ),
                                        Err(_) => {
                                            e.message = Some((
                                                format!("“{t}” is not a whole number."),
                                                true,
                                            ))
                                        }
                                    },
                                )
                            });
                        }
                    }
                });
            }
            Field::Target => self.ask_text(
                "Field",
                "The object field to set.",
                match &step.action {
                    StepAction::SetValue { field, .. } => field,
                    _ => "",
                },
                move |e, t| {
                    e.update_step(&id, |s| {
                        if let StepAction::SetValue { field, .. } = &mut s.action {
                            *field = t.trim().to_owned();
                        }
                    })
                },
            ),
            Field::SetValue => self.ask_text(
                "Value",
                "A whole number.",
                &match &step.action {
                    StepAction::SetValue { value, .. } => value.to_string(),
                    _ => String::new(),
                },
                move |e, t| match t.trim().parse::<i64>() {
                    Ok(n) => e.update_step(&id, |s| {
                        if let StepAction::SetValue { value, .. } = &mut s.action {
                            *value = n;
                        }
                    }),
                    Err(_) => e.message = Some((format!("“{t}” is not a whole number."), true)),
                },
            ),
            Field::Tag => {
                let current = match &step.action {
                    StepAction::AddTag { tag } | StepAction::RemoveTag { tag } => tag.to_string(),
                    _ => String::new(),
                };
                self.ask_named("Tag", context.tags, &current, move |e, t| {
                    e.update_step(&id, |s| match &mut s.action {
                        StepAction::AddTag { tag } | StepAction::RemoveTag { tag } => {
                            *tag = TagId::new(&t)
                        }
                        _ => {}
                    })
                });
            }
            Field::ApplyModifier => self.ask_modifier(move |e, m| {
                e.update_step(&id, |s| {
                    if let StepAction::ApplyModifier { modifier, .. } = &mut s.action {
                        *modifier = m;
                    }
                })
            }),
            Field::Setting => {
                let current = match &step.action {
                    StepAction::ReadSetting { setting } => setting.to_string(),
                    _ => String::new(),
                };
                let options = context
                    .settings
                    .iter()
                    .map(|s| settlement_core::engine::Named {
                        id: s.id.clone(),
                        name: s.name.clone(),
                    })
                    .collect();
                self.ask_named("Setting", options, &current, move |e, v| {
                    e.update_step(&id, |s| {
                        if let StepAction::ReadSetting { setting } = &mut s.action {
                            *setting = SettingId::new(&v);
                        }
                    })
                });
            }
            Field::SecondaryLabel | Field::SecondaryDice => {
                let (label, dice) = match &step.action {
                    StepAction::SecondaryRoll { label, dice } => (label.clone(), dice.clone()),
                    _ => return,
                };
                let is_label = field == Field::SecondaryLabel;
                self.ask_text(
                    if is_label { "Roll label" } else { "Dice" },
                    "",
                    if is_label { &label } else { &dice },
                    move |e, t| {
                        e.update_step(&id, |s| {
                            if let StepAction::SecondaryRoll { label, dice } = &mut s.action {
                                if is_label {
                                    *label = t;
                                } else {
                                    *dice = t;
                                }
                            }
                        })
                    },
                );
            }
            Field::When => self.open_when(CondTarget::Step(id), step.when.clone(), ctx),
            Field::Active => self.update_step(&id, |s| s.active = !s.active),
            Field::Planned => self.update_step(&id, |s| {
                s.status = if s.status.is_active() {
                    settlement_core::content::EffectStatus::Planned
                } else {
                    settlement_core::content::EffectStatus::Active
                }
            }),
            Field::Description => {
                self.ask_text("Description", "", &step.description, move |e, t| {
                    e.update_step(&id, |s| s.description = t)
                })
            }
        }
    }

    fn open_when(&mut self, target: CondTarget, value: Option<Condition>, ctx: &Ctx) {
        let mut editor = ConditionEditor::for_flow("Only run when", value, &self.context());
        editor.sentence = editor.value.as_ref().map(|c| describe(ctx, &self.draft, c));
        self.condition = Some((target, editor));
    }

    fn finish_condition(&mut self, target: CondTarget, value: Option<Condition>) {
        match target {
            CondTarget::Step(id) => self.update_step(&id, |s| s.when = value),
            CondTarget::Case(id, i) => self.update_step(&id, |s| {
                if let StepAction::Branch { cases, .. } = &mut s.action
                    && let Some(case) = cases.get_mut(i)
                {
                    case.when = value.unwrap_or(Condition::All(Vec::new()));
                }
            }),
            CondTarget::Modifier(id, i) => self.update_step(&id, |s| {
                if let StepAction::Roll { modifiers, .. } = &mut s.action
                    && let Some(m) = modifiers.get_mut(i)
                {
                    m.when = value;
                }
            }),
        }
    }

    // ----- keys ---------------------------------------------------------------------

    fn step_history(&mut self, back: bool, ctx: &mut Ctx) {
        let (from, to) = if back {
            (&mut self.undo, &mut self.redo)
        } else {
            (&mut self.redo, &mut self.undo)
        };
        self.stepped = true;
        if let Some(previous) = from.pop() {
            to.push(std::mem::replace(&mut self.draft, previous));
            self.after_change(ctx);
        }
    }

    fn save(&mut self, ctx: &mut Ctx) {
        match ctx.service.save_flow(self.draft.clone()) {
            Ok(saved) => {
                self.draft = saved.clone();
                self.saved = Some(saved);
                self.restored = false;
                if self.origin.is_none() {
                    self.origin = Some(Origin::Custom);
                }
                self.message = Some(("Flow saved.".into(), false));
                self.recheck(ctx);
            }
            Err(e) => self.message = Some((e, true)),
        }
    }

    /// Moves `id` to `to` (or connects it there, if it is detached).
    fn put(&mut self, id: StepId, to: Position) -> bool {
        if self.draft.editor.detached.iter().any(|s| s.id == id) {
            return self.edit(FlowEdit::Attach { step: id, to });
        }
        let mut to = to;
        if let Some(from) = position_of(&self.draft, &id)
            && from.slot == to.slot
            && to.index > from.index
        {
            to.index -= 1;
        }
        self.edit(FlowEdit::Move { step: id, to })
    }

    fn handle_form(&mut self, key: KeyEvent, ctx: &mut Ctx) -> Nav {
        let Some(id) = self.form.clone() else {
            return Nav::Stay;
        };
        let Some(step) = self.step(&id) else {
            self.form = None;
            return Nav::Stay;
        };
        let rows = self.form_rows(&step, ctx);
        if self.form_state.handle_nav(&rows, key) {
            return Nav::Stay;
        }
        let current = self.form_state.current(&rows).cloned();
        match (key.code, current) {
            (KeyCode::Esc | KeyCode::Backspace, _) => {
                self.form = None;
            }
            (KeyCode::Enter | KeyCode::Char(' '), Some(field)) => self.edit_field(id, field, ctx),
            (KeyCode::Char('x') | KeyCode::Delete, Some(Field::Modifier(i))) => {
                self.update_step(&id, |s| {
                    if let StepAction::Roll { modifiers, .. } = &mut s.action {
                        modifiers.remove(i);
                    }
                });
            }
            (KeyCode::Char('w'), Some(Field::Modifier(i))) => {
                let when = match &step.action {
                    StepAction::Roll { modifiers, .. } => {
                        modifiers.get(i).and_then(|m| m.when.clone())
                    }
                    _ => None,
                };
                self.open_when(CondTarget::Modifier(id, i), when, ctx);
            }
            (KeyCode::Char('x') | KeyCode::Delete, Some(Field::Case(i))) => {
                self.edit(FlowEdit::RemoveCase { step: id, case: i });
            }
            _ => {}
        }
        Nav::Stay
    }

    fn handle_key(&mut self, key: KeyEvent, ctx: &mut Ctx) -> Nav {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        if self.condition.is_none() && self.modal.is_none() {
            let history = match key.code {
                KeyCode::Char('z') if ctrl => Some(true),
                KeyCode::Char('y') if ctrl => Some(false),
                KeyCode::Char('u') => Some(true),
                KeyCode::Char('U') => Some(false),
                _ => None,
            };
            if let Some(back) = history {
                self.step_history(back, ctx);
                return Nav::Stay;
            }
        }
        if let Some((target, editor)) = &mut self.condition {
            if let Outcome::Done = editor.handle(key, ctx) {
                let (target, value) = (target.clone(), editor.value.clone());
                self.condition = None;
                self.finish_condition(target, value);
            } else {
                let sentence = editor.value.clone().map(|c| describe(ctx, &self.draft, &c));
                if let Some((_, editor)) = &mut self.condition {
                    editor.sentence = sentence;
                }
            }
            return Nav::Stay;
        }
        if let Some((modal, _)) = &mut self.modal {
            let result = modal.handle(key, &ctx.styles);
            if let ModalResult::Open = result {
                return Nav::Stay;
            }
            let (_, apply) = self.modal.take().expect("an open modal");
            apply(self, result, ctx);
            return Nav::Stay;
        }
        if self.form.is_some() {
            return self.handle_form(key, ctx);
        }
        let rows = self.rows(ctx);
        if self.state.handle_nav(&rows, key) {
            return Nav::Stay;
        }
        let current = self.state.current(&rows).cloned();
        match (key.code, current) {
            (KeyCode::Esc | KeyCode::Backspace, _) if self.moving.is_some() => {
                self.moving = None;
            }
            (KeyCode::Esc | KeyCode::Backspace, _) => {
                if self.view != View::Tree {
                    self.view = View::Tree;
                } else if self.dirty() {
                    self.ask(
                        Modal::confirm(
                            "Leave with unsaved changes? They are kept as a draft for next time.",
                            "Leave",
                        ),
                        |e, r, _| {
                            if let ModalResult::Yes = r {
                                e.leave = true;
                            }
                        },
                    );
                } else {
                    return Nav::Pop;
                }
            }
            (KeyCode::Tab, _) => {
                self.view = match self.view {
                    View::Tree => View::Pseudocode,
                    View::Pseudocode => View::Problems,
                    View::Problems => View::Tree,
                };
                self.state = RowsState::default();
            }
            (KeyCode::Char('s'), _) => {
                if self.errors() > 0 {
                    self.message = Some(("Fix the errors before saving.".into(), true));
                } else {
                    self.save(ctx);
                }
            }
            (KeyCode::Char('/'), _) => {
                let current = self.search.clone();
                self.ask_text(
                    "Find a step",
                    "Words from its name, kind or text.",
                    &current,
                    |e, t| {
                        e.search = t;
                        e.next_match_from(None);
                    },
                );
            }
            (KeyCode::Char('n'), current) if !self.search.is_empty() => {
                let from = match current {
                    Some(Act::Step(id)) => Some(id),
                    _ => None,
                };
                self.next_match_from(from);
            }
            (KeyCode::Char('t'), _) => {
                if self.errors() > 0 {
                    self.message = Some(("Fix the errors before testing.".into(), true));
                } else {
                    return Nav::Push(Box::new(TestScreen::new(Target::Flow(self.draft.clone()))));
                }
            }
            (KeyCode::Char('D'), _) => {
                let copy = Flow {
                    id: AppService::new_flow().id,
                    name: format!("Copy of {}", self.draft.name),
                    ..self.draft.clone()
                };
                return Nav::Push(Box::new(FlowEditor::open(ctx, None, Some(copy), None)));
            }
            (KeyCode::Char('X'), _) if self.saved.is_some() => {
                let name = self.draft.name.clone();
                self.ask(
                    Modal::confirm(
                        &format!(
                            "Delete the flow “{name}”? Anything that runs it must be changed first."
                        ),
                        "Delete",
                    ),
                    |e, r, ctx| {
                        if let ModalResult::Yes = r {
                            match ctx.service.delete_flow(&e.draft.id) {
                                Ok(_) => {
                                    let _ = ctx.service.discard_flow_draft(&e.draft.id);
                                    ctx.flash = Some("Flow deleted".into());
                                    e.leave = true;
                                }
                                Err(err) => e.message = Some((err, true)),
                            }
                        }
                    },
                );
            }
            (KeyCode::Char('R'), _) if self.origin == Some(Origin::Modified) => {
                self.ask(
                    Modal::confirm("Restore this flow to its installed default?", "Restore"),
                    |e, r, ctx| {
                        if let ModalResult::Yes = r {
                            match ctx
                                .service
                                .reset_framework_item(ItemKind::Flow, e.draft.id.as_str())
                                .and_then(|_| ctx.service.flow(&e.draft.id))
                            {
                                Ok(flow) => {
                                    let _ = ctx.service.discard_flow_draft(&flow.id);
                                    e.draft = flow.clone();
                                    e.saved = Some(flow);
                                    e.origin = Some(Origin::Bundled);
                                    e.undo.clear();
                                    e.redo.clear();
                                    e.recheck(ctx);
                                }
                                Err(err) => e.message = Some((err, true)),
                            }
                        }
                    },
                );
            }
            (KeyCode::Enter, Some(Act::Open(item))) => return open(ctx, &item),
            (KeyCode::Enter, Some(Act::Name)) => {
                let name = self.draft.name.clone();
                self.ask_text("Flow name", "", &name, |e, t| e.draft.name = t);
            }
            (KeyCode::Enter, Some(Act::Description)) => {
                let text = self.draft.description.clone();
                self.ask_text("Description", "", &text, |e, t| e.draft.description = t);
            }
            (KeyCode::Enter, Some(Act::Port(at))) => match self.moving.take() {
                Some(id) => {
                    if self.put(id.clone(), at) {
                        self.after_change(ctx);
                        self.select_step(ctx, &id);
                    }
                }
                None => self.ask_kind(at),
            },
            (KeyCode::Enter, Some(Act::Step(id) | Act::Detached(id))) => {
                if self.view != View::Tree {
                    self.select_step(ctx, &id);
                } else {
                    self.form = Some(id);
                    self.form_state = RowsState::default();
                }
            }
            (KeyCode::Char('a' | 'i'), Some(Act::Step(id))) => {
                if let Some(mut at) = position_of(&self.draft, &id) {
                    if key.code == KeyCode::Char('a') {
                        at.index += 1;
                    }
                    self.ask_kind(at);
                }
            }
            (KeyCode::Char('x') | KeyCode::Delete, Some(Act::Step(id) | Act::Detached(id))) => {
                let inside = self.step(&id).map_or(0, |s| count_inside(&s));
                if inside == 0 {
                    self.edit(FlowEdit::Remove { step: id });
                } else {
                    self.ask(
                        Modal::confirm(
                            &format!("Delete this step and the {inside} step(s) inside it?"),
                            "Delete",
                        ),
                        move |e, r, _| {
                            if let ModalResult::Yes = r {
                                e.edit(FlowEdit::Remove { step: id });
                            }
                        },
                    );
                }
            }
            (KeyCode::Char('c'), Some(Act::Step(id))) => {
                let before: Vec<StepId> = self
                    .draft
                    .all_steps()
                    .iter()
                    .map(|s| s.id.clone())
                    .collect();
                if self.edit(FlowEdit::Duplicate { step: id }) {
                    self.pending_select = self
                        .draft
                        .all_steps()
                        .iter()
                        .find(|s| !before.contains(&s.id))
                        .map(|s| s.id.clone());
                }
            }
            (KeyCode::Char('d'), Some(Act::Step(id))) => {
                if self.edit(FlowEdit::Detach { step: id.clone() }) {
                    self.pending_select = Some(id);
                }
            }
            (KeyCode::Char('m'), Some(Act::Step(id) | Act::Detached(id))) => {
                self.moving = Some(id);
                self.message = Some((
                    "Choose where it goes and press Enter; Esc cancels.".into(),
                    false,
                ));
            }
            (KeyCode::Char('K'), Some(Act::Step(id))) => self.shift(&id, -1),
            (KeyCode::Char('J'), Some(Act::Step(id))) => self.shift(&id, 1),
            (KeyCode::Char('w'), Some(Act::Step(id))) => {
                let when = self.step(&id).and_then(|s| s.when);
                self.open_when(CondTarget::Step(id), when, ctx);
            }
            _ => {}
        }
        Nav::Stay
    }

    fn shift(&mut self, id: &StepId, delta: isize) {
        let Some(from) = position_of(&self.draft, id) else {
            return;
        };
        let Some(index) = from.index.checked_add_signed(delta) else {
            return;
        };
        let length = self.list_len(&from.slot);
        if index >= length {
            return;
        }
        // The selection follows the step.
        if self.edit(FlowEdit::Move {
            step: id.clone(),
            to: Position {
                slot: from.slot,
                index,
            },
        }) {
            self.pending_select = Some(id.clone());
        }
    }

    fn list_len(&self, slot: &Slot) -> usize {
        match slot {
            Slot::Top => self.draft.steps.len(),
            Slot::Body { step } | Slot::Case { step, .. } | Slot::Otherwise { step } => {
                match self.step(step).map(|s| s.action) {
                    Some(
                        StepAction::Repeat { steps, .. }
                        | StepAction::ForEach { steps, .. }
                        | StepAction::CreateObject { steps, .. },
                    ) => steps.len(),
                    Some(StepAction::Branch { cases, otherwise }) => match slot {
                        Slot::Case { case, .. } => cases.get(*case).map_or(0, |c| c.steps.len()),
                        _ => otherwise.len(),
                    },
                    _ => 0,
                }
            }
        }
    }

    fn next_match_from(&mut self, from: Option<StepId>) {
        let order: Vec<StepId> = self
            .draft
            .all_steps()
            .iter()
            .map(|s| s.id.clone())
            .chain(self.draft.editor.detached.iter().map(|s| s.id.clone()))
            .collect();
        let start = from
            .and_then(|f| order.iter().position(|id| id == &f))
            .map_or(0, |i| i + 1);
        let found = order
            .iter()
            .cycle()
            .skip(start)
            .take(order.len())
            .find(|id| self.matches(id))
            .cloned();
        self.view = View::Tree;
        match found {
            Some(id) => self.pending_select = Some(id),
            None => self.message = Some(("No step matches.".into(), true)),
        }
    }
}

/// A modifier in plain words, indented under its row.
fn modifier_lines(ctx: &Ctx, table: &TableId, modifier: Modifier) -> Vec<Row<Field>> {
    if table.as_str().is_empty() {
        return Vec::new();
    }
    ctx.service
        .explain_modifier(table, modifier)
        .map(|e| {
            super::tags::explanation_lines(&e, ctx)
                .into_iter()
                .map(|line| {
                    let mut spans = vec![Span::raw("      ")];
                    spans.extend(line.spans);
                    Row::info(Line::from(spans).style(line.style))
                })
                .collect()
        })
        .unwrap_or_default()
}

/// How many steps are inside `step`.
fn count_inside(step: &FlowStep) -> usize {
    step.children().iter().map(|c| 1 + count_inside(c)).sum()
}

/// A condition in plain words, from the core.
fn describe(ctx: &Ctx, flow: &Flow, condition: &Condition) -> String {
    ctx.service
        .describe_condition(flow, condition)
        .unwrap_or_else(|_| "?".into())
}

impl Screen for FlowEditor {
    fn title(&self) -> String {
        format!(
            "Flows › {}",
            if self.draft.name.is_empty() {
                "Unnamed"
            } else {
                &self.draft.name
            }
        )
    }

    fn render(&mut self, f: &mut Frame, area: Rect, ctx: &Ctx) {
        let s = &ctx.styles;
        if let Some(id) = self.pending_select.take() {
            self.select_step(ctx, &id);
        }
        if let Some((_, editor)) = &mut self.condition {
            editor.render(f, area, ctx);
            return;
        }
        let [head, body] =
            Layout::vertical([Constraint::Length(4), Constraint::Fill(1)]).areas(area);
        let status = match (&self.saved, self.dirty()) {
            (None, _) => "new custom flow (not saved)".to_owned(),
            (Some(_), true) => format!(
                "{} · unsaved changes, kept as a draft",
                origin_label(self.origin, false)
            ),
            (Some(_), false) => origin_label(self.origin, false).to_owned(),
        };
        let tab = |v: View, label: &str| {
            Span::styled(
                format!(" {label} "),
                if self.view == v {
                    s.selected()
                } else {
                    s.dim()
                },
            )
        };
        let (errors, warnings) = (self.errors(), self.warnings());
        let mut lines = vec![
            Line::from(vec![
                Span::styled(format!("ID {}", self.draft.id), s.dim()),
                Span::styled(format!(" · {status}"), s.dim()),
                Span::styled(
                    if self.restored {
                        " · unsaved changes from earlier were restored"
                    } else {
                        ""
                    },
                    s.key(),
                ),
            ]),
            Line::from(vec![
                tab(View::Tree, "Steps"),
                tab(View::Pseudocode, "Pseudocode"),
                tab(View::Problems, &format!("Problems ({})", errors + warnings)),
                Span::styled("   Tab switches", s.dim()),
            ]),
        ];
        lines.push(if errors + warnings == 0 {
            Line::styled("No problems found.", s.good())
        } else {
            Line::styled(
                format!(
                    "{errors} error(s), {warnings} warning(s).{}",
                    if errors > 0 {
                        " Fix the errors to save."
                    } else {
                        ""
                    }
                ),
                if errors > 0 { s.error() } else { s.dim() },
            )
        });
        if let Some((m, error)) = &self.message {
            lines.push(Line::styled(
                m.clone(),
                if *error { s.error() } else { s.good() },
            ));
        }
        f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: true }), head);
        if let Some(id) = self.form.clone()
            && let Some(step) = self.step(&id)
        {
            let rows = self.form_rows(&step, ctx);
            self.form_state.render(f, body, &rows, s, true);
        } else {
            let rows = self.rows(ctx);
            self.state.render(f, body, &rows, s, true);
        }
        if let Some((m, _)) = &mut self.modal {
            m.render(f, area, s);
        }
    }

    fn handle(&mut self, key: KeyEvent, ctx: &mut Ctx) -> Nav {
        let before = self.draft.clone();
        self.stepped = false;
        if !matches!(key.code, KeyCode::Char('u' | 'U')) {
            self.message = None;
        }
        let nav = self.handle_key(key, ctx);
        // Undo and redo move along the history themselves.
        if self.draft != before && !self.stepped {
            self.undo.push(before);
            self.redo.clear();
            self.after_change(ctx);
        }
        if let Some(id) = self.pending_select.take() {
            self.select_step(ctx, &id);
        }
        if self.leave {
            return Nav::Pop;
        }
        nav
    }

    fn hints(&self) -> Vec<(&'static str, &'static str)> {
        if self.condition.is_some() {
            vec![
                ("Enter", "edit"),
                ("t", "type"),
                ("a", "add"),
                ("d", "remove"),
                ("Esc", "done"),
            ]
        } else if self.form.is_some() {
            vec![
                ("↑↓", "move"),
                ("Enter", "change"),
                ("x", "remove modifier/case"),
                ("w", "modifier condition"),
                ("u/U", "undo/redo"),
                ("Esc", "back to steps"),
            ]
        } else if self.moving.is_some() {
            vec![
                ("↑↓", "choose a place"),
                ("Enter", "put it here"),
                ("Esc", "cancel"),
            ]
        } else {
            vec![
                ("Enter", "open/add"),
                ("a/i", "add after/before"),
                ("x", "delete"),
                ("c", "duplicate"),
                ("m", "move"),
                ("J/K", "down/up"),
                ("d", "disconnect"),
                ("w", "condition"),
                ("/", "find"),
                ("t", "test"),
                ("u/U", "undo/redo"),
                ("s", "save"),
                ("Tab", "view"),
                ("Esc", "back"),
            ]
        }
    }
}
