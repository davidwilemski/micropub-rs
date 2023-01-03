use std::env;
use std::sync::Arc;

use anyhow::anyhow;
use log::{debug, error, info};
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

use micropub_rs::config::*;
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

    let mut args = std::env::args();
    args.next(); // skip first arg (usually binary name)
    let config_path = args.next().expect("missing config file arg");
    debug!("config_path: {:?}", config_path);

    let config_contents: String = std::fs::read_to_string(&config_path)?;
    let site_config: Arc<micropub_rs::MicropubSiteConfig> = Arc::new(
        toml::from_str(&config_contents)
            .map_err(|e| {
                error!("error reading config file: {}: {:?}", config_path, e);
                e
            })?
    );
    debug!("loaded site_config: {:?}", site_config);

    let micropub_version = env!("CARGO_PKG_VERSION");

    let dbpool = Arc::new(micropub_rs::new_dbconn_pool(&site_config.database_url)?);
    let micropub_db = Arc::new(handler::MicropubDB::new(dbpool.clone()));
    let http_client = reqwest::Client::new();
    info!("created dbpool from {:?}", &site_config.database_url);

    let template_pattern = std::path::Path::new(&site_config.template_dir).join("templates/**/*.html");
    let tera = Arc::new(tera::Tera::new(
        template_pattern
            .to_str()
            .ok_or(anyhow!("missing templates directory"))?,
    )?);
    let mut base_ctx = tera::Context::new();
    base_ctx.insert("MICROPUB_RS_VERSION", micropub_version);
    base_ctx.insert("DEFAULT_LANG", "en-US");
    base_ctx.insert("SITENAME", &site_config.site.site_name);
    base_ctx.insert("SITEURL", "");
    base_ctx.insert("MENUITEMS", &site_config.site.menu_items);
    base_ctx.insert("FEED_DOMAIN", "");
    base_ctx.insert("FEED_ALL_ATOM", "feeds/all.atom.xml");
    info!(
        "initialized template system with templates in {:?}",
        &site_config.template_dir
    );

    let media_config = Arc::new(json!({
        "media-endpoint": site_config.micropub.media_endpoint,
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
                    let c = site_config.clone();
                    move || handlers::get_index_handler(dbpool.clone(), templates.clone(), c.clone())
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
                let cfg = site_config.clone();

                move |headers, multipart| {
                    handlers::handle_media_upload(
                        client.clone(), 
                        db.clone(),
                        headers,
                        multipart,
                        cfg.clone(),
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
                    let blobject_store = Arc::new(site_config.blobject_store_base_uri.clone());
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
                let c = site_config.clone();

                move |headers: HeaderMap, body| {
                    handlers::handle_post(client.clone(), db.clone(), c.clone(), headers, body)
                }
            }).get({
                let client = http_client.clone();
                let config = media_config.clone();
                let c = site_config.clone();

                move |headers, query| {
                    handlers::handle_query(
                        client.clone(),
                        config.clone(),
                        c.clone(),
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
                        std::path::Path::new(&site_config.template_dir).join("static")
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
