-- Building details (see maps/details.rs and docs/MAPS.md §12). A building
-- gets a row the first time it is opened; buildings never opened have none.
-- One row per building, so editing a building writes only that row.
--
-- Rows belong to the settlement, not to a map set: regenerating maps gives
-- buildings new IDs and never reuses old ones, so earlier rows are kept but
-- can never attach to a different building. Deleting the maps deletes them.
CREATE TABLE settlement_map_buildings (
    settlement_id TEXT NOT NULL
                  REFERENCES settlements (id) ON DELETE CASCADE,
    building_id   TEXT NOT NULL,
    map_id        TEXT NOT NULL,
    map_format    INTEGER NOT NULL,
    version       INTEGER NOT NULL CHECK (version >= 1),
    document      TEXT NOT NULL, -- JSON BuildingRecord
    PRIMARY KEY (settlement_id, building_id)
) STRICT;
