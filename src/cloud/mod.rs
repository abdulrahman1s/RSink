mod adapter;
mod providers;

use crate::config::CloudOptions;
pub use adapter::*;
use providers::*;

pub async fn select_provider(options: CloudOptions) -> impl CloudAdapter {
    match options {
        CloudOptions::S3 { .. } => S3Storage::init(options).await,
        // TODO: Support more providers
        // CloudOptions::GoogleDrive { .. } => GoogleDrive:new(options),
        // CloudOptions::Dropbox { .. } => Dropbox::new(options),
        // CloudOptions::Mega { .. } => Mega:new(options),
        // CloudOptions::OneDrive { .. } => OneDrive::new(options),
        // CloudOptions::ProtonDrive { .. } => ProtonDrive::new(options),
        // _ => unimplemented!(),
    }
}
