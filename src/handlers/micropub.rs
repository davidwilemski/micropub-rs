use std::collections::HashMap;
use std::sync::Arc;

use bytes::BufMut;
use chrono::Local;
use diesel::prelude::*;
use diesel::r2d2;
use futures::{StreamExt, TryStreamExt};
use log::{info, error};
use reqwest;
use url::form_urlencoded::parse;
use serde::{Deserialize, Serialize};
use serde_json::json;
use thiserror::Error;
use warp::http::{Response, StatusCode};
use warp::http::{header as http_header};
use warp::{reject, Rejection};
use warp::multipart;

use crate::errors::*;
use crate::handler::{MicropubDB, WithDB};
use crate::models::{NewCategory, NewOriginalBlob, NewPost, NewPhoto, NewMediaUpload};
use crate::post_util;
use crate::schema::{categories, original_blobs, posts, photos, media};

#[derive(Debug, Error)]
enum MicropubFormError {
    #[error("Required field '{0}' is missing.")]
    MissingField(String),
}

#[derive(Clone, Debug, Deserialize)]
#[serde(untagged)]
enum MicropubPropertyValue {
    Value(String),
    Values(Vec<String>),
    Map(HashMap<String, MicropubPropertyValue>),
    VecMap(Vec<std::collections::HashMap<String, MicropubPropertyValue>>),
    ValueVec(Vec<MicropubPropertyValue>),
}

#[derive(Clone, Debug, Deserialize)]
struct MicropubProperties(std::collections::HashMap<String, MicropubPropertyValue>);

impl MicropubProperties {
    fn new(props: std::collections::HashMap<String, MicropubPropertyValue>) -> Self {
        Self(props)
    }

    fn get(&self, prop: &str) -> Option<&MicropubPropertyValue> {
        self.0.get(prop)
    }
}

#[derive(Debug, Deserialize)]
struct MicropubJSONCreate {
    #[serde(rename = "type")]
    entry_type: Vec<String>,
    properties: MicropubProperties,
}

// An earlier take on this was an enum with Url and Props variants
#[derive(Clone, PartialEq, Debug, Deserialize, Serialize)]
struct Photo {
    url: String,
    alt: Option<String>,
}

// TODO:
// - quill appears to include 'published' and 'created' properties
// - food entries seem... complex. See food entry test case below
//   e.g. a 'drank' property may have a whole sub type/properties object...
//   I'd really like to support recording this for e.g. tea blogging but this might require a
//   larger refactor.
// - bookmark might have a bookmark-of property (possibly more likely to be a form encoded than
//   json encoded entry
// - review types (https://quill.p3k.io/review)
//   quill doesn't appeart to include categories in the form but that would be nice to support
//   adding a test case below, commented out
#[derive(Debug, Deserialize)]
struct MicropubFormBuilder {
    access_token: Option<String>,
    h: Option<String>,
    content: Option<String>,
    content_type: Option<String>,
    category: Option<Vec<String>>,
    name: Option<String>,
    created_at: Option<String>,
    updated_at: Option<String>,
    slug: Option<String>,
    bookmark_of: Option<String>,
    photos: Option<Vec<Photo>>,
}

fn set_from_prop<F>(builder: &mut MicropubFormBuilder, setter: &mut F, props: &MicropubProperties, prop: &str) -> bool
where F: Fn(&mut MicropubFormBuilder, MicropubPropertyValue) {
    props.get(prop).map(|prop| {
        setter(builder, (*prop).clone())
    }).is_some()
}

fn set_from_props<F>(builder: &mut MicropubFormBuilder, mut setter: F, props: &MicropubProperties, props_to_check: &[&str]) -> bool
where F: Fn(&mut MicropubFormBuilder, MicropubPropertyValue) {
    for prop in props_to_check {
        if set_from_prop(builder, &mut setter, props, prop) {
            return true;
        }
    }

    false
}

impl MicropubFormBuilder {
    fn new() -> Self {
        Self {
            access_token: None,
            h: None,
            content: None,
            content_type: None,
            category: None,
            name: None,
            created_at: None,
            updated_at: None,
            slug: None,
            bookmark_of: None,
            photos: None,
        }
    }

