use axum::{
    http::StatusCode,
    response::Json,
    routing::get,
    Router,
};
use micropub_rs::auth::TokenValidateResponse;

async fn validate_token() -> Result<Json<TokenValidateResponse>, StatusCode> {
    // Return a fake but valid token validation response
    let response = TokenValidateResponse {
        me: "https://example.com".to_string(),
        client_id: "https://test-client.example.com".to_string(),
        issued_at: 1640995200, // 2022-01-01 00:00:00 UTC
        scope: "create update delete".to_string(),
        nonce: 12345,
    };
    
    Ok(Json(response))
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let app = Router::new()
        .route("/token", get(validate_token));

    let listener = tokio::net::TcpListener::bind("127.0.0.1:3001").await?;
    println!("Fake auth server listening on {}", listener.local_addr()?);
    
    axum::serve(listener, app).await?;
    
    Ok(())
}