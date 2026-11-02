//! Building details: generated lazily, one building at a time, from content
//! tables, then edited by the user. See `docs/MAPS.md` §12.
//!
//! - Map generation creates geometry only. A building has no details until
//!   it is first opened; then they are rolled once and stored, and every
//!   later opening reads them back.
//! - Generated values and user edits are kept apart. A user value always
//!   wins, and generation never replaces it unless the user discards it.
//! - Every generated value records where it came from: a table roll (with
//!   every die), a fixed inference from the map, or a table that was not
//!   rolled.

use serde::{Deserialize, Serialize};
use thiserror::Error;
use time::OffsetDateTime;

use crate::VERSION;
use crate::content::{ContentSet, ContentStatus};
use crate::dice::FaceSource;
use crate::engine::{
    GenerationError, GenerationEvent, Generator, Named, Request, Selections, SkipReason, State,
    TableRollRecord,
};
use crate::ids::{BuildingId, MapId, TableId, TagId};
use crate::locations::District;
use crate::settlements::{SettlementId, now};

use super::model::{MapBuilding, SettlementMap, StreetKind};

/// What an ordinary building is used for.
pub const TYPE_TABLE: &str = "table.building.type";
pub const CONDITION_TABLE: &str = "table.building.condition";
pub const OCCUPANCY_TABLE: &str = "table.building.occupancy";
/// How many people; each result's `value` is the number.
pub const OCCUPANTS_TABLE: &str = "table.building.occupants";
/// One noticeable feature, used as the short description.
pub const FEATURE_TABLE: &str = "table.building.feature";

/// The tables rolled for a building, in this order. Each sees the tags of
/// the results rolled before it (for example Occupancy sees Condition).
pub const DETAIL_TABLES: [&str; 5] = [
    TYPE_TABLE,
    CONDITION_TABLE,
    OCCUPANCY_TABLE,
    OCCUPANTS_TABLE,
    FEATURE_TABLE,
];

/// Longest name or address.
pub const MAX_NAME_CHARS: usize = 120;
/// Longest building type, condition or occupancy.
pub const MAX_LABEL_CHARS: usize = 80;
pub const MAX_DESCRIPTION_CHARS: usize = 1000;
pub const MAX_NOTES_CHARS: usize = 5000;
pub const MAX_OCCUPANTS: u32 = 10_000;

/// Whether a building's details have been generated or changed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DetailsState {
    /// Nothing generated yet: every building starts here, and has no
    /// stored record.
    Ungenerated,
    /// Generated and stored; the user has changed nothing.
    Generated,
    /// The user has changed at least one field (or written notes). Those
    /// values are protected from regeneration.
    Customised,
}

// ----- generated values ---------------------------------------------------------

/// Why a detail table was not rolled for a building.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NotRolledReason {
    /// The table is switched off.
    Inactive,
    /// The table's eligibility does not hold for this building (for
    /// example no occupants for an empty building).
    NotEligible,
    /// The content has no such table (a deleted default).
    Missing,
    /// The table is marked incomplete.
    Incomplete,
}

/// Where a generated value came from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum GeneratedSource {
    /// Rolled on a content table; the whole roll is kept.
    Table { roll: Box<TableRollRecord> },
    /// The table was not rolled, so there is no value.
    NotRolled {
        table: TableId,
        reason: NotRolledReason,
    },
    /// Derived from the map by a fixed rule, described here.
    Inferred { rule: String },
}

/// A generated value and its source. `value` is absent when nothing was
/// generated.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeneratedValue<T> {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<T>,
    pub source: GeneratedSource,
}

/// Everything generated for a building, as rolled.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeneratedDetails {
    /// An address inferred from the map.
    pub name: GeneratedValue<String>,
    pub building_type: GeneratedValue<String>,
    pub description: GeneratedValue<String>,
    pub condition: GeneratedValue<String>,
    pub occupancy: GeneratedValue<String>,
    pub occupants: GeneratedValue<u32>,
    /// Tags emitted by this building's own results. The settlement's tags,
    /// which the rolls also ran under, are in the map set's inputs.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<Named<TagId>>,
    #[serde(with = "time::serde::rfc3339")]
    pub generated_at: OffsetDateTime,
    pub app_version: String,
}

// ----- user edits -----------------------------------------------------------------

/// A value the user set. `value: null` is a field left blank on purpose.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct UserValue<T> {
    #[serde(default)]
    pub value: Option<T>,
}

