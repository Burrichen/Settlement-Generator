//! The generation framework: the generic, data-driven building blocks every
//! generator is made of. See `docs/FRAMEWORK.md`.
//!
//! - **Tables** (in [`crate::content`]) are rolled with physical dice. Their
//!   results can list **shared results**, so one outcome is defined once and
//!   reused by several tables.
//! - **Tags** carry effects, including **modifiers**: Shift moves a result
//!   along an ordered **scale**; Plus and Minus change the rolled total; Cap
//!   at Highest / Lowest gives the end of the scale.
//! - **Conditions** combine tags, **settings** and, inside flows, earlier
//!   steps' results and values with AND / OR / NOT.
//! - **Flows** are ordered steps: rolls, secondary rolls, branches,
//!   repeats, subflows, **objects** and naming. The content's `start` flow
//!   is the generation pipeline; each of its top-level steps is a stage.
//!   Steps can be inactive or `planned` (never run).
//!
//! Nothing in this module names a specific generator's concepts. The
//! settlement stages that have their own editing screens (roles, figures,
//! locations) are [built-in steps](builtin), configured by data.

pub mod builtin;
pub mod describe;
pub mod edit;
pub mod layers;
mod model;
mod records;
pub(crate) mod runner;
mod shared;
mod validate;

use std::collections::BTreeMap;

pub use model::*;
pub use records::*;
pub use shared::{link_shared_results, relink_shared_results};

use crate::content::{ContentSet, StepTarget};
use crate::ids::{SettingId, StepId};
use crate::naming::NamingSubject;

/// Values of content-defined settings, by setting ID.
pub type SettingValues = BTreeMap<SettingId, SettingValue>;

impl ContentSet {
    /// The generation pipeline: the top-level steps of the `start` flow, or
    /// (for content in the legacy format) the `sequence` as steps. Every
    /// step is included, inactive and planned ones too.
    pub fn pipeline(&self) -> Vec<FlowStep> {
        match &self.start {
            Some(id) => self.flow(id).map(|f| f.steps.clone()).unwrap_or_default(),
            None => legacy_pipeline(self),
        }
    }

    /// Where the pipeline is defined, for reporting: the start flow, if any.
    pub fn uses_flows(&self) -> bool {
        self.start.is_some()
    }
}

/// The legacy `sequence` as flow steps. Built-in stages get the
/// configuration the legacy format implied.
fn legacy_pipeline(content: &ContentSet) -> Vec<FlowStep> {
    let mut used: Vec<String> = Vec::new();
    content
        .sequence
        .iter()
        .map(|step| {
            let (base, action) = match &step.target {
                StepTarget::Table(table) => (
                    table.to_string(),
                    StepAction::Roll {
                        table: table.clone(),
                        field: None,
                        modifiers: Vec::new(),
                    },
                ),
                StepTarget::Naming => (
                    "name".to_owned(),
                    StepAction::Name {
                        subject: NamingSubject::Settlement,
                    },
                ),
                StepTarget::AuthorityRoles => builtin::legacy_step(builtin::ROLES_HANDLER),
                StepTarget::AuthorityFigures => builtin::legacy_step(builtin::FIGURES_HANDLER),
                StepTarget::NotableLocations => builtin::legacy_step(builtin::LOCATIONS_HANDLER),
            };
            let mut id = base.clone();
            let mut n = 1;
            while used.contains(&id) {
                n += 1;
                id = format!("{base}-{n}");
            }
            used.push(id.clone());
            FlowStep {
                id: StepId::new(id),
                name: step.name.clone(),
                description: String::new(),
                when: None,
                active: true,
                status: Default::default(),
                note: String::new(),
                sub_stage: step.sub_stage,
                action,
            }
        })
        .collect()
}
