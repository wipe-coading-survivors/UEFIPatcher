pub mod artifact;
pub mod edit;
pub mod image;
pub mod session;
pub mod setup;
pub mod upload;
pub mod ws;

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
        .route("/api/v1/image/open", axum::routing::post(image::open))
        .route("/api/v1/image/upload", axum::routing::post(upload::upload))
        .route("/api/v1/image/:id/dump", axum::routing::get(image::dump))
        .route("/api/v1/image/:id/dump/ws", axum::routing::get(ws::dump_ws))
        .route("/api/v1/image/:id/items", axum::routing::get(image::items))
        .route("/api/v1/image/:id/find", axum::routing::get(image::find))
        .route("/api/v1/image/:id/save", axum::routing::post(image::save))
        .route(
            "/api/v1/image/:id/download",
            axum::routing::get(upload::download),
        )
        .route(
            "/api/v1/image/:id/insert",
            axum::routing::post(edit::insert),
        )
        .route(
            "/api/v1/image/:id/remove",
            axum::routing::post(edit::remove),
        )
        .route(
            "/api/v1/image/:id/replace",
            axum::routing::post(edit::replace),
        )
        .route(
            "/api/v1/image/:id/rebuild",
            axum::routing::post(edit::rebuild),
        )
        .route(
            "/api/v1/image/:id/set-visibility",
            axum::routing::post(setup::set_visibility),
        )
        .route(
            "/api/v1/image/:id/setup-items",
            axum::routing::get(setup::list_items),
        )
        .route(
            "/api/v1/image/:id/extract",
            axum::routing::post(artifact::extract),
        )
        .route(
            "/api/v1/artifact/:id/export",
            axum::routing::post(artifact::export),
        )
        .route(
            "/api/v1/artifact/import",
            axum::routing::post(artifact::import),
        )
        .route("/api/v1/artifacts", axum::routing::get(artifact::list))
        .with_state(state)
}
