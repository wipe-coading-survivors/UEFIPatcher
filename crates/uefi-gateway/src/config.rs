use anyhow::Result;
use std::net::SocketAddr;
use std::path::PathBuf;

pub struct Config {
    pub listen: SocketAddr,
    pub sock_path: PathBuf,
}

pub fn load_config() -> Result<Config> {
    let listen: SocketAddr = std::env::var("UEFIPATCHER_GATEWAY_LISTEN")
        .unwrap_or_else(|_| "0.0.0.0:8080".into())
        .parse()?;
    let sock_path = std::env::var("UEFIPATCHER_SOCK")
        .map(PathBuf::from)
        .unwrap_or_else(|_| uefi_common::state::default_sock());
    Ok(Config { listen, sock_path })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsString;
    use std::sync::{Mutex, MutexGuard, OnceLock};

    fn lock() -> &'static Mutex<()> {
        static L: OnceLock<Mutex<()>> = OnceLock::new();
        L.get_or_init(|| Mutex::new(()))
    }

    fn lock_guard() -> MutexGuard<'static, ()> {
        lock().lock().unwrap()
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
    fn env_guard(key: &'static str, val: Option<&str>) -> EnvGuard {
        let prev = std::env::var_os(key);
        // SAFETY: serialized by `lock()`.
        unsafe {
            match val {
                Some(v) => std::env::set_var(key, v),
                None => std::env::remove_var(key),
            }
        }
        EnvGuard { key, prev }
    }

    #[test]
    fn sock_default_follows_uefi_common_default_sock() {
        let _g = lock_guard();
        let _s = env_guard("UEFIPATCHER_SOCK", None);
        let _l = env_guard("UEFIPATCHER_GATEWAY_LISTEN", None);
        let cfg = load_config().unwrap();
        assert_eq!(cfg.sock_path, uefi_common::state::default_sock());
    }

    #[test]
    fn sock_env_overrides_default() {
        let _g = lock_guard();
        let _s = env_guard("UEFIPATCHER_SOCK", Some("/explicit/uefipatcher.sock"));
        let _l = env_guard("UEFIPATCHER_GATEWAY_LISTEN", None);
        let cfg = load_config().unwrap();
        assert_eq!(cfg.sock_path, PathBuf::from("/explicit/uefipatcher.sock"));
    }
}
