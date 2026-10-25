//! Storage of settlement maps (`settlement_map_sets`, `settlement_maps`) and
//! building details (`settlement_map_buildings`).
//!
//! A map set is always written whole, in one transaction, so a failure can
//! never leave a mix of old and new maps. Building details are written one
//! building at a time. The settlement row is never touched: maps can be lost
//! or regenerated without affecting it.

use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use serde::{Deserialize, Serialize};

use super::{Database, DbError, DbResult};
use crate::archive::{ArchivedSettlement, SettlementArchive};
use crate::ids::BuildingId;
use crate::ids::MapId;
use crate::maps::directory::{DistrictNames, LocationBook, LocationRecord};
use crate::maps::geometry::Point;
use crate::maps::migrate::{ReadError, read_document};
use crate::maps::{
    BuildingRecord, MAP_FORMAT_VERSION, MapInputs, MapLayout, MapMetadata, MapOverrides,
    SettlementMap, SettlementMapSet,
};
use crate::maps::{DamagedKind, DamagedRecord, PointOfInterest, SavedView};
use crate::settlements::SNAPSHOT_FORMAT_VERSION;
use crate::settlements::{SettlementId, from_unix_millis, to_unix_millis};

/// The `header` column: the map set without its maps and metadata.
#[derive(Serialize, Deserialize)]
struct Header {
    layout: MapLayout,
    seed: u64,
    #[serde(default)]
    overrides: MapOverrides,
    inputs: MapInputs,
    inputs_fingerprint: String,
}

fn read<T: serde::de::DeserializeOwned>(
    what: impl Fn() -> String,
    format: u32,
    json: &str,
) -> DbResult<T> {
    read_document(format, json).map_err(|e| match e {
        ReadError::Newer { found } => DbError::NewerFormat {
            what: what(),
            found,
            supported: MAP_FORMAT_VERSION,
        },
        ReadError::Corrupt(source) => DbError::Corrupt {
            what: what(),
            source,
        },
    })
}

impl Database {
    /// Replaces a settlement's maps with `set`, all or nothing.
    pub fn save_map_set(&mut self, set: &SettlementMapSet) -> DbResult<()> {
        let tx = self.conn.transaction()?;
        write_map_set(&tx, set)?;
        tx.commit()?;
        Ok(())
    }

