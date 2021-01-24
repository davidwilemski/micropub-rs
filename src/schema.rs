table! {
    categories (id) {
        id -> Integer,
        post_id -> Integer,
        category -> Text,
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
    posts (id) {
        id -> Integer,
        slug -> Text,
        entry_type -> Text,
        name -> Nullable<Text>,
        content -> Nullable<Text>,
        client_id -> Nullable<Text>,
        created_at -> Text,
        updated_at -> Text,
        content_type -> Nullable<Text>,
        bookmark_of -> Nullable<Text>,
    }
}

joinable!(categories -> posts (post_id));
joinable!(original_blobs -> posts (post_id));

allow_tables_to_appear_in_same_query!(
    categories,
    original_blobs,
    posts,
);
