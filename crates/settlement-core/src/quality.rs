//! The approved Quality scale, lowest first:
//!
//! ```text
//! Poor → Below Average → Average → Above Average → Excellent
//! ```
//!
//! Only the vocabulary exists. How a Quality is rolled has not been designed,
//! so nothing assigns one yet and no distribution is defined here. Content can
//! already mark a location for a later shift along the scale (a Specialist
//! Artisan is one step better); see [`QualityShift`]. A shift never goes past
//! either end of the scale.

use serde::{Deserialize, Serialize};

use crate::engine::Named;
use crate::ids::ResultId;

/// One step of the Quality scale. Ordered lowest first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Quality {
    #[serde(rename = "quality.poor")]
    Poor,
    #[serde(rename = "quality.belowAverage")]
    BelowAverage,
    #[serde(rename = "quality.average")]
    Average,
    #[serde(rename = "quality.aboveAverage")]
    AboveAverage,
    #[serde(rename = "quality.excellent")]
    Excellent,
}

impl Quality {
    /// The whole scale, lowest first.
    pub const SCALE: [Quality; 5] = [
        Quality::Poor,
        Quality::BelowAverage,
        Quality::Average,
        Quality::AboveAverage,
        Quality::Excellent,
    ];

    /// The stable ID, e.g. `quality.aboveAverage`.
    pub fn id(self) -> &'static str {
        match self {
            Quality::Poor => "quality.poor",
            Quality::BelowAverage => "quality.belowAverage",
            Quality::Average => "quality.average",
            Quality::AboveAverage => "quality.aboveAverage",
            Quality::Excellent => "quality.excellent",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Quality::Poor => "Poor",
            Quality::BelowAverage => "Below Average",
            Quality::Average => "Average",
            Quality::AboveAverage => "Above Average",
            Quality::Excellent => "Excellent",
        }
    }

    /// `steps` along the scale (negative is down), stopping at Poor and at
    /// Excellent: there is nothing above Excellent.
    pub fn shifted(self, steps: i32) -> Quality {
        let at = Self::SCALE.iter().position(|q| *q == self).unwrap_or(0) as i32;
        let last = Self::SCALE.len() as i32 - 1;
        Self::SCALE[(at + steps).clamp(0, last) as usize]
    }
}

/// A shift along the Quality scale, recorded now and applied once the
/// location has a Quality.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QualityShift {
    /// Steps up the scale.
    pub steps: u8,
    /// The result that gave it, e.g. Specialist Artisan.
    pub source: Named<ResultId>,
}

impl QualityShift {
    /// The Quality after the shift, never above Excellent.
    pub fn apply(&self, base: Quality) -> Quality {
        base.shifted(i32::from(self.steps))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_scale_is_ordered_with_stable_ids() {
        let ids: Vec<_> = Quality::SCALE.iter().map(|q| q.id()).collect();
        assert_eq!(
            ids,
            [
                "quality.poor",
                "quality.belowAverage",
                "quality.average",
                "quality.aboveAverage",
                "quality.excellent"
            ]
        );
        assert!(Quality::SCALE.windows(2).all(|w| w[0] < w[1]));
        for q in Quality::SCALE {
            assert_eq!(serde_json::to_value(q).unwrap(), q.id());
        }
    }

    #[test]
    fn shifts_stop_at_both_ends() {
        assert_eq!(Quality::Average.shifted(1), Quality::AboveAverage);
        assert_eq!(Quality::AboveAverage.shifted(1), Quality::Excellent);
        assert_eq!(Quality::Excellent.shifted(1), Quality::Excellent);
        assert_eq!(Quality::Poor.shifted(9), Quality::Excellent);
        assert_eq!(Quality::BelowAverage.shifted(-3), Quality::Poor);
    }
}
