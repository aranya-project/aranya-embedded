pub(crate) mod internal;
pub(crate) mod sd;

#[cfg(not(any(feature = "storage-internal", feature = "storage-sd")))]
compile_error!("One of \"storage-internal\" or \"storage-sd\" must be enabled");

#[cfg(all(feature = "storage-internal", feature = "storage-sd"))]
compile_error!("Only one of 'storage-internal' or 'storage-sd' can be enabled");

#[cfg(feature = "storage-sd")]
pub mod imp {
    pub use super::sd::GraphManager;
}

#[cfg(feature = "storage-internal")]
pub mod imp {
    pub use esp_storage::FlashStorage;
    pub use super::internal::EspPartitionIoManager;
}

use thiserror::Error;

#[derive(Debug, Error)]
pub enum StorageError {
    #[error("no data partition found")]
    NoDataPartitionFound,
    #[error("bad header")]
    BadHeader,
    #[error("serialization: {0}")]
    Serialization(#[from] rkyv::rancor::Error),
    #[error("write")]
    Write,
    #[error("Aranya error")]
    AranyaError(#[from] aranya_runtime::storage::StorageError),
}
