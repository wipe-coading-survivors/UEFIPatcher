use axum::Json;
use axum::extract::{Path, Query, State};
use axum_extra::extract::CookieJar;
use serde::Deserialize;
use serde_json::{Value, json};

use super::AppState;
use crate::error::AppError;
use crate::session::{extract_session_id, make_image_cookie};

#[derive(Deserialize)]
pub struct OpenBody {
    pub path: String,
    pub mode: String,
    pub name: Option<String>,
}

pub async fn open(
    State(state): State<AppState>,
    jar: CookieJar,
    Json(body): Json<OpenBody>,
) -> Result<(CookieJar, Json<Value>), AppError> {
    let sid = extract_session_id(&jar).ok_or(AppError::Auth)?;
    let mode = match body.mode.as_str() {
        "read" => 0,
        "write" => 1,
        _ => return Err(AppError::BadRequest("mode must be read|write".into())),
    };
    let name = body.name.unwrap_or_default();
    let mut c = state.client.lock().await;
    let r = c
        .image_open(&state.sessions, &sid, &body.path, &name, mode)
        .await
        .map_err(AppError::from)?;
    let jar = jar.add(make_image_cookie(&r.image_id));
    Ok((
        jar,
        Json(json!({ "image_id": r.image_id, "root_guid": r.root_guid, "name": r.name })),
    ))
}

#[derive(Deserialize)]
pub struct ItemsQuery {
    pub filter: Option<String>,
}

pub async fn dump(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(id): Path<String>,
    Query(_q): Query<ItemsQuery>,
) -> Result<Json<Value>, AppError> {
    let sid = extract_session_id(&jar).ok_or(AppError::Auth)?;
    let mut c = state.client.lock().await;
    let nodes = c
        .image_nodes_list(&state.sessions, &sid, &id, "")
        .await
        .map_err(AppError::from)?;
    Ok(Json(json!({ "nodes": nodes })))
}

pub async fn items(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(id): Path<String>,
    Query(q): Query<ItemsQuery>,
) -> Result<Json<Value>, AppError> {
    let sid = extract_session_id(&jar).ok_or(AppError::Auth)?;
    let filter = q.filter.as_deref().unwrap_or("");
    let mut c = state.client.lock().await;
    let items = c
        .image_nodes_list(&state.sessions, &sid, &id, filter)
        .await
        .map_err(AppError::from)?;
    Ok(Json(json!({ "items": items })))
}

#[derive(Deserialize)]
pub struct SaveBody {
    pub output_path: String,
}

pub async fn save(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(id): Path<String>,
    Json(body): Json<SaveBody>,
) -> Result<Json<Value>, AppError> {
    let sid = extract_session_id(&jar).ok_or(AppError::Auth)?;
    let mut c = state.client.lock().await;
    c.image_save(&state.sessions, &sid, &id, &body.output_path)
        .await
        .map_err(AppError::from)?;
    Ok(Json(json!({ "ok": true })))
}
