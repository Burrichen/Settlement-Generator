//! The Notable Location directory of a settlement: every location the map
//! screens can place on a building, with its name, marker number and the
//! building it stands on. See `docs/MAPS.md` §13.
//!
//! - **Settlement locations** come from the saved snapshot (rolled by the
//!   Notable Locations stage, plus the Bare Minimum Buildings Tavern). The
//!   snapshot is never changed: everything the user sets is stored here,
//!   keyed by the snapshot's own stable ID. A future location generator
//!   connects by producing the same IDs.
//! - **Custom locations** are created by the user (`custom-location-N`).
//! - A location stands on at most one building, and a building holds at most
//!   one location. Moving or replacing is always an explicit choice.
//! - **Marker numbers** are issued once per location, from one sequence per
//!   settlement, the first time it is placed. They never change or get
//!   reused: unplacing or deleting leaves a gap.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use thiserror::Error;
use time::OffsetDateTime;

use crate::ids::{BuildingId, MapId, PoiId};
use crate::locations::{District, NotableLocation, NotableLocations};
use crate::settlements::{Settlement, now};

use super::geometry::Point;
use super::model::{
    DamagedRecord, MapLegendEntry, MapMarker, MapScope, MarkerTarget, PointOfInterest,
    SettlementMap, SettlementMapSet,
};

pub const CUSTOM_PREFIX: &str = "custom-location-";
pub const MAX_LOCATION_NAME_CHARS: usize = 80;
pub const MAX_LOCATION_TYPE_CHARS: usize = 80;
pub const MAX_LOCATION_NOTES_CHARS: usize = 5000;
pub const MAX_POI_LABEL_CHARS: usize = 80;
pub const MAX_DISTRICT_NAME_CHARS: usize = 60;

/// The user's names for districts, by district ID.
pub type DistrictNames = BTreeMap<String, String>;

/// `district` under the user's name for it, if any.
pub fn named(district: &District, names: &DistrictNames) -> District {
    District {
        id: district.id.clone(),
        label: names
            .get(&district.id)
            .cloned()
            .unwrap_or_else(|| district.label.clone()),
    }
}

/// Shows a map set under the user's district names: each district map's
/// label and district. Only the shown copy changes; stored maps keep the
/// names they were drawn with, and every ID stays the same.
pub fn apply_district_names(set: &mut SettlementMapSet, names: &DistrictNames) {
    for map in &mut set.maps {
        if let MapScope::District { district } = &mut map.scope {
            *district = named(district, names);
            map.label = district.label.clone();
        }
    }
}

/// Where a location comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LocationOrigin {
    /// The settlement's own Notable Locations (its saved snapshot).
    Settlement,
    /// Added by the user on the map screens.
    Custom,
}

/// The building a location stands on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocationAssignment {
    pub map_id: MapId,
    pub building_id: BuildingId,
    /// The district of that map, for district maps.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub district: Option<String>,
    #[serde(with = "time::serde::rfc3339")]
    pub assigned_at: OffsetDateTime,
}

/// What is stored for one location. A settlement location has a record
/// only once the user has set something; a custom one always has one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocationRecord {
    pub id: String,
    pub origin: LocationOrigin,
    /// Increases with every write.
    pub version: u32,
    /// The user's name; absent means the default name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// A custom location's type. Settlement locations take theirs from the
    /// snapshot.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub location_type: Option<String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub notes: String,
    /// Issued when first placed; kept for good.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub marker: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub assignment: Option<LocationAssignment>,
}

impl LocationRecord {
    fn new(id: &str, origin: LocationOrigin) -> Self {
        LocationRecord {
            id: id.to_owned(),
            origin,
            version: 0,
            name: None,
            location_type: None,
            notes: String::new(),
            marker: None,
            assignment: None,
        }
    }
}

