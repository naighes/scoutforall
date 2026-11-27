-- Schema v8 (TODO/011): optional details shared by a person across teams.
ALTER TABLE persons ADD COLUMN nationality TEXT CHECK (
    (nationality IS NULL OR (length(nationality) = 2 AND nationality GLOB '[A-Z][A-Z]'))
    AND (deleted = 0 OR nationality IS NULL)
);
ALTER TABLE persons ADD COLUMN dominant_hand TEXT CHECK (
    (dominant_hand IS NULL OR dominant_hand IN ('right', 'left'))
    AND (deleted = 0 OR dominant_hand IS NULL)
);
ALTER TABLE persons ADD COLUMN federation_id TEXT NOT NULL DEFAULT '' CHECK (
    deleted = 0 OR federation_id = ''
);
