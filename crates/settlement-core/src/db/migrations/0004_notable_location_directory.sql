-- The Notable Location directory (see maps/directory.rs and docs/MAPS.md
-- §13): names, notes, marker numbers and building placements of a
-- settlement's Notable Locations, and the user's custom locations. The
-- settlement snapshot is never changed; rows refer to its location IDs.
CREATE TABLE settlement_locations (
    settlement_id TEXT NOT NULL
                  REFERENCES settlements (id) ON DELETE CASCADE,
    location_id   TEXT NOT NULL,
    map_format    INTEGER NOT NULL,
    -- The building it stands on, if any (also in the document).
    building_id   TEXT,
    document      TEXT NOT NULL, -- JSON LocationRecord
    PRIMARY KEY (settlement_id, location_id)
) STRICT;

-- A building holds at most one location (NULLs, unplaced, are not counted).
CREATE UNIQUE INDEX settlement_locations_one_per_building
    ON settlement_locations (settlement_id, building_id);

-- Per settlement: the next marker number and custom location number. Never
-- decreases, so a number is never given to another location.
CREATE TABLE settlement_location_sequences (
    settlement_id TEXT PRIMARY KEY NOT NULL
                  REFERENCES settlements (id) ON DELETE CASCADE,
    next_marker   INTEGER NOT NULL CHECK (next_marker >= 1),
    next_custom   INTEGER NOT NULL CHECK (next_custom >= 1)
) STRICT;