/// Every stored location record and point of interest of a settlement,
/// and the sequences they share.
#[derive(Debug, Clone, PartialEq)]
pub struct LocationBook {
    pub records: BTreeMap<String, LocationRecord>,
    /// Points of interest by ID.
    pub pois: BTreeMap<String, PointOfInterest>,
    /// The next marker number to issue (from 1).
    pub next_marker: u32,
    /// The next custom location number to issue (from 1).
    pub next_custom: u32,
    /// Stored rows that could not be read: left untouched, and reported.
    pub damaged: Vec<DamagedRecord>,
}

impl Default for LocationBook {
    fn default() -> Self {
        LocationBook {
            records: BTreeMap::new(),
            pois: BTreeMap::new(),
            damaged: Vec::new(),
            next_marker: 1,
            next_custom: 1,
        }
    }
}

// ----- what interfaces show ------------------------------------------------------

/// Where a location stands, as shown.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocationPlacement {
    pub map_id: MapId,
    pub building_id: BuildingId,
    pub map_label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub district: Option<District>,
    /// The building is no longer on the saved maps (they were redrawn): the
    /// location must be placed again.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub missing: bool,
}

/// One location of the directory.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocationEntry {
    pub id: String,
    pub origin: LocationOrigin,
    /// The user's name, or the default.
    pub name: String,
    /// The name used when the user has set none.
    pub default_name: String,
    /// The user has named it.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub renamed: bool,
    pub location_type: String,
    /// The district the settlement records it in. A location with one can
    /// only stand on that district's map.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub district: Option<District>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub marker: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub placement: Option<LocationPlacement>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub notes: String,
    /// The stored version; 0 when nothing is stored yet.
    pub version: u32,
    /// A settlement location the settlement no longer has (it was edited
    /// since). Kept because the user set something on it.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub removed: bool,
}

/// A change to a location's name, type and notes.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocationEdit {
    /// Blank or absent: the default name.
    #[serde(default)]
    pub name: Option<String>,
    /// Custom locations only.
    #[serde(default)]
    pub location_type: Option<String>,
    #[serde(default)]
    pub notes: String,
}

/// Explicit consent for a placement that changes another placement.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssignOptions {
    /// The location stands on another building: move it here.
    #[serde(default)]
    pub move_location: bool,
    /// Another location stands on this building: take it off (it is kept,
    /// unplaced, with its number).
    #[serde(default)]
    pub replace_occupant: bool,
}

/// A building to place a new location on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlaceOn {
    pub map_id: MapId,
    pub building_id: BuildingId,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum LocationError {
    #[error("There is no Notable Location `{0}` in this settlement.")]
    Unknown(String),
    #[error("{0} is no longer one of this settlement's Notable Locations, so it cannot be placed.")]
    Removed(String),
    #[error("This settlement has no maps yet.")]
    NoMaps,
    #[error("There is no building `{building}` on the map `{map}`.")]
    UnknownBuilding { map: MapId, building: BuildingId },
    #[error("{location} belongs to {district}. Open the map of {district} to place it.")]
    WrongDistrict { location: String, district: String },
    #[error("{location} is already on {place}. Choose to move it if that is what you want.")]
    AlreadyPlaced { location: String, place: String },
    #[error("{building} already holds {occupant}. Choose to replace it if that is what you want.")]
    BuildingTaken { building: String, occupant: String },
    #[error("{0} is not placed on any building.")]
    NotPlaced(String),
    #[error("Only custom locations can be deleted; the settlement's own are kept.")]
    NotCustom,
    #[error("A settlement location's type comes from the settlement and cannot be changed.")]
    TypeFixed,
    #[error("{field} can be at most {max} characters.")]
    TooLong { field: &'static str, max: usize },
    #[error("{0} must be a single line.")]
    MultiLine(&'static str),
    #[error(
        "This location was changed elsewhere after it was opened. It has been reloaded; make \
         your change again."
    )]
    Changed,
    #[error("There is no map `{0}` on this settlement.")]
    UnknownMap(MapId),
    #[error("There is no point of interest `{0}` in this settlement.")]
    UnknownPoi(String),
    #[error("Give the point of interest a name.")]
    PoiNameRequired,
    #[error("A point of interest must be inside its map.")]
    PoiOutside,
}

