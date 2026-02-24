#![forbid(unsafe_code)]
//! ZFS client SDK — identity, connect, encrypt, sign, upload, fetch.
//!
//! Wraps `zero-neural`, `zfs-net`, `zfs-crypto`, `zfs-programs`, and
//! `zfs-proof` into a unified client API. Does **not** use RocksDB.
//!
//! # Quick start
//!
//! ```rust,no_run
//! # async fn example() -> Result<(), zfs_sdk::SdkError> {
//! use zfs_sdk::{SdkConfig, Client};
//!
//! let client = Client::connect(&SdkConfig::default()).await?;
//! // ... generate keys, encrypt, upload, fetch ...
//! # Ok(())
//! # }
//! ```

mod client;
mod error;
mod helpers;
mod identity;
mod upload;

pub use client::{Client, SdkConfig};
pub use error::SdkError;
pub use helpers::{zchat_descriptor, zid_descriptor};
pub use identity::{derive_identity_signing_key, derive_machine_keypair};
pub use upload::{fetch, fetch_head, upload, FetchResult, StoreResult};

// Re-export frequently used types so callers don't need extra deps.
pub use zero_neural::{
    HybridSignature, IdentitySigningKey, MachineKeyCapabilities, MachineKeyPair, MachinePublicKey,
    NeuralKey,
};
pub use zfs_core::{Cid, Head, ProgramId, SectorId};
pub use zfs_crypto::{decrypt_sector, encrypt_sector, SectorKey};
pub use zfs_programs::{program_topic, ZChatDescriptor, ZChatMessage, ZidDescriptor, ZidMessage};
