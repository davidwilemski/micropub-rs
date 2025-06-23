use std::sync::Arc;
use axum::{
    extract::State,
    http::StatusCode,
    response::Json,
    routing::get,
    Router,
};
use clap::Parser;
use micropub_rs::auth::TokenValidateResponse;

#[derive(Parser, Clone)]
#[command(name = "fake_auth_server")]
#[command(about = "A fake IndieAuth token validation server for testing micropub-rs")]
#[command(version)]
struct Args {
    /// Port to listen on
    #[arg(short, long, default_value = "3001")]
    port: u16,

    /// The 'me' field in auth response
    #[arg(long, default_value = "https://example.com")]
    me: String,

    /// The 'client_id' field in auth response  
    #[arg(long, default_value = "https://test-client.example.com")]
    client_id: String,

    /// The 'scope' field in auth response
    #[arg(long, default_value = "create update delete")]
    scope: String,
}

async fn validate_token(State(config): State<Arc<Args>>) -> Result<Json<TokenValidateResponse>, StatusCode> {
    // Return a fake but valid token validation response
    let response = TokenValidateResponse {
        me: config.me.clone(),
        client_id: config.client_id.clone(),
        issued_at: 1640995200, // 2022-01-01 00:00:00 UTC
        scope: config.scope.clone(),
        nonce: 12345,
    };
    
    Ok(Json(response))
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = Arc::new(Args::parse());
    
    let app = Router::new()
        .route("/token", get(validate_token))
        .with_state(config.clone());

    let bind_addr = format!("127.0.0.1:{}", config.port);
    let listener = tokio::net::TcpListener::bind(&bind_addr).await?;
    
    println!("Fake auth server listening on {}", listener.local_addr()?);
    println!("Configuration:");
    println!("  me: {}", config.me);
    println!("  client_id: {}", config.client_id);
    println!("  scope: {}", config.scope);
    println!();
    println!("Responding to GET /token with valid TokenValidateResponse");
    
    axum::serve(listener, app).await?;
    
    Ok(())
}