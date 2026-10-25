//! View Generation Settings: the settings the content defines (on/off,
//! number, dropdown, multi-select, text), which flow and table conditions
//! test. Create, rename, change options and defaults, mark as planned,
//! inspect what depends on a setting, delete it safely or restore it.

use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use settlement_core::content::EffectStatus;
use settlement_core::framework::describe::SettingType;
use settlement_core::framework::layers::ItemKind;
use settlement_core::framework::{SettingDef, SettingKind, SettingOption};
use settlement_core::ids::SettingId;
use settlement_core::layers::Origin;
use settlement_core::service::{AppService, DraftCheck, SettingDetail};

use super::common::issue_lines;
use super::open::{ItemRef, of_usage, open};
use super::tags::origin_label;
use crate::tui::widgets::{Modal, ModalResult, PickItem, Row, RowsState};
use crate::tui::{Ctx, Nav, Screen};

const KINDS: [(SettingType, &str); 5] = [
    (SettingType::Toggle, "On/off"),
    (SettingType::Number, "Number"),
    (SettingType::Choice, "Dropdown"),
    (SettingType::MultiSelect, "Multi-select"),
    (SettingType::Text, "Text"),
];

// ----- list ---------------------------------------------------------------------

enum ListPending {
    NewKind,
    Restore(String),
}

pub struct SettingDefsScreen {
    items: Vec<(String, String, Option<Origin>, bool)>,
    state: RowsState,
    error: Option<String>,
    modal: Option<(Modal, ListPending)>,
}

impl SettingDefsScreen {
    pub fn new(ctx: &mut Ctx) -> Self {
        let mut s = SettingDefsScreen {
            items: Vec::new(),
            state: RowsState::default(),
            error: None,
            modal: None,
        };
        s.resume(ctx);
        s
    }

    fn rows(&self, ctx: &Ctx) -> Vec<Row<(String, bool)>> {
        let s = &ctx.styles;
        self.items
            .iter()
            .map(|(id, name, origin, deleted)| {
                Row::item(
                    Line::from(vec![
                        Span::styled(
                            format!("{name:<34}"),
                            if *deleted { s.dim() } else { s.title() },
                        ),
                        Span::styled(format!("{id:<38}"), s.dim()),
                        Span::styled(origin_label(*origin, *deleted), s.dim()),
                    ]),
                    (id.clone(), *deleted),
                )
            })
            .collect()
    }
}

impl Screen for SettingDefsScreen {
    fn title(&self) -> String {
        "Generation Settings".into()
    }

    fn render(&mut self, f: &mut Frame, area: Rect, ctx: &Ctx) {
        let s = &ctx.styles;
        let [head, list] =
            Layout::vertical([Constraint::Length(2), Constraint::Fill(1)]).areas(area);
        let line = match &self.error {
            Some(e) => Line::styled(e.clone(), s.error()),
            None => Line::styled(
                "Settings generation offers; flow and table conditions can test them.",
                s.dim(),
            ),
        };
        f.render_widget(Paragraph::new(line), head);
        let rows = self.rows(ctx);
        self.state.render(f, list, &rows, s, true);
        if let Some((m, _)) = &mut self.modal {
            m.render(f, area, s);
        }
    }

    fn handle(&mut self, key: KeyEvent, ctx: &mut Ctx) -> Nav {
        if let Some((modal, pending)) = &mut self.modal {
            let result = modal.handle(key, &ctx.styles);
            if let ModalResult::Open = result {
                return Nav::Stay;
            }
            let pending = std::mem::replace(pending, ListPending::NewKind);
            self.modal = None;
            match (result, pending) {
                (ModalResult::Picked(k), ListPending::NewKind) => {
                    let setting = AppService::new_setting(KINDS[k].0);
                    return Nav::Push(Box::new(SettingEditor::open(ctx, None, Some(setting))));
                }
                (ModalResult::Yes, ListPending::Restore(id)) => {
                    if let Err(e) = ctx.service.reset_framework_item(ItemKind::Setting, &id) {
                        self.error = Some(e);
                    }
                    self.resume(ctx);
                }
                _ => {}
            }
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
                        Modal::confirm("Restore this deleted default setting?", "Restore"),
                        ListPending::Restore(id),
                    ));
                    Nav::Stay
                }
                Some((id, false)) => match ctx.service.setting_detail(&SettingId::new(&id)) {
                    Ok(d) => Nav::Push(Box::new(SettingEditor::open(ctx, Some(d), None))),
                    Err(e) => {
                        self.error = Some(e);
                        Nav::Stay
                    }
                },
                None => Nav::Stay,
            },
            KeyCode::Char('n') => {
                self.modal = Some((
                    Modal::pick(
                        "New setting: which kind?",
                        KINDS.iter().map(|(_, l)| PickItem::new(*l)).collect(),
                        0,
                    ),
                    ListPending::NewKind,
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
            ("n", "new setting"),
            ("Esc", "back"),
        ]
    }

    fn resume(&mut self, ctx: &mut Ctx) {
        match ctx.service.framework_items() {
            Ok(items) => {
                self.items = items
                    .into_iter()
                    .filter(|i| i.kind == ItemKind::Setting)
                    .map(|i| (i.id, i.name, i.origin, i.deleted))
                    .collect();
            }
            Err(e) => self.error = Some(e),
        }
    }
}

