use crate::SYNC_DIR;
pub use anyhow::Result;
use spinners::{Spinner, Spinners};
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

pub fn spinner<F>(message: &str, stop_message: &str, operation: F)
where
    F: Fn() -> Result<()>,
{
    let mut sp = Spinner::new(Spinners::Dots9, message.into());

    operation().unwrap();

    if stop_message.is_empty() {
        sp.stop();
    } else {
        sp.stop_with_message(stop_message.into());
    }
}
