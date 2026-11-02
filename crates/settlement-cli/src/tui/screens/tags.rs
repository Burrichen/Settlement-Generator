//! View Tags: the tag list and a stepped tag editor. Effects are edited from
//! the core's effect schema, so new effect kinds need no new screen code.

use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use serde_json::{Map, Value};
use settlement_core::content::{EffectDef, Tag};
use settlement_core::editor::{EffectKindSchema, FieldKind, UsageKind, effect_schema};
use settlement_core::explain::Explanation;
use settlement_core::framework::layers::ItemKind;
use settlement_core::ids::{TableId, TagId};
use settlement_core::service::{AppService, ContentOverview, DraftCheck, TagDetail, TagInsight};

use super::common::issue_lines;
use super::open::{ItemRef, of_tag_usage, open};
use crate::tui::widgets::{Modal, ModalResult, PickItem, Row, RowsState};
use crate::tui::{Ctx, Nav, Screen};

pub fn origin_label(
    origin: Option<settlement_core::layers::Origin>,
    deleted: bool,
) -> &'static str {
    use settlement_core::layers::Origin;
    match (deleted, origin) {
        (true, _) => "deleted default",
        (_, Some(Origin::Bundled)) => "default",
        (_, Some(Origin::Modified)) => "customised",
        (_, Some(Origin::Custom)) => "custom",
        _ => "",
    }
}

// ----- list ---------------------------------------------------------------------

pub struct TagsScreen {
    overview: Option<ContentOverview>,
    state: RowsState,
    filter: String,
    error: Option<String>,
    modal: Option<(Modal, ListPending)>,
}

enum ListPending {
    Filter,
    Restore(TagId),
}

impl TagsScreen {
    pub fn new(ctx: &mut Ctx) -> Self {
        let mut s = TagsScreen {
            overview: None,
            state: RowsState::default(),
            filter: String::new(),
            error: None,
            modal: None,
        };
        s.resume(ctx);
        s
    }

    fn rows(&self, ctx: &Ctx) -> Vec<Row<(TagId, bool)>> {
        let s = &ctx.styles;
        let needle = self.filter.to_lowercase();
        let Some(o) = &self.overview else {
            return vec![];
        };
        o.tags
            .iter()
            .filter(|t| {
                needle.is_empty()
                    || t.name.to_lowercase().contains(&needle)
                    || t.id.as_str().to_lowercase().contains(&needle)
            })
            .map(|t| {
                let mut badges = vec![origin_label(t.origin, t.deleted)];
                if !t.enabled && !t.deleted {
                    badges.push("disabled");
                }
                let style = if t.enabled && !t.deleted {
                    s.title()
                } else {
                    s.dim()
                };
                Row::item(
                    Line::from(vec![
                        Span::styled(format!("{:<28}", t.name), style),
                        Span::styled(format!("{:<34}", t.id.to_string()), s.dim()),
                        Span::styled(badges.join(" · "), s.dim()),
                    ]),
                    (t.id.clone(), t.deleted),
                )
            })
            .collect()
    }
}

impl Screen for TagsScreen {
    fn title(&self) -> String {
        "Tags".into()
    }

