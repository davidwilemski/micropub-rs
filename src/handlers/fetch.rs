use std::sync::Arc;

use axum::{
    extract::Path,
    response::{Html, IntoResponse},
};
use bytes::Bytes;
use diesel::prelude::*;
use diesel::r2d2;
use futures::join;
use http::StatusCode;
use tracing::{debug, error, Instrument, debug_span};

use crate::errors::*;
use crate::handler::{handle_db_errors, MicropubDB, WithDB};
use crate::models::Post;
use crate::post_util;
use crate::templates;
use crate::view_models::{Date as DateView, Post as PostView};

#[tracing::instrument(level = "info", skip(pool, templates, site_config))]
pub async fn get_post_handler(
    url_slug: String,
    pool: Arc<r2d2::Pool<r2d2::ConnectionManager<SqliteConnection>>>,
    templates: Arc<templates::Templates>,
    site_config: Arc<crate::MicropubSiteConfig>,
) -> Result<impl IntoResponse, StatusCode> {
    let db = MicropubDB::new(pool);
    let mut conn = db.dbconn()?;

    let slug_clone = url_slug.clone();
    let mut slug_conn = db.dbconn()?;
    let mut post: Post = 
        tokio::task::spawn_blocking(move || {
            Post::by_slug(&slug_clone)
                .first::<Post>(&mut slug_conn)
                .map_err(|e| handle_db_errors(e))
        })
        .instrument(debug_span!("post_by_slug"))
        .await.map_err(|e| Into::<ServerError>::into(e))??;

    let post_id = post.id;
    let mut tags_conn = db.dbconn()?;
    let tags_fut =
        tokio::task::spawn_blocking(move || {
            use crate::schema::categories::dsl as category_dsl;
             category_dsl::categories
                .select(category_dsl::category)
                .filter(category_dsl::post_id.eq(post_id))
                .get_results(&mut tags_conn)
                .map_err(|e| handle_db_errors(e))
        })
        .instrument(debug_span!("tags_by_post_id"));

    let photos_fut =
        tokio::task::spawn_blocking(move || {
            use crate::schema::photos::dsl as photos_dsl;
            photos_dsl::photos
                .select((photos_dsl::url, photos_dsl::alt))
                .filter(photos_dsl::post_id.eq(post_id))
                .get_results(&mut conn)
                .map_err(|e| handle_db_errors(e))
        })
        .instrument(debug_span!("photos_by_post_id"));

    let (tags_result, photos_result)= join!(tags_fut, photos_fut);
    let tags = tags_result.map_err(|e| Into::<ServerError>::into(e))??;
    let photos = photos_result.map_err(|e| Into::<ServerError>::into(e))??;

    debug!("input datetime: {:?}", post.created_at);
    let datetime = post_util::get_local_datetime(&post.created_at, &site_config.micropub.current_timezone_offset).map_err(|e| {
        error!("date parsing error: {:?}", e);
        // TODO shouldn't be a template error but realistically this would only happen if
        // the DB had malformed data for template rendering...
        TemplateError
    })?;
    post.created_at = datetime.to_rfc3339();

    let post_view = PostView::new_from(post, tags, DateView::from(&datetime), photos);
    let _templates = debug_span!("template_render");
    _templates.in_scope(|| {
        let page = templates
            .add_context("article", &post_view)
            .render("article.html")
            .map_err(|e| {
                error!("{:?}", e);
                TemplateError
            })?;
        Ok(Html(page))
    })
}

#[tracing::instrument(level = "info", skip(pool, client, blobject_store_base_uri))]
pub async fn get_media_handler(
    Path(media_id): Path<String>,
    client: reqwest::Client,
    pool: Arc<r2d2::Pool<r2d2::ConnectionManager<SqliteConnection>>>,
    blobject_store_base_uri: Arc<String>,
) -> Result<impl IntoResponse, StatusCode> {
    let resp = client
        .get(format!("{}/{}", blobject_store_base_uri, media_id))
        .send()
        .instrument(debug_span!("blobject request send"))
        .await
        .map_err(|e| {
            error!("error in GET to rustyblobjectstore: {:?}", e);
            MediaFetchError
        })?;

    use crate::schema::media::dsl::*;
    let db = MicropubDB::new(pool);
    let mut conn = db.dbconn()?;
    let media_content_type: Option<String> = media
        .select(content_type)
        .filter(hex_digest.eq(media_id))
        .first(&mut conn)
        .map_err(|e| db.handle_errors(e))?;

    if resp.status() != 200 {
        Err(StatusCode::NOT_FOUND)
    } else {
        let media_body: Bytes = resp.bytes()
            .instrument(debug_span!("blobject store resp get"))
            .await.map_err(|e| {
                error!("error in receiving body as bytes(): {:?}", e);
                MediaFetchError
            })?;
        Ok((
            StatusCode::OK,
            [(
                http::header::CONTENT_TYPE,
                media_content_type.unwrap_or("application/octet-stream".into()),
            )],
            media_body,
        ))
    }
}
