#[macro_use]
extern crate diesel;

#[macro_use]
extern crate anyhow;

use std::env;
use std::sync::Arc;

use diesel::prelude::*;
use diesel::r2d2;
use warp::http::StatusCode;
use warp::{Filter, Rejection};

mod errors;
mod handlers;
mod models;
mod post_util;
mod schema;
mod templates;
mod view_models;

// TODO make these configurable via command line, environment, or config file?
const MAX_CONTENT_LENGTH: u64 = 1024 * 1024 * 50; // 50 megabytes
const AUTH_TOKEN_ENDPOINT: &str = "https://tokens.indieauth.com/token";
const HOST_WEBSITE: &str = "https://davidwilemski.com/";
const MENU_ITEMS: &[(&str, &str)] = &[("Archive", "/archives")];
const TEMPLATE_DIR_VAR: &str = "MICROPUB_RS_TEMPLATE_DIR";
const SOCIAL: &str = "https://github.com/davidwilemski";
const MICROPUB_ENDPOINT: &str = "/micropub";
const AUTH_ENDPOINT: &str = "https://indieauth.com/auth";
const TOKEN_ENDPOINT: &str = "https://tokens.indieauth.com/token";

fn new_dbconn_pool(
    db_file: &str,
) -> Result<r2d2::Pool<r2d2::ConnectionManager<SqliteConnection>>, anyhow::Error> {
    let manager = r2d2::ConnectionManager::<SqliteConnection>::new(db_file);
    Ok(r2d2::Pool::new(manager)?)
}

async fn handle_rejection(err: Rejection) -> Result<impl warp::Reply, Rejection> {
    // TODO JSON errors?
    if let Some(errors::NotAuthorized) = err.find() {
        return Ok(warp::reply::with_status(
            "Not Authorized",
            StatusCode::FORBIDDEN,
        ));
    }

    let internal_server_error =
        warp::reply::with_status("", warp::http::StatusCode::INTERNAL_SERVER_ERROR);

    // Technically these really are not needed as 500 is the default response
    // for custom rejections but we could do some instrumentation or logging
    // here or whatever.
    if let Some(errors::HTTPClientError) = err.find() {
        return Ok(internal_server_error);
    }
    if let Some(errors::ValidateResponseDeserializeError) = err.find() {
        return Ok(internal_server_error);
    }

    // Otherwise pass the rejection through the filter stack
    Err(err)
}

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    let dbfile = env::var("DATABASE_URL")?;
    let template_dir = env::var(TEMPLATE_DIR_VAR)?;
    let dbpool = Arc::new(new_dbconn_pool(&dbfile)?);
    let template_pattern = std::path::Path::new(&template_dir).join("templates/**/*.html");
    let tera = Arc::new(tera::Tera::new(
        template_pattern
            .to_str()
            .ok_or(anyhow!("missing templates directory"))?,
    )?);
    let mut base_ctx = tera::Context::new();
    base_ctx.insert("DEFAULT_LANG", "en-US");
    base_ctx.insert("SITENAME", "David's Blog");
    base_ctx.insert("SITEURL", "");
    base_ctx.insert("MENUITEMS", crate::MENU_ITEMS);
    base_ctx.insert("FEED_DOMAIN", "");
    base_ctx.insert("FEED_ALL_ATOM", "feeds/all.atom.xml");

    let atom_ctx = base_ctx.clone();

    let templates = Arc::new(templates::Templates::new(tera, base_ctx));
    let micropub_handler = Arc::new(handlers::MicropubHandler::new(dbpool.clone()));
    let fetch_handler = Arc::new(handlers::FetchHandler::new(
        dbpool.clone(),
        templates.clone(),
    ));
    let archive_handler = Arc::new(handlers::ArchiveHandler::new(
        dbpool.clone(),
        templates.clone(),
    ));
    let atom_handler = Arc::new(handlers::AtomHandler::new(
        dbpool.clone(),
        Arc::new(crate::templates::Templates::atom_default(atom_ctx)),
    ));
    let index_handler = Arc::new(handlers::IndexHandler::new(
        dbpool.clone(),
        templates.clone(),
    ));
    let static_files = warp::filters::fs::dir(std::path::Path::new(&template_dir).join("static"));

    let micropub = warp::path!("micropub")
        .and(warp::post())
        .and(warp::filters::header::optional::<String>("Content-Type"))
        .and(warp::header::<String>("Authorization"))
        .and(warp::body::content_length_limit(MAX_CONTENT_LENGTH))
        .and(warp::body::bytes())
        .and_then(move |ct, a, body| {
            let h = micropub_handler.clone();
            async move { h.verify_auth(ct, a, body).await }
        })
        .recover(handle_rejection);

    let fetch_post = warp::any()
        .and(warp::path::full())
        .map(move |path: warp::path::FullPath| {
            path.as_str().to_string()
        }).and_then(move |path: String| {
            //full path includes leading /, remove that
            let slug = path.as_str().strip_prefix('/').map(|s| s.to_string());
            let h = fetch_handler.clone();
            async move { h.fetch_post(&slug.unwrap_or(path)).await }
        });

    let archives = warp::path!("archives").and(warp::get()).and_then(move || {
        let h = archive_handler.clone();
        async move { h.get().await }
    });

    let atom = warp::path!("feeds" / "all.atom.xml")
        .and(warp::get())
        .and_then(move || {
            let h = atom_handler.clone();
            async move { h.get().await }
        });

    let index = warp::path::end().and(warp::get()).and_then(move || {
        let h = index_handler.clone();
        async move { h.get().await }
    });

    warp::serve(
        index.or(micropub.or(archives.or(atom
            .or(warp::path("theme").and(static_files))
            .or(fetch_post)))),
    )
    .run(([127, 0, 0, 1], 3030))
    .await;

    Ok(())
}
