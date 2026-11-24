-- Schema v7 (TODO/011): the images, apart from the records that show them
-- (never inside the JSON, nor as Base64). An asset is tied to the id of its
-- team or person, not to a name; the records point to it, none has one by
-- default.

CREATE TABLE assets (
    id      TEXT PRIMARY KEY CHECK (length(id) = 36),
    kind    TEXT NOT NULL CHECK (kind IN ('team_logo', 'person_photo')),
    mime    TEXT NOT NULL CHECK (mime IN ('image/png', 'image/jpeg')),
    width   INTEGER NOT NULL CHECK (width > 0),
    height  INTEGER NOT NULL CHECK (height > 0),
    data    BLOB NOT NULL
) STRICT;

ALTER TABLE teams ADD COLUMN logo_asset_id TEXT REFERENCES assets (id);
ALTER TABLE persons ADD COLUMN photo_asset_id TEXT REFERENCES assets (id);