// ----- editor -------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq)]
enum Act {
    Name,
    Description,
    Kind,
    Planned,
    Note,
    Default,
    Min,
    Max,
    Option(usize),
    AddOption,
    Save,
    Revert,
    Reset,
    Delete,
    /// Open a dependent item.
    Open(ItemRef),
}

type Apply = Box<dyn FnOnce(&mut SettingEditor, ModalResult, &mut Ctx) -> Option<Nav>>;

pub struct SettingEditor {
    draft: SettingDef,
    original: Option<SettingDef>,
    detail: Option<SettingDetail>,
    check: Option<DraftCheck>,
    state: RowsState,
    modal: Option<(Modal, Apply)>,
    message: Option<(String, bool)>,
}

impl SettingEditor {
    fn open(ctx: &mut Ctx, detail: Option<SettingDetail>, new: Option<SettingDef>) -> Self {
        let draft = new
            .clone()
            .unwrap_or_else(|| detail.as_ref().expect("detail or new").setting.clone());
        let mut e = SettingEditor {
            original: new.is_none().then(|| draft.clone()),
            draft,
            detail,
            check: None,
            state: RowsState::default(),
            modal: None,
            message: None,
        };
        e.recheck(ctx);
        e
    }

    fn is_new(&self) -> bool {
        self.original.is_none()
    }

    fn dirty(&self) -> bool {
        self.original.as_ref() != Some(&self.draft)
    }

    fn recheck(&mut self, ctx: &mut Ctx) {
        self.check = ctx.service.check_setting(&self.draft).ok();
    }

    fn can_save(&self) -> bool {
        self.dirty() && self.check.as_ref().is_some_and(DraftCheck::is_acceptable)
    }

    fn ask(
        &mut self,
        modal: Modal,
        apply: impl FnOnce(&mut SettingEditor, ModalResult, &mut Ctx) -> Option<Nav> + 'static,
    ) {
        self.modal = Some((modal, Box::new(apply)));
    }

    fn ask_text(
        &mut self,
        title: &str,
        help: &str,
        value: &str,
        set: impl FnOnce(&mut SettingEditor, String) + 'static,
    ) {
        self.ask(Modal::text(title, help, value), move |e, r, _| {
            if let ModalResult::Text(t) = r {
                set(e, t);
            }
            None
        });
    }

