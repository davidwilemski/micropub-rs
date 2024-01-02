use std::collections::HashMap;
use std::sync::Arc;

use chrono::Local;
use diesel::prelude::*;
use log::{info, error, warn};
use reqwest;
use url::form_urlencoded::parse;
use urlencoding::decode;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::errors::*;
use crate::handler::{MicropubDB, WithDB};
use crate::models::{NewCategory, NewOriginalBlob, NewPost, NewPhoto, NewMediaUpload, Post};
use crate::{media_util, post_util};
use crate::schema::{categories, original_blobs, posts, photos, media};

use axum::{
    extract::{Multipart, RawBody, Query},
    response::IntoResponse,
};
use http::{header, StatusCode, HeaderValue};

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
                    let photo = Photo {url, alt};
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
                        let photo = Photo {url, alt};
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

    fn from_post(p: &Post, categories: &[String], photos: &[(String, Option<String>)]) -> Self {
        let photos_out = if photos.len() > 0 {
            Some(photos.into_iter().map(|(url, alt)| Photo { url: url.clone(), alt: alt.clone() }).collect())
        } else {
            None
        };

        Self {
            access_token: None,
            h: p.entry_type.clone(),
            content: p.content.clone().unwrap_or("".into()),
            content_type: p.content_type.clone(),
            category: Vec::from(categories),
            name: p.name.clone(),
            created_at: Some(p.created_at.clone()),
            updated_at: Some(p.updated_at.clone()),
            slug: Some(p.slug.clone()),
            bookmark_of: p.bookmark_of.clone(),
            photos: photos_out,
        }
    }

    fn to_properties_json(&self) -> Result<String, anyhow::Error> {
        let mut result = json!({
            "type": vec![format!("h-{}", self.h)],
            "properties": {
                "mp-slug": vec![&self.slug],
                "category": self.category.clone(),
                "published": vec![&self.created_at],
                "updated": vec![&self.updated_at],
            }
        });

        let m = result.get_mut("properties")
            .expect("we know the key exists")
            .as_object_mut()
            .expect("we know this is an object");
        match self.content_type.as_ref().map(|s| s.as_str()) {
            None => {
                m.insert("content".into(), json!(vec![serde_json::Value::String(self.content.clone())]));
            },
            Some("html") => {
                m.insert("content".into(), json!(vec![json!({"html": &self.content})]));
            },
            Some("markdown") => {
                // for now, just send as non-rendered (raw markdown)
                m.insert("content".into(), json!(vec![serde_json::Value::String(self.content.clone())]));
            },
            Some(_) => panic!("unimplemented"),
        };
        self.name.iter().for_each(|n| {
            m.insert("name".into(), json!(vec![n]));
        });
        self.bookmark_of.iter().for_each(|b| {
            m.insert("bookmark-of".into(), json!(vec![b]));
        });
        self.photos.iter().for_each(|photos| {
            let photos_out: Vec<serde_json::Value> = photos.iter().map(|p| {
                let mut photo = json!({"value": p.url});
                if let Some(alt) = &p.alt {
                    photo.as_object_mut().expect("is object").insert("alt".into(), json!(alt));
                }
                photo
            }).collect();
            m.insert("photo".into(), json!(photos_out));
        });

        Ok(serde_json::to_string(&result)?)
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

fn get_latest_post_id(conn: &mut SqliteConnection) -> Result<i32, diesel::result::Error> {
    use crate::schema::posts::dsl::*;
    posts.select(id).order(id.desc()).limit(1).first(conn)
}

pub async fn handle_post(
    http_client: reqwest::Client,
    db: Arc<MicropubDB>,
    site_config: Arc<crate::MicropubSiteConfig>,
    headers: http::header::HeaderMap,
    RawBody(body): RawBody<axum::body::Body>,
) -> Result<impl IntoResponse, StatusCode> {
    let content_type = headers.get("Content-Type");
    let auth = headers.get("Authorization");

    if let None = auth {
        return Err(StatusCode::FORBIDDEN);
    }

    let auth_header_val: String = auth
        .expect("checked auth contents")
        .to_str()
        .map_err(|e| {
            error!("error getting authorization header ascii contents: {:?}", e);
            StatusCode::INTERNAL_SERVER_ERROR

        })?
        .into();

    let validate_response = verify_auth(
        http_client,
        site_config.clone(),
        &auth_header_val
    ).await?;

    if validate_response.me != site_config.micropub.host_website {
        error!(
            "mismatched authorization: me: {} host_website: {}",
           validate_response.me,
           site_config.micropub.host_website
        );
        return Err(StatusCode::FORBIDDEN);
    }

    let body_bytes: bytes::Bytes = hyper::body::to_bytes(body)
        .await
        .map_err(|e| {
            error!("error reading bytes from body: {:?}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    let slug = create_post(
        db.clone(),
        content_type,
        body_bytes,
        validate_response.client_id.as_str()
    ).await?;

    Ok((
        StatusCode::CREATED,
        [
            (header::LOCATION, format!("https://davidwilemski.com/{}", slug)),
        ],
    ))
}

pub async fn handle_query(
    http_client: reqwest::Client,
    config: Arc<serde_json::Value>,
    site_config: Arc<crate::MicropubSiteConfig>,
    headers: axum::http::HeaderMap,
    query: Query<Vec<(String, String)>>,
    db: Arc<MicropubDB>,
) -> Result<impl IntoResponse, StatusCode> {
    // looking for ?q=config
    info!("query: {:?}", query);
    let is_query = query.iter().find_map(|(header, value)| {
        if header == "q" {
            Some(value)
        } else {
            None
        }
    });
    if let Some(q) = is_query {
        // verify auth
        if let Some(auth_val) = headers.get("Authorization") {
            let auth: &str = auth_val.to_str()
                .map_err(|e| {
                    error!("failed to to_str() on auth_val: {:?}", e);
                    StatusCode::INTERNAL_SERVER_ERROR
                })?;
            let validate_response = verify_auth(
                http_client,
                site_config.clone(),
                auth
            ).await?;

            if validate_response.me != site_config.micropub.host_website {
                return Err(StatusCode::FORBIDDEN);
            }

            match q.as_str() {
                "config" => {
                    // return media endpoint
                    return Ok(config.to_string())
                },
                "source" => {
                    // return properties requested (or all?) if url in query matches one the server
                    // can provide.
                    let url = query.iter().find_map(|(key, value)| {
                        if key == "url" {
                            Some(value)
                        } else {
                            None
                        }
                    });
                    info!("source query for url: {:?}", url);

                    if let Some(url) = url {
                        let decoded_url = decode(url)
                            .map_err(|e| {
                                warn!("error decoding url: {}, error: {}", url, e);
                                StatusCode::BAD_REQUEST
                            })?;
                        info!("decoded url: {}", decoded_url);
                        if let Some(slug) = decoded_url.strip_prefix(site_config.micropub.host_website.as_str()) {
                            info!("stripped host website prefix");
                            // get post + categories + photos for the slug
                            let mut conn = db.dbconn()?;

                            let post = Post::by_slug(&slug)
                                .first::<Post>(&mut conn)
                                .map_err(|e| db.handle_errors(e))?;

                            use crate::schema::categories::dsl as category_dsl;
                            let tags: Vec<String> = category_dsl::categories
                                .select(category_dsl::category)
                                .filter(category_dsl::post_id.eq(post.id))
                                .get_results(&mut conn)
                                .map_err(|e| db.handle_errors(e))?;

                            use crate::schema::photos::dsl as photos_dsl;
                            let photos: Vec<(String, Option<String>)> = photos_dsl::photos
                                .select((photos_dsl::url, photos_dsl::alt))
                                .filter(photos_dsl::post_id.eq(post.id))
                                .get_results(&mut conn)
                                .map_err(|e| db.handle_errors(e))?;

                            let micropub_form = MicropubForm::from_post(&post, &tags, &photos);

                            // TODO only return the properties requested
                            return Ok(micropub_form.to_properties_json()
                                    .map_err(|e| {
                                        error!("error producing properties json: {:?}", e);
                                        StatusCode::INTERNAL_SERVER_ERROR
                                    })?
                                )
                        } else {
                            warn!("bad request - something else: {}", decoded_url);
                            return Err(StatusCode::BAD_REQUEST)
                        }
                    } else {
                        warn!("bad request - something");
                        return Err(StatusCode::BAD_REQUEST)
                    }
                },
                _ => {
                    warn!("bad request - passthrough query type: {}", q);
                    return Err(StatusCode::BAD_REQUEST)
                }
            }
        } else {
            warn!("unauthorized micropub query - missing authorization header");
            return Err(StatusCode::UNAUTHORIZED);
        }
    }

    // TODO handle other types of queries like content queries
    return Err(StatusCode::NOT_FOUND);
}

// TODO look at axum DefaultBodyLimit and adjust
pub async fn handle_media_upload(
    http_client: reqwest::Client,
    db: Arc<MicropubDB>,
    headers: axum::http::HeaderMap,
    mut multipart_data: Multipart,
    site_config: Arc<crate::MicropubSiteConfig>,
) -> Result<impl IntoResponse, StatusCode> {
    // verify auth
    if let Some(auth_val) = headers.get("Authorization") {
        let auth: &str = auth_val.to_str()
            .map_err(|e| {
                error!("failed to to_str() on auth_val: {:?}", e);
                StatusCode::INTERNAL_SERVER_ERROR
            })?;

        let validate_response = verify_auth(
            http_client.clone(),
            site_config.clone(),
            &auth
        ).await?;

        if validate_response.me != site_config.micropub.host_website {
            return Err(StatusCode::FORBIDDEN);
        }
    } else {
        return Err(StatusCode::UNAUTHORIZED);
    }

    // find Part that has the name 'file'
    // from the docs:
    // For security reasons it’s recommended to combine this with ContentLengthLimit to limit the size of the request payload.
    while let Ok(Some(field)) = multipart_data.next_field().await {
        match field.name() {
            Some(name) if name == "file" => {
                let filename: Option<String> = field.file_name().map(|s| s.into());
                let content_type: Option<String> = field.content_type().map(|s| s.into());
                let mut contents = field.bytes().await
                    .map_err(|e| {
                        error!("error reading request body: {:?}", e);
                        MediaUploadError
                    })?;

                // Pass media contents through imagemagick's strip functionality to remove things
                // like EXIF tags that might contain location or other private information.
                // attempt to get format
                let format = media_util::guess_format(&content_type.as_deref());
                match format {
                    // we think the content is some sort of image, strip it and reject the request
                    // if the strip operation fails
                    Some(f) => {
                        info!("content-type: {}", f);
                        info!("attempting to strip media starting with: {:?}", &contents[0..64]);
                        info!("length of media: {}", contents.len());
                        contents = media_util::strip_media(&contents, &f).map(|b| bytes::Bytes::from(b))?;
                    }
                    // still attempt to strip but don't reject if we fail
                    None => {
                        let f = "jpg";
                        match media_util::strip_media(&contents, f).map(|b| bytes::Bytes::from(b)) {
                            Ok(c) => contents = c,
                            Err(e) => {
                                // log error but we don't need to reject the whole request at this
                                // point because we don't know for sure the content type was
                                // image... this is not great given that there could still be exif
                                // data to stip in a non-image format and it could fail in this
                                // branch.
                                error!("error in stripping tags in unknown content-type: {:?}", e);
                            }
                        }
                    }
                };

                // PUT to rustyblobjectstore backend
                // TODO make object store URL configurable
                let resp = http_client.put(&site_config.blobject_store_base_uri)
                    .body(contents)
                    .send()
                    .await
                    .map_err(|e| {
                        error!("error in PUT to rustyblobjectstore: {:?}", e);
                        MediaUploadError
                    })?;

                let status = resp.status();
                if status != 201 && status != 200 {
                    error!("unsuccessful response status from rustyblobjectstore: {:?}", status);
                    return Err(StatusCode::INTERNAL_SERVER_ERROR);
                }

                // get the key of the blob to construct the URL used for fetching
                // record rustyblobjectstore response (the blobject key), create media table entry respond
                // with media URL (micropub-rs needs to handle this still because we want to proxy the
                // rustyblobjectstore backend).
                let hex_digest = resp.text()
                    .await
                    .map_err(|e| {
                        error!("failure to read response body from rustyblobjectstore: {:?}", e);
                        MediaUploadError
                    })?;

                let new_media = NewMediaUpload {
                    hex_digest: &hex_digest,
                    filename: filename.as_deref(),
                    content_type: content_type.as_deref(),
                };
                let mut conn = db.dbconn()?;
                diesel::insert_into(media::table)
                    .values(&new_media)
                    .execute(&mut conn)
                    .map_err(|e| {
                        error!("error inserting hex digest into media uploads: {:?}", e);
                        DBError::new()
                    })?;


                return Ok((
                    StatusCode::CREATED,
                    // XXX the http crate forces header names to be lower case, even if you
                    // pass in a string that contains upper case characters. The 1.x
                    // standard says header names should be case insensitive and the 2.0
                    // standard forces lower case I guess.  The problem is that Quill is
                    // currently case sensitive and won't find the location header:
                    // https://github.com/aaronpk/Quill/blob/cdbc6aa4f305529f618e19b5af31ed896fb0a673/lib/helpers.php#L123
                    // A proxy may be needed to resolve this if a fix cannot be pushed to
                    // the quill client.
                    [(
                        header::LOCATION,
                        format!("https://davidwilemski.com/media/{}", hex_digest) // TODO don't hardcode domain
                    )
                    ],
                ))
            }
            _ => {
                // Do nothing as we didn't find the upload
                ()
            }
        }
    }
    // TODO handle Err response from next_field here?
    // If we got here it was either an err or we didn't find the file upload
    // No 'file' part found in multipart form
    Err(StatusCode::BAD_REQUEST)
}

/// Given an content type and body bytes, parse body and create post entry in the database.
///
/// Returns slug string if successful
pub async fn create_post(
    db: Arc<MicropubDB>,
    content_type: Option<&HeaderValue>,
    body: bytes::Bytes,
    client_id: &str,
) -> Result<String, StatusCode> {
        // .map_err(|e| {
        //     error!("Error getting content type: {:?}", e);
        //     StatusCode::INTERNAL_SERVER_ERROR
        // })?;
    let ct: String = content_type
        .map(move |c| {
            c.to_str()
                .unwrap_or("x-www-form-url-encoded".into())
                .into()
        })
        .unwrap_or("x-www-form-url-encoded".into());
    let form = match ct.to_lowercase().as_str() {
        "application/json" => {
            MicropubForm::from_json_bytes(&body.slice(..)).map_err(|e| {
                error!("{:?}", e);
                ValidateResponseDeserializeError
            })?
        }
        _ => {
            // x-www-form-urlencoded
            MicropubForm::from_form_bytes(&body.slice(..)).map_err(|e| {
                error!("{:?}", e);
                ValidateResponseDeserializeError
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

    db.run_txn(|conn| {
        diesel::insert_into(posts::table)
            .values(&new_post)
            .execute(conn)?;
        let post_id = get_latest_post_id(conn)?;
        let new_categories: Vec<NewCategory> = form
            .category
            .iter()
            .map(|c| NewCategory {
                post_id,
                category: c.as_str(),
            })
            .collect();

        for c in new_categories {
            diesel::insert_into(categories::table)
                .values(c)
                .execute(conn)?;
        }

        let original_blob = NewOriginalBlob {
            post_id,
            post_blob: &body,
        };
        diesel::insert_into(original_blobs::table)
            .values(original_blob)
            .execute(conn)?;

        if let Some(ref photos) = form.photos {
            let new_photos: Vec<NewPhoto> = photos
                .iter()
                .map(|p| NewPhoto {
                    post_id,
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
    http_client: reqwest::Client,
    site_config: Arc<crate::MicropubSiteConfig>,
    auth: &str,
) -> Result<TokenValidateResponse, StatusCode> {

    let r = http_client
        .get(&site_config.micropub.auth_token_endpoint)
        .header("accept", "application/json")
        .header("Authorization", auth)
        .send()
        .await;

    let validate_response: TokenValidateResponse = r
        .map_err(|e| {
            error!("{:?}", e);
            HTTPClientError
        })?
        .json()
        .await
        .map_err(|e| {
            error!("{:?}", e);
            ValidateResponseDeserializeError
        })?;

    info!(
        "validate_resp: {:?}, scopes: {:?}",
        validate_response,
        validate_response.scopes()
    );

    Ok(validate_response)
}

#[cfg(test)]
mod test {
    use super::{Photo, MicropubForm};
    use crate::models::Post;

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

    #[test]
    fn micropub_encode_post_to_properties() {
        let post = Post {
            id: 3,
            slug: "slug".into(),
            entry_type: "entry".into(),
            name: Some("title".into()),
            content: Some("test content".into()),
            client_id: None,
            created_at: "2020-04-04 15:30:00".into(),
            updated_at: "2022-04-08 19:30:00".into(),
            content_type: None,
            bookmark_of: None,
        };
        let form = MicropubForm::from_post(&post, &vec![], &vec![]);
        let json_properties = b"{\"type\":[\"h-entry\"],\"properties\":{\"mp-slug\":[\"slug\"],\"name\":[\"title\"],\"content\":[\"test content\"],\"published\":[\"2020-04-04 15:30:00\"],\"updated\":[\"2022-04-08 19:30:00\"]}}";

        assert_eq!(
            MicropubForm::from_json_bytes(form.to_properties_json().unwrap().as_bytes()).unwrap(),
            MicropubForm::from_json_bytes(json_properties).unwrap()
        );
    }

    #[test]
    fn micropub_encode_post_to_properties_with_html_content() {
        let post = Post {
            id: 3,
            slug: "slug".into(),
            entry_type: "entry".into(),
            name: Some("title".into()),
            content: Some("<b>test content</b>".into()),
            client_id: None,
            created_at: "2020-04-04 15:30:00".into(),
            updated_at: "2022-04-08 19:30:00".into(),
            content_type: Some("html".into()),
            bookmark_of: None,
        };
        let form = MicropubForm::from_post(&post, &vec![], &vec![]);
        eprintln!("form: {:?}", form);
        let json_properties = b"{\"type\":[\"h-entry\"],\"properties\":{\"mp-slug\":[\"slug\"],\"name\":[\"title\"],\"content\":[{\"html\":\"<b>test content</b>\"}],\"published\":[\"2020-04-04 15:30:00\"],\"updated\":[\"2022-04-08 19:30:00\"]}}";

        assert_eq!(
            MicropubForm::from_json_bytes(form.to_properties_json().unwrap().as_bytes()).unwrap(),
            MicropubForm::from_json_bytes(json_properties).unwrap()
        );
    }

    #[test]
    fn micropub_encode_post_to_properties_without_name() {
        let post = Post {
            id: 3,
            slug: "slug".into(),
            entry_type: "entry".into(),
            name: None,
            content: Some("test content".into()),
            client_id: None,
            created_at: "2020-04-04 15:30:00".into(),
            updated_at: "2022-04-08 19:30:00".into(),
            content_type: None,
            bookmark_of: None,
        };
        let form = MicropubForm::from_post(&post, &vec![], &vec![]);
        let json_properties = b"{\"type\":[\"h-entry\"],\"properties\":{\"mp-slug\":[\"slug\"],\"content\":[\"test content\"],\"published\":[\"2020-04-04 15:30:00\"],\"updated\":[\"2022-04-08 19:30:00\"]}}";

        assert_eq!(
            MicropubForm::from_json_bytes(form.to_properties_json().unwrap().as_bytes()).unwrap(),
            MicropubForm::from_json_bytes(json_properties).unwrap()
        );
    }

    #[test]
    fn micropub_encode_post_to_properties_with_categories() {
        let post = Post {
            id: 3,
            slug: "slug".into(),
            entry_type: "entry".into(),
            name: None,
            content: Some("test content".into()),
            client_id: None,
            created_at: "2020-04-04 15:30:00".into(),
            updated_at: "2022-04-08 19:30:00".into(),
            content_type: None,
            bookmark_of: None,
        };
        let categories: Vec<String> = vec!["tag1".into(), "tag2".into()];
        let form = MicropubForm::from_post(&post, &categories, &vec![]);
        let json_properties = b"{\"type\":[\"h-entry\"],\"properties\":{\"mp-slug\":[\"slug\"],\"content\":[\"test content\"],\"published\":[\"2020-04-04 15:30:00\"],\"updated\":[\"2022-04-08 19:30:00\"],\"category\":[\"tag1\",\"tag2\"]}}";

        assert_eq!(
            MicropubForm::from_json_bytes(form.to_properties_json().unwrap().as_bytes()).unwrap(),
            MicropubForm::from_json_bytes(json_properties).unwrap()
        );
    }

    #[test]
    fn micropub_encode_post_to_properties_with_photos() {
        let post = Post {
            id: 3,
            slug: "slug".into(),
            entry_type: "entry".into(),
            name: None,
            content: Some("test content".into()),
            client_id: None,
            created_at: "2020-04-04 15:30:00".into(),
            updated_at: "2022-04-08 19:30:00".into(),
            content_type: None,
            bookmark_of: None,
        };
        let photos: Vec<(String, Option<String>)> = vec![("url1".into(), None), ("url2".into(), Some("alt text".into()))];
        let form = MicropubForm::from_post(&post, &vec![], &photos);
        let json_properties = b"{\"type\":[\"h-entry\"],\"properties\":{\"mp-slug\":[\"slug\"],\"content\":[\"test content\"],\"published\":[\"2020-04-04 15:30:00\"],\"updated\":[\"2022-04-08 19:30:00\"],\"photo\":[{\"value\":\"url1\"},{\"value\":\"url2\",\"alt\":\"alt text\"}]}}";

        assert_eq!(
            MicropubForm::from_json_bytes(form.to_properties_json().unwrap().as_bytes()).unwrap(),
            MicropubForm::from_json_bytes(json_properties).unwrap()
        );
    }
}