// ----- the directory --------------------------------------------------------------

/// A snapshot location's default name: its subtype, or its type.
fn default_name(l: &NotableLocation) -> String {
    l.subtype
        .as_ref()
        .map_or_else(|| l.location_type.name.clone(), |s| s.name.clone())
}

/// "Trade / Shop (Magic Store · Weapons)": the Type, with the subtype and
/// any speciality as recorded.
fn type_label(l: &NotableLocation) -> String {
    let detail: Vec<&str> = [&l.subtype, &l.speciality]
        .into_iter()
        .flatten()
        .map(|n| n.name.as_str())
        .collect();
    if detail.is_empty() {
        l.location_type.name.clone()
    } else {
        format!("{} ({})", l.location_type.name, detail.join(" · "))
    }
}

fn district_of(locations: &NotableLocations, id: Option<&str>) -> Option<District> {
    let id = id?;
    locations
        .areas
        .iter()
        .filter_map(|a| a.district.as_ref())
        .find(|d| d.id == id)
        .cloned()
}

fn snapshot<'s>(settlement: &'s Settlement, id: &str) -> Option<&'s NotableLocation> {
    settlement
        .snapshot
        .notable_locations()?
        .locations
        .iter()
        .find(|l| l.id == id)
}

/// "Building 12, District 2".
fn building_label(set: Option<&SettlementMapSet>, a: &LocationAssignment) -> String {
    let number = a
        .building_id
        .as_str()
        .rsplit('-')
        .next()
        .and_then(|n| n.parse::<u32>().ok())
        .map_or_else(|| a.building_id.to_string(), |n| format!("Building {n}"));
    match set.and_then(|s| s.map(&a.map_id)) {
        Some(map) => format!("{number}, {}", map.label),
        None => number,
    }
}

fn placement(set: Option<&SettlementMapSet>, a: &LocationAssignment) -> LocationPlacement {
    let map = set.and_then(|s| s.map(&a.map_id));
    LocationPlacement {
        map_id: a.map_id.clone(),
        building_id: a.building_id.clone(),
        map_label: map.map_or_else(|| a.map_id.to_string(), |m| m.label.clone()),
        district: map.and_then(|m| m.district().cloned()),
        missing: map.and_then(|m| m.building(&a.building_id)).is_none(),
    }
}

/// Every location: the settlement's own in snapshot order, then custom ones
/// in the order they were made.
pub fn entries(
    settlement: &Settlement,
    set: Option<&SettlementMapSet>,
    book: &LocationBook,
    names: &DistrictNames,
) -> Vec<LocationEntry> {
    let from_snapshot = settlement.snapshot.notable_locations();
    let mut out: Vec<LocationEntry> = Vec::new();
    for l in from_snapshot.map(|n| n.locations.as_slice()).unwrap_or(&[]) {
        let record = book.records.get(&l.id);
        let default = default_name(l);
        out.push(LocationEntry {
            id: l.id.clone(),
            origin: LocationOrigin::Settlement,
            name: record
                .and_then(|r| r.name.clone())
                .unwrap_or_else(|| default.clone()),
            renamed: record.is_some_and(|r| r.name.is_some()),
            default_name: default,
            location_type: type_label(l),
            district: from_snapshot
                .and_then(|n| district_of(n, l.district.as_deref()))
                .map(|d| named(&d, names)),
            marker: record.and_then(|r| r.marker),
            placement: record
                .and_then(|r| r.assignment.as_ref())
                .map(|a| placement(set, a)),
            notes: record.map(|r| r.notes.clone()).unwrap_or_default(),
            version: record.map_or(0, |r| r.version),
            removed: false,
        });
    }
    let mut rest: Vec<&LocationRecord> = book
        .records
        .values()
        .filter(|r| !out.iter().any(|e| e.id == r.id))
        .collect();
    rest.sort_by_key(|r| {
        (
            r.origin == LocationOrigin::Custom,
            custom_number(&r.id).unwrap_or(0),
            r.id.clone(),
        )
    });
    for r in rest {
        let custom = r.origin == LocationOrigin::Custom;
        let default = if custom {
            r.location_type
                .clone()
                .unwrap_or_else(|| "Custom location".into())
        } else {
            r.id.clone()
        };
        out.push(LocationEntry {
            id: r.id.clone(),
            origin: r.origin,
            name: r.name.clone().unwrap_or_else(|| default.clone()),
            renamed: r.name.is_some(),
            default_name: default,
            location_type: r.location_type.clone().unwrap_or_else(|| "Custom".into()),
            district: None,
            marker: r.marker,
            placement: r.assignment.as_ref().map(|a| placement(set, a)),
            notes: r.notes.clone(),
            version: r.version,
            removed: !custom,
        });
    }
    out
}

