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
    let mut c = state.client.lock().await;
    let r = c
        .open_image(&state.sessions, &sid, &body.path, mode)
        .await
        .map_err(AppError::from)?;
    let jar = jar.add(make_image_cookie(&r.image_id));
    Ok((
        jar,
        Json(json!({ "image_id": r.image_id, "root_guid": r.root_guid })),
    ))
}

#[derive(Deserialize)]
pub struct DumpQuery {
    pub format: Option<String>,
}

pub async fn dump(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(id): Path<String>,
    Query(q): Query<DumpQuery>,
) -> Result<Json<Value>, AppError> {
    let sid = extract_session_id(&jar).ok_or(AppError::Auth)?;
    let fmt = if q.format.as_deref() == Some("tsv") {
        1
    } else {
        0
    };
    let mut c = state.client.lock().await;
    let text = c
        .dump_tree(&state.sessions, &sid, &id, fmt)
        .await
        .map_err(AppError::from)?;
    Ok(Json(json!({ "text": text })))
}

#[derive(Deserialize)]
pub struct ItemsQuery {
    pub filter: Option<String>,
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
        .list_items(&state.sessions, &sid, &id, filter)
        .await
        .map_err(AppError::from)?;
    Ok(Json(json!({ "items": items })))
}

#[derive(Deserialize)]
pub struct FindQuery {
    pub target: String,
}

pub async fn find(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(id): Path<String>,
    Query(q): Query<FindQuery>,
) -> Result<Json<Value>, AppError> {
    let sid = extract_session_id(&jar).ok_or(AppError::Auth)?;
    let mut c = state.client.lock().await;
    let item_id = c
        .find_item(&state.sessions, &sid, &id, &q.target)
        .await
        .map_err(AppError::from)?;
    Ok(Json(json!({ "item_id": item_id })))
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
    c.save_image(&state.sessions, &sid, &id, &body.output_path)
        .await
        .map_err(AppError::from)?;
    Ok(Json(json!({ "ok": true })))
}
