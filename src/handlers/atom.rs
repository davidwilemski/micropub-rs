use std::sync::Arc;

use diesel::prelude::*;
use diesel::r2d2;
use itertools::Itertools;
use warp::{http::Response, reject, Rejection};

use crate::errors::*;
use crate::models::Post;
use crate::templates;
use crate::view_models::{Date as DateView, Post as PostView};

pub struct AtomHandler {
    dbpool: Arc<r2d2::Pool<r2d2::ConnectionManager<SqliteConnection>>>,
    templates: Arc<templates::Templates>,
}

impl AtomHandler {
    pub fn new(
        pool: Arc<r2d2::Pool<r2d2::ConnectionManager<SqliteConnection>>>,
        templates: Arc<templates::Templates>,
    ) -> Self {
        Self {
            dbpool: pool,
            templates,
        }
    }

    pub async fn get(&self) -> Result<impl warp::Reply, Rejection> {
        let conn = self.dbpool.get().map_err(|e| {
            println!("{:?}", e);
            reject::custom(DBError)
        })?;

        let posts =
            Post::all()
                .load::<Post>(&conn)
                .map_err(|e: diesel::result::Error| match e {
                    diesel::result::Error::NotFound => warp::reject::not_found(),
                    _ => {
                        println!("{:?}", e);
                        reject::custom(DBError)
                    }
                })?;

        use crate::schema::categories::dsl::*;
        let mut posts_views = vec![];
        for mut post in posts {
            let tags = categories
                .select(category)
                .filter(post_id.eq(post.id))
                .get_results(&conn)
                .map_err(|e| {
                    println!("{:?}", e);
                    reject::custom(DBError)
                })?;

            // TODO this is copied from FetchHandler. Both should not do this and should instead be
            // handled e.g. at the view model creation time.
            let datetime =
                chrono::NaiveDateTime::parse_from_str(&post.created_at, "%Y-%m-%d %H:%M:%S")
                    .map(|ndt| {
                        chrono::DateTime::<chrono::Local>::from_utc(
                            ndt,
                            chrono::FixedOffset::east(7 * 3600),
                        )
                    })
                    .map_err(|e| {
                        println!("date parsing error: {:?}", e);
                        // TODO shouldn't be a template error but realistically this would only happen if
                        // the DB had malformed data for template rendering...
                        reject::custom(TemplateError)
                    })?;
            post.created_at = datetime.to_rfc3339();

            let post_view = PostView::new_from(post, tags, DateView::from(&datetime));
            posts_views.push(post_view);
        }

        let sorted_posts = posts_views
            .iter()
            .map(|p| p.updated.as_str())
            .sorted();
        let last_updated = sorted_posts
            .as_slice()
            .last()
            .unwrap_or(&"2020-11-27 16:14:30"); // TODO allow configuration?

        let template = self
            .templates
            .add_context("updated_date", last_updated)
            .add_context("posts", &posts_views);
        let feed = template.render("atom.xml").map_err(|e| {
            println!("{:?}", e);
            reject::custom(TemplateError)
        })?;

        Ok(Response::builder()
            .status(200)
            .header("Content-Type", "text/xml")
            .body(feed))
    }
}