    fn render(&mut self, f: &mut Frame, area: Rect, ctx: &Ctx) {
        let s = &ctx.styles;
        let [head, list] = ratatui::layout::Layout::vertical([
            ratatui::layout::Constraint::Length(2),
            ratatui::layout::Constraint::Fill(1),
        ])
        .areas(area);
        let mut line = vec![Span::styled(
            if self.filter.is_empty() {
                "All tags".to_owned()
            } else {
                format!("Filter: {}", self.filter)
            },
            s.dim(),
        )];
        if let Some(e) = &self.error {
            line.push(Span::styled(format!("   {e}"), s.error()));
        }
        f.render_widget(ratatui::widgets::Paragraph::new(Line::from(line)), head);
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
                    if let Err(e) = ctx.service.reset_tag(&id) {
                        self.error = Some(e);
                    }
                    self.modal = None;
                    self.resume(ctx);
                    return Nav::Stay;
                }
                _ => {}
            }
            self.modal = None;
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
                        Modal::confirm("Restore this deleted default tag?", "Restore"),
                        ListPending::Restore(id),
                    ));
                    Nav::Stay
                }
                Some((id, false)) => match ctx.service.tag_detail(&id) {
                    Ok(detail) => Nav::Push(Box::new(TagEditor::open(ctx, Some(detail), None))),
                    Err(e) => {
                        self.error = Some(e);
                        Nav::Stay
                    }
                },
                None => Nav::Stay,
            },
            KeyCode::Char('n') => Nav::Push(Box::new(TagEditor::open(
                ctx,
                None,
                Some(AppService::new_tag()),
            ))),
            KeyCode::Char('/') => {
                self.modal = Some((
                    Modal::text("Filter tags", "Name or ID; empty shows all.", &self.filter),
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
            ("n", "new tag"),
            ("/", "filter"),
            ("Esc", "back"),
        ]
    }

    fn resume(&mut self, ctx: &mut Ctx) {
        match ctx.service.overview() {
            Ok(o) => self.overview = Some(o),
            Err(e) => self.error = Some(e),
        }
    }
}

// ----- editor -------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq)]
enum Act {
    Name,
    Description,
    Enabled,
    Effect(usize),
    AddEffect,
    Save,
    Revert,
    Reset,
    Delete,
    // Effect page
    Kind,
    Status,
    Field(String),
    Note,
    RemoveEffect,
    Done,
    /// Open a dependent item.
    Open(ItemRef),
}

enum Pending {
    Name,
    Description,
    AddEffect,
    Kind,
    Note,
    Field(String),
    /// Pick or checklist values for a field, by index into `choices`.
    FieldChoice(String, Vec<String>, bool),
    ConfirmDelete,
    ConfirmLeave,
}

/// The editable fields of a tag: name, description, enabled, effects.
type TagFields = (String, String, bool, Vec<Map<String, Value>>);

pub struct TagEditor {
    id: TagId,
    name: String,
    description: String,
    enabled: bool,
    /// Effects as JSON objects, so incomplete ones can be edited.
    effects: Vec<Map<String, Value>>,
    original: Option<TagFields>,
    detail: Option<TagDetail>,
    is_new: bool,
    check: Option<DraftCheck>,
    build_error: Option<String>,
    /// The draft's effects in plain words, from the core.
    insight: Option<TagInsight>,
    schema: Vec<EffectKindSchema>,
    page: Option<usize>,
    state: RowsState,
    modal: Option<(Modal, Pending)>,
    message: Option<(String, bool)>,
}

fn effect_maps(tag: &Tag) -> Vec<Map<String, Value>> {
    tag.effects
        .iter()
        .map(|e| match serde_json::to_value(e) {
            Ok(Value::Object(m)) => m,
            _ => Map::new(),
        })
        .collect()
}

impl TagEditor {
    pub(crate) fn open(ctx: &mut Ctx, detail: Option<TagDetail>, new: Option<Tag>) -> Self {
        let is_new = new.is_some();
        let tag = new.unwrap_or_else(|| detail.as_ref().expect("detail or new").tag.clone());
        let mut editor = TagEditor {
            id: tag.id.clone(),
            name: tag.name.clone(),
            description: tag.description.clone(),
            enabled: tag.enabled,
            effects: effect_maps(&tag),
            original: None,
            detail,
            is_new,
            check: None,
            build_error: None,
            insight: None,
            schema: effect_schema(),
            page: None,
            state: RowsState::default(),
            modal: None,
            message: None,
        };
        editor.original = Some(editor.snapshot());
        editor.recheck(ctx);
        editor
    }

    fn snapshot(&self) -> TagFields {
        (
            self.name.clone(),
            self.description.clone(),
            self.enabled,
            self.effects.clone(),
        )
    }

    fn dirty(&self) -> bool {
        self.is_new || self.original.as_ref() != Some(&self.snapshot())
    }

