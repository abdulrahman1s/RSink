#[macro_use]
extern crate lazy_static;
#[macro_use]
extern crate async_trait;
extern crate notify;

mod backends;
mod util;

use backends::*;
use dashmap::DashSet;
use log::LevelFilter;
use notify::{event::*, recommended_watcher, RecursiveMode, Watcher};
use std::{
    str::FromStr,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::{fs, spawn, sync::mpsc::channel, time::sleep};
use util::{cache::*, config::*, *};

lazy_static! {
    pub static ref IS_INTERNET_AVAILABLE: Mutex<bool> = Mutex::new(false);
    pub static ref SYNCING: Mutex<bool> = Mutex::new(false);
    pub static ref SYNCED_PATHS: Cache = Cache::new("synced_paths");
}

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::builder()
        .parse_filters("serde_xml_rs=off,rustls=off,mio=off,want=off")
        .format_timestamp(None)
        .filter_level(LevelFilter::from_str(&CONFIG.log).expect("Invalid log level format"))
        .init();

    let cloud_ref = Arc::new(init_backend(CONFIG.backend.clone()).await);

    log::info!("Syncing directory: {:?}", CONFIG.path);
    log::info!("Syncing delay: {}ms", CONFIG.interval);

    let cloud = cloud_ref.clone();
    let fs_task = spawn(async move {
        let (tx, mut rx) = channel(100);
        let mut watcher = recommended_watcher(move |event| {
            futures::executor::block_on(async { tx.send(event).await.unwrap() });
        })?;
        let changes = Arc::new(DashSet::new());
        let mut tasks = vec![];

        watcher.watch(&CONFIG.path, RecursiveMode::Recursive)?;

        while let Some(event) = rx.recv().await {
            let event = match event {
                Ok(x) => x,
                Err(e) => {
                    log::error!("Notify Error {e}");
                    continue;
                }
            };

            log::debug!("{:?}", event);

            if !*IS_INTERNET_AVAILABLE.lock().unwrap() {
                log::warn!("Skip local syncing.. there are no internet connection");
                continue;
            }

            if *SYNCING.lock().unwrap() {
                log::debug!("Ignore event since online syncing is working");
                continue;
            }

            let cloud = cloud.clone();
            let changes = changes.clone();
            let task = spawn(async move {
                let path = &event.paths[0];
                let normalized_path = normalize_path(path);
                let is_file_exists = || async {
                    let debug_statement = |x| {
                        log::debug!("Is {:?} valid file path: {}", path, x);
                        x
                    };
                    debug_statement(
                        fs::metadata(path)
                            .await
                            .map(|m| m.is_file())
                            .unwrap_or(false),
                    )
                };

                match event.kind {
                    EventKind::Create(_) if is_file_exists().await => {
                        log::debug!("Uploading {:?}...", path);

                        cloud
                            .upload(&normalized_path, &fs::read(path).await?)
                            .await
                            .and_then(|_| {
                                if SYNCED_PATHS.inner.insert(normalized_path) {
                                    SYNCED_PATHS.save()?;
                                }
                                Ok(())
                            })
                            .or_else(log_error)?;
                    }
                    EventKind::Remove(_) => {
                        log::debug!("Removing {:?}...", path);

                        cloud
                            .remove(&normalized_path)
                            .await
                            .and_then(|_| {
                                if SYNCED_PATHS.inner.remove(&normalized_path).is_some() {
                                    SYNCED_PATHS.save()?;
                                }
                                Ok(())
                            })
                            .or_else(log_error)?;
                    }
                    EventKind::Access(AccessKind::Close(AccessMode::Write)) => {
                        if changes.remove(path).is_some() && is_file_exists().await {
                            log::debug!("Re-Uploading {:?}...", path);
                            cloud
                                .upload(&normalized_path, &fs::read(path).await?)
                                .await
                                .or_else(log_error)?;
                        }
                    }
                    EventKind::Modify(kind) => match kind {
                        ModifyKind::Data(_) => {
                            if SYNCED_PATHS.inner.contains(&normalized_path) {
                                changes.insert(path.clone());
                            }
                        }
                        ModifyKind::Name(_) if event.paths.len() == 2 => {
                            log::debug!("Moving from {:?} to {:?}", path, event.paths[1]);

                            if SYNCED_PATHS.inner.remove(&normalized_path).is_none() {
                                cloud
                                    .upload(
                                        &normalize_path(&event.paths[1]),
                                        &fs::read(&event.paths[1]).await?,
                                    )
                                    .await
                                    .and_then(|_| {
                                        SYNCED_PATHS.inner.insert(normalize_path(&event.paths[1]));
                                        SYNCED_PATHS.save()
                                    })
                                    .or_else(log_error)?;
                            } else {
                                cloud
                                    .rename(&normalized_path, &normalize_path(&event.paths[1]))
                                    .await
                                    .and_then(|_| {
                                        SYNCED_PATHS.inner.remove(&normalized_path);
                                        SYNCED_PATHS.inner.insert(normalize_path(&event.paths[1]));
                                        SYNCED_PATHS.save()
                                    })
                                    .or_else(log_error)?;
                            }
                        }
                        #[cfg(target_os = "android")]
                        ModifyKind::Metadata(MetadataKind::WriteTime) if is_file_exists().await => {
                            log::debug!("Uploading {:?}...", path);
                            cloud
                                .upload(&normalized_path, &fs::read(path).await?)
                                .await
                                .or_else(log_error)?;
                        }
                        _ => {}
                    },
                    _ => {}
                }

                Result::<()>::Ok(())
            });

            tasks.push(task);

            if tasks.len() == 5 {
                // The maximum running tasks is 5
                log::debug!("Flushing {} tasks", tasks.len());
                while let Some(task) = tasks.pop() {
                    task.await??;
                }
            }
        }

        Result::<()>::Ok(())
    });

    let cloud = cloud_ref;
    let cloud_task = spawn(async move {
        loop {
            check_connectivity().await;

            if *IS_INTERNET_AVAILABLE.lock().unwrap() {
                *SYNCING.lock().unwrap() = true;

                let mut synced = 0;
                let operations = cloud.sync().await?;
                let objects = operations
                    .iter()
                    .map(|x| x.path())
                    .collect::<DashSet<PathBuf>>();

                log::debug!("Sync operations: {}", operations.len());

                for op in &operations {
                    match op {
                        Operation::Checked(path) => {
                            SYNCED_PATHS.inner.insert(normalize_path(&path));
                        }
                        Operation::Upload(path) => {
                            log::debug!("Saving {path:?}");
                            cloud
                                .upload(&normalize_path(&path), &fs::read(&path).await?)
                                .await?;
                            synced += 1;
                        }
                        Operation::Write(path) => {
                            let buffer = cloud.download(&normalize_path(&path)).await?;
                            log::debug!("Writing {} bytes to {path:?}", buffer.len());
                            fs::write(&path, &buffer).await?;
                            synced += 1;
                        }
                        Operation::WriteEmpty(path) => {
                            log::debug!("Writing empty buffer to {path:?}");
                            fs::write(&path, &[]).await?;
                            synced += 1;
                        }
                    }
                }

                for entry in walk_dir(&CONFIG.path)? {
                    let path = entry.path();
                    let normalized_path = normalize_path(&path);

                    if !objects.contains(&path) {
                        if SYNCED_PATHS.inner.remove(&normalized_path).is_some() {
                            if path.is_dir() {
                                fs::remove_dir(&path).await?;
                            } else if path.is_file() {
                                fs::remove_file(&path).await?;
                            } else {
                                unreachable!()
                            }
                        } else {
                            log::debug!("{:?} not synced, Uploading...", path);
                            cloud
                                .upload(&normalized_path, &fs::read(&path).await?)
                                .await?;
                            SYNCED_PATHS.inner.insert(normalized_path);
                            synced += 1;
                        }
                    }
                }

                log::debug!("{synced:?} file has synced");

                SYNCED_PATHS.save()?;
                *SYNCING.lock().unwrap() = false;
            } else {
                log::warn!("Skip syncing.. there are no internet connection");
            }

            sleep(Duration::from_millis(CONFIG.interval)).await;
        }

        #[allow(unreachable_code)]
        Result::<()>::Ok(())
    });

    if let Err(err) = tokio::try_join!(fs_task, cloud_task) {
        panic!("An unexpected error occurred: {err:?}")
    } else {
        unreachable!()
    }
}
