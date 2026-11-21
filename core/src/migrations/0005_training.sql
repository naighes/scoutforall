-- Schema v5 (TODO/021): the training of a team. A program holds
-- mesocycles, these microcycles, these sessions; every level is optional
-- (a parent is nullable on the child) and belongs to one team. The window of
-- each container is its own, declared when it's made; that a child is inside
-- the window of its parent, and that the sessions of a team don't overlap, is
-- checked by the core (the messages name the other one), not here.
--
-- The rules of v1 hold: UUID keys, `deleted` and no row removed, nothing
-- derived. Dates are `YYYY-MM-DD`; the time of a session is local, without
-- offset, `YYYY-MM-DDTHH:MM`, the one the user wrote.

CREATE TABLE training_programs (
    id          TEXT PRIMARY KEY CHECK (length(id) = 36),
    team_id     TEXT NOT NULL REFERENCES teams (id),
    title       TEXT NOT NULL DEFAULT '',
    start_date  TEXT NOT NULL CHECK (date(start_date) IS start_date),
    end_date    TEXT NOT NULL CHECK (date(end_date) IS end_date),
    notes       TEXT NOT NULL DEFAULT '',
    deleted     INTEGER NOT NULL DEFAULT 0 CHECK (deleted IN (0, 1)),
    CHECK (end_date >= start_date)
) STRICT;
CREATE INDEX training_programs_by_team ON training_programs (team_id, start_date);

CREATE TABLE mesocycles (
    id          TEXT PRIMARY KEY CHECK (length(id) = 36),
    team_id     TEXT NOT NULL REFERENCES teams (id),
    program_id  TEXT REFERENCES training_programs (id),
    title       TEXT NOT NULL DEFAULT '',
    start_date  TEXT NOT NULL CHECK (date(start_date) IS start_date),
    weeks       INTEGER NOT NULL CHECK (weeks BETWEEN 1 AND 65535),
    objectives  TEXT NOT NULL DEFAULT '',
    notes       TEXT NOT NULL DEFAULT '',
    deleted     INTEGER NOT NULL DEFAULT 0 CHECK (deleted IN (0, 1))
) STRICT;
CREATE INDEX mesocycles_by_team ON mesocycles (team_id, start_date);
CREATE INDEX mesocycles_by_program ON mesocycles (program_id) WHERE program_id IS NOT NULL;

CREATE TABLE microcycles (
    id           TEXT PRIMARY KEY CHECK (length(id) = 36),
    team_id      TEXT NOT NULL REFERENCES teams (id),
    mesocycle_id TEXT REFERENCES mesocycles (id),
    title        TEXT NOT NULL DEFAULT '',
    start_date   TEXT NOT NULL CHECK (date(start_date) IS start_date),
    weeks        INTEGER NOT NULL CHECK (weeks BETWEEN 1 AND 65535),
    objectives   TEXT NOT NULL DEFAULT '',
    notes        TEXT NOT NULL DEFAULT '',
    deleted      INTEGER NOT NULL DEFAULT 0 CHECK (deleted IN (0, 1))
) STRICT;
CREATE INDEX microcycles_by_team ON microcycles (team_id, start_date);
CREATE INDEX microcycles_by_mesocycle ON microcycles (mesocycle_id) WHERE mesocycle_id IS NOT NULL;

CREATE TABLE training_sessions (
    id                  TEXT PRIMARY KEY CHECK (length(id) = 36),
    team_id             TEXT NOT NULL REFERENCES teams (id),
    microcycle_id       TEXT REFERENCES microcycles (id),
    -- the sessions made together by a repetition (no rule is kept)
    series_id           TEXT CHECK (series_id IS NULL OR length(series_id) = 36),
    start_local         TEXT NOT NULL CHECK (strftime('%Y-%m-%dT%H:%M', start_local) IS start_local),
    duration_min        INTEGER NOT NULL CHECK (duration_min BETWEEN 1 AND 65535),
    actual_duration_min INTEGER CHECK (actual_duration_min IS NULL
                                       OR actual_duration_min BETWEEN 1 AND 65535),
    location            TEXT NOT NULL DEFAULT '',
    title               TEXT NOT NULL DEFAULT '',
    objectives          TEXT NOT NULL DEFAULT '',
    notes               TEXT NOT NULL DEFAULT '',
    final_remarks       TEXT NOT NULL DEFAULT '',
    status              TEXT NOT NULL DEFAULT 'planned'
                        CHECK (status IN ('planned', 'completed', 'cancelled')),
    deleted             INTEGER NOT NULL DEFAULT 0 CHECK (deleted IN (0, 1))
) STRICT;
CREATE INDEX training_sessions_by_team ON training_sessions (team_id, start_local);
CREATE INDEX training_sessions_by_microcycle ON training_sessions (microcycle_id)
    WHERE microcycle_id IS NOT NULL;