    /// The draft as a tag, or why it is not one yet.
    fn build(&self) -> Result<Tag, String> {
        let effects: Result<Vec<EffectDef>, String> = self
            .effects
            .iter()
            .enumerate()
            .map(|(i, m)| {
                serde_json::from_value(Value::Object(m.clone()))
                    .map_err(|e| format!("Effect {} is incomplete: {e}", i + 1))
            })
            .collect();
        Ok(Tag {
            id: self.id.clone(),
            name: self.name.clone(),
            description: self.description.clone(),
            effects: effects?,
            enabled: self.enabled,
        })
    }

    fn recheck(&mut self, ctx: &mut Ctx) {
        match self.build() {
            Ok(tag) => {
                self.build_error = None;
                self.insight = ctx.service.explain_tag(&tag).ok();
                self.check = ctx
                    .service
                    .check_tag(tag)
                    .map_err(|e| ctx.flash = Some(e))
                    .ok();
            }
            Err(e) => {
                self.build_error = Some(e);
                self.check = None;
                self.insight = None;
            }
        }
    }

    fn can_save(&self) -> bool {
        self.dirty()
            && self.build_error.is_none()
            && self.check.as_ref().is_some_and(DraftCheck::is_acceptable)
    }

    fn kind_schema(&self, kind: &str) -> Option<&EffectKindSchema> {
        self.schema.iter().find(|k| k.kind == kind)
    }

    fn describe_effect(&self, m: &Map<String, Value>) -> String {
        let kind = m.get("kind").and_then(Value::as_str).unwrap_or("?");
        let label = self.kind_schema(kind).map_or(kind, |k| k.label.as_str());
        let fields: Vec<String> = self
            .kind_schema(kind)
            .map(|k| {
                k.fields
                    .iter()
                    .filter_map(|f| {
                        m.get(&f.name)
                            .map(|v| format!("{} {}", f.label.to_lowercase(), show(v)))
                    })
                    .collect()
            })
            .unwrap_or_default();
        let planned = m.get("status").and_then(Value::as_str) == Some("planned");
        format!(
            "{label}{}{}",
            if fields.is_empty() {
                String::new()
            } else {
                format!(": {}", fields.join(", "))
            },
            if planned { "  [planned]" } else { "" }
        )
    }

