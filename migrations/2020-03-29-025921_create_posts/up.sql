CREATE TABLE posts(
    id INTEGER PRIMARY KEY NOT NULL,
    slug TEXT NOT NULL,
    entry_type TEXT NOT NULL,
    name TEXT,
    content TEXT,
    client_id TEXT,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE UNIQUE INDEX index_posts_slug ON posts(slug);
CREATE INDEX index_posts_entry_type ON posts(entry_type);

CREATE TABLE categories (
    id INTEGER PRIMARY KEY NOT NULL,
    post_id INTEGER REFERENCES posts(id) NOT NULL,
    category TEXT NOT NULL
);

CREATE INDEX index_post_id ON categories(post_id);
CREATE UNIQUE INDEX index_category_post ON categories(post_id, category);
