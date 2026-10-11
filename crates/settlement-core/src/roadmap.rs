//! The published product roadmap (see `docs/VERSIONING.md`). Shared so both
//! interfaces show identical wording. No dates or promises beyond these.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RoadmapHorizon {
    NextMinor,
    NextMajor,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoadmapEntry {
    pub horizon: RoadmapHorizon,
    /// e.g. "Next minor update".
    pub label: String,
    /// e.g. "Improved name generation".
    pub summary: String,
}

pub fn roadmap() -> Vec<RoadmapEntry> {
    vec![
        RoadmapEntry {
            horizon: RoadmapHorizon::NextMinor,
            label: "Next minor update".into(),
            summary: "Improved name generation".into(),
        },
        RoadmapEntry {
            horizon: RoadmapHorizon::NextMajor,
            label: "Next major update".into(),
            summary: "New tables".into(),
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roadmap_matches_versioning_policy() {
        let entries = roadmap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].summary, "Improved name generation");
        assert_eq!(entries[1].summary, "New tables");
        let json = serde_json::to_value(&entries[0]).unwrap();
        assert_eq!(json["horizon"], "next_minor");
    }
}