impl<T> UserValue<T> {
    pub fn new(value: Option<T>) -> Self {
        UserValue { value }
    }
}

/// The user's changes. A field left out keeps its generated value.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct BuildingEdits {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<UserValue<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub building_type: Option<UserValue<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<UserValue<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub condition: Option<UserValue<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub occupancy: Option<UserValue<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub occupants: Option<UserValue<u32>>,
    /// The user's own notes. Never generated.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub notes: String,
}

impl BuildingEdits {
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    /// The edits with every generated field given back: only the notes.
    pub fn notes_only(&self) -> Self {
        BuildingEdits {
            notes: self.notes.clone(),
            ..Self::default()
        }
    }

    /// Checks the user's values and tidies them: text is trimmed, and text
    /// that is empty after trimming is a field left blank on purpose.
    pub fn cleaned(self) -> Result<Self, DetailsError> {
        let line = |field, max, v: Option<UserValue<String>>| {
            v.map(|v| clean_text(field, max, false, v)).transpose()
        };
        let notes = clean_text(
            "Notes",
            MAX_NOTES_CHARS,
            true,
            UserValue::new(Some(self.notes)),
        )?;
        let occupants = self.occupants;
        if occupants
            .as_ref()
            .and_then(|o| o.value)
            .is_some_and(|n| n > MAX_OCCUPANTS)
        {
            return Err(DetailsError::Occupants);
        }
        Ok(BuildingEdits {
            name: line("Name", MAX_NAME_CHARS, self.name)?,
            building_type: line("Building type", MAX_LABEL_CHARS, self.building_type)?,
            description: self
                .description
                .map(|v| clean_text("Description", MAX_DESCRIPTION_CHARS, true, v))
                .transpose()?,
            condition: line("Condition", MAX_LABEL_CHARS, self.condition)?,
            occupancy: line("Occupancy", MAX_LABEL_CHARS, self.occupancy)?,
            occupants,
            notes: notes.value.unwrap_or_default(),
        })
    }
}

fn clean_text(
    field: &'static str,
    max: usize,
    multi_line: bool,
    v: UserValue<String>,
) -> Result<UserValue<String>, DetailsError> {
    let Some(text) = v.value else {
        return Ok(v);
    };
    let text = text.replace("\r\n", "\n");
    let text = text.trim();
    if !multi_line && text.contains('\n') {
        return Err(DetailsError::MultiLine { field });
    }
    if text.chars().count() > max {
        return Err(DetailsError::TooLong { field, max });
    }
    Ok(UserValue::new((!text.is_empty()).then(|| text.to_owned())))
}

// ----- the stored record ----------------------------------------------------------

/// A building's stored details: what was generated and what the user
/// changed, kept apart. Buildings without one are ungenerated.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BuildingRecord {
    pub map_id: MapId,
    pub building_id: BuildingId,
    /// Increases with every write. A change must name the version it was
    /// made from, so a stale copy can never overwrite a newer one.
    pub version: u32,
    /// How many times the details have been generated (1 the first time).
    pub generation: u32,
    pub generated: GeneratedDetails,
    #[serde(default, skip_serializing_if = "BuildingEdits::is_empty")]
    pub edits: BuildingEdits,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "time::serde::rfc3339::option"
    )]
    pub edited_at: Option<OffsetDateTime>,
}

impl BuildingRecord {
    pub(crate) fn new(map_id: MapId, building_id: BuildingId, generated: GeneratedDetails) -> Self {
        BuildingRecord {
            map_id,
            building_id,
            version: 1,
            generation: 1,
            generated,
            edits: BuildingEdits::default(),
            edited_at: None,
        }
    }

    pub fn state(&self) -> DetailsState {
        if self.edits.is_empty() {
            DetailsState::Generated
        } else {
            DetailsState::Customised
        }
    }

    /// Newly generated values. The user's changes are kept unless
    /// `discard_edits`; notes are always kept, since they are never
    /// generated.
    pub(crate) fn regenerated(&self, generated: GeneratedDetails, discard_edits: bool) -> Self {
        let edits = if discard_edits {
            self.edits.notes_only()
        } else {
            self.edits.clone()
        };
        BuildingRecord {
            version: self.version + 1,
            generation: self.generation + 1,
            edited_at: if edits == self.edits {
                self.edited_at
            } else {
                Some(now())
            },
            generated,
            edits,
            ..self.clone()
        }
    }

