-- The scouting rules v4 (`ScoutingRulesEnum`): the block only of an
-- attack. SQLite doesn't change a CHECK: the column is made again with the
-- new one, its values kept, last among the columns as it was.
ALTER TABLE sets ADD COLUMN rules TEXT CHECK (rules IN ('v1', 'v2', 'v3', 'v4'));
UPDATE sets SET rules = scouting_rules;
ALTER TABLE sets DROP COLUMN scouting_rules;
ALTER TABLE sets RENAME COLUMN rules TO scouting_rules;
