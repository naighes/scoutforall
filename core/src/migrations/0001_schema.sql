-- Schema v1 (TODO/001): persons apart from teams, matches between team A
-- and team B, competitions, the roster of each match with number and role,
-- one row per touch.
--
-- Rules of every table (D7, for the future sync):
-- - the key is a UUID (lowercase text with hyphens), never an autoincrement;
-- - rows aren't removed, `deleted` is set (the cache `match_summary` aside);
-- - nothing derived is saved as truth: `match_summary` is a cache, rebuilt
--   from the events.
--
-- Closed sets of values (roles, sides, stages...) are checked here; the
-- codes of the events (type, evaluation, technique) are checked by the
-- code that reads them, as the files did: a new code needs no rebuild of
-- the table. Enumerations are snake_case words, the database's own, not
-- the serde names of the files.
--
-- Times are RFC 3339 text: with the offset where the match has one
-- (`matches.start`), UTC for the events.

CREATE TABLE persons (
    id          TEXT PRIMARY KEY CHECK (length(id) = 36),
    name        TEXT NOT NULL,
    birth_date  TEXT CHECK (birth_date IS NULL OR date(birth_date) IS birth_date),
    height_cm   INTEGER CHECK (height_cm IS NULL OR height_cm BETWEEN 1 AND 65535),
    notes       TEXT NOT NULL DEFAULT '',
    deleted     INTEGER NOT NULL DEFAULT 0 CHECK (deleted IN (0, 1)),
    -- D16: a deleted person keeps only its id and the flag
    CHECK (deleted = 0 OR (name = '' AND birth_date IS NULL
                           AND height_cm IS NULL AND notes = ''))
) STRICT;

CREATE TABLE teams (
    id              TEXT PRIMARY KEY CHECK (length(id) = 36),
    name            TEXT NOT NULL,
    year            INTEGER NOT NULL CHECK (year BETWEEN 0 AND 65535),
    classification  TEXT CHECK (classification IN (
                        'top_international', 'high_national', 'national_mid_level',
                        'national_low_level', 'regional', 'local_division')),
    gender          TEXT CHECK (gender IN ('men', 'women')),
    country         TEXT,
    deleted         INTEGER NOT NULL DEFAULT 0 CHECK (deleted IN (0, 1))
) STRICT;

-- The membership of a person in a team: the team's number and role, and
-- its place in the list of the team's players.
CREATE TABLE team_members (
    team_id     TEXT NOT NULL REFERENCES teams (id),
    person_id   TEXT NOT NULL REFERENCES persons (id),
    -- the order of the players, a removed one keeps its place
    position    INTEGER NOT NULL CHECK (position BETWEEN 0 AND 65535),
    number      INTEGER NOT NULL CHECK (number BETWEEN 0 AND 255),
    role        TEXT CHECK (role IN (
                    'libero', 'opposite_hitter', 'setter', 'outside_hitter', 'middle_blocker')),
    -- removed from the team (the person stays)
    deleted     INTEGER NOT NULL DEFAULT 0 CHECK (deleted IN (0, 1)),
    PRIMARY KEY (team_id, person_id),
    UNIQUE (team_id, position)
) STRICT;
CREATE INDEX team_members_by_person ON team_members (person_id);

-- Shared by the teams that play it (D12).
CREATE TABLE competitions (
    id          TEXT PRIMARY KEY CHECK (length(id) = 36),
    name        TEXT NOT NULL,
    format      TEXT NOT NULL CHECK (format IN ('league', 'cup', 'tournament')),
    season      TEXT,
    gender      TEXT CHECK (gender IN ('men', 'women')),
    country     TEXT,
    -- the default of its matches
    friendly    INTEGER NOT NULL DEFAULT 0 CHECK (friendly IN (0, 1)),
    deleted     INTEGER NOT NULL DEFAULT 0 CHECK (deleted IN (0, 1))
) STRICT;

