use rocksdb::{ColumnFamilyDescriptor, Options, DB};

use crate::config::CompressionType;
use crate::error::StorageError;
use crate::traits::{BlockStore, HeadStore, ProgramIndex, StorageBackend, StorageStats};
use crate::StorageConfig;
use zfs_core::{Cid, Head, ProgramId, SectorId};

const CF_BLOCKS: &str = "blocks";
const CF_HEADS: &str = "heads";
const CF_PROGRAM_INDEX: &str = "program_index";
const CF_METADATA: &str = "metadata";

/// RocksDB-backed storage implementation.
///
/// Provides all four storage responsibilities via column families:
/// `blocks`, `heads`, `program_index`, and `metadata`.
pub struct RocksStorage {
    db: DB,
    #[allow(dead_code)]
    config: StorageConfig,
}

impl RocksStorage {
    /// Open (or create) a RocksDB database with the given config.
    pub fn open(config: StorageConfig) -> Result<Self, StorageError> {
        let mut db_opts = Options::default();
        db_opts.create_if_missing(true);
        db_opts.create_missing_column_families(true);

        if let Some(max_files) = config.max_open_files {
            db_opts.set_max_open_files(max_files);
        }

        let compression = match config.compression {
            CompressionType::None => rocksdb::DBCompressionType::None,
            CompressionType::Lz4 => rocksdb::DBCompressionType::Lz4,
            CompressionType::Zstd => rocksdb::DBCompressionType::Zstd,
        };

        let cf_names = [CF_BLOCKS, CF_HEADS, CF_PROGRAM_INDEX, CF_METADATA];
        let cf_descriptors: Vec<ColumnFamilyDescriptor> = cf_names
            .iter()
            .map(|name| {
                let mut cf_opts = Options::default();
                cf_opts.set_compression_type(compression);
                ColumnFamilyDescriptor::new(*name, cf_opts)
            })
            .collect();

        let db = DB::open_cf_descriptors(&db_opts, &config.path, cf_descriptors)?;
        Ok(Self { db, config })
    }

    fn cf_handle(&self, name: &str) -> Result<&rocksdb::ColumnFamily, StorageError> {
        self.db
            .cf_handle(name)
            .ok_or_else(|| StorageError::CfNotFound(name.to_owned()))
    }
}

impl BlockStore for RocksStorage {
    fn put(&self, cid: &Cid, ciphertext: &[u8]) -> Result<(), StorageError> {
        let cf = self.cf_handle(CF_BLOCKS)?;
        self.db.put_cf(cf, cid.as_bytes(), ciphertext)?;
        Ok(())
    }

    fn get(&self, cid: &Cid) -> Result<Option<Vec<u8>>, StorageError> {
        let cf = self.cf_handle(CF_BLOCKS)?;
        Ok(self.db.get_cf(cf, cid.as_bytes())?)
    }

    fn delete(&self, cid: &Cid) -> Result<(), StorageError> {
        let cf = self.cf_handle(CF_BLOCKS)?;
        self.db.delete_cf(cf, cid.as_bytes())?;
        Ok(())
    }
}

impl HeadStore for RocksStorage {
    fn put_head(&self, sector_id: &SectorId, head: &Head) -> Result<(), StorageError> {
        let cf = self.cf_handle(CF_HEADS)?;
        let value =
            zfs_core::encode_canonical(head).map_err(|e| StorageError::Encode(e.to_string()))?;
        self.db.put_cf(cf, sector_id.as_bytes(), value)?;
        Ok(())
    }

    fn get_head(&self, sector_id: &SectorId) -> Result<Option<Head>, StorageError> {
        let cf = self.cf_handle(CF_HEADS)?;
        match self.db.get_cf(cf, sector_id.as_bytes())? {
            Some(bytes) => {
                let head: Head = zfs_core::decode_canonical(&bytes)
                    .map_err(|e| StorageError::Decode(e.to_string()))?;
                Ok(Some(head))
            }
            None => Ok(None),
        }
    }