    fn from_json(json_bytes: &[u8]) -> Result<Self, anyhow::Error> {
        let json_create: MicropubJSONCreate = serde_json::from_slice(json_bytes)?;
        let mut builder = MicropubFormBuilder::new();

        if let Some(entry_type) = json_create.entry_type.first() {
            // Normalizes h-entry or h-food into entry and food
            builder.set_h(entry_type.strip_prefix("h-").unwrap_or(&entry_type).into())
        }

        let prop_setter_pairs: Vec<(&[&str], Box<dyn Fn(&mut MicropubFormBuilder, MicropubPropertyValue)>)> = vec![
            (&["content", "content[html]"][..], Box::new(|builder: &mut MicropubFormBuilder, val: MicropubPropertyValue| {
                match val {
                    MicropubPropertyValue::Values(vals) => {
                        vals.first().iter().for_each(|s| {
                            builder.set_content((**s).clone())
                        });
                    }
                    MicropubPropertyValue::VecMap(vecmap) => {
                        // we may get {"content": [{"html": "blah"}]}
                        // see test case
                        vecmap.first().iter().for_each(|map| {
                            if let Some(MicropubPropertyValue::Value(content)) = map.get("html") {
                                builder.set_content_type("html".into());
                                builder.set_content(content.clone());
                            } else if let Some(MicropubPropertyValue::Value(content)) = map.get("markdown") {
                                builder.set_content_type("markdown".into());
                                builder.set_content(content.clone());
                            }
                        });
                    }
                    MicropubPropertyValue::Value(val) => {
                        builder.set_content(val.clone());
                    }
                    _ => error!("unexpected content type")
                };
            })),
            (&["name"][..], Box::new(|builder: &mut MicropubFormBuilder, val: MicropubPropertyValue| {
                match val {
                    MicropubPropertyValue::Values(vals) => {
                        vals.first().iter().for_each(|s| {
                            builder.set_name((**s).clone())
                        });
                    }
                    _ => error!("unexpected name type")
                };
            })),
            (&["category"][..], Box::new(|builder: &mut MicropubFormBuilder, props: MicropubPropertyValue| {
                match props {
                    MicropubPropertyValue::Value(c) => {
                        builder.add_category(c);
                    }
                    MicropubPropertyValue::Values(cs) => {
                        cs.iter().for_each(|c| builder.add_category(c.clone()));
                    }
                    _ => error!("unexpected category type")
                };
            })),
            (&["published"][..], Box::new(|builder: &mut MicropubFormBuilder, props: MicropubPropertyValue| {
                match props {
                    MicropubPropertyValue::Values(dates) => {
                        if dates.len() != 1 {
                            error!("unexpected published dates length");
                            return;
                        }
                        builder.set_created_at(dates[0].clone())
                    },
                    _ => error!("unexpected published type"),
                }
            })),
            (&["mp-slug"][..], Box::new(|builder: &mut MicropubFormBuilder, props: MicropubPropertyValue| {
                match props {
                    MicropubPropertyValue::Values(slugs) => {
                        if slugs.len() != 1 {
                            error!("unexpected slugs length");
                            return;
                        }
                        builder.set_slug(slugs[0].clone())
                    },
                    MicropubPropertyValue::Value(slug) => builder.set_slug(slug),
                    _ => error!("unexpected slug type"),
                }
            })),
            (&["bookmark-of"][..], Box::new(|builder: &mut MicropubFormBuilder, props: MicropubPropertyValue| {
                match props {
                    MicropubPropertyValue::Values(mut bookmark_urls) => {
                        if bookmark_urls.len() != 1 {
                            // TODO log
                            return;
                        }
                        // TODO is there a different entry type we should set here? Should an extra
                        // post type column be added? Seems others (and clients) still set
                        // entry_type as h-entry so maybe the latter?
                        builder.set_bookmark_of(bookmark_urls.pop().expect("bookmark_urls len was checked as 1"));
                    }
                    _ => eprintln!("unexpected bookmark_of property type"),
                }
            })),
            (&["photo"][..], Box::new(|builder: &mut MicropubFormBuilder, props: MicropubPropertyValue| {
                builder.on_photo_props(props);
            })),
        ];

        for (props, setter) in prop_setter_pairs {
            set_from_props(&mut builder, setter, &json_create.properties, &props);
        }

        Ok(builder)
    }