CREATE TABLE matches (
    id                  TEXT PRIMARY KEY CHECK (length(id) = 36),
    -- D3: A is always a team; B a team or only a name
    team_a              TEXT NOT NULL REFERENCES teams (id),
    team_b              TEXT REFERENCES teams (id),
    team_b_name         TEXT,
    -- null: neutral court
    home                TEXT CHECK (home IN ('a', 'b')),
    scouting            TEXT NOT NULL DEFAULT 'a' CHECK (scouting IN ('a', 'both')),
    -- D11: date and time, with its offset
    start               TEXT NOT NULL,
    duration_min        INTEGER CHECK (duration_min IS NULL OR duration_min > 0),
    friendly            INTEGER NOT NULL DEFAULT 0 CHECK (friendly IN (0, 1)),
    competition_id      TEXT REFERENCES competitions (id),
    stage               TEXT CHECK (stage IN (
                            'regular', 'pool', 'qualification', 'playoff', 'playout',
                            'final_round', 'placement', 'other')),
    round               TEXT,
    max_substitutions   INTEGER NOT NULL CHECK (max_substitutions BETWEEN 0 AND 255),
    notes               TEXT NOT NULL DEFAULT '',
    source              TEXT NOT NULL DEFAULT 'scout4all' CHECK (source IN ('scout4all', 'dvw')),
    video_clock         INTEGER NOT NULL DEFAULT 0 CHECK (video_clock IN (0, 1)),
    -- the id and team of the files before the migration (the upload queue,
    -- the reports): none for the matches created since
    legacy_id           TEXT,
    legacy_team         TEXT,
    deleted             INTEGER NOT NULL DEFAULT 0 CHECK (deleted IN (0, 1)),
    CHECK ((team_b IS NULL) <> (team_b_name IS NULL)),
    CHECK (team_b IS NOT team_a),
    -- with only a name there are no players to scout
    CHECK (scouting = 'a' OR team_b IS NOT NULL),
    CHECK ((legacy_id IS NULL) = (legacy_team IS NULL))
) STRICT;
CREATE INDEX matches_by_team_a ON matches (team_a);
CREATE INDEX matches_by_team_b ON matches (team_b) WHERE team_b IS NOT NULL;
CREATE INDEX matches_by_competition ON matches (competition_id) WHERE competition_id IS NOT NULL;
CREATE UNIQUE INDEX matches_by_legacy_id ON matches (legacy_team, legacy_id)
    WHERE legacy_id IS NOT NULL;

-- D9: the roster of each team in the match, number and role copied from
-- the membership when the match is created.
CREATE TABLE match_roster (
    match_id    TEXT NOT NULL REFERENCES matches (id),
    team_id     TEXT NOT NULL REFERENCES teams (id),
    person_id   TEXT NOT NULL REFERENCES persons (id),
    number      INTEGER NOT NULL CHECK (number BETWEEN 0 AND 255),
    role        TEXT CHECK (role IN (
                    'libero', 'opposite_hitter', 'setter', 'outside_hitter', 'middle_blocker')),
    PRIMARY KEY (match_id, team_id, person_id),
    -- the number is unique in the roster of the match, not of the team
    UNIQUE (match_id, team_id, number)
) STRICT;
CREATE INDEX match_roster_by_person ON match_roster (person_id);

-- a roster of a team that plays the match
CREATE TRIGGER match_roster_team_insert BEFORE INSERT ON match_roster
WHEN NOT EXISTS (SELECT 1 FROM matches m WHERE m.id = NEW.match_id
                 AND NEW.team_id IN (m.team_a, m.team_b))
BEGIN
    SELECT RAISE(ABORT, 'match_roster: the team does not play the match');
END;
CREATE TRIGGER match_roster_team_update BEFORE UPDATE OF match_id, team_id ON match_roster
WHEN NOT EXISTS (SELECT 1 FROM matches m WHERE m.id = NEW.match_id
                 AND NEW.team_id IN (m.team_a, m.team_b))
BEGIN
    SELECT RAISE(ABORT, 'match_roster: the team does not play the match');
END;

