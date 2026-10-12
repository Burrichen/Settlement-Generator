//! Who represents a settlement's Authority: the data contract for the
//! planned Authority Figure NPC generator, which does not exist yet.
//!
//! The Authority stage is followed by two conditional sub-stages, defined
//! entirely in content (`content/10-authority.json`):
//!
//! - **Representation** (1d4) decides between a single representative and a
//!   group. Authority results that are always single or always a group emit
//!   the representation tag themselves (an `add_tag` effect), and the table's
//!   eligibility skips it for them.
//! - **Authority Size** (1d10) gives the number of primary representatives of
//!   a group. It is only eligible while the group tag is active.
//!
//! [`AuthorityRepresentatives::from_log`] reads the outcome back from a
//! generation log: from the recorded tags and the value recorded with the
//! Authority Size roll, never from current content, so a saved settlement
//! always reports what it was generated with.

use serde::{Deserialize, Serialize};

use crate::engine::{GenerationLog, Named};
use crate::ids::{ResultId, TableId, TagId};

/// The Authority table.
pub const AUTHORITY_TABLE: &str = "table.authority";
/// The Authority Size table, rolled for groups.
pub const AUTHORITY_SIZE_TABLE: &str = "table.authoritySize";
/// Emitted when the authority has a single primary representative.
pub const SINGLE_TAG: &str = "authorityRepresentation.single";
/// Emitted when the authority has a group of primary representatives.
pub const GROUP_TAG: &str = "authorityRepresentation.group";

/// Whether an authority is represented by one person or a group.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Representation {
    Single,
    Group,
}

/// What the future Authority Figure NPC generator receives: the authority,
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
    /// Reads the representatives from a log. `None` when Authority was not
    /// generated, or its representation is not recorded (a settlement
    /// generated before representation existed, or not yet finished).
    pub fn from_log(log: &GenerationLog) -> Option<Self> {
        let record = log.record_for(&TableId::new(AUTHORITY_TABLE))?;
        let representation = if log.has_tag(&TagId::new(GROUP_TAG)) {
            Representation::Group
        } else if log.has_tag(&TagId::new(SINGLE_TAG)) {
            Representation::Single
        } else {
            return None;
        };
        let size = match representation {
            Representation::Single => 1,
            Representation::Group => log
                .record_for(&TableId::new(AUTHORITY_SIZE_TABLE))?
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
