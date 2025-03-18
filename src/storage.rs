pub mod internal;
pub mod sd;

#[cfg(not(any(feature = "storage-internal", feature = "storage-sd")))]
compile_error!("One of \"storage-internal\" or \"storage-sd\" must be enabled");

#[cfg(all(feature = "storage-internal", feature = "storage-sd"))]
compile_error!("Only one of 'storage-internal' or 'storage-sd' can be enabled");

use thiserror::Error;

#[derive(Debug, Error)]
pub enum StorageError {
    #[error("no data partition found")]
    NoDataPartitionFound,
    #[error("Aranya error")]
    AranyaError(#[from] aranya_runtime::storage::StorageError),
}
