-- Schema v9 (TODO/011): the contacts of a person, apart from the details
-- of v8 (some databases of the development already had v8 without them).
ALTER TABLE persons ADD COLUMN email TEXT NOT NULL DEFAULT '' CHECK (
    deleted = 0 OR email = ''
);
ALTER TABLE persons ADD COLUMN phone TEXT NOT NULL DEFAULT '' CHECK (
    deleted = 0 OR phone = ''
);
ALTER TABLE persons ADD COLUMN birth_place TEXT NOT NULL DEFAULT '' CHECK (
    deleted = 0 OR birth_place = ''
);
