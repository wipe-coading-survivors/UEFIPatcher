use axum::Json;
use axum::extract::State;
use axum_extra::extract::CookieJar;
use serde::Deserialize;
use serde_json::{Value, json};

use super::AppState;
use crate::error::AppError;
use crate::session::{extract_session_id, make_session_cookie};

#[derive(Deserialize, Default)]
pub struct CreateBody {
    #[serde(default)]
    pub name: Option<String>,
}

pub async fn create(
    State(state): State<AppState>,
    Json(body): Json<CreateBody>,
) -> Result<(CookieJar, Json<Value>), AppError> {
    let mut c = state.client.lock().await;
    let name = body.name.filter(|n| !n.is_empty()).unwrap_or_else(|| {
        std::env::current_dir()
            .map(|p| p.display().to_string())
            .unwrap_or_default()
    });
    let (sid, tok) = c.session_create(&name).await.map_err(AppError::from)?;
    state.sessions.insert(sid.clone(), tok).await;
    let jar = CookieJar::new().add(make_session_cookie(&sid));
    Ok((jar, Json(json!({ "session_id": sid }))))
}

pub async fn destroy(
    State(state): State<AppState>,
    jar: CookieJar,
) -> Result<Json<Value>, AppError> {
    let sid = extract_session_id(&jar).ok_or(AppError::Auth)?;
    let mut c = state.client.lock().await;
    c.session_destroy(&sid).await.map_err(AppError::from)?;
    state.sessions.remove(&sid).await;
    Ok(Json(json!({ "ok": true })))
}

pub async fn list(State(state): State<AppState>, jar: CookieJar) -> Result<Json<Value>, AppError> {
    let _sid = extract_session_id(&jar).ok_or(AppError::Auth)?;
    let mut c = state.client.lock().await;
    let rows = c.sessions_list().await.map_err(AppError::from)?;
    Ok(Json(json!({ "sessions": rows })))
}
