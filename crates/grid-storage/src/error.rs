use thiserror::Error;

/// Errors from the Grid storage layer.
#[derive(Debug, Error)]
pub enum StorageError {
    #[error("rocksdb error: {0}")]
    RocksDb(#[from] rocksdb::Error),

    #[error("encode error: {0}")]
    Encode(String),

    #[error("decode error: {0}")]
    Decode(String),

    #[error("storage full: {reason}")]
    Full { reason: String },

    #[error("column family not found: {0}")]
    CfNotFound(String),

    #[error("batch too large: {0}")]
    BatchTooLarge(String),
}

impl From<StorageError> for grid_core::GridError {
    fn from(e: StorageError) -> Self {
        match e {
            StorageError::Full { .. } => grid_core::GridError::StorageFull,
            StorageError::Encode(msg) => grid_core::GridError::Encode(msg),
            StorageError::Decode(msg) => grid_core::GridError::Decode(msg),
            StorageError::BatchTooLarge(msg) => grid_core::GridError::InvalidPayload(msg),
            other => grid_core::GridError::Other(other.to_string()),
        }
    }
}
