-- Settlement maps (see maps/ and docs/MAPS.md). Generated geometry is stored
-- and read back; it is never re-derived when a settlement is opened. Each
-- JSON column holds a document versioned by its map_format.

-- One row per settlement with maps: how they were generated, and the
-- user's editable map data, kept apart from the geometry.
CREATE TABLE settlement_map_sets (
    settlement_id TEXT PRIMARY KEY NOT NULL
                  REFERENCES settlements (id) ON DELETE CASCADE,
    map_format    INTEGER NOT NULL,
    revision      INTEGER NOT NULL CHECK (revision >= 1),
    generated_at  INTEGER NOT NULL,
    app_version   TEXT NOT NULL,
    header        TEXT NOT NULL, -- JSON: layout, seed, overrides, inputs, fingerprint
    metadata      TEXT NOT NULL  -- JSON MapMetadata: details, placements, tokens
) STRICT;

-- One row per map: parameters, engine and geometry.
CREATE TABLE settlement_maps (
    settlement_id TEXT NOT NULL
                  REFERENCES settlement_map_sets (settlement_id) ON DELETE CASCADE,
    map_id        TEXT NOT NULL,
    position      INTEGER NOT NULL,
    map_format    INTEGER NOT NULL,
    engine        TEXT NOT NULL,
    engine_version TEXT NOT NULL,
    document      TEXT NOT NULL, -- JSON SettlementMap
    PRIMARY KEY (settlement_id, map_id)
) STRICT;
