//! Notable Locations: places important enough to get their own identity,
//! record and (later) a numbered map marker. Not every building of a kind is
//! a Notable Location; a Town may have several taverns of which one is.
//!
//! A **location scheme** (content, chosen by the Size tag) names the tables:
//!
//! - Trading Posts, Villages and Towns roll their `count` table once;
//! - Cities and Capitals first roll their `districts` table, then the
//!   `count` table once **per district**.
//!
//! Each rolled location then rolls the Notable Location Type table. Counts
//! are the results' `value`s, recorded with each roll.
//!
//! **Subtypes.** A Type with a subtype table (`location_subtype_tables`)
//! rolls it next. Every subtype has **one canonical ID** across the whole
//! system, listed once in the content's `location_subtypes`: a subtype table
//! lists it as a result with that ID, and the result's name is its label in
//! that table (Trade's "Jeweller / Precious Goods" and Craft's "Jeweller /
//! Goldsmith" are both `locationSubtype.jeweller`). The location keeps its
//! Type separately, so the same subtype can come from different Types.
//!
//! A subtype may need more dice before it is final ([`SubtypeResolution`]):
//!
//! - **split**: roll another table to decide between subtypes (Weaponsmith /
//!   Armoursmith, 1d4);
//! - **reroll**: roll the same table again, as often as the result comes up,
//!   and give the location a Quality shift once (Specialist Artisan).
//!
//! A subtype with a `speciality` table then rolls it once; the result is
//! child data of the location (a Magic Store's speciality), never a location
//! of its own. Every roll is kept.
//!
//! **Bare Minimum Buildings** (a setting) adds exactly one more location to
//! the whole settlement, never one per district: a Tavern / Inn with 4 spare
//! rooms. It is additive (it never takes a rolled location's place), neither
//! its Type nor its subtype is rolled, and its [`LocationSource`] tells it
//! apart from a rolled Hospitality location.

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::VERSION;
use crate::content::{Condition, ContentSet};
use crate::dice::FaceSource;
use crate::engine::{
    GenerationError, GenerationEvent, Generator, MAX_REROLLS, Named, Request, ResultOption,
    Selections, State, TableRollRecord,
};
use crate::ids::{LocationSchemeId, ResultId, TableId, TagId};
use crate::quality::QualityShift;

/// The stage table recorded in snapshots for the Notable Locations stage.
pub const LOCATIONS_STAGE_ID: &str = "stage.notableLocations";
/// Rolled once for each rolled Notable Location.
pub const TYPE_TABLE: &str = "table.notableLocationType";
/// The Bare Minimum Buildings Tavern / Inn's type, which must exist in the
/// type table.
pub const HOSPITALITY_TYPE: &str = "locationType.hospitality";
/// The Bare Minimum Buildings Tavern / Inn's fixed subtype, which must be a
/// canonical subtype.
pub const TAVERN_INN_SUBTYPE: &str = "locationSubtype.tavernInn";
/// The Bare Minimum Buildings Tavern / Inn's stable location ID.
pub const GUARANTEED_TAVERN_ID: &str = "notable-location-guaranteed-tavern";
/// The Bare Minimum Buildings Tavern / Inn always has exactly this many.
pub const TAVERN_SPARE_ROOMS: u32 = 4;

/// One canonical Notable Location subtype. It has this one ID in every
/// subtype table that lists it; the tables only label and weight it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LocationSubtype {
    pub id: ResultId,
    /// The canonical name; a table may label the subtype differently.
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    /// More dice needed before the subtype is final, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolve: Option<SubtypeResolution>,
    /// Rolled once for each location of this subtype; its result is the
    /// location's speciality (e.g. a Magic Store's).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub speciality: Option<TableId>,
    /// What the subtype will offer systems that do not exist yet (e.g. Keep
    /// upgrades). Design metadata: generation only copies it to the
    /// location.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub capabilities: Vec<PlannedCapability>,
}

