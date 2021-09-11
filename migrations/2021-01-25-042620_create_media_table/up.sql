CREATE TABLE media(
    id INTEGER PRIMARY KEY NOT NULL,
    hex_digest TEXT NOT NULL,
    filename TEXT,
    content_type TEXT,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX index_media_hex_digest ON media(hex_digest, id, filename, content_type);