    fn build(self) -> Result<MicropubForm, MicropubFormError> {
        Ok(MicropubForm {
            access_token: self.access_token,
            h: self.h.ok_or(MicropubFormError::MissingField("h".into()))?,
            content: self.content.ok_or(MicropubFormError::MissingField("content".into()))?,
            content_type: self.content_type,
            category: self.category.unwrap_or(vec![]),
            name: self.name,
            created_at: self.created_at,
            updated_at: self.updated_at,
            slug: self.slug,
            bookmark_of: self.bookmark_of,
            photos: self.photos,
        })
    }

    fn set_access_token(&mut self, val: String) {
        self.access_token = Some(val);
    }

    fn set_h(&mut self, val: String) {
        self.h = Some(val);
    }

    fn set_content(&mut self, val: String) {
        self.content = Some(val);
    }

    fn set_content_type(&mut self, val: String) {
        self.content_type = Some(val)
    }

    fn add_category(&mut self, val: String) {
        if let None = self.category {
            self.category = Some(vec![])
        }

        self.category.as_mut().map(|categories| categories.push(val));
    }

    fn set_name(&mut self, val: String) {
        self.name = Some(val);
    }

    fn set_created_at(&mut self, val: String) {
        self.created_at = Some(val)
    }

    fn set_slug(&mut self, val: String) {
        self.slug = Some(val)
    }

    fn set_bookmark_of(&mut self, val: String) {
        self.bookmark_of = Some(val)
    }

    fn add_photo(&mut self, val: Photo) {
        if let None = self.photos {
            self.photos = Some(vec![]);
        }

        self.photos.as_mut().map(|photos| photos.push(val));
    }

    fn on_photo_props(
        &mut self,
        props: MicropubPropertyValue) 
    {
        match props {
            MicropubPropertyValue::Value(photo_url) => {
                self.add_photo(Photo{url: photo_url, alt: None});
            },
            MicropubPropertyValue::Values(mut photo_urls) => {
                photo_urls.drain(..).for_each(|photo_url| {
                    self.add_photo(Photo{url: photo_url, alt: None});
                });
            },
            MicropubPropertyValue::Map(mut props) => {
                if let Some(MicropubPropertyValue::Value(url)) = props.remove("value") {
                    let alt = match props.remove("alt") {
                        Some(MicropubPropertyValue::Value(alt)) => Some(alt),
                        _ => None
                    };
                    let photo = Photo {
                        url: url,
                        alt: alt,
                    };
                    self.add_photo(photo);
                }
            },
            MicropubPropertyValue::VecMap(mut props_vec) => {
                for mut props in props_vec.drain(..) {
                    if let Some(MicropubPropertyValue::Value(url)) = props.remove("value") {
                        let alt = match props.remove("alt") {
                            Some(MicropubPropertyValue::Value(alt)) => Some(alt),
                            _ => None
                        };
                        let photo = Photo {
                            url: url,
                            alt: alt,
                        };
                        self.add_photo(photo);
                    }
                }
            },
            MicropubPropertyValue::ValueVec(photos) => {
                for photo in photos {
                    self.on_photo_props(photo)
                }
            }
        }
    }
}

#[derive(Debug, PartialEq, Clone)]
struct MicropubForm {
    /// Access token (token used to authenticate the operation).
    /// May be used in place of a bearer token authorization header.
    access_token: Option<String>,

    /// Entry type
    h: String,

    /// Text content of the entry
    content: String,

    /// Content type of the entry. None for plain text / default, "html" for already rendered html,
    /// or "markdown" for content that should be rendered as html from markdown at post render
    /// time.
    content_type: Option<String>,

    /// Categories (tags) for the entry
    category: Vec<String>,

    /// Name/Title of the h-entry (article/blog post).
    /// Note that h-notes do not contain a name.
    name: Option<String>,

    /// Created and Updated at datetimes of the post
    /// The database schema has a default of the current time but this can also be provided at post
    /// time.
    created_at: Option<String>,
    updated_at: Option<String>,

    /// Slug to use as part of URI
    slug: Option<String>,

    /// Indicates entry is a bookmark type. String should be a URL.
    bookmark_of: Option<String>,

    /// Photos included with the entry
    photos: Option<Vec<Photo>>,

