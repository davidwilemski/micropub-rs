use std::collections::HashMap;
use std::sync::Arc;

use diesel::prelude::*;
use diesel::r2d2;
use log::error;

use crate::errors::*;
use crate::handler::{MicropubDB, WithDB};
use crate::models::Post;
use crate::post_util;
use crate::templates;
use crate::view_models::{Date as DateView, Post as PostView};

use axum::response::IntoResponse;
use http::{header, StatusCode};

pub struct AtomHandler<DB: WithDB> {
    db: DB,
    templates: Arc<templates::Templates>,
}

pub async fn get_atom_handler(
    pool: Arc<r2d2::Pool<r2d2::ConnectionManager<SqliteConnection>>>,
    templates: Arc<templates::Templates>,
    site_config: Arc<crate::MicropubSiteConfig>,
) -> Result<impl IntoResponse, StatusCode> {
    let db = MicropubDB::new(pool);
    let mut conn = db.dbconn()?;

    let posts = Post::all()
        .load::<Post>(&mut conn)
        .map_err(|e: diesel::result::Error| db.handle_errors(e))?;

    use crate::schema::categories::dsl::*;
    let mut posts_views = vec![];
    let post_ids = posts.iter().map(|p| p.id).collect::<Vec<i32>>();
    let mut query_result: Vec<(i32, String)> = categories
        .select((post_id, category))
        .filter(post_id.eq_any(&post_ids))
        .get_results(&mut conn)
        .map_err(|e| db.handle_errors(e))?;

    query_result.sort_by_key(|item| item.0);
    let mut tags: HashMap<i32, Vec<String>> = HashMap::new();
    for (post_id_, tag) in query_result {
        tags.entry(post_id_).or_default().push(tag);
    }

    use crate::schema::photos::dsl as photos_dsl;
    let photos: Vec<(i32, String, Option<String>)> = photos_dsl::photos
        .select((photos_dsl::post_id, photos_dsl::url, photos_dsl::alt))
        .filter(photos_dsl::post_id.eq_any(&post_ids))
        .get_results(&mut conn)
        .map_err(|e| db.handle_errors(e))?;
    let mut photos_by_post: HashMap<i32, Vec<(String, Option<String>)>> = HashMap::new();
    for (post_id_, url, alt) in photos {
        photos_by_post.entry(post_id_).or_default().push((url, alt));
    }

    for mut post in posts {
        // TODO this is copied from FetchHandler. Both should not do this and should instead be
        // handled e.g. at the view model creation time.
        let datetime = post_util::get_local_datetime(&post.created_at, &site_config.micropub.current_timezone_offset).map_err(|e| {
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
            photos_by_post.remove(&pid).unwrap_or(vec![]),
        );
        posts_views.push(post_view);
    }

    // posts_views is sorted desc from the DB
    let last_updated = posts_views
        .iter()
        .map(|p| p.updated.as_str())
        .nth(0)
        .unwrap_or(&"2020-11-27 16:14:30"); // TODO allow configuration?

    let template = templates
        .add_context("updated_date", last_updated)
        .add_context("posts", &posts_views);
    let feed = template.render("atom.xml").map_err(|e| {
        error!("{:?}", e);
        TemplateError
    })?;

    Ok((StatusCode::OK, [(header::CONTENT_TYPE, "text/xml")], feed))
}
