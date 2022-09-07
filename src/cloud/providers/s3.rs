use crate::cloud::CloudAdapter;
use crate::cloud::Operation;
use crate::config::*;
use crate::util::*;
use crate::SYNCED_PATHS;
use dashmap::DashSet;
use s3::{creds::Credentials, Bucket, Region};
use std::path::Path;
use std::path::PathBuf;
use time::{format_description::well_known::Rfc3339, OffsetDateTime};

const TRASH_PATH: &str = ".trash/";

pub struct S3Storage {
    bucket: Bucket,
}

#[async_trait]
impl CloudAdapter for S3Storage {
    async fn init(options: CloudOptions) -> Self {
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

    async fn sync(&self) -> Result<(DashSet<PathBuf>, Vec<Operation>)> {
        let objects = DashSet::new();
        let mut operations = vec![];

        for list in self.bucket.list("/".to_owned(), None).await? {
            for obj in list.contents {
                if obj.key.starts_with(TRASH_PATH) {
                    continue;
                }

                let path = key_to_path(&obj.key);

                objects.insert(path.clone());

                let (exists, size, last_modified) = metadata_of(&path).await;

                SYNCED_PATHS.0.insert(stringify_path(&path));

                if size == obj.size {
                    if obj.size == 0 && !exists {
                        operations.push(Operation::WriteEmpty(path));
                    }
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
                        let local_last_modified = last_modified;
                        log::debug!("{path:?} last modified: local({local_last_modified}) > cloud({cloud_last_modified}) = {}", local_last_modified > cloud_last_modified);
                        return obj.size == 0 || local_last_modified > cloud_last_modified;
                    }

                    obj.size == 0
                };

                if prefer_local() {
                    log::debug!("Preferring local {path:?} instead of cloud version");
                    operations.push(Operation::Save(path));
                } else {
                    operations.push(Operation::Write(path));
                }
            }
        }

        Ok((objects, operations))
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
        self.bucket
            .copy_object_internal(
                normalize_path(path),
                TRASH_PATH.to_owned() + &normalize_path(path),
            )
            .await?;
        self.bucket.delete_object(normalize_path(path)).await?;
        Ok(())
    }

    async fn save(&self, path: &Path, content: &[u8]) -> Result<()> {
        self.bucket
            .put_object(normalize_path(path), content)
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
