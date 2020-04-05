use diesel::Queryable;
use serde::Serialize;

use crate::schema::{posts, categories};

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
