use serde::{Deserialize, Serialize};
use zfs_core::{FetchRequest, FetchResponse, StoreRequest, StoreResponse};

/// Unified ZFS request type for the request-response protocol.
///
/// Wraps `StoreRequest` and `FetchRequest` so both can be served over
/// a single libp2p request-response protocol (`/zfs/1.0.0`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ZfsRequest {
    Store(Box<StoreRequest>),
    Fetch(FetchRequest),
}

/// Unified ZFS response type for the request-response protocol.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ZfsResponse {
    Store(StoreResponse),
    Fetch(Box<FetchResponse>),
}
