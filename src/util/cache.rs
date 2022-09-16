use anyhow::Result;
use dashmap::DashSet;
use std::{fs, io::Write, path::PathBuf};

pub struct Cache {
    path: PathBuf,
    pub inner: DashSet<String>,
}

impl Cache {
    pub fn new(name: &str) -> Self {
        let mut path = dirs::cache_dir().unwrap();

        path.push("rsink");

        fs::create_dir_all(&path).ok();

        path.push(name);

        let mut file = fs::File::options()
            .read(true)
            .write(true)
            .create(true)
            .open(&path)
            .unwrap();

        if file.metadata().unwrap().len() == 0 {
            file.write_all(b"[]").unwrap();
        }

        let array: Vec<String> = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();

        Self {
            inner: DashSet::from_iter(array.into_iter()),
            path,
        }
    }

    pub fn save(&self) -> Result<()> {
        let array: Vec<String> = self.inner.iter().map(|x| x.to_string()).collect();
        fs::write(&self.path, serde_json::to_string(&array)?)?;
        log::debug!("Saved cache file includes {} item", array.len());
        Ok(())
    }
}
