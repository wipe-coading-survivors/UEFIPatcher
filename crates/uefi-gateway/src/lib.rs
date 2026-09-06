pub mod client;
pub mod config;
pub mod error;
pub mod routes;
pub mod session;

use std::path::Path;
use std::sync::Arc;

use tower_http::cors::CorsLayer;

pub async fn serve(listener: tokio::net::TcpListener, sock_path: &Path) -> anyhow::Result<()> {
    let engine = client::EngineClient::connect(sock_path).await?;
    let state = routes::AppState {
        client: Arc::new(tokio::sync::Mutex::new(engine)),
        sessions: Arc::new(session::SessionMap::new()),
    };
    let app = routes::router(state).layer(CorsLayer::very_permissive());
    axum::serve(listener, app).await?;
    Ok(())
}

pub async fn main_inner() -> anyhow::Result<()> {
    let cfg = config::load_config()?;
    let listener = tokio::net::TcpListener::bind(cfg.listen).await?;
    serve(listener, &cfg.sock_path).await
}
