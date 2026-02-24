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
pub use libp2p::Multiaddr;

/// A Zode's identity on the network (wraps libp2p `PeerId`).
pub type ZodeId = libp2p::PeerId;

/// The human-readable prefix for Zode addresses.
const ZODE_ID_PREFIX: &str = "Zx";

/// Format a [`ZodeId`] for display with the canonical `Zx` prefix.
///
/// On the wire and in storage the raw `PeerId` bytes are used; this
/// prefix is **display-only**.
pub fn format_zode_id(id: &ZodeId) -> String {
    format!("{ZODE_ID_PREFIX}{id}")
}

/// Parse a `Zx`-prefixed Zode address back into a [`ZodeId`].
///
/// Accepts both `Zx`-prefixed and bare `PeerId` strings for
/// backwards-compatibility.
pub fn parse_zode_id(s: &str) -> Result<ZodeId, libp2p::identity::ParseError> {
    let raw = s.strip_prefix(ZODE_ID_PREFIX).unwrap_or(s);
    raw.parse()
}
