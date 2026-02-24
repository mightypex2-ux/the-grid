#![forbid(unsafe_code)]
//! ZFS Zode node — ties together storage, network, proof, and programs.
//!
//! The Zode is the storage node: it runs libp2p + QUIC via `zfs-net`,
//! subscribes to program topics via GossipSub, persists blocks and heads
//! in RocksDB via `zfs-storage`, verifies proofs when required, and
//! enforces local storage policy.
//!
//! # Usage
//!
//! ```rust,no_run
//! # async fn example() -> Result<(), zfs_zode::ZodeError> {
//! use zfs_zode::{Zode, ZodeConfig};
//!
//! let config = ZodeConfig::default();
//! let zode = Zode::start(config).await?;
//! let status = zode.status().await;
//! println!("peer count: {}", status.peer_count);
//! zode.shutdown().await;
//! # Ok(())
//! # }
//! ```

mod config;
mod error;
mod handler;
mod metrics;
mod zode;

pub use config::{DefaultProgramsConfig, LimitsConfig, ProofPolicyConfig, ZodeConfig};
pub use error::ZodeError;
pub use metrics::{MetricsSnapshot, ZodeMetrics};
pub use zode::{LogEvent, Zode, ZodeStatus};
