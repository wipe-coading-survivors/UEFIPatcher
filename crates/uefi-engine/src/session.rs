use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::storage::{Db, SessionRow};
use anyhow::Result;
use uuid::Uuid;

pub struct SessionManager {
    db: Arc<Mutex<Db>>,
    pub data_dir: PathBuf,
    pub ttl: Duration,
    pub gc_interval: Duration,
    pub purge_artifacts: bool,
}

impl SessionManager {
    pub fn new(
        db: Db,
        data_dir: PathBuf,
        ttl: Duration,
        gc_interval: Duration,
        purge_artifacts: bool,
    ) -> Self {
        Self {
            db: Arc::new(Mutex::new(db)),
            data_dir,
            ttl,
            gc_interval,
            purge_artifacts,
        }
    }

    pub fn create_session(&self, name: &str) -> Result<(String, String)> {
        let id = Uuid::new_v4().to_string();
        let token = Uuid::new_v4().to_string();
        self.db.lock().unwrap().insert_session(&id, &token, name)?;
        let sess_dir = self.data_dir.join("sessions").join(&id);
        fs::create_dir_all(&sess_dir)?;
        Ok((id, token))
    }

    pub fn destroy_session(&self, id: &str, purge_files: bool) -> Result<()> {
        self.db.lock().unwrap().delete_session_metadata(id)?;
        if purge_files {
            let sess_dir = self.data_dir.join("sessions").join(id);
            let _ = fs::remove_dir_all(&sess_dir);
        }
        Ok(())
    }

    pub fn list_sessions(&self) -> Result<Vec<SessionRow>> {
        self.db.lock().unwrap().list_sessions()
    }

    pub fn touch(&self, id: &str) -> Result<()> {
        self.db.lock().unwrap().touch_session(id)
    }

    pub fn validate_token(&self, session_id: &str, token: &str) -> bool {
        match self.db.lock().unwrap().get_session(session_id) {
            Ok(Some(row)) => row.token == token,
            _ => false,
        }
    }

    pub fn spawn_gc(self: Arc<Self>) -> tokio::task::JoinHandle<()> {
        let interval = self.gc_interval;
        let ttl_secs = self.ttl.as_secs() as i64;
        let purge = self.purge_artifacts;
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(interval);
            loop {
                ticker.tick().await;
                let expired = self
                    .db
                    .lock()
                    .unwrap()
                    .list_expired(ttl_secs)
                    .unwrap_or_default();
                for id in expired {
                    let _ = self.destroy_session(&id, purge);
                    if purge {
                        tracing::info!("GC purged session {id} (files+metadata)");
                    } else {
                        tracing::info!("GC removed session {id} metadata (files preserved)");
                    }
                }
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn sm() -> (TempDir, SessionManager) {
        let td = TempDir::new().unwrap();
        let db = crate::storage::open_db(&td.path().join("s.db")).unwrap();
        let sm = SessionManager {
            db: Arc::new(Mutex::new(db)),
            data_dir: td.path().to_path_buf(),
            ttl: Duration::from_secs(1),
            gc_interval: Duration::from_millis(100),
            purge_artifacts: false,
        };
        (td, sm)
    }

    #[test]
    fn create_session_with_name() {
        let (_td, sm) = sm();
        let (id, tok) = sm.create_session("/home/user/bios").unwrap();
        assert!(sm.validate_token(&id, &tok));
        let sessions = sm.list_sessions().unwrap();
        assert_eq!(sessions[0].name, "/home/user/bios");
    }

    #[test]
    fn destroy_without_purge_keeps_files() {
        let (td, sm) = sm();
        let (id, _) = sm.create_session("n").unwrap();
        let art_dir = td.path().join("sessions").join(&id).join("artifacts");
        std::fs::create_dir_all(&art_dir).unwrap();
        std::fs::write(art_dir.join("a1.bin"), b"data").unwrap();
        sm.destroy_session(&id, false).unwrap();
        assert!(sm.list_sessions().unwrap().is_empty());
        assert!(
            art_dir.join("a1.bin").exists(),
            "files must survive when purge=false"
        );
    }

    #[test]
    fn destroy_with_purge_removes_files() {
        let (td, sm) = sm();
        let (id, _) = sm.create_session("n").unwrap();
        let sess_dir = td.path().join("sessions").join(&id);
        std::fs::create_dir_all(&sess_dir).unwrap();
        sm.destroy_session(&id, true).unwrap();
        assert!(!sess_dir.exists(), "files must be removed when purge=true");
    }
}
