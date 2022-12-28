use std::env;
use std::sync::Arc;

use anyhow::anyhow;
use log::{error, info};
use serde_json::json;
use warp::http::StatusCode;
use warp::Rejection;

use axum::{
    extract::{Path, Query},
    http::HeaderMap,
    response::IntoResponse,
    routing::{get, on_service, post, MethodFilter},
    Router,
};
use std::net::SocketAddr;
use tower_http::services::ServeDir;

use micropub_rs::constants::*;
use micropub_rs::errors;
use micropub_rs::handler;
use micropub_rs::handlers;
use micropub_rs::templates;

async fn handle_error(_err: std::io::Error) -> impl IntoResponse {
    (StatusCode::INTERNAL_SERVER_ERROR, "Something went wrong...")
}

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

    let dbfile = env::var("DATABASE_URL")
        .map_err(|e| anyhow!(format!("error reading env var {}: {:?}", "DATABASE_URL", e)))?;
    let template_dir = env::var(TEMPLATE_DIR_VAR)
        .map_err(|e| anyhow!(format!("error reading env var {}: {:?}", TEMPLATE_DIR_VAR, e)))?;
    let media_endpoint = env::var(MEDIA_ENDPOINT_VAR)
        .map_err(|e| anyhow!(format!("error reading env var {}: {:?}", MEDIA_ENDPOINT_VAR, e)))?;
    let dbpool = Arc::new(micropub_rs::new_dbconn_pool(&dbfile)?);
    let micropub_db = Arc::new(handler::MicropubDB::new(dbpool.clone()));
    let http_client = Arc::new(reqwest::Client::new());
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
    info!(
        "initialized template system with templates in {:?}",
        template_dir
    );

    let media_config = Arc::new(json!({
        "media-endpoint": media_endpoint,
    }));

    let atom_ctx = base_ctx.clone();

    let templates = Arc::new(templates::Templates::new(tera, base_ctx));

                eprintln!("{:?}", std::path::Path::new(&template_dir).join("static"));
    let app = Router::new()
        .route(
            "/",
            get({
                let dbpool = dbpool.clone();
                let templates = templates.clone();
                move || handlers::get_index_handler(dbpool.clone(), templates.clone())
            }),
        )
        .route(
            "/archives",
            get({
                let dbpool = dbpool.clone();
                let templates = templates.clone();
                move || handlers::get_archive_handler(None, dbpool.clone(), templates.clone())
            }),
        )
        .route(
            "/feeds/all.atom.xml",
            get({
                let dbpool = dbpool.clone();
                let templates = Arc::new(crate::templates::Templates::atom_default(atom_ctx));
                move || handlers::get_atom_handler(dbpool.clone(), templates.clone())
            }),
        )
        .route(
            "/media/:media_id",
            get({
                let dbpool = dbpool.clone();
                let client = http_client.clone();
                move |media_id| {
                    handlers::get_media_handler(media_id, client.clone(), dbpool.clone())
                }
            }),
        )
        .route(
            "/micropub",
            post({
                let db = micropub_db.clone();
                let client = http_client.clone();
                let config = media_config.clone();

                move |headers: HeaderMap, body| {
                    handlers::handle_post(client.clone(), db.clone(), config.clone(), headers, body)
                }
            }).get({
                let db = micropub_db.clone();
                let client = http_client.clone();
                let config = media_config.clone();

                move |headers, query| {
                    handlers::handle_query(
                        client.clone(),
                        db.clone(),
                        config.clone(),
                        headers,
                        query
                    )
                }

            })
        )
        .route(
            "/tag/:tag",
            get({
                let dbpool = dbpool.clone();
                let templates = templates.clone();
                move |Path(tag): Path<String>| {
                    handlers::get_archive_handler(Some(tag), dbpool.clone(), templates.clone())
                }
            }),
        )
        .nest(
            "/theme",
            on_service(
                MethodFilter::GET.union(MethodFilter::HEAD),
                ServeDir::new(
                    std::path::Path::new(&template_dir).join("static")
                )
            )
            .handle_error(handle_error),
        )
        // Handle posts as a fallback
        // XXX not sure if there's a more idiomatic way.
        // Tried a wildcard match on /*url_slug but that panicked due to path conflicts
        .fallback(
            get({
                let dbpool = dbpool.clone();
                move |uri: axum::http::Uri| {
                    info!("in get post handler");
                    handlers::get_post_handler(uri, dbpool.clone(), templates.clone())
                }
            })
        );

    let addr = SocketAddr::from(([127, 0, 0, 1], 3030));
    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await
        .unwrap();

    Ok(())
}
