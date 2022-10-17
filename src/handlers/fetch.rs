use std::sync::Arc;

use diesel::prelude::*;
use diesel::r2d2;
use log::{debug, info, error};
use warp::{reject, Rejection};
use warp::http::{header as http_header, Response};

use crate::errors::*;
use crate::handler::{MicropubDB, WithDB};
use crate::models::Post;
use crate::post_util;
use crate::templates;
use crate::view_models::{Date as DateView, Post as PostView};

use axum::{
    extract::Path,
    response::{Html, IntoResponse},
};
use http::StatusCode;

pub struct FetchHandler<DB: WithDB> {
    db: DB,
    templates: Arc<templates::Templates>,
    client: reqwest::Client,
}

impl FetchHandler<MicropubDB> {
    pub fn new(
        pool: Arc<r2d2::Pool<r2d2::ConnectionManager<SqliteConnection>>>,
        templates: Arc<templates::Templates>,
    ) -> Self {
        FetchHandler {
            db: MicropubDB::new(pool),
            templates,
            client: reqwest::Client::new(),
        }
    }

    pub async fn fetch_post(&self, url_slug: &str) -> Result<impl warp::Reply, Rejection> {
        info!("fetch_post url_slug:{:?}", url_slug);
        let conn = self.db.dbconn()?;

        let mut post = Post::by_slug(url_slug).first::<Post>(&conn).map_err(
            |e: diesel::result::Error| match e {
                diesel::result::Error::NotFound => warp::reject::not_found(),
                _ => reject::custom(self.db.handle_errors(e)),
            },
        )?;

        use crate::schema::categories::dsl as category_dsl;
        let tags: Vec<String> = category_dsl::categories
            .select(category_dsl::category)
            .filter(category_dsl::post_id.eq(post.id))
            .get_results(&conn)
            .map_err(|e| self.db.handle_errors(e))?;

        use crate::schema::photos::dsl as photos_dsl;
        let photos: Vec<(String, Option<String>)> = photos_dsl::photos
            .select((photos_dsl::url, photos_dsl::alt))
            .filter(photos_dsl::post_id.eq(post.id))
            .get_results(&conn)
            .map_err(|e| self.db.handle_errors(e))?;

        debug!("input datetime: {:?}", post.created_at);
        let datetime = post_util::get_local_datetime(&post.created_at, None)
            .map_err(|e| {
                error!("date parsing error: {:?}", e);
                // TODO shouldn't be a template error but realistically this would only happen if
                // the DB had malformed data for template rendering...
                reject::custom(TemplateError)
            })?;
        post.created_at = datetime.to_rfc3339();

        let post_view = PostView::new_from(post, tags, DateView::from(&datetime), photos);
        let page = self.templates
            .add_context("article", &post_view)
            .render("article.html")
            .map_err(|e| {
                error!("{:?}", e);
                reject::custom(TemplateError)
            })?;
        Ok(warp::reply::html(page))
    }

    pub async fn fetch_media(&self, media_id: &str) -> Result<impl warp::Reply, Rejection> {
        let resp = self.client.get(format!("http://rustyblobjectstore:3031/{}", media_id))
            .send()
            .await
            .map_err(|e| {
                error!("error in GET to rustyblobjectstore: {:?}", e);
                reject::custom(MediaFetchError)
            })?;

        use crate::schema::media::dsl::*;
        let conn = self.db.dbconn()?;
        let media_content_type: Option<String> = media.select(content_type)
            .filter(hex_digest.eq(media_id))
            .first(&conn)
            .map_err(|e| self.db.handle_errors(e))?;

        if resp.status() != 200 {
            Err(warp::reject::not_found())
        } else {
            let media_body = resp.bytes()
                .await
                .map_err(|e| {
                    error!("error in receiving body as bytes(): {:?}", e);
                    reject::custom(MediaFetchError)
                })?;
            Ok(
                Response::builder()
                    .status(200)
                    .header(
                        http_header::CONTENT_TYPE,
                        media_content_type
                            .unwrap_or("application/octet-stream".into())
                    )
                    .body(media_body)
                    .map_err(|e| {
                        error!("error building media response: {:?}", e);
                        reject::custom(MediaFetchError)
                    })?
            )
        }
    }
}

pub async fn get_post_handler(
    Path(url_slug): Path<String>,
    pool: Arc<r2d2::Pool<r2d2::ConnectionManager<SqliteConnection>>>,
    templates: Arc<templates::Templates>,
) -> Result<impl IntoResponse, StatusCode> {
    info!("fetch_post url_slug:{:?}", url_slug);
    let db = MicropubDB::new(pool);
    let conn = db.dbconn()?;

    let mut post = Post::by_slug(&url_slug).first::<Post>(&conn)
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
    let datetime = post_util::get_local_datetime(&post.created_at, None)
        .map_err(|e| {
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
