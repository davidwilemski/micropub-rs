use serde::{Deserialize, Serialize};

use crate::models::Post as DBPost;

#[derive(Debug, Serialize, Deserialize)]
pub struct Date {
    year: i32,
    month: u32,
    day: u32,
    date: String,
    time: String,
}

impl<T> std::convert::From<&T> for Date
where
    T: chrono::Datelike + chrono::Timelike,
{
    fn from(date: &T) -> Self {
        let date_str = format!("{}-{}-{}", date.year(), date.month(), date.day());
        let time_str = format!("{}:{}:{}", date.hour(), date.minute(), date.second());
        Self {
            year: date.year(),
            month: date.month(),
            day: date.day(),
            date: date_str,
            time: time_str,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Post {
    pub slug: String,
    pub entry_type: String,
    pub title: Option<String>,
    pub content: Option<String>,
    pub content_type: Option<String>,
    pub client_id: Option<String>,
    pub published: String,
    pub updated: String,
    pub tags: Vec<String>,
    pub date: Date,
}

impl Post {
    pub fn new_from(post: DBPost, categories: Vec<String>, date: Date) -> Self {
        Post {
            slug: post.slug,
            entry_type: post.entry_type,
            title: post.name,
            content: post.content,
            content_type: post.content_type,
            client_id: post.client_id,
            published: post.created_at,
            updated: post.updated_at,
            date: date,
            tags: categories,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ArticlesPage {
    pub number: u32,
    pub object_list: Vec<Post>,
}