fn custom_number(id: &str) -> Option<u32> {
    id.strip_prefix(CUSTOM_PREFIX)?.parse().ok()
}

/// The numbered markers of one map and its legend: every location placed
/// on a building of this map, then user tokens, by marker number. Numbers
/// are the persistent ones, so the legend and the markers always agree and
/// adding a marker never renumbers another.
pub fn markers(
    set: &SettlementMapSet,
    map_id: &MapId,
    locations: &[LocationEntry],
    pois: &[PoiEntry],
) -> (Vec<MapMarker>, Vec<MapLegendEntry>) {
    let Some(map) = set.map(map_id) else {
        return (Vec::new(), Vec::new());
    };
    let mut items: Vec<(MapMarker, MapLegendEntry)> = Vec::new();
    for entry in locations {
        let (Some(number), Some(p)) = (entry.marker, &entry.placement) else {
            continue;
        };
        if &p.map_id != map_id {
            continue;
        }
        let Some(building) = map.building(&p.building_id) else {
            continue;
        };
        let target = MarkerTarget::NotableLocation {
            location_id: entry.id.clone(),
            building_id: building.id.clone(),
        };
        items.push((
            MapMarker {
                number,
                map_id: map_id.clone(),
                position: building.centroid,
                target: target.clone(),
            },
            MapLegendEntry {
                number,
                label: entry.name.clone(),
                location_type: Some(entry.location_type.clone()),
                district: map.district().cloned(),
                target,
            },
        ));
    }
    for poi in pois.iter().filter(|p| &p.map_id == map_id) {
        let target = MarkerTarget::PointOfInterest {
            poi_id: poi.id.clone(),
        };
        items.push((
            MapMarker {
                number: poi.number,
                map_id: map_id.clone(),
                position: poi.position,
                target: target.clone(),
            },
            MapLegendEntry {
                number: poi.number,
                label: poi.label.clone(),
                location_type: None,
                district: map.district().cloned(),
                target,
            },
        ));
    }
    items.sort_by_key(|(m, _)| m.number);
    items.into_iter().unzip()
}

// ----- changes -----------------------------------------------------------------------

/// The display name of a location, for messages.
fn name_of(settlement: &Settlement, book: &LocationBook, id: &str) -> String {
    book.records
        .get(id)
        .and_then(|r| r.name.clone())
        .or_else(|| snapshot(settlement, id).map(default_name))
        .or_else(|| book.records.get(id).and_then(|r| r.location_type.clone()))
        .unwrap_or_else(|| id.to_owned())
}