/// How a subtype table result that is not final is resolved.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SubtypeResolution {
    /// Roll `table`; its result (a canonical subtype) is the subtype.
    Split { table: TableId },
    /// Roll the same subtype table again until another result comes up, and
    /// shift the location's future Quality by `quality_shift` steps once,
    /// however often this result repeats.
    Reroll { quality_shift: u8 },
}

/// Something a subtype will make possible once a future system exists.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlannedCapability {
    /// Stable, e.g. `keepUpgrade.structural`.
    pub id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
}

/// The subtype table rolled for locations of one Type.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubtypeTable {
    /// A result of the Notable Location Type table.
    #[serde(rename = "type")]
    pub location_type: ResultId,
    pub table: TableId,
}

/// The canonical subtype `id`, if there is one.
pub fn subtype<'c>(content: &'c ContentSet, id: &ResultId) -> Option<&'c LocationSubtype> {
    content.location_subtypes.iter().find(|s| &s.id == id)
}

/// The subtype table of `location_type`, if it has one.
pub fn subtype_table<'c>(content: &'c ContentSet, location_type: &ResultId) -> Option<&'c TableId> {
    content
        .location_subtype_tables
        .iter()
        .find(|s| &s.location_type == location_type)
        .map(|s| &s.table)
}

/// The Notable Location tables of one Size.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LocationScheme {
    pub id: LocationSchemeId,
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    /// The scheme applies while this holds; the first that applies is used.
    pub when: Condition,
    /// How many Notable Locations: for the settlement, or per district.
    pub count: TableId,
    /// How many districts, for settlements mapped by district.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub districts: Option<TableId>,
}

/// The scheme that applies under `tags`.
pub fn scheme_for<'c>(content: &'c ContentSet, tags: &[TagId]) -> Option<&'c LocationScheme> {
    content
        .location_schemes
        .iter()
        .find(|s| s.when.evaluate(&|t| tags.contains(t)))
}

/// Where a Notable Location came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LocationSource {
    /// Rolled: one of the Notable Location Count.
    Rolled,
    /// Added by the Bare Minimum Buildings setting, on top of the count.
    BareMinimumBuildings,
}

/// A roll on a count table and the number it gave.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CountRoll {
    /// Table, dice, every die and the result.
    pub roll: TableRollRecord,
    pub count: u32,
}

/// A district of a settlement mapped by district.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct District {
    /// Stable, e.g. `district-1`.
    pub id: String,
    pub label: String,
}

/// The rolled Notable Location Count of the settlement, or of one district.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocationArea {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub district: Option<District>,
    pub count: CountRoll,
}

/// One Notable Location.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NotableLocation {
    /// Stable, never derived from display text.
    pub id: String,
    pub source: LocationSource,
    /// The district it belongs to; none for an undistricted settlement, and
    /// none (not yet placed) for the settlement-level guaranteed Tavern.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub district: Option<String>,
    /// The parent Type: what generated the subtype.
    #[serde(rename = "type")]
    pub location_type: Named<ResultId>,
    /// The Type roll; absent for the guaranteed Tavern, which is not rolled.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub type_roll: Option<TableRollRecord>,
    /// The canonical subtype (e.g. `locationSubtype.jeweller`), named with
    /// its label, at the time, in the table that gave it (e.g. "Jeweller /
    /// Goldsmith"). None for a Type without subtypes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subtype: Option<Named<ResultId>>,
    /// Every roll that found the subtype. Absent for the guaranteed Tavern,
    /// whose subtype is not rolled.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subtype_rolls: Option<SubtypeRolls>,
    /// The subtype's speciality, e.g. a Magic Store's "Weapons".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub speciality: Option<Named<ResultId>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub speciality_roll: Option<TableRollRecord>,
    /// A shift of the location's Quality, waiting for Quality to be
    /// generated (e.g. +1 step for a Specialist Artisan).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quality_shift: Option<QualityShift>,
    /// The subtype's planned capabilities when it was generated (e.g.
    /// structural Keep upgrades). Nothing uses them yet.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub capabilities: Vec<Named<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spare_rooms: Option<u32>,
}

