use crate::{IS_INTERNET_AVAILABLE, SYNC_DIR};
pub use anyhow::Result;
use std::{
    fs::{self, DirEntry},
    io,
    path::{Path, PathBuf},
    time::SystemTime,
};

pub fn normalize_path(path: &Path) -> String {
    let mut normalized_path = PathBuf::new();
    let mut found = false;
    let root = SYNC_DIR.components().last().unwrap();

    for part in path.components() {
        if found {
            normalized_path.push(part);
        } else if part == root {
            found = true;
        }
    }

    normalized_path.to_string_lossy().to_string()
}

#[allow(dead_code)]
pub fn remove_timezone(s: String) -> String {
    let mut s = s.split(':').collect::<Vec<&str>>();
    s.remove(2);
    s.join(":")
}

#[allow(dead_code, unused_variables)]
pub fn compare_date(local: SystemTime, cloud: String) -> bool {
    todo!()
}

pub fn walk_dir(dir: PathBuf) -> io::Result<Vec<DirEntry>> {
    let mut result = vec![];

    if dir.is_dir() {
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();

            if path.is_dir() {
                result.append(&mut walk_dir(path)?);
            } else {
                result.push(entry);
            }
        }
    }

    Ok(result)
}

pub fn settings_file_path() -> Result<PathBuf> {
    let mut path = dirs::config_dir().unwrap();

    path.push("rsink");

    fs::create_dir_all(&path)?;

    path.push("config.toml");

    let file = fs::File::options()
        .write(true)
        .read(true)
        .create(true)
        .open(&path)?;

    if file.metadata()?.len() == 0 {
        return Err(anyhow::anyhow!(
            "Please configure the settings file at {}",
            path.to_string_lossy()
        ));
    }

    Ok(path)
}

pub fn check_connectivity() {
    *IS_INTERNET_AVAILABLE.lock().unwrap() = online::sync::check(None).is_ok();
}

pub fn maybe_error(result: Result<()>) {
    if let Err(error) = result {
        log::error!("An error has occurred: {error:?}");
    }
}
