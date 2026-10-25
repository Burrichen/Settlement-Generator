//! Authority Role Allocation: the Role / Title of each Authority Figure,
//! allocated before the figures themselves are generated.
//!
//! A **role scheme** (content, chosen by the first `when` that holds over
//! the settlement's tags, e.g. `authority.council`) names an Authority's
//! role tables:
//!
//! - a single representative rolls once on the `single` table;
//! - a group rolls exactly one role on the `lead` table, then
//!   `Authority Size − 1` roles on the `member` table.
//!
//! Role tables are ordinary, editable tables. A result marked `unique` can
//! be held once per allocation; rolling (or choosing) it again resolves as
//! its `fallback` result, which must be repeatable, so there is never a
//! reroll and the outcome follows from the dice alone. Both the result
//! rolled and the role finally given are recorded.
//!
//! Roles have stable result IDs (e.g. `role.council.treasurer`) but emit no
//! tags: they never enter the settlement's tag pool. A role's displayed
//! title can depend on its holder (e.g. Lord / Lady by gender) through the
//! result's `label_variants`; see [`RoleAssignment::label_for`].

use serde::{Deserialize, Serialize};

use crate::VERSION;
use crate::authority::{AuthorityConfig, AuthorityRepresentatives, Representation};
use crate::content::{Condition, ContentSet, LabelVariant};
use crate::dice::FaceSource;
use crate::engine::{
    GenerationError, GenerationEvent, Generator, Named, Request, ResultOption, Selections, State,
    TableRollRecord,
};
use crate::ids::{ResultId, RoleSchemeId, TableId, TagId};

/// The stage table recorded in snapshots for the Role Allocation stage,
/// which has no table of its own.
pub const ROLES_STAGE_ID: &str = "stage.authorityRoles";

/// The Role Allocation built-in step's configuration: where the Authority
/// to allocate roles for is found. The role tables come from the content's
/// role schemes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RolesConfig {
    pub authority: AuthorityConfig,
}

impl RolesConfig {
    /// The configuration of content in the legacy `sequence` format.
    pub fn legacy() -> Self {
        RolesConfig {
            authority: AuthorityConfig::legacy(),
        }
    }
}

/// The role tables of one kind of Authority.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RoleScheme {
    pub id: RoleSchemeId,
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    /// The scheme applies while this holds; the first that applies is used.
    pub when: Condition,
    /// Rolled for a single representative.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub single: Option<TableId>,
    /// Rolled once for a group's lead.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lead: Option<TableId>,
    /// Rolled for each other member of a group.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub member: Option<TableId>,
}

/// How a role was allocated.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Allocation {
    Single,
    Lead,
    Member,
}

/// One allocated role, with how it came about.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoleAssignment {
    pub allocation: Allocation,
    /// The roll (or choice) on the role table: table, dice, every die, and
    /// the result rolled.
    pub roll: TableRollRecord,
    /// The result rolled or chosen, before the unique-role rule.
    pub initial: Named<ResultId>,
    /// The role given instead, when `initial` is unique and was already
    /// held.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fallback: Option<Named<ResultId>>,
    /// The role given: `fallback` if any, else `initial`. Its name is the
    /// neutral title (e.g. "Lord / Lady").
    pub role: Named<ResultId>,
    /// The final role's display variants at allocation time.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub label_variants: Vec<LabelVariant>,
}

impl RoleAssignment {
    /// The title as shown for a holder with `tags` (e.g. "Lady" for
    /// `gender.woman`).
    pub fn label_for(&self, tags: &[TagId]) -> String {
        LabelVariant::pick(&self.label_variants, &self.role.name, tags)
    }
}

/// Every role of an Authority, lead first: the record saved with the
/// settlement.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoleAllocation {
    /// The Authority and how many roles it needs.
    pub request: AuthorityRepresentatives,
    pub scheme: Named<RoleSchemeId>,
    /// One per Authority Figure, in figure order.
    pub roles: Vec<RoleAssignment>,
    pub app_version: String,
}

/// One role to allocate: how, and on which table.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoleSlot {
    pub allocation: Allocation,
    pub table: TableId,
}

/// The role scheme that applies under `tags`.
pub fn scheme_for<'c>(content: &'c ContentSet, tags: &[TagId]) -> Option<&'c RoleScheme> {
    content
        .role_schemes
        .iter()
        .find(|s| s.when.evaluate(&|t| tags.contains(t)))
}

/// The roles `request` needs from `scheme`: exactly one per representative.
pub fn slots(
    scheme: &RoleScheme,
    request: &AuthorityRepresentatives,
) -> Result<Vec<RoleSlot>, GenerationError> {
    let missing = |what: &str| GenerationError::RoleTableMissing {
        scheme: scheme.id.to_string(),
        what: what.to_owned(),
    };
    Ok(match request.representation {
        Representation::Single => vec![RoleSlot {
            allocation: Allocation::Single,
            table: scheme.single.clone().ok_or_else(|| missing("single"))?,
        }],
        Representation::Group => {
            let lead = scheme.lead.clone().ok_or_else(|| missing("lead"))?;
            let member = scheme.member.clone().ok_or_else(|| missing("member"))?;
            std::iter::once(RoleSlot {
                allocation: Allocation::Lead,
                table: lead,
            })
            .chain((1..request.size).map(|_| RoleSlot {
                allocation: Allocation::Member,
                table: member.clone(),
            }))
            .collect()
        }
    })
}