    fn rows(&self, ctx: &Ctx) -> Vec<Row<Act>> {
        let s = &ctx.styles;
        let field = |label: &str, value: String| {
            Line::from(vec![
                Span::raw(format!("{label:<18}")),
                Span::styled(value, s.title()),
            ])
        };
        let d = &self.draft;
        let origin = if self.is_new() {
            "new custom setting (not saved)"
        } else {
            origin_label(self.detail.as_ref().and_then(|x| x.origin), false)
        };
        let mut rows = vec![
            Row::info(Line::from(vec![
                Span::styled("ID ", s.dim()),
                Span::raw(d.id.to_string()),
                Span::styled(format!(" · {origin}"), s.dim()),
            ])),
            Row::blank(),
            Row::item(field("Name", d.name.clone()), Act::Name),
            Row::item(
                field(
                    "Description",
                    if d.description.is_empty() {
                        "—".into()
                    } else {
                        d.description.clone()
                    },
                ),
                Act::Description,
            ),
            Row::item(field("Kind", d.kind.label().into()), Act::Kind),
            Row::item(
                field(
                    "Planned",
                    if d.is_planned() {
                        "Yes: documented, not offered; generation uses the default".into()
                    } else {
                        "No".into()
                    },
                ),
                Act::Planned,
            ),
        ];
        if d.is_planned() {
            rows.push(Row::item(
                field(
                    "Note",
                    if d.note.is_empty() {
                        "—".into()
                    } else {
                        d.note.clone()
                    },
                ),
                Act::Note,
            ));
        }
        rows.push(Row::blank());
        rows.push(Row::info(Line::styled("VALUES", s.dim())));
        rows.push(Row::item(
            field("Default", d.describe_value(&d.default_value())),
            Act::Default,
        ));
        match &d.kind {
            SettingKind::Number { min, max, .. } => {
                let show = |v: &Option<i64>| v.map_or_else(|| "none".into(), |n| n.to_string());
                rows.push(Row::item(field("Lowest", show(min)), Act::Min));
                rows.push(Row::item(field("Highest", show(max)), Act::Max));
            }
            SettingKind::Choice { options, .. } | SettingKind::MultiSelect { options, .. } => {
                for (i, o) in options.iter().enumerate() {
                    rows.push(Row::item(
                        Line::from(vec![
                            Span::raw(format!("  Option {:<9}", i + 1)),
                            Span::styled(o.name.clone(), s.title()),
                            Span::styled(format!("  ({})", o.id), s.dim()),
                        ]),
                        Act::Option(i),
                    ));
                }
                rows.push(Row::item(Line::raw("  + Add option"), Act::AddOption));
            }
            _ => {}
        }
        rows.push(Row::blank());
        rows.push(Row::info(Line::styled("DEPENDENCIES", s.dim())));
        match self.detail.as_ref().map(|x| &x.usages) {
            Some(usages) if !usages.is_empty() => {
                for u in usages {
                    rows.push(match of_usage(u) {
                        Some(item) => Row::item(
                            Line::raw(format!("  {}  ›", u.description)),
                            Act::Open(item),
                        ),
                        None => Row::info(Line::styled(format!("  {}", u.description), s.dim())),
                    });
                }
            }
            _ => rows.push(Row::info(Line::styled(
                "  Nothing tests this setting yet.",
                s.dim(),
            ))),
        }
        rows.push(Row::blank());
        rows.push(Row::info(Line::styled("VALIDATION", s.dim())));
        let (issues, error) = self
            .check
            .as_ref()
            .map_or((Vec::new(), None), |c| (c.issues.clone(), c.error.clone()));
        for line in issue_lines(&issues, error.as_deref(), s) {
            rows.push(Row::info(line));
        }
        rows.push(Row::blank());
        if self.can_save() {
            rows.push(Row::item(
                Line::styled("Save changes", s.heading()),
                Act::Save,
            ));
        } else {
            rows.push(Row::info(Line::styled(
                if self.dirty() {
                    "Save changes (fix the problems above)"
                } else {
                    "Save changes (no changes)"
                },
                s.dim(),
            )));
        }
        if self.dirty() && !self.is_new() {
            rows.push(Row::item(Line::raw("Revert"), Act::Revert));
        }
        let built_in = self.detail.as_ref().is_some_and(|x| x.built_in);
        let modified = self.detail.as_ref().and_then(|x| x.origin) == Some(Origin::Modified);
        if built_in && modified {
            rows.push(Row::item(Line::raw("Reset to default"), Act::Reset));
        }
        if !self.is_new() {
            rows.push(Row::item(Line::styled("Delete", s.error()), Act::Delete));
        }
        if let Some((m, error)) = &self.message {
            rows.push(Row::info(Line::styled(
                m.clone(),
                if *error { s.error() } else { s.good() },
            )));
        }
        rows
    }

