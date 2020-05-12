use std::sync::Arc;

use diesel::prelude::*;
use diesel::r2d2;
use tera::{Context, Tera};
use warp::{reject, Rejection};

use crate::errors::*;
use crate::models::Post;
use crate::view_models::{ArticlesPage, Date as DateView, Post as PostView};

pub struct IndexHandler {
    dbpool: Arc<r2d2::Pool<r2d2::ConnectionManager<SqliteConnection>>>,
    templates: Arc<Tera>,
}

impl IndexHandler {
    pub fn new(
        pool: Arc<r2d2::Pool<r2d2::ConnectionManager<SqliteConnection>>>,
        templates: Arc<Tera>,
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

        let mut post =
            Post::latest()
                .first::<Post>(&conn)
                .map_err(|e: diesel::result::Error| match e {
                    diesel::result::Error::NotFound => warp::reject::not_found(),
                    _ => {
                        println!("{:?}", e);
                        reject::custom(DBError)
                    }
                })?;

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
        let articles_page = ArticlesPage { number: 1, object_list: vec![post_view] };
        base_ctx.insert("articles_page", &articles_page);

        let page = self
            .templates
            .render("index.html", &base_ctx)
            .map_err(|e| {
                println!("{:?}", e);
                reject::custom(TemplateError)
            })?;
        Ok(warp::reply::html(page))
    }
}
