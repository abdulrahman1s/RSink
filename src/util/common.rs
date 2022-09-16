use crate::config::CONFIG;
use crate::IS_INTERNET_AVAILABLE;
pub use anyhow::Result;
use std::{
    fs::{self, DirEntry},
    path::{Path, PathBuf},
};
use time::OffsetDateTime;

pub fn normalize_path(path: &Path) -> String {
    let mut normalized_path = PathBuf::new();
    let mut found = false;
    let root = CONFIG.path.components().last().unwrap();

    for part in path.components() {
        if found {
            normalized_path.push(part);
        } else if part == root {
            found = true;
        }
    }

    normalized_path.to_string_lossy().to_string()
}

pub fn walk_dir(dir: &Path) -> Result<Vec<DirEntry>> {
    let mut result = vec![];

    if dir.is_dir() {
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();

            if path.is_dir() {
                result.append(&mut walk_dir(&path)?);
            } else {
                result.push(entry);
            }
        }
    }

    Ok(result)
}

pub fn settings_file_path() -> PathBuf {
    let mut path = dirs::config_dir().unwrap();

    path.push("rsink");

    fs::create_dir_all(&path).ok();

    path.push("rsink.conf");

    path
}

pub async fn check_connectivity() {
    *IS_INTERNET_AVAILABLE.lock().unwrap() = online::check(None).await.is_ok();
}

pub fn log_error(err: anyhow::Error) -> Result<()> {
    log::error!("An error has occurred: {err:?}");
    Ok(())
}

pub fn key_to_path(key: &str) -> PathBuf {
    let mut path = CONFIG.path.clone();
    path.push(key);
    path
}

pub async fn metadata_of(path: &Path) -> (bool, u64, Option<OffsetDateTime>) {
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await.ok();
    }

    tokio::fs::metadata(path)
        .await
        .map(|m| (true, m.len(), m.modified().map(|x| x.into()).ok()))
        .unwrap_or((false, 0, None))
}