/// Allocates roles with shared content and physical dice.
pub(crate) struct RoleAllocator<'g, 'c> {
    pub generator: &'g Generator<'c>,
    pub content: &'c ContentSet,
    /// Tags active in the settlement before roles are allocated.
    pub settlement_tags: &'g [TagId],
}

impl RoleAllocator<'_, '_> {
    /// The scheme that applies, or an error naming the Authority.
    pub fn scheme(
        &self,
        request: &AuthorityRepresentatives,
    ) -> Result<&RoleScheme, GenerationError> {
        scheme_for(self.content, self.settlement_tags).ok_or_else(|| {
            GenerationError::NoRoleScheme {
                authority: request.authority.name.clone(),
            }
        })
    }

    /// Allocates every role, lead first, each against the roles before it.
    pub fn allocate(
        &self,
        request: &AuthorityRepresentatives,
        faces: &mut dyn FaceSource,
    ) -> Result<RoleAllocation, GenerationError> {
        let scheme = self.scheme(request)?;
        let mut roles: Vec<RoleAssignment> = Vec::new();
        for slot in slots(scheme, request)? {
            let held: Vec<ResultId> = roles.iter().map(|r| r.role.id.clone()).collect();
            roles.push(self.assign(&slot, &held, None, faces)?);
        }
        Ok(RoleAllocation {
            request: request.clone(),
            scheme: Named {
                id: scheme.id.clone(),
                name: scheme.name.clone(),
            },
            roles,
            app_version: VERSION.to_owned(),
        })
    }

    /// Rolls (or uses the chosen result for) one role again, checked
    /// against every other role held.
    pub fn set_role(
        &self,
        record: &mut RoleAllocation,
        index: usize,
        choice: Option<ResultId>,
        faces: &mut dyn FaceSource,
    ) -> Result<(), GenerationError> {
        let scheme = self.scheme(&record.request)?;
        let slot = slots(scheme, &record.request)?
            .into_iter()
            .nth(index)
            .ok_or(GenerationError::NoSuchRole { index })?;
        let held: Vec<ResultId> = record
            .roles
            .iter()
            .enumerate()
            .filter(|(i, _)| *i != index)
            .map(|(_, r)| r.role.id.clone())
            .collect();
        let assignment = self.assign(&slot, &held, choice, faces)?;
        let role = record
            .roles
            .get_mut(index)
            .ok_or(GenerationError::NoSuchRole { index })?;
        *role = assignment;
        record.app_version = VERSION.to_owned();
        Ok(())
    }

    /// For each slot of `request`, its table and results under the current
    /// tags. Empty when no scheme applies.
    pub fn slot_options(&self, request: &AuthorityRepresentatives) -> Vec<RoleSlotOptions> {
        let Ok(scheme) = self.scheme(request) else {
            return Vec::new();
        };
        let Ok(slots) = slots(scheme, request) else {
            return Vec::new();
        };
        let state = State::with_tags(self.settlement_tags);
        slots
            .into_iter()
            .filter_map(|slot| {
                let (table, options) = self.generator.options_for(&slot.table, &state).ok()?;
                Some(RoleSlotOptions {
                    allocation: slot.allocation,
                    table,
                    options,
                })
            })
            .collect()
    }

    /// Rolls (or uses the chosen result of) one slot's table, then applies
    /// the unique-role rule against the roles already `held`.
    fn assign(
        &self,
        slot: &RoleSlot,
        held: &[ResultId],
        choice: Option<ResultId>,
        faces: &mut dyn FaceSource,
    ) -> Result<RoleAssignment, GenerationError> {
        let mut state = State::with_tags(self.settlement_tags);
        self.generator.process(
            &mut state,
            [Request::new(slot.table.clone()).with_choice(choice)].into(),
            &Selections::new(),
            faces,
        )?;
        let roll = state
            .take_events()
            .into_iter()
            .find_map(|e| match e {
                GenerationEvent::TableRolled(r) if !r.is_routed() => Some(*r),
                _ => None,
            })
            .ok_or_else(|| GenerationError::FigureTableSkipped {
                table: slot.table.clone(),
            })?;
        let table = self
            .content
            .table(&roll.table.id)
            .ok_or_else(|| GenerationError::UnknownTable(roll.table.id.clone()))?;
        let rolled = table
            .result(&roll.result.id)
            .ok_or_else(|| GenerationError::UnknownTable(roll.table.id.clone()))?;
        let initial = roll.result.clone();
        // A unique role already held becomes its fallback: no reroll.
        let (fallback, given) = match table.fallback_for(rolled, held) {
            Some(fallback) => (Some(Named::new(&fallback.id, &fallback.name)), fallback),
            None => (None, rolled),
        };
        Ok(RoleAssignment {
            allocation: slot.allocation,
            initial,
            role: Named::new(&given.id, &given.name),
            fallback,
            label_variants: given.label_variants.clone(),
            roll,
        })
    }
}

/// One role slot's table and its results.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoleSlotOptions {
    pub allocation: Allocation,
    pub table: Named<TableId>,
    pub options: Vec<ResultOption>,
}
