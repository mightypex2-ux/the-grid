use serde::{Deserialize, Serialize};
use zfs_core::{ProgramId, ZfsError};

/// ZID (Zero Identity) program descriptor.
///
/// Defines the identity program parameters. The `program_id` is derived
/// as `SHA-256(canonical_cbor(self))`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ZidDescriptor {
    pub name: String,
    pub version: u32,
    pub proof_required: bool,
}

impl ZidDescriptor {
    /// Create the canonical v1 ZID descriptor.
    pub fn v1() -> Self {
        Self {
            name: "zid".to_owned(),
            version: 1,
            proof_required: false,
        }
    }

    /// Derive the ProgramId from this descriptor.
    pub fn program_id(&self) -> Result<ProgramId, ZfsError> {
        let canonical = self.encode_canonical()?;
        Ok(ProgramId::from_descriptor_bytes(&canonical))
    }

    /// Build the GossipSub topic string.
    pub fn topic(&self) -> Result<String, ZfsError> {
        Ok(crate::program_topic(&self.program_id()?))
    }

    /// Encode to canonical CBOR bytes.
    pub fn encode_canonical(&self) -> Result<Vec<u8>, ZfsError> {
        zfs_core::encode_canonical(self)
    }

    /// Decode from canonical CBOR bytes.
    pub fn decode_canonical(bytes: &[u8]) -> Result<Self, ZfsError> {
        zfs_core::decode_canonical(bytes)
    }
}

/// A ZID identity message (claim or update).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ZidMessage {
    /// DID of the identity owner.
    pub owner_did: String,
    /// Display name (optional).
    pub display_name: Option<String>,
    /// Timestamp of the claim (milliseconds since epoch).
    pub timestamp_ms: u64,
}

impl ZidMessage {
    /// Encode to canonical CBOR bytes.
    pub fn encode_canonical(&self) -> Result<Vec<u8>, ZfsError> {
        zfs_core::encode_canonical(self)
    }

    /// Decode from canonical CBOR bytes.
    pub fn decode_canonical(bytes: &[u8]) -> Result<Self, ZfsError> {
        zfs_core::decode_canonical(bytes)
    }
}
