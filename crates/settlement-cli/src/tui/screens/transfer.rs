//! Import / Export: content files to share, back up or move between
//! installs. Exports go to the exports folder; imports are checked in full
//! (and the ID-collision choice made) before anything changes.

use std::path::PathBuf;

use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use settlement_core::framework::layers::ItemKind;
use settlement_core::service::{AppService, ExportScope};
use settlement_core::transfer::{
    Collision, ContentPack, ImportAction, ImportPlan, PackItem, PackKind,
};

use super::common::issue_lines;
use crate::tui::widgets::{Modal, ModalResult, PickItem, Row, RowsState};
use crate::tui::{Ctx, Nav, Screen};

#[derive(Clone, Debug, PartialEq)]
enum Act {
    ExportMine,
    ExportAll,
    ExportChosen,
    File(PathBuf),
    TypePath,
    Policy,
    Import,
    Cancel,
}

enum Pending {
    Choose(Vec<PackItem>),
    Needs(Vec<PackItem>),
    Path,
    Policy,
}

/// A file being imported: the pack and what importing it would do.
struct Importing {
    path: PathBuf,
    pack: ContentPack,
    policy: Collision,
    plan: ImportPlan,
}

pub struct TransferScreen {
    state: RowsState,
    modal: Option<(Modal, Pending)>,
    importing: Option<Importing>,
    message: Option<(String, bool)>,
}

const POLICIES: [(Collision, &str); 3] = [
    (Collision::Rename, "Keep both (import as new copies)"),
    (Collision::Replace, "Replace mine"),
    (Collision::Skip, "Keep mine (skip those items)"),
];

fn policy_label(policy: Collision) -> &'static str {
    POLICIES
        .iter()
        .find(|(p, _)| *p == policy)
        .map_or("", |(_, l)| *l)
}

fn action_words(action: &ImportAction) -> String {
    match action {
        ImportAction::Add => "add".into(),
        ImportAction::AddAs { .. } => "add as a new copy".into(),
        ImportAction::Replace { existing } => format!("replace “{existing}”"),
        ImportAction::Skip { existing } => format!("skip (keep “{existing}”)"),
    }
}

impl TransferScreen {
    pub fn new(_ctx: &mut Ctx) -> Self {
        TransferScreen {
            state: RowsState::default(),
            modal: None,
            importing: None,
            message: None,
        }
    }

    fn rows(&self, ctx: &Ctx) -> Vec<Row<Act>> {
        let s = &ctx.styles;
        let mut rows = Vec::new();
        if let Some((m, error)) = &self.message {
            rows.push(Row::info(Line::styled(
                m.clone(),
                if *error { s.error() } else { s.good() },
            )));
            rows.push(Row::blank());
        }
        if let Some(i) = &self.importing {
            rows.push(Row::info(Line::styled(
                format!("IMPORT {}", i.path.display()),
                s.dim(),
            )));
            rows.push(Row::info(Line::styled(i.plan.summary.clone(), s.title())));
            for item in &i.plan.items {
                rows.push(Row::info(Line::raw(format!(
                    "  {} “{}”: {}",
                    item.kind.label(),
                    item.name,
                    action_words(&item.action)
                ))));
            }
            if !i.plan.issues.is_empty() || i.plan.error.is_some() {
                rows.extend(
                    issue_lines(&i.plan.issues, i.plan.error.as_deref(), s)
                        .into_iter()
                        .map(Row::info),
                );
            }
            rows.push(Row::blank());
            rows.push(Row::item(
                Line::from(vec![
                    Span::styled("If an ID is already used  ", s.dim()),
                    Span::raw(policy_label(i.policy)),
                ]),
                Act::Policy,
            ));
            if i.plan.can_import {
                rows.push(Row::item(Line::styled("Import", s.heading()), Act::Import));
            } else {
                rows.push(Row::info(Line::styled(
                    "Import (not possible: see above)",
                    s.dim(),
                )));
            }
            rows.push(Row::item(Line::raw("Cancel"), Act::Cancel));
            return rows;
        }
        let folder = ctx
            .exports
            .clone()
            .or_else(settlement_core::storage::exports_dir)
            .map_or_else(|| "(no folder)".into(), |p| p.display().to_string());
        rows.extend([
            Row::info(Line::styled("EXPORT", s.dim())),
            Row::info(Line::styled(
                format!("Files are saved in {folder}"),
                s.dim(),
            )),
            Row::item(
                Line::raw("Export everything I made or changed"),
                Act::ExportMine,
            ),
            Row::item(Line::raw("Export a complete content pack"), Act::ExportAll),
            Row::item(Line::raw("Choose what to export…"), Act::ExportChosen),
            Row::blank(),
            Row::info(Line::styled(
                "IMPORT (checked before anything changes)",
                s.dim(),
            )),
        ]);
        for path in AppService::list_packs(ctx.exports.clone()) {
            let name = path.file_name().map_or_else(
                || path.display().to_string(),
                |n| n.to_string_lossy().into(),
            );
            rows.push(Row::item(
                Line::raw(format!("Import {name}")),
                Act::File(path),
            ));
        }
        rows.push(Row::item(
            Line::raw("Import a file by its path…"),
            Act::TypePath,
        ));
        rows
    }