CREATE INDEX training_sessions_by_series ON training_sessions (series_id)
    WHERE series_id IS NOT NULL;

-- The activities of a session, in order.
CREATE TABLE training_activities (
    id          TEXT PRIMARY KEY CHECK (length(id) = 36),
    session_id  TEXT NOT NULL REFERENCES training_sessions (id),
    position    INTEGER NOT NULL CHECK (position BETWEEN 0 AND 65535),
    name        TEXT NOT NULL CHECK (name <> ''),
    description TEXT NOT NULL DEFAULT '',
    planned_min INTEGER CHECK (planned_min IS NULL OR planned_min BETWEEN 1 AND 65535),
    actual_min  INTEGER CHECK (actual_min IS NULL OR actual_min BETWEEN 1 AND 65535),
    notes       TEXT NOT NULL DEFAULT '',
    status      TEXT NOT NULL DEFAULT 'planned'
                CHECK (status IN ('planned', 'completed', 'skipped')),
    deleted     INTEGER NOT NULL DEFAULT 0 CHECK (deleted IN (0, 1)),
    UNIQUE (session_id, position)
) STRICT;
CREATE INDEX training_activities_by_name ON training_activities (name);

-- Who was at a session: a player of the team or a guest from another one
-- (`guest`, with the number of that moment). No reason for an absence:
-- nothing about health.
CREATE TABLE training_attendance (
    session_id  TEXT NOT NULL REFERENCES training_sessions (id),
    person_id   TEXT NOT NULL REFERENCES persons (id),
    status      TEXT NOT NULL CHECK (status IN ('present', 'absent', 'late', 'unavailable')),
    note        TEXT NOT NULL DEFAULT '',
    number      INTEGER CHECK (number IS NULL OR number BETWEEN 0 AND 255),
    guest       INTEGER NOT NULL DEFAULT 0 CHECK (guest IN (0, 1)),
    deleted     INTEGER NOT NULL DEFAULT 0 CHECK (deleted IN (0, 1)),
    PRIMARY KEY (session_id, person_id)
) STRICT;
CREATE INDEX training_attendance_by_person ON training_attendance (person_id);

-- a parent and its child of the same team
CREATE TRIGGER mesocycles_team_insert BEFORE INSERT ON mesocycles
WHEN NEW.program_id IS NOT NULL AND NOT EXISTS
    (SELECT 1 FROM training_programs p WHERE p.id = NEW.program_id AND p.team_id = NEW.team_id)
BEGIN
    SELECT RAISE(ABORT, 'mesocycles: the program is of another team');
END;
CREATE TRIGGER mesocycles_team_update BEFORE UPDATE OF team_id, program_id ON mesocycles
WHEN NEW.program_id IS NOT NULL AND NOT EXISTS
    (SELECT 1 FROM training_programs p WHERE p.id = NEW.program_id AND p.team_id = NEW.team_id)
BEGIN
    SELECT RAISE(ABORT, 'mesocycles: the program is of another team');
END;
CREATE TRIGGER microcycles_team_insert BEFORE INSERT ON microcycles
WHEN NEW.mesocycle_id IS NOT NULL AND NOT EXISTS
    (SELECT 1 FROM mesocycles m WHERE m.id = NEW.mesocycle_id AND m.team_id = NEW.team_id)
BEGIN
    SELECT RAISE(ABORT, 'microcycles: the mesocycle is of another team');
END;
CREATE TRIGGER microcycles_team_update BEFORE UPDATE OF team_id, mesocycle_id ON microcycles
WHEN NEW.mesocycle_id IS NOT NULL AND NOT EXISTS
    (SELECT 1 FROM mesocycles m WHERE m.id = NEW.mesocycle_id AND m.team_id = NEW.team_id)
BEGIN
    SELECT RAISE(ABORT, 'microcycles: the mesocycle is of another team');
END;
CREATE TRIGGER training_sessions_team_insert BEFORE INSERT ON training_sessions
WHEN NEW.microcycle_id IS NOT NULL AND NOT EXISTS
    (SELECT 1 FROM microcycles c WHERE c.id = NEW.microcycle_id AND c.team_id = NEW.team_id)
BEGIN
    SELECT RAISE(ABORT, 'training_sessions: the microcycle is of another team');
END;
CREATE TRIGGER training_sessions_team_update BEFORE UPDATE OF team_id, microcycle_id ON training_sessions
WHEN NEW.microcycle_id IS NOT NULL AND NOT EXISTS
    (SELECT 1 FROM microcycles c WHERE c.id = NEW.microcycle_id AND c.team_id = NEW.team_id)
BEGIN
    SELECT RAISE(ABORT, 'training_sessions: the microcycle is of another team');
END;
