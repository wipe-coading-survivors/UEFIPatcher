mod config;
mod error;

use axum::Json;
use axum::routing::get;
use serde_json::{Value, json};
use tower_http::cors::CorsLayer;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cfg = config::load_config()?;
    let app = axum::Router::new()
        .route("/api/v1/health", get(health))
        .layer(CorsLayer::very_permissive());
    let listener = tokio::net::TcpListener::bind(cfg.listen).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

async fn health() -> Json<Value> {
    Json(json!({ "ok": true }))
}
