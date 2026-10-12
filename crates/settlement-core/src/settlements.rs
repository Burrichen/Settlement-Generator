//! Saved settlements: immutable snapshots of what was generated.
//!
//! A snapshot copies every label, tag, die and applied modifier at the time
//! of generation (see [`GenerationLog`]), so later changes to content, by the
//! user or by an update, can never change a saved settlement. Only explicit
//! edits or rerolls replace a snapshot.

use std::fmt;

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use crate::authority::AuthorityRepresentatives;
use crate::engine::{GenerationEvent, GenerationLog, Named};
use crate::figures::AuthorityFigures;
use crate::ids::{ResultId, TableId, TagId};
use crate::locations::NotableLocations;
use crate::naming::NameRecord;
use crate::roles::RoleAllocation;

/// Longest settlement name accepted (after trimming).
pub const MAX_NAME_LENGTH: usize = 80;

/// Version of the snapshot format written by this build.
pub const SNAPSHOT_FORMAT_VERSION: u32 = 1;

/// Stable, randomly generated settlement ID.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SettlementId(String);

impl SettlementId {
    pub fn new_random() -> Self {
        let bits: u128 = rand::random();
        SettlementId(format!("stl-{bits:032x}"))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<&str> for SettlementId {
    fn from(id: &str) -> Self {
        SettlementId(id.to_owned())
    }
}

impl fmt::Display for SettlementId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// What was generated: results, display labels, tags, dice, routing, applied
/// modifiers and options, all copied at generation time.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SettlementSnapshot {
    pub log: GenerationLog,
    /// The pipeline stages as they stood when saved: labels, tables, results
    /// and tags, so the settlement can be shown without current content.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub stages: Vec<SnapshotStage>,
    /// Who represents the Authority, as generated: the input to the future
    /// Authority Figure NPC generator. Absent in snapshots saved before it
    /// existed; see [`SettlementSnapshot::authority_representatives`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub authority: Option<AuthorityRepresentatives>,
}

/// One pipeline stage of a saved settlement, as it was when saved.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SnapshotStage {
    /// Stage label, e.g. "Region".
    pub label: String,
    /// The stage table the pipeline names (before redirects).
    pub stage_table: TableId,
    /// The table actually used, e.g. "Kurovia Region".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub table: Option<Named<TableId>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<Named<ResultId>>,
    /// Tags this stage activated.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<Named<TagId>>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub skipped: bool,
    /// The stage elaborates the one before it (see
    /// [`SequenceStep::sub_stage`](crate::content::SequenceStep::sub_stage)).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub sub_stage: bool,
    /// The Notable Locations stage's record (only on that stage): counts,
    /// districts and every location with its rolls and source.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub locations: Option<NotableLocations>,
    /// The Role Allocation stage's record (only on that stage): every role,
    /// lead first, with its roll and any unique-role fallback.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub roles: Option<RoleAllocation>,
    /// The Authority Figures stage's record (only on that stage): every
    /// figure and relationship with its rolls, as generated.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub figures: Option<AuthorityFigures>,
    /// The name stage's record (only on the name stage).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<NameRecord>,
}

impl From<GenerationLog> for SettlementSnapshot {
    fn from(log: GenerationLog) -> Self {
        SettlementSnapshot {
            log,
            stages: Vec::new(),
            authority: None,
        }
    }
}

impl SettlementSnapshot {
    /// The Notable Locations, as generated.
    pub fn notable_locations(&self) -> Option<&NotableLocations> {
        self.stages.iter().find_map(|s| s.locations.as_ref())
    }

    /// The roles allocated to the Authority Figures, as allocated.
    pub fn authority_roles(&self) -> Option<&RoleAllocation> {
        self.stages.iter().find_map(|s| s.roles.as_ref())
    }

    /// The Authority Figures and their relationships, as generated.
    pub fn authority_figures(&self) -> Option<&AuthorityFigures> {
        self.stages.iter().find_map(|s| s.figures.as_ref())
    }

    /// Who represents the Authority: as stored, or, for a snapshot saved
    /// without it, as recorded in its log. Never re-read from content.
    pub fn authority_representatives(&self) -> Option<AuthorityRepresentatives> {
        self.authority
            .clone()
            .or_else(|| AuthorityRepresentatives::from_log(&self.log))
    }

    /// The stages to display: the recorded stages, or, for snapshots saved
    /// without them, one entry per top-level table record in the log.
    pub fn display_stages(&self) -> Vec<SnapshotStage> {
        if !self.stages.is_empty() {
            return self.stages.clone();
        }
        self.log
            .events
            .iter()
            .filter_map(|event| match event {
                GenerationEvent::TableRolled(r) if r.routed_by.is_none() => {
                    let requested = r.redirected_from.as_ref().unwrap_or(&r.table);
                    Some(SnapshotStage {
                        label: requested.name.clone(),
                        stage_table: requested.id.clone(),
                        table: Some(r.table.clone()),
                        result: Some(r.result.clone()),
                        tags: r.tags.clone(),
                        skipped: false,
                        sub_stage: false,
                        figures: None,
                        roles: None,
                        locations: None,
                        name: None,
                    })
                }
                _ => None,
            })
            .collect()
    }
}

/// Listing information for a saved settlement (no snapshot).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SettlementSummary {
    pub id: SettlementId,
    pub name: String,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    pub updated_at: OffsetDateTime,
    /// Application version that generated the current snapshot.
    pub app_version: String,
    pub favourite: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Settlement {
    #[serde(flatten)]
    pub summary: SettlementSummary,
    pub snapshot: SettlementSnapshot,
}

/// Current time in UTC, truncated to the millisecond precision stored.
pub fn now() -> OffsetDateTime {
    from_unix_millis(to_unix_millis(OffsetDateTime::now_utc()))
}

pub(crate) fn to_unix_millis(t: OffsetDateTime) -> i64 {
    (t.unix_timestamp_nanos() / 1_000_000) as i64
}

pub(crate) fn from_unix_millis(ms: i64) -> OffsetDateTime {
    OffsetDateTime::from_unix_timestamp_nanos(i128::from(ms) * 1_000_000)
        .unwrap_or(OffsetDateTime::UNIX_EPOCH)
}