    // TODO: support additional fields and properties
}

impl MicropubForm {
    fn from_form_bytes(b: &[u8]) -> Result<Self, anyhow::Error> {
        let p = parse(b);
        let mut builder = MicropubFormBuilder::new();
        for (k, v) in p {
            match &*k {
                "access_token" => builder.set_access_token(v.into_owned()),
                "h" => builder.set_h(v.into_owned()),
                content_key @ "content" | content_key @ "content[html]" => {
                    builder.set_content(v.into_owned());
                    if content_key == "content[html]" {
                        builder.set_content_type("html".into())
                    }
                },
                "category" | "category[]" => builder.add_category(v.into_owned()),
                "name" => builder.set_name(v.into_owned()),
                "bookmark-of" => builder.set_bookmark_of(v.into_owned()),
                _ => (),
            }
        }

        Ok(builder.build()?)
    }

    fn from_json_bytes(b: &[u8]) -> Result<Self, anyhow::Error> {
        Ok(MicropubFormBuilder::from_json(b)?.build()?)
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

pub struct MicropubHandler<DB: WithDB> {
    http_client: reqwest::Client,
    db: DB,
    config: serde_json::Value,
}

impl MicropubHandler<MicropubDB> {
    pub fn new(pool: Arc<r2d2::Pool<r2d2::ConnectionManager<SqliteConnection>>>, media_endpoint: String) -> Self {
        // TODO probably make this a struct down the road?
        let config = json!({
            "media-endpoint": media_endpoint,
        });

        let handler = MicropubHandler {
            http_client: reqwest::Client::new(),
            db: MicropubDB::new(pool),
            config: config,
        };

        handler
    }

    pub async fn handle_post(
        &self,
        content_type: Option<String>,
        auth: String,
        body: bytes::Bytes,
    ) -> Result<impl warp::Reply, Rejection> {
        info!("body: {:?}", &body.slice(..));

        let validate_response = self.verify_auth(&auth).await?;

        if validate_response.me != crate::HOST_WEBSITE {
            return Err(reject::custom(NotAuthorized));
        }

        let slug = self.create_post(
            content_type,
            body,
            validate_response.client_id.as_str()
        ).await?;

        Ok(
            warp::reply::with_header(
                warp::reply::with_status(
                    warp::reply::reply(),
                    StatusCode::CREATED,
                ),
                "Location",
                format!("https://davidwilemski.com/{}", slug)
            )
        )
    }

    pub async fn handle_query(
        &self,
        auth: String,
        query: Vec<(String, String)>,
    ) -> Result<impl warp::Reply, Rejection> {
        // looking for ?q=config
        info!("query: {:?}", query);
        let is_query = query.iter().find_map(|(header, value)| {
            if header == "q" && value == "config" {
                Some(value)
            } else {
                None
            }
        });
        if let Some(_) = is_query {
            // verify auth
            let validate_response = self.verify_auth(&auth).await?;

            if validate_response.me != crate::HOST_WEBSITE {
                return Err(reject::custom(NotAuthorized));
            }

            // return media endpoint
            return Ok(self.config.to_string())
        }

        // TODO handle other types of queries like content queries
        return Err(reject::not_found());
    }

    pub async fn handle_media_upload(
        &self,
        auth: String,
        multipart_data: multipart::FormData,
    ) -> Result<impl warp::Reply, Rejection> {
        // verify auth
        let validate_response = self.verify_auth(&auth).await?;

        if validate_response.me != crate::HOST_WEBSITE {
            return Err(reject::custom(NotAuthorized));
        }

        // find Part that has the name 'file'
        let maybe_file = Box::pin(multipart_data.filter_map(|maybe_part| async {
            match maybe_part {
                Ok(part) if part.name() == "file" => Some(Ok(part)),
                Ok(_) => None,
                Err(e) => {
                    error!("Error reading multipart form: {:?}", e);
                    Some(Err(reject::custom(MediaUploadError)))
                }
            }
        }))
        .next()
        .await;

        match maybe_file {
            // now try to read in part data
            Some(Ok(part)) => {
                let filename: Option<String> = part.filename().map(|s| s.into());
                let content_type: Option<String> = part.content_type().map(|s| s.into());
                // read in upload and PUT to rustyblobjectstore backend
                // TODO it seems possible to pass the Stream of parts directly
                // to reqwest rather than buffering in memory. We should do that here.
                let contents: Vec<u8> = part.stream()
                    .try_fold(Vec::new(), |mut acc, data| {
                        acc.put(data);
                        async move { Ok(acc) }
                    })
                    .await
                    .map_err(|e| {
                        error!("error accumulating request body: {:?}", e);
                        reject::custom(MediaUploadError)
                    })?;
                let client = reqwest::Client::new();
                let resp = client.put("http://rustyblobjectstore:3031/")
                    .body(contents)
                    .send()
                    .await
                    .map_err(|e| {
                        error!("error in PUT to rustyblobjectstore: {:?}", e);
                        reject::custom(MediaUploadError)
                    })?;

                let status = resp.status();
                if status != 201 && status != 200 {
                    error!("unsuccessful response status from rustyblobjectstore: {:?}", status);
                    return Err(reject::custom(MediaUploadError))
                }

                // get the key of the blob to construct the URL used for fetching
                let hex_digest = resp.text()
                    .await
                    .map_err(|e| {
                        error!("failure to read response body from rustyblobjectstore: {:?}", e);
                        reject::custom(MediaUploadError)
                    })?;

                let new_media = NewMediaUpload {
                    hex_digest: &hex_digest,
                    filename: filename.as_deref(),
                    content_type: content_type.as_deref(),
                };
                let conn = self.db.dbconn()?;
                diesel::insert_into(media::table)
                    .values(&new_media)
                    .execute(&conn)
                    .map_err(|e| {
                        error!("error inserting hex digest into media uploads: {:?}", e);
                        reject::custom(DBError)
                    })?;


                // TODO it seems like the Location header is not being respected due to the body
                // content of empty string?
                let result = Ok(
                    Response::builder()
                        .status(StatusCode::CREATED)
                        .header(
                            // XXX the http crate forces header names to be lower case, even if you
                            // pass in a string that contains upper case characters. The 1.x
                            // standard says header names should be case insensitive and the 2.0
                            // standard forces lower case I guess.  The problem is that Quill is
                            // currently case sensitive and won't find the location header:
                            // https://github.com/aaronpk/Quill/blob/cdbc6aa4f305529f618e19b5af31ed896fb0a673/lib/helpers.php#L123
                            // A proxy may be needed to resolve this if a fix cannot be pushed to
                            // the quill client.
                            http_header::LOCATION,
                            format!("https://davidwilemski.com/media/{}", hex_digest)
                        )
                        .body(warp::hyper::Body::empty())
                        .map_err(|e| {
                            error!("error building response: {:?}", e);
                            reject::custom(MediaUploadError)
                        })?
                    // Box::new(warp::reply::with_status(
                    //     Box::new(warp::reply::with_header(
                    //         warp::reply::reply(),
                    //         "Location",
                    //         // TODO figure out media path/subdomain format
                    //     )),
                    //     StatusCode::CREATED,
                    // ))
                );
                info!("returning 201 on media upload: {:?}", result);
                result

            },
            // Some error reading multipart form parts
            Some(Err(e)) => Err(e),
            // No 'file' part found in multipart form
            _ => Ok(
                    Response::builder()
                        .status(StatusCode::BAD_REQUEST)
                        .body(warp::hyper::Body::empty())
                        .map_err(|e| {
                            error!("error building response: {:?}", e);
                            reject::custom(MediaUploadError)
                        })?
                )
        }

        // record rustyblobjectstore response (the blobject key), create media table entry respond
        // with media URL (micropub-rs needs to handle this still because we want to proxy the
        // rustyblobjectstore backend).
        // Ok("test")
    }

    /// Given an content type and body bytes, parse body and create post entry in the database.
    ///
    /// Returns slug string if successful
    pub async fn create_post(
        &self,
        content_type: Option<String>,
        body: bytes::Bytes,
        client_id: &str,
    ) -> Result<String, Rejection> {
        let ct = content_type.unwrap_or("x-www-form-url-encoded".into());
        let form = match ct.to_lowercase().as_str() {
            "application/json" => {
                MicropubForm::from_json_bytes(&body.slice(..)).map_err(|e| {
                    error!("{:?}", e);
                    reject::custom(ValidateResponseDeserializeError)
                })?
            }
            _ => {
                // x-www-form-urlencoded
                MicropubForm::from_form_bytes(&body.slice(..)).map_err(|e| {
                    error!("{:?}", e);
                    reject::custom(ValidateResponseDeserializeError)
                })?
            }
        };

        let slug = match form.slug {
            Some(ref s) => s.clone(),
            None => post_util::get_slug(form.name.as_deref(), Local::now),
        };

        let new_post = NewPost {
            name: form.name.as_deref(),
            slug: &slug, // TODO support inputting slug as part of the Micropub document/form
            entry_type: &form.h,
            content: Some(&form.content),
            content_type: form.content_type.as_ref().map(|s| s.as_ref()),
            client_id: Some(client_id),
            created_at: form.created_at.as_deref(),
            updated_at: form.updated_at.as_deref(),
            bookmark_of: form.bookmark_of.as_deref(),
        };

        self.db.run_txn(|conn| {
            diesel::insert_into(posts::table)
                .values(&new_post)
                .execute(conn)?;
            let post_id = get_latest_post_id(conn)?;
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
                    .execute(conn)?;
            }

            let original_blob = NewOriginalBlob {
                post_id: post_id,
                post_blob: &body,
            };
            diesel::insert_into(original_blobs::table)
                .values(original_blob)
                .execute(conn)?;

            if let Some(ref photos) = form.photos {
                let new_photos: Vec<NewPhoto> = photos
                    .iter()
                    .map(|p| NewPhoto {
                        post_id: post_id,
                        url: p.url.as_str(),
                        alt: p.alt.as_ref().map(|a| a.as_str()),
                    })
                    .collect();

                for p in new_photos {
                    diesel::insert_into(photos::table)
                        .values(p)
                        .execute(conn)?;
                }
            }

            Ok(())
        })?;

        Ok(slug)
    }

    async fn verify_auth(
        &self,
        auth: &str,
    ) -> Result<TokenValidateResponse, Rejection> {

        let r = self
            .http_client
            .get(crate::AUTH_TOKEN_ENDPOINT)
            .header("accept", "application/json")
            .header("Authorization", auth)
            .send()
            .await;

        let validate_response: TokenValidateResponse = r
            .map_err(|e| {
                error!("{:?}", e);
                reject::custom(HTTPClientError)
            })?
            .json()
            .await
            .map_err(|e| {
                error!("{:?}", e);
                reject::custom(ValidateResponseDeserializeError)
            })?;

        info!(
            "validate_resp: {:?}, scopes: {:?}",
            validate_response,
            validate_response.scopes()
        );

        Ok(validate_response)
    }
}

#[cfg(test)]
mod test {
    use super::{Photo, MicropubForm};