impl NotableLocation {
    fn clear_subtype(&mut self) {
        self.subtype = None;
        self.subtype_rolls = None;
        self.speciality = None;
        self.speciality_roll = None;
        self.quality_shift = None;
        self.capabilities.clear();
    }
}

/// How a rolled location's subtype was found.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubtypeRolls {
    /// The roll (or choice) on the Type's subtype table, e.g. Craft /
    /// Workshop 20: Specialist Artisan.
    pub roll: TableRollRecord,
    /// The same table rolled again because `roll` was a reroll result, in
    /// order. Each but the last is that result again.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub rerolls: Vec<TableRollRecord>,
    /// The roll that split the result, e.g. Weaponsmith / Armoursmith 1d4.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub split: Option<TableRollRecord>,
}

impl SubtypeRolls {
    /// The subtype table roll the subtype comes from: the last reroll, if
    /// any, else the first roll.
    pub fn resolved(&self) -> &TableRollRecord {
        self.rerolls.last().unwrap_or(&self.roll)
    }
}

/// What can be rolled or chosen again on a rolled location. Each detail
/// re-rolls everything that follows from it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LocationDetail {
    Type,
    /// The roll on the Type's subtype table.
    Subtype,
    /// What a reroll result (Specialist Artisan) becomes.
    Reroll,
    /// The roll deciding a split result (Weaponsmith / Armoursmith).
    Split,
    Speciality,
}

impl fmt::Display for LocationDetail {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            LocationDetail::Type => "Type",
            LocationDetail::Subtype => "subtype",
            LocationDetail::Reroll => "subtype reroll",
            LocationDetail::Split => "subtype split",
            LocationDetail::Speciality => "speciality",
        })
    }
}

/// The settlement's Notable Locations, with every roll: the record saved
/// with the settlement.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NotableLocations {
    /// The Size they were generated for.
    pub size: Named<ResultId>,
    pub scheme: Named<LocationSchemeId>,
    /// The District Count roll, for settlements mapped by district.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub districts: Option<CountRoll>,
    /// One rolled count for the settlement, or one per district.
    pub areas: Vec<LocationArea>,
    /// The guaranteed Tavern first (if any), then the rolled locations by
    /// area.
    pub locations: Vec<NotableLocation>,
    /// The highest rolled-location number issued, so a removed location's ID
    /// is never given to another.
    #[serde(default)]
    pub issued: usize,
    pub app_version: String,
}

impl NotableLocations {
    /// The rolled Notable Location Count: the sum over the areas. The
    /// guaranteed Tavern is not part of it.
    pub fn rolled_count(&self) -> u32 {
        self.areas.iter().map(|a| a.count.count).sum()
    }

    /// The location added by Bare Minimum Buildings, if any.
    pub fn guaranteed(&self) -> Option<&NotableLocation> {
        self.locations
            .iter()
            .find(|l| l.source == LocationSource::BareMinimumBuildings)
    }

    fn rolled_in<'a>(&'a self, district: Option<&'a str>) -> impl Iterator<Item = usize> + 'a {
        self.locations.iter().enumerate().filter_map(move |(i, l)| {
            (l.source == LocationSource::Rolled && l.district.as_deref() == district).then_some(i)
        })
    }

    /// Issues the next rolled-location number (IDs are never reused).
    fn next_number(&mut self) -> usize {
        let listed = self
            .locations
            .iter()
            .filter_map(|l| {
                l.id.strip_prefix("notable-location-")?
                    .parse::<usize>()
                    .ok()
            })
            .max()
            .unwrap_or(0);
        self.issued = self.issued.max(listed) + 1;
        self.issued
    }
}

