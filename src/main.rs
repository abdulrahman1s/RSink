#[macro_use]
extern crate lazy_static;
extern crate notify;

mod cloud;
mod util;

use cloud::{adapters::*, CloudAdapter};
use config::Config;
use notify::{event::*, recommended_watcher, RecursiveMode, Watcher};
use std::{
    collections::HashSet,
    path::PathBuf,
    process::exit,
    sync::{mpsc::channel, Arc, Mutex},
    thread,
    time::Duration,
};

use util::*;

lazy_static! {
    pub static ref SETTINGS: Config = Config::builder()
        .add_source(config::File::from(
            settings_file_path().expect("Couldn't retrieve settings file path")
        ))
        .build()
        .unwrap();
    pub static ref SYNC_DIR: PathBuf = SETTINGS
        .get_string("main.path")
        .expect("Missing syncing path. Please fill main.path in the settings file")
        .parse()
        .expect("Invalid path string");
    pub static ref IS_INTERNET_AVAILABLE: Mutex<bool> = Mutex::new(false);
    pub static ref SYNCING: Mutex<bool> = Mutex::new(false);
}

fn main() -> Result<()> {
    let cloud_ref = Arc::new(
        match SETTINGS
            .get_string("main.provider")
            .expect("Missing cloud provider in config/settings")
            .replace('_', "")
            .replace('-', "")
            .as_str()
        {
            "s3" => s3::Cloud::new(),
            // TODO: Support more cloud
            // "googledrive" | "gdrive" => googledrive::Cloud::new(),
            // "dropbox" => dropbox::Cloud::new(),
            // "mega" => mega::Cloud::new(),
            // "onedrive" => onedrive::Cloud::new(),
            // "protondrive" => protondrive::Cloud::new(),
            x => {
                println!("Unspported cloud provider: {}", x);
                exit(1);
            }
        },
    );

    let interval: u64 = SETTINGS
        .get("main.interval")
        .expect("Missing/Invalid interval value");

    let cloud = cloud_ref.clone();

    let online_task = thread::spawn(move || loop {
        check_connectivity();

        if *IS_INTERNET_AVAILABLE.lock().unwrap() {
            *SYNCING.lock().unwrap() = true;
            spinner("Syncing...", "Synced!", || cloud.sync().map(|_| {}));
            *SYNCING.lock().unwrap() = false;
            thread::sleep(Duration::from_millis(interval));
        } else {
            println!("Skip syncing.. there are no internet connection");
        }
    });

    let cloud = cloud_ref;
    let local_task = thread::spawn(move || {
        let (tx, rx) = channel();
        let mut watcher = recommended_watcher(tx)?;
        let mut changes = HashSet::new();

        watcher.watch(&SYNC_DIR, RecursiveMode::Recursive)?;

        while let Ok(event) = rx.recv() {
            let event = match event {
                Ok(e) => e,
                Err(err) => {
                    println!("Notify Error: {:?}", err);
                    continue;
                }
            };

            println!("{:?}", event);

            if !*IS_INTERNET_AVAILABLE.lock().unwrap() {
                println!("Skip local syncing.. there are no internet connection");
                continue;
            }

            if *SYNCING.lock().unwrap() {
                println!("Ignore event since online syncing is working");
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