    #[test]
    fn micropub_form_decode_category_as_array() {
        let qs = b"h=entry&content=this+is+only+a+test+of+micropub&category%5B%5D=test&category%5B%5D=micropub";
        let form = MicropubForm {
            access_token: None,
            name: None,
            h: "entry".into(),
            content: "this is only a test of micropub".into(),
            content_type: None,
            category: vec!["test".into(), "micropub".into()],
            created_at: None,
            updated_at: None,
            slug: None,
            bookmark_of: None,
            photos: None,
        };

        assert_eq!(form, MicropubForm::from_form_bytes(&qs[..]).unwrap());
    }

    #[test]
    fn micropub_form_decode_category_as_single_param_into_vec() {
        let qs = b"h=entry&content=this+is+only+a+test+of+micropub&category=micropub";
        let form = MicropubForm {
            access_token: None,
            name: None,
            h: "entry".into(),
            content: "this is only a test of micropub".into(),
            content_type: None,
            category: vec!["micropub".into()],
            created_at: None,
            updated_at: None,
            slug: None,
            bookmark_of: None,
            photos: None,
        };

        assert_eq!(form, MicropubForm::from_form_bytes(&qs[..]).unwrap());
    }

    #[test]
    fn micropub_form_decode_category_missing_empty_vec() {
        let qs = b"h=entry&content=this+is+only+a+test+of+micropub";
        let form = MicropubForm {
            access_token: None,
            name: None,
            h: "entry".into(),
            content: "this is only a test of micropub".into(),
            content_type: None,
            category: vec![],
            created_at: None,
            updated_at: None,
            slug: None,
            bookmark_of: None,
            photos: None,
        };

        assert_eq!(form, MicropubForm::from_form_bytes(&qs[..]).unwrap());
    }

