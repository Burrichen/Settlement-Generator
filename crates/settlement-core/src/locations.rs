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
//! **Bare Minimum Buildings** (a setting) adds exactly one more location to
//! the whole settlement, never one per district: a Tavern with 4 spare
//! rooms. It is additive (it never takes a rolled location's place), its
//! type is not rolled, and its [`LocationSource`] tells it apart from a
//! rolled Hospitality location. Subtypes are future work; the Tavern's is
//! fixed by this rule.

use serde::{Deserialize, Serialize};

use crate::VERSION;
use crate::content::{Condition, ContentSet};
use crate::dice::FaceSource;
use crate::engine::{
    GenerationError, GenerationEvent, Generator, Named, Request, ResultOption, Selections, State,
    TableRollRecord,
};
use crate::ids::{LocationSchemeId, ResultId, TableId, TagId};

/// The stage table recorded in snapshots for the Notable Locations stage.
pub const LOCATIONS_STAGE_ID: &str = "stage.notableLocations";
/// Rolled once for each rolled Notable Location.
pub const TYPE_TABLE: &str = "table.notableLocationType";
/// The Bare Minimum Buildings Tavern's type, which must exist in the type
/// table.
pub const HOSPITALITY_TYPE: &str = "locationType.hospitality";
/// The Bare Minimum Buildings Tavern's fixed subtype.
pub const TAVERN_SUBTYPE: &str = "locationSubtype.tavern";
/// The Bare Minimum Buildings Tavern's stable location ID.
pub const GUARANTEED_TAVERN_ID: &str = "notable-location-guaranteed-tavern";
/// The Bare Minimum Buildings Tavern always has exactly this many.
pub const TAVERN_SPARE_ROOMS: u32 = 4;

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
    #[serde(rename = "type")]
    pub location_type: Named<ResultId>,
    /// The Type roll; absent for the guaranteed Tavern, which is not rolled.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub type_roll: Option<TableRollRecord>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subtype: Option<Named<ResultId>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spare_rooms: Option<u32>,
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

    /// Every count and type, then the guaranteed Tavern if enabled.
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

    /// Rolls (or chooses) one rolled location's Type again.
    pub fn set_type(
        &self,
        record: &mut NotableLocations,
        index: usize,
        choice: Option<ResultId>,
        faces: &mut dyn FaceSource,
    ) -> Result<(), GenerationError> {
        let location = record
            .locations
            .get(index)
            .ok_or(GenerationError::NoSuchLocation { index })?;
        if location.source != LocationSource::Rolled {
            return Err(GenerationError::GuaranteedLocationNotRolled);
        }
        let roll = self.roll(&TableId::new(TYPE_TABLE), choice, faces)?;
        let location = &mut record.locations[index];
        location.location_type = roll.result.clone();
        location.type_roll = Some(roll);
        record.app_version = VERSION.to_owned();
        Ok(())
    }

    /// Adds the guaranteed Tavern if enabled and absent; removes it (and
    /// only it) if disabled. Rolled locations are never touched.
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
            record.locations.insert(
                0,
                NotableLocation {
                    id: GUARANTEED_TAVERN_ID.to_owned(),
                    source: LocationSource::BareMinimumBuildings,
                    district: None,
                    location_type: Named::new(&hospitality.id, &hospitality.name),
                    type_roll: None,
                    subtype: Some(Named::new(&ResultId::new(TAVERN_SUBTYPE), "Tavern")),
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
    /// results under the current tags.
    pub fn options(&self, size: Option<&Named<ResultId>>) -> LocationOptions {
        let state = State::with_tags(self.settlement_tags);
        let table = |id: &TableId| self.generator.options_for(id, &state).ok();
        let scheme = size.and_then(|s| self.scheme(s).ok());
        LocationOptions {
            districts: scheme
                .and_then(|s| s.districts.as_ref())
                .and_then(table)
                .map(|(table, options)| TableOptions { table, options }),
            count: scheme
                .and_then(|s| table(&s.count))
                .map(|(table, options)| TableOptions { table, options }),
            types: table(&TableId::new(TYPE_TABLE))
                .map(|(table, options)| TableOptions { table, options }),
        }
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
        Ok(NotableLocation {
            id: format!("notable-location-{:02}", record.next_number()),
            source: LocationSource::Rolled,
            district,
            location_type: roll.result.clone(),
            type_roll: Some(roll),
            subtype: None,
            spare_rooms: None,
        })
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
}
