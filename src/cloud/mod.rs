mod adapter;
mod providers;

use crate::config::CloudOptions;
pub use adapter::*;
use providers::*;

pub fn cloud_storage(options: CloudOptions) -> impl CloudAdapter {
    match options {
        CloudOptions::S3 { .. } => S3Storage::new(options),
        // TODO: Support more providers
        // CloudOptions::GoogleDrive { .. } => GoogleDrive:new(options),
        // CloudOptions::Dropbox { .. } => Dropbox::new(options),
        // CloudOptions::Mega { .. } => Mega:new(options),
        // CloudOptions::OneDrive { .. } => OneDrive::new(options),
        // CloudOptions::ProtonDrive { .. } => ProtonDrive::new(options),
        // _ => unimplemented!(),
    }
}
