use axum::Json;
use axum::extract::{Path, State};
use axum_extra::extract::CookieJar;
use serde::Deserialize;
use serde_json::{Value, json};

use super::AppState;
use crate::error::AppError;
use crate::session::extract_session_id;

#[derive(Deserialize)]
pub struct InsertBody {
    pub target: String,
    pub ffs_path: Option<String>,
    pub artifact_id: Option<String>,
    pub mode: String,
}
pub async fn insert(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(id): Path<String>,
    Json(body): Json<InsertBody>,
) -> Result<Json<Value>, AppError> {
    let sid = extract_session_id(&jar).ok_or(AppError::Auth)?;
    let mode = match body.mode.as_str() {
        "into" => 0,
        "before" => 1,
        "after" => 2,
        _ => {
            return Err(AppError::BadRequest(
                "mode must be into|before|after".into(),
            ));
        }
    };
    let mut c = state.client.lock().await;
    let item_id = c
        .image_node_insert(
            &state.sessions,
            &sid,
            &id,
            &body.target,
            body.ffs_path.as_deref().unwrap_or(""),
            body.artifact_id.as_deref().unwrap_or(""),
            mode,
        )
        .await
        .map_err(AppError::from)?;
    Ok(Json(json!({ "item_id": item_id })))
}

#[derive(Deserialize)]
pub struct TargetBody {
    pub target: String,
}
pub async fn remove(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(id): Path<String>,
    Json(body): Json<TargetBody>,
) -> Result<Json<Value>, AppError> {
    let sid = extract_session_id(&jar).ok_or(AppError::Auth)?;
    let mut c = state.client.lock().await;
    c.image_node_remove(&state.sessions, &sid, &id, &body.target)
        .await
        .map_err(AppError::from)?;
    Ok(Json(json!({ "ok": true })))
}

#[derive(Deserialize)]
pub struct ReplaceBody {
    pub target: String,
    pub data_path: Option<String>,
    pub artifact_id: Option<String>,
    pub body_only: bool,
}
pub async fn replace(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(id): Path<String>,
    Json(body): Json<ReplaceBody>,
) -> Result<Json<Value>, AppError> {
    let sid = extract_session_id(&jar).ok_or(AppError::Auth)?;
    let mut c = state.client.lock().await;
    let item_id = c
        .image_node_replace(
            &state.sessions,
            &sid,
            &id,
            &body.target,
            body.data_path.as_deref().unwrap_or(""),
            body.artifact_id.as_deref().unwrap_or(""),
            body.body_only,
        )
        .await
        .map_err(AppError::from)?;
    Ok(Json(json!({ "item_id": item_id })))
}

pub async fn rebuild(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(id): Path<String>,
    Json(body): Json<TargetBody>,
) -> Result<Json<Value>, AppError> {
    let sid = extract_session_id(&jar).ok_or(AppError::Auth)?;
    let mut c = state.client.lock().await;
    c.image_node_rebuild(&state.sessions, &sid, &id, &body.target)
        .await
        .map_err(AppError::from)?;
    Ok(Json(json!({ "ok": true })))
}