    /// A settlement's saved maps, or `None` if it has none. A newer or
    /// damaged map document is an error for the maps only; the settlement
    /// still opens.
    pub fn load_map_set(&self, id: &SettlementId) -> DbResult<Option<SettlementMapSet>> {
        type Row = (u32, u32, i64, String, String, String);
        let row: Option<Row> = self
            .conn
            .query_row(
                "SELECT map_format, revision, generated_at, app_version, header, metadata
                 FROM settlement_map_sets WHERE settlement_id = ?1",
                [id.as_str()],
                |r| {
                    Ok((
                        r.get(0)?,
                        r.get(1)?,
                        r.get(2)?,
                        r.get(3)?,
                        r.get(4)?,
                        r.get(5)?,
                    ))
                },
            )
            .optional()?;
        let Some((format, revision, generated_at, app_version, header, metadata)) = row else {
            return Ok(None);
        };
        let what = || format!("the maps of settlement `{id}`");
        let header: Header = read(what, format, &header)?;
        let metadata: MapMetadata = read(what, format, &metadata)?;
        let mut stmt = self.conn.prepare(
            "SELECT map_id, map_format, document FROM settlement_maps
             WHERE settlement_id = ?1 ORDER BY position",
        )?;
        let rows = stmt.query_map([id.as_str()], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, u32>(1)?,
                r.get::<_, String>(2)?,
            ))
        })?;
        let mut maps: Vec<SettlementMap> = Vec::new();
        for row in rows {
            let (map_id, format, json) = row?;
            maps.push(read(
                || format!("map `{map_id}` of settlement `{id}`"),
                format,
                &json,
            )?);
        }
        Ok(Some(SettlementMapSet {
            settlement_id: id.clone(),
            format_version: MAP_FORMAT_VERSION,
            layout: header.layout,
            revision,
            seed: header.seed,
            overrides: header.overrides,
            inputs: header.inputs,
            inputs_fingerprint: header.inputs_fingerprint,
            generated_at: from_unix_millis(generated_at),
            app_version,
            maps,
            metadata,
        }))
    }

    /// Deletes a settlement's maps and their building details, together,
    /// and takes every Notable Location off its building (the locations,
    /// their names and marker numbers are kept). A later first drawing
    /// reuses building IDs, so no placement may outlive the maps.
    /// Returns whether it had any maps.
    pub fn delete_map_set(&mut self, id: &SettlementId) -> DbResult<bool> {
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        tx.execute(
            "DELETE FROM settlement_map_buildings WHERE settlement_id = ?1",
            [id.as_str()],
        )?;
        let before = read_book(&tx, id)?;
        let mut after = before.clone();
        for record in after.records.values_mut() {
            record.assignment = None;
        }
        after.pois.clear();
        write_book(&tx, id, &before, &after)?;
        tx.execute(
            "DELETE FROM settlement_map_views WHERE settlement_id = ?1",
            [id.as_str()],
        )?;
        let n = tx.execute(
            "DELETE FROM settlement_map_sets WHERE settlement_id = ?1",
            [id.as_str()],
        )?;
        tx.commit()?;
        Ok(n > 0)
    }

    /// Deletes one map of a settlement (one district's) and everything on
    /// it, in one transaction: its building details, its points of interest
    /// and its saved view. Notable Locations on it are taken off (kept, with
    /// their numbers). The other maps, the map set's seed and revision, and
    /// district names are kept, so drawing the map again gives new building
    /// IDs. Returns whether the map existed.
    pub fn delete_map(&mut self, settlement: &SettlementId, map: &MapId) -> DbResult<bool> {
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let n = tx.execute(
            "DELETE FROM settlement_maps WHERE settlement_id = ?1 AND map_id = ?2",
            params![settlement.as_str(), map.as_str()],
        )?;
        tx.execute(
            "DELETE FROM settlement_map_buildings WHERE settlement_id = ?1 AND map_id = ?2",
            params![settlement.as_str(), map.as_str()],
        )?;
        tx.execute(
            "DELETE FROM settlement_map_views WHERE settlement_id = ?1 AND map_id = ?2",
            params![settlement.as_str(), map.as_str()],
        )?;
        let before = read_book(&tx, settlement)?;
        let mut after = before.clone();
        for record in after.records.values_mut() {
            if record.assignment.as_ref().is_some_and(|a| &a.map_id == map) {
                record.assignment = None;
            }
        }
        after.pois.retain(|_, p| &p.map_id != map);
        write_book(&tx, settlement, &before, &after)?;
        tx.commit()?;
        Ok(n > 0)
    }

    // ----- district names and saved views -----------------------------------

    pub fn load_district_names(&self, settlement: &SettlementId) -> DbResult<DistrictNames> {
        let mut stmt = self.conn.prepare(
            "SELECT district_id, name FROM settlement_district_names WHERE settlement_id = ?1",
        )?;
        let rows = stmt.query_map([settlement.as_str()], |r| Ok((r.get(0)?, r.get(1)?)))?;
        rows.collect::<Result<_, _>>().map_err(Into::into)
    }

    /// Names a district, or (with `None`) gives it back its default name.
    pub fn set_district_name(
        &mut self,
        settlement: &SettlementId,
        district: &str,
        name: Option<&str>,
    ) -> DbResult<()> {
        match name {
            Some(name) => self.conn.execute(
                "INSERT INTO settlement_district_names (settlement_id, district_id, name)
                 VALUES (?1, ?2, ?3)
                 ON CONFLICT (settlement_id, district_id) DO UPDATE SET name = ?3",
                params![settlement.as_str(), district, name],
            )?,
            None => self.conn.execute(
                "DELETE FROM settlement_district_names
                 WHERE settlement_id = ?1 AND district_id = ?2",
                params![settlement.as_str(), district],
            )?,
        };
        Ok(())
    }

    pub fn load_map_views(&self, settlement: &SettlementId) -> DbResult<Vec<SavedView>> {
        let mut stmt = self.conn.prepare(
            "SELECT map_id, centre_x, centre_y, zoom FROM settlement_map_views
             WHERE settlement_id = ?1 ORDER BY map_id",
        )?;
        let rows = stmt.query_map([settlement.as_str()], |r| {
            Ok(SavedView {
                map_id: MapId::from(r.get::<_, String>(0)?.as_str()),
                centre: Point(r.get(1)?, r.get(2)?),
                zoom: r.get(3)?,
            })
        })?;
        rows.collect::<Result<_, _>>().map_err(Into::into)
    }

    /// Remembers where a map was left. One small row; written often.
    pub fn save_map_view(&mut self, settlement: &SettlementId, view: &SavedView) -> DbResult<()> {
        self.conn.execute(
            "INSERT INTO settlement_map_views (settlement_id, map_id, centre_x, centre_y, zoom)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT (settlement_id, map_id) DO UPDATE
             SET centre_x = ?3, centre_y = ?4, zoom = ?5",
            params![
                settlement.as_str(),
                view.map_id.as_str(),
                view.centre.0,
                view.centre.1,
                view.zoom
            ],
        )?;
        Ok(())
    }

    // ----- the Notable Location directory ------------------------------------

    /// A settlement's stored location records and sequences.
    pub fn load_location_book(&self, settlement: &SettlementId) -> DbResult<LocationBook> {
        read_book(&self.conn, settlement)
    }

    /// Changes a settlement's location records in one transaction: reads
    /// them, applies `change`, and writes only the records that changed
    /// (each with its version increased) and the sequences. If `change`
    /// fails, nothing is written. The transaction is taken before reading,
    /// so the GUI and the terminal interface cannot interleave changes.
    pub fn update_location_book<T, E>(
        &mut self,
        settlement: &SettlementId,
        change: impl FnOnce(&mut LocationBook) -> Result<T, E>,
    ) -> DbResult<Result<T, E>> {
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let exists: bool = tx.query_row(
            "SELECT EXISTS (SELECT 1 FROM settlements WHERE id = ?1)",
            [settlement.as_str()],
            |row| row.get(0),
        )?;
        if !exists {
            return Err(DbError::SettlementNotFound(settlement.clone()));
        }
        let before = read_book(&tx, settlement)?;
        let mut after = before.clone();
        let outcome = match change(&mut after) {
            Ok(outcome) => outcome,
            Err(e) => return Ok(Err(e)),
        };
        write_book(&tx, settlement, &before, &after)?;
        tx.commit()?;
        Ok(Ok(outcome))
    }

    // ----- building details (one row per opened building) -------------------

    /// A building's stored details, or `None` if it was never opened.
    pub fn load_building(
        &self,
        settlement: &SettlementId,
        building: &BuildingId,
    ) -> DbResult<Option<BuildingRecord>> {
        let row: Option<(u32, String)> = self
            .conn
            .query_row(
                "SELECT map_format, document FROM settlement_map_buildings
                 WHERE settlement_id = ?1 AND building_id = ?2",
                [settlement.as_str(), building.as_str()],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        row.map(|(format, json)| read_building(settlement, building, format, &json))
            .transpose()
    }

    /// Every readable stored building of a settlement. A damaged row is
    /// left out here; opening that building reports it.
    pub fn load_buildings(&self, settlement: &SettlementId) -> DbResult<Vec<BuildingRecord>> {
        Ok(self.load_buildings_checked(settlement)?.0)
    }

    /// Every stored building of a settlement, and the rows that could not be
    /// read (reported so the user can discard them; never silently lost).
    pub fn load_buildings_checked(
        &self,
        settlement: &SettlementId,
    ) -> DbResult<(Vec<BuildingRecord>, Vec<DamagedRecord>)> {
        let mut stmt = self.conn.prepare(
            "SELECT building_id, map_format, document FROM settlement_map_buildings
             WHERE settlement_id = ?1 ORDER BY building_id",
        )?;
        let rows = stmt.query_map([settlement.as_str()], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, u32>(1)?,
                r.get::<_, String>(2)?,
            ))
        })?;
        let mut out = Vec::new();
        let mut damaged = Vec::new();
        for row in rows {
            let (building, format, json) = row?;
            let id = BuildingId::from(building.as_str());
            match read_building(settlement, &id, format, &json) {
                Ok(record) => out.push(record),
                Err(e) => damaged.push(DamagedRecord {
                    kind: DamagedKind::Building,
                    id: building,
                    problem: e.to_string(),
                }),
            }
        }
        Ok((out, damaged))
    }

    /// Deletes stored rows that could not be read (the recovery for damaged
    /// optional map data). The settlement itself is never touched.
    pub fn discard_damaged(
        &mut self,
        settlement: &SettlementId,
        records: &[DamagedRecord],
    ) -> DbResult<()> {
        let tx = self.conn.transaction()?;
        for r in records {
            let (sql, id) = match r.kind {
                DamagedKind::Maps => (
                    "DELETE FROM settlement_map_sets WHERE settlement_id = ?1 AND ?2 = ?2",
                    "",
                ),
                DamagedKind::Building => (
                    "DELETE FROM settlement_map_buildings WHERE settlement_id = ?1 AND building_id = ?2",
                    r.id.as_str(),
                ),
                DamagedKind::Location => (
                    "DELETE FROM settlement_locations WHERE settlement_id = ?1 AND location_id = ?2",
                    r.id.as_str(),
                ),
                DamagedKind::Poi => (
                    "DELETE FROM settlement_map_pois WHERE settlement_id = ?1 AND poi_id = ?2",
                    r.id.as_str(),
                ),
            };
            tx.execute(sql, params![settlement.as_str(), id])?;
        }
        tx.commit()?;
        Ok(())
    }

    /// Adds an exported settlement as a new one (new ID), with all its map
    /// records, in one transaction: on any error nothing is stored.
    pub fn import_settlement(&mut self, archive: &SettlementArchive) -> DbResult<SettlementId> {
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let id = insert_settlement(&tx, &archive.settlement)?;
        if let Some(maps) = &archive.maps {
            let mut set = maps.set.clone();
            set.settlement_id = id.clone();
            for map in &mut set.maps {
                map.settlement_id = id.clone();
                for b in &mut map.geometry.buildings {
                    b.settlement_id = id.clone();
                }
            }
            write_map_set(&tx, &set)?;
            for record in &maps.buildings {
                tx.execute(
                    "INSERT INTO settlement_map_buildings
                         (settlement_id, building_id, map_id, map_format, version, document)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                    params![
                        id.as_str(),
                        record.building_id.as_str(),
                        record.map_id.as_str(),
                        MAP_FORMAT_VERSION,
                        record.version,
                        serde_json::to_string(record).expect("building record serialises"),
                    ],
                )?;
            }
            let mut book = LocationBook {
                next_marker: maps.next_marker,
                next_custom: maps.next_custom,
                ..LocationBook::default()
            };
            for l in &maps.locations {
                let mut l = l.clone();
                // write_book increases versions; keep the exported ones.
                l.version = l.version.saturating_sub(1);
                book.records.insert(l.id.clone(), l);
            }
            for poi in &maps.pois {
                book.pois.insert(poi.id.to_string(), poi.clone());
            }
            write_book(&tx, &id, &LocationBook::default(), &book)?;
            for (district, name) in &maps.district_names {
                tx.execute(
                    "INSERT INTO settlement_district_names (settlement_id, district_id, name)
                     VALUES (?1, ?2, ?3)",
                    params![id.as_str(), district, name],
                )?;
            }
            for v in &maps.views {
                tx.execute(
                    "INSERT INTO settlement_map_views (settlement_id, map_id, centre_x, centre_y, zoom)
                     VALUES (?1, ?2, ?3, ?4, ?5)",
                    params![id.as_str(), v.map_id.as_str(), v.centre.0, v.centre.1, v.zoom],
                )?;
            }
        }
        tx.commit()?;
        Ok(id)
    }

    /// Stores a building's first details, unless it already has some (for
    /// example opened meanwhile in the other interface). Returns what is
    /// stored, so a building is only ever generated once.
    pub fn insert_building(
        &mut self,
        settlement: &SettlementId,
        record: &BuildingRecord,
    ) -> DbResult<BuildingRecord> {
        let exists: bool = self.conn.query_row(
            "SELECT EXISTS (SELECT 1 FROM settlements WHERE id = ?1)",
            [settlement.as_str()],
            |row| row.get(0),
        )?;
        if !exists {
            return Err(DbError::SettlementNotFound(settlement.clone()));
        }
        self.conn.execute(
            "INSERT INTO settlement_map_buildings
                 (settlement_id, building_id, map_id, map_format, version, document)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT (settlement_id, building_id) DO NOTHING",
            params![
                settlement.as_str(),
                record.building_id.as_str(),
                record.map_id.as_str(),
                MAP_FORMAT_VERSION,
                record.version,
                serde_json::to_string(record).expect("building record serialises"),
            ],
        )?;
        self.load_building(settlement, &record.building_id)?
            .ok_or_else(|| DbError::SettlementNotFound(settlement.clone()))
    }

    /// Replaces a building's stored details with `record`, but only if they
    /// are still at version `expected`. Returns whether it was written: a
    /// `false` means they were changed meanwhile, and nothing was written.
    pub fn update_building(
        &mut self,
        settlement: &SettlementId,
        record: &BuildingRecord,
        expected: u32,
    ) -> DbResult<bool> {
        let n = self.conn.execute(
            "UPDATE settlement_map_buildings
             SET map_format = ?3, version = ?4, document = ?5
             WHERE settlement_id = ?1 AND building_id = ?2 AND version = ?6",
            params![
                settlement.as_str(),
                record.building_id.as_str(),
                MAP_FORMAT_VERSION,
                record.version,
                serde_json::to_string(record).expect("building record serialises"),
                expected,
            ],
        )?;
        Ok(n == 1)
    }
}

