mod client;
mod config;
mod error;
mod routes;
mod session;

use std::sync::Arc;

use tower_http::cors::CorsLayer;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cfg = config::load_config()?;
    let engine = client::EngineClient::connect(&cfg.sock_path).await?;
    let state = routes::AppState {
        client: Arc::new(tokio::sync::Mutex::new(engine)),
        sessions: Arc::new(session::SessionMap::new()),
    };
    let app = routes::router(state).layer(CorsLayer::very_permissive());
    let listener = tokio::net::TcpListener::bind(cfg.listen).await?;
    axum::serve(listener, app).await?;
    Ok(())
}
