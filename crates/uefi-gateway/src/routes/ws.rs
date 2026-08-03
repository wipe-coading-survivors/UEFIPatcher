use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path, Query, State};
use axum_extra::extract::CookieJar;
use serde::Deserialize;

use super::AppState;
use crate::error::AppError;
use crate::session::extract_session_id;

#[derive(Deserialize)]
pub struct WsQuery {
    pub format: Option<String>,
}

pub async fn dump_ws(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
    jar: CookieJar,
    Path(id): Path<String>,
    Query(q): Query<WsQuery>,
) -> Result<axum::response::Response, AppError> {
    let sid = extract_session_id(&jar).ok_or(AppError::Auth)?;
    let fmt = if q.format.as_deref() == Some("tsv") {
        1
    } else {
        0
    };
    Ok(ws.on_upgrade(move |socket| handle_ws(socket, state, sid, id, fmt)))
}

async fn handle_ws(
    mut socket: WebSocket,
    state: AppState,
    sid: String,
    image_id: String,
    fmt: i32,
) {
    let mut c = state.client.lock().await;
    match c.dump_tree(&state.sessions, &sid, &image_id, fmt).await {
        Ok(text) => {
            let _ = socket.send(Message::Text(text)).await;
        }
        Err(e) => {
            let _ = socket.send(Message::Text(format!("error: {e}"))).await;
        }
    }
    let _ = socket.close().await;
}
