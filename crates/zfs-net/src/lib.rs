#![forbid(unsafe_code)]
//! ZFS network abstraction over libp2p.
//!
//! Provides QUIC transport, GossipSub topic subscription, and a
//! request-response protocol for `StoreRequest`/`FetchRequest` exchanges.
//!
//! This is the **only** crate in the workspace that depends on libp2p.
//!
//! # Usage
//!
//! Create a [`NetworkService`] with a [`NetworkConfig`], then drive the event
//! loop by calling [`NetworkService::next_event`] in a loop:
//!
//! ```rust,no_run
//! # async fn example() -> Result<(), zfs_net::NetworkError> {
//! let mut svc = zfs_net::NetworkService::new(Default::default()).await?;
//! svc.subscribe("prog/abc123")?;
//! loop {
//!     if let Some(event) = svc.next_event().await {
//!         // handle event
//!     }
//! }
//! # }
//! ```

mod behaviour;
mod config;
mod error;
mod event;
mod protocol;
mod service;

pub use config::{DiscoveryConfig, KademliaMode, NetworkConfig};
pub use error::NetworkError;
pub use event::NetworkEvent;
pub use protocol::{ZfsRequest, ZfsResponse};
pub use service::NetworkService;

// Re-export libp2p types that appear in the public API so consumers
// do not need a direct libp2p dependency.
pub use libp2p::request_response::{OutboundRequestId, ResponseChannel};
pub use libp2p::{Multiaddr, PeerId};
