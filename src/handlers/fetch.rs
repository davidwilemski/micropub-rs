use std::sync::Arc;

use axum::{
    extract::Path,
    response::{Html, IntoResponse},
};
use bytes::Bytes;
use diesel::prelude::*;
use diesel::r2d2;
use http::StatusCode;
use log::{debug, error, info};

use crate::errors::*;
use crate::handler::{MicropubDB, WithDB};
use crate::models::Post;
use crate::post_util;
use crate::templates;
use crate::view_models::{Date as DateView, Post as PostView};

pub async fn get_post_handler(
    uri: axum::http::Uri,
    pool: Arc<r2d2::Pool<r2d2::ConnectionManager<SqliteConnection>>>,
    templates: Arc<templates::Templates>,
) -> Result<impl IntoResponse, StatusCode> {
    let url_slug: &str = uri.path().trim_start_matches('/');
    info!("fetch_post url_slug:{:?}", url_slug);
    let db = MicropubDB::new(pool);
    let conn = db.dbconn()?;

    let mut post = Post::by_slug(url_slug)
        .first::<Post>(&conn)
        .map_err(|e| db.handle_errors(e))?;

    use crate::schema::categories::dsl as category_dsl;
    let tags: Vec<String> = category_dsl::categories
        .select(category_dsl::category)
        .filter(category_dsl::post_id.eq(post.id))
        .get_results(&conn)
        .map_err(|e| db.handle_errors(e))?;

    use crate::schema::photos::dsl as photos_dsl;
    let photos: Vec<(String, Option<String>)> = photos_dsl::photos
        .select((photos_dsl::url, photos_dsl::alt))
        .filter(photos_dsl::post_id.eq(post.id))
        .get_results(&conn)
        .map_err(|e| db.handle_errors(e))?;

    debug!("input datetime: {:?}", post.created_at);
    let datetime = post_util::get_local_datetime(&post.created_at, None).map_err(|e| {
        error!("date parsing error: {:?}", e);
        // TODO shouldn't be a template error but realistically this would only happen if
        // the DB had malformed data for template rendering...
        TemplateError
    })?;
    post.created_at = datetime.to_rfc3339();

    let post_view = PostView::new_from(post, tags, DateView::from(&datetime), photos);
    let page = templates
        .add_context("article", &post_view)
        .render("article.html")
        .map_err(|e| {
            error!("{:?}", e);
            TemplateError
        })?;
    Ok(Html(page))
}

pub async fn get_media_handler(
    Path(media_id): Path<String>,
    client: Arc<reqwest::Client>,
    pool: Arc<r2d2::Pool<r2d2::ConnectionManager<SqliteConnection>>>,
) -> Result<impl IntoResponse, StatusCode> {
    let resp = client
        .get(format!("http://rustyblobjectstore:3031/{}", media_id))
        .send()
        .await
        .map_err(|e| {
            error!("error in GET to rustyblobjectstore: {:?}", e);
            MediaFetchError
        })?;

    use crate::schema::media::dsl::*;
    let db = MicropubDB::new(pool);
    let conn = db.dbconn()?;
    let media_content_type: Option<String> = media
        .select(content_type)
        .filter(hex_digest.eq(media_id))
        .first(&conn)
        .map_err(|e| db.handle_errors(e))?;

    if resp.status() != 200 {
        Err(StatusCode::NOT_FOUND)
    } else {
        let media_body: Bytes = resp.bytes().await.map_err(|e| {
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
