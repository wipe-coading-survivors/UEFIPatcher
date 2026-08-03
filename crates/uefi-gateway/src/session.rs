use axum_extra::extract::CookieJar;
use axum_extra::extract::cookie::{Cookie, SameSite};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

pub struct SessionMap {
    map: Arc<Mutex<HashMap<String, String>>>,
}

impl SessionMap {
    pub fn new() -> Self {
        Self {
            map: Arc::new(Mutex::new(HashMap::new())),
        }
    }
    pub async fn insert(&self, session_id: String, token: String) {
        self.map.lock().await.insert(session_id, token);
    }
    pub async fn get_token(&self, session_id: &str) -> Option<String> {
        self.map.lock().await.get(session_id).cloned()
    }
    pub async fn remove(&self, session_id: &str) {
        self.map.lock().await.remove(session_id);
    }
}

pub fn extract_session_id(jar: &CookieJar) -> Option<String> {
    jar.get("uefipatcher_session")
        .map(|c| c.value().to_string())
}

pub fn make_session_cookie(session_id: &str) -> Cookie<'static> {
    Cookie::build(("uefipatcher_session", session_id.to_string()))
        .http_only(true)
        .same_site(SameSite::Strict)
        .path("/")
        .build()
}

pub fn make_image_cookie(image_id: &str) -> Cookie<'static> {
    Cookie::build(("uefipatcher_image", image_id.to_string()))
        .http_only(true)
        .same_site(SameSite::Strict)
        .path("/")
        .build()
}
