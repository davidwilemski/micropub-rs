CREATE TABLE photos(
    id INTEGER PRIMARY KEY NOT NULL,
    post_id INTEGER REFERENCES posts(id) NOT NULL,
    url TEXT NOT NULL,
    alt TEXT
);

CREATE INDEX index_photos_post_id ON photos(post_id);