/// Rolls Notable Locations with shared content and physical dice.
pub(crate) struct LocationGenerator<'g, 'c> {
    pub generator: &'g Generator<'c>,
    pub content: &'c ContentSet,
    /// Tags active in the settlement before the locations are generated.
    pub settlement_tags: &'g [TagId],
}

impl LocationGenerator<'_, '_> {
    /// The scheme that applies, or an error naming the Size.
    pub fn scheme(&self, size: &Named<ResultId>) -> Result<&LocationScheme, GenerationError> {
        scheme_for(self.content, self.settlement_tags).ok_or_else(|| {
            GenerationError::NoLocationScheme {
                size: size.name.clone(),
            }
        })
    }

    /// Every count, Type and subtype, then (once, after every district) the
    /// guaranteed Tavern / Inn if enabled.
    pub fn generate(
        &self,
        size: &Named<ResultId>,
        bare_minimum_buildings: bool,
        faces: &mut dyn FaceSource,
    ) -> Result<NotableLocations, GenerationError> {
        let scheme = self.scheme(size)?;
        let mut record = NotableLocations {
            size: size.clone(),
            scheme: Named::new(&scheme.id, &scheme.name),
            districts: None,
            areas: Vec::new(),
            locations: Vec::new(),
            issued: 0,
            app_version: VERSION.to_owned(),
        };
        match &scheme.districts {
            Some(table) => {
                let districts = self.count(table, None, faces)?;
                let n = districts.count as usize;
                record.districts = Some(districts);
                for i in 0..n {
                    self.add_area(&mut record, Some(i), faces)?;
                }
            }
            None => self.add_area(&mut record, None, faces)?,
        }
        self.set_bare_minimum(&mut record, bare_minimum_buildings)?;
        Ok(record)
    }

    /// Rolls (or chooses) the District Count again, keeping the districts
    /// that remain and rolling any new ones.
    pub fn set_districts(
        &self,
        record: &mut NotableLocations,
        choice: Option<ResultId>,
        faces: &mut dyn FaceSource,
    ) -> Result<(), GenerationError> {
        let scheme = self.scheme(&record.size)?;
        let table = scheme
            .districts
            .as_ref()
            .ok_or(GenerationError::NoDistricts)?;
        let districts = self.count(table, choice, faces)?;
        let n = districts.count as usize;
        record.districts = Some(districts);
        while record.areas.len() > n {
            let area = record.areas.pop().expect("more areas than districts");
            let gone = area.district.map(|d| d.id);
            record
                .locations
                .retain(|l| l.source != LocationSource::Rolled || l.district != gone);
        }
        while record.areas.len() < n {
            let i = record.areas.len();
            self.add_area(record, Some(i), faces)?;
        }
        record.app_version = VERSION.to_owned();
        Ok(())
    }

    /// Rolls (or chooses) one area's Notable Location Count again, keeping
    /// its locations up to the new count and rolling any new ones.
    pub fn set_count(
        &self,
        record: &mut NotableLocations,
        area: usize,
        choice: Option<ResultId>,
        faces: &mut dyn FaceSource,
    ) -> Result<(), GenerationError> {
        let scheme = self.scheme(&record.size)?;
        let existing = record
            .areas
            .get(area)
            .ok_or(GenerationError::NoSuchArea { index: area })?;
        let district = existing.district.clone();
        let count = self.count(&scheme.count, choice, faces)?;
        let n = count.count as usize;
        record.areas[area].count = count;
        let id = district.as_ref().map(|d| d.id.clone());
        let mine: Vec<usize> = record.rolled_in(id.as_deref()).collect();
        for &i in mine.iter().skip(n).rev() {
            record.locations.remove(i);
        }
        for _ in mine.len()..n {
            let location = self.rolled_location(record, id.clone(), faces)?;
            record.locations.push(location);
        }
        record.app_version = VERSION.to_owned();
        Ok(())
    }

