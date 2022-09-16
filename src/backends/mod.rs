pub mod interface;
#[path = "./s3/s3.rs"]
pub mod s3;
pub use interface::*;

pub async fn init_backend(options: BackendOptions) -> impl Backend {
    match options {
        BackendOptions::S3(_) => s3::S3::init(options).await,
        // _ => unreachable!()
    }
}
