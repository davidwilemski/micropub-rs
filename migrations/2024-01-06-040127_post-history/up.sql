CREATE TABLE IF NOT EXISTS "post_history"(
    id INTEGER PRIMARY KEY NOT NULL,
    post_id INTEGER NOT NULL,
    slug TEXT NOT NULL,
    entry_type TEXT NOT NULL,
    name TEXT,
    content TEXT,
    client_id TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    content_type TEXT,
    bookmark_of TEXT
);

CREATE INDEX index_slug_on_post_history ON post_history(slug);
CREATE INDEX index_post_id_on_post_history ON post_history(post_id);
