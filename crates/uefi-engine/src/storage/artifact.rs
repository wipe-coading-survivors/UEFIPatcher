use anyhow::Result;
use std::fs;
use std::path::{Path, PathBuf};

pub fn store_artifact_file(
    data_dir: &Path,
    session_id: &str,
    artifact_id: &str,
    bytes: &[u8],
) -> Result<PathBuf> {
    let dir = data_dir.join("sessions").join(session_id).join("artifacts");
    fs::create_dir_all(&dir)?;
    let path = dir.join(format!("{artifact_id}.bin"));
    fs::write(&path, bytes)?;
    Ok(path)
}

pub fn read_artifact_file(data_dir: &Path, session_id: &str, artifact_id: &str) -> Result<Vec<u8>> {
    let path = data_dir
        .join("sessions")
        .join(session_id)
        .join("artifacts")
        .join(format!("{artifact_id}.bin"));
    Ok(fs::read(&path)?)
}

pub fn write_artifact_to_output(
    data_dir: &Path,
    session_id: &str,
    artifact_id: &str,
    output_path: &str,
) -> Result<()> {
    let bytes = read_artifact_file(data_dir, session_id, artifact_id)?;
    fs::write(output_path, &bytes)?;
    Ok(())
}