/// Reads one stored building, checking that the document is the building
/// its row says it is, so two buildings can never share a record.
fn read_building(
    settlement: &SettlementId,
    building: &BuildingId,
    format: u32,
    json: &str,
) -> DbResult<BuildingRecord> {
    let what = || format!("the details of building `{building}` of settlement `{settlement}`");
    let record: BuildingRecord = read(what, format, json)?;
    if &record.building_id != building {
        return Err(DbError::Corrupt {
            what: what(),
            source: serde::de::Error::custom(format!(
                "the record is for building `{}`",
                record.building_id
            )),
        });
    }
    Ok(record)
}

fn read_book(conn: &Connection, settlement: &SettlementId) -> DbResult<LocationBook> {
    let mut book = LocationBook::default();
    if let Some((marker, custom)) = conn
        .query_row(
            "SELECT next_marker, next_custom FROM settlement_location_sequences
             WHERE settlement_id = ?1",
            [settlement.as_str()],
            |r| Ok((r.get::<_, u32>(0)?, r.get::<_, u32>(1)?)),
        )
        .optional()?
    {
        book.next_marker = marker;
        book.next_custom = custom;
    }
    let mut stmt = conn.prepare(
        "SELECT location_id, map_format, document FROM settlement_locations
         WHERE settlement_id = ?1",
    )?;
    let rows = stmt.query_map([settlement.as_str()], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, u32>(1)?,
            r.get::<_, String>(2)?,
        ))
    })?;
    for row in rows {
        let (id, format, json) = row?;
        let what = || format!("Notable Location `{id}` of settlement `{settlement}`");
        match read::<LocationRecord>(what, format, &json) {
            Ok(record) if record.id == id => {
                book.records.insert(id, record);
            }
            Ok(record) => book.damaged.push(DamagedRecord {
                kind: DamagedKind::Location,
                problem: format!("{}: the record is for `{}`", what(), record.id),
                id,
            }),
            Err(e) => book.damaged.push(DamagedRecord {
                kind: DamagedKind::Location,
                id,
                problem: e.to_string(),
            }),
        }
    }
    let mut stmt = conn.prepare(
        "SELECT poi_id, map_format, document FROM settlement_map_pois WHERE settlement_id = ?1",
    )?;
    let rows = stmt.query_map([settlement.as_str()], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, u32>(1)?,
            r.get::<_, String>(2)?,
        ))
    })?;
    for row in rows {
        let (id, format, json) = row?;
        let what = || format!("point of interest `{id}` of settlement `{settlement}`");
        match read::<PointOfInterest>(what, format, &json) {
            Ok(poi) => {
                book.pois.insert(id, poi);
            }
            Err(e) => book.damaged.push(DamagedRecord {
                kind: DamagedKind::Poi,
                id,
                problem: e.to_string(),
            }),
        }
    }
    Ok(book)
}

