//! Resolution of a table's base or variant layout: which dice are rolled and
//! which faces select which result. Shared by validation and the engine so
//! both interpret variants identically.

use crate::content::{FaceRange, Table, TableVariant};
use crate::ids::ResultId;

pub(crate) struct Layout<'a> {
    pub variant: Option<&'a TableVariant>,
    /// `None` for tables (or variants of tables) without dice.
    pub dice: Option<&'a str>,
    pub entries: Vec<(&'a ResultId, FaceRange)>,
}

impl<'a> Layout<'a> {
    pub fn base(table: &'a Table) -> Self {
        Layout {
            variant: None,
            dice: table.dice.as_deref(),
            entries: table
                .results
                .iter()
                .filter_map(|r| r.range.map(|range| (&r.id, range)))
                .collect(),
        }
    }

    pub fn for_variant(table: &'a Table, variant: &'a TableVariant) -> Self {
        let base = Layout::base(table);
        Layout {
            variant: Some(variant),
            dice: variant.dice.as_deref().or(table.dice.as_deref()),
            entries: match &variant.ranges {
                Some(ranges) => ranges.iter().map(|a| (&a.result, a.range)).collect(),
                None => base.entries,
            },
        }
    }

    /// The base layout followed by every variant layout.
    pub fn all(table: &'a Table) -> Vec<Self> {
        std::iter::once(Layout::base(table))
            .chain(table.variants.iter().map(|v| Layout::for_variant(table, v)))
            .collect()
    }

    pub fn result_at(&self, total: i32) -> Option<&'a ResultId> {
        self.entries
            .iter()
            .find(|(_, range)| range.contains(total))
            .map(|(id, _)| *id)
    }

    /// Lowest and highest face assigned to any result.
    pub fn bounds(&self) -> Option<FaceRange> {
        let min = self.entries.iter().map(|(_, r)| r.min).min()?;
        let max = self.entries.iter().map(|(_, r)| r.max).max()?;
        Some(FaceRange::new(min, max))
    }
}