    fn save(&mut self, pack: Result<ContentPack, String>, name: &str, ctx: &Ctx) {
        self.message = Some(
            match pack.and_then(|p| AppService::write_pack(&p, name, ctx.exports.clone())) {
                Ok(path) => (format!("Saved to {}", path.display()), false),
                Err(e) => (e, true),
            },
        );
    }

    fn open_file(&mut self, path: PathBuf, ctx: &mut Ctx) {
        match AppService::read_pack_file(&path) {
            Ok(pack) => self.plan(path, pack, Collision::Rename, ctx),
            Err(e) => self.message = Some((e, true)),
        }
    }

    fn plan(&mut self, path: PathBuf, pack: ContentPack, policy: Collision, ctx: &mut Ctx) {
        match ctx.service.plan_import(&pack, policy) {
            Ok(plan) => {
                self.importing = Some(Importing {
                    path,
                    pack,
                    policy,
                    plan,
                });
                self.state = RowsState::default();
            }
            Err(e) => self.message = Some((e, true)),
        }
    }

    /// Every item that can be exported, by kind and name.
    fn choices(ctx: &mut Ctx) -> Vec<(PackItem, String)> {
        let mut out = Vec::new();
        if let Ok(o) = ctx.service.overview() {
            out.extend(o.tables.into_iter().filter(|t| !t.deleted).map(|t| {
                (
                    PackItem {
                        kind: PackKind::Table,
                        id: t.id.to_string(),
                    },
                    t.name,
                )
            }));
            out.extend(o.tags.into_iter().filter(|t| !t.deleted).map(|t| {
                (
                    PackItem {
                        kind: PackKind::Tag,
                        id: t.id.to_string(),
                    },
                    t.name,
                )
            }));
        }
        if let Ok(items) = ctx.service.framework_items() {
            out.extend(items.into_iter().filter(|i| !i.deleted).map(|i| {
                let kind = match i.kind {
                    ItemKind::SharedResult => PackKind::SharedResult,
                    ItemKind::Scale => PackKind::Scale,
                    ItemKind::Setting => PackKind::Setting,
                    ItemKind::ObjectType => PackKind::ObjectType,
                    ItemKind::Flow => PackKind::Flow,
                };
                (PackItem { kind, id: i.id }, i.name)
            }));
        }
        out
    }