    /// The user's (already cleaned) changes, replacing the earlier ones.
    pub(crate) fn edited(&self, edits: BuildingEdits) -> Self {
        BuildingRecord {
            version: self.version + 1,
            edits,
            edited_at: Some(now()),
            ..self.clone()
        }
    }
}

// ----- generation -------------------------------------------------------------------

/// Rolls a building's details on the detail tables under the settlement's
/// tags, and infers its address from the map.
pub(crate) fn generate(
    generator: &Generator<'_>,
    content: &ContentSet,
    settlement_tags: &[TagId],
    map: &SettlementMap,
    building: &MapBuilding,
    faces: &mut dyn FaceSource,
) -> Result<GeneratedDetails, GenerationError> {
    let mut state = State::with_tags(settlement_tags);
    let mut rolls = Vec::with_capacity(DETAIL_TABLES.len());
    for table in DETAIL_TABLES {
        rolls.push(roll(generator, content, &mut state, table, faces)?);
    }
    let tags = rolls
        .iter()
        .filter_map(|r| r.as_ref().ok())
        .flat_map(|r| r.tags.iter().cloned())
        .collect();
    let mut rolls = rolls.into_iter().zip(DETAIL_TABLES);
    let mut text = || {
        let (rolled, table) = rolls.next().expect("one roll per detail table");
        to_value(table, rolled, |r| Some(r.result.name.clone()))
    };
    let building_type = text();
    let condition = text();
    let occupancy = text();
    let (rolled, table) = rolls.next().expect("one roll per detail table");
    let occupants = to_value(table, rolled, |r| {
        r.value.and_then(|v| u32::try_from(v).ok())
    });
    let (rolled, table) = rolls.next().expect("one roll per detail table");
    let description = to_value(table, rolled, |r| Some(r.result.name.clone()));
    Ok(GeneratedDetails {
        name: address(map, building),
        building_type,
        description,
        condition,
        occupancy,
        occupants,
        tags,
        generated_at: now(),
        app_version: VERSION.to_owned(),
    })
}

fn to_value<T>(
    table: &str,
    rolled: Result<TableRollRecord, NotRolledReason>,
    value: impl Fn(&TableRollRecord) -> Option<T>,
) -> GeneratedValue<T> {
    match rolled {
        Ok(roll) => GeneratedValue {
            value: value(&roll),
            source: GeneratedSource::Table {
                roll: Box::new(roll),
            },
        },
        Err(reason) => GeneratedValue {
            value: None,
            source: GeneratedSource::NotRolled {
                table: TableId::new(table),
                reason,
            },
        },
    }
}

/// One detail table, rolled in `state` so that its results' tags reach
/// the tables after it.
fn roll(
    generator: &Generator<'_>,
    content: &ContentSet,
    state: &mut State,
    table: &str,
    faces: &mut dyn FaceSource,
) -> Result<Result<TableRollRecord, NotRolledReason>, GenerationError> {
    let id = TableId::new(table);
    match content.table(&id) {
        None => return Ok(Err(NotRolledReason::Missing)),
        Some(t) if t.status == ContentStatus::Incomplete => {
            return Ok(Err(NotRolledReason::Incomplete));
        }
        Some(_) => {}
    }
    generator.process(state, [Request::new(id)].into(), &Selections::new(), faces)?;
    let outcome = state.take_events().into_iter().find_map(|e| match e {
        GenerationEvent::TableRolled(r) if r.routed_by.is_none() => Some(Ok(*r)),
        GenerationEvent::TableSkipped { reason, .. } => Some(Err(match reason {
            SkipReason::Inactive => NotRolledReason::Inactive,
            _ => NotRolledReason::NotEligible,
        })),
        _ => None,
    });
    Ok(outcome.unwrap_or(Err(NotRolledReason::NotEligible)))
}

/// "No. 42, Lane 3": the building's number (from its ID) and the street it
/// faces. No address without a street.
fn address(map: &SettlementMap, building: &MapBuilding) -> GeneratedValue<String> {
    let number = building
        .id
        .as_str()
        .rsplit('-')
        .next()
        .and_then(|n| n.parse::<u32>().ok());
    let street = building
        .street
        .as_ref()
        .and_then(|id| map.geometry.streets.iter().find(|s| &s.id == id))
        .map(|s| {
            let kind = match s.kind {
                StreetKind::Road => "Road",
                StreetKind::Street => "Street",
                StreetKind::Lane => "Lane",
            };
            match s.id.rsplit('-').next() {
                Some(n) if n.chars().all(|c| c.is_ascii_digit()) => format!("{kind} {n}"),
                _ => kind.to_owned(),
            }
        });
    GeneratedValue {
        value: number.zip(street).map(|(n, s)| format!("No. {n}, {s}")),
        source: GeneratedSource::Inferred {
            rule: "Address from the building's number and the street it faces.".into(),
        },
    }
}

