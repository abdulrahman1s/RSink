use super::interface::*;
use crate::util::*;
use s3::{creds::Credentials, Bucket, Region};
use time::{format_description::well_known::Rfc3339, OffsetDateTime};

#[derive(Deserialize, Clone)]
pub struct S3Options {
    pub bucket_name: String,
    pub access_key_id: String,
    pub secret_access_key: String,
    pub region: Option<String>,
    pub endpoint: Option<String>,
    #[serde(default)]
    pub size_only: bool,
    #[serde(default)]
    pub checksum: bool,
    #[serde(default)]
    pub move_to_trash: bool,
}

pub struct S3 {
    opts: S3Options,
    bucket: Bucket,
}

#[async_trait]
impl Backend for S3 {
    async fn init(options: BackendOptions) -> Self {
        let BackendOptions::S3(opts) = options;
        let bucket = Bucket::new(
            &opts.bucket_name,
            Region::Custom {
                endpoint: opts.endpoint.clone().unwrap_or_default(),
                region: opts.region.clone().unwrap_or_default(),
            },
            Credentials::new(
                Some(&opts.access_key_id),
                Some(&opts.secret_access_key),
                None,
                None,
                None,
            )
            .unwrap(),
        )
        .unwrap()
        .with_path_style();

        Self { opts, bucket }
    }

    async fn sync(&self) -> Result<Vec<Operation>> {
        let mut operations = vec![];

        for list in self.bucket.list("/".to_owned(), None).await? {
            for obj in list.contents {
                if obj.key.starts_with(TRASH_PATH) {
                    continue;
                }

                let path = key_to_path(&obj.key);
                let (exists, size, last_modified) = metadata_of(&path).await;

                if size == obj.size {
                    if obj.size == 0 && !exists {
                        operations.push(Operation::WriteEmpty(path));
                    } else {
                        operations.push(Operation::Checked(path))
                    }
                    continue;
                }

                log::debug!(
                    "{:?} has different size, cloud({}) != local({})",
                    path,
                    obj.size,
                    size
                );

                let prefer_local = match last_modified {
                    Some(last_modified) if !self.opts.size_only => {
                        let cloud_last_modified =
                            OffsetDateTime::parse(&obj.last_modified, &Rfc3339).unwrap();
                        let local_last_modified = last_modified;
                        log::debug!("{path:?} last modified: local({local_last_modified}) > cloud({cloud_last_modified}) = {}", local_last_modified > cloud_last_modified);
                        obj.size == 0 || local_last_modified > cloud_last_modified
                    }
                    _ => obj.size == 0,
                };

                if prefer_local {
                    log::debug!("Preferring local {path:?} instead of cloud version");
                    operations.push(Operation::Upload(path));
                } else {
                    operations.push(Operation::Write(path));
                }
            }
        }

        Ok(operations)
    }

    async fn download(&self, path: &str) -> Result<Vec<u8>> {
        let res = self.bucket.get_object(path).await?;
        Ok(res.bytes().into())
    }

    async fn exists(&self, path: &str) -> Result<bool> {
        let (_, code) = self.bucket.head_object(path).await?;
        Ok(code == 200)
    }

    async fn remove(&self, path: &str) -> Result<()> {
        if self.opts.move_to_trash {
            self.bucket
                .copy_object_internal(path, TRASH_PATH.to_owned() + path)
                .await?;
        }
        self.bucket.delete_object(path).await?;
        Ok(())
    }

    async fn upload(&self, path: &str, content: &[u8]) -> Result<()> {
        self.bucket.put_object(path, content).await?;
        Ok(())
    }

    async fn rename(&self, from: &str, to: &str) -> Result<()> {
        self.bucket.copy_object_internal(from, to).await?;
        self.bucket.delete_object(from).await?;
        Ok(())
    }
}
