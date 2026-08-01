use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::error::{AppError, ErrKind};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct State {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active_image_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sock_path: Option<String>,
}

pub fn state_path() -> PathBuf {
    PathBuf::from(".uefipatcher")
}

pub fn read_state() -> Result<State, AppError> {
    let path = state_path();
    if !path.exists() {
        return Ok(State::default());
    }
    let content = std::fs::read_to_string(&path)
        .map_err(|e| AppError::new(ErrKind::IoError, e.to_string()))?;
    toml::from_str(&content).map_err(|e| AppError::new(ErrKind::StateCorrupt, e.to_string()))
}

pub fn require_state() -> Result<State, AppError> {
    let path = state_path();
    if !path.exists() {
        return Err(AppError::new(
            ErrKind::StateMissing,
            "run `uefi-cli session init` first",
        ));
    }
    read_state()
}

pub fn write_state(state: &State) -> Result<(), AppError> {
    let s =
        toml::to_string(state).map_err(|e| AppError::new(ErrKind::StateCorrupt, e.to_string()))?;
    let tmp = PathBuf::from(".uefipatcher.tmp");
    std::fs::write(&tmp, s).map_err(|e| AppError::new(ErrKind::IoError, e.to_string()))?;
    std::fs::rename(&tmp, state_path())
        .map_err(|e| AppError::new(ErrKind::IoError, e.to_string()))?;
    Ok(())
}

pub fn resolve_sock(cli_sock: Option<&str>, state: &State) -> PathBuf {
    if let Some(s) = cli_sock {
        return PathBuf::from(s);
    }
    if let Ok(env) = std::env::var("UEFIPATCHER_SOCK") {
        return PathBuf::from(env);
    }
    if let Some(s) = &state.sock_path {
        return PathBuf::from(s);
    }
    default_sock()
}

pub fn default_sock() -> PathBuf {
    if let Some(b) = directories::BaseDirs::new()
        && let Some(state) = b.state_dir()
    {
        return state.join("uefipatcher").join("uefipatcher.sock");
    }
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
    PathBuf::from(home)
        .join(".local")
        .join("state")
        .join("uefipatcher")
        .join("uefipatcher.sock")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsString;
    use std::path::Path;
    use std::sync::{Mutex, MutexGuard, OnceLock};
    use tempfile::TempDir;

    fn lock() -> &'static Mutex<()> {
        static L: OnceLock<Mutex<()>> = OnceLock::new();
        L.get_or_init(|| Mutex::new(()))
    }

    fn lock_guard() -> MutexGuard<'static, ()> {
        lock().lock().unwrap()
    }

    struct CwdGuard(PathBuf);
    impl Drop for CwdGuard {
        fn drop(&mut self) {
            let _ = std::env::set_current_dir(&self.0);
        }
    }

    fn in_tempdir<F: FnOnce(&Path)>(f: F) {
        let _g = lock_guard();
        let td = TempDir::new().unwrap();
        let prev = std::env::current_dir().unwrap();
        let _cwd = CwdGuard(prev);
        std::env::set_current_dir(td.path()).unwrap();
        f(td.path());
    }

    struct EnvGuard {
        key: &'static str,
        prev: Option<OsString>,
    }
    impl Drop for EnvGuard {
        fn drop(&mut self) {
            // SAFETY: tests are serialized by `lock()`, so no other test
            // touches this process-global env var concurrently.
            unsafe {
                match &self.prev {
                    Some(v) => std::env::set_var(self.key, v),
                    None => std::env::remove_var(self.key),
                }
            }
        }
    }
    fn env_guard(key: &'static str, val: &str) -> EnvGuard {
        let prev = std::env::var_os(key);
        // SAFETY: serialized by `lock()`.
        unsafe {
            std::env::set_var(key, val);
        }
        EnvGuard { key, prev }
    }

    #[test]
    fn write_read_roundtrip() {
        in_tempdir(|_| {
            let s = State {
                session_id: Some("s1".into()),
                token: Some("t1".into()),
                active_image_id: None,
                sock_path: Some("/tmp/sock".into()),
            };
            write_state(&s).unwrap();
            let r = read_state().unwrap();
            assert_eq!(r.session_id.as_deref(), Some("s1"));
            assert_eq!(r.sock_path.as_deref(), Some("/tmp/sock"));
        });
    }

    #[test]
    fn read_missing_returns_empty() {
        in_tempdir(|_| {
            let r = read_state().unwrap();
            assert!(r.session_id.is_none());
        });
    }

    #[test]
    fn require_missing_errors() {
        in_tempdir(|_| {
            let r = require_state();
            assert_eq!(r.unwrap_err().kind, ErrKind::StateMissing);
        });
    }

    #[test]
    fn corrupt_toml_errors() {
        in_tempdir(|_| {
            std::fs::write(".uefipatcher", "not = valid = toml").unwrap();
            let r = read_state();
            assert_eq!(r.unwrap_err().kind, ErrKind::StateCorrupt);
        });
    }

    #[test]
    fn resolve_sock_priority_cli_over_env() {
        let _g = lock_guard();
        let st = State {
            sock_path: Some("/from-state".into()),
            ..Default::default()
        };
        let _env = env_guard("UEFIPATCHER_SOCK", "/from-env");
        assert_eq!(
            resolve_sock(Some("/from-cli"), &st),
            PathBuf::from("/from-cli")
        );
    }

    #[test]
    fn resolve_sock_priority_env_over_state() {
        let _g = lock_guard();
        let st = State {
            sock_path: Some("/from-state".into()),
            ..Default::default()
        };
        let _env = env_guard("UEFIPATCHER_SOCK", "/from-env");
        assert_eq!(resolve_sock(None, &st), PathBuf::from("/from-env"));
    }

    #[test]
    fn resolve_sock_priority_state_over_default() {
        let _g = lock_guard();
        let st = State {
            sock_path: Some("/from-state".into()),
            ..Default::default()
        };
        assert_eq!(resolve_sock(None, &st), PathBuf::from("/from-state"));
    }
}
