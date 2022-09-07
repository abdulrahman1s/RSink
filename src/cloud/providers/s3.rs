use crate::cloud::CloudAdapter;
use crate::config::*;
use crate::util::*;
use crate::SYNCED_PATHS;
use dashmap::DashSet;
use s3::{creds::Credentials, Bucket, Region};
use std::path::Path;
use time::{format_description::well_known::Rfc3339, OffsetDateTime};
use tokio::fs;

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

                let (size, last_modified) = fs::metadata(&path)
                    .await
                    .map(|m| (m.len(), m.modified().ok()))
                    .unwrap_or((0, None));

                SYNCED_PATHS.0.insert(stringify_path(&path));

                if size == obj.size {
                    continue;
                }

                log::debug!(
                    "{:?} has different size, cloud({}) != local({})",
                    path,
                    obj.size,
                    size
                );

                let prefer_local = || {
                    if let Some(last_modified) = last_modified {
                        let cloud_last_modified =
                            OffsetDateTime::parse(&obj.last_modified, &Rfc3339).unwrap();
                        let local_last_modified = OffsetDateTime::from(last_modified);
                        log::debug!("{path:?} last modified: local({local_last_modified}) > cloud({cloud_last_modified}) = {}", local_last_modified > cloud_last_modified);
                        return obj.size == 0 || local_last_modified > cloud_last_modified;
                    }

                    obj.size == 0
                };

                if prefer_local() {
                    log::debug!("Preferring local {path:?} instead of cloud version");
                    self.save(&path).await?;
                } else {
                    let buffer = if obj.size == 0 {
                        vec![]
                    } else {
                        self.get(&path).await?
                    };
                    fs::write(&path, &buffer).await?;
                }

                synced += 1;
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
                    log::debug!("{:?} not synced, Saving to cloud...", path);
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
        let (_, code) = self.bucket.head_object(normalize_path(path)).await?;
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