/// Writes the difference between two books. Changed rows are deleted first
/// and then inserted, so moving a location between buildings never trips
/// the one-location-per-building index halfway.
fn write_book(
    conn: &Connection,
    settlement: &SettlementId,
    before: &LocationBook,
    after: &LocationBook,
) -> DbResult<()> {
    let mut changed: Vec<LocationRecord> = Vec::new();
    for (id, old) in &before.records {
        let new = after.records.get(id);
        if new != Some(old) {
            conn.execute(
                "DELETE FROM settlement_locations WHERE settlement_id = ?1 AND location_id = ?2",
                params![settlement.as_str(), id],
            )?;
        }
    }
    for (id, new) in &after.records {
        let old = before.records.get(id);
        if old == Some(new) {
            continue;
        }
        let mut record = new.clone();
        record.version = old.map_or(0, |o| o.version) + 1;
        changed.push(record);
    }
    for record in &changed {
        conn.execute(
            "INSERT INTO settlement_locations
                 (settlement_id, location_id, map_format, building_id, document)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                settlement.as_str(),
                record.id,
                MAP_FORMAT_VERSION,
                record.assignment.as_ref().map(|a| a.building_id.as_str()),
                serde_json::to_string(record).expect("location record serialises"),
            ],
        )?;
    }
    for (id, old) in &before.pois {
        if after.pois.get(id) != Some(old) {
            conn.execute(
                "DELETE FROM settlement_map_pois WHERE settlement_id = ?1 AND poi_id = ?2",
                params![settlement.as_str(), id],
            )?;
        }
    }
    for (id, new) in &after.pois {
        if before.pois.get(id) == Some(new) {
            continue;
        }
        conn.execute(
            "INSERT INTO settlement_map_pois (settlement_id, poi_id, map_id, map_format, document)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                settlement.as_str(),
                id,
                new.map_id.as_str(),
                MAP_FORMAT_VERSION,
                serde_json::to_string(new).expect("point of interest serialises"),
            ],
        )?;
    }
    if (before.next_marker, before.next_custom) != (after.next_marker, after.next_custom) {
        conn.execute(
            "INSERT INTO settlement_location_sequences (settlement_id, next_marker, next_custom)
             VALUES (?1, ?2, ?3)
             ON CONFLICT (settlement_id) DO UPDATE
             SET next_marker = max(next_marker, ?2), next_custom = max(next_custom, ?3)",
            params![settlement.as_str(), after.next_marker, after.next_custom],
        )?;
    }
    Ok(())
}

