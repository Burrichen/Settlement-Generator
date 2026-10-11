-- Initial schema. Timestamps are Unix milliseconds (UTC). JSON columns hold
-- documents whose format is versioned inside the document or alongside it.

CREATE TABLE settings (
    key   TEXT PRIMARY KEY NOT NULL,
    value TEXT NOT NULL -- JSON
) STRICT;

-- The single user override document (see layers.rs).
CREATE TABLE content_overrides (
    id         INTEGER PRIMARY KEY CHECK (id = 1),
    document   TEXT NOT NULL, -- JSON UserOverrides
    updated_at INTEGER NOT NULL
) STRICT;

CREATE TABLE settlements (
    id              TEXT PRIMARY KEY NOT NULL,
    name            TEXT NOT NULL CHECK (length(trim(name)) > 0),
    created_at      INTEGER NOT NULL,
    updated_at      INTEGER NOT NULL,
    app_version     TEXT NOT NULL,
    favourite       INTEGER NOT NULL DEFAULT 0 CHECK (favourite IN (0, 1)),
    snapshot_format INTEGER NOT NULL,
    snapshot        TEXT NOT NULL -- JSON SettlementSnapshot
) STRICT;

CREATE INDEX settlements_by_updated ON settlements (updated_at DESC);

-- Defence in depth for the favourite/delete invariant, which the core also
-- checks before deleting.
CREATE TRIGGER settlements_keep_favourites
BEFORE DELETE ON settlements
WHEN OLD.favourite = 1
BEGIN
    SELECT RAISE(ABORT, 'favourite settlements cannot be deleted');
END;
