use diesel::expression::{AsExpression, Expression};
use diesel::sqlite::Sqlite;
use diesel::prelude::*;
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
);

type PostSqlType = <AllColumns as Expression>::SqlType;
type WithSlug<'a> = diesel::dsl::Eq<posts::slug, &'a str>;
type BySlug<'a> = diesel::dsl::Filter<Post, WithSlug<'a>>;
type BoxedPostsQuery<'a> = posts::BoxedQuery<'a, Sqlite, PostSqlType>;

#[derive(Debug, Queryable, Serialize)]
pub struct Post {
    pub id: i32,
    pub slug: String,
    pub entry_type: String,
    pub name: Option<String>,
    pub content: Option<String>,
    pub client_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

impl Post {

    pub fn by_slug<'a>(url_slug: &'a str) -> BoxedPostsQuery<'a> {
        use crate::schema::posts::dsl::*;
        posts
            .filter(slug.eq(url_slug))
            .into_boxed()
    }

    pub fn all<'a>() -> BoxedPostsQuery<'a> {
        use crate::schema::posts::dsl::*;
        posts
            .order_by(created_at)
            .into_boxed()
    }
}

#[derive(Debug, Insertable)]
#[table_name="posts"]
pub struct NewPost<'a> {
    pub slug: &'a str,
    pub entry_type: &'a str,
    pub name: Option<&'a str>,
    pub content: Option<&'a str>,
    pub client_id: Option<&'a str>,
}

#[derive(Debug, Insertable)]
#[table_name="categories"]
pub struct NewCategory<'a> {
    pub post_id: i32,
    pub category: &'a str,
}
