use crate::cloud::*;
use crate::util::*;
use crate::{SETTINGS, SYNC_DIR};
use s3::serde_types::ListBucketResult;
use s3::{creds::Credentials, Bucket, Region};
use std::fs;
use std::io::Write;
use std::path::PathBuf;

pub struct Cloud {
    bucket: Bucket,
}

fn init_bucket() -> Result<Bucket> {
    let s3_secret = SETTINGS.get_string("s3.secret")?;
    let s3_key = SETTINGS.get_string("s3.key")?;

    Ok(Bucket::new(
        &SETTINGS.get_string("s3.bucket_name")?,
        Region::Custom {
            endpoint: SETTINGS.get_string("s3.endpoint").unwrap_or_default(),
            region: SETTINGS.get_string("s3.region").unwrap_or_default(),
        },
        Credentials::new(Some(&s3_key), Some(&s3_secret), None, None, None)?,
    )?
    .with_path_style())
}

fn key_to_path(key: &str) -> PathBuf {
    let mut path = SYNC_DIR.clone();
    path.push(key);
    path
}

impl CloudAdapter for Cloud {
    fn new() -> Self {
        Self {
            bucket: init_bucket().unwrap(),
        }
    }

    fn sync(&self) -> Result<u32> {
        let mut synced = 0;

        for list in self.bucket.list("/".to_owned(), None)? as Vec<ListBucketResult> {
            for obj in list.contents {
                let path = key_to_path(&obj.key);
                let mut file = fs::File::options()
                    .read(true)
                    .write(true)
                    .create(true)
                    .open(&path)?;
                let metadata = file.metadata()?;

                if metadata.len() != obj.size
                /* || !compare_date(metadata.modified()?, obj.last_modified) */
                {
                    file.write_all(&self.get(&path)?)?;
                    synced += 1;
                }
            }
        }

        for entry in walk_dir(SYNC_DIR.to_path_buf())? {
            let path = entry.path();
            if !self.exists(&path)? {
                self.save(&path).unwrap();
                synced += 1;
            }
        }

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
            .put_object(normalize_path(path), &Cloud::read_file(path)?)?;
        Ok(())
    }

    fn rename(&self, oldpath: &Path, path: &Path) -> Result<()> {
        self.bucket
            .copy_object_internal(normalize_path(oldpath), normalize_path(path))?;
        self.delete(oldpath)?;
        Ok(())
    }
}