    fn act(&mut self, act: Act, ctx: &mut Ctx) -> Nav {
        self.message = None;
        match act {
            Act::ExportMine => {
                let pack = ctx.service.export_everything(ExportScope::Customised);
                self.save(pack, "My content", ctx);
            }
            Act::ExportAll => {
                let pack = ctx.service.export_everything(ExportScope::Everything);
                self.save(pack, "Complete content pack", ctx);
            }
            Act::ExportChosen => {
                let choices = Self::choices(ctx);
                let labels = choices
                    .iter()
                    .map(|(i, n)| format!("{n} ({})", i.kind.label()))
                    .collect::<Vec<_>>();
                let ticks = vec![false; labels.len()];
                self.modal = Some((
                    Modal::check("Export which? (Space to tick)", labels, ticks),
                    Pending::Choose(choices.into_iter().map(|(i, _)| i).collect()),
                ));
            }
            Act::File(path) => self.open_file(path, ctx),
            Act::TypePath => {
                self.modal = Some((
                    Modal::text("File to import", "The full path of a content file.", ""),
                    Pending::Path,
                ));
            }
            Act::Policy => {
                self.modal = Some((
                    Modal::pick(
                        "If an ID is already used",
                        POLICIES.iter().map(|(_, l)| PickItem::new(*l)).collect(),
                        0,
                    ),
                    Pending::Policy,
                ));
            }
            Act::Import => {
                if let Some(i) = self.importing.take() {
                    self.message = Some(match ctx.service.import_pack(&i.pack, i.policy) {
                        Ok(plan) => (format!("Imported: {}", plan.summary), false),
                        Err(e) => (e, true),
                    });
                    self.state = RowsState::default();
                }
            }
            Act::Cancel => {
                self.importing = None;
                self.state = RowsState::default();
            }
        }
        Nav::Stay
    }

    fn finish(&mut self, result: ModalResult, pending: Pending, ctx: &mut Ctx) {
        match (result, pending) {
            (ModalResult::Checked(ticks), Pending::Choose(items)) => {
                let picked: Vec<PackItem> = items
                    .into_iter()
                    .zip(ticks)
                    .filter(|(_, t)| *t)
                    .map(|(i, _)| i)
                    .collect();
                if picked.is_empty() {
                    self.message = Some(("Nothing was chosen.".into(), true));
                    return;
                }
                self.modal = Some((
                    Modal::pick(
                        "Include what they need (custom tags, tables, flows)?",
                        vec![PickItem::new("Yes"), PickItem::new("No, only these")],
                        0,
                    ),
                    Pending::Needs(picked),
                ));
            }
            (ModalResult::Picked(k), Pending::Needs(items)) => {
                let pack = ctx.service.export_items(&items, k == 0, "My export");
                self.save(pack, "My export", ctx);
            }
            (ModalResult::Text(path), Pending::Path) => {
                self.open_file(PathBuf::from(path.trim()), ctx);
            }
            (ModalResult::Picked(k), Pending::Policy) => {
                if let Some(i) = self.importing.take() {
                    self.plan(i.path, i.pack, POLICIES[k].0, ctx);
                }
            }
            _ => {}
        }
    }
}

impl Screen for TransferScreen {
    fn title(&self) -> String {
        "Import / Export".into()
    }

    fn render(&mut self, f: &mut Frame, area: Rect, ctx: &Ctx) {
        let rows = self.rows(ctx);
        self.state.render(f, area, &rows, &ctx.styles, true);
        if let Some((m, _)) = &mut self.modal {
            m.render(f, area, &ctx.styles);
        }
    }

    fn handle(&mut self, key: KeyEvent, ctx: &mut Ctx) -> Nav {
        if let Some((modal, _)) = &mut self.modal {
            let result = modal.handle(key, &ctx.styles);
            if let ModalResult::Open = result {
                return Nav::Stay;
            }
            let (_, pending) = self.modal.take().expect("an open modal");
            self.finish(result, pending, ctx);
            return Nav::Stay;
        }
        let rows = self.rows(ctx);
        if self.state.handle_nav(&rows, key) {
            return Nav::Stay;
        }
        match key.code {
            KeyCode::Esc | KeyCode::Backspace if self.importing.is_some() => {
                self.act(Act::Cancel, ctx)
            }
            KeyCode::Esc | KeyCode::Backspace => Nav::Pop,
            KeyCode::Enter | KeyCode::Char(' ') => match self.state.current(&rows).cloned() {
                Some(act) => self.act(act, ctx),
                None => Nav::Stay,
            },
            _ => Nav::Stay,
        }
    }

    fn hints(&self) -> Vec<(&'static str, &'static str)> {
        vec![("↑↓", "move"), ("Enter", "choose"), ("Esc", "back")]
    }
}
