#[macro_use]
extern crate diesel;

use std::env;
use std::sync::Arc;

use diesel::prelude::*;
use diesel::r2d2;
use reqwest;
use serde::Deserialize;
use serde_qs;
use warp::http::StatusCode;
use warp::{reject, Filter, Rejection};

mod schema;
mod post_util;

use schema::{posts, categories};

// TODO make these configurable via command line, environment, or config file?
const MAX_CONTENT_LENGTH: u64 = 1024 * 1024 * 50; // 50 megabytes
const AUTH_TOKEN_ENDPOINT: &str = "https://tokens.indieauth.com/token";
const HOST_WEBSITE: &str = "https://davidwilemski.com/";

#[derive(Debug, Deserialize)]
struct TokenValidateResponse {
    me: String,
    client_id: String,
    issued_at: i64,
    scope: String,
    nonce: i64,
}

impl TokenValidateResponse {
    fn scopes(&self) -> Vec<&str> {
        if self.scope == "" {
            vec![]
        } else {
            self.scope.split_whitespace().collect()
        }
    }
}

#[derive(Debug, PartialEq, Clone, Deserialize)]
struct MicropubForm {
    /// Access token (token used to authenticate the operation).
    /// May be used in place of a bearer token authorization header.
    access_token: Option<String>,

    /// Entry type
    h: String,

    /// Text content of the entry
    content: String,

    /// Categories (tags) for the entry
    category: Vec<String>,

    /// Name/Title of the h-entry (article/blog post).
    /// Note that h-notes do not contain a name.
    name: Option<String>,
    // TODO: support additional fields and properties
}

impl MicropubForm {
    fn from_bytes(b: &[u8]) -> Result<Self, serde_qs::Error> {
        let parser = serde_qs::Config::new(5, false);
        let v = parser.deserialize_bytes(b).unwrap();
        Ok(v)
    }
}

#[derive(Debug, Insertable)]
#[table_name="posts"]
pub struct NewPost<'a> {
    pub slug: &'a str,
    pub entry_type: &'a str,
    pub name: Option<&'a str>,
    pub content: Option<&'a str>,
    pub client_id: Option<&'a str>,
}

#[derive(Debug, Insertable)]
#[table_name="categories"]
pub struct NewCategory<'a> {
    post_id: i32,
    pub category: &'a str,
}

#[derive(Debug)]
struct HTTPClientError;
impl reject::Reject for HTTPClientError {}

#[derive(Debug)]
struct ValidateResponseDeserializeError;
impl reject::Reject for ValidateResponseDeserializeError {}

#[derive(Debug)]
struct NotAuthorized;
impl reject::Reject for NotAuthorized {}

#[derive(Debug)]
struct DBError;
impl reject::Reject for DBError {}

struct MicropubHandler {
    http_client: reqwest::Client,
    dbpool: Arc<r2d2::Pool<r2d2::ConnectionManager<SqliteConnection>>>,
}

fn new_dbconn_pool(db_file: &str) -> Result<r2d2::Pool<r2d2::ConnectionManager<SqliteConnection>>, anyhow::Error> {
        let manager = r2d2::ConnectionManager::<SqliteConnection>::new(db_file);
        Ok(r2d2::Pool::new(manager)?)
}

impl MicropubHandler {
    fn new(pool: Arc<r2d2::Pool<r2d2::ConnectionManager<SqliteConnection>>>) -> Self {
        let handler = MicropubHandler {
            http_client: reqwest::Client::new(),
            dbpool: pool,
        };

        handler
    }