CREATE TABLE sets (
    match_id        TEXT NOT NULL REFERENCES matches (id),
    number          INTEGER NOT NULL CHECK (number BETWEEN 1 AND 5),
    serving         TEXT NOT NULL CHECK (serving IN ('a', 'b')),
    -- the rules the set is replayed with (`LineupRulesEnum`); none: v1,
    -- the sets written before they were saved
    lineup_rules    TEXT CHECK (lineup_rules IN ('v1', 'v2')),
    deleted         INTEGER NOT NULL DEFAULT 0 CHECK (deleted IN (0, 1)),
    PRIMARY KEY (match_id, number)
) STRICT;

-- The starting lineup of a side: B's only when both are scouted.
CREATE TABLE set_lineups (
    match_id        TEXT NOT NULL,
    set_number      INTEGER NOT NULL,
    side            TEXT NOT NULL CHECK (side IN ('a', 'b')),
    p1              TEXT NOT NULL REFERENCES persons (id),
    p2              TEXT NOT NULL REFERENCES persons (id),
    p3              TEXT NOT NULL REFERENCES persons (id),
    p4              TEXT NOT NULL REFERENCES persons (id),
    p5              TEXT NOT NULL REFERENCES persons (id),
    p6              TEXT NOT NULL REFERENCES persons (id),
    libero          TEXT NOT NULL REFERENCES persons (id),
    fallback_libero TEXT REFERENCES persons (id),
    setter          TEXT NOT NULL REFERENCES persons (id),
    PRIMARY KEY (match_id, set_number, side),
    FOREIGN KEY (match_id, set_number) REFERENCES sets (match_id, number),
    -- six different players, the libero not among them, the setter yes
    CHECK (p1 NOT IN (p2, p3, p4, p5, p6) AND p2 NOT IN (p3, p4, p5, p6)
           AND p3 NOT IN (p4, p5, p6) AND p4 NOT IN (p5, p6) AND p5 <> p6),
    CHECK (libero NOT IN (p1, p2, p3, p4, p5, p6)),
    CHECK (fallback_libero IS NULL
           OR fallback_libero NOT IN (libero, p1, p2, p3, p4, p5, p6)),
    CHECK (setter IN (p1, p2, p3, p4, p5, p6))
) STRICT;

CREATE TRIGGER set_lineups_side_b_insert BEFORE INSERT ON set_lineups
WHEN NEW.side = 'b' AND NOT EXISTS (SELECT 1 FROM matches m
                                    WHERE m.id = NEW.match_id AND m.scouting = 'both')
BEGIN
    SELECT RAISE(ABORT, 'set_lineups: side b of a match not scouted on both sides');
END;
CREATE TRIGGER set_lineups_side_b_update BEFORE UPDATE OF match_id, side ON set_lineups
WHEN NEW.side = 'b' AND NOT EXISTS (SELECT 1 FROM matches m
                                    WHERE m.id = NEW.match_id AND m.scouting = 'both')
BEGIN
    SELECT RAISE(ABORT, 'set_lineups: side b of a match not scouted on both sides');
END;

-- D8: one row per touch, both sides in one sequence.
CREATE TABLE events (
    id              TEXT PRIMARY KEY CHECK (length(id) = 36),
    match_id        TEXT NOT NULL,
    set_number      INTEGER NOT NULL,
    -- the order in the set; only appended, an undone event keeps its place
    seq             INTEGER NOT NULL CHECK (seq >= 0),
    side            TEXT NOT NULL CHECK (side IN ('a', 'b')),
    timestamp       TEXT NOT NULL,
    event_type      TEXT NOT NULL,
    player          TEXT REFERENCES persons (id),
    eval            TEXT,
    target_player   TEXT REFERENCES persons (id),
    technique       TEXT,
    -- `EventFlags`: the bits of a later version are kept
    flags           INTEGER NOT NULL DEFAULT 0 CHECK (flags BETWEEN 0 AND 65535),
    deleted         INTEGER NOT NULL DEFAULT 0 CHECK (deleted IN (0, 1)),
    FOREIGN KEY (match_id, set_number) REFERENCES sets (match_id, number),
    UNIQUE (match_id, set_number, seq)
) STRICT;
CREATE INDEX events_by_player ON events (player) WHERE player IS NOT NULL;
CREATE INDEX events_by_target_player ON events (target_player) WHERE target_player IS NOT NULL;

