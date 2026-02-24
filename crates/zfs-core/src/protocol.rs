use serde::{Deserialize, Serialize};
use zero_neural::HybridSignature;

use crate::{Cid, ErrorCode, Head, KeyEnvelope, ProgramId, SectorId};

/// Client → Zode: store a block (ciphertext) with metadata, optional proof,
/// and cryptographic signature.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StoreRequest {
    pub program_id: ProgramId,
    pub cid: Cid,
    #[serde(with = "serde_bytes")]
    pub ciphertext: Vec<u8>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub head: Option<Head>,
    #[serde(
        skip_serializing_if = "Option::is_none",
        default,
        with = "crate::serde_helpers::opt_bytes"
    )]
    pub proof: Option<Vec<u8>>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub key_envelope: Option<KeyEnvelope>,
    pub machine_did: String,
    pub signature: HybridSignature,
}

/// Zode → Client: acknowledgement of a store request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoreResponse {
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub error_code: Option<ErrorCode>,
}

/// Client → Zode: fetch a block or head by CID or sector ID.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FetchRequest {
    pub program_id: ProgramId,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub by_cid: Option<Cid>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub by_sector_id: Option<SectorId>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub machine_did: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub signature: Option<HybridSignature>,
}

/// Zode → Client: response to a fetch request.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FetchResponse {
    #[serde(
        skip_serializing_if = "Option::is_none",
        default,
        with = "crate::serde_helpers::opt_bytes"
    )]
    pub ciphertext: Option<Vec<u8>>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub head: Option<Head>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub error_code: Option<ErrorCode>,
}
