#![forbid(unsafe_code)]

mod config;
mod error;
mod rocks;
mod traits;

pub use config::{CompressionType, StorageConfig};
pub use error::StorageError;
pub use rocks::RocksStorage;
pub use traits::{BlockStore, HeadStore, ProgramIndex, StorageBackend, StorageStats};

#[cfg(test)]
mod tests;