    #[test]
    fn micropub_form_decode_content_html() {
        let qs = b"h=entry&name=Test%20Article%20from%20Micropublish.net&content[html]=%3Cdiv%3EThis%20is%20a%20test%20article%3Cbr%3E%3Cbr%3E%3Cstrong%3EIt%20has%20formatting%3Cbr%3E%3Cbr%3E%3C%2Fstrong%3EIt%20can%20%3Ca%20href%3D%22https%3A%2F%2Fdavidwilemski.com%22%3Eembed%20links%3C%2Fa%3E%3C%2Fdiv%3E&category=test&post-status=published&mp-slug=test-article-micropublish-net";
        let form = MicropubForm {
            access_token: None,
            name: Some("Test Article from Micropublish.net".into()),
            h: "entry".into(),
            content: "<div>This is a test article<br><br><strong>It has formatting<br><br></strong>It can <a href=\"https://davidwilemski.com\">embed links</a></div>".into(),
            content_type: Some("html".into()),
            category: vec!["test".into()],
            created_at: None,
            updated_at: None,
            slug: None,
            bookmark_of: None,
            photos: None,
        };

        assert_eq!(form, MicropubForm::from_form_bytes(&qs[..]).unwrap());
    }

    // #[test]
    // fn micropub_json_decode_food_entry() {
    //     b"{\"type\":[\"h-entry\"],\"properties\":{\"published\":[\"2020-10-03T14:10:06-05:00\"],\"created\":[\"2020-10-03T14:10:06-05:00\"],\"summary\":[\"Just drank: Earl Grey Tea\"],\"drank\":[{\"type\":[\"h-food\"],\"properties\":{\"name\":\"Earl Grey Tea\"}}]}}"
    // }

