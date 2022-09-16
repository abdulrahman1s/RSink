use super::*;
pub use anyhow::Result;
pub use dashmap::DashSet;
pub use serde::Deserialize;
pub use std::path::{Path, PathBuf};

#[derive(Deserialize, Clone)]
#[serde(tag = "provider", rename_all = "snake_case")]
pub enum BackendOptions {
    S3(s3::S3Options),
}

pub static TRASH_PATH: &str = ".trash/";

pub enum Operation {
    Write(PathBuf),
    WriteEmpty(PathBuf),
    Upload(PathBuf),
    Checked(PathBuf),
}

impl Operation {
    pub fn path(&self) -> PathBuf {
        match self {
            Operation::Write(p)
            | Operation::Upload(p)
            | Operation::Checked(p)
            | Operation::WriteEmpty(p) => p.clone(),
        }
    }
}

#[async_trait]
pub trait Backend {
    async fn init(options: BackendOptions) -> Self;
    async fn remove(&self, path: &str) -> Result<()>;
    async fn download(&self, path: &str) -> Result<Vec<u8>>;
    async fn exists(&self, path: &str) -> Result<bool>;
    async fn rename(&self, old_path: &str, path: &str) -> Result<()>;
    async fn sync(&self) -> Result<Vec<Operation>>;
    async fn upload(&self, path: &str, content: &[u8]) -> Result<()>;
}