    /// Rolls (or chooses) one detail of a rolled location again, then
    /// everything that follows from it: a new Type rolls a new subtype, a
    /// new subtype its rerolls, split and speciality, and so on. A detail
    /// that does not apply to the location (a speciality for a General
    /// Smithy) is an error.
    pub fn set_detail(
        &self,
        record: &mut NotableLocations,
        index: usize,
        detail: LocationDetail,
        choice: Option<ResultId>,
        faces: &mut dyn FaceSource,
    ) -> Result<(), GenerationError> {
        let location = record
            .locations
            .get_mut(index)
            .ok_or(GenerationError::NoSuchLocation { index })?;
        if location.source != LocationSource::Rolled {
            return Err(GenerationError::GuaranteedLocationNotRolled);
        }
        if !self.applies(location, detail) {
            return Err(GenerationError::NoLocationDetail { index, detail });
        }
        match detail {
            LocationDetail::Type => {
                let roll = self.roll(&TableId::new(TYPE_TABLE), choice, faces)?;
                location.location_type = roll.result.clone();
                location.type_roll = Some(roll);
                self.roll_subtype(location, None, faces)?;
            }
            LocationDetail::Subtype => self.roll_subtype(location, choice, faces)?,
            LocationDetail::Reroll => self.roll_rerolls(location, choice, faces)?,
            LocationDetail::Split => self.roll_split(location, choice, faces)?,
            LocationDetail::Speciality => self.roll_speciality(location, choice, faces)?,
        }
        record.app_version = VERSION.to_owned();
        Ok(())
    }

    /// Adds the guaranteed Tavern / Inn if enabled and absent; removes it
    /// (and only it) if disabled. Rolled locations are never touched.
    pub fn set_bare_minimum(
        &self,
        record: &mut NotableLocations,
        enabled: bool,
    ) -> Result<(), GenerationError> {
        let present = record.guaranteed().is_some();
        if enabled && !present {
            let hospitality = self
                .content
                .table(&TableId::new(TYPE_TABLE))
                .and_then(|t| t.result(&ResultId::new(HOSPITALITY_TYPE)))
                .ok_or_else(|| GenerationError::UnknownTable(TableId::new(TYPE_TABLE)))?;
            // Assigned, not rolled: labelled as the Hospitality table labels
            // it, else by its canonical name.
            let tavern = ResultId::new(TAVERN_INN_SUBTYPE);
            let label = subtype_table(self.content, &hospitality.id)
                .and_then(|t| self.content.table(t))
                .and_then(|t| t.result(&tavern))
                .map(|r| r.name.clone())
                .or_else(|| subtype(self.content, &tavern).map(|s| s.name.clone()))
                .ok_or_else(|| GenerationError::UnknownLocationSubtype {
                    subtype: tavern.clone(),
                })?;
            record.locations.insert(
                0,
                NotableLocation {
                    id: GUARANTEED_TAVERN_ID.to_owned(),
                    source: LocationSource::BareMinimumBuildings,
                    district: None,
                    location_type: Named::new(&hospitality.id, &hospitality.name),
                    type_roll: None,
                    capabilities: self.capabilities(&tavern),
                    subtype: Some(Named::new(&tavern, &label)),
                    subtype_rolls: None,
                    speciality: None,
                    speciality_roll: None,
                    quality_shift: None,
                    spare_rooms: Some(TAVERN_SPARE_ROOMS),
                },
            );
        } else if !enabled && present {
            record
                .locations
                .retain(|l| l.source != LocationSource::BareMinimumBuildings);
        }
        Ok(())
    }

