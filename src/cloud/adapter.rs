use crate::config::CloudOptions;
pub use anyhow::Result;
pub use std::path::Path;
use tokio::fs;

#[async_trait]
pub trait CloudAdapter {
    fn new(options: CloudOptions) -> Self;
    async fn sync(&self) -> Result<u32>;
    async fn get(&self, path: &Path) -> Result<Vec<u8>>;
    async fn exists(&self, path: &Path) -> Result<bool>;
    async fn delete(&self, path: &Path) -> Result<()>;
    async fn save(&self, path: &Path) -> Result<()>;
    async fn rename(&self, oldpath: &Path, path: &Path) -> Result<()>;
    async fn read_file(path: &Path) -> Result<Vec<u8>> {
        Ok(fs::read(path).await?)
    }
    fn kind(&self) -> &'static str;
}
