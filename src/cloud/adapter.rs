use crate::config::CloudOptions;
pub use anyhow::Result;
use dashmap::DashSet;
pub use std::path::{Path, PathBuf};

pub enum Operation {
    Write(PathBuf), // Load file from cloud
    WriteEmpty(PathBuf),
    Save(PathBuf), // Save local file to cloud
}

#[async_trait]
pub trait CloudAdapter {
    async fn init(options: CloudOptions) -> Self;
    async fn sync(&self) -> Result<(DashSet<PathBuf>, Vec<Operation>)>;
    async fn get(&self, path: &Path) -> Result<Vec<u8>>;
    async fn exists(&self, path: &Path) -> Result<bool>;
    async fn delete(&self, path: &Path) -> Result<()>;
    async fn save(&self, path: &Path, content: &[u8]) -> Result<()>;
    async fn rename(&self, oldpath: &Path, path: &Path) -> Result<()>;
    fn kind(&self) -> &'static str;
}
