use std::sync::Arc;

use diesel::prelude::*;
use diesel::r2d2;
use log::error;
use warp::{reject, Rejection};

use crate::errors::*;
use crate::handler::{MicropubDB, WithDB};
use crate::models::Post;
use crate::post_util;
use crate::templates;
use crate::view_models::{ArticlesPage, Date as DateView, Post as PostView};

pub struct IndexHandler<DB: WithDB> {
    db: DB,
    templates: Arc<templates::Templates>,
}

impl IndexHandler<MicropubDB> {
    pub fn new(
        pool: Arc<r2d2::Pool<r2d2::ConnectionManager<SqliteConnection>>>,
        templates: Arc<templates::Templates>,
    ) -> Self {
        Self {
            db: MicropubDB::new(pool),
            templates,
        }
    }

    pub async fn get(&self) -> Result<impl warp::Reply, Rejection> {
        let conn = self.db.dbconn()?;

        let mut post =
            Post::latest()
                .first::<Post>(&conn)
                .map_err(|e: diesel::result::Error| match e {
                    diesel::result::Error::NotFound => warp::reject::not_found(),
                    _ => reject::custom(self.db.handle_errors(e)),
                })?;

        use crate::schema::categories::dsl::*;
        let tags: Vec<String> = categories
            .select(category)
            .filter(post_id.eq(post.id))
            .get_results(&conn)
            .map_err(|e| self.db.handle_errors(e))?;

        // Only on main page for indieauth login
        let template = self.templates
            .add_context("SOCIAL", &[crate::SOCIAL])
            .add_context("AUTH_ENDPOINT", crate::AUTH_ENDPOINT)
            .add_context("TOKEN_ENDPOINT", crate::TOKEN_ENDPOINT)
            .add_context("MICROPUB_ENDPOINT", crate::MICROPUB_ENDPOINT);

        let datetime = post_util::get_local_datetime(&post.created_at, None)
            .map_err(|e| {
                error!("date parsing error: {:?}", e);
                // TODO shouldn't be a template error but realistically this would only happen if
                // the DB had malformed data for template rendering...
                reject::custom(TemplateError)
            })?;
        post.created_at = datetime.to_rfc3339();

        let post_view = PostView::new_from(post, tags, DateView::from(&datetime));
        let articles_page = ArticlesPage { number: 1, object_list: vec![post_view] };
        let page = template
            .add_context("articles_page", &articles_page)
            .render("index.html")
            .map_err(|e| {
                error!("{:?}", e);
                reject::custom(TemplateError)
            })?;
        Ok(warp::reply::html(page))
    }
}
