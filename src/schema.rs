table! {
    categories (id) {
        id -> Integer,
        post_id -> Integer,
        category -> Text,
    }
}

table! {
    media (id) {
        id -> Integer,
        hex_digest -> Text,
        filename -> Nullable<Text>,
        content_type -> Nullable<Text>,
        created_at -> Text,
        updated_at -> Text,
    }
}

table! {
    original_blobs (id) {
        id -> Integer,
        post_id -> Integer,
        post_blob -> Binary,
    }
}

table! {
    photos (id) {
        id -> Integer,
        post_id -> Integer,
        url -> Text,
        alt -> Nullable<Text>,
    }
}

table! {
    posts (id) {
        id -> Integer,
        slug -> Text,
        entry_type -> Text,
        name -> Nullable<Text>,
        content -> Nullable<Text>,
        content_type -> Nullable<Text>,
        client_id -> Nullable<Text>,
        created_at -> Text,
        updated_at -> Text,
        bookmark_of -> Nullable<Text>,
    }
}

joinable!(categories -> posts (post_id));
joinable!(original_blobs -> posts (post_id));
joinable!(photos -> posts (post_id));

allow_tables_to_appear_in_same_query!(
    categories,
    media,
    original_blobs,
    photos,
    posts,
);
