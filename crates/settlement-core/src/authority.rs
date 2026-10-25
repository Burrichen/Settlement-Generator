//! Who represents a settlement's Authority: the input to Role Allocation and
//! the Authority Figures.
//!
//! The Authority stage is followed by two conditional sub-stages, defined
//! entirely in content (`content/10-authority/`):
//!
//! - **Representation** (1d4) decides between a single representative and a
//!   group. Authority results that are always single or always a group emit
//!   the representation tag themselves (an `add_tag` effect), and the table's
//!   eligibility skips it for them.
//! - **Authority Size** (1d10) gives the number of primary representatives of
//!   a group. It is only eligible while the group tag is active.
//!
//! Which tables and tags these are is configuration ([`AuthorityConfig`],
//! part of the built-in steps' config in content).
//! [`AuthorityRepresentatives::from_log_with`] reads the outcome back from a
//! generation log: from the recorded tags and the value recorded with the
//! Authority Size roll, never from current content, so a saved settlement
//! always reports what it was generated with.

use serde::{Deserialize, Serialize};

use crate::engine::{GenerationLog, Named};
use crate::ids::{ResultId, TableId, TagId};

/// The Authority table of content in the legacy format.
pub const AUTHORITY_TABLE: &str = "table.authority";
/// The Authority Size table of content in the legacy format.
pub const AUTHORITY_SIZE_TABLE: &str = "table.authoritySize";
/// The single-representative tag of content in the legacy format.
pub const SINGLE_TAG: &str = "authorityRepresentation.single";
/// The group-representative tag of content in the legacy format.
pub const GROUP_TAG: &str = "authorityRepresentation.group";

/// Where an Authority and its representatives are found in a generation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthorityConfig {
    /// The table whose result is the Authority.
    pub table: TableId,
    /// The table whose result's `value` is the size of a group.
    pub size_table: TableId,
    /// Emitted when the Authority has a single primary representative.
    pub single_tag: TagId,
    /// Emitted when the Authority has a group of primary representatives.
    pub group_tag: TagId,
}

impl AuthorityConfig {
    /// The configuration of content in the legacy `sequence` format, and of
    /// settlements saved before it was configurable.
    pub fn legacy() -> Self {
        AuthorityConfig {
            table: TableId::new(AUTHORITY_TABLE),
            size_table: TableId::new(AUTHORITY_SIZE_TABLE),
            single_tag: TagId::new(SINGLE_TAG),
            group_tag: TagId::new(GROUP_TAG),
        }
    }

    pub fn tables(&self) -> [&TableId; 2] {
        [&self.table, &self.size_table]
    }

    pub fn tags(&self) -> [&TagId; 2] {
        [&self.single_tag, &self.group_tag]
    }
}

/// Whether an authority is represented by one person or a group.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Representation {
    Single,
    Group,
}

/// What Role Allocation and the Authority Figures receive: the authority,
/// how it is represented, and how many primary representatives to create.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthorityRepresentatives {
    /// The Authority result, as recorded.
    pub authority: Named<ResultId>,
    /// The tags the Authority result emitted (e.g. `authority.council`).
    pub tags: Vec<Named<TagId>>,
    pub representation: Representation,
    /// Number of primary representatives: 1 for a single representative,
    /// the Authority Size value for a group. Not the size of the wider
    /// institution.
    pub size: u32,
}

impl AuthorityRepresentatives {
    /// Reads the representatives from a log with the legacy configuration.
    pub fn from_log(log: &GenerationLog) -> Option<Self> {
        Self::from_log_with(log, &AuthorityConfig::legacy())
    }

    /// Reads the representatives from a log. `None` when Authority was not
    /// generated, or its representation is not recorded (a settlement
    /// generated before representation existed, or not yet finished).
    pub fn from_log_with(log: &GenerationLog, config: &AuthorityConfig) -> Option<Self> {
        let record = log.record_for(&config.table)?;
        let representation = if log.has_tag(&config.group_tag) {
            Representation::Group
        } else if log.has_tag(&config.single_tag) {
            Representation::Single
        } else {
            return None;
        };
        let size = match representation {
            Representation::Single => 1,
            Representation::Group => log
                .record_for(&config.size_table)?
                .value
                .and_then(|v| u32::try_from(v).ok())
                .filter(|v| *v > 0)?,
        };
        Some(AuthorityRepresentatives {
            authority: record.result.clone(),
            tags: record.tags.clone(),
            representation,
            size,
        })
    }
}
