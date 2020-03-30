table! {
    categories (id) {
        id -> Integer,
        post_id -> Integer,
        category -> Text,
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
    }
}

joinable!(categories -> posts (post_id));

allow_tables_to_appear_in_same_query!(
    categories,
    posts,
);