    // #[test]
    // fn micropub_json_decode_review() {
    //     b"{\"type\":[\"h-review\"],\"properties\":{\"item\":[{\"type\":[\"h-product\"],\"properties\":{\"name\":[\"Something something something tea\"],\"url\":[\"\"]}}],\"rating\":[3],\"content\":[{\"html\":\"test review\"}],\"summary\":[\"it's ok\"]}}";
    // }

    #[test]
    fn micropub_json_decode_post_entry_from_quill() {
        let bytes = b"{\"type\":[\"h-entry\"],\"properties\":{\"name\":[\"Testing quill\"],\"content\":[{\"html\":\"<p>This is a test of https:\\/\\/quill.p3k.io<\\/p>\\n<p>\\n  hello hello\\n  <br \\/>\\n<\\/p>\"}],\"category\":[\"test\"],\"mp-slug\":[\"quill-test\"]}}";
        let form = MicropubForm {
            access_token: None,
            name: Some("Testing quill".into()),
            h: "entry".into(),
            content: "<p>This is a test of https://quill.p3k.io</p>\n<p>\n  hello hello\n  <br />\n</p>".into(),
            content_type: Some("html".into()),
            category: vec!["test".into()],
            created_at: None,
            updated_at: None,
            slug: Some("quill-test".into()),
            bookmark_of: None,
            photos: None,
        };

        assert_eq!(form, MicropubForm::from_json_bytes(&bytes[..]).unwrap());
    }

    #[test]
    fn micropub_json_decode_bookmark_of_entry() {
        let bytes = b"{\"type\":[\"h-entry\"],\"properties\":{\"name\":[\"Testing bookmarks\"],\"content\":[\"Bookmark test\"],\"bookmark-of\":[\"https://davidwilemski.com\"]}}";
        let form = MicropubForm {
            access_token: None,
            name: Some("Testing bookmarks".into()),
            h: "entry".into(),
            content: "Bookmark test".into(),
            content_type: None,
            category: vec![],
            created_at: None,
            updated_at: None,
            slug: None,
            bookmark_of: Some("https://davidwilemski.com".into()),
            photos: None,
        };

        assert_eq!(form, MicropubForm::from_json_bytes(&bytes[..]).unwrap());
    }

