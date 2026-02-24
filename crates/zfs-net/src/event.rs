use libp2p::request_response::{OutboundRequestId, ResponseChannel};
use libp2p::{Multiaddr, PeerId};
use zfs_core::{FetchRequest, FetchResponse, StoreRequest, StoreResponse};

use crate::protocol::ZfsResponse;

/// Events produced by the [`NetworkService`](crate::NetworkService) event loop.
pub enum NetworkEvent {
    /// A new peer connection was established.
    PeerConnected(PeerId),

    /// A peer connection was closed.
    PeerDisconnected(PeerId),

    /// A new peer was discovered via DHT or mDNS (not yet necessarily connected).
    PeerDiscovered {
        peer_id: PeerId,
        addresses: Vec<Multiaddr>,
    },

    /// An incoming store request from a remote peer.
    IncomingStore {
        peer: PeerId,
        request: Box<StoreRequest>,
        channel: ResponseChannel<ZfsResponse>,
    },

    /// An incoming fetch request from a remote peer.
    IncomingFetch {
        peer: PeerId,
        request: FetchRequest,
        channel: ResponseChannel<ZfsResponse>,
    },

    /// Response received for an outbound store request.
    StoreResult {
        peer: PeerId,
        request_id: OutboundRequestId,
        response: StoreResponse,
    },

    /// Response received for an outbound fetch request.
    FetchResult {
        peer: PeerId,
        request_id: OutboundRequestId,
        response: FetchResponse,
    },

    /// A GossipSub message received on a subscribed topic.
    GossipMessage {
        source: Option<PeerId>,
        topic: String,
        data: Vec<u8>,
    },

    /// A new listen address was established.
    ListenAddress(Multiaddr),

    /// An outbound request failed.
    OutboundFailure {
        peer: PeerId,
        request_id: OutboundRequestId,
        error: String,
    },
}
