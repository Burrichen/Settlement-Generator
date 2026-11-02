//! Settlement export files: one saved settlement with everything stored for
//! it (snapshot, maps, building details, Notable Location directory, points
//! of interest, district names and saved views), as one JSON document.
//!
//! - **Versioned.** `format` names the kind of file and `format_version` its
//!   layout. A newer version, or a snapshot or map format this build does not
//!   know, is refused with a clear message, never half read.
//! - **Import adds a new settlement** with a new ID (an export never
//!   overwrites anything), in one transaction: all of it or nothing.
//! - Map geometry is imported as saved, never regenerated.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;
use time::OffsetDateTime;

use crate::maps::directory::{DistrictNames, LocationRecord};
use crate::maps::{
    BuildingRecord, MAP_FORMAT_VERSION, PointOfInterest, SavedView, SettlementMapSet,
};
use crate::settlements::{SNAPSHOT_FORMAT_VERSION, SettlementSnapshot};

/// The `format` of a settlement export file.
pub const ARCHIVE_FORMAT: &str = "settlement-generator.settlement";
/// The newest settlement export layout this build writes and reads.
pub const ARCHIVE_VERSION: u32 = 1;
/// The file extension used for settlement exports.
pub const ARCHIVE_EXTENSION: &str = "settlement.json";

/// One exported settlement.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SettlementArchive {
    pub format: String,
    pub format_version: u32,
    /// The application version that wrote the file.
    pub app_version: String,
    #[serde(with = "time::serde::rfc3339")]
    pub exported_at: OffsetDateTime,
    pub settlement: ArchivedSettlement,
    /// Absent when the settlement has no maps.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub maps: Option<ArchivedMaps>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ArchivedSettlement {
    pub name: String,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
    /// The application version that generated its snapshot.
    pub app_version: String,
    pub snapshot_format: u32,
    pub snapshot: SettlementSnapshot,
}

/// Everything stored beside a settlement's maps.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ArchivedMaps {
    pub map_format: u32,
    pub set: SettlementMapSet,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub buildings: Vec<BuildingRecord>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub locations: Vec<LocationRecord>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pois: Vec<PointOfInterest>,
    pub next_marker: u32,
    pub next_custom: u32,
    #[serde(default, skip_serializing_if = "DistrictNames::is_empty")]
    pub district_names: DistrictNames,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub views: Vec<SavedView>,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ArchiveError {
    #[error("This file is not valid JSON: {0}")]
    NotJson(String),
    #[error("This is not a Settlement Generator settlement export.")]
    NotAnArchive,
    #[error(
        "This settlement was exported by a newer version of Settlement Generator (export format \
         {found}; this version reads up to {supported}). Update the application to import it."
    )]
    Newer { found: u32, supported: u32 },
    #[error("The export is damaged: {0}")]
    Damaged(String),
}

impl SettlementArchive {
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("settlement archive serialises")
    }

    /// Reads and checks an export file's text.
    pub fn from_json(text: &str) -> Result<Self, ArchiveError> {
        let value: Value =
            serde_json::from_str(text).map_err(|e| ArchiveError::NotJson(e.to_string()))?;
        if value.get("format").and_then(Value::as_str) != Some(ARCHIVE_FORMAT) {
            return Err(ArchiveError::NotAnArchive);
        }
        let version = value
            .get("format_version")
            .and_then(Value::as_u64)
            .ok_or(ArchiveError::NotAnArchive)?;
        if version > u64::from(ARCHIVE_VERSION) {
            return Err(ArchiveError::Newer {
                found: version as u32,
                supported: ARCHIVE_VERSION,
            });
        }
        let archive: SettlementArchive =
            serde_json::from_value(value).map_err(|e| ArchiveError::Damaged(e.to_string()))?;
        archive.check()?;
        Ok(archive)
    }

    /// Consistency checks beyond the JSON shape.
    fn check(&self) -> Result<(), ArchiveError> {
        let damaged = |m: String| Err(ArchiveError::Damaged(m));
        if self.settlement.snapshot_format > SNAPSHOT_FORMAT_VERSION {
            return Err(ArchiveError::Newer {
                found: self.settlement.snapshot_format,
                supported: SNAPSHOT_FORMAT_VERSION,
            });
        }
        let Some(maps) = &self.maps else {
            return Ok(());
        };
        if maps.map_format > MAP_FORMAT_VERSION {
            return Err(ArchiveError::Newer {
                found: maps.map_format,
                supported: MAP_FORMAT_VERSION,
            });
        }
        let set = &maps.set;
        let mut ids = std::collections::HashSet::new();
        for map in &set.maps {
            if !ids.insert(&map.id) {
                return damaged(format!("the map `{}` appears twice", map.id));
            }
            if map.settlement_id != set.settlement_id
                || map
                    .geometry
                    .buildings
                    .iter()
                    .any(|b| b.map_id != map.id || b.settlement_id != set.settlement_id)
            {
                return damaged(format!(
                    "the map `{}` belongs to another settlement",
                    map.id
                ));
            }
        }
        let mut buildings = std::collections::HashSet::new();
        for b in &maps.buildings {
            if !buildings.insert(&b.building_id) {
                return damaged(format!("the building `{}` appears twice", b.building_id));
            }
        }
        if maps.next_marker == 0 || maps.next_custom == 0 {
            return damaged("its marker numbering is missing".into());
        }
        let highest = maps
            .locations
            .iter()
            .filter_map(|l| l.marker)
            .chain(maps.pois.iter().map(|p| p.number))
            .max()
            .unwrap_or(0);
        if highest >= maps.next_marker {
            return damaged("a marker number is not below the next one to issue".into());
        }
        let mut placed = std::collections::HashSet::new();
        for l in &maps.locations {
            if let Some(a) = &l.assignment {
                if !placed.insert(&a.building_id) {
                    return damaged(format!("two locations stand on `{}`", a.building_id));
                }
            }
        }
        Ok(())
    }
}