    #[test]
    fn micropub_json_decode_post_entry_markdown_format() {
        let bytes = b"{\"type\":[\"h-entry\"],\"properties\":{\"name\":[\"Testing markdown\"],\"content\":[{\"markdown\":\"This _is_ a *markdown* document. \\n # Header 1 \\n normal text\"}],\"category\":[\"markdown\"],\"mp-slug\":[\"markdown-test\"]}}";
        let form = MicropubForm {
            access_token: None,
            name: Some("Testing markdown".into()),
            h: "entry".into(),
            content: "This _is_ a *markdown* document. \n # Header 1 \n normal text".into(),
            content_type: Some("markdown".into()),
            category: vec!["markdown".into()],
            created_at: None,
            updated_at: None,
            slug: Some("markdown-test".into()),
            bookmark_of: None,
            photos: None,
        };

        assert_eq!(form, MicropubForm::from_json_bytes(&bytes[..]).unwrap());
    }

    #[test]
    fn micropub_json_decode_handles_published_property() {
        let bytes = b"{\"type\":[\"h-entry\"],\"properties\":{\"name\":[\"Testing published\"],\"content\":[{\"html\":\"content!\"}],\"category\":[\"publish-date\"],\"mp-slug\":[\"publish-date-slug\"], \"published\":[\"2020-04-04 15:30:00\"]}}";
        let form = MicropubForm {
            access_token: None,
            name: Some("Testing published".into()),
            h: "entry".into(),
            content: "content!".into(),
            content_type: Some("html".into()),
            category: vec!["publish-date".into()],
            created_at: Some("2020-04-04 15:30:00".into()),
            updated_at: None,
            slug: Some("publish-date-slug".into()),
            bookmark_of: None,
            photos: None,
        };

        assert_eq!(form, MicropubForm::from_json_bytes(&bytes[..]).unwrap());
    }

    #[test]
    fn micropub_form_decode_photo_property() {
        let bytes = b"{\"type\":[\"h-entry\"],\"properties\":{\"content\":[\"test upload\"],\"photo\":[{\"value\":\"https:\\/\\/davidwilemski.com\\/media\\/2a2ae02f9addf60f708298221e661db15b8afc340d8b934bc94b9e37f293074f\",\"alt\":\"test upload\"}]}}";
        let form = MicropubForm {
            access_token: None,
            name: None,
            h: "entry".into(),
            content: "test upload".into(),
            content_type: None,
            category: vec![],
            created_at: None,
            updated_at: None,
            slug: None,
            bookmark_of: None,
            photos: Some(vec![
                Photo {
                    url: "https://davidwilemski.com/media/2a2ae02f9addf60f708298221e661db15b8afc340d8b934bc94b9e37f293074f".into(),
                    alt: Some("test upload".into()),
                }
            ]),
        };

        assert_eq!(form, MicropubForm::from_json_bytes(&bytes[..]).unwrap());
    }

    #[test]
    fn micropub_form_decode_multiple_photo_property() {
        let bytes = b"{\"type\":[\"h-entry\"],\"properties\":{\"content\":[\"test upload\"],\"photo\":[{\"value\":\"https:\\/\\/davidwilemski.com\\/media\\/2a2ae02f9addf60f708298221e661db15b8afc340d8b934bc94b9e37f293074f\",\"alt\":\"test upload\"},\"https:\\/\\/davidwilemski.com\\/media\\/df1dfea9b0a062e8e27ee6fed1df597995547e16a73570107ff475b33d59f4fb\"]}}";
        let form = MicropubForm {
            access_token: None,
            name: None,
            h: "entry".into(),
            content: "test upload".into(),
            content_type: None,
            category: vec![],
            created_at: None,
            updated_at: None,
            slug: None,
            bookmark_of: None,
            photos: Some(vec![
                Photo {
                    url: "https://davidwilemski.com/media/2a2ae02f9addf60f708298221e661db15b8afc340d8b934bc94b9e37f293074f".into(),
                    alt: Some("test upload".into()),
                },
                Photo {
                    url: "https://davidwilemski.com/media/df1dfea9b0a062e8e27ee6fed1df597995547e16a73570107ff475b33d59f4fb".into(),
                    alt: None,
                }
            ]),
        };

        assert_eq!(form, MicropubForm::from_json_bytes(&bytes[..]).unwrap());
    }
}
