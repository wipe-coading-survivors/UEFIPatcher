use axum::Json;
use axum::body::Body;
use axum::extract::{Multipart, Path, State};
use axum::http::header;
use axum::response::Response;
use axum_extra::extract::CookieJar;
use serde_json::{Value, json};
use std::path::PathBuf;
use uuid::Uuid;

use super::AppState;
use crate::error::AppError;
use crate::session::extract_session_id;

pub async fn upload(
    State(_state): State<AppState>,
    _jar: CookieJar,
    mut multipart: Multipart,
) -> Result<Json<Value>, AppError> {
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
            let id = Uuid::new_v4();
            let path = PathBuf::from(format!("/tmp/uefipatcher-upload-{id}.bin"));
            tokio::fs::write(&path, &data)
                .await
                .map_err(|e| AppError::Internal(e.to_string()))?;
            return Ok(Json(json!({ "path": path.display().to_string() })));
        }
    }
    Err(AppError::BadRequest("no file field in multipart".into()))
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
