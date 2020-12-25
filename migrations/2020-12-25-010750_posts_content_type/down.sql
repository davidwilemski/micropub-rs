-- Apparently sqlite does not support an explicit drop column
-- See https://www.sqlite.org/lang_altertable.html#making_other_kinds_of_table_schema_changes for a migration process.

-- 2. begin txn
-- diesel appears to do this

-- 3. Use CREATE TABLE to create a new table
-- Copied from posts up.sql :/

CREATE TABLE posts_downgrade(
    id INTEGER PRIMARY KEY NOT NULL,
    slug TEXT NOT NULL,
    entry_type TEXT NOT NULL,
    name TEXT,
    content TEXT,
    client_id TEXT,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

-- 5. Transfer content from X into new_X using a statement like: INSERT INTO
-- new_X SELECT ... FROM X. 
INSERT INTO posts_downgrade(
    id,
    slug,
    entry_type,
    name,
    content,
    client_id,
    created_at,
    updated_at
) SELECT
    id,
    slug,
    entry_type,
    name,
    content,
    client_id,
    created_at,
    updated_at
FROM posts;

-- 6 Drop the old table X
DROP TABLE posts;

-- 7. Change the name of new_X to X using: ALTER TABLE new_X RENAME TO X.
ALTER TABLE posts_downgrade RENAME TO posts;

-- 8. Use CREATE INDEX, CREATE TRIGGER, and CREATE VIEW to reconstruct
-- indexes, triggers, and views associated with table X. 
-- Copied from posts up.sql :/
CREATE UNIQUE INDEX index_posts_slug ON posts(slug);
CREATE INDEX index_posts_entry_type ON posts(entry_type);

-- 11. Commit the transaction started in step 2. 
-- diesel appears to do this
