use std::sync::Arc;

use diesel::prelude::*;
use diesel::r2d2;
use log::{debug, info, error};
use warp::{reject, Rejection};

use crate::errors::*;
use crate::handler::{MicropubDB, WithDB};
use crate::models::Post;
use crate::post_util;
use crate::templates;
use crate::view_models::{Date as DateView, Post as PostView};

pub struct FetchHandler<DB: WithDB> {
    db: DB,
    templates: Arc<templates::Templates>,
}

impl FetchHandler<MicropubDB> {
    pub fn new(
        pool: Arc<r2d2::Pool<r2d2::ConnectionManager<SqliteConnection>>>,
        templates: Arc<templates::Templates>,
    ) -> Self {
        FetchHandler {
            db: MicropubDB::new(pool),
            templates,
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

        use crate::schema::categories::dsl::*;
        let tags: Vec<String> = categories
            .select(category)
            .filter(post_id.eq(post.id))
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

        let post_view = PostView::new_from(post, tags, DateView::from(&datetime));
        let page = self.templates
            .add_context("article", &post_view)
            .render("article.html")
            .map_err(|e| {
                error!("{:?}", e);
                reject::custom(TemplateError)
            })?;
        Ok(warp::reply::html(page))
    }
}
