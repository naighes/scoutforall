-- Schema v6 (TODO/021 §13): the gym of a match. The matches already there
-- have none.

ALTER TABLE matches ADD COLUMN venue_id TEXT REFERENCES venues (id);
CREATE INDEX matches_by_venue ON matches (venue_id) WHERE venue_id IS NOT NULL;