-- The video of a match and how its events line up with it (`VideoProfile`).
CREATE TABLE match_videos (
    match_id            TEXT PRIMARY KEY REFERENCES matches (id),
    -- the anchor: the time of an event and the moment of the video
    anchor_at           TEXT,
    anchor_offset_ms    INTEGER,
    -- chosen by the user; none: found by itself
    anchor_event        TEXT REFERENCES events (id),
    CHECK ((anchor_at IS NULL) = (anchor_offset_ms IS NULL)),
    CHECK (anchor_event IS NULL OR anchor_at IS NOT NULL)
) STRICT;

-- The files of the video, in the order they're played.
CREATE TABLE match_video_files (
    match_id    TEXT NOT NULL REFERENCES match_videos (match_id),
    position    INTEGER NOT NULL CHECK (position >= 0),
    -- relative to the data folder when in it, absolute otherwise
    path        TEXT NOT NULL,
    start       TEXT,
    PRIMARY KEY (match_id, position)
) STRICT;

-- An event put at its own time: `by_ms` from where the anchor puts it.
CREATE TABLE match_video_corrections (
    match_id    TEXT NOT NULL REFERENCES match_videos (match_id),
    event_id    TEXT NOT NULL REFERENCES events (id),
    by_ms       INTEGER NOT NULL CHECK (by_ms <> 0),
    PRIMARY KEY (match_id, event_id)
) STRICT;

CREATE TRIGGER match_video_event_insert BEFORE INSERT ON match_video_corrections
WHEN NOT EXISTS (SELECT 1 FROM events e WHERE e.id = NEW.event_id AND e.match_id = NEW.match_id)
BEGIN
    SELECT RAISE(ABORT, 'match_video_corrections: the event is of another match');
END;
CREATE TRIGGER match_video_anchor_insert BEFORE INSERT ON match_videos
WHEN NEW.anchor_event IS NOT NULL
     AND NOT EXISTS (SELECT 1 FROM events e WHERE e.id = NEW.anchor_event
                     AND e.match_id = NEW.match_id)
BEGIN
    SELECT RAISE(ABORT, 'match_videos: the anchor is an event of another match');
END;
CREATE TRIGGER match_video_anchor_update BEFORE UPDATE OF anchor_event ON match_videos
WHEN NEW.anchor_event IS NOT NULL
     AND NOT EXISTS (SELECT 1 FROM events e WHERE e.id = NEW.anchor_event
                     AND e.match_id = NEW.match_id)
BEGIN
    SELECT RAISE(ABORT, 'match_videos: the anchor is an event of another match');
END;

-- The matches waiting to be uploaded (of this device: never synced).
CREATE TABLE upload_queue (
    match_id    TEXT PRIMARY KEY REFERENCES matches (id),
    -- seconds since the epoch
    queued_at   INTEGER NOT NULL,
    retry_count INTEGER NOT NULL DEFAULT 0 CHECK (retry_count >= 0)
) STRICT;

-- D7: a cache, from A's side, updated in the transaction of the event,
-- rebuilt from the events when missing; never synced. Rows are removed.
CREATE TABLE match_summary (
    match_id        TEXT PRIMARY KEY REFERENCES matches (id) ON DELETE CASCADE,
    sets            INTEGER NOT NULL CHECK (sets BETWEEN 0 AND 5),
    a_wins          INTEGER NOT NULL CHECK (a_wins BETWEEN 0 AND 3),
    b_wins          INTEGER NOT NULL CHECK (b_wins BETWEEN 0 AND 3),
    finished        INTEGER NOT NULL CHECK (finished IN (0, 1)),
    -- the set to resume, if any
    resume          INTEGER CHECK (resume BETWEEN 1 AND 5),
    next_set_number INTEGER CHECK (next_set_number BETWEEN 1 AND 5)
) STRICT;
