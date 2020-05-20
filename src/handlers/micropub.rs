use std::sync::Arc;

use diesel::prelude::*;
use diesel::r2d2;
use reqwest;
use serde::Deserialize;
use serde_qs;
use warp::http::StatusCode;
use warp::{reject, Rejection};

use crate::errors::*;
use crate::models::{NewCategory, NewPost};
use crate::post_util;
use crate::schema::{categories, posts};

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

fn get_latest_post_id(conn: &SqliteConnection) -> Result<i32, diesel::result::Error> {
    use crate::schema::posts::dsl::*;
    posts.select(id).order(id.desc()).limit(1).first(conn)
}

pub struct MicropubHandler {
    http_client: reqwest::Client,
    dbpool: Arc<r2d2::Pool<r2d2::ConnectionManager<SqliteConnection>>>,
}

impl MicropubHandler {
    pub fn new(pool: Arc<r2d2::Pool<r2d2::ConnectionManager<SqliteConnection>>>) -> Self {
        let handler = MicropubHandler {
            http_client: reqwest::Client::new(),
            dbpool: pool,
        };

        handler
    }

    pub async fn verify_auth(
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
            .get(crate::AUTH_TOKEN_ENDPOINT)
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

        if validate_response.me != crate::HOST_WEBSITE {
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

        let conn = self.dbpool.get().map_err(|e| {
            println!("{:?}", e);
            reject::custom(DBError)
        })?;

        conn.transaction::<_, anyhow::Error, _>(|| {
            diesel::insert_into(posts::table)
                .values(&new_post)
                .execute(&conn)?;
            let post_id = get_latest_post_id(&conn)?;
            let new_categories: Vec<NewCategory> = form
                .category
                .iter()
                .map(|c| NewCategory {
                    post_id: post_id,
                    category: c.as_str(),
                })
                .collect();

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
            StatusCode::CREATED,
        ))
    }
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
