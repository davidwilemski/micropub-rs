use std::sync::Arc;

use diesel::prelude::*;
use diesel::r2d2;
use warp::http::StatusCode;
use warp::{reject, Filter, Rejection};

use crate::models::Post;
use crate::errors::*;


pub struct FetchHandler {
    dbpool: Arc<r2d2::Pool<r2d2::ConnectionManager<SqliteConnection>>>,
}

impl FetchHandler {
    pub fn new(pool: Arc<r2d2::Pool<r2d2::ConnectionManager<SqliteConnection>>>) -> Self {
        FetchHandler {
            dbpool: pool
        }
    }

    pub async fn fetch_post(&self, url_slug: &str) -> Result<impl warp::Reply, Rejection> {
        let conn = self.dbpool.get()
            .map_err(|e| {
                println!("{:?}", e);
                reject::custom(DBError)
            })?;

        let post = Post::by_slug(url_slug)
            .first::<Post>(&conn)
            .map_err(|e: diesel::result::Error| {
                match e {
                    diesel::result::Error::NotFound => {
                        warp::reject::not_found()
                    }
                    _ => {
                        println!("{:?}", e);
                        reject::custom(DBError)
                    }
                }
            })?;

        // TODO get categories

        let result = serde_json::to_string(&post)
            .map_err(|e| {
                println!("{:?}", e);
                reject::custom(DBError)
            })?;
        Ok(result)
    }
}
