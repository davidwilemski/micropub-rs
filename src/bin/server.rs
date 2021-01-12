#[macro_use]
extern crate anyhow;

use std::env;
use std::sync::Arc;

use log::{info, error};
use warp::http::StatusCode;
use warp::{Filter, Rejection};

use micropub_rs::constants::*;
use micropub_rs::errors;
use micropub_rs::handlers;
use micropub_rs::templates;

async fn handle_rejection(err: Rejection) -> Result<impl warp::Reply, Rejection> {
    // TODO JSON errors?
    if let Some(errors::NotAuthorized) = err.find() {
        error!("Handling NotAuthorized error: {:?}", err);
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
        error!("Handling HTTPClientError: {:?}", err);
        return Ok(internal_server_error);
    }
    if let Some(errors::ValidateResponseDeserializeError) = err.find() {
        error!("Handling ValidateResponseDeserializeError: {:?}", err);
        return Ok(internal_server_error);
    }

    // Otherwise pass the rejection through the filter stack
    Err(err)
}

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    env_logger::init();

    let dbfile = env::var("DATABASE_URL")?;
    let template_dir = env::var(TEMPLATE_DIR_VAR)?;
    let dbpool = Arc::new(micropub_rs::new_dbconn_pool(&dbfile)?);
    info!("created dbpool from {:?}", dbfile);

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
    info!("initialized template system with templates in {:?}", template_dir);

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
    let tag_archive_handler = Arc::new(handlers::ArchiveHandler::new(
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
            async move { h.handle_post(ct, a, body).await }
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
        async move { h.get(None).await }
    });

    let tag_archives = warp::path!("tag" / String).and(warp::get()).and_then(move |tag: String| {
        let h = tag_archive_handler.clone();
        async move { h.get(Some(tag.as_str())).await }
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

    let log = warp::log("micropub::server");

    warp::serve(
        index.or(micropub.or(tag_archives.or(archives.or(atom
            .or(warp::path("theme").and(static_files))
            .or(fetch_post)))))
        .with(log),
    )
    .run(([0, 0, 0, 0], 3030))
    .await;

    Ok(())
}
