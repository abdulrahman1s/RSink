use crate::cloud::CloudAdapter;
use crate::config::*;
use crate::util::*;
use crate::SYNCED_PATHS;
use dashmap::DashSet;
use s3::{creds::Credentials, Bucket, Region};
use std::path::Path;
use tokio::{fs, io::AsyncWriteExt};

pub struct S3Storage {
    bucket: Bucket,
}

#[async_trait]
impl CloudAdapter for S3Storage {
    fn new(options: CloudOptions) -> Self {
        let CloudOptions::S3 {
            key,
            secret,
            region,
            endpoint,
            bucket_name,
        } = options;

        Self {
            bucket: Bucket::new(
                &bucket_name,
                Region::Custom {
                    endpoint: endpoint.unwrap_or_default(),
                    region: region.unwrap_or_default(),
                },
                Credentials::new(Some(&key), Some(&secret), None, None, None).unwrap(),
            )
            .unwrap()
            .with_path_style(),
        }
    }

    async fn sync(&self) -> Result<u32> {
        let mut synced = 0;
        let objects = DashSet::new();

        for list in self.bucket.list("/".to_owned(), None).await? {
            for obj in list.contents {
                let path = key_to_path(&obj.key);

                objects.insert(path.clone());

                if let Some(parent) = path.parent() {
                    fs::create_dir_all(parent).await?;
                }

                let mut file = fs::OpenOptions::new()
                    .write(true)
                    .read(true)
                    .create(true)
                    .open(&path)
                    .await?;
                let metadata = file.metadata().await?;

                SYNCED_PATHS.0.insert(stringify_path(&path));

                if metadata.len() != obj.size
                /* || !compare_date(metadata.modified().await?, obj.last_modified) */
                {
                    file.write_all(&self.get(&path).await?).await?;
                    synced += 1;
                }
            }
        }

        for entry in walk_dir(&CONFIG.path)? {
            let path = entry.path();

            if !objects.contains(&path) {
                if SYNCED_PATHS.0.remove(&stringify_path(&path)).is_some() {
                    if path.is_dir() {
                        fs::remove_dir(&path).await?;
                    } else if path.is_file() {
                        fs::remove_file(&path).await?;
                    } else {
                        unreachable!()
                    }
                } else {
                    self.save(&path).await?;
                    SYNCED_PATHS.0.insert(stringify_path(&path));
                    synced += 1;
                }
            }
        }

        SYNCED_PATHS.save()?;

        Ok(synced)
    }

    async fn exists(&self, path: &Path) -> Result<bool> {
        let (_, code): (_, u16) = self.bucket.head_object(normalize_path(path)).await?;
        Ok(code == 200)
    }

    async fn get(&self, path: &Path) -> Result<Vec<u8>> {
        let res = self.bucket.get_object(normalize_path(path)).await?;
        Ok(res.bytes().into())
    }

    async fn delete(&self, path: &Path) -> Result<()> {
        self.bucket.delete_object(normalize_path(path)).await?;
        Ok(())
    }

    async fn save(&self, path: &Path) -> Result<()> {
        self.bucket
            .put_object(normalize_path(path), &Self::read_file(path).await?)
            .await?;
        Ok(())
    }

    async fn rename(&self, from: &Path, to: &Path) -> Result<()> {
        self.bucket
            .copy_object_internal(normalize_path(from), normalize_path(to))
            .await?;
        self.delete(from).await?;
        Ok(())
    }

    fn kind(&self) -> &'static str {
        "s3"
    }
}