/// Replaces a settlement's map set inside the caller's transaction.
fn write_map_set(tx: &Connection, set: &SettlementMapSet) -> DbResult<()> {
    let header = Header {
        layout: set.layout,
        seed: set.seed,
        overrides: set.overrides.clone(),
        inputs: set.inputs.clone(),
        inputs_fingerprint: set.inputs_fingerprint.clone(),
    };
    let exists: bool = tx.query_row(
        "SELECT EXISTS (SELECT 1 FROM settlements WHERE id = ?1)",
        [set.settlement_id.as_str()],
        |row| row.get(0),
    )?;
    if !exists {
        return Err(DbError::SettlementNotFound(set.settlement_id.clone()));
    }
    tx.execute(
        "DELETE FROM settlement_map_sets WHERE settlement_id = ?1",
        [set.settlement_id.as_str()],
    )?;
    tx.execute(
        "INSERT INTO settlement_map_sets
             (settlement_id, map_format, revision, generated_at, app_version, header, metadata)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            set.settlement_id.as_str(),
            MAP_FORMAT_VERSION,
            set.revision,
            to_unix_millis(set.generated_at),
            set.app_version,
            serde_json::to_string(&header).expect("map header serialises"),
            serde_json::to_string(&set.metadata).expect("map metadata serialises"),
        ],
    )?;
    for (position, map) in set.maps.iter().enumerate() {
        tx.execute(
            "INSERT INTO settlement_maps
                 (settlement_id, map_id, position, map_format, engine, engine_version, document)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                set.settlement_id.as_str(),
                map.id.as_str(),
                position as i64,
                MAP_FORMAT_VERSION,
                map.engine.id,
                map.engine.version,
                serde_json::to_string(map).expect("map serialises"),
            ],
        )?;
    }
    Ok(())
}

/// Inserts an archived settlement under a new ID.
fn insert_settlement(tx: &Connection, s: &ArchivedSettlement) -> DbResult<SettlementId> {
    let name = super::clean_name(&s.name)?;
    let id = SettlementId::new_random();
    let at = crate::settlements::now();
    tx.execute(
        "INSERT INTO settlements
             (id, name, created_at, updated_at, app_version, favourite, snapshot_format, snapshot)
         VALUES (?1, ?2, ?3, ?4, ?5, 0, ?6, ?7)",
        params![
            id.as_str(),
            name,
            to_unix_millis(s.created_at),
            to_unix_millis(at),
            s.app_version,
            SNAPSHOT_FORMAT_VERSION,
            serde_json::to_string(&s.snapshot).expect("snapshot serialises"),
        ],
    )?;
    Ok(id)
}