    /// For the view: the District Count, count and Type tables with their
    /// results under the current tags, and for each location of `record`
    /// the tables of the details that apply to it.
    pub fn options(
        &self,
        size: Option<&Named<ResultId>>,
        record: Option<&NotableLocations>,
    ) -> LocationOptions {
        let state = State::with_tags(self.settlement_tags);
        let table = |id: &TableId| {
            self.generator
                .options_for(id, &state)
                .ok()
                .map(|(table, options)| TableOptions { table, options })
        };
        let scheme = size.and_then(|s| self.scheme(s).ok());
        let details = record
            .map(|r| {
                r.locations
                    .iter()
                    .map(|l| self.detail_options(l, &table))
                    .collect()
            })
            .unwrap_or_default();
        LocationOptions {
            districts: scheme.and_then(|s| s.districts.as_ref()).and_then(table),
            count: scheme.and_then(|s| table(&s.count)),
            types: table(&TableId::new(TYPE_TABLE)),
            details,
        }
    }

    /// The tables of the details that apply to `location` now.
    fn detail_options(
        &self,
        location: &NotableLocation,
        table: &dyn Fn(&TableId) -> Option<TableOptions>,
    ) -> LocationDetailOptions {
        if location.source != LocationSource::Rolled {
            return LocationDetailOptions::default();
        }
        let applies = |detail| self.applies(location, detail);
        let rolls = location.subtype_rolls.as_ref();
        LocationDetailOptions {
            subtype: subtype_table(self.content, &location.location_type.id).and_then(table),
            // What the reroll becomes: never the reroll result again.
            reroll: rolls
                .filter(|_| applies(LocationDetail::Reroll))
                .and_then(|r| table(&r.roll.table.id))
                .map(|mut t| {
                    t.options
                        .retain(|o| self.reroll_shift(&o.result.id).is_none());
                    t
                }),
            split: rolls
                .filter(|_| applies(LocationDetail::Split))
                .and_then(|r| self.split_table(&r.resolved().result.id))
                .and_then(table),
            speciality: location
                .subtype
                .as_ref()
                .filter(|_| applies(LocationDetail::Speciality))
                .and_then(|s| self.speciality_table(&s.id))
                .and_then(table),
        }
    }

    /// Whether `detail` can be rolled for the rolled `location` now.
    fn applies(&self, location: &NotableLocation, detail: LocationDetail) -> bool {
        let rolls = location.subtype_rolls.as_ref();
        match detail {
            LocationDetail::Type => true,
            LocationDetail::Subtype => {
                subtype_table(self.content, &location.location_type.id).is_some()
            }
            LocationDetail::Reroll => {
                rolls.is_some_and(|r| self.reroll_shift(&r.roll.result.id).is_some())
            }
            LocationDetail::Split => {
                rolls.is_some_and(|r| self.split_table(&r.resolved().result.id).is_some())
            }
            LocationDetail::Speciality => location
                .subtype
                .as_ref()
                .is_some_and(|s| self.speciality_table(&s.id).is_some()),
        }
    }

    /// Rolls (or chooses) the location's subtype on its Type's subtype
    /// table, then everything that follows. A Type without a subtype table
    /// has no subtype.
    fn roll_subtype(
        &self,
        location: &mut NotableLocation,
        choice: Option<ResultId>,
        faces: &mut dyn FaceSource,
    ) -> Result<(), GenerationError> {
        location.clear_subtype();
        let Some(table) = subtype_table(self.content, &location.location_type.id) else {
            return Ok(());
        };
        location.subtype_rolls = Some(SubtypeRolls {
            roll: self.roll(table, choice, faces)?,
            rerolls: Vec::new(),
            split: None,
        });
        self.roll_rerolls(location, None, faces)
    }

