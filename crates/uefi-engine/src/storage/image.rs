use anyhow::Result;
use std::fs;
use std::path::{Path, PathBuf};

pub fn store_image_file(
    data_dir: &Path,
    session_id: &str,
    image_id: &str,
    bytes: &[u8],
) -> Result<PathBuf> {
    let dir = data_dir.join("sessions").join(session_id).join("images");
    fs::create_dir_all(&dir)?;
    let path = dir.join(format!("{image_id}.bin"));
    atomic_write(&path, bytes)?;
    Ok(path)
}

pub fn read_image_file(data_dir: &Path, session_id: &str, image_id: &str) -> Result<Vec<u8>> {
    let path = data_dir
        .join("sessions")
        .join(session_id)
        .join("images")
        .join(format!("{image_id}.bin"));
    Ok(fs::read(&path)?)
}

pub fn remove_image_file(data_dir: &Path, session_id: &str, image_id: &str) -> Result<()> {
    let path = data_dir
        .join("sessions")
        .join(session_id)
        .join("images")
        .join(format!("{image_id}.bin"));
    if path.exists() {
        fs::remove_file(&path)?;
    }
    Ok(())
}

pub fn store_snapshot_file(
    data_dir: &Path,
    session_id: &str,
    image_id: &str,
    snapshot_id: &str,
    bytes: &[u8],
) -> Result<()> {
    let dir = data_dir
        .join("sessions")
        .join(session_id)
        .join("images")
        .join(format!("{image_id}.snapshots"));
    fs::create_dir_all(&dir)?;
    atomic_write(&dir.join(format!("{snapshot_id}.bin")), bytes)
}

pub fn read_snapshot_file(
    data_dir: &Path,
    session_id: &str,
    image_id: &str,
    snapshot_id: &str,
) -> Result<Vec<u8>> {
    let p = data_dir
        .join("sessions")
        .join(session_id)
        .join("images")
        .join(format!("{image_id}.snapshots"))
        .join(format!("{snapshot_id}.bin"));
    fs::read(p).map_err(Into::into)
}

pub fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("bin.tmp");
    fs::write(&tmp, bytes)?;
    fs::rename(&tmp, path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn store_and_read_roundtrip() {
        let td = TempDir::new().unwrap();
        let bytes = b"hello bios";
        let path = store_image_file(td.path(), "s1", "img1", bytes).unwrap();
        assert!(path.ends_with("sessions/s1/images/img1.bin"));
        let read = read_image_file(td.path(), "s1", "img1").unwrap();
        assert_eq!(read, bytes);
    }

    #[test]
    fn store_overwrites_existing() {
        let td = TempDir::new().unwrap();
        store_image_file(td.path(), "s1", "img1", b"old").unwrap();
        store_image_file(td.path(), "s1", "img1", b"new longer").unwrap();
        let read = read_image_file(td.path(), "s1", "img1").unwrap();
        assert_eq!(read, b"new longer");
    }

    #[test]
    fn remove_existing_file() {
        let td = TempDir::new().unwrap();
        store_image_file(td.path(), "s1", "img1", b"x").unwrap();
        remove_image_file(td.path(), "s1", "img1").unwrap();
        assert!(read_image_file(td.path(), "s1", "img1").is_err());
    }

    #[test]
    fn remove_missing_file_is_ok() {
        let td = TempDir::new().unwrap();
        remove_image_file(td.path(), "s1", "missing").unwrap();
    }

    #[test]
    fn atomic_write_persists_bytes() {
        let td = TempDir::new().unwrap();
        let path = td.path().join("a.bin");
        atomic_write(&path, b"data").unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"data");
    }

    #[test]
    fn atomic_write_no_tmp_left_behind() {
        let td = TempDir::new().unwrap();
        let path = td.path().join("a.bin");
        atomic_write(&path, b"data").unwrap();
        let tmp = path.with_extension("bin.tmp");
        assert!(!tmp.exists(), "tmp file must be renamed, not left behind");
    }
}