    fn act(&mut self, act: Act, ctx: &mut Ctx) -> Option<Nav> {
        match act {
            Act::Open(item) => return Some(open(ctx, &item)),
            Act::Name => {
                let name = self.draft.name.clone();
                self.ask_text(
                    "Name",
                    "Renaming is safe: conditions use the ID.",
                    &name,
                    |e, t| e.draft.name = t,
                );
            }
            Act::Description => {
                let text = self.draft.description.clone();
                self.ask_text("Description", "", &text, |e, t| e.draft.description = t);
            }
            Act::Note => {
                let text = self.draft.note.clone();
                self.ask_text("Note", "What the planned setting is for.", &text, |e, t| {
                    e.draft.note = t
                });
            }
            Act::Kind => {
                self.ask(
                    Modal::pick(
                        "Kind (options and default start afresh)",
                        KINDS.iter().map(|(_, l)| PickItem::new(*l)).collect(),
                        0,
                    ),
                    |e, r, _| {
                        if let ModalResult::Picked(k) = r {
                            e.draft.kind = AppService::new_setting(KINDS[k].0).kind;
                        }
                        None
                    },
                );
            }
            Act::Planned => {
                self.draft.status = if self.draft.is_planned() {
                    EffectStatus::Active
                } else {
                    EffectStatus::Planned
                };
            }
            Act::Default => self.edit_default(),
            Act::Min | Act::Max => {
                let is_min = act == Act::Min;
                let current = match &self.draft.kind {
                    SettingKind::Number { min, max, .. } => {
                        if is_min {
                            *min
                        } else {
                            *max
                        }
                    }
                    _ => None,
                };
                self.ask_text(
                    if is_min {
                        "Lowest value"
                    } else {
                        "Highest value"
                    },
                    "A whole number; empty for no limit.",
                    &current.map(|n| n.to_string()).unwrap_or_default(),
                    move |e, t| {
                        let t = t.trim().to_owned();
                        let value = if t.is_empty() {
                            None
                        } else if let Ok(n) = t.parse() {
                            Some(n)
                        } else {
                            e.message = Some((format!("“{t}” is not a whole number."), true));
                            return;
                        };
                        if let SettingKind::Number { min, max, .. } = &mut e.draft.kind {
                            *(if is_min { min } else { max }) = value;
                        }
                    },
                );
            }
            Act::Option(i) => {
                let name = self
                    .draft
                    .kind
                    .options()
                    .get(i)
                    .map(|o| o.name.clone())
                    .unwrap_or_default();
                self.ask(
                    Modal::pick(
                        &format!("Option “{name}”"),
                        vec![PickItem::new("Rename"), PickItem::new("Remove")],
                        0,
                    ),
                    move |e, r, _| {
                        match r {
                            ModalResult::Picked(0) => e.ask_text(
                                "Option name",
                                "Its ID stays the same, so conditions keep working.",
                                &name,
                                move |e, t| {
                                    if let Some(o) = options_mut(&mut e.draft.kind).get_mut(i) {
                                        o.name = t;
                                    }
                                },
                            ),
                            ModalResult::Picked(1) => remove_option(&mut e.draft.kind, i),
                            _ => {}
                        }
                        None
                    },
                );
            }
            Act::AddOption => {
                let options = options_mut(&mut self.draft.kind);
                let taken: Vec<String> = options.iter().map(|o| o.id.clone()).collect();
                let name = format!("Option {}", options.len() + 1);
                options.push(SettingOption {
                    id: option_id(&name, &taken),
                    name,
                });
            }
            Act::Save => match ctx.service.save_setting(self.draft.clone()) {
                Ok(detail) => {
                    self.draft = detail.setting.clone();
                    self.original = Some(self.draft.clone());
                    self.detail = Some(detail);
                    self.message = Some(("Saved".into(), false));
                }
                Err(e) => self.message = Some((e, true)),
            },
            Act::Revert => {
                if let Some(o) = &self.original {
                    self.draft = o.clone();
                }
            }
            Act::Reset => {
                self.ask(
                    Modal::confirm("Restore this setting to its installed default?", "Restore"),
                    |e, r, ctx| {
                        if let ModalResult::Yes = r {
                            let id = e.draft.id.clone();
                            match ctx
                                .service
                                .reset_framework_item(ItemKind::Setting, id.as_str())
                                .and_then(|_| ctx.service.setting_detail(&id))
                            {
                                Ok(d) => {
                                    e.draft = d.setting.clone();
                                    e.original = Some(e.draft.clone());
                                    e.detail = Some(d);
                                }
                                Err(err) => e.message = Some((err, true)),
                            }
                        }
                        None
                    },
                );
            }
            Act::Delete => {
                let name = self.draft.name.clone();
                self.ask(
                    Modal::confirm(
                        &format!("Delete the setting “{name}”? A default can be restored later."),
                        "Delete",
                    ),
                    |e, r, ctx| {
                        if let ModalResult::Yes = r {
                            match ctx.service.delete_setting(&e.draft.id) {
                                Ok(_) => {
                                    ctx.flash = Some("Setting deleted".into());
                                    return Some(Nav::Pop);
                                }
                                Err(err) => e.message = Some((err, true)),
                            }
                        }
                        None
                    },
                );
            }
        }
        None
    }

