use std::env;
use std::sync::Arc;

use anyhow::anyhow;
use log::info;
use serde_json::json;

use axum::{
    extract::Path,
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    routing::{get, on, on_service, post, MethodFilter},
    Router,
};
use std::net::SocketAddr;
use tower_http::services::ServeDir;

use micropub_rs::constants::*;
use micropub_rs::handler;
use micropub_rs::handlers;
use micropub_rs::templates;

async fn handle_error(_err: std::io::Error) -> impl IntoResponse {
    (StatusCode::INTERNAL_SERVER_ERROR, "Something went wrong...")
}

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    env_logger::init();

    let micropub_version = env!("CARGO_PKG_VERSION");

    let dbfile = env::var("DATABASE_URL")
        .map_err(|e| anyhow!(format!("error reading env var {}: {:?}", "DATABASE_URL", e)))?;
    let template_dir = env::var(TEMPLATE_DIR_VAR)
        .map_err(|e| anyhow!(format!("error reading env var {}: {:?}", TEMPLATE_DIR_VAR, e)))?;
    let media_endpoint = env::var(MEDIA_ENDPOINT_VAR)
        .map_err(|e| anyhow!(format!("error reading env var {}: {:?}", MEDIA_ENDPOINT_VAR, e)))?;
    let blobject_store_base_uri: Arc<String> = env::var(BLOBJECT_STORE_BASE_URI_VAR)
        .map(|s| Arc::new(s))
        .map_err(|e| anyhow!(format!("error reading env var {}: {:?}", BLOBJECT_STORE_BASE_URI_VAR, e)))?;
    let dbpool = Arc::new(micropub_rs::new_dbconn_pool(&dbfile)?);
    let micropub_db = Arc::new(handler::MicropubDB::new(dbpool.clone()));
    let http_client = reqwest::Client::new();
    info!("created dbpool from {:?}", dbfile);

    let template_pattern = std::path::Path::new(&template_dir).join("templates/**/*.html");
    let tera = Arc::new(tera::Tera::new(
        template_pattern
            .to_str()
            .ok_or(anyhow!("missing templates directory"))?,
    )?);
    let mut base_ctx = tera::Context::new();
    base_ctx.insert("MICROPUB_RS_VERSION", micropub_version);
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

    let app = Router::new()
        .route(
            "/",
            on(
                MethodFilter::GET.union(MethodFilter::HEAD),
                {
                    let dbpool = dbpool.clone();
                    let templates = templates.clone();
                    move || handlers::get_index_handler(dbpool.clone(), templates.clone())
                }
            ),
        )
        .route(
            "/archives",
            on(
                MethodFilter::GET.union(MethodFilter::HEAD),
                {
                    let dbpool = dbpool.clone();
                    let templates = templates.clone();
                    move || handlers::get_archive_handler(None, dbpool.clone(), templates.clone())
                }
            ),
        )
        .route(
            "/feeds/all.atom.xml",
            on(
                MethodFilter::GET.union(MethodFilter::HEAD),
                {
                    let dbpool = dbpool.clone();
                    let templates = Arc::new(crate::templates::Templates::atom_default(atom_ctx));
                    move || handlers::get_atom_handler(dbpool.clone(), templates.clone())
                }
            ),
        )
        .route(
            "/media",
            post({
                let db = micropub_db.clone();
                let client = http_client.clone();
                let blobject_store = blobject_store_base_uri.clone();

                move |headers, multipart| {
                    handlers::handle_media_upload(
                        client.clone(), 
                        db.clone(),
                        headers,
                        multipart,
                        blobject_store.clone(),
                    )
                }
            }),
        )
        .route(
            "/media/:media_id",
            on(
                MethodFilter::GET.union(MethodFilter::HEAD),
                {
                    let dbpool = dbpool.clone();
                    let client = http_client.clone();
                    let blobject_store = blobject_store_base_uri.clone();
                    move |media_id| {
                        handlers::get_media_handler(
                            media_id,
                            client.clone(),
                            dbpool.clone(),
                            blobject_store.clone(),
                        )
                }
            }),
        )
        .route(
            "/micropub",
            post({
                let db = micropub_db.clone();
                let client = http_client.clone();

                move |headers: HeaderMap, body| {
                    handlers::handle_post(client.clone(), db.clone(), headers, body)
                }
            }).get({
                let client = http_client.clone();
                let config = media_config.clone();

                move |headers, query| {
                    handlers::handle_query(
                        client.clone(),
                        config.clone(),
                        headers,
                        query
                    )
                }

            })
        )
        .route(
            "/tag/:tag",
            on(
                MethodFilter::GET.union(MethodFilter::HEAD),
                {
                    let dbpool = dbpool.clone();
                    let templates = templates.clone();
                    move |Path(tag): Path<String>| {
                        handlers::get_archive_handler(Some(tag), dbpool.clone(), templates.clone())
                    }
                }
            ),
        )
        .nest(
            "/theme",
            Router::new().route(
                "/*path",
                on_service(
                    MethodFilter::GET.union(MethodFilter::HEAD),
                    ServeDir::new(
                        std::path::Path::new(&template_dir).join("static")
                    )
                )
                .handle_error(handle_error)
            )
        )
        .route(
            "/*post_slug",
            on(
                MethodFilter::GET.union(MethodFilter::HEAD),
                {
                    let dbpool = dbpool.clone();
                    move |Path(post_slug): Path<String>| {
                        info!("in get post handler");
                        handlers::get_post_handler(post_slug, dbpool.clone(), templates.clone())
                    }
                }
            )
        );

    let addr = SocketAddr::from(([0, 0, 0, 0], 3030));
    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await
        .unwrap();

    Ok(())
}
