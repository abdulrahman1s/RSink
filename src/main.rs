#[macro_use]
extern crate lazy_static;
extern crate notify;

mod cache;
mod cloud;
mod config;
mod util;

use cache::*;
use cloud::{cloud_storage, CloudAdapter};
use config::*;
use log::LevelFilter;
use notify::{event::*, recommended_watcher, RecursiveMode, Watcher};
use std::{
    collections::HashSet,
    fs,
    str::FromStr,
    sync::{mpsc::channel, Arc, Mutex},
    thread,
    time::Duration,
};
use util::*;

lazy_static! {
    pub static ref IS_INTERNET_AVAILABLE: Mutex<bool> = Mutex::new(false);
    pub static ref SYNCING: Mutex<bool> = Mutex::new(false);
    pub static ref SYNCED_PATHS: Cache = Cache::new();
}

fn main() -> Result<()> {
    env_logger::builder()
        .format_timestamp(None)
        .filter_level(
            LevelFilter::from_str(&CONFIG.log.to_uppercase()).expect("Invalid log level format"),
        )
        .init();

    let cloud_ref = Arc::new(cloud_storage(CONFIG.cloud.clone()));

    log::info!("Selected cloud provider: {}", cloud_ref.kind());
    log::info!("Syncing directory: {:?}", CONFIG.path);
    log::info!("Syncing delay: {}ms", CONFIG.interval);

    let cloud = cloud_ref.clone();
    let fs_task = thread::spawn(move || {
        let (tx, rx) = channel();
        let mut watcher = recommended_watcher(tx)?;
        let mut changes = HashSet::new();

        watcher.watch(&CONFIG.path, RecursiveMode::Recursive)?;

        while let Ok(event) = rx.recv() {
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

            match event.kind {
                EventKind::Create(_) if !fs::metadata(&event.paths[0])?.is_file() => {
                    maybe_error(cloud.save(&event.paths[0]).and_then(|_| {
                        if SYNCED_PATHS.0.insert(stringify_path(&event.paths[0])) {
                            SYNCED_PATHS.save()?;
                        }
                        Ok(())
                    }))
                }
                EventKind::Remove(_) => maybe_error(cloud.delete(&event.paths[0]).and_then(|_| {
                    if SYNCED_PATHS
                        .0
                        .remove(&stringify_path(&event.paths[0]))
                        .is_some()
                    {
                        SYNCED_PATHS.save()?;
                    }
                    Ok(())
                })),
                EventKind::Access(AccessKind::Close(AccessMode::Write)) => {
                    if changes.remove(&event.paths[0]) {
                        maybe_error(cloud.save(&event.paths[0]));
                    }
                }
                EventKind::Modify(kind) => match kind {
                    ModifyKind::Data(_) => {
                        changes.insert(event.paths[0].clone());
                    }
                    ModifyKind::Name(_) => {
                        if event.paths.len() == 2 {
                            maybe_error(cloud.rename(&event.paths[0], &event.paths[1]).and_then(
                                |_| {
                                    SYNCED_PATHS.0.remove(&stringify_path(&event.paths[0]));
                                    SYNCED_PATHS.0.insert(stringify_path(&event.paths[1]));
                                    SYNCED_PATHS.save()?;
                                    Ok(())
                                },
                            ));
                        }
                    }
                    #[cfg(target_os = "android")]
                    ModifyKind::Metadata(MetadataKind::WriteTime) if event.paths[0].is_file() => {
                        maybe_error(cloud.save(&event.paths[0]));
                    }
                    _ => {}
                },
                _ => {}
            }
        }

        Result::<()>::Ok(())
    });

    let cloud = cloud_ref;
    let cloud_task = thread::spawn(move || loop {
        check_connectivity();

        if *IS_INTERNET_AVAILABLE.lock().unwrap() {
            *SYNCING.lock().unwrap() = true;

            maybe_error(
                cloud
                    .sync()
                    .map(|count| log::debug!("{count:?} file has synced")),
            );

            *SYNCING.lock().unwrap() = false;

            thread::sleep(Duration::from_millis(CONFIG.interval));
        } else {
            log::warn!("Skip syncing.. there are no internet connection");
        }
    });

    for task in [fs_task, cloud_task] {
        task.join().unwrap()?;
    }

    Ok(())
}
