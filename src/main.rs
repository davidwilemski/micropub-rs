use std::collections::HashMap;

use reqwest;
use serde::Deserialize;
use warp::http::StatusCode;
use warp::{reject, Filter, Rejection};

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

#[derive(Debug)]
struct HTTPClientError;
impl reject::Reject for HTTPClientError {}

#[derive(Debug)]
struct ValidateResponseDeserializeError;
impl reject::Reject for ValidateResponseDeserializeError {}

#[derive(Debug)]
struct NotAuthorized;
impl reject::Reject for NotAuthorized {}

struct MicropubHandler {
    http_client: reqwest::Client,
}

impl MicropubHandler {
    fn new() -> Self {
        MicropubHandler {
            http_client: reqwest::Client::new(),
        }
    }

    async fn verify_auth(
        &self,
        auth: String,
        form: HashMap<String, String>,
    ) -> Result<impl warp::Reply, Rejection> {
        println!("auth: {:?}, form: {:?}", auth, form);

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

        Ok(warp::reply::with_status(
            warp::reply::reply(),
            StatusCode::OK,
        ))
    }
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
async fn main() {
    use std::sync::Arc;
    let handler = Arc::new(MicropubHandler::new());

    let micropub = warp::path!("micropub")
        .and(warp::post())
        .and(warp::header::<String>("Authorization"))
        .and(warp::body::content_length_limit(MAX_CONTENT_LENGTH))
        .and(warp::body::form::<HashMap<String, String>>())
        // .map(|auth, form| "blah");
        // .and_then(move |auth, form| async {handler.verify_auth(auth, form)});
        .and_then(move |a, f| {
            let h = handler.clone();
            async move { h.verify_auth(a, f).await }
        })
        .recover(handle_rejection);

    warp::serve(micropub).run(([127, 0, 0, 1], 3030)).await;
}
