-- The scouting rules each set was saved with (`ScoutingRulesEnum`): what
-- can follow what, what each evaluation means. None: v1, the sets saved
-- before they were (up to v1.0.0, the data of the versions before the
-- database too); a set read with older rules is translated to the current
-- ones, the rows stay as they are until the set is written again.
ALTER TABLE sets ADD COLUMN scouting_rules TEXT CHECK (scouting_rules IN ('v1', 'v2'));