// ----- what interfaces show ----------------------------------------------------------

/// Where a field's current value came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FieldOrigin {
    /// Rolled on a content table.
    Table,
    /// Derived from the map.
    Inferred,
    /// Not generated (its table was not rolled); blank.
    NotRolled,
    /// Set by the user (possibly blank on purpose).
    User,
}

/// One field as shown: the current value, where it came from, and what
/// generation gave (kept even when the user has changed it).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BuildingField<T> {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<T>,
    pub origin: FieldOrigin,
    pub generated: GeneratedValue<T>,
}

impl<T: Clone> BuildingField<T> {
    fn new(generated: &GeneratedValue<T>, edit: Option<&UserValue<T>>) -> Self {
        let (value, origin) = match edit {
            Some(edit) => (edit.value.clone(), FieldOrigin::User),
            None => (
                generated.value.clone(),
                match generated.source {
                    GeneratedSource::Table { .. } => FieldOrigin::Table,
                    GeneratedSource::Inferred { .. } => FieldOrigin::Inferred,
                    GeneratedSource::NotRolled { .. } => FieldOrigin::NotRolled,
                },
            ),
        };
        BuildingField {
            value,
            origin,
            generated: generated.clone(),
        }
    }
}

/// Suggested values for the free-text fields: the results of the detail
/// tables in the current content.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DetailChoices {
    pub building_type: Vec<String>,
    pub condition: Vec<String>,
    pub occupancy: Vec<String>,
}

impl DetailChoices {
    pub fn from_content(content: &ContentSet) -> Self {
        let names = |table: &str| {
            content
                .table(&TableId::new(table))
                .map(|t| {
                    t.results
                        .iter()
                        .filter(|r| r.enabled)
                        .map(|r| r.name.clone())
                        .collect()
                })
                .unwrap_or_default()
        };
        DetailChoices {
            building_type: names(TYPE_TABLE),
            condition: names(CONDITION_TABLE),
            occupancy: names(OCCUPANCY_TABLE),
        }
    }
}

/// One building's details as the Location Inspector shows them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BuildingDetailsView {
    pub settlement_id: SettlementId,
    pub map_id: MapId,
    pub building_id: BuildingId,
    /// The map's label (the district's, for a district map).
    pub map_label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub district: Option<District>,
    pub state: DetailsState,
    pub version: u32,
    pub generation: u32,
    pub name: BuildingField<String>,
    pub building_type: BuildingField<String>,
    pub description: BuildingField<String>,
    pub condition: BuildingField<String>,
    pub occupancy: BuildingField<String>,
    pub occupants: BuildingField<u32>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub notes: String,
    /// Tags emitted by the building's own results.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<Named<TagId>>,
    #[serde(with = "time::serde::rfc3339")]
    pub generated_at: OffsetDateTime,
    /// The version of the application that generated the details.
    pub app_version: String,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "time::serde::rfc3339::option"
    )]
    pub edited_at: Option<OffsetDateTime>,
    pub choices: DetailChoices,
}

impl BuildingDetailsView {
    pub(crate) fn new(
        settlement_id: &SettlementId,
        map: &SettlementMap,
        record: &BuildingRecord,
        choices: DetailChoices,
    ) -> Self {
        let g = &record.generated;
        let e = &record.edits;
        BuildingDetailsView {
            settlement_id: settlement_id.clone(),
            map_id: record.map_id.clone(),
            building_id: record.building_id.clone(),
            map_label: map.label.clone(),
            district: map.district().cloned(),
            state: record.state(),
            version: record.version,
            generation: record.generation,
            name: BuildingField::new(&g.name, e.name.as_ref()),
            building_type: BuildingField::new(&g.building_type, e.building_type.as_ref()),
            description: BuildingField::new(&g.description, e.description.as_ref()),
            condition: BuildingField::new(&g.condition, e.condition.as_ref()),
            occupancy: BuildingField::new(&g.occupancy, e.occupancy.as_ref()),
            occupants: BuildingField::new(&g.occupants, e.occupants.as_ref()),
            notes: e.notes.clone(),
            tags: g.tags.clone(),
            generated_at: g.generated_at,
            app_version: g.app_version.clone(),
            edited_at: record.edited_at,
            choices,
        }
    }
}