    fn rows(&self, ctx: &Ctx) -> Vec<Row<Act>> {
        let s = &ctx.styles;
        let field = |label: &str, value: String| {
            Line::from(vec![
                Span::raw(format!("{label:<16}")),
                Span::styled(value, s.title()),
            ])
        };
        let mut rows = Vec::new();
        if let Some(i) = self.page {
            let m = &self.effects[i];
            let kind = m.get("kind").and_then(Value::as_str).unwrap_or("");
            let schema = self.kind_schema(kind);
            rows.push(Row::info(Line::styled(
                format!("EFFECT {}", i + 1),
                s.dim(),
            )));
            rows.push(Row::item(
                field("Kind", schema.map_or(kind.to_owned(), |k| k.label.clone())),
                Act::Kind,
            ));
            if let Some(k) = schema {
                rows.push(Row::info(Line::styled(k.description.clone(), s.dim())));
            }
            let status = m.get("status").and_then(Value::as_str).unwrap_or("active");
            if schema.is_some_and(|k| k.executable) {
                rows.push(Row::item(field("Status", status.to_owned()), Act::Status));
            } else {
                rows.push(Row::info(
                    field("Status", "planned (always)".into()).style(s.dim()),
                ));
            }
            for f in schema.map(|k| k.fields.clone()).unwrap_or_default() {
                let value = m.get(&f.name).map_or_else(
                    || {
                        if f.required {
                            "(required)".into()
                        } else {
                            "—".into()
                        }
                    },
                    show,
                );
                rows.push(Row::item(
                    field(&f.label, value),
                    Act::Field(f.name.clone()),
                ));
            }
            rows.push(Row::item(
                field("Note", m.get("note").map_or("—".into(), show)),
                Act::Note,
            ));
            if let Some(e) = self.explanation(i) {
                rows.push(Row::blank());
                rows.push(Row::info(Line::styled("IN PLAIN WORDS", s.dim())));
                rows.extend(explanation_lines(e, ctx).into_iter().map(Row::info));
            }
            rows.push(Row::blank());
            rows.push(Row::item(
                Line::styled("Remove this effect", s.error()),
                Act::RemoveEffect,
            ));
            rows.push(Row::item(Line::raw("‹ Done"), Act::Done));
            return rows;
        }

        let origin = if self.is_new {
            "new custom tag (not saved)"
        } else {
            origin_label(self.detail.as_ref().and_then(|d| d.origin), false)
        };
        rows.push(Row::info(Line::from(vec![
            Span::styled("ID ", s.dim()),
            Span::raw(self.id.to_string()),
            Span::styled(format!(" · {origin}"), s.dim()),
        ])));
        rows.push(Row::blank());
        rows.push(Row::item(field("Name", self.name.clone()), Act::Name));
        rows.push(Row::item(
            field(
                "Description",
                if self.description.is_empty() {
                    "—".into()
                } else {
                    self.description.clone()
                },
            ),
            Act::Description,
        ));
        rows.push(Row::item(
            field(
                "Enabled",
                if self.enabled {
                    "On".into()
                } else {
                    "Off".into()
                },
            ),
            Act::Enabled,
        ));
        rows.push(Row::blank());
        rows.push(Row::info(Line::styled("EFFECTS", s.dim())));
        for (i, m) in self.effects.iter().enumerate() {
            rows.push(Row::item(
                Line::raw(format!("{}. {}", i + 1, self.describe_effect(m))),
                Act::Effect(i),
            ));
            if let Some(e) = self.explanation(i) {
                for line in wrap(&e.summary, 100) {
                    rows.push(Row::info(Line::styled(format!("     {line}"), s.dim())));
                }
            }
        }
        rows.push(Row::item(Line::raw("+ Add effect"), Act::AddEffect));
        if let Some(insight) = &self.insight {
            if insight.informational {
                rows.push(Row::info(Line::styled(
                    "Informational only: it labels the settlement, and conditions can still test it.",
                    s.dim(),
                )));
            }
            for p in &insight.combined {
                rows.push(Row::info(Line::from(vec![
                    Span::styled(format!("Together on {}: ", p.table.name), s.dim()),
                    Span::raw(format!("{}.", p.steps.join(" → "))),
                ])));
            }
        }
        rows.push(Row::blank());
        rows.push(Row::info(Line::styled("DEPENDENCIES", s.dim())));
        match self.detail.as_ref().map(|d| &d.usages) {
            Some(usages) if !usages.is_empty() => {
                for (kind, title) in [
                    (UsageKind::EmittedBy, "Given by"),
                    (UsageKind::AddedBy, "Added by"),
                    (UsageKind::RemovedBy, "Removed by"),
                    (UsageKind::CheckedBy, "Checked by"),
                ] {
                    let of_kind: Vec<_> = usages.iter().filter(|u| u.kind == kind).collect();
                    if of_kind.is_empty() {
                        continue;
                    }
                    rows.push(Row::info(Line::raw(format!("{title}:"))));
                    for u in of_kind {
                        let text = u
                            .description
                            .strip_prefix(&format!("{title} "))
                            .unwrap_or(&u.description);
                        rows.push(match of_tag_usage(u) {
                            Some(item) => {
                                Row::item(Line::raw(format!("  {text}  ›")), Act::Open(item))
                            }
                            None => Row::info(Line::styled(format!("  {text}"), s.dim())),
                        });
                    }
                }
            }
            _ => rows.push(Row::info(Line::styled(
                "Nothing uses this tag; it can be deleted safely.",
                s.dim(),
            ))),
        }
        rows.push(Row::blank());
        rows.push(Row::info(Line::styled("VALIDATION", s.dim())));
        let issues = self
            .check
            .as_ref()
            .map(|c| c.issues.clone())
            .unwrap_or_default();
        let error = self
            .build_error
            .clone()
            .or_else(|| self.check.as_ref().and_then(|c| c.error.clone()));
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
        if self.dirty() && !self.is_new {
            rows.push(Row::item(Line::raw("Revert"), Act::Revert));
        }
        let built_in = self.detail.as_ref().is_some_and(|d| d.built_in);
        let modified = self.detail.as_ref().and_then(|d| d.origin)
            == Some(settlement_core::layers::Origin::Modified);
        if built_in && modified {
            rows.push(Row::item(Line::raw("Reset to default"), Act::Reset));
        }
        if !self.is_new {
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

    fn choices_for(
        &self,
        ctx: &mut Ctx,
        kind: &FieldKind,
        effect: &Map<String, Value>,
    ) -> Vec<(String, String)> {
        let overview = ctx.service.overview().map_err(|e| ctx.flash = Some(e)).ok();
        match kind {
            FieldKind::Table => overview
                .map(|o| {
                    o.tables
                        .into_iter()
                        .filter(|t| !t.deleted)
                        .map(|t| (t.id.to_string(), t.name))
                        .collect()
                })
                .unwrap_or_default(),
            FieldKind::Flow => ctx
                .service
                .framework_items()
                .map(|items| {
                    items
                        .into_iter()
                        .filter(|i| i.kind == ItemKind::Flow && !i.deleted)
                        .map(|i| (i.id, i.name))
                        .collect()
                })
                .unwrap_or_default(),
            FieldKind::Tag => overview
                .map(|o| {
                    o.tags
                        .into_iter()
                        .filter(|t| !t.deleted)
                        .map(|t| (t.id.to_string(), t.name))
                        .collect()
                })
                .unwrap_or_default(),
            FieldKind::Variant { table_field }
            | FieldKind::Result { table_field }
            | FieldKind::ResultList { table_field } => {
                let Some(table) = effect.get(table_field).and_then(Value::as_str) else {
                    return vec![];
                };
                let Ok(detail) = ctx.service.table_detail(&TableId::new(table)) else {
                    return vec![];
                };
                if matches!(kind, FieldKind::Variant { .. }) {
                    detail
                        .table
                        .variants
                        .into_iter()
                        .map(|v| (v.id.to_string(), v.name))
                        .collect()
                } else {
                    detail
                        .table
                        .results
                        .into_iter()
                        .map(|r| (r.id.to_string(), r.name))
                        .collect()
                }
            }
            FieldKind::Choice { options } => options
                .iter()
                .map(|o| (o.value.clone(), o.label.clone()))
                .collect(),
            _ => vec![],
        }
    }

    fn edit_field(&mut self, name: String, ctx: &mut Ctx) {
        let i = self.page.expect("on an effect page");
        let effect = self.effects[i].clone();
        let kind = effect.get("kind").and_then(Value::as_str).unwrap_or("");
        let Some(field) = self
            .kind_schema(kind)
            .and_then(|k| k.fields.iter().find(|f| f.name == name))
            .cloned()
        else {
            return;
        };
        match &field.kind {
            FieldKind::Dice | FieldKind::Text | FieldKind::Integer => {
                let current = effect.get(&name).map_or(String::new(), show);
                self.modal = Some((
                    Modal::text(&field.label, &field.help, &current),
                    Pending::Field(name),
                ));
            }
            kind => {
                let choices = self.choices_for(ctx, kind, &effect);
                if choices.is_empty() {
                    self.message =
                        Some(("Choose the table first (nothing to pick yet).".into(), true));
                    return;
                }
                let ids: Vec<String> = choices.iter().map(|(id, _)| id.clone()).collect();
                if matches!(kind, FieldKind::ResultList { .. }) {
                    let selected: Vec<String> = effect
                        .get(&name)
                        .and_then(Value::as_array)
                        .map(|a| {
                            a.iter()
                                .filter_map(|v| v.as_str().map(str::to_owned))
                                .collect()
                        })
                        .unwrap_or_default();
                    self.modal = Some((
                        Modal::check(
                            &field.label,
                            choices.iter().map(|(_, n)| n.clone()).collect(),
                            ids.iter().map(|id| selected.contains(id)).collect(),
                        ),
                        Pending::FieldChoice(name, ids, true),
                    ));
                } else {
                    let mut items: Vec<PickItem> = choices
                        .iter()
                        .map(|(id, n)| PickItem::new(n.clone()).hint(id.clone()))
                        .collect();
                    let mut ids = ids;
                    if !field.required {
                        items.insert(0, PickItem::new("(none)"));
                        ids.insert(0, String::new());
                    }
                    self.modal = Some((
                        Modal::pick(&field.label, items, 0),
                        Pending::FieldChoice(name, ids, false),
                    ));
                }
            }
        }
    }

    fn set_field(&mut self, name: &str, value: Option<Value>) {
        if let Some(i) = self.page {
            match value {
                Some(v) => {
                    self.effects[i].insert(name.to_owned(), v);
                }
                None => {
                    self.effects[i].remove(name);
                }
            }
        }
    }

    fn handle_modal(&mut self, key: KeyEvent, ctx: &mut Ctx) -> Option<Nav> {
        let (modal, pending) = self.modal.as_mut()?;
        let result = modal.handle(key, &ctx.styles);
        if matches!(result, ModalResult::Open) {
            return Some(Nav::Stay);
        }
        let pending = std::mem::replace(pending, Pending::Name);
        self.modal = None;
        match (result, pending) {
            (ModalResult::Text(t), Pending::Name) => self.name = t,
            (ModalResult::Text(t), Pending::Description) => self.description = t,
            (ModalResult::Text(t), Pending::Note) => {
                self.set_field("note", (!t.is_empty()).then_some(Value::String(t)))
            }
            (ModalResult::Text(t), Pending::Field(name)) => {
                let kind = self.page.and_then(|i| {
                    let k = self.effects[i]
                        .get("kind")
                        .and_then(Value::as_str)?
                        .to_owned();
                    self.kind_schema(&k)?
                        .fields
                        .iter()
                        .find(|f| f.name == name)
                        .map(|f| f.kind.clone())
                });
                let value = if t.trim().is_empty() {
                    None
                } else if matches!(kind, Some(FieldKind::Integer)) {
                    match t.trim().parse::<i64>() {
                        Ok(n) => Some(Value::from(n)),
                        Err(_) => {
                            self.message = Some(("Enter a whole number.".into(), true));
                            return Some(Nav::Stay);
                        }
                    }
                } else {
                    Some(Value::String(t))
                };
                self.set_field(&name, value);
            }
            (ModalResult::Picked(k), Pending::FieldChoice(name, ids, false)) => {
                let id = ids[k].clone();
                self.set_field(&name, (!id.is_empty()).then_some(Value::String(id)));
            }
            (ModalResult::Checked(checked), Pending::FieldChoice(name, ids, true)) => {
                let chosen: Vec<Value> = ids
                    .iter()
                    .zip(checked)
                    .filter(|(_, c)| *c)
                    .map(|(id, _)| Value::String(id.clone()))
                    .collect();
                self.set_field(&name, (!chosen.is_empty()).then_some(Value::Array(chosen)));
            }
            (ModalResult::Picked(k), Pending::AddEffect) => {
                let (kind, preset) = add_choices(&self.schema)[k];
                let schema = &self.schema[kind];
                let mut m = Map::new();
                m.insert("kind".into(), Value::String(schema.kind.clone()));
                if !schema.executable {
                    m.insert("status".into(), Value::String("planned".into()));
                }
                // A preset such as "Shift +1" fills its fields in.
                if let Some(p) = preset.and_then(|p| schema.presets.get(p)) {
                    m.extend(p.values.iter().map(|(k, v)| (k.clone(), v.clone())));
                }
                self.effects.push(m);
                self.page = Some(self.effects.len() - 1);
                self.state = RowsState::default();
            }
            (ModalResult::Picked(k), Pending::Kind) => {
                if let Some(i) = self.page {
                    let schema = &self.schema[k];
                    let note = self.effects[i].get("note").cloned();
                    let mut m = Map::new();
                    m.insert("kind".into(), Value::String(schema.kind.clone()));
                    if !schema.executable {
                        m.insert("status".into(), Value::String("planned".into()));
                    }
                    if let Some(n) = note {
                        m.insert("note".into(), n);
                    }
                    self.effects[i] = m;
                }
            }
            (ModalResult::Yes, Pending::ConfirmDelete) => {
                return Some(match ctx.service.delete_tag(&self.id) {
                    Ok(_) => {
                        ctx.flash = Some("Tag deleted".into());
                        Nav::Pop
                    }
                    Err(e) => {
                        self.message = Some((e, true));
                        Nav::Stay
                    }
                });
            }
            (ModalResult::Yes, Pending::ConfirmLeave) => return Some(Nav::Pop),
            _ => {}
        }
        self.recheck(ctx);
        Some(Nav::Stay)
    }

    fn save(&mut self, ctx: &mut Ctx) {
        let result = self.build().and_then(|t| ctx.service.save_tag(t));
        match result {
            Ok(detail) => {
                let tag = detail.tag.clone();
                self.effects = effect_maps(&tag);
                self.name = tag.name;
                self.description = tag.description;
                self.enabled = tag.enabled;
                self.detail = Some(detail);
                self.is_new = false;
                self.original = Some(self.snapshot());
                self.message = Some(("Saved".into(), false));
                self.recheck(ctx);
            }
            Err(e) => self.message = Some((e, true)),
        }
    }
}

/// A JSON field value as plain text.
fn show(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Array(a) => a.iter().map(show).collect::<Vec<_>>().join(", "),
        Value::Null => "—".into(),
        other => other.to_string(),
    }
}

impl Screen for TagEditor {
    fn title(&self) -> String {
        format!(
            "Tags › {}",
            if self.name.is_empty() {
                "Unnamed"
            } else {
                &self.name
            }
        )
    }

