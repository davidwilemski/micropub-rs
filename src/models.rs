use diesel::expression::Expression;
use diesel::prelude::*;
use diesel::sqlite::Sqlite;
use diesel::Queryable;
use serde::Serialize;

use crate::schema::*;

type AllColumns = (
    posts::id,
    posts::slug,
    posts::entry_type,
    posts::name,
    posts::content,
    posts::client_id,
    posts::created_at,
    posts::updated_at,
    posts::content_type,
    posts::bookmark_of,
);

const ALL_COLUMNS: AllColumns = (
    posts::id,
    posts::slug,
    posts::entry_type,
    posts::name,
    posts::content,
    posts::client_id,
    posts::created_at,
    posts::updated_at,
    posts::content_type,
    posts::bookmark_of,
);

type PostSqlType = <AllColumns as Expression>::SqlType;
type BoxedPostsQuery<'a> = posts::BoxedQuery<'a, Sqlite, PostSqlType>;

fn posts_for_category(tag: &str) -> categories::BoxedQuery<'_, Sqlite, diesel::sql_types::Integer> {
    use crate::schema::categories::dsl::*;
    categories
        .select(post_id)
        .filter(category.eq(tag))
        .into_boxed()
}

#[derive(Clone, Debug, Queryable, Serialize)]
pub struct Post {
    pub id: i32,
    pub slug: String,
    pub entry_type: String,
    pub name: Option<String>,
    pub content: Option<String>,
    pub client_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub content_type: Option<String>,
    pub bookmark_of: Option<String>,
}

impl Post {
    pub fn by_slug<'a>(url_slug: &'a str) -> BoxedPostsQuery<'a> {
        use crate::schema::posts::dsl::*;
        Post::all().filter(slug.eq(url_slug))
    }

    pub fn all<'a>() -> BoxedPostsQuery<'a> {
        use crate::schema::posts::dsl::*;
        posts
            .select(ALL_COLUMNS)
            .order_by(created_at.desc())
            .into_boxed()
    }

    // TODO make tag lookup case insensitive?
    pub fn by_tag<'a>(tag: &'a str) -> BoxedPostsQuery<'a> {
        use crate::schema::posts::dsl::*;
        Post::all()
            .filter(id.eq_any(posts_for_category(tag)))
            .order_by(created_at.desc())
    }

    pub fn latest<'a>() -> BoxedPostsQuery<'a> {
        use crate::schema::posts::dsl::*;
        Post::all().order_by(created_at.desc()).limit(1)
    }
}

#[derive(Debug, Insertable)]
#[diesel(table_name = posts)]
pub struct NewPost<'a> {
    pub slug: &'a str,
    pub entry_type: &'a str,
    pub name: Option<&'a str>,
    pub content: Option<&'a str>,
    pub content_type: Option<&'a str>,
    pub client_id: Option<&'a str>,
    pub created_at: Option<&'a str>,
    pub updated_at: Option<&'a str>,
    pub bookmark_of: Option<&'a str>,
}

#[derive(Debug, Insertable)]
#[diesel(table_name = post_history)]
pub struct NewPostHistory {
    pub post_id: i32,
    pub slug: String,
    pub entry_type: String,
    pub name: Option<String>,
    pub content: Option<String>,
    pub client_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub content_type: Option<String>,
    pub bookmark_of: Option<String>,
}

impl From<Post> for NewPostHistory {
    fn from(post: Post) -> Self {
        Self {
            post_id: post.id,
            slug: post.slug,
            entry_type: post.entry_type,
            name: post.name,
            content: post.content,
            client_id: post.client_id,
            created_at: post.created_at,
            updated_at: post.updated_at,
            content_type: post.content_type,
            bookmark_of: post.bookmark_of,
        }
    }
}

#[derive(Debug, Insertable)]
#[diesel(table_name = categories)]
pub struct NewCategory<'a> {
    pub post_id: i32,
    pub category: &'a str,
}

#[derive(Debug, Insertable)]
#[diesel(table_name = original_blobs)]
pub struct NewOriginalBlob<'a> {
    pub post_id: i32,
    pub post_blob: &'a [u8],
}

#[derive(Debug, Insertable)]
#[diesel(table_name = media)]
pub struct NewMediaUpload<'a> {
    pub hex_digest: &'a str,
    pub filename: Option<&'a str>,
    pub content_type: Option<&'a str>,
}

#[derive(Debug, Insertable)]
#[diesel(table_name = photos)]
pub struct NewPhoto<'a> {
    pub post_id: i32,
    pub url: &'a str,
    pub alt: Option<&'a str>,
}
