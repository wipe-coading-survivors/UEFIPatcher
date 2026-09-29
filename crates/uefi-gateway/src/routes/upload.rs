use axum::Json;
use axum::body::Body;
use axum::extract::{Multipart, Path, State};
use axum::http::{HeaderValue, header};
use axum::response::Response;
use axum_extra::extract::CookieJar;
use serde_json::{Value, json};
use std::path::PathBuf;
use uuid::Uuid;

use super::AppState;
use crate::error::AppError;
use crate::session::extract_session_id;

pub async fn upload(
    State(state): State<AppState>,
    jar: CookieJar,
    mut multipart: Multipart,
) -> Result<Json<Value>, AppError> {
    let sid = extract_session_id(&jar).ok_or(AppError::Auth)?;
    let mut data: Option<bytes::Bytes> = None;
    let mut name = String::new();
    let mut mode = 0i32;
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| AppError::BadRequest(e.to_string()))?
    {
        match field.name() {
            Some("file") => {
                name = field.file_name().unwrap_or_default().to_string();
                data = Some(
                    field
                        .bytes()
                        .await
                        .map_err(|e| AppError::BadRequest(e.to_string()))?,
                );
            }
            Some("mode") => {
                let m = field
                    .text()
                    .await
                    .map_err(|e| AppError::BadRequest(e.to_string()))?;
                mode = if m == "write" { 1 } else { 0 };
            }
            _ => {}
        }
    }
    let data = data.ok_or_else(|| AppError::BadRequest("no file field in multipart".into()))?;
    let mut c = state.client.lock().await;
    let resp = c
        .image_upload(&state.sessions, &sid, data.to_vec(), &name, mode)
        .await?;
    Ok(Json(json!({
        "image_id": resp.image_id,
        "root_guid": resp.root_guid,
        "name": resp.name,
    })))
}

pub async fn artifact_upload(
    State(state): State<AppState>,
    jar: CookieJar,
    mut multipart: Multipart,
) -> Result<Json<Value>, AppError> {
    let sid = extract_session_id(&jar).ok_or(AppError::Auth)?;
    let mut data: Option<bytes::Bytes> = None;
    let mut name = String::new();
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| AppError::BadRequest(e.to_string()))?
    {
        if field.name() == Some("file") {
            name = format!("upload-{}", Uuid::new_v4());
            data = Some(
                field
                    .bytes()
                    .await
                    .map_err(|e| AppError::BadRequest(e.to_string()))?,
            );
        }
    }
    let data = data.ok_or_else(|| AppError::BadRequest("no file field in multipart".into()))?;
    let path = PathBuf::from(format!("/tmp/uefipatcher-artifact-{name}.bin"));
    tokio::fs::write(&path, &data)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;
    let mut c = state.client.lock().await;
    let import = c
        .artifact_import(&state.sessions, &sid, &path.display().to_string())
        .await;
    let _ = tokio::fs::remove_file(&path).await;
    let artifact_id = import?;
    Ok(Json(json!({ "artifact_id": artifact_id })))
}

pub async fn download(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(id): Path<String>,
) -> Result<Response<Body>, AppError> {
    let sid = extract_session_id(&jar).ok_or(AppError::Auth)?;
    let out_path = format!("/tmp/uefipatcher-download-{}.bin", Uuid::new_v4());
    let mut c = state.client.lock().await;
    c.image_save(&state.sessions, &sid, &id, &out_path)
        .await
        .map_err(AppError::from)?;
    let data = tokio::fs::read(&out_path)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;
    let _ = tokio::fs::remove_file(&out_path).await;
    Ok(Response::builder()
        .header(header::CONTENT_TYPE, "application/octet-stream")
        .header(
            header::CONTENT_DISPOSITION,
            "attachment; filename=\"patched.bin\"",
        )
        .body(Body::from(data))
        .unwrap())
}

pub async fn artifact_download(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(id): Path<String>,
) -> Result<Response<Body>, AppError> {
    let sid = extract_session_id(&jar).ok_or(AppError::Auth)?;
    let content_disposition =
        HeaderValue::from_str(&format!("attachment; filename=\"artifact-{id}.bin\""))
            .map_err(|e| AppError::Internal(format!("invalid artifact id in filename: {e}")))?;
    let out_path = format!("/tmp/uefipatcher-artifact-dl-{}.bin", Uuid::new_v4());
    let mut c = state.client.lock().await;
    c.artifact_export(&state.sessions, &sid, &id, &out_path)
        .await
        .map_err(AppError::from)?;
    let data = tokio::fs::read(&out_path)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;
    let _ = tokio::fs::remove_file(&out_path).await;
    Ok(Response::builder()
        .header(header::CONTENT_TYPE, "application/octet-stream")
        .header(header::CONTENT_DISPOSITION, content_disposition)
        .body(Body::from(data))
        .unwrap())
}