    fn render(&mut self, f: &mut Frame, area: Rect, ctx: &Ctx) {
        let rows = self.rows(ctx);
        self.state.render(f, area, &rows, &ctx.styles, true);
        if let Some((m, _)) = &mut self.modal {
            m.render(f, area, &ctx.styles);
        }
    }

    fn handle(&mut self, key: KeyEvent, ctx: &mut Ctx) -> Nav {
        if let Some(nav) = self.handle_modal(key, ctx) {
            return nav;
        }
        let rows = self.rows(ctx);
        if self.state.handle_nav(&rows, key) {
            return Nav::Stay;
        }
        if matches!(key.code, KeyCode::Esc | KeyCode::Backspace) {
            if self.page.is_some() {
                self.page = None;
                self.state = RowsState::default();
                return Nav::Stay;
            }
            if self.dirty() {
                self.modal = Some((
                    Modal::confirm("Discard your unsaved changes?", "Discard"),
                    Pending::ConfirmLeave,
                ));
                return Nav::Stay;
            }
            return Nav::Pop;
        }
        if key.code == KeyCode::Char('s') && self.page.is_none() && self.can_save() {
            self.save(ctx);
            return Nav::Stay;
        }
        if !matches!(key.code, KeyCode::Enter | KeyCode::Char(' ')) {
            return Nav::Stay;
        }
        self.message = None;
        let Some(act) = self.state.current(&rows).cloned() else {
            return Nav::Stay;
        };
        match act {
            Act::Open(item) => return open(ctx, &item),
            Act::Name => {
                self.modal = Some((
                    Modal::text(
                        "Name",
                        "Renaming is safe: references use the ID.",
                        &self.name,
                    ),
                    Pending::Name,
                ))
            }
            Act::Description => {
                self.modal = Some((
                    Modal::text("Description", "", &self.description),
                    Pending::Description,
                ))
            }
            Act::Enabled => {
                self.enabled = !self.enabled;
                self.recheck(ctx);
            }
            Act::Effect(i) => {
                self.page = Some(i);
                self.state = RowsState::default();
            }
            Act::AddEffect => {
                let items = add_choices(&self.schema)
                    .into_iter()
                    .map(|(kind, preset)| {
                        let k = &self.schema[kind];
                        match preset {
                            Some(p) => PickItem::new(format!("  {}", k.presets[p].label))
                                .hint(k.kind.clone()),
                            None => PickItem::new(k.label.clone()).hint(k.group.clone()),
                        }
                    })
                    .collect();
                self.modal = Some((Modal::pick("Add effect", items, 0), Pending::AddEffect));
            }
            Act::Kind => {
                self.modal = Some((
                    Modal::pick(
                        "Effect kind",
                        self.schema
                            .iter()
                            .map(|k| PickItem::new(k.label.clone()).hint(k.kind.clone()))
                            .collect(),
                        0,
                    ),
                    Pending::Kind,
                ));
            }
            Act::Status => {
                let planned = self
                    .page
                    .and_then(|i| self.effects[i].get("status").and_then(Value::as_str))
                    == Some("planned");
                self.set_field(
                    "status",
                    (!planned).then(|| Value::String("planned".into())),
                );
                self.recheck(ctx);
            }
            Act::Field(name) => self.edit_field(name, ctx),
            Act::Note => {
                let current = self
                    .page
                    .and_then(|i| self.effects[i].get("note").map(show))
                    .unwrap_or_default();
                self.modal = Some((
                    Modal::text("Note", "Design note for this effect.", &current),
                    Pending::Note,
                ));
            }
            Act::RemoveEffect => {
                if let Some(i) = self.page.take() {
                    self.effects.remove(i);
                    self.state = RowsState::default();
                    self.recheck(ctx);
                }
            }
            Act::Done => {
                self.page = None;
                self.state = RowsState::default();
            }
            Act::Save => self.save(ctx),
            Act::Revert => {
                if let Some((n, d, e, fx)) = self.original.clone() {
                    self.name = n;
                    self.description = d;
                    self.enabled = e;
                    self.effects = fx;
                    self.recheck(ctx);
                }
            }
            Act::Reset => match ctx
                .service
                .reset_tag(&self.id)
                .and_then(|_| ctx.service.tag_detail(&self.id))
            {
                Ok(detail) => *self = TagEditor::open(ctx, Some(detail), None),
                Err(e) => self.message = Some((e, true)),
            },
            Act::Delete => {
                self.modal = Some((
                    Modal::confirm(
                        &format!(
                            "Delete the custom tag “{}”? This cannot be undone.",
                            self.name
                        ),
                        "Delete",
                    ),
                    Pending::ConfirmDelete,
                ));
            }
        }
        Nav::Stay
    }

