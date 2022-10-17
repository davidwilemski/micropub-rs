use std::env;
use std::io::Read;
use std::sync::Arc;

use bytes::Bytes;
use http::header::HeaderValue;

use micropub_rs::{handler::MicropubDB, handlers};

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    let dbfile = env::var("DATABASE_URL")?;
    let dbpool = Arc::new(micropub_rs::new_dbconn_pool(&dbfile)?);

    // read in body from stdin
    let mut body = String::new();
    let mut stdin = std::io::stdin();
    stdin.read_to_string(&mut body)?;

    // assume json for now, perhaps provide a flag for later

    let result = handlers::micropub::create_post(
        Arc::new(MicropubDB::new(dbpool)),
        Some(&HeaderValue::from_static("application/json")),
        Bytes::from(body),
        "micropub/import_entry",
    )
    .await;

    match result {
        Ok(slug) => println!("created post with slug: '{}'", slug),
        Err(rejection) => println!("error creating post: {:?}", rejection),
    };

    Ok(())
}
