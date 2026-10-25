//! View Tables: the table list and a stepped table editor (basics, dice
//! coverage, results, variants, follow-up rolls, conditions). One generic
//! editor for every table.

use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use settlement_core::content::{
    Condition, FollowUpOutcome, FollowUpRoll, LabelVariant, RangeAssignment, SelectionMode, Table,
    TableResult, TableVariant,
};
use settlement_core::dice_assist::{DiceAdvice, DiceFit, EntryChangeKind, FitKind};
use settlement_core::engine::GenerationEvent;
use settlement_core::framework::layers::ItemKind;
use settlement_core::ids::{ResultId, TableId, TagId, VariantId};
use settlement_core::layers::{Origin, new_user_id};
use settlement_core::service::{AppService, ContentOverview, DraftCheck, TableDetail};
use settlement_core::table_editor::is_roll_again;

use super::common::{coverage_lines, issue_lines, parse_range, percent};
use super::condition::{ConditionEditor, Outcome, summary};
use super::open::{ItemRef, of_usage, open};
use super::tags::origin_label;
use super::test::{Target, TestScreen};
use crate::tui::widgets::{Modal, ModalResult, PickItem, Row, RowsState};
use crate::tui::{Ctx, Nav, Screen};

// ----- list ---------------------------------------------------------------------

enum ListPending {
    Filter,
    Restore(TableId),
    RestoreAll,
}

pub struct TablesScreen {
    overview: Option<ContentOverview>,
    state: RowsState,
    filter: String,
    error: Option<String>,
    modal: Option<(Modal, ListPending)>,
}

impl TablesScreen {
    pub fn new(ctx: &mut Ctx) -> Self {
        let mut s = TablesScreen {
            overview: None,
            state: RowsState::default(),
            filter: String::new(),
            error: None,
            modal: None,
        };
        s.resume(ctx);
        s
    }

    fn rows(&self, ctx: &Ctx) -> Vec<Row<(TableId, bool)>> {
        let s = &ctx.styles;
        let needle = self.filter.to_lowercase();
        let Some(o) = &self.overview else {
            return vec![];
        };
        o.tables
            .iter()
            .filter(|t| {
                needle.is_empty()
                    || t.name.to_lowercase().contains(&needle)
                    || t.id.as_str().to_lowercase().contains(&needle)
            })
            .map(|t| {
                let mut badges = vec![origin_label(t.origin, t.deleted)];
                if t.incomplete {
                    badges.push("incomplete");
                }
                if !t.active && !t.deleted {
                    badges.push("disabled");
                }
                let mut spans = vec![];
                if t.errors > 0 {
                    spans.push(Span::styled("! ", s.error()));
                }
                let style = if t.active && !t.deleted {
                    s.title()
                } else {
                    s.dim()
                };
                spans.push(Span::styled(format!("{:<34}", t.name), style));
                spans.push(Span::styled(format!("{:<38}", t.id.to_string()), s.dim()));
                spans.push(Span::styled(badges.join(" · "), s.dim()));
                Row::item(Line::from(spans), (t.id.clone(), t.deleted))
            })
            .collect()
    }
}

