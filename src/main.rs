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
use cloud::{cloud_storage, CloudAdapter};
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

    let cloud_ref = Arc::new(cloud_storage(CONFIG.cloud.clone()));

    log::info!("Selected cloud provider: {}", cloud_ref.kind());
    log::info!("Syncing directory: {:?}", CONFIG.path);
    log::info!("Syncing delay: {}ms", CONFIG.interval);

    let cloud = cloud_ref.clone();
    let fs_task = spawn(async move {
        let (tx, mut rx) = channel(5);
        let mut watcher = recommended_watcher(move |event| {
            futures::executor::block_on(async { tx.send(event).await.unwrap() });
        })?;
        let changes = Arc::new(DashSet::new());

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

            spawn(async move {
                let is_file_exists = || async {
                    fs::metadata(&event.paths[0])
                        .await
                        .map(|m| m.is_file())
                        .unwrap_or(false)
                };

                match event.kind {
                    EventKind::Create(_) if is_file_exists().await => {
                        cloud
                            .save(&event.paths[0])
                            .await
                            .and_then(|_| {
                                if SYNCED_PATHS.0.insert(stringify_path(&event.paths[0])) {
                                    SYNCED_PATHS.save()?;
                                }
                                Ok(())
                            })
                            .or_else(log_error)?;
                    }

                    EventKind::Remove(_) => {
                        cloud
                            .delete(&event.paths[0])
                            .await
                            .and_then(|_| {
                                if SYNCED_PATHS
                                    .0
                                    .remove(&stringify_path(&event.paths[0]))
                                    .is_some()
                                {
                                    SYNCED_PATHS.save()?;
                                }
                                Ok(())
                            })
                            .or_else(log_error)?;
                    }
                    EventKind::Access(AccessKind::Close(AccessMode::Write)) => {
                        if changes.remove(&event.paths[0]).is_some() && is_file_exists().await {
                            cloud.save(&event.paths[0]).await.or_else(log_error)?;
                        }
                    }
                    EventKind::Modify(kind) => match kind {
                        ModifyKind::Data(_) => {
                            changes.insert(event.paths[0].clone());
                        }
                        ModifyKind::Name(_) if event.paths.len() == 2 => {
                            cloud
                                .rename(&event.paths[0], &event.paths[1])
                                .await
                                .and_then(|_| {
                                    SYNCED_PATHS.0.remove(&stringify_path(&event.paths[0]));
                                    SYNCED_PATHS.0.insert(stringify_path(&event.paths[1]));
                                    SYNCED_PATHS.save()?;
                                    Ok(())
                                })
                                .or_else(log_error)?;
                        }
                        #[cfg(target_os = "android")]
                        ModifyKind::Metadata(MetadataKind::WriteTime) if is_file_exists().await => {
                            cloud.save(&event.paths[0]).await.or_else(log_error)?;
                        }
                        _ => {}
                    },
                    _ => {}
                }

                Result::<()>::Ok(())
            });
        }

        Result::<()>::Ok(())
    });

    let cloud = cloud_ref;
    let cloud_task = spawn(async move {
        loop {
            check_connectivity().await;

            if *IS_INTERNET_AVAILABLE.lock().unwrap() {
                *SYNCING.lock().unwrap() = true;

                cloud
                    .sync()
                    .await
                    .map(|count| log::debug!("{count:?} file has synced"))
                    .or_else(log_error)?;

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
