#[macro_use]
extern crate lazy_static;
#[macro_use]
extern crate async_trait;
extern crate notify;

mod cache;
mod cloud;
mod config;
mod util;

use cache::*;
use cloud::{select_provider, CloudAdapter, Operation};
use config::*;
use dashmap::DashSet;
use log::LevelFilter;
use notify::{event::*, recommended_watcher, RecursiveMode, Watcher};
use std::{
    str::FromStr,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::{fs, spawn, sync::mpsc::channel, time::sleep};
use util::*;

lazy_static! {
    pub static ref IS_INTERNET_AVAILABLE: Mutex<bool> = Mutex::new(false);
    pub static ref SYNCING: Mutex<bool> = Mutex::new(false);
    pub static ref SYNCED_PATHS: Cache = Cache::new();
}

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::builder()
        .format_timestamp(None)
        .filter_level(LevelFilter::from_str(&CONFIG.log).expect("Invalid log level format"))
        .init();

    let cloud_ref = Arc::new(select_provider(CONFIG.cloud.clone()).await);

    log::info!("Selected cloud provider: {}", cloud_ref.kind());
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
                Ok(e) => e,
                Err(err) => {
                    log::error!("Notify Error: {:?}", err);
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
                let is_file_exists = || async {
                    log::debug!("Checking {:?} metadata", path);

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
                        log::debug!("Saving {:?}", path);

                        cloud
                            .save(path, &fs::read(path).await?)
                            .await
                            .and_then(|_| {
                                if SYNCED_PATHS.0.insert(stringify_path(path)) {
                                    SYNCED_PATHS.save()?;
                                }
                                Ok(())
                            })
                            .or_else(log_error)?;
                    }
                    EventKind::Remove(_) => {
                        log::debug!("Deleting {:?}", path);

                        cloud
                            .delete(path)
                            .await
                            .and_then(|_| {
                                if SYNCED_PATHS.0.remove(&stringify_path(path)).is_some() {
                                    SYNCED_PATHS.save()?;
                                }
                                Ok(())
                            })
                            .or_else(log_error)?;
                    }
                    EventKind::Access(AccessKind::Close(AccessMode::Write)) => {
                        if changes.remove(path).is_some() && is_file_exists().await {
                            log::debug!("Updating {:?}", path);
                            cloud
                                .save(path, &fs::read(path).await?)
                                .await
                                .or_else(log_error)?;
                        }
                    }
                    EventKind::Modify(kind) => match kind {
                        ModifyKind::Data(_) => {
                            if SYNCED_PATHS.0.contains(&stringify_path(path)) {
                                changes.insert(path.clone());
                            }
                        }
                        ModifyKind::Name(_) if event.paths.len() == 2 => {
                            log::debug!("Moving from {:?} to {:?}", path, event.paths[1]);

                            if SYNCED_PATHS.0.remove(&stringify_path(path)).is_none() {
                                cloud
                                    .save(&event.paths[1], &fs::read(&event.paths[1]).await?)
                                    .await
                                    .and_then(|_| {
                                        SYNCED_PATHS.0.insert(stringify_path(&event.paths[1]));
                                        SYNCED_PATHS.save()
                                    })
                                    .or_else(log_error)?;
                            } else {
                                cloud
                                    .rename(path, &event.paths[1])
                                    .await
                                    .and_then(|_| {
                                        SYNCED_PATHS.0.remove(&stringify_path(path));
                                        SYNCED_PATHS.0.insert(stringify_path(&event.paths[1]));
                                        SYNCED_PATHS.save()
                                    })
                                    .or_else(log_error)?;
                            }
                        }
                        #[cfg(target_os = "android")]
                        ModifyKind::Metadata(MetadataKind::WriteTime) if is_file_exists().await => {
                            log::debug!("Saving {:?}", path);
                            cloud
                                .save(path, &fs::read(path).await?)
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

                let (objects, operations) = cloud.sync().await?;
                let mut synced = 0;

                log::debug!("Sync operations: {}", operations.len());

                for op in operations {
                    match op {
                        Operation::Save(path) => {
                            log::debug!("Saving {path:?}");
                            cloud.save(&path, &fs::read(&path).await?).await?;
                            synced += 1;
                        }
                        Operation::Write(path) => {
                            let buffer = cloud.get(&path).await?;
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
                            cloud.save(&path, &fs::read(&path).await?).await?;
                            SYNCED_PATHS.0.insert(stringify_path(&path));
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

    match tokio::try_join!(fs_task, cloud_task) {
        Err(err) => panic!("An unexpected error occurred: {err:?}"),
        _ => unreachable!(),
    }
}
