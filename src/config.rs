use crate::util::settings_file_path;
use figment::{
    providers::{Format, Json, Toml},
    Figment,
};
use notify_rust::Notification;
use serde::Deserialize;
use std::path::PathBuf;

#[derive(Deserialize, Clone)]
#[serde(tag = "provider", rename_all = "snake_case")]
pub enum CloudOptions {
    S3 {
        bucket_name: String,
        key: String,
        secret: String,
        endpoint: Option<String>,
        region: Option<String>,
    },
}

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
    pub cloud: CloudOptions,
}

lazy_static! {
    pub static ref CONFIG: Config = Figment::new()
        .merge(Toml::file(settings_file_path("toml")))
        .merge(Json::file(settings_file_path("json")))
        .merge(Toml::file("config.toml"))
        .merge(Json::file("config.json"))
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
