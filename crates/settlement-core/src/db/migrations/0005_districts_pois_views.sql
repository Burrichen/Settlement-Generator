-- Districts, points of interest and saved views (see docs/MAPS.md §14).

-- The user's names for a City's or Capital's districts. The district IDs
-- (`district-1` …) come from the settlement snapshot and never change, so a
-- rename never touches a map, building or location. Kept when maps are
-- deleted: a district belongs to the settlement, not to its map.
CREATE TABLE settlement_district_names (
    settlement_id TEXT NOT NULL
                  REFERENCES settlements (id) ON DELETE CASCADE,
    district_id   TEXT NOT NULL,
    name          TEXT NOT NULL,
    PRIMARY KEY (settlement_id, district_id)
) STRICT;

-- Points of interest: user tokens at a position on one map. Their marker
-- numbers come from settlement_location_sequences, shared with Notable
-- Locations, so a number is never used twice in a settlement.
CREATE TABLE settlement_map_pois (
    settlement_id TEXT NOT NULL
                  REFERENCES settlements (id) ON DELETE CASCADE,
    poi_id        TEXT NOT NULL,
    map_id        TEXT NOT NULL,
    map_format    INTEGER NOT NULL,
    document      TEXT NOT NULL, -- JSON PointOfInterest
    PRIMARY KEY (settlement_id, poi_id)
) STRICT;

-- The view each map was last left at: its centre in map metres and its zoom
-- relative to the map's default view, so it fits any window size.
CREATE TABLE settlement_map_views (
    settlement_id TEXT NOT NULL
                  REFERENCES settlements (id) ON DELETE CASCADE,
    map_id        TEXT NOT NULL,
    centre_x      REAL NOT NULL,
    centre_y      REAL NOT NULL,
    zoom          REAL NOT NULL CHECK (zoom > 0),
    PRIMARY KEY (settlement_id, map_id)
) STRICT;
