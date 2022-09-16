use crate::backends::*;
use crate::util::settings_file_path;
use figment::{
    providers::{Format, Toml},
    Figment,
};
use notify_rust::Notification;
use serde::Deserialize;
use std::path::PathBuf;

fn default_interval() -> u64 {
    180000
}

fn default_log_level() -> String {
    "info".to_owned()
}

#[derive(Deserialize)]
pub struct Config {
    pub path: PathBuf,
    #[serde(default = "default_log_level")]
    pub log: String,
    #[serde(default = "default_interval")]
    pub interval: u64,
    pub backend: BackendOptions,
}

lazy_static! {
    pub static ref CONFIG: Config = Figment::new()
        .merge(Toml::file(settings_file_path()))
        .merge(Toml::file("rsink.conf"))
        .extract()
        .unwrap_or_else(|err| {
            Notification::new()
                .summary("RSink")
                .body("Please configure correctly the missing settings for RSink to work")
                .icon("dialog-error")
                .show()
                .ok();
            panic!("Missing/Invalid configuration: {err}");
        });
}
