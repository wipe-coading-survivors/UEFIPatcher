use axum::Json;
use axum::extract::{Path, State};
use axum_extra::extract::CookieJar;
use serde::Deserialize;
use serde_json::{Value, json};

use super::AppState;
use crate::error::AppError;
use crate::session::extract_session_id;

#[derive(Deserialize)]
pub struct VisibilityBody {
    pub item_id: String,
    pub visible: bool,
}
pub async fn set_visibility(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(id): Path<String>,
    Json(body): Json<VisibilityBody>,
) -> Result<Json<Value>, AppError> {
    let sid = extract_session_id(&jar).ok_or(AppError::Auth)?;
    let mut c = state.client.lock().await;
    c.hii_set_form_visibility(&state.sessions, &sid, &id, &body.item_id, body.visible)
        .await
        .map_err(AppError::from)?;
    Ok(Json(json!({ "ok": true })))
}

pub async fn list_items(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(id): Path<String>,
) -> Result<Json<Value>, AppError> {
    let sid = extract_session_id(&jar).ok_or(AppError::Auth)?;
    let mut c = state.client.lock().await;
    let items = c
        .image_nodes_list(&state.sessions, &sid, &id, "")
        .await
        .map_err(AppError::from)?;
    let setup: Vec<_> = items.into_iter().filter(|i| i.r#type == 67).collect();
    Ok(Json(json!({ "items": setup })))
}
