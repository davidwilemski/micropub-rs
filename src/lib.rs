#[macro_use]
extern crate anyhow;

#[macro_use]
extern crate diesel;

#[macro_use]
extern crate serde_json;

use std::time::Duration;

use diesel::prelude::SqliteConnection;
use diesel::r2d2;

pub mod config;
pub mod constants;
pub mod errors;
pub mod handler;
pub mod handlers;
pub mod media_util;
pub mod models;
pub mod post_util;
pub mod schema;
pub mod templates;
pub mod view_models;

pub use crate::config::*;
pub use crate::constants::*;

pub fn new_dbconn_pool(
    db_file: &str,
) -> Result<r2d2::Pool<r2d2::ConnectionManager<SqliteConnection>>, anyhow::Error> {
    let manager = r2d2::ConnectionManager::<SqliteConnection>::new(db_file);
    let pool = r2d2::Builder::new()
        .max_size(36)
        .connection_timeout(Duration::new(5, 0))
        .build(manager)?;
    Ok(pool)
}
