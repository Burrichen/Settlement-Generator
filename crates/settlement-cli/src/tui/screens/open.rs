//! Opening a dependent item straight from a "used by" row: its editor is
//! pushed on top, and Esc comes back.

use settlement_core::editor::TagUsage;
use settlement_core::ids::{FlowId, TableId, TagId};
use settlement_core::table_editor::Usage;

use super::flows::FlowEditor;
use super::tables::TableEditor;
use super::tags::TagEditor;
use crate::tui::{Ctx, Nav};

/// An item a usage points at.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ItemRef {
    Table(String),
    Tag(String),
    Flow(String),
}

/// What a usage belongs to: its flow, else its table, else its tag.
pub fn of_usage(u: &Usage) -> Option<ItemRef> {
    if let Some(f) = &u.flow {
        return Some(ItemRef::Flow(f.to_string()));
    }
    if let Some(t) = &u.table {
        return Some(ItemRef::Table(t.to_string()));
    }
    u.tag.as_ref().map(|t| ItemRef::Tag(t.to_string()))
}

pub fn of_tag_usage(u: &TagUsage) -> Option<ItemRef> {
    let l = &u.location;
    if let Some(f) = &l.flow {
        return Some(ItemRef::Flow(f.clone()));
    }
    if let Some(t) = &l.table {
        return Some(ItemRef::Table(t.to_string()));
    }
    l.tag.as_ref().map(|t| ItemRef::Tag(t.to_string()))
}

/// Opens `item` in its editor; a message if it cannot be opened.
pub fn open(ctx: &mut Ctx, item: &ItemRef) -> Nav {
    let result = match item {
        ItemRef::Table(id) => ctx
            .service
            .table_detail(&TableId::new(id))
            .map(|d| Nav::Push(Box::new(TableEditor::open(ctx, Some(d), None)))),
        ItemRef::Tag(id) => ctx
            .service
            .tag_detail(&TagId::new(id))
            .map(|d| Nav::Push(Box::new(TagEditor::open(ctx, Some(d), None)))),
        ItemRef::Flow(id) => {
            let id = FlowId::new(id);
            ctx.service.flow(&id).map(|saved| {
                let kept = ctx.service.flow_draft(&id).ok().flatten();
                Nav::Push(Box::new(FlowEditor::open(ctx, Some(saved), kept, None)))
            })
        }
    };
    result.unwrap_or_else(|e| {
        ctx.flash = Some(e);
        Nav::Stay
    })
}
