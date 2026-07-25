use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct State {
    pub session_id: Option<String>,
    pub token: Option<String>,
    pub active_image_id: Option<String>,
    pub sock_path: Option<String>,
}

pub fn state_path() -> PathBuf {
    PathBuf::from(".uefipatcher")
}