/// The record of a location that exists (in the snapshot or as a custom
/// record), created if the location had none.
fn record_mut<'b>(
    settlement: &Settlement,
    book: &'b mut LocationBook,
    id: &str,
) -> Result<&'b mut LocationRecord, LocationError> {
    if snapshot(settlement, id).is_some() {
        return Ok(book
            .records
            .entry(id.to_owned())
            .or_insert_with(|| LocationRecord::new(id, LocationOrigin::Settlement)));
    }
    match book.records.get_mut(id) {
        Some(r) if r.origin == LocationOrigin::Custom => Ok(r),
        Some(_) => Err(LocationError::Removed(id.to_owned())),
        None => Err(LocationError::Unknown(id.to_owned())),
    }
}

/// Places a location on a building. A location already standing elsewhere
/// is only moved, and a building's other location only taken off, when the
/// options say so. The building's details are not touched.
pub fn assign(
    settlement: &Settlement,
    set: &SettlementMapSet,
    book: &mut LocationBook,
    location_id: &str,
    target: &PlaceOn,
    options: AssignOptions,
) -> Result<(), LocationError> {
    let map = locate(set, target)?;
    let name = name_of(settlement, book, location_id);
    // A settlement location belongs to its recorded district.
    if let Some(l) = snapshot(settlement, location_id) {
        if let Some(district) = &l.district {
            if map.district().map(|d| &d.id) != Some(district) {
                let label = settlement
                    .snapshot
                    .notable_locations()
                    .and_then(|n| district_of(n, Some(district)))
                    .map_or_else(|| district.clone(), |d| d.label);
                return Err(LocationError::WrongDistrict {
                    location: name,
                    district: label,
                });
            }
        }
    }
    let here =
        |a: &LocationAssignment| a.map_id == target.map_id && a.building_id == target.building_id;
    // Where it stands now (a building that still exists).
    let current = record_mut(settlement, book, location_id)?
        .assignment
        .clone();
    if let Some(a) = &current {
        if here(a) {
            return Ok(());
        }
        let exists = set
            .map(&a.map_id)
            .and_then(|m| m.building(&a.building_id))
            .is_some();
        if exists && !options.move_location {
            return Err(LocationError::AlreadyPlaced {
                location: name,
                place: building_label(Some(set), a),
            });
        }
    }
    let occupant = book
        .records
        .values()
        .find(|r| r.id != location_id && r.assignment.as_ref().is_some_and(here))
        .map(|r| r.id.clone());
    if let Some(occupant) = occupant {
        if !options.replace_occupant {
            let building = building_label(
                Some(set),
                &LocationAssignment {
                    map_id: target.map_id.clone(),
                    building_id: target.building_id.clone(),
                    district: None,
                    assigned_at: now(),
                },
            );
            return Err(LocationError::BuildingTaken {
                building,
                occupant: name_of(settlement, book, &occupant),
            });
        }
        if let Some(r) = book.records.get_mut(&occupant) {
            r.assignment = None;
        }
    }
    let next = book.next_marker;
    let record = record_mut(settlement, book, location_id)?;
    record.assignment = Some(LocationAssignment {
        map_id: target.map_id.clone(),
        building_id: target.building_id.clone(),
        district: map.district().map(|d| d.id.clone()),
        assigned_at: now(),
    });
    if record.marker.is_none() {
        record.marker = Some(next);
        book.next_marker += 1;
    }
    Ok(())
}

/// Takes a location off its building. Its record, name, notes and marker
/// number are kept, and so are the building's details.
pub fn unassign(
    settlement: &Settlement,
    book: &mut LocationBook,
    location_id: &str,
) -> Result<(), LocationError> {
    let name = name_of(settlement, book, location_id);
    let record = book
        .records
        .get_mut(location_id)
        .filter(|r| r.assignment.is_some())
        .ok_or(LocationError::NotPlaced(name))?;
    record.assignment = None;
    Ok(())
}

fn clean_line(
    field: &'static str,
    max: usize,
    text: Option<String>,
) -> Result<Option<String>, LocationError> {
    let Some(text) = text else { return Ok(None) };
    let text = text.trim();
    if text.contains('\n') {
        return Err(LocationError::MultiLine(field));
    }
    if text.chars().count() > max {
        return Err(LocationError::TooLong { field, max });
    }
    Ok((!text.is_empty()).then(|| text.to_owned()))
}

