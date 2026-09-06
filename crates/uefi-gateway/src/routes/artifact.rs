use axum::Json;
use axum::extract::{Multipart, Path, Query, State};
use axum_extra::extract::CookieJar;
use serde::Deserialize;
use serde_json::{Value, json};
use std::path::PathBuf;
use uuid::Uuid;

use super::AppState;
use crate::error::AppError;
use crate::session::extract_session_id;

#[derive(Deserialize)]
pub struct ExtractBody {
    pub target: String,
    pub body_only: bool,
}
pub async fn extract(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(id): Path<String>,
    Json(body): Json<ExtractBody>,
) -> Result<Json<Value>, AppError> {
    let sid = extract_session_id(&jar).ok_or(AppError::Auth)?;
    let mut c = state.client.lock().await;
    let artifact_id = c
        .image_node_extract(&state.sessions, &sid, &id, &body.target, body.body_only)
        .await
        .map_err(AppError::from)?;
    Ok(Json(json!({ "artifact_id": artifact_id })))
}

#[derive(Deserialize)]
pub struct ExportBody {
    pub output_path: String,
}
pub async fn export(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(id): Path<String>,
    Json(body): Json<ExportBody>,
) -> Result<Json<Value>, AppError> {
    let sid = extract_session_id(&jar).ok_or(AppError::Auth)?;
    let mut c = state.client.lock().await;
    c.artifact_export(&state.sessions, &sid, &id, &body.output_path)
        .await
        .map_err(AppError::from)?;
    Ok(Json(json!({ "ok": true })))
}

pub async fn import(
    State(state): State<AppState>,
    jar: CookieJar,
    mut multipart: Multipart,
) -> Result<Json<Value>, AppError> {
    let sid = extract_session_id(&jar).ok_or(AppError::Auth)?;
    let mut file_path: Option<String> = None;
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| AppError::BadRequest(e.to_string()))?
    {
        if field.name() == Some("file") {
            let data = field
                .bytes()
                .await
                .map_err(|e| AppError::BadRequest(e.to_string()))?;
            let path = PathBuf::from(format!("/tmp/uefipatcher-import-{}.bin", Uuid::new_v4()));
            tokio::fs::write(&path, &data)
                .await
                .map_err(|e| AppError::Internal(e.to_string()))?;
            file_path = Some(path.display().to_string());
        }
    }
    let fp = file_path.ok_or_else(|| AppError::BadRequest("no file field in multipart".into()))?;
    let mut c = state.client.lock().await;
    let artifact_id = c
        .artifact_import(&state.sessions, &sid, &fp)
        .await
        .map_err(AppError::from)?;
    Ok(Json(json!({ "artifact_id": artifact_id })))
}

#[derive(Deserialize)]
pub struct ListQuery {
    pub session_id: Option<String>,
}
pub async fn list(
    State(state): State<AppState>,
    jar: CookieJar,
    Query(q): Query<ListQuery>,
) -> Result<Json<Value>, AppError> {
    let sid = q
        .session_id
        .or_else(|| extract_session_id(&jar))
        .ok_or(AppError::Auth)?;
    let mut c = state.client.lock().await;
    let artifacts = c
        .artifacts_list(&state.sessions, &sid)
        .await
        .map_err(AppError::from)?;
    Ok(Json(json!({ "artifacts": artifacts })))
}