/// A generated building, for lists: its state and current name and type.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BuildingSummary {
    pub map_id: MapId,
    pub building_id: BuildingId,
    pub state: DetailsState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub building_type: Option<String>,
}

impl From<&BuildingRecord> for BuildingSummary {
    fn from(record: &BuildingRecord) -> Self {
        let pick = |edit: &Option<UserValue<String>>, generated: &GeneratedValue<String>| match edit
        {
            Some(edit) => edit.value.clone(),
            None => generated.value.clone(),
        };
        BuildingSummary {
            map_id: record.map_id.clone(),
            building_id: record.building_id.clone(),
            state: record.state(),
            name: pick(&record.edits.name, &record.generated.name),
            building_type: pick(&record.edits.building_type, &record.generated.building_type),
        }
    }
}

/// Why building details could not be read or changed.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum DetailsError {
    #[error("This settlement has no maps yet.")]
    NoMaps,
    #[error("There is no map `{0}` on this settlement.")]
    UnknownMap(MapId),
    #[error("There is no building `{building}` on the map `{map}`.")]
    UnknownBuilding { map: MapId, building: BuildingId },
    #[error("This building's details have not been generated yet. Open the building first.")]
    NotGenerated,
    #[error(
        "These details were changed elsewhere after they were opened. They have been \
         reloaded; make your change again."
    )]
    Changed,
    #[error("{field} can be at most {max} characters.")]
    TooLong { field: &'static str, max: usize },
    #[error("{field} must be a single line.")]
    MultiLine { field: &'static str },
    #[error("Occupants must be a whole number from 0 to {MAX_OCCUPANTS}.")]
    Occupants,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(value: &str) -> Option<UserValue<String>> {
        Some(UserValue::new(Some(value.to_owned())))
    }

    #[test]
    fn cleaning_trims_and_keeps_blank_fields_blank() {
        let edits = BuildingEdits {
            name: text("  The Old Mill  "),
            description: text("   "),
            condition: Some(UserValue::new(None)),
            notes: "  a note \r\n second line ".into(),
            ..Default::default()
        }
        .cleaned()
        .unwrap();
        assert_eq!(edits.name, text("The Old Mill"));
        // Whitespace only, or null, is a field left blank on purpose.
        assert_eq!(edits.description, Some(UserValue::new(None)));
        assert_eq!(edits.condition, Some(UserValue::new(None)));
        // Fields not sent stay generated.
        assert_eq!(edits.building_type, None);
        assert_eq!(edits.notes, "a note \n second line");
    }

    #[test]
    fn cleaning_refuses_bad_values() {
        let long = "x".repeat(MAX_NAME_CHARS + 1);
        let too_long = BuildingEdits {
            name: text(&long),
            ..Default::default()
        };
        assert!(matches!(
            too_long.cleaned(),
            Err(DetailsError::TooLong { field: "Name", .. })
        ));
        let two_lines = BuildingEdits {
            building_type: text("Shop\nHouse"),
            ..Default::default()
        };
        assert!(matches!(
            two_lines.cleaned(),
            Err(DetailsError::MultiLine { .. })
        ));
        let crowded = BuildingEdits {
            occupants: Some(UserValue::new(Some(MAX_OCCUPANTS + 1))),
            ..Default::default()
        };
        assert_eq!(crowded.cleaned(), Err(DetailsError::Occupants));
        // Description and notes may span lines.
        let multi = BuildingEdits {
            description: text("One.\nTwo."),
            ..Default::default()
        };
        assert!(multi.cleaned().is_ok());
    }

    #[test]
    fn blank_and_absent_user_values_serialise_differently() {
        let edits = BuildingEdits {
            occupants: Some(UserValue::new(None)),
            ..Default::default()
        };
        let json = serde_json::to_value(&edits).unwrap();
        assert_eq!(json, serde_json::json!({ "occupants": { "value": null } }));
        let back: BuildingEdits = serde_json::from_value(json).unwrap();
        assert_eq!(back, edits);
        assert!(!back.is_empty());
    }
}
