use serde::{Deserialize, Serialize};
use zero_neural::HybridSignature;

use crate::{Cid, ProgramId, SectorId, ZfsError};

/// Sector head: the current version pointer for a sector within a program.
///
/// Tracks lineage via `prev_head_cid` and supports optional cryptographic
/// attribution via `signature` (added by the protocol layer for signed updates).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Head {
    pub sector_id: SectorId,
    pub cid: Cid,
    pub version: u64,
    pub program_id: ProgramId,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub prev_head_cid: Option<Cid>,
    pub timestamp_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub signature: Option<HybridSignature>,
}

impl Head {
    /// Encode to canonical CBOR bytes.
    pub fn encode_canonical(&self) -> Result<Vec<u8>, ZfsError> {
        crate::encode_canonical(self)
    }

    /// Decode from canonical CBOR bytes.
    pub fn decode_canonical(bytes: &[u8]) -> Result<Self, ZfsError> {
        crate::decode_canonical(bytes)
    }
}
