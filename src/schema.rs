// @generated automatically by Diesel CLI.

diesel::table! {
    categories (id) {
        id -> Integer,
        post_id -> Integer,
        category -> Text,
    }
}

diesel::table! {
    media (id) {
        id -> Integer,
        hex_digest -> Text,
        filename -> Nullable<Text>,
        content_type -> Nullable<Text>,
        created_at -> Text,
        updated_at -> Text,
    }
}

diesel::table! {
    original_blobs (id) {
        id -> Integer,
        post_id -> Integer,
        post_blob -> Binary,
    }
}

diesel::table! {
    photos (id) {
        id -> Integer,
        post_id -> Integer,
        url -> Text,
        alt -> Nullable<Text>,
    }
}

diesel::table! {
    post_history (id) {
        id -> Integer,
        post_id -> Integer,
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

diesel::table! {
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

diesel::joinable!(categories -> posts (post_id));
diesel::joinable!(original_blobs -> posts (post_id));
diesel::joinable!(photos -> posts (post_id));

diesel::allow_tables_to_appear_in_same_query!(
    categories,
    media,
    original_blobs,
    photos,
    post_history,
    posts,
);
