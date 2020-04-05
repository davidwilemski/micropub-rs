use serde::{Serialize, Deserialize};

use crate::models::{Post as DBPost};

#[derive(Debug, Serialize, Deserialize)]
pub struct Post {
    slug: String,
    entry_type: String,
    name: Option<String>,
    content: Option<String>,
    client_id: Option<String>,
    published: String,
    updated: String,
    categories: Vec<String>,
}

impl Post {
    pub fn new_from(post: DBPost, categories: Vec<String>) -> Self {
        Post {
            slug: post.slug,
            entry_type: post.entry_type,
            name: post.name,
            content: post.content,
            client_id: post.client_id,
            published: post.created_at,
            updated: post.updated_at,
            categories,
        }
    }
}
