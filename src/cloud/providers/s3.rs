use crate::cloud::CloudAdapter;
use crate::config::*;
use crate::util::*;
use crate::SYNCED_PATHS;
use s3::{
    serde_types::ListBucketResult,
    {creds::Credentials, Bucket, Region},
};
use std::{collections::HashSet, fs, io::Write, path::Path};

pub struct S3Storage {
    bucket: Bucket,
}

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

    fn sync(&self) -> Result<u32> {
        let mut synced = 0;
        let mut objects = HashSet::new();

        for list in self.bucket.list("/".to_owned(), None)? as Vec<ListBucketResult> {
            for obj in list.contents {
                let path = key_to_path(&obj.key);

                objects.insert(path.clone());

                if let Some(parent) = path.parent() {
                    fs::create_dir_all(parent)?;
                }

                let mut file = fs::OpenOptions::new()
                    .write(true)
                    .read(true)
                    .create(true)
                    .open(&path)?;
                let metadata = file.metadata()?;

                SYNCED_PATHS.0.insert(stringify_path(&path));

                if metadata.len() != obj.size
                /* || !compare_date(metadata.modified()?, obj.last_modified) */
                {
                    file.write_all(&self.get(&path)?)?;
                    synced += 1;
                }
            }
        }

        for entry in walk_dir(&CONFIG.path)? {
            let path = entry.path();

            if !objects.contains(&path) {
                if SYNCED_PATHS.0.remove(&stringify_path(&path)).is_some() {
                    if path.is_dir() {
                        fs::remove_dir(&path)?;
                    } else if path.is_file() {
                        fs::remove_file(&path)?;
                    } else {
                        unreachable!()
                    }
                } else {
                    self.save(&path)?;
                    SYNCED_PATHS.0.insert(stringify_path(&path));
                    synced += 1;
                }
            }
        }

        SYNCED_PATHS.save()?;

        Ok(synced)
    }

    fn exists(&self, path: &Path) -> Result<bool> {
        let (_, code): (_, u16) = self.bucket.head_object(normalize_path(path))?;
        Ok(code == 200)
    }

    fn get(&self, path: &Path) -> Result<Vec<u8>> {
        let res = self.bucket.get_object(normalize_path(path))?;
        Ok(res.bytes().into())
    }

    fn delete(&self, path: &Path) -> Result<()> {
        self.bucket.delete_object(normalize_path(path))?;
        Ok(())
    }

    fn save(&self, path: &Path) -> Result<()> {
        self.bucket
            .put_object(normalize_path(path), &Self::read_file(path)?)?;
        Ok(())
    }

    fn rename(&self, from: &Path, to: &Path) -> Result<()> {
        self.bucket
            .copy_object_internal(normalize_path(from), normalize_path(to))?;
        self.delete(from)?;
        Ok(())
    }

    fn kind(&self) -> &'static str {
        "s3"
    }
}