impl Screen for TablesScreen {
    fn title(&self) -> String {
        "Tables".into()
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
                "All tables".to_owned()
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
                    if let Err(e) = ctx.service.reset_table(&id) {
                        self.error = Some(e);
                    }
                }
                (ModalResult::Yes, ListPending::RestoreAll) => match ctx.service.reset_all() {
                    Ok(_) => ctx.flash = Some("All defaults restored".into()),
                    Err(e) => self.error = Some(e),
                },
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
                        Modal::confirm("Restore this deleted default table?", "Restore"),
                        ListPending::Restore(id),
                    ));
                    Nav::Stay
                }
                Some((id, false)) => match ctx.service.table_detail(&id) {
                    Ok(d) => Nav::Push(Box::new(TableEditor::open(ctx, Some(d), None))),
                    Err(e) => {
                        self.error = Some(e);
                        Nav::Stay
                    }
                },
                None => Nav::Stay,
            },
            KeyCode::Char('n') => Nav::Push(Box::new(TableEditor::open(
                ctx,
                None,
                Some(AppService::new_table()),
            ))),
            // Create from existing: a copy of the selected table.
            KeyCode::Char('c') => match self.state.current(&rows).cloned() {
                Some((id, false)) => match ctx.service.copy_table(&id) {
                    Ok(copy) => Nav::Push(Box::new(TableEditor::open(ctx, None, Some(copy)))),
                    Err(e) => {
                        self.error = Some(e);
                        Nav::Stay
                    }
                },
                _ => Nav::Stay,
            },
            KeyCode::Char('R') => {
                self.modal = Some((
                    Modal::confirm(
                        "Restore all tables and tags to the installed defaults? Every customisation and custom item is removed.",
                        "Restore all",
                    ),
                    ListPending::RestoreAll,
                ));
                Nav::Stay
            }
            KeyCode::Char('/') => {
                self.modal = Some((
                    Modal::text(
                        "Filter tables",
                        "Name or ID; empty shows all.",
                        &self.filter,
                    ),
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
            ("n", "new table"),
            ("c", "copy"),
            ("R", "restore all defaults"),
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

#[derive(Clone, Copy, Debug, PartialEq)]
enum Page {
    Main,
    Result(usize),
    Variant(usize),
    FollowUp(usize),
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum CondTarget {
    Table,
    Result(usize),
    Variant(usize),
    FollowUp(usize),
}

#[derive(Clone, Debug, PartialEq)]
enum Act {
    Name,
    Description,
    Dice,
    Selection,
    Active,
    Condition(CondTarget),
    Open(Page),
    AddResult,
    AddVariant,
    AddFollowUp,
    Save,
    Revert,
    Reset,
    Delete,
    // Simple view
    Mode,
    UseFit(usize),
    AddEntries(u32),
    Preview,
    Test,
    /// Open a dependent item.
    OpenItem(ItemRef),
    TestTotal,
    Advanced,
    RestoreDraft,
    DiscardDraft,
    Copy,
    // Result page
    ResultWeight,
    ResultShared,
    ResultThenRoll,
    MoveUp,
    MoveDown,
    Duplicate,
    ResultName,
    ResultDescription,
    ResultFaces,
    ResultValue,
    ResultTags,
    ResultEnabled,
    ResultUnique,
    ResultFallback,
    ResultVariants,
    RemoveResult,
    // Variant page
    VariantName,
    VariantDice,
    VariantOwnRanges,
    VariantRange(ResultId),
    RemoveVariant,
    // Follow-up page
    FollowName,
    FollowDice,
    FollowOn,
    Outcome(usize),
    AddOutcome,
    RemoveFollowUp,
    Done,
}

enum Pending {
    Text(Act),
    Tags(Vec<TagId>),
    OnResults(Vec<ResultId>),
    OutcomeMenu(usize),
    OutcomeFaces(usize),
    OutcomeBecomes(usize, Vec<Option<ResultId>>),
    Fallback(Vec<Option<ResultId>>),
    Shared(Vec<Option<String>>),
    ThenRoll(Vec<TableId>),
    ConfirmDelete,
    ConfirmLeave,
}

pub struct TableEditor {
    draft: Table,
    original: Table,
    detail: Option<TableDetail>,
    is_new: bool,
    check: Option<DraftCheck>,
    tags: Vec<(TagId, String)>,
    page: Page,
    condition: Option<(CondTarget, ConditionEditor)>,
    state: RowsState,
    modal: Option<(Modal, Pending)>,
    message: Option<(String, bool)>,
    /// Equal or custom chances (custom shows each result's relative chance).
    equal: bool,
    advanced: bool,
    undo: Vec<Table>,
    redo: Vec<Table>,
    /// A draft kept from an earlier session, offered for restoring.
    kept: Option<Table>,
    /// The last preview roll, as lines.
    preview: Vec<String>,
    /// The key just handled was an undo or redo.
    stepped: bool,
}

/// Sides of a single die (`1d10` → 10), or `None` for sums of dice.
fn single_die(dice: Option<&str>) -> Option<u32> {
    let d = dice?.trim().to_lowercase();
    let rest = d.strip_prefix("1d").or_else(|| d.strip_prefix('d'))?;
    rest.parse().ok()
}

/// The results that take part in the chances: enabled, not "Roll again".
fn entries(table: &Table) -> Vec<&TableResult> {
    table
        .results
        .iter()
        .filter(|r| r.enabled && !is_roll_again(r))
        .collect()
}

/// Each entry's relative chance: its faces on a single die (else 1).
fn weights(table: &Table) -> Vec<u32> {
    let single = single_die(table.dice.as_deref()).is_some();
    entries(table)
        .iter()
        .map(|r| match (single, r.range) {
            (true, Some(range)) => u32::try_from(range.max - range.min + 1).unwrap_or(1).max(1),
            _ => 1,
        })
        .collect()
}

impl TableEditor {
    pub(crate) fn open(ctx: &mut Ctx, detail: Option<TableDetail>, new: Option<Table>) -> Self {
        let is_new = new.is_some();
        let table = new.unwrap_or_else(|| detail.as_ref().expect("detail or new").table.clone());
        let tags = ctx
            .service
            .overview()
            .map(|o| {
                o.tags
                    .into_iter()
                    .filter(|t| !t.deleted)
                    .map(|t| (t.id, t.name))
                    .collect()
            })
            .unwrap_or_default();
        let mut editor = TableEditor {
            original: table.clone(),
            draft: table,
            detail,
            is_new,
            check: None,
            tags,
            page: Page::Main,
            condition: None,
            state: RowsState::default(),
            modal: None,
            message: None,
            equal: false,
            advanced: false,
            undo: Vec::new(),
            redo: Vec::new(),
            kept: None,
            preview: Vec::new(),
            stepped: false,
        };
        let w = weights(&editor.draft);
        editor.equal = w.windows(2).all(|p| p[0] == p[1]);
        editor.kept = ctx
            .service
            .table_draft(&editor.draft.id)
            .ok()
            .flatten()
            .filter(|kept| kept != &editor.draft);
        editor.recheck(ctx);
        editor
    }

    /// Lays the entries out on the table's dice with `weights` (equal ones
    /// in Equal mode), when the dice are one die.
    fn relayout(&mut self, given: Option<Vec<u32>>) {
        let Some(dice) = self.draft.dice.clone() else {
            return;
        };
        if self.draft.selection != SelectionMode::Roll || single_die(Some(&dice)).is_none() {
            return;
        }
        let w = given.unwrap_or_else(|| {
            if self.equal {
                vec![1; entries(&self.draft).len()]
            } else {
                weights(&self.draft)
            }
        });
        if w.is_empty() {
            return;
        }
        match AppService::dice_fit(&w, &dice) {
            Ok(fit) => self.apply_fit(&fit),
            Err(e) => {
                self.message = Some((
                    format!("{e} The Dice Assistant suggests dice that can."),
                    true,
                ))
            }
        }
    }

    fn apply_fit(&mut self, fit: &DiceFit) {
        let ids: Vec<ResultId> = entries(&self.draft).iter().map(|r| r.id.clone()).collect();
        match AppService::apply_dice_fit(&self.draft, &ids, fit) {
            Ok(table) => {
                self.draft = table;
                if fit.kind == FitKind::Approximate {
                    self.message = Some((fit.explanation.clone(), false));
                }
            }
            Err(e) => self.message = Some((e, true)),
        }
    }

    fn advice(&self) -> DiceAdvice {
        let w = if self.equal {
            vec![1; entries(&self.draft).len()]
        } else {
            weights(&self.draft)
        };
        AppService::dice_advice(&w)
    }

    /// Plain-English problems with the current faces.
    fn face_problems(&self) -> Vec<String> {
        let Some(base) = self.check.as_ref().and_then(|c| {
            c.coverage
                .iter()
                .find(|c| c.variant.is_none() && c.follow_up.is_none())
        }) else {
            return Vec::new();
        };
        let faces = |r: &settlement_core::content::FaceRange| {
            if r.min == r.max {
                format!("face {}", r.min)
            } else {
                format!("faces {}–{}", r.min, r.max)
            }
        };
        let mut out = Vec::new();
        if let Some(e) = &base.dice_error {
            out.push(format!("The dice cannot be read: {e}."));
        }
        for r in &base.uncovered {
            out.push(format!("No result is on {}.", faces(r)));
        }
        for r in &base.overlapping {
            out.push(format!("More than one result is on {}.", faces(r)));
        }
        for r in &base.impossible {
            out.push(format!(
                "{} is on {}, which {} cannot roll.",
                self.result_name(&ResultId::new(r.result.as_str())),
                faces(&r.range),
                base.dice
            ));
        }
        out
    }

    /// What a typed dice total gives, from the current faces.
    fn test_total(&self, text: &str) -> String {
        let Some(base) = self.check.as_ref().and_then(|c| {
            c.coverage
                .iter()
                .find(|c| c.variant.is_none() && c.follow_up.is_none())
        }) else {
            return "The table has no dice to test.".into();
        };
        let Some(face) = text
            .trim()
            .parse::<i32>()
            .ok()
            .and_then(|t| base.faces.iter().find(|f| f.face == t))
        else {
            return format!("{} cannot roll {}.", base.dice, text.trim());
        };
        let name = |id: &String| self.result_name(&ResultId::new(id.as_str()));
        match face.claimed_by.as_slice() {
            [] => format!("No result is on {}.", face.face),
            [one] => format!(
                "{} gives {} ({} to roll exactly {}).",
                face.face,
                name(one),
                percent(face.chance),
                face.face
            ),
            many => format!(
                "{} is claimed by {}: only one result can have a face.",
                face.face,
                many.iter().map(name).collect::<Vec<_>>().join(" and ")
            ),
        }
    }

    /// The preview roll as lines.
    fn preview_lines(events: &[GenerationEvent]) -> Vec<String> {
        events
            .iter()
            .map(|e| match e {
                GenerationEvent::TableRolled(r) => {
                    let mut line = format!(
                        "{}: rolled {}",
                        r.table.name,
                        r.dice.clone().unwrap_or_default()
                    );
                    for a in r.attempts.iter().take(r.attempts.len().saturating_sub(1)) {
                        line.push_str(&format!(" {} → {} (rolled again),", a.roll, a.result.name));
                    }
                    if let Some(a) = r.accepted() {
                        line.push_str(&format!(" {} →", a.roll));
                    }
                    line.push_str(&format!(" {}", r.result.name));
                    line
                }
                GenerationEvent::TableSkipped { table, .. } => format!("{}: skipped", table.name),
                GenerationEvent::SecondaryRoll { label, roll, .. } => {
                    format!("{label}: {}", roll.total)
                }
            })
            .collect()
    }

    fn dirty(&self) -> bool {
        self.is_new || self.draft != self.original
    }

    fn built_in(&self) -> bool {
        self.detail.as_ref().is_some_and(|d| d.built_in)
    }

    fn recheck(&mut self, ctx: &mut Ctx) {
        // A failure to check (not a validation problem) is shown, not hidden.
        self.check = ctx
            .service
            .check_table(self.draft.clone())
            .map_err(|e| ctx.flash = Some(e))
            .ok();
    }

    fn can_save(&self) -> bool {
        self.dirty() && self.check.as_ref().is_some_and(DraftCheck::is_acceptable)
    }

    fn tag_names(&self, ids: &[TagId]) -> String {
        ids.iter()
            .map(|id| {
                self.tags
                    .iter()
                    .find(|(t, _)| t == id)
                    .map_or_else(|| id.to_string(), |(_, n)| n.clone())
            })
            .collect::<Vec<_>>()
            .join(", ")
    }

    fn result_name(&self, id: &ResultId) -> String {
        self.draft
            .result(id)
            .map_or_else(|| id.to_string(), |r| r.name.clone())
    }

    fn rows(&self, ctx: &Ctx, width: u16) -> Vec<Row<Act>> {
        let s = &ctx.styles;
        let field = |label: &str, value: String| {
            Line::from(vec![
                Span::raw(format!("{label:<22}")),
                Span::styled(value, s.title()),
            ])
        };
        let mut rows = Vec::new();
        let rolled = self.draft.selection == SelectionMode::Roll;
        match self.page {
            Page::Result(i) => {
                let r = &self.draft.results[i];
                rows.push(Row::info(Line::styled(
                    format!("RESULT · {}", r.id),
                    s.dim(),
                )));
                rows.push(Row::item(field("Name", r.name.clone()), Act::ResultName));
                rows.push(Row::item(
                    field(
                        "Description",
                        if r.description.is_empty() {
                            "—".into()
                        } else {
                            r.description.clone()
                        },
                    ),
                    Act::ResultDescription,
                ));
                if rolled {
                    rows.push(Row::item(
                        field(
                            "Faces",
                            r.range
                                .map_or("none (variants only)".into(), |x| x.to_string()),
                        ),
                        Act::ResultFaces,
                    ));
                    let index = entries(&self.draft).iter().position(|e| e.id == r.id);
                    if let (false, Some(index), Some(_)) =
                        (self.equal, index, single_die(self.draft.dice.as_deref()))
                    {
                        rows.push(Row::item(
                            field("Relative chance", weights(&self.draft)[index].to_string()),
                            Act::ResultWeight,
                        ));
                    }
                }
                rows.push(Row::item(
                    field(
                        "Shared result",
                        r.shared
                            .as_ref()
                            .map_or("— its own —".into(), ToString::to_string),
                    ),
                    Act::ResultShared,
                ));
                rows.push(Row::item(
                    field(
                        "Then roll",
                        if r.then_roll.is_empty() {
                            "—".into()
                        } else {
                            r.then_roll
                                .iter()
                                .map(ToString::to_string)
                                .collect::<Vec<_>>()
                                .join(", ")
                        },
                    ),
                    Act::ResultThenRoll,
                ));
                // Tables whose results stand for numbers (e.g. Authority
                // Size) show them.
                if self.draft.results.iter().any(|r| r.value.is_some()) {
                    rows.push(Row::item(
                        field("Value", r.value.map_or("—".into(), |v| v.to_string())),
                        Act::ResultValue,
                    ));
                }
                rows.push(Row::item(
                    field(
                        "Emits tags",
                        if r.tags.is_empty() {
                            "—".into()
                        } else {
                            self.tag_names(&r.tags)
                        },
                    ),
                    Act::ResultTags,
                ));
                rows.push(Row::item(
                    field("Eligible when", summary(&r.eligibility, &self.tags)),
                    Act::Condition(CondTarget::Result(i)),
                ));
                rows.push(Row::item(
                    field(
                        "Enabled",
                        if r.enabled { "On".into() } else { "Off".into() },
                    ),
                    Act::ResultEnabled,
                ));
                // Role allocation: unique results and their fallbacks.
                rows.push(Row::item(
                    field(
                        "Unique",
                        if r.unique {
                            "Yes".into()
                        } else {
                            "No (repeatable)".into()
                        },
                    ),
                    Act::ResultUnique,
                ));
                if r.unique {
                    let fallback = r
                        .fallback
                        .as_ref()
                        .and_then(|f| self.draft.result(f))
                        .map_or("— (required)".into(), |f| f.name.clone());
                    rows.push(Row::item(field("Fallback", fallback), Act::ResultFallback));
                }
                rows.push(Row::item(
                    field(
                        "Display variants",
                        if r.label_variants.is_empty() {
                            "—".into()
                        } else {
                            variants_text(&r.label_variants)
                        },
                    ),
                    Act::ResultVariants,
                ));
                rows.push(Row::blank());
                rows.push(Row::item(Line::raw("Move up"), Act::MoveUp));
                rows.push(Row::item(Line::raw("Move down"), Act::MoveDown));
                rows.push(Row::item(Line::raw("Duplicate"), Act::Duplicate));
                rows.push(Row::item(
                    Line::styled("Remove this result", s.error()),
                    Act::RemoveResult,
                ));
                if let Some((m, error)) = &self.message {
                    rows.push(Row::info(Line::styled(
                        m.clone(),
                        if *error { s.error() } else { s.good() },
                    )));
                }
                rows.push(Row::item(Line::raw("‹ Done"), Act::Done));
            }
            Page::Variant(i) => {
                let v = &self.draft.variants[i];
                rows.push(Row::info(Line::styled(
                    format!("VARIANT · {}", v.id),
                    s.dim(),
                )));
                rows.push(Row::item(field("Name", v.name.clone()), Act::VariantName));
                rows.push(Row::item(
                    field(
                        "Dice",
                        v.dice.clone().unwrap_or_else(|| {
                            format!(
                                "same as table ({})",
                                self.draft.dice.clone().unwrap_or_default()
                            )
                        }),
                    ),
                    Act::VariantDice,
                ));
                rows.push(Row::item(
                    field("Used when", summary(&v.when, &self.tags)),
                    Act::Condition(CondTarget::Variant(i)),
                ));
                rows.push(Row::item(
                    field(
                        "Own ranges",
                        if v.ranges.is_some() {
                            "Yes".into()
                        } else {
                            "No (table's ranges)".into()
                        },
                    ),
                    Act::VariantOwnRanges,
                ));
                if let Some(ranges) = &v.ranges {
                    for r in &self.draft.results {
                        let current = ranges
                            .iter()
                            .find(|a| a.result == r.id)
                            .map_or("—".into(), |a| a.range.to_string());
                        rows.push(Row::item(
                            field(&format!("  {}", r.name), current),
                            Act::VariantRange(r.id.clone()),
                        ));
                    }
                }
                self.push_coverage(&mut rows, ctx, width, |c| c.variant.as_ref() == Some(&v.id));
                rows.push(Row::blank());
                rows.push(Row::item(
                    Line::styled("Remove this variant", s.error()),
                    Act::RemoveVariant,
                ));
                rows.push(Row::item(Line::raw("‹ Done"), Act::Done));
            }
            Page::FollowUp(i) => {
                let fu = &self.draft.follow_ups[i];
                rows.push(Row::info(Line::styled(
                    format!("FOLLOW-UP ROLL · {}", fu.id),
                    s.dim(),
                )));
                rows.push(Row::item(field("Name", fu.name.clone()), Act::FollowName));
                rows.push(Row::item(field("Dice", fu.dice.clone()), Act::FollowDice));
                rows.push(Row::item(
                    field("Only when", summary(&fu.when, &self.tags)),
                    Act::Condition(CondTarget::FollowUp(i)),
                ));
                rows.push(Row::item(
                    field(
                        "After results",
                        if fu.on_results.is_empty() {
                            "—".into()
                        } else {
                            fu.on_results
                                .iter()
                                .map(|r| self.result_name(r))
                                .collect::<Vec<_>>()
                                .join(", ")
                        },
                    ),
                    Act::FollowOn,
                ));
                rows.push(Row::info(Line::styled("OUTCOMES", s.dim())));
                for (k, o) in fu.outcomes.iter().enumerate() {
                    rows.push(Row::item(
                        Line::raw(format!(
                            "  {:>7}  →  {}",
                            o.range.to_string(),
                            o.becomes
                                .as_ref()
                                .map_or("keep the original".into(), |b| format!(
                                    "change to {}",
                                    self.result_name(b)
                                ))
                        )),
                        Act::Outcome(k),
                    ));
                }
                rows.push(Row::item(Line::raw("  + Add outcome"), Act::AddOutcome));
                self.push_coverage(&mut rows, ctx, width, |c| {
                    c.follow_up.as_deref() == Some(fu.id.as_str())
                });
                rows.push(Row::blank());
                rows.push(Row::item(
                    Line::styled("Remove this follow-up roll", s.error()),
                    Act::RemoveFollowUp,
                ));
                rows.push(Row::item(Line::raw("‹ Done"), Act::Done));
            }
            Page::Main => {
                let origin = if self.is_new {
                    "new custom table (not saved)"
                } else {
                    origin_label(self.detail.as_ref().and_then(|d| d.origin), false)
                };
                rows.push(Row::info(Line::from(vec![
                    Span::styled("ID ", s.dim()),
                    Span::raw(self.draft.id.to_string()),
                    Span::styled(format!(" · {origin}"), s.dim()),
                    Span::styled(
                        if self.dirty() {
                            " · unsaved changes kept as a draft"
                        } else {
                            ""
                        },
                        s.dim(),
                    ),
                ])));
                if self.kept.is_some() {
                    rows.push(Row::info(Line::styled(
                        "You have unsaved changes to this table from earlier.",
                        ratatui::style::Style::new().fg(s.accent),
                    )));
                    rows.push(Row::item(Line::raw("  Restore them"), Act::RestoreDraft));
                    rows.push(Row::item(Line::raw("  Discard them"), Act::DiscardDraft));
                }
                let issues = self
                    .check
                    .as_ref()
                    .map(|c| c.issues.as_slice())
                    .unwrap_or_default();
                let errors = issues
                    .iter()
                    .filter(|i| i.severity == settlement_core::validation::Severity::Error)
                    .count()
                    + usize::from(self.check.as_ref().is_some_and(|c| c.error.is_some()));
                let warnings = issues.len()
                    + usize::from(self.check.as_ref().is_some_and(|c| c.error.is_some()))
                    - errors;
                rows.push(Row::info(if errors == 0 && warnings == 0 {
                    Line::styled("No problems: this table can be saved.", s.good())
                } else {
                    Line::styled(
                        format!(
                            "{errors} problem(s) to fix before saving · {warnings} warning(s) (see Validation)"
                        ),
                        if errors > 0 { s.error() } else { s.dim() },
                    )
                }));
                if self.draft.status == settlement_core::content::ContentStatus::Incomplete {
                    rows.push(Row::info(Line::styled(
                        format!("Marked incomplete: {}", self.draft.status_note),
                        ratatui::style::Style::new().fg(s.accent),
                    )));
                }
                rows.push(Row::blank());
                rows.push(Row::item(
                    field("Table name", self.draft.name.clone()),
                    Act::Name,
                ));
                if rolled {
                    rows.push(Row::item(
                        field(
                            "Chance mode",
                            if self.equal {
                                "Equal (every result equally likely)".into()
                            } else {
                                "Custom (each result's relative chance)".into()
                            },
                        ),
                        Act::Mode,
                    ));
                    rows.push(Row::item(
                        field("Dice", self.draft.dice.clone().unwrap_or("—".into())),
                        Act::Dice,
                    ));
                    rows.push(Row::blank());
                    rows.push(Row::info(Line::styled("DICE ASSISTANT", s.dim())));
                    let problems = self.face_problems();
                    if problems.is_empty() && self.check.is_some() {
                        rows.push(Row::info(Line::styled(
                            "  Every face is covered exactly once",
                            s.good(),
                        )));
                    }
                    for p in problems {
                        rows.push(Row::info(Line::styled(format!("  {p}"), s.error())));
                    }
                    let advice = self.advice();
                    rows.push(Row::info(Line::raw(format!("  {}", advice.summary))));
                    for (k, fit) in advice.suggestions.iter().take(4).enumerate() {
                        rows.push(Row::item(
                            Line::from(vec![
                                Span::styled(format!("  • Use {}", fit.label), s.title()),
                                Span::styled(format!("  {}", fit.explanation), s.dim()),
                            ]),
                            Act::UseFit(k),
                        ));
                    }
                    for change in advice.entry_changes {
                        let line = Line::styled(format!("  {}", change.explanation), s.dim());
                        if change.kind == EntryChangeKind::Add {
                            rows.push(Row::item(line, Act::AddEntries(change.count)));
                        } else {
                            rows.push(Row::info(line));
                        }
                    }
                }
                if self.advanced {
                    rows.push(Row::blank());
                    rows.push(Row::info(Line::styled("ADVANCED", s.dim())));
                    rows.push(Row::item(
                        field(
                            "Description",
                            if self.draft.description.is_empty() {
                                "—".into()
                            } else {
                                self.draft.description.clone()
                            },
                        ),
                        Act::Description,
                    ));
                    let selection = if rolled {
                        "rolled with dice"
                    } else {
                        "always chosen"
                    };
                    if self.built_in() {
                        rows.push(Row::info(
                            field("Results are", selection.into()).style(s.dim()),
                        ));
                    } else {
                        rows.push(Row::item(
                            field("Results are", selection.into()),
                            Act::Selection,
                        ));
                    }
                    rows.push(Row::item(
                        field(
                            "Active",
                            if self.draft.active {
                                "On".into()
                            } else {
                                "Off".into()
                            },
                        ),
                        Act::Active,
                    ));
                    rows.push(Row::item(
                        field("Rolled when", summary(&self.draft.eligibility, &self.tags)),
                        Act::Condition(CondTarget::Table),
                    ));
                    if rolled {
                        self.push_coverage(&mut rows, ctx, width, |c| {
                            c.variant.is_none() && c.follow_up.is_none()
                        });
                    }
                    let usages = self
                        .detail
                        .as_ref()
                        .map(|d| d.usages.clone())
                        .unwrap_or_default();
                    if usages.is_empty() {
                        rows.push(Row::info(Line::styled(
                            "Used by: nothing, so it can be deleted",
                            s.dim(),
                        )));
                    } else {
                        rows.push(Row::info(Line::styled("Used by:", s.dim())));
                        for u in &usages {
                            rows.push(match of_usage(u) {
                                Some(item) => Row::item(
                                    Line::raw(format!("  {}  ›", u.description)),
                                    Act::OpenItem(item),
                                ),
                                None => {
                                    Row::info(Line::styled(format!("  {}", u.description), s.dim()))
                                }
                            });
                        }
                    }
                }
                rows.push(Row::blank());
                rows.push(Row::info(Line::styled("RESULTS", s.dim())));
                let base = self.check.as_ref().and_then(|c| {
                    c.coverage
                        .iter()
                        .find(|c| c.variant.is_none() && c.follow_up.is_none())
                });
                for (i, r) in self.draft.results.iter().enumerate() {
                    let chance = base
                        .and_then(|b| b.results.iter().find(|x| x.result == r.id.as_str()))
                        .map_or(String::new(), |x| percent(x.chance));
                    let mut spans = vec![
                        Span::styled(
                            format!("  {:>7} ", r.range.map_or("—".into(), |x| x.to_string())),
                            s.dim(),
                        ),
                        Span::styled(format!("{chance:>6}  "), s.dim()),
                        Span::styled(r.name.clone(), if r.enabled { s.title() } else { s.dim() }),
                    ];
                    if !r.tags.is_empty() {
                        spans.push(Span::styled(
                            format!("   {}", self.tag_names(&r.tags)),
                            s.dim(),
                        ));
                    }
                    if r.eligibility.is_some() {
                        spans.push(Span::styled("   (conditional)", s.dim()));
                    }
                    rows.push(Row::item(Line::from(spans), Act::Open(Page::Result(i))));
                }
                rows.push(Row::item(Line::raw("  + Add result"), Act::AddResult));
                if rolled {
                    rows.push(Row::blank());
                    rows.push(Row::item(Line::raw("Preview roll"), Act::Preview));
                    rows.push(Row::item(
                        Line::raw("Test this table (Run 100 / 1,000 / 10,000)  ›"),
                        Act::Test,
                    ));
                    for line in &self.preview {
                        rows.push(Row::info(Line::styled(format!("  {line}"), s.title())));
                    }
                    rows.push(Row::item(Line::raw("Test a dice total…"), Act::TestTotal));
                }
                rows.push(Row::blank());
                rows.push(Row::item(
                    Line::styled(
                        if self.advanced {
                            "Advanced (hide)"
                        } else {
                            "Advanced (show)"
                        },
                        s.heading(),
                    ),
                    Act::Advanced,
                ));
                if rolled && self.advanced {
                    rows.push(Row::blank());
                    rows.push(Row::info(Line::styled("TAG-DRIVEN VARIANTS", s.dim())));
                    for (i, v) in self.draft.variants.iter().enumerate() {
                        rows.push(Row::item(
                            Line::raw(format!(
                                "  {}  — when {}",
                                v.name,
                                summary(&v.when, &self.tags)
                            )),
                            Act::Open(Page::Variant(i)),
                        ));
                    }
                    rows.push(Row::item(Line::raw("  + Add variant"), Act::AddVariant));
                    rows.push(Row::blank());
                    rows.push(Row::info(Line::styled("FOLLOW-UP ROLLS", s.dim())));
                    for (i, fu) in self.draft.follow_ups.iter().enumerate() {
                        rows.push(Row::item(
                            Line::raw(format!(
                                "  {}  — {} after {} result(s)",
                                fu.name,
                                fu.dice,
                                fu.on_results.len()
                            )),
                            Act::Open(Page::FollowUp(i)),
                        ));
                    }
                    rows.push(Row::item(
                        Line::raw("  + Add follow-up roll"),
                        Act::AddFollowUp,
                    ));
                }
                rows.push(Row::blank());
                rows.push(Row::info(Line::styled("VALIDATION", s.dim())));
                let issues = self
                    .check
                    .as_ref()
                    .map(|c| c.issues.clone())
                    .unwrap_or_default();
                let error = self.check.as_ref().and_then(|c| c.error.clone());
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
                let modified =
                    self.detail.as_ref().and_then(|d| d.origin) == Some(Origin::Modified);
                if self.built_in() && modified {
                    rows.push(Row::item(Line::raw("Reset to default"), Act::Reset));
                }
                if !self.is_new {
                    rows.push(Row::item(Line::raw("Copy to a new table"), Act::Copy));
                }
                if !self.built_in() && !self.is_new {
                    rows.push(Row::item(Line::styled("Delete", s.error()), Act::Delete));
                }
                if let Some((m, error)) = &self.message {
                    rows.push(Row::info(Line::styled(
                        m.clone(),
                        if *error { s.error() } else { s.good() },
                    )));
                }
            }
        }
        rows
    }

    fn push_coverage(
        &self,
        rows: &mut Vec<Row<Act>>,
        ctx: &Ctx,
        width: u16,
        which: impl Fn(&settlement_core::editor::Coverage) -> bool,
    ) {
        let Some(check) = &self.check else { return };
        let name_of = |id: &str| self.result_name(&ResultId::new(id));
        for c in check.coverage.iter().filter(|c| which(c)) {
            rows.push(Row::blank());
            for line in coverage_lines(c, &name_of, width.saturating_sub(4), &ctx.styles) {
                rows.push(Row::info(line));
            }
        }
    }

    fn text(&mut self, title: &str, help: &str, value: &str, act: Act) {
        self.modal = Some((Modal::text(title, help, value), Pending::Text(act)));
    }

    fn apply_text(&mut self, act: Act, value: String) -> Result<(), String> {
        let t = value.trim().to_owned();
        match act {
            Act::Name => self.draft.name = value,
            Act::Description => self.draft.description = value,
            Act::Dice => {
                self.draft.dice = (!t.is_empty()).then_some(t);
                self.relayout(None);
            }
            Act::TestTotal => {
                self.message = Some((self.test_total(&t), false));
            }
            Act::ResultWeight => {
                if let Page::Result(i) = self.page {
                    let w: u32 = t
                        .parse()
                        .ok()
                        .filter(|w| *w > 0)
                        .ok_or_else(|| "Enter a whole number of 1 or more.".to_owned())?;
                    let id = self.draft.results[i].id.clone();
                    let mut all = weights(&self.draft);
                    if let Some(index) = entries(&self.draft).iter().position(|e| e.id == id) {
                        all[index] = w;
                        self.relayout(Some(all));
                    }
                }
            }
            Act::ResultName => {
                if let Page::Result(i) = self.page {
                    self.draft.results[i].name = value;
                }
            }
            Act::ResultDescription => {
                if let Page::Result(i) = self.page {
                    self.draft.results[i].description = value;
                }
            }
            Act::ResultFaces => {
                if let Page::Result(i) = self.page {
                    self.draft.results[i].range = parse_range(&t)?;
                }
            }
            Act::ResultVariants => {
                if let Page::Result(i) = self.page {
                    self.draft.results[i].label_variants = parse_variants(&t)?;
                }
            }
            Act::ResultValue => {
                if let Page::Result(i) = self.page {
                    self.draft.results[i].value = if t.is_empty() {
                        None
                    } else {
                        Some(
                            t.parse()
                                .map_err(|_| format!("“{t}” is not a whole number."))?,
                        )
                    };
                }
            }
            Act::VariantName => {
                if let Page::Variant(i) = self.page {
                    self.draft.variants[i].name = value;
                }
            }
            Act::VariantDice => {
                if let Page::Variant(i) = self.page {
                    self.draft.variants[i].dice = (!t.is_empty()).then_some(t);
                }
            }
            Act::VariantRange(result) => {
                if let Page::Variant(i) = self.page {
                    let range = parse_range(&t)?;
                    let ranges = self.draft.variants[i].ranges.get_or_insert_with(Vec::new);
                    ranges.retain(|a| a.result != result);
                    if let Some(range) = range {
                        ranges.push(RangeAssignment { result, range });
                        let order: Vec<ResultId> =
                            self.draft.results.iter().map(|r| r.id.clone()).collect();
                        ranges.sort_by_key(|a| order.iter().position(|r| *r == a.result));
                    }
                }
            }
            Act::FollowName => {
                if let Page::FollowUp(i) = self.page {
                    self.draft.follow_ups[i].name = value;
                }
            }
            Act::FollowDice => {
                if let Page::FollowUp(i) = self.page {
                    self.draft.follow_ups[i].dice = t;
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn handle_modal(&mut self, key: KeyEvent, ctx: &mut Ctx) -> Option<Nav> {
        let (modal, pending) = self.modal.as_mut()?;
        let result = modal.handle(key, &ctx.styles);
        if matches!(result, ModalResult::Open) {
            return Some(Nav::Stay);
        }
        // Text edits validate here and keep the dialog open on bad input.
        if let (ModalResult::Text(value), Pending::Text(act)) = (&result, &*pending) {
            let act = act.clone();
            if let Err(e) = self.apply_text(act, value.clone()) {
                if let Some((m, _)) = &mut self.modal {
                    m.set_error(e);
                }
                return Some(Nav::Stay);
            }
            self.modal = None;
            self.recheck(ctx);
            return Some(Nav::Stay);
        }
        let pending = std::mem::replace(pending, Pending::ConfirmLeave);
        self.modal = None;
        match (result, pending) {
            (ModalResult::Checked(checked), Pending::Tags(ids)) => {
                if let Page::Result(i) = self.page {
                    self.draft.results[i].tags = ids
                        .into_iter()
                        .zip(checked)
                        .filter(|(_, c)| *c)
                        .map(|(id, _)| id)
                        .collect();
                }
            }
            (ModalResult::Checked(checked), Pending::OnResults(ids)) => {
                if let Page::FollowUp(i) = self.page {
                    self.draft.follow_ups[i].on_results = ids
                        .into_iter()
                        .zip(checked)
                        .filter(|(_, c)| *c)
                        .map(|(id, _)| id)
                        .collect();
                }
            }
            (ModalResult::Picked(choice), Pending::OutcomeMenu(k)) => match choice {
                0 => {
                    let current = self
                        .follow_up_outcome(k)
                        .map(|o| o.range.to_string())
                        .unwrap_or_default();
                    self.modal = Some((
                        Modal::text("Outcome faces", "A number, or two like 1-3.", &current),
                        Pending::OutcomeFaces(k),
                    ));
                }
                1 => {
                    let mut ids = vec![None];
                    let mut items = vec![PickItem::new("Keep the original result")];
                    for r in &self.draft.results {
                        ids.push(Some(r.id.clone()));
                        items.push(PickItem::new(format!("Change to {}", r.name)));
                    }
                    self.modal = Some((
                        Modal::pick("Outcome result", items, 0),
                        Pending::OutcomeBecomes(k, ids),
                    ));
                }
                _ => {
                    if let Page::FollowUp(i) = self.page {
                        self.draft.follow_ups[i].outcomes.remove(k);
                    }
                }
            },
            (ModalResult::Text(t), Pending::OutcomeFaces(k)) => match parse_range(&t) {
                Ok(Some(range)) => {
                    if let Page::FollowUp(i) = self.page {
                        self.draft.follow_ups[i].outcomes[k].range = range;
                    }
                }
                _ => self.message = Some(("Enter faces like 4 or 1-3.".into(), true)),
            },
            (ModalResult::Picked(choice), Pending::Shared(ids)) => {
                if let Page::Result(i) = self.page {
                    let r = &mut self.draft.results[i];
                    match ids[choice].clone() {
                        // Linking clears what the shared result supplies.
                        Some(id) => {
                            r.shared = Some(id.as_str().into());
                            r.name.clear();
                            r.tags.clear();
                        }
                        None => r.shared = None,
                    }
                }
            }
            (ModalResult::Checked(checked), Pending::ThenRoll(ids)) => {
                if let Page::Result(i) = self.page {
                    self.draft.results[i].then_roll = ids
                        .into_iter()
                        .zip(checked)
                        .filter(|(_, c)| *c)
                        .map(|(id, _)| id)
                        .collect();
                }
            }
            (ModalResult::Picked(choice), Pending::Fallback(ids)) => {
                if let Page::Result(i) = self.page {
                    self.draft.results[i].fallback = ids[choice].clone();
                }
            }
            (ModalResult::Picked(choice), Pending::OutcomeBecomes(k, ids)) => {
                if let Page::FollowUp(i) = self.page {
                    self.draft.follow_ups[i].outcomes[k].becomes = ids[choice].clone();
                }
            }
            (ModalResult::Yes, Pending::ConfirmDelete) => {
                return Some(match ctx.service.delete_table(&self.draft.id) {
                    Ok(_) => {
                        ctx.flash = Some("Table deleted".into());
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

    fn follow_up_outcome(&self, k: usize) -> Option<&FollowUpOutcome> {
        match self.page {
            Page::FollowUp(i) => self.draft.follow_ups[i].outcomes.get(k),
            _ => None,
        }
    }

    fn condition_of(&mut self, target: CondTarget) -> &mut Option<Condition> {
        match target {
            CondTarget::Table => &mut self.draft.eligibility,
            CondTarget::Result(i) => &mut self.draft.results[i].eligibility,
            CondTarget::Variant(i) => &mut self.draft.variants[i].when,
            CondTarget::FollowUp(i) => &mut self.draft.follow_ups[i].when,
        }
    }

    fn save(&mut self, ctx: &mut Ctx) {
        match ctx.service.save_table(self.draft.clone()) {
            Ok(detail) => {
                self.draft = detail.table.clone();
                self.original = detail.table.clone();
                self.detail = Some(detail);
                self.is_new = false;
                self.message = Some(("Saved".into(), false));
                self.recheck(ctx);
            }
            Err(e) => self.message = Some((e, true)),
        }
    }

    fn act(&mut self, act: Act, ctx: &mut Ctx) -> Nav {
        self.message = None;
        match act.clone() {
            Act::Name => self.text(
                "Name",
                "Renaming is safe: references use the ID.",
                &self.draft.name.clone(),
                act,
            ),
            Act::Description => self.text("Description", "", &self.draft.description.clone(), act),
            Act::Dice => self.text(
                "Dice",
                "d4, d6, d8, d10, d12, d20, d100, or sums such as 2d6 or 1d8+1d4.",
                &self.draft.dice.clone().unwrap_or_default(),
                act,
            ),
            Act::Mode => {
                self.equal = !self.equal;
                if self.equal {
                    self.relayout(None);
                }
            }
            Act::UseFit(k) => {
                if let Some(fit) = self.advice().suggestions.get(k).cloned() {
                    self.apply_fit(&fit);
                }
            }
            Act::AddEntries(n) => {
                for _ in 0..n {
                    let mut r = blank_result();
                    r.name = format!("New result {}", self.draft.results.len() + 1);
                    self.draft.results.push(r);
                }
                self.relayout(None);
            }
            Act::Preview => match ctx.service.preview_roll(self.draft.clone()) {
                Ok(p) => {
                    self.preview = Self::preview_lines(&p.events);
                    self.preview.push(p.note);
                }
                Err(e) => self.message = Some((e, true)),
            },
            Act::TestTotal => {
                self.text(
                    "Test a dice total",
                    "Type what the dice showed to see which result it gives.",
                    "",
                    act,
                );
                return Nav::Stay;
            }
            Act::Advanced => self.advanced = !self.advanced,
            Act::RestoreDraft => {
                if let Some(kept) = self.kept.take() {
                    self.draft = kept;
                }
            }
            Act::DiscardDraft => {
                self.kept = None;
                let _ = ctx.service.discard_table_draft(&self.draft.id);
            }
            Act::OpenItem(item) => return open(ctx, &item),
            Act::Test => {
                return Nav::Push(Box::new(TestScreen::new(Target::Table(self.draft.clone()))));
            }
            Act::Copy => match ctx.service.copy_table(&self.draft.id) {
                Ok(copy) => {
                    *self = TableEditor::open(ctx, None, Some(copy));
                    return Nav::Stay;
                }
                Err(e) => self.message = Some((e, true)),
            },
            Act::ResultWeight => {
                if let Page::Result(i) = self.page {
                    let id = self.draft.results[i].id.clone();
                    let current = entries(&self.draft)
                        .iter()
                        .position(|e| e.id == id)
                        .map(|k| weights(&self.draft)[k].to_string())
                        .unwrap_or_default();
                    self.text(
                        "Relative chance",
                        "2 is twice as likely as 1. The faces are laid out again.",
                        &current,
                        act,
                    );
                    return Nav::Stay;
                }
            }
            Act::ResultShared => {
                let shared: Vec<(String, String)> = ctx
                    .service
                    .framework_items()
                    .unwrap_or_default()
                    .into_iter()
                    .filter(|i| i.kind == ItemKind::SharedResult && !i.deleted)
                    .map(|i| (i.id, i.name))
                    .collect();
                let mut ids = vec![None];
                let mut items = vec![PickItem::new("— its own —")];
                for (id, name) in shared {
                    items.push(PickItem::new(name).hint(id.clone()));
                    ids.push(Some(id));
                }
                self.modal = Some((Modal::pick("Shared result", items, 0), Pending::Shared(ids)));
                return Nav::Stay;
            }
            Act::ResultThenRoll => {
                if let Page::Result(i) = self.page {
                    let current = self.draft.results[i].then_roll.clone();
                    let tables: Vec<(TableId, String)> = ctx
                        .service
                        .overview()
                        .map(|o| {
                            o.tables
                                .into_iter()
                                .filter(|t| !t.deleted && t.id != self.draft.id)
                                .map(|t| (t.id, t.name))
                                .collect()
                        })
                        .unwrap_or_default();
                    let ids: Vec<TableId> = tables.iter().map(|(id, _)| id.clone()).collect();
                    self.modal = Some((
                        Modal::check(
                            "Then roll these tables",
                            tables
                                .iter()
                                .map(|(id, n)| format!("{n}  ({id})"))
                                .collect(),
                            ids.iter().map(|id| current.contains(id)).collect(),
                        ),
                        Pending::ThenRoll(ids),
                    ));
                    return Nav::Stay;
                }
            }
            Act::MoveUp | Act::MoveDown => {
                if let Page::Result(i) = self.page {
                    let j = if act == Act::MoveUp {
                        i.checked_sub(1)
                    } else {
                        Some(i + 1)
                    };
                    if let Some(j) = j.filter(|j| *j < self.draft.results.len()) {
                        self.draft.results.swap(i, j);
                        self.page = Page::Result(j);
                        self.relayout(None);
                    }
                }
            }
            Act::Duplicate => {
                if let Page::Result(i) = self.page {
                    let mut copy = self.draft.results[i].clone();
                    copy.id = ResultId::new(new_user_id("result"));
                    copy.name = format!("Copy of {}", copy.name);
                    copy.unique = false;
                    copy.fallback = None;
                    self.draft.results.insert(i + 1, copy);
                    self.page = Page::Result(i + 1);
                    self.relayout(None);
                }
            }
            Act::Selection => {
                self.draft.selection = match self.draft.selection {
                    SelectionMode::Roll => SelectionMode::Choose,
                    SelectionMode::Choose => SelectionMode::Roll,
                };
            }
            Act::Active => self.draft.active = !self.draft.active,
            Act::Condition(target) => {
                let value = self.condition_of(target).clone();
                self.condition = Some((
                    target,
                    ConditionEditor::new(
                        "Condition",
                        value,
                        self.tags.clone(),
                        ctx.service
                            .toggle_settings()
                            .unwrap_or_default()
                            .into_iter()
                            .map(|s| (s.id, s.name))
                            .collect(),
                    ),
                ));
                return Nav::Stay;
            }
            Act::Open(page) => {
                self.page = page;
                self.state = RowsState::default();
                return Nav::Stay;
            }
            Act::AddResult => {
                self.draft.results.push(blank_result());
                self.relayout(None);
                self.page = Page::Result(self.draft.results.len() - 1);
                self.state = RowsState::default();
            }
            Act::AddVariant => {
                self.draft.variants.push(TableVariant {
                    id: VariantId::new(new_user_id("variant")),
                    name: "New variant".into(),
                    description: String::new(),
                    when: None,
                    dice: None,
                    ranges: None,
                });
                self.page = Page::Variant(self.draft.variants.len() - 1);
                self.state = RowsState::default();
            }
            Act::AddFollowUp => {
                self.draft.follow_ups.push(FollowUpRoll {
                    id: new_user_id("followUp"),
                    name: "New follow-up roll".into(),
                    description: String::new(),
                    when: None,
                    on_results: Vec::new(),
                    dice: "1d4".into(),
                    outcomes: vec![FollowUpOutcome {
                        range: settlement_core::content::FaceRange::new(1, 4),
                        becomes: None,
                    }],
                });
                self.page = Page::FollowUp(self.draft.follow_ups.len() - 1);
                self.state = RowsState::default();
            }
            Act::Save => self.save(ctx),
            Act::Revert => self.draft = self.original.clone(),
            Act::Reset => match ctx
                .service
                .reset_table(&self.draft.id)
                .and_then(|_| ctx.service.table_detail(&self.draft.id))
            {
                Ok(d) => *self = TableEditor::open(ctx, Some(d), None),
                Err(e) => self.message = Some((e, true)),
            },
            Act::Delete => {
                self.modal = Some((
                    Modal::confirm(
                        &format!(
                            "Delete the custom table “{}”? This cannot be undone.",
                            self.draft.name
                        ),
                        "Delete",
                    ),
                    Pending::ConfirmDelete,
                ));
                return Nav::Stay;
            }
            Act::ResultName | Act::ResultDescription | Act::ResultFaces | Act::ResultValue => {
                if let Page::Result(i) = self.page {
                    let r = &self.draft.results[i];
                    let faces_help = "Faces: a number, or two like 1-10; empty for none.";
                    let (title, value, help) = match act {
                        Act::ResultName => ("Result name", r.name.clone(), faces_help),
                        Act::ResultDescription => {
                            ("Result description", r.description.clone(), faces_help)
                        }
                        Act::ResultValue => (
                            "Value",
                            r.value.map_or(String::new(), |v| v.to_string()),
                            "The number this result stands for: a positive whole number.",
                        ),
                        _ => (
                            "Faces",
                            r.range.map_or(String::new(), |x| x.to_string()),
                            faces_help,
                        ),
                    };
                    self.text(title, help, &value, act);
                }
            }
            Act::ResultTags => {
                if let Page::Result(i) = self.page {
                    let current = self.draft.results[i].tags.clone();
                    let mut ids: Vec<TagId> = self.tags.iter().map(|(id, _)| id.clone()).collect();
                    // Keep any tag the catalogue does not list, so confirming
                    // never drops a reference silently.
                    for id in &current {
                        if !ids.contains(id) {
                            ids.push(id.clone());
                        }
                    }
                    let labels = ids
                        .iter()
                        .map(|id| match self.tags.iter().find(|(t, _)| t == id) {
                            Some((_, n)) => format!("{n}  ({id})"),
                            None => format!("{id}  (not in the catalogue)"),
                        })
                        .collect();
                    self.modal = Some((
                        Modal::check(
                            "Emitted tags",
                            labels,
                            ids.iter().map(|id| current.contains(id)).collect(),
                        ),
                        Pending::Tags(ids),
                    ));
                }
            }
            Act::ResultEnabled => {
                if let Page::Result(i) = self.page {
                    self.draft.results[i].enabled = !self.draft.results[i].enabled;
                    self.relayout(None);
                }
            }
            Act::ResultUnique => {
                if let Page::Result(i) = self.page {
                    let r = &mut self.draft.results[i];
                    r.unique = !r.unique;
                    if !r.unique {
                        r.fallback = None;
                    }
                }
            }
            Act::ResultFallback => {
                if let Page::Result(i) = self.page {
                    let this = self.draft.results[i].id.clone();
                    let mut ids = vec![None];
                    let mut items = vec![PickItem::new("None")];
                    for r in self
                        .draft
                        .results
                        .iter()
                        .filter(|r| r.id != this && !r.unique)
                    {
                        ids.push(Some(r.id.clone()));
                        items.push(PickItem::new(r.name.clone()));
                    }
                    self.modal = Some((
                        Modal::pick("Fallback when already held", items, 0),
                        Pending::Fallback(ids),
                    ));
                }
            }
            Act::ResultVariants => {
                if let Page::Result(i) = self.page {
                    let current = variants_text(&self.draft.results[i].label_variants);
                    self.text(
                        "Display variants",
                        "tag=Label pairs separated by ;, e.g. gender.man=Lord; gender.woman=Lady",
                        &current,
                        Act::ResultVariants,
                    );
                }
            }
            Act::RemoveResult => {
                if let Page::Result(i) = self.page {
                    let id = self.draft.results[i].id.clone();
                    let usages = ctx
                        .service
                        .result_usages(&self.draft, &id)
                        .unwrap_or_default();
                    if usages.is_empty() {
                        self.draft.results.remove(i);
                        self.relayout(None);
                        self.page = Page::Main;
                        self.state = RowsState::default();
                    } else {
                        let list: Vec<&str> =
                            usages.iter().map(|u| u.description.as_str()).collect();
                        self.message = Some((
                            format!(
                                "This result cannot be removed while it is used by: {}. Disable it instead, or remove those uses first.",
                                list.join("; ")
                            ),
                            true,
                        ));
                    }
                }
            }
            Act::VariantName | Act::VariantDice => {
                if let Page::Variant(i) = self.page {
                    let v = &self.draft.variants[i];
                    let (title, value) = if act == Act::VariantName {
                        ("Variant name", v.name.clone())
                    } else {
                        ("Variant dice", v.dice.clone().unwrap_or_default())
                    };
                    self.text(
                        title,
                        "Leave dice empty to use the table's dice.",
                        &value,
                        act,
                    );
                }
            }
            Act::VariantOwnRanges => {
                if let Page::Variant(i) = self.page {
                    let results = self.draft.results.clone();
                    let v = &mut self.draft.variants[i];
                    v.ranges = match v.ranges {
                        Some(_) => None,
                        None => Some(
                            results
                                .iter()
                                .filter_map(|r| {
                                    r.range.map(|range| RangeAssignment {
                                        result: r.id.clone(),
                                        range,
                                    })
                                })
                                .collect(),
                        ),
                    };
                }
            }
            Act::VariantRange(result) => {
                if let Page::Variant(i) = self.page {
                    let current = self.draft.variants[i]
                        .ranges
                        .as_ref()
                        .and_then(|r| r.iter().find(|a| a.result == result))
                        .map_or(String::new(), |a| a.range.to_string());
                    self.text(
                        &format!("Faces for {}", self.result_name(&result)),
                        "A number, or two like 1-10; empty removes it from this variant.",
                        &current,
                        act,
                    );
                }
            }
            Act::RemoveVariant => {
                if let Page::Variant(i) = self.page {
                    self.draft.variants.remove(i);
                    self.page = Page::Main;
                    self.state = RowsState::default();
                }
            }
            Act::FollowName | Act::FollowDice => {
                if let Page::FollowUp(i) = self.page {
                    let fu = &self.draft.follow_ups[i];
                    let (title, value) = if act == Act::FollowName {
                        ("Follow-up name", fu.name.clone())
                    } else {
                        ("Follow-up dice", fu.dice.clone())
                    };
                    self.text(title, "", &value, act);
                }
            }
            Act::FollowOn => {
                if let Page::FollowUp(i) = self.page {
                    let current = self.draft.follow_ups[i].on_results.clone();
                    let ids: Vec<ResultId> =
                        self.draft.results.iter().map(|r| r.id.clone()).collect();
                    self.modal = Some((
                        Modal::check(
                            "After these results",
                            self.draft.results.iter().map(|r| r.name.clone()).collect(),
                            ids.iter().map(|id| current.contains(id)).collect(),
                        ),
                        Pending::OnResults(ids),
                    ));
                }
            }
            Act::Outcome(k) => {
                self.modal = Some((
                    Modal::pick(
                        "Outcome",
                        vec![
                            PickItem::new("Edit faces"),
                            PickItem::new("Change result"),
                            PickItem::new("Remove outcome"),
                        ],
                        0,
                    ),
                    Pending::OutcomeMenu(k),
                ));
            }
            Act::AddOutcome => {
                if let Page::FollowUp(i) = self.page {
                    self.draft.follow_ups[i].outcomes.push(FollowUpOutcome {
                        range: settlement_core::content::FaceRange::single(1),
                        becomes: None,
                    });
                }
            }
            Act::RemoveFollowUp => {
                if let Page::FollowUp(i) = self.page {
                    self.draft.follow_ups.remove(i);
                    self.page = Page::Main;
                    self.state = RowsState::default();
                }
            }
            Act::Done => {
                self.page = Page::Main;
                self.state = RowsState::default();
            }
        }
        self.recheck(ctx);
        Nav::Stay
    }
}

impl Screen for TableEditor {
    fn title(&self) -> String {
        format!(
            "Tables › {}",
            if self.draft.name.is_empty() {
                "Unnamed"
            } else {
                &self.draft.name
            }
        )
    }

    fn render(&mut self, f: &mut Frame, area: Rect, ctx: &Ctx) {
        if let Some((_, editor)) = &mut self.condition {
            editor.render(f, area, ctx);
            return;
        }
        let rows = self.rows(ctx, area.width);
        self.state.render(f, area, &rows, &ctx.styles, true);
        if let Some((m, _)) = &mut self.modal {
            m.render(f, area, &ctx.styles);
        }
    }

    fn handle(&mut self, key: KeyEvent, ctx: &mut Ctx) -> Nav {
        let before = self.draft.clone();
        self.stepped = false;
        let nav = self.handle_key(key, ctx);
        // Undo and redo move along the history themselves.
        if self.draft != before && !self.stepped {
            self.undo.push(before);
            self.redo.clear();
            self.after_change(ctx);
        }
        nav
    }

    fn hints(&self) -> Vec<(&'static str, &'static str)> {
        self.key_hints()
    }
}

impl TableEditor {
    /// After any change: check it again and keep it as a draft.
    fn after_change(&mut self, ctx: &mut Ctx) {
        self.recheck(ctx);
        if self.dirty() {
            let _ = ctx.service.save_table_draft(&self.draft);
        }
    }

    fn step_history(&mut self, back: bool, ctx: &mut Ctx) {
        let (from, to) = if back {
            (&mut self.undo, &mut self.redo)
        } else {
            (&mut self.redo, &mut self.undo)
        };
        self.stepped = true;
        if let Some(previous) = from.pop() {
            to.push(std::mem::replace(&mut self.draft, previous));
            if let Page::Result(i) = self.page
                && i >= self.draft.results.len()
            {
                self.page = Page::Main;
            }
            self.after_change(ctx);
        }
    }

    fn handle_key(&mut self, key: KeyEvent, ctx: &mut Ctx) -> Nav {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        if self.condition.is_none() && self.modal.is_none() {
            match key.code {
                KeyCode::Char('z') if ctrl => {
                    self.step_history(true, ctx);
                    return Nav::Stay;
                }
                KeyCode::Char('y') if ctrl => {
                    self.step_history(false, ctx);
                    return Nav::Stay;
                }
                KeyCode::Char('u') => {
                    self.step_history(true, ctx);
                    return Nav::Stay;
                }
                KeyCode::Char('U') => {
                    self.step_history(false, ctx);
                    return Nav::Stay;
                }
                _ => {}
            }
        }
        if let Some((target, editor)) = &mut self.condition {
            if let Outcome::Done = editor.handle(key, ctx) {
                let (target, value) = (*target, editor.value.clone());
                *self.condition_of(target) = value;
                self.condition = None;
                self.recheck(ctx);
            }
            return Nav::Stay;
        }
        if let Some(nav) = self.handle_modal(key, ctx) {
            return nav;
        }
        let rows = self.rows(ctx, 120);
        if self.state.handle_nav(&rows, key) {
            return Nav::Stay;
        }
        if matches!(key.code, KeyCode::Esc | KeyCode::Backspace) {
            if self.page != Page::Main {
                self.page = Page::Main;
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
        if key.code == KeyCode::Char('s') && self.page == Page::Main && self.can_save() {
            self.save(ctx);
            return Nav::Stay;
        }
        if matches!(key.code, KeyCode::Enter | KeyCode::Char(' '))
            && let Some(act) = self.state.current(&rows).cloned()
        {
            return self.act(act, ctx);
        }
        Nav::Stay
    }

    fn key_hints(&self) -> Vec<(&'static str, &'static str)> {
        if self.condition.is_some() {
            vec![
                ("Enter", "edit"),
                ("t", "type"),
                ("a", "add"),
                ("d", "remove"),
                ("Esc", "done"),
            ]
        } else if self.page == Page::Main {
            vec![
                ("↑↓", "move"),
                ("Enter", "edit"),
                ("u/U", "undo/redo"),
                ("s", "save"),
                ("Esc", "back"),
            ]
        } else {
            vec![
                ("↑↓", "move"),
                ("Enter", "edit"),
                ("u/U", "undo/redo"),
                ("Esc", "back to table"),
            ]
        }
    }
}

/// A new, enabled result with a fresh user ID.
fn blank_result() -> TableResult {
    TableResult {
        id: ResultId::new(new_user_id("result")),
        name: "New result".into(),
        shared: None,
        description: String::new(),
        range: None,
        tags: Vec::new(),
        eligibility: None,
        enabled: true,
        generator: None,
        value: None,
        unique: false,
        fallback: None,
        label_variants: Vec::new(),
        then_roll: Vec::new(),
    }
}

/// "gender.man=Lord; gender.woman=Lady".
fn variants_text(variants: &[LabelVariant]) -> String {
    variants
        .iter()
        .map(|v| format!("{}={}", v.tag, v.name))
        .collect::<Vec<_>>()
        .join("; ")
}

/// Reads `tag=Label` pairs separated by `;`. Empty means none.
fn parse_variants(text: &str) -> Result<Vec<LabelVariant>, String> {
    text.split(';')
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .map(|pair| {
            let (tag, name) = pair
                .split_once('=')
                .ok_or_else(|| format!("“{pair}” needs the form tag=Label."))?;
            let (tag, name) = (tag.trim(), name.trim());
            if tag.is_empty() || name.is_empty() {
                return Err(format!("“{pair}” needs both a tag and a label."));
            }
            Ok(LabelVariant {
                tag: TagId::new(tag),
                name: name.to_owned(),
            })
        })
        .collect()
}
