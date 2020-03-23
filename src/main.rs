use std::collections::HashMap;

use reqwest;
use serde::Deserialize;
use warp::Filter;

// TODO make these configurable via command line, environment, or config file?
const MAX_CONTENT_LENGTH: u64 = 1024 * 1024 * 50; // 50 megabytes
const AUTH_TOKEN_ENDPOINT: &str = "https://tokens.indieauth.com/token";

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
impl warp::reject::Reject for HTTPClientError {}

#[derive(Debug)]
struct ValidateResponseDeserializeError;
impl warp::reject::Reject for ValidateResponseDeserializeError {}

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
    ) -> Result<impl warp::Reply, warp::reject::Rejection> {
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
                warp::reject::custom(HTTPClientError)
            })?
            .json()
            .await
            .map_err(|e| {
                println!("{:?}", e);
                warp::reject::custom(ValidateResponseDeserializeError)
            })?;

        println!("validate_resp: {:?}, scopes: {:?}", validate_response, validate_response.scopes());
        Ok(warp::reply::with_status(
            warp::reply::reply(),
            warp::http::StatusCode::OK,
        ))
            // Err(_) => Ok(warp::reply::with_status(
            //     warp::reply::reply(),
            //     warp::http::StatusCode::INTERNAL_SERVER_ERROR,
            // )),
    }
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
            async move {
                h.verify_auth(a, f).await
            }
        });

    warp::serve(micropub).run(([127, 0, 0, 1], 3030)).await;
}
