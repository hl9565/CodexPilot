use anyhow::Context;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackendStatus {
    pub status: String,
    pub version: String,
}

pub fn status_path() -> PathBuf {
    crate::app_paths::app_state_dir().join("status.json")
}

pub fn write_status(status: &BackendStatus) -> anyhow::Result<()> {
    let path = status_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    fs::write(&path, serde_json::to_vec_pretty(status)?)
        .with_context(|| format!("failed to write {}", path.display()))
}

pub fn clear_status() -> anyhow::Result<()> {
    let path = status_path();
    if !path.exists() {
        return Ok(());
    }
    fs::remove_file(&path).with_context(|| format!("failed to remove {}", path.display()))
}

pub fn read_status() -> anyhow::Result<Option<BackendStatus>> {
    let path = status_path();
    if !path.exists() {
        return Ok(None);
    }
    let bytes = fs::read(&path).with_context(|| format!("failed to read {}", path.display()))?;
    Ok(Some(serde_json::from_slice(&bytes)?))
}