    fn list_all_heads(&self) -> Result<Vec<Head>, StorageError> {
        let cf = self.cf_handle(CF_HEADS)?;
        let mut heads = Vec::new();
        let iter = self.db.iterator_cf(cf, rocksdb::IteratorMode::Start);
        for item in iter {
            let (_key, value) = item?;
            let head: Head = zfs_core::decode_canonical(&value)
                .map_err(|e| StorageError::Decode(e.to_string()))?;
            heads.push(head);
        }
        Ok(heads)
    }
}

impl ProgramIndex for RocksStorage {
    fn add_cid(&self, program_id: &ProgramId, cid: &Cid) -> Result<(), StorageError> {
        let cf = self.cf_handle(CF_PROGRAM_INDEX)?;
        let mut cids = self.load_cid_list(program_id)?;
        if !cids.iter().any(|c| c == cid) {
            cids.push(*cid);
            self.save_cid_list(program_id, &cids)?;
        }
        self.db.flush_cf(cf)?;
        Ok(())
    }

    fn list_cids(&self, program_id: &ProgramId) -> Result<Vec<Cid>, StorageError> {
        self.load_cid_list(program_id)
    }

    fn remove_cid(&self, program_id: &ProgramId, cid: &Cid) -> Result<(), StorageError> {
        let mut cids = self.load_cid_list(program_id)?;
        cids.retain(|c| c != cid);
        self.save_cid_list(program_id, &cids)?;
        Ok(())
    }
}

impl RocksStorage {
    fn load_cid_list(&self, program_id: &ProgramId) -> Result<Vec<Cid>, StorageError> {
        let cf = self.cf_handle(CF_PROGRAM_INDEX)?;
        match self.db.get_cf(cf, program_id.as_bytes())? {
            Some(bytes) => {
                let raw: Vec<serde_bytes::ByteBuf> = zfs_core::decode_canonical(&bytes)
                    .map_err(|e| StorageError::Decode(e.to_string()))?;
                let cids =
                    raw.into_iter()
                        .map(|buf: serde_bytes::ByteBuf| {
                            let arr: [u8; 32] = buf.as_ref().try_into().map_err(|_| {
                                StorageError::Decode("CID must be 32 bytes".to_owned())
                            })?;
                            Ok(Cid::from(arr))
                        })
                        .collect::<Result<Vec<_>, StorageError>>()?;
                Ok(cids)
            }
            None => Ok(Vec::new()),
        }
    }

    fn save_cid_list(&self, program_id: &ProgramId, cids: &[Cid]) -> Result<(), StorageError> {
        let cf = self.cf_handle(CF_PROGRAM_INDEX)?;
        let raw: Vec<serde_bytes::ByteBuf> = cids
            .iter()
            .map(|c| serde_bytes::ByteBuf::from(c.as_bytes().to_vec()))
            .collect();
        let encoded =
            zfs_core::encode_canonical(&raw).map_err(|e| StorageError::Encode(e.to_string()))?;
        self.db.put_cf(cf, program_id.as_bytes(), encoded)?;
        Ok(())
    }
}

impl StorageBackend for RocksStorage {
    fn stats(&self) -> Result<StorageStats, StorageError> {
        let db_size_bytes = self
            .db
            .property_int_value("rocksdb.estimate-live-data-size")
            .ok()
            .flatten()
            .unwrap_or(0);

        let block_count = self.count_keys(CF_BLOCKS)?;
        let head_count = self.count_keys(CF_HEADS)?;
        let program_count = self.count_keys(CF_PROGRAM_INDEX)?;

        Ok(StorageStats {
            db_size_bytes,
            block_count,
            head_count,
            program_count,
        })
    }
}

impl RocksStorage {
    fn count_keys(&self, cf_name: &str) -> Result<u64, StorageError> {
        let cf = self.cf_handle(cf_name)?;
        let mut count = 0u64;
        let iter = self.db.iterator_cf(cf, rocksdb::IteratorMode::Start);
        for item in iter {
            let _ = item?;
            count += 1;
        }
        Ok(count)
    }
}