    fn hints(&self) -> Vec<(&'static str, &'static str)> {
        if self.page.is_some() {
            vec![("↑↓", "move"), ("Enter", "edit"), ("Esc", "back to tag")]
        } else {
            vec![
                ("↑↓", "move"),
                ("Enter", "edit"),
                ("s", "save"),
                ("Esc", "back"),
            ]
        }
    }
}

/// The core's explanation of effect `i`, if the draft could be read.
impl TagEditor {
    fn explanation(&self, i: usize) -> Option<&Explanation> {
        self.insight.as_ref().and_then(|x| x.effects.get(i))
    }
}

/// An explanation as lines: its sentence, the roll step by step, and what
/// it affects.
pub fn explanation_lines(e: &Explanation, ctx: &Ctx) -> Vec<Line<'static>> {
    let s = &ctx.styles;
    let mut lines: Vec<Line<'static>> = wrap(&e.summary, 100).into_iter().map(Line::raw).collect();
    if !e.preview.is_empty() {
        lines.push(Line::styled(format!("{}.", e.preview.join(" → ")), s.key()));
    }
    if !e.affects.is_empty() {
        let names: Vec<&str> = e.affects.iter().map(|a| a.name.as_str()).collect();
        lines.push(Line::styled(
            format!("Affects: {}", names.join(", ")),
            s.dim(),
        ));
    }
    lines
}

/// `text` broken into lines of at most `width` characters, at spaces.
pub fn wrap(text: &str, width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut line = String::new();
    for word in text.split_whitespace() {
        if !line.is_empty() && line.chars().count() + 1 + word.chars().count() > width {
            lines.push(std::mem::take(&mut line));
        }
        if !line.is_empty() {
            line.push(' ');
        }
        line.push_str(word);
    }
    if !line.is_empty() {
        lines.push(line);
    }
    lines
}

/// The "Add effect" choices: each effect kind, followed by its presets
/// (e.g. Shift +1), as (kind index, preset index).
fn add_choices(schema: &[EffectKindSchema]) -> Vec<(usize, Option<usize>)> {
    schema
        .iter()
        .enumerate()
        .flat_map(|(i, k)| {
            std::iter::once((i, None)).chain((0..k.presets.len()).map(move |p| (i, Some(p))))
        })
        .collect()
}
