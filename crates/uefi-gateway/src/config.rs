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
        .unwrap_or_else(|_| PathBuf::from("/run/uefipatcher.sock"));
    Ok(Config { listen, sock_path })
}