    /// After the first subtype roll: while a reroll result (Specialist
    /// Artisan) comes up, rolls the same table again, recording every roll.
    /// Its Quality shift is given once, however often it repeats. A chosen
    /// result replaces the rolls and cannot be the reroll result itself.
    /// Then the split and speciality.
    fn roll_rerolls(
        &self,
        location: &mut NotableLocation,
        choice: Option<ResultId>,
        faces: &mut dyn FaceSource,
    ) -> Result<(), GenerationError> {
        let rolls = location
            .subtype_rolls
            .as_mut()
            .expect("the subtype was rolled");
        rolls.rerolls.clear();
        location.quality_shift = None;
        let first = rolls.roll.result.clone();
        if let Some(steps) = self.reroll_shift(&first.id) {
            let table = rolls.roll.table.id.clone();
            match choice {
                Some(choice) if self.reroll_shift(&choice).is_some() => {
                    return Err(GenerationError::RerollChoice {
                        table,
                        result: choice,
                    });
                }
                Some(choice) => rolls.rerolls.push(self.roll(&table, Some(choice), faces)?),
                None => loop {
                    if rolls.rerolls.len() >= MAX_REROLLS {
                        return Err(GenerationError::RerollLimit { table });
                    }
                    let roll = self.roll(&table, None, faces)?;
                    let again = self.reroll_shift(&roll.result.id).is_some();
                    rolls.rerolls.push(roll);
                    if !again {
                        break;
                    }
                },
            }
            location.quality_shift = Some(QualityShift {
                steps,
                source: first,
            });
        }
        self.roll_split(location, None, faces)
    }

    /// Splits the subtype table's result if it needs it (Weaponsmith /
    /// Armoursmith → Weaponsmith or Armoursmith), sets the final subtype,
    /// then rolls its speciality.
    fn roll_split(
        &self,
        location: &mut NotableLocation,
        choice: Option<ResultId>,
        faces: &mut dyn FaceSource,
    ) -> Result<(), GenerationError> {
        let rolls = location
            .subtype_rolls
            .as_mut()
            .expect("the subtype was rolled");
        rolls.split = None;
        let resolved = rolls.resolved().result.clone();
        if let Some(table) = self.split_table(&resolved.id) {
            rolls.split = Some(self.roll(table, choice, faces)?);
        }
        let subtype = rolls.split.as_ref().map_or(resolved, |s| s.result.clone());
        location.capabilities = self.capabilities(&subtype.id);
        location.subtype = Some(subtype);
        self.roll_speciality(location, None, faces)
    }

    /// Rolls (or chooses) the subtype's speciality, if it has one.
    fn roll_speciality(
        &self,
        location: &mut NotableLocation,
        choice: Option<ResultId>,
        faces: &mut dyn FaceSource,
    ) -> Result<(), GenerationError> {
        location.speciality = None;
        location.speciality_roll = None;
        let table = location
            .subtype
            .as_ref()
            .and_then(|s| self.speciality_table(&s.id));
        if let Some(table) = table {
            let roll = self.roll(table, choice, faces)?;
            location.speciality = Some(roll.result.clone());
            location.speciality_roll = Some(roll);
        }
        Ok(())
    }

    /// The Quality shift of `id`, if it is a reroll result.
    fn reroll_shift(&self, id: &ResultId) -> Option<u8> {
        match subtype(self.content, id)?.resolve.as_ref()? {
            SubtypeResolution::Reroll { quality_shift } => Some(*quality_shift),
            SubtypeResolution::Split { .. } => None,
        }
    }

    /// The table that splits `id`, if it is a split result.
    fn split_table(&self, id: &ResultId) -> Option<&TableId> {
        match subtype(self.content, id)?.resolve.as_ref()? {
            SubtypeResolution::Split { table } => Some(table),
            SubtypeResolution::Reroll { .. } => None,
        }
    }

    fn speciality_table(&self, id: &ResultId) -> Option<&TableId> {
        subtype(self.content, id)?.speciality.as_ref()
    }

