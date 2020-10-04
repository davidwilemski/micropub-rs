use std::sync::Arc;

use diesel::prelude::*;
use diesel::r2d2;
use tera::{Context, Tera};
use warp::{reject, Rejection};

use crate::errors::*;
use crate::models::Post;
use crate::view_models::{Date as DateView, Post as PostView};

pub struct FetchHandler {
    dbpool: Arc<r2d2::Pool<r2d2::ConnectionManager<SqliteConnection>>>,
    templates: Arc<Tera>,
}

impl FetchHandler {
    pub fn new(
        pool: Arc<r2d2::Pool<r2d2::ConnectionManager<SqliteConnection>>>,
        templates: Arc<Tera>,
    ) -> Self {
        FetchHandler {
            dbpool: pool,
            templates,
        }
    }

    pub async fn fetch_post(&self, url_slug: &str) -> Result<impl warp::Reply, Rejection> {
        let conn = self.dbpool.get().map_err(|e| {
            println!("{:?}", e);
            reject::custom(DBError)
        })?;

        let mut post = Post::by_slug(url_slug).first::<Post>(&conn).map_err(
            |e: diesel::result::Error| match e {
                diesel::result::Error::NotFound => warp::reject::not_found(),
                _ => {
                    println!("{:?}", e);
                    reject::custom(DBError)
                }
            },
        )?;

        use crate::schema::categories::dsl::*;
        let tags: Vec<String> = categories
            .select(category)
            .filter(post_id.eq(post.id))
            .get_results(&conn)
            .map_err(|e| {
                println!("{:?}", e);
                reject::custom(DBError)
            })?;

        let mut base_ctx = Context::new();
        base_ctx.insert("DEFAULT_LANG", "en-US");
        base_ctx.insert("SITENAME", "David's Blog");
        base_ctx.insert("SITEURL", "");
        base_ctx.insert("MENUITEMS", crate::MENU_ITEMS);

        println!("input datetime: {:?}", post.created_at);
        let datetime = chrono::NaiveDateTime::parse_from_str(&post.created_at, "%Y-%m-%d %H:%M:%S")
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
        base_ctx.insert("article", &post_view);

        let page = self
            .templates
            .render("article.html", &base_ctx)
            .map_err(|e| {
                println!("{:?}", e);
                reject::custom(TemplateError)
            })?;
        Ok(warp::reply::html(page))
    }
}
