#[macro_use]
extern crate diesel;

use diesel::prelude::SqliteConnection;
use diesel::r2d2;

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

pub use crate::constants::*;

pub fn new_dbconn_pool(
    db_file: &str,
) -> Result<r2d2::Pool<r2d2::ConnectionManager<SqliteConnection>>, anyhow::Error> {
    let manager = r2d2::ConnectionManager::<SqliteConnection>::new(db_file);
    Ok(r2d2::Pool::new(manager)?)
}