    fn capabilities(&self, id: &ResultId) -> Vec<Named<String>> {
        subtype(self.content, id)
            .map(|s| {
                s.capabilities
                    .iter()
                    .map(|c| Named::new(&c.id, &c.name))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Rolls the count for a new area (district `index`, or the whole
    /// settlement) and that many locations.
    fn add_area(
        &self,
        record: &mut NotableLocations,
        district: Option<usize>,
        faces: &mut dyn FaceSource,
    ) -> Result<(), GenerationError> {
        let scheme = self.scheme(&record.size)?;
        let district = district.map(|i| District {
            id: format!("district-{}", i + 1),
            label: format!("District {}", i + 1),
        });
        let count = self.count(&scheme.count, None, faces)?;
        let n = count.count;
        let id = district.as_ref().map(|d| d.id.clone());
        record.areas.push(LocationArea { district, count });
        for _ in 0..n {
            let location = self.rolled_location(record, id.clone(), faces)?;
            record.locations.push(location);
        }
        Ok(())
    }

    fn rolled_location(
        &self,
        record: &mut NotableLocations,
        district: Option<String>,
        faces: &mut dyn FaceSource,
    ) -> Result<NotableLocation, GenerationError> {
        let roll = self.roll(&TableId::new(TYPE_TABLE), None, faces)?;
        let mut location = NotableLocation {
            id: format!("notable-location-{:02}", record.next_number()),
            source: LocationSource::Rolled,
            district,
            location_type: roll.result.clone(),
            type_roll: Some(roll),
            subtype: None,
            subtype_rolls: None,
            speciality: None,
            speciality_roll: None,
            quality_shift: None,
            capabilities: Vec::new(),
            spare_rooms: None,
        };
        self.roll_subtype(&mut location, None, faces)?;
        Ok(location)
    }

    /// A count: the rolled result's value.
    fn count(
        &self,
        table: &TableId,
        choice: Option<ResultId>,
        faces: &mut dyn FaceSource,
    ) -> Result<CountRoll, GenerationError> {
        let roll = self.roll(table, choice, faces)?;
        let count = roll
            .value
            .and_then(|v| u32::try_from(v).ok())
            .filter(|v| *v > 0)
            .ok_or_else(|| GenerationError::CountWithoutValue {
                table: roll.table.id.clone(),
            })?;
        Ok(CountRoll { roll, count })
    }

    fn roll(
        &self,
        table: &TableId,
        choice: Option<ResultId>,
        faces: &mut dyn FaceSource,
    ) -> Result<TableRollRecord, GenerationError> {
        let mut state = State::with_tags(self.settlement_tags);
        self.generator.process(
            &mut state,
            [Request::new(table.clone()).with_choice(choice)].into(),
            &Selections::new(),
            faces,
        )?;
        state
            .take_events()
            .into_iter()
            .find_map(|e| match e {
                GenerationEvent::TableRolled(r) if r.routed_by.is_none() => Some(*r),
                _ => None,
            })
            .ok_or_else(|| GenerationError::FigureTableSkipped {
                table: table.clone(),
            })
    }
}

/// A table and its results under the current tags.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TableOptions {
    pub table: Named<TableId>,
    pub options: Vec<ResultOption>,
}

/// The tables the Notable Locations stage rolls, for choosing results.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocationOptions {
    /// The District Count table, for settlements mapped by district.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub districts: Option<TableOptions>,
    /// The Notable Location Count table (per district where there are
    /// districts).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub count: Option<TableOptions>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub types: Option<TableOptions>,
    /// For each location, in order, the details that apply to it.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub details: Vec<LocationDetailOptions>,
}

/// The tables of one location's details, only those that apply to it: a
/// Magic Store Speciality only for a Magic Store, a reroll only after a
/// Specialist Artisan. Empty for the guaranteed Tavern / Inn.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocationDetailOptions {
    /// The Type's subtype table.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subtype: Option<TableOptions>,
    /// The subtype table again, without the results that roll again: what
    /// a reroll result (Specialist Artisan) becomes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reroll: Option<TableOptions>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub split: Option<TableOptions>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub speciality: Option<TableOptions>,
}
