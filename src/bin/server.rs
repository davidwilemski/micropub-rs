use std::env;
use std::sync::Arc;

use anyhow::anyhow;
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
    let media_endpoint = env::var(MEDIA_ENDPOINT_VAR)?;
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
    let micropub_post_handler = Arc::new(
        handlers::MicropubHandler::new(dbpool.clone(), media_endpoint.clone())
    );
    // TODO unify these three via routing?
    let micropub_query_handler = Arc::new(
        handlers::MicropubHandler::new(dbpool.clone(), media_endpoint.clone())
    );
    let micropub_media_handler = Arc::new(
        handlers::MicropubHandler::new(dbpool.clone(), media_endpoint)
    );
    let fetch_handler = Arc::new(handlers::FetchHandler::new(
        dbpool.clone(),
        templates.clone(),
    ));
    let media_fetch_handler = fetch_handler.clone();
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

    let micropub_post = warp::path!("micropub")
        .and(warp::post())
        .and(warp::filters::header::optional::<String>("Content-Type"))
        .and(warp::header::<String>("Authorization"))
        .and(warp::body::content_length_limit(MAX_CONTENT_LENGTH))
        .and(warp::body::bytes())
        .and_then(move |ct, a, body| {
            let h = micropub_post_handler.clone();
            async move { h.handle_post(ct, a, body).await }
        })
        .recover(handle_rejection);
    let micropub_get = warp::path!("micropub")
        .and(warp::get().or(warp::head()))
        .and(warp::header::<String>("Authorization"))
        .and(warp::filters::query::query())
        .and_then(move |_method, auth, query| {
            let h = micropub_query_handler.clone();
            async move { h.handle_query(auth, query).await }
        });
    let media_post = warp::path!("media")
        .and(warp::body::content_length_limit(MAX_CONTENT_LENGTH))
        .and(warp::post())
        .and(warp::header::<String>("Authorization"))
        .and(warp::filters::multipart::form().max_length(MAX_CONTENT_LENGTH))
        .and_then(move |auth, multipart_data| {
            let h = micropub_media_handler.clone();
            async move { h.handle_media_upload(auth, multipart_data).await }
        });

    let media_get = warp::path!("media" / String)
        .and(warp::get().or(warp::head()))
        // Second argument is an Either (I think to represent the get.or(head))
        .and_then(move |media_id: String, _| {
            dbg!(&media_id);
            info!("fetch_media media_id: {:?}", media_id);
            let h = media_fetch_handler.clone();
            async move { h.fetch_media(&media_id).await }
        });

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

    let index = warp::path::end().and(warp::get().or(warp::head())).and_then(move |_method| {
        let h = index_handler.clone();
        async move { h.get().await }
    });

    let log = warp::log("micropub::server");

    let route =
        index.or(
            micropub_post.or(
                micropub_get.or(
                    media_post.or(
                        tag_archives.or(
                            archives.or(
                                atom.or(
                                    warp::path("theme").and(static_files)
                                ).or(
                                    media_get.or(fetch_post)
                                )
                            )
                        )
                    )
                )
            )
        )
        .with(log);
    let svc = warp::service(route);
    let make_service = tower::make::Shared::new(svc);

    hyper::Server::bind(&([0, 0, 0, 0], 3030).into())
            .http1_title_case_headers(true)
            .serve(make_service)
                .await?;

    Ok(())
}