    async fn verify_auth(
        &self,
        auth: String,
        body: bytes::Bytes,
    ) -> Result<impl warp::Reply, Rejection> {
        println!("body: {:?}", &body.slice(..));
        // TODO support other content types than x-www-form-urlencoded (e.g. JSON)
        // The urlencoded support is a must in the spec whereas JSON is a should.
        // V1 doesn't need it but it will need to come eventually.
        let form = MicropubForm::from_bytes(&body.slice(..)).map_err(|e| {
            println!("{:?}", e);
            reject::custom(ValidateResponseDeserializeError)
        })?;

        println!("auth: {:?} \n form: {:?}", auth, form);

        let r = self
            .http_client
            .get(AUTH_TOKEN_ENDPOINT)
            .header("accept", "application/json")
            .header("Authorization", auth)
            .send()
            .await;

        let validate_response: TokenValidateResponse = r
            .map_err(|e| {
                println!("{:?}", e);
                reject::custom(HTTPClientError)
            })?
            .json()
            .await
            .map_err(|e| {
                println!("{:?}", e);
                reject::custom(ValidateResponseDeserializeError)
            })?;

        println!(
            "validate_resp: {:?}, scopes: {:?}",
            validate_response,
            validate_response.scopes()
        );

        if validate_response.me != HOST_WEBSITE {
            return Err(reject::custom(NotAuthorized));
        }

        let slug = post_util::get_slug(form.name.as_deref(), &form.content);

        let new_post = NewPost {
            name: form.name.as_deref(),
            slug: &slug, // TODO support inputting slug as part of the Micropub document/form
            entry_type: &form.h,
            content: Some(&form.content),
            client_id: Some(&validate_response.client_id),
        };

        let conn = self.dbpool.get()
            .map_err(|e| {
                println!("{:?}", e);
                reject::custom(DBError)
            })?;

        conn
            .transaction::<_, anyhow::Error, _>(|| {
                diesel::insert_into(posts::table)
                    .values(&new_post)
                    .execute(&conn)?;
                let post_id = get_latest_post_id(&conn)?;
                let new_categories: Vec<NewCategory> = form
                    .category
                    .iter()
                    .map(|c| {
                        NewCategory {
                            post_id: post_id,
                            category: c.as_str()
                        }
                    }).collect();

                for c in new_categories {
                    diesel::insert_into(categories::table)
                        .values(c)
                        .execute(&conn)?;
                }

                Ok(())
            })
            .map_err(|e| {
                println!("{:?}", e);
                reject::custom(DBError)
            })?;

        Ok(warp::reply::with_status(
            warp::reply::reply(),
            StatusCode::OK,
        ))
    }
}

fn get_latest_post_id(conn: &SqliteConnection) -> Result<i32, diesel::result::Error> {
    use schema::posts::dsl::*;
    posts
        .select(id)
        .order(id.desc())
        .limit(1)
        .first(conn)
}

async fn handle_rejection(err: Rejection) -> Result<impl warp::Reply, Rejection> {
    // TODO JSON errors?
    if let Some(NotAuthorized) = err.find() {
        return Ok(warp::reply::with_status(
            "Not Authorized",
            StatusCode::FORBIDDEN,
        ));
    }

    let internal_server_error =
        warp::reply::with_status("", warp::http::StatusCode::INTERNAL_SERVER_ERROR);

    // Technically these really are not needed as 500 is the default response
    // for custom rejections but we could do some instrumentation or logging
    // here or whatever.
    if let Some(HTTPClientError) = err.find() {
        return Ok(internal_server_error);
    }
    if let Some(ValidateResponseDeserializeError) = err.find() {
        return Ok(internal_server_error);
    }

    // Otherwise pass the rejection through the filter stack
    Err(err)
}

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    let dbfile = env::var("DATABASE_URL")?;
    let dbpool = Arc::new(new_dbconn_pool(&dbfile)?);
    let micropub_handler = Arc::new(
        MicropubHandler::new(dbpool.clone())
    );

    let micropub = warp::path!("micropub")
        .and(warp::post())
        .and(warp::header::<String>("Authorization"))
        .and(warp::body::content_length_limit(MAX_CONTENT_LENGTH))
        .and(warp::body::bytes())
        .and_then(move |a, body| {
            let h = micropub_handler.clone();
            async move { h.verify_auth(a, body).await }
        })
        .recover(handle_rejection);

    warp::serve(micropub).run(([127, 0, 0, 1], 3030)).await;

    Ok(())
}

#[cfg(test)]
mod test {
    use super::MicropubForm;

    #[test]
    fn micropub_form_decode() {
        let qs = b"h=entry&content=this+is+only+a+test+of+micropub&category%5B%5D=test&category%5B%5D=micropub";
        let form = MicropubForm {
            access_token: None,
            name: None,
            h: "entry".into(),
            content: "this is only a test of micropub".into(),
            category: vec!["test".into(), "micropub".into()],
        };

        assert_eq!(form, MicropubForm::from_bytes(&qs[..]).unwrap());
    }
}