    fn edit_default(&mut self) {
        match self.draft.kind.clone() {
            SettingKind::Toggle { default } => {
                self.draft.kind = SettingKind::Toggle { default: !default };
            }
            SettingKind::Number { default, .. } => self.ask_text(
                "Default",
                "A whole number.",
                &default.to_string(),
                |e, t| match t.trim().parse::<i64>() {
                    Ok(n) => {
                        if let SettingKind::Number { default, .. } = &mut e.draft.kind {
                            *default = n;
                        }
                    }
                    Err(_) => e.message = Some((format!("“{t}” is not a whole number."), true)),
                },
            ),
            SettingKind::Text { default } => {
                self.ask_text("Default", "Up to 200 characters.", &default, |e, t| {
                    e.draft.kind = SettingKind::Text { default: t }
                })
            }
            SettingKind::Choice { options, default } => {
                let at = options.iter().position(|o| o.id == default).unwrap_or(0);
                let items = options
                    .iter()
                    .map(|o| PickItem::new(o.name.clone()))
                    .collect();
                self.ask(Modal::pick("Default", items, at), move |e, r, _| {
                    if let (ModalResult::Picked(k), SettingKind::Choice { options, default }) =
                        (r, &mut e.draft.kind)
                        && let Some(o) = options.get(k)
                    {
                        *default = o.id.clone();
                    }
                    None
                });
            }
            SettingKind::MultiSelect { options, default } => {
                let names = options.iter().map(|o| o.name.clone()).collect();
                let ticked = options.iter().map(|o| default.contains(&o.id)).collect();
                self.ask(
                    Modal::check("Ticked by default (Space to tick)", names, ticked),
                    move |e, r, _| {
                        if let (
                            ModalResult::Checked(ticks),
                            SettingKind::MultiSelect { options, default },
                        ) = (r, &mut e.draft.kind)
                        {
                            *default = options
                                .iter()
                                .zip(ticks)
                                .filter(|(_, t)| *t)
                                .map(|(o, _)| o.id.clone())
                                .collect();
                        }
                        None
                    },
                );
            }
        }
    }
}

fn options_mut(kind: &mut SettingKind) -> &mut Vec<SettingOption> {
    match kind {
        SettingKind::Choice { options, .. } | SettingKind::MultiSelect { options, .. } => options,
        _ => unreachable!("only dropdowns and multi-selects have options"),
    }
}

/// Removes option `i`; a default that named it moves to what remains.
fn remove_option(kind: &mut SettingKind, i: usize) {
    match kind {
        SettingKind::Choice { options, default } if options.len() > 1 => {
            let gone = options.remove(i);
            if *default == gone.id {
                *default = options[0].id.clone();
            }
        }
        SettingKind::MultiSelect { options, default } if options.len() > 1 => {
            let gone = options.remove(i);
            default.retain(|d| *d != gone.id);
        }
        _ => {}
    }
}

/// A new option ID from its name, unique among `taken`.
fn option_id(name: &str, taken: &[String]) -> String {
    let slug: String = name
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    let base = slug.trim_matches('-').to_owned();
    let base = if base.is_empty() {
        "option".into()
    } else {
        base
    };
    let mut id = base.clone();
    let mut n = 2;
    while taken.contains(&id) {
        id = format!("{base}-{n}");
        n += 1;
    }
    id
}

impl Screen for SettingEditor {
    fn title(&self) -> String {
        format!("Generation Settings › {}", self.draft.name)
    }

    fn render(&mut self, f: &mut Frame, area: Rect, ctx: &Ctx) {
        let rows = self.rows(ctx);
        self.state.render(f, area, &rows, &ctx.styles, true);
        if let Some((m, _)) = &mut self.modal {
            m.render(f, area, &ctx.styles);
        }
    }

    fn handle(&mut self, key: KeyEvent, ctx: &mut Ctx) -> Nav {
        let before = self.draft.clone();
        let nav = if let Some((modal, _)) = &mut self.modal {
            let result = modal.handle(key, &ctx.styles);
            if let ModalResult::Open = result {
                return Nav::Stay;
            }
            let (_, apply) = self.modal.take().expect("an open modal");
            apply(self, result, ctx)
        } else {
            let rows = self.rows(ctx);
            if self.state.handle_nav(&rows, key) {
                return Nav::Stay;
            }
            match key.code {
                KeyCode::Esc | KeyCode::Backspace if self.dirty() => {
                    self.ask(
                        Modal::confirm("Discard your unsaved changes?", "Discard"),
                        |_, r, _| matches!(r, ModalResult::Yes).then_some(Nav::Pop),
                    );
                    None
                }
                KeyCode::Esc | KeyCode::Backspace => Some(Nav::Pop),
                KeyCode::Char('s') if self.can_save() => self.act(Act::Save, ctx),
                KeyCode::Enter | KeyCode::Char(' ') => match self.state.current(&rows).cloned() {
                    Some(act) => self.act(act, ctx),
                    None => None,
                },
                _ => None,
            }
        };
        if self.draft != before {
            self.message = None;
            self.recheck(ctx);
        }
        nav.unwrap_or(Nav::Stay)
    }

    fn hints(&self) -> Vec<(&'static str, &'static str)> {
        vec![
            ("↑↓", "move"),
            ("Enter", "change"),
            ("s", "save"),
            ("Esc", "back"),
        ]
    }
}
