use serde::{Serialize, Deserialize};

use crate::models::{Post as DBPost};


#[derive(Debug, Serialize, Deserialize)]
pub struct Date {
    year: i32,
    month: u32,
    day: u32,
    date: String,
}

impl<T> std::convert::From<&T> for Date
    where
    T: chrono::Datelike
{
    fn from(date: &T) -> Self {
        let date_str = format!("{}-{}-{}", date.year(), date.month(), date.day());
        Self { year: date.year(), month: date.month(), day: date.day(), date: date_str }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Post {
    slug: String,
    entry_type: String,
    title: Option<String>,
    content: Option<String>,
    client_id: Option<String>,
    published: String,
    updated: String,
    categories: Vec<String>,
    date: Date,
}

impl Post {
    pub fn new_from(post: DBPost, categories: Vec<String>, date: Date) -> Self {
        Post {
            slug: post.slug,
            entry_type: post.entry_type,
            title: post.name,
            content: post.content,
            client_id: post.client_id,
            published: post.created_at,
            updated: post.updated_at,
            date: date,
            categories,
        }
    }
}
