use zfs_core::{Cid, Head, ProgramId, SectorId};

use crate::StorageError;

/// Content-addressed block storage (ciphertext keyed by CID).
pub trait BlockStore {
    fn put(&self, cid: &Cid, ciphertext: &[u8]) -> Result<(), StorageError>;
    fn get(&self, cid: &Cid) -> Result<Option<Vec<u8>>, StorageError>;
    fn delete(&self, cid: &Cid) -> Result<(), StorageError>;
}

/// Sector head storage (latest version pointer per sector).
pub trait HeadStore {
    fn put_head(&self, sector_id: &SectorId, head: &Head) -> Result<(), StorageError>;
    fn get_head(&self, sector_id: &SectorId) -> Result<Option<Head>, StorageError>;
    fn list_all_heads(&self) -> Result<Vec<Head>, StorageError>;
}

/// Index from program to CIDs stored for that program.
pub trait ProgramIndex {
    fn add_cid(&self, program_id: &ProgramId, cid: &Cid) -> Result<(), StorageError>;
    fn list_cids(&self, program_id: &ProgramId) -> Result<Vec<Cid>, StorageError>;
    fn remove_cid(&self, program_id: &ProgramId, cid: &Cid) -> Result<(), StorageError>;
}

/// Aggregate storage backend combining all stores with statistics.
pub trait StorageBackend: BlockStore + HeadStore + ProgramIndex {
    fn stats(&self) -> Result<StorageStats, StorageError>;
}

/// Storage statistics for monitoring and policy enforcement.
#[derive(Debug, Clone, Default)]
pub struct StorageStats {
    /// Approximate total database size in bytes.
    pub db_size_bytes: u64,
    /// Total number of blocks stored.
    pub block_count: u64,
    /// Total number of tracked sector heads.
    pub head_count: u64,
    /// Total number of programs in the index.
    pub program_count: u64,
}
