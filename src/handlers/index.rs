use std::sync::Arc;

use axum::response::{Html, IntoResponse};
use diesel::prelude::*;
use diesel::r2d2;
use http::StatusCode;
use log::error;

use crate::errors::*;
use crate::handler::{MicropubDB, WithDB};
use crate::models::Post;
use crate::post_util;
use crate::templates;
use crate::view_models::{ArticlesPage, Date as DateView, Post as PostView};

pub async fn get_index_handler(
    pool: Arc<r2d2::Pool<r2d2::ConnectionManager<SqliteConnection>>>,
    templates: Arc<templates::Templates>,
    site_config: Arc<crate::MicropubSiteConfig>,
) -> Result<impl IntoResponse, StatusCode> {
    let db = MicropubDB::new(pool);
    let mut conn = db.dbconn()?;

    let mut post = Post::latest()
        .first::<Post>(&mut conn)
        .map_err(|e: diesel::result::Error| db.handle_errors(e))?;

    use crate::schema::categories::dsl::*;
    let tags: Vec<String> = categories
        .select(category)
        .filter(post_id.eq(post.id))
        .get_results(&mut conn)
        .map_err(|e| db.handle_errors(e))?;

    use crate::schema::photos::dsl as photos_dsl;
    let photos: Vec<(String, Option<String>)> = photos_dsl::photos
        .select((photos_dsl::url, photos_dsl::alt))
        .filter(photos_dsl::post_id.eq(post.id))
        .get_results(&mut conn)
        .map_err(|e| db.handle_errors(e))?;

    // Only on main page for indieauth login
    let template = templates
        .add_context("SOCIAL", &site_config.site.socials)
        .add_context("AUTH_ENDPOINT", &site_config.micropub.auth_endpoint)
        .add_context("TOKEN_ENDPOINT", &site_config.micropub.auth_token_endpoint)
        .add_context("MICROPUB_ENDPOINT", &site_config.micropub.micropub_endpoint);

    let datetime = post_util::get_local_datetime(&post.created_at, &site_config.micropub.current_timezone_offset).map_err(|e| {
        error!("date parsing error: {:?}", e);
        // TODO shouldn't be a template error but realistically this would only happen if
        // the DB had malformed data for template rendering...
        TemplateError
    })?;
    post.created_at = datetime.to_rfc3339();

    let post_view = PostView::new_from(post, tags, DateView::from(&datetime), photos);
    let articles_page = ArticlesPage {
        number: 1,
        object_list: vec![post_view],
    };
    let page = template
        .add_context("articles_page", &articles_page)
        .render("index.html")
        .map_err(|e| {
            error!("{:?}", e);
            TemplateError
        })?;

    Ok(Html(page))
}
