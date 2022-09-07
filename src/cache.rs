use crate::util::*;
use dashmap::DashSet;
use std::{fs, path::PathBuf};

lazy_static! {
    static ref PATH: PathBuf = cache_file_path();
}

pub struct Cache(pub DashSet<String>);

impl Cache {
    pub fn new() -> Self {
        let array: Vec<String> = serde_json::from_slice(&std::fs::read(&*PATH).unwrap()).unwrap();
        Self(DashSet::from_iter(array.into_iter()))
    }

    pub fn save(&self) -> Result<()> {
        let array: Vec<String> = self.0.iter().map(|x| x.to_string()).collect();
        fs::write(&*PATH, serde_json::to_string(&array)?)?;
        log::debug!("Saved cache file includes {} item", array.len());
        Ok(())
    }
}

impl Default for Cache {
    fn default() -> Self {
        Self::new()
    }
}
