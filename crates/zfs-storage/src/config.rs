use std::path::PathBuf;

/// RocksDB compression algorithm selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompressionType {
    None,
    Lz4,
    Zstd,
}

/// Configuration for the ZFS storage backend.
#[derive(Debug, Clone)]
pub struct StorageConfig {
    /// Filesystem path for the RocksDB database.
    pub path: PathBuf,
    /// Maximum number of open file descriptors for RocksDB.
    pub max_open_files: Option<i32>,
    /// Compression algorithm for block and head column families.
    pub compression: CompressionType,
    /// Optional global size limit (bytes). Enforced at the application layer.
    pub max_db_size_bytes: Option<u64>,
}

impl StorageConfig {
    /// Create a config with sensible defaults for a given path.
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            max_open_files: Some(512),
            compression: CompressionType::Lz4,
            max_db_size_bytes: None,
        }
    }
}
