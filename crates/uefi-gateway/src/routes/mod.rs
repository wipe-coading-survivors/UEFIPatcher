pub mod session;
pub mod upload;

use std::sync::Arc;

use tokio::sync::Mutex;

use crate::client::EngineClient;
use crate::session::SessionMap;

#[derive(Clone)]
pub struct AppState {
    pub client: Arc<Mutex<EngineClient>>,
    pub sessions: Arc<SessionMap>,
}

pub fn router(state: AppState) -> axum::Router {
    axum::Router::new()
        .route(
            "/api/v1/health",
            axum::routing::get(|| async { axum::Json(serde_json::json!({"ok": true})) }),
        )
        .route(
            "/api/v1/session",
            axum::routing::post(session::create).delete(session::destroy),
        )
        .route("/api/v1/sessions", axum::routing::get(session::list))
        .route(
            "/api/v1/rpc/:method",
            axum::routing::post(crate::bridge::call),
        )
        .route("/api/v1/image/upload", axum::routing::post(upload::upload))
        .route(
            "/api/v1/artifact/upload",
            axum::routing::post(upload::artifact_upload),
        )
        .route(
            "/api/v1/image/:id/download",
            axum::routing::get(upload::download),
        )
        .layer(axum::extract::DefaultBodyLimit::max(64 * 1024 * 1024))
        .with_state(state)
}