fn cleaned(edit: LocationEdit) -> Result<LocationEdit, LocationError> {
    let notes = edit.notes.replace("\r\n", "\n").trim().to_owned();
    if notes.chars().count() > MAX_LOCATION_NOTES_CHARS {
        return Err(LocationError::TooLong {
            field: "Notes",
            max: MAX_LOCATION_NOTES_CHARS,
        });
    }
    Ok(LocationEdit {
        name: clean_line("Name", MAX_LOCATION_NAME_CHARS, edit.name)?,
        location_type: clean_line("Type", MAX_LOCATION_TYPE_CHARS, edit.location_type)?,
        notes,
    })
}

/// Renames a location and sets its notes (and a custom location's type).
/// `version` is the version the change was made from.
pub fn update(
    settlement: &Settlement,
    book: &mut LocationBook,
    location_id: &str,
    version: u32,
    edit: LocationEdit,
) -> Result<(), LocationError> {
    let edit = cleaned(edit)?;
    let stored = book.records.get(location_id).map_or(0, |r| r.version);
    if stored != version {
        return Err(LocationError::Changed);
    }
    let record = record_mut(settlement, book, location_id)?;
    if record.origin == LocationOrigin::Settlement && edit.location_type.is_some() {
        return Err(LocationError::TypeFixed);
    }
    record.name = edit.name;
    record.notes = edit.notes;
    if record.origin == LocationOrigin::Custom {
        record.location_type = edit.location_type;
    }
    Ok(())
}

/// Adds a custom location, placed on a building if `place` is given.
/// Returns its ID.
pub fn create(
    settlement: &Settlement,
    set: Option<&SettlementMapSet>,
    book: &mut LocationBook,
    edit: LocationEdit,
    place: Option<&PlaceOn>,
    options: AssignOptions,
) -> Result<String, LocationError> {
    let edit = cleaned(edit)?;
    let id = format!("{CUSTOM_PREFIX}{}", book.next_custom);
    book.next_custom += 1;
    let mut record = LocationRecord::new(&id, LocationOrigin::Custom);
    record.name = edit.name;
    record.location_type = edit.location_type;
    record.notes = edit.notes;
    book.records.insert(id.clone(), record);
    if let Some(place) = place {
        let set = set.ok_or(LocationError::NoMaps)?;
        assign(settlement, set, book, &id, place, options)?;
    }
    Ok(id)
}

/// Deletes a custom location. Its marker number is not reused.
pub fn delete(book: &mut LocationBook, location_id: &str) -> Result<(), LocationError> {
    match book.records.get(location_id) {
        Some(r) if r.origin == LocationOrigin::Custom => {
            book.records.remove(location_id);
            Ok(())
        }
        Some(_) => Err(LocationError::NotCustom),
        None => Err(LocationError::Unknown(location_id.to_owned())),
    }
}

fn locate<'s>(
    set: &'s SettlementMapSet,
    target: &PlaceOn,
) -> Result<&'s SettlementMap, LocationError> {
    set.map(&target.map_id)
        .filter(|m| m.building(&target.building_id).is_some())
        .ok_or_else(|| LocationError::UnknownBuilding {
            map: target.map_id.clone(),
            building: target.building_id.clone(),
        })
}

// ----- points of interest ---------------------------------------------------------------

/// A point of interest as the directory and legend show it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PoiEntry {
    pub id: PoiId,
    /// Its marker number (shared sequence with Notable Locations).
    pub number: u32,
    pub map_id: MapId,
    pub map_label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub district: Option<District>,
    pub position: Point,
    pub label: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub notes: String,
}

