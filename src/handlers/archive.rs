use std::collections::HashMap;
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
use crate::view_models::{Date as DateView, Post as PostView};

use axum::response::{Html, IntoResponse};
use http::StatusCode;

pub struct ArchiveHandler<DB: WithDB> {
    db: DB,
    templates: Arc<templates::Templates>,
}

impl ArchiveHandler<MicropubDB> {
    pub fn new(
        pool: Arc<r2d2::Pool<r2d2::ConnectionManager<SqliteConnection>>>,
        templates: Arc<templates::Templates>,
    ) -> Self {
        ArchiveHandler {
            db: MicropubDB::new(pool),
            templates,
        }
    }

    pub async fn get(&self, tag: Option<&str>) -> Result<impl warp::Reply, Rejection> {
        let conn = self.db.dbconn()?;
        let posts = tag
            .map(|t| Post::by_tag(t))
            .unwrap_or(Post::all())
            .load::<Post>(&conn)
            .map_err(|e: diesel::result::Error| match e {
                diesel::result::Error::NotFound => warp::reject::not_found(),
                _ => reject::custom(self.db.handle_errors(e)),
            })?;

        use crate::schema::categories::dsl::*;
        let mut posts_views = vec![];
        let post_ids = posts.iter().map(|p| p.id).collect::<Vec<i32>>();
        let mut query_result: Vec<(i32, String)> = categories
            .select((post_id, category))
            .filter(post_id.eq_any(post_ids))
            .get_results(&conn)
            .map_err(|e| self.db.handle_errors(e))?;
        query_result.sort_by_key(|item| item.0);
        let mut tags: HashMap<i32, Vec<String>> = HashMap::new();
        for (post_id_, tag) in query_result {
            tags.entry(post_id_).or_default().push(tag);
        }

        for mut post in posts {
            // TODO this is copied from FetchHandler. Both should not do this and should instead be
            // handled e.g. at the view model creation time.
            let datetime = post_util::get_local_datetime(&post.created_at, None).map_err(|e| {
                error!("date parsing error: {:?}", e);
                // TODO shouldn't be a template error but realistically this would only happen if
                // the DB had malformed data for template rendering...
                reject::custom(TemplateError)
            })?;
            post.created_at = datetime.to_rfc3339();

            let pid = post.id;
            let post_view = PostView::new_from(
                post,
                tags.remove(&pid).unwrap_or(vec![]),
                DateView::from(&datetime),
                vec![],
            );
            posts_views.push(post_view);
        }

        let template = self
            .templates
            .add_context("articles", &posts_views)
            .add_context("dates", &posts_views)
            .add_context("tag", &tag);
        let page = template.render("archives.html").map_err(|e| {
            error!("{:?}", e);
            reject::custom(TemplateError)
        })?;

        Ok(warp::reply::html(page))
    }
}

pub async fn get_archive_handler(
    tag: Option<String>,
    pool: Arc<r2d2::Pool<r2d2::ConnectionManager<SqliteConnection>>>,
    templates: Arc<templates::Templates>,
) -> Result<impl IntoResponse, StatusCode> {
    let tag_ref = tag.as_ref().map(|t| t.as_str());
    let db = MicropubDB::new(pool);
    let conn = db.dbconn()?;
    let posts = tag_ref
        .map(|t| Post::by_tag(t))
        .unwrap_or(Post::all())
        .load::<Post>(&conn)
        .map_err(|e| db.handle_errors(e))?;

    use crate::schema::categories::dsl::*;
    let mut posts_views = vec![];
    let post_ids = posts.iter().map(|p| p.id).collect::<Vec<i32>>();
    let mut query_result: Vec<(i32, String)> = categories
        .select((post_id, category))
        .filter(post_id.eq_any(post_ids))
        .get_results(&conn)
        .map_err(|e| db.handle_errors(e))?;
    query_result.sort_by_key(|item| item.0);
    let mut tags: HashMap<i32, Vec<String>> = HashMap::new();
    for (post_id_, tag) in query_result {
        tags.entry(post_id_).or_default().push(tag);
    }

    for mut post in posts {
        // TODO this is copied from FetchHandler. Both should not do this and should instead be
        // handled e.g. at the view model creation time.
        let datetime = post_util::get_local_datetime(&post.created_at, None).map_err(|e| {
            error!("date parsing error: {:?}", e);
            // TODO shouldn't be a template error but realistically this would only happen if
            // the DB had malformed data for template rendering...
            TemplateError
        })?;
        post.created_at = datetime.to_rfc3339();

        let pid = post.id;
        let post_view = PostView::new_from(
            post,
            tags.remove(&pid).unwrap_or(vec![]),
            DateView::from(&datetime),
            vec![],
        );
        posts_views.push(post_view);
    }

    let template = templates
        .add_context("articles", &posts_views)
        .add_context("dates", &posts_views)
        .add_context("tag", &tag_ref);
    let page = template.render("archives.html").map_err(|e| {
        error!("{:?}", e);
        TemplateError
    })?;

    Ok(Html(page))
}
