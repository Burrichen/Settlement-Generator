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

    /// This layout with each result's chance multiplied by its factors
    /// (`percent` / 100 each), laid out again on `dice`. Results keep their
    /// order. Unchanged if the dice cannot hold the results.
    pub fn reweighted(self, dice: &str, factors: &[(&ResultId, u32)]) -> Self {
        let ranges: Vec<FaceRange> = self.entries.iter().map(|(_, r)| *r).collect();
        let Some(base) = crate::dice_assist::range_weights(&ranges, dice) else {
            return self;
        };
        let weights: Vec<u32> = self
            .entries
            .iter()
            .zip(base)
            .map(|((id, _), w)| {
                let factor: f64 = factors
                    .iter()
                    .filter(|(r, _)| r == id)
                    .map(|(_, p)| f64::from(*p) / 100.0)
                    .product();
                ((w as f64) * factor * 100.0).round().max(1.0) as u32
            })
            .collect();
        match crate::dice_assist::fit(&weights, dice) {
            Ok(fit) => Layout {
                entries: self
                    .entries
                    .iter()
                    .zip(fit.ranges)
                    .map(|((id, _), range)| (*id, range))
                    .collect(),
                ..self
            },
            Err(_) => self,
        }
    }

    /// Lowest and highest face assigned to any result.
    pub fn bounds(&self) -> Option<FaceRange> {
        let min = self.entries.iter().map(|(_, r)| r.min).min()?;
        let max = self.entries.iter().map(|(_, r)| r.max).max()?;
        Some(FaceRange::new(min, max))
    }
}
