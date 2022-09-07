use crate::config::CONFIG;
use crate::IS_INTERNET_AVAILABLE;
pub use anyhow::Result;
use std::{
    fs::{self, DirEntry},
    io::{self, Write},
    path::{Path, PathBuf},
};

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

pub fn walk_dir(dir: &Path) -> io::Result<Vec<DirEntry>> {
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

pub fn settings_file_path(extension: &str) -> PathBuf {
    let mut path = dirs::config_dir().unwrap();

    path.push("rsink");

    fs::create_dir_all(&path).ok();

    path.push(format!("config.{extension}"));
    path
}

pub async fn check_connectivity() {
    *IS_INTERNET_AVAILABLE.lock().unwrap() = online::check(None).await.is_ok();
}

pub fn log_error(err: anyhow::Error) -> Result<()> {
    log::error!("An error has occurred: {err:?}");
    Ok(())
}

pub fn stringify_path(path: &Path) -> String {
    path.to_string_lossy().to_string()
}

pub fn key_to_path(key: &str) -> PathBuf {
    let mut path = CONFIG.path.clone();
    path.push(key);
    path
}

pub fn cache_file_path() -> PathBuf {
    let mut path = dirs::cache_dir().unwrap();

    path.push("rsink");

    fs::create_dir_all(&path).unwrap();

    path.push("cache.json");

    let mut file = fs::File::options()
        .read(true)
        .write(true)
        .create(true)
        .open(&path)
        .unwrap();

    if file.metadata().unwrap().len() == 0 {
        file.write_all(b"[]").unwrap();
    }

    path
}
