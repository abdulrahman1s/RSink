#[macro_use]
extern crate lazy_static;
extern crate notify;

mod cloud;
mod util;

use cloud::{adapters::*, CloudAdapter};
use config::Config;
use notify::{event::*, EventKind, RecursiveMode, Watcher};
use std::{
    collections::HashSet,
    path::PathBuf,
    sync::{mpsc::channel, Arc, Mutex},
    thread,
    time::Duration,
};

use util::*;

lazy_static! {
    pub static ref SETTINGS: Config = Config::builder()
        .add_source(config::File::with_name("config"))
        .build()
        .expect("Cannot init settings");
    pub static ref SYNC_DIR: PathBuf = SETTINGS.get_string("main.path").expect("Missing main.path env").parse().unwrap();
}

fn main() -> Result<()> {
    let cloud_ref = Arc::new(s3::Cloud::new());
    let syncing_ref = Arc::new(Mutex::new(false));
    let interval: u64 = SETTINGS
        .get("main.interval")
        .expect("Missing/Invalid interval value");

    let cloud = cloud_ref.clone();
    let syncing = syncing_ref.clone();

    let online_task = thread::spawn(move || loop {
        *syncing.lock().unwrap() = true;
        spinner("Syncing...", "Synced!", || cloud.sync().map(|_| {}));
        *syncing.lock().unwrap() = false;
        thread::sleep(Duration::from_millis(interval));
    });

    let cloud = cloud_ref;
    let syncing = syncing_ref;
    let local_task = thread::spawn(move || {
        let (tx, rx) = channel();
        let mut watcher = notify::recommended_watcher(tx)?;

        watcher.watch(&SYNC_DIR, RecursiveMode::Recursive)?;

        let mut changes = HashSet::<PathBuf>::new();

        while let Ok(Ok(event)) = rx.recv() {
            if *syncing.lock().unwrap() {
                continue;
            }

            match event.kind {
                EventKind::Create(CreateKind::File) => {
                    spinner("Saveing file...", "File saved", || {
                        cloud.save(&event.paths[0])
                    });
                }
                EventKind::Remove(RemoveKind::File | RemoveKind::Folder) => {
                    spinner("Deleteing file...", "File deleted", || {
                        cloud.delete(&event.paths[0])
                    });
                }
                EventKind::Access(AccessKind::Close(AccessMode::Write)) => {
                    if changes.remove(&event.paths[0]) {
                        spinner("Changeing file...", "File changed", || {
                            cloud.save(&event.paths[0])
                        });
                    }
                }
                EventKind::Modify(kind) => match kind {
                    ModifyKind::Data(_) => {
                        changes.insert(event.paths[0].clone());
                    }
                    ModifyKind::Name(_) => {
                        if event.paths.len() == 2 {
                            spinner("Renaming file...", "File renamed", || {
                                cloud.rename(&event.paths[0], &event.paths[1])
                            });
                        }
                    }
                    _ => {}
                },
                _ => {}
            }
        }

        Result::<()>::Ok(())
    });

    for task in [online_task, local_task] {
        task.join().unwrap()?;
    }

    Ok(())
}
