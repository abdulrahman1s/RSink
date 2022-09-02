pub use anyhow::Result;
pub use std::path::Path;

pub trait CloudAdapter {
    fn new() -> Self;
    fn sync(&self) -> Result<u32>;
    fn get(&self, path: &Path) -> Result<Vec<u8>>;
    fn exists(&self, path: &Path) -> Result<bool>;
    fn delete(&self, path: &Path) -> Result<()>;
    fn save(&self, path: &Path) -> Result<()>;
    fn rename(&self, oldpath: &Path, path: &Path) -> Result<()>;
    fn read_file(path: &Path) -> Result<Vec<u8>> {
        Ok(std::fs::read(path)?)
    }
}
