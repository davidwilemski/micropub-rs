CREATE TABLE original_blobs(
    id INTEGER PRIMARY KEY NOT NULL,
    post_id INTEGER REFERENCES posts(id) NOT NULL,
    post_blob BLOB NOT NULL
);

CREATE UNIQUE INDEX original_blobs_index_post_id ON original_blobs(post_id);