/// Every point of interest on the saved maps, by number. (A map's points
/// are deleted with it, so none is ever left without its map.)
pub fn poi_entries(set: Option<&SettlementMapSet>, book: &LocationBook) -> Vec<PoiEntry> {
    let mut out: Vec<PoiEntry> = book
        .pois
        .values()
        .filter_map(|p| {
            let map = set?.map(&p.map_id)?;
            Some(PoiEntry {
                id: p.id.clone(),
                number: p.number,
                map_id: p.map_id.clone(),
                map_label: map.label.clone(),
                district: map.district().cloned(),
                position: p.position,
                label: p.label.clone(),
                notes: p.notes.clone(),
            })
        })
        .collect();
    out.sort_by_key(|p| p.number);
    out
}

fn poi_text(label: String, notes: String) -> Result<(String, String), LocationError> {
    let label = clean_line("Name", MAX_POI_LABEL_CHARS, Some(label))?
        .ok_or(LocationError::PoiNameRequired)?;
    let notes = notes.replace("\r\n", "\n").trim().to_owned();
    if notes.chars().count() > MAX_LOCATION_NOTES_CHARS {
        return Err(LocationError::TooLong {
            field: "Notes",
            max: MAX_LOCATION_NOTES_CHARS,
        });
    }
    Ok((label, notes))
}

fn inside(set: &SettlementMapSet, map_id: &MapId, position: Point) -> Result<(), LocationError> {
    let map = set
        .map(map_id)
        .ok_or_else(|| LocationError::UnknownMap(map_id.clone()))?;
    let finite = position.0.is_finite() && position.1.is_finite();
    if !finite || !map.geometry.bounds.contains(position) {
        return Err(LocationError::PoiOutside);
    }
    Ok(())
}

/// Adds a point of interest at `position` on a map. It takes the next
/// marker number of the settlement; its ID is made from that number, so
/// neither is ever reused.
pub fn add_poi(
    set: &SettlementMapSet,
    book: &mut LocationBook,
    map_id: &MapId,
    position: Point,
    label: String,
    notes: String,
) -> Result<PoiId, LocationError> {
    inside(set, map_id, position)?;
    let (label, notes) = poi_text(label, notes)?;
    let number = book.next_marker;
    book.next_marker += 1;
    let id = PoiId::from(format!("poi-{number}").as_str());
    book.pois.insert(
        id.to_string(),
        PointOfInterest {
            id: id.clone(),
            number,
            map_id: map_id.clone(),
            position: round(position),
            label,
            notes,
        },
    );
    Ok(id)
}

/// Renames a point of interest and sets its notes.
pub fn update_poi(
    book: &mut LocationBook,
    id: &str,
    label: String,
    notes: String,
) -> Result<(), LocationError> {
    let (label, notes) = poi_text(label, notes)?;
    let poi = book
        .pois
        .get_mut(id)
        .ok_or_else(|| LocationError::UnknownPoi(id.to_owned()))?;
    poi.label = label;
    poi.notes = notes;
    Ok(())
}

/// Moves a point of interest within its map. Its number stays.
pub fn move_poi(
    set: &SettlementMapSet,
    book: &mut LocationBook,
    id: &str,
    position: Point,
) -> Result<(), LocationError> {
    let map_id = book
        .pois
        .get(id)
        .ok_or_else(|| LocationError::UnknownPoi(id.to_owned()))?
        .map_id
        .clone();
    inside(set, &map_id, position)?;
    book.pois.get_mut(id).expect("checked above").position = round(position);
    Ok(())
}

/// Deletes a point of interest. Its number is not reused.
pub fn delete_poi(book: &mut LocationBook, id: &str) -> Result<(), LocationError> {
    book.pois
        .remove(id)
        .map(|_| ())
        .ok_or_else(|| LocationError::UnknownPoi(id.to_owned()))
}

/// Stored positions are rounded to the centimetre, like geometry.
fn round(p: Point) -> Point {
    Point((p.0 * 100.0).round() / 100.0, (p.1 * 100.0).round() / 100.0)
}